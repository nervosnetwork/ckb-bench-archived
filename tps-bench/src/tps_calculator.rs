use crate::config::Config;
use crate::rpcs::Jsonrpcs;

use ckb_types::core::BlockView;
use log::info;
use metrics::gauge;
use serde_derive::{Deserialize, Serialize};
use serde_json::json;
use std::cmp::max;
use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;
use std::thread::{sleep, spawn, JoinHandle};
use std::time::Duration;

const RECENT_BLOCKS: usize = 10;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Metrics {
    tps: u64,
    average_block_time_ms: u64,
    average_block_transactions: u64,
    start_block_number: u64,
    end_block_number: u64,
}

pub struct TPSCalculator {
    rpcs: Jsonrpcs,
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
        let rpcs = match Jsonrpcs::connect_all(config.rpc_urls()) {
            Ok(rpcs) => rpcs,
            Err(err) => prompt_and_exit!(
                "Jsonrpcs::connect_all({:?}) error: {}",
                config.rpc_urls(),
                err
            ),
        };
        let metrics_file = File::create(config.metrics_path()).unwrap();
        TPSCalculator {
            rpcs,
            metrics_file,
            recent_blocks: Default::default(),
            recent_total_txns: 0,
        }
    }

    pub fn update(&mut self) -> bool {
        let tip_number = self.rpcs.get_fixed_tip_number();
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
                if let Some(block) = self.rpcs.get_block_by_number(number) {
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
        let elapsed_ms = end_block
            .timestamp()
            .saturating_sub(start_block.timestamp());
        if elapsed_ms == 0 {
            return;
        }

        let average_block_time_ms = elapsed_ms / self.recent_blocks.len() as u64;
        let average_block_transactions = self.recent_total_txns / self.recent_blocks.len() as u64;
        let tps = (self.recent_total_txns as f64 * 1000.0 / elapsed_ms as f64) as u64;
        let json = json!(Metrics {
            tps,
            average_block_time_ms,
            average_block_transactions,
            start_block_number: start_block.number(),
            end_block_number: end_block.number(),
        });

        self.metrics_file.set_len(0).unwrap();
        serde_json::to_writer_pretty(&self.metrics_file, &json).unwrap();
        self.metrics_file.flush().unwrap();

        info!("metrics: {}", json.to_string());

        gauge!("tps", tps as i64);
        gauge!("average_block_time", average_block_time_ms as i64);
        gauge!(
            "average_block_transactions",
            average_block_transactions as i64
        );
    }
}
