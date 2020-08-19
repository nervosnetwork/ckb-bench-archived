use crate::net::Net;
use ckb_types::core::BlockView;
use log::info;
use serde_derive::{Deserialize, Serialize};
use serde_json::json;
use std::cmp::max;
use std::collections::VecDeque;
use std::thread::sleep;
use std::time::Duration;

enum MethodToEvalNetStable {
    #[allow(dead_code)]
    RecentBlocktxnsNearly { window: u64, margin: u64 },
    #[allow(dead_code)]
    CustomBlocksElapsed { warmup: u64, window: u64 },
}

// const METHOD_TO_EVAL_NET_STABLE: MethodToEvalNetStable =
//     MethodToEvalNetStable::RecentBlocktxnsNearly {
//         window: 21,
//         margin: 10,
//     };
const METHOD_TO_EVAL_NET_STABLE: MethodToEvalNetStable =
    MethodToEvalNetStable::CustomBlocksElapsed {
        warmup: 20,
        window: 21,
    };

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Metrics {
    tps: u64,
    average_block_time_ms: u64,
    average_block_transactions: u64,
    start_block_number: u64,
    end_block_number: u64,
}

pub fn wait_network_stabled(net: &Net) -> Metrics {
    match METHOD_TO_EVAL_NET_STABLE {
        MethodToEvalNetStable::RecentBlocktxnsNearly { window, margin } => {
            wait_recent_blocktxns_nearly(net, window, margin)
        }
        MethodToEvalNetStable::CustomBlocksElapsed { window, warmup } => {
            wait_custom_blocks_elapsed(net, window, warmup)
        }
    }
}

pub fn wait_network_txpool_empty(net: &Net) {
    info!("[START] net_monitor::wait_network_txpool_empty()");
    while !is_network_txpool_empty(net) {
        sleep(Duration::from_secs(1));
    }
    info!("[END] net_monitor::wait_network_txpool_empty()");
}

fn wait_custom_blocks_elapsed(net: &Net, window: u64, warmup: u64) -> Metrics {
    info!("[START] net_monitor::wait_custom_blocks_elapsed");
    let current_tip_number = net.get_confirmed_tip_number();
    while current_tip_number + warmup > net.get_confirmed_tip_number() {
        sleep(Duration::from_secs(1));
    }

    let current_tip_number = net.get_confirmed_tip_number();
    while current_tip_number + window > net.get_confirmed_tip_number() {
        sleep(Duration::from_secs(1));
    }

    let blocks = (current_tip_number..current_tip_number + window)
        .map(|number| net.get_block_by_number(number).unwrap())
        .map(|block| block.into())
        .collect::<Vec<_>>();
    Metrics::eval_blocks(blocks)
}

fn wait_recent_blocktxns_nearly(net: &Net, window: u64, margin: u64) -> Metrics {
    info!("[START] net_monitor::wait_recent_blocktxns_nearly");
    let mut queue = VecDeque::with_capacity(window as usize);
    queue.push_back(net.get_confirmed_tip_block());
    loop {
        loop {
            let tip_number = net.get_confirmed_tip_number();
            let back = queue.back().unwrap();
            if tip_number > back.number() {
                let next_block = net.get_block_by_number(back.number() + 1).unwrap().into();
                while queue.len() >= window as usize {
                    queue.pop_front();
                }
                queue.push_back(next_block);
                break;
            } else {
                sleep(Duration::from_secs(1));
            }
        }

        if queue.len() >= window as usize {
            let metrics = Metrics::eval_blocks(queue.iter().cloned().collect());
            info!("[metrics] {}", json!(metrics));

            let mintxns = queue.iter().map(|b| b.transactions().len()).min().unwrap();
            let maxtxns = queue.iter().map(|b| b.transactions().len()).max().unwrap();
            if maxtxns <= mintxns + margin as usize {
                return metrics;
            }
        }
    }
}

fn is_network_txpool_empty(net: &Net) -> bool {
    for rpc in net.endpoints() {
        let tx_pool_info = rpc.tx_pool_info();
        if tx_pool_info.pending.value() != 0 || tx_pool_info.proposed.value() != 0 {
            return false;
        }
    }
    true
}

impl Metrics {
    fn eval_blocks(blocks: Vec<BlockView>) -> Self {
        let totaltxns: usize = blocks.iter().map(|block| block.transactions().len()).sum();
        let front = blocks.first().unwrap();
        let back = blocks.last().unwrap();
        let average_block_transactions = (totaltxns / blocks.len()) as u64;
        let elapsed_ms = front.timestamp().saturating_sub(back.timestamp());
        let average_block_time_ms = max(1, elapsed_ms / (blocks.len() as u64));
        let tps = (totaltxns as f64 * 1000.0 / elapsed_ms as f64) as u64;
        Metrics {
            tps,
            average_block_time_ms,
            average_block_transactions,
            start_block_number: front.number(),
            end_block_number: back.number(),
        }
    }
}
