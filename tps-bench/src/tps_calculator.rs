use crate::config::Config;
use crate::rpc::Jsonrpc;
use ckb_types::core::BlockView;
use log::info;
use metrics::gauge;
use std::cmp::max;
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::thread::{sleep, spawn, JoinHandle};
use std::time::Duration;

const RECENT_BLOCKS: usize = 10;

pub struct TPSCalculator {
    rpc: Jsonrpc,
    recent_blocks: VecDeque<BlockView>,
    recent_total_txns: u64,
    metrics_file: File,
}

impl TPSCalculator {
    pub fn async_run(mut self) -> JoinHandle<()> {
        spawn(move || loop {
            if self.update() {
                self.print_tps();
            }
            sleep(Duration::from_secs(1));
        })
    }

    pub fn new(config: &Config) -> Self {
        let url = &config.rpc_urls()[0];
        let rpc = match Jsonrpc::connect(url.as_str()) {
            Ok(rpc) => rpc,
            Err(err) => prompt_and_exit!("Jsonrpc::connect({}) error: {}", url.as_str(), err),
        };
        let metrics_file = OpenOptions::new()
            .create(true)
            .write(true)
            .open(config.metrics_path())
            .unwrap();
        TPSCalculator {
            rpc,
            recent_blocks: Default::default(),
            recent_total_txns: 0,
            metrics_file,
        }
    }

    pub fn update(&mut self) -> bool {
        let tip_number = self.rpc.get_tip_block_number();
        let recent_number = self
            .recent_blocks
            .back()
            .map(|block| block.number())
            .unwrap_or(0);

        let mut updated = false;
        if tip_number > recent_number {
            updated = true;
            let start_number = max(
                tip_number.saturating_sub(RECENT_BLOCKS as u64),
                recent_number + 1,
            );
            for number in start_number..=tip_number {
                if let Some(block) = self.rpc.get_block_by_number(number) {
                    self.recent_total_txns += block.transactions.len() as u64;
                    self.recent_blocks.push_back(block.into());
                }
            }
        }

        if self.recent_blocks.len() > RECENT_BLOCKS {
            let pop_count = self.recent_blocks.len() - RECENT_BLOCKS;
            for _ in 0..pop_count {
                if let Some(block) = self.recent_blocks.pop_front() {
                    self.recent_total_txns -= block.transactions().len() as u64;
                }
            }
        }

        updated
    }

    pub fn print_tps(&mut self) {
        if self.recent_blocks.len() < 2 {
            return;
        }

        let start_block = self.recent_blocks.front().unwrap();
        let end_block = self.recent_blocks.back().unwrap();
        let elapsed = end_block
            .timestamp()
            .saturating_sub(start_block.timestamp())
            / 1000;
        let average_block_time = elapsed / self.recent_blocks.len() as u64;
        let average_block_transactions = self.recent_total_txns / self.recent_blocks.len() as u64;
        let tps = if elapsed == 0 {
            self.recent_total_txns as f64
        } else {
            self.recent_total_txns as f64 / elapsed as f64
        };
        let json = serde_json::json!({
            "start_block_number": start_block.number(),
            "end_block_number": end_block.number(),
            "tps": format!("{:.2}", tps),
            "average_block_time_secs": average_block_time,
            "average_block_transactions": average_block_transactions,
        });

        self.metrics_file.set_len(0).unwrap();
        serde_json::to_writer_pretty(&self.metrics_file, &json).unwrap();
        self.metrics_file.flush().unwrap();

        info!("metrics: {}", json.to_string());

        gauge!("tps", tps as i64);
        gauge!("average_block_time", average_block_time as i64);
        gauge!(
            "average_block_transactions",
            average_block_transactions as i64
        );
    }
}
