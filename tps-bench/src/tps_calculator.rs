use crate::config::Config;
use crate::rpc::Jsonrpc;
use ckb_types::core::BlockView;
use std::cmp::max;
use std::collections::VecDeque;
use std::thread::{sleep, spawn, JoinHandle};
use std::time::Duration;

const RECENT_BLOCKS: usize = 60;

pub struct TPSCalculator {
    rpc: Jsonrpc,
    recent_blocks: VecDeque<BlockView>,
    recent_total_txns: u64,
}

impl TPSCalculator {
    pub fn async_run(mut self) -> JoinHandle<()> {
        spawn(move || loop {
            self.update();
            self.print_tps();
            sleep(Duration::from_secs(10));
        })
    }

    pub fn new(config: &Config) -> Self {
        let url = config.node_urls.first().expect("checked");
        let rpc = match Jsonrpc::connect(url.as_str()) {
            Ok(rpc) => rpc,
            Err(err) => prompt_and_exit!("Jsonrpc::connect({}) error: {}", url.as_str(), err),
        };
        TPSCalculator {
            rpc,
            recent_blocks: Default::default(),
            recent_total_txns: 0,
        }
    }

    pub fn update(&mut self) {
        let tip_number = self.rpc.get_tip_block_number();
        let recent_number = self
            .recent_blocks
            .back()
            .map(|block| block.number())
            .unwrap_or(0);

        if tip_number > recent_number {
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
    }

    pub fn print_tps(&self) -> f64 {
        if self.recent_blocks.len() < 2 {
            return 0.0;
        }

        let start_block = self.recent_blocks.front().unwrap();
        let end_block = self.recent_blocks.back().unwrap();
        let elapsed = end_block
            .timestamp()
            .saturating_sub(start_block.timestamp())
            / 1000;
        let tps = self.recent_total_txns as f64 / elapsed as f64;

        println!(
            "[{}, {}] txns: {}, elapsed: {}, tps: {}",
            start_block.number(),
            end_block.number(),
            self.recent_total_txns,
            elapsed,
            tps,
        );

        tps
    }
}
