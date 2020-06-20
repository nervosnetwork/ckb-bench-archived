use crate::config::Config;
use crate::controller::Controller;
use crate::rpc::Jsonrpc;
use std::collections::HashMap;
use std::ops::Range;
use std::time::{Duration, Instant};

const PRINT_TABLE_EVERY: Duration = Duration::from_secs(10);
const EVAL_WINDOW: Range<u64> = 50..51;

pub struct GlobalController {
    rpc: Jsonrpc,
    end_number: u64,
    last_print_time: Instant,
    txn_table: HashMap<u64, (u64, u64)>, // block_number => (accumulative_transactions_count, block_timestamp)
}

impl Controller for GlobalController {
    fn new(config: &Config) -> Self {
        let url = config.node_urls.first().expect("checked");
        let rpc = match Jsonrpc::connect(url.as_str()) {
            Ok(rpc) => rpc,
            Err(err) => prompt_and_exit!("Jsonrpc::connect({}) error: {}", url.as_str(), err),
        };
        let end_number = rpc.get_tip_block_number();
        GlobalController {
            rpc,
            end_number,
            last_print_time: Instant::now(),
            txn_table: HashMap::new(),
        }
    }

    fn add(&mut self) -> Duration {
        if self.last_print_time.elapsed() >= PRINT_TABLE_EVERY {
            self.last_print_time = Instant::now();
            self.update_txn_table();
            self.print_table();
        }

        Duration::from_secs(0)
    }
}

impl GlobalController {
    fn print_table(&self) {
        let mut final_tps = 0f64;
        for gap in EVAL_WINDOW {
            let start_number = self.end_number.saturating_sub(gap);
            if start_number == 0 {
                break;
            }

            let start = self
                .txn_table
                .get(&start_number)
                .cloned()
                .unwrap_or((0u64, 0u64));
            let end = self
                .txn_table
                .get(&self.end_number)
                .cloned()
                .unwrap_or((0u64, 0u64));
            let delta_txn = end.0.saturating_sub(start.0);
            let delta_timestamp = end.1.saturating_sub(start.1);
            if delta_timestamp == 0 {
                break;
            }

            let tps = delta_txn as f64 / (delta_timestamp as f64 / 1000f64);
            if final_tps as u64 == 0 {
                final_tps = tps;
            } else {
                final_tps = (final_tps + tps) / 2f64;
            }
        }
        println!("tps={}", final_tps as u64);
    }

    fn update_txn_table(&mut self) {
        let tip_number = self.rpc.get_tip_block_number();
        let synced_tip_number = tip_number.saturating_sub(8); // assume tip - 8 is synced
        if synced_tip_number <= self.end_number {
            return;
        }

        for number in self.end_number + 1..=synced_tip_number {
            let block = self.rpc.get_block_by_number(number).unwrap();
            let timestamp = block.header.inner.timestamp.value();
            let parent_number = number.saturating_sub(1);
            let parent_acc = self
                .txn_table
                .get(&parent_number)
                .cloned()
                .unwrap_or((0u64, 0u64))
                .0;
            let acc = parent_acc + block.transactions.len() as u64;

            assert!(!self.txn_table.contains_key(&number));

            self.txn_table.insert(number, (acc, timestamp));
        }
        self.end_number = synced_tip_number;
    }
}
