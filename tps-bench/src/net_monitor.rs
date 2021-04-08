use crate::net::Net;
use ckb_types::core::BlockView;
use log::info;
use serde_derive::{Deserialize, Serialize};
use serde_json::json;
use std::cmp::max;
use std::collections::VecDeque;
use std::thread::sleep;
use std::time::{Duration, Instant};

#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
pub enum MethodToEvalNetStable {
    #[allow(dead_code)]
    RecentBlocktxnsNearly { window: u64, margin: u64 },
    #[allow(dead_code)]
    CustomBlocksElapsed { warmup: u64, window: u64 },
    #[allow(dead_code)]
    Never,
    #[allow(dead_code)]
    TimedTask { duration_time: u64 },
}

impl Default for MethodToEvalNetStable {
    fn default() -> Self {
        Self::CustomBlocksElapsed {
            warmup: 20,
            window: 21,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Metrics {
    pub tps: u64,
    pub average_block_time_ms: u64,
    pub average_block_transactions: u64,
    pub start_block_number: u64,
    pub end_block_number: u64,
    pub network_nodes: u64,
    pub bench_nodes: u64,
    pub total_transactions_size: u64,
}

pub fn wait_network_stabled(net: &Net, evaluation: MethodToEvalNetStable) -> Metrics {
    match evaluation {
        MethodToEvalNetStable::RecentBlocktxnsNearly { window, margin } => {
            wait_recent_blocktxns_nearly(net, window, margin)
        }
        MethodToEvalNetStable::CustomBlocksElapsed { window, warmup } => {
            wait_custom_blocks_elapsed(net, window, warmup)
        }
        MethodToEvalNetStable::Never => loop {
            sleep(Duration::from_secs(60 * 10));
            info!("net_monitor use MethodToEvalNetStable::Never will never exit");
        },
        MethodToEvalNetStable::TimedTask { duration_time } => {
            wait_duration_time_elapsed(net, duration_time)
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
    let current_tip_number = net.get_confirmed_tip_number();
    let (mut last_print, start_time) = (Instant::now(), Instant::now());
    while current_tip_number + warmup > net.get_confirmed_tip_number() {
        if last_print.elapsed() >= Duration::from_secs(60) {
            last_print = Instant::now();
            info!(
                "warmup progress ({}/{}) ...",
                current_tip_number,
                current_tip_number + warmup
            );
        }
        sleep(Duration::from_secs(1));
    }
    info!("complete warmup, took {:?}", start_time.elapsed());

    let current_tip_number = net.get_confirmed_tip_number();
    let (mut last_print, start_time) = (Instant::now(), Instant::now());
    while current_tip_number + window > net.get_confirmed_tip_number() {
        if last_print.elapsed() >= Duration::from_secs(60) {
            last_print = Instant::now();
            info!(
                "evaluation progress ({}/{}) ...",
                current_tip_number,
                current_tip_number + warmup
            );
        }
        sleep(Duration::from_secs(1));
    }
    info!("complete evaluation, took {:?}", start_time.elapsed());

    Metrics::eval_blocks(net, current_tip_number, current_tip_number + window)
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
            let from_number = queue.pop_front().unwrap().number();
            let end_number = queue.pop_back().unwrap().number();
            let metrics = Metrics::eval_blocks(net, from_number, end_number);
            info!("[metrics] {}", json!(metrics));

            let mintxns = queue.iter().map(|b| b.transactions().len()).min().unwrap();
            let maxtxns = queue.iter().map(|b| b.transactions().len()).max().unwrap();
            if maxtxns <= mintxns + margin as usize {
                return metrics;
            }
        }
    }
}

fn wait_duration_time_elapsed(net: &Net, duration_time: u64) -> Metrics {
    info!("[START] net_monitor::wait_duration_time_elapsed");
    let first_tip_number = net.get_confirmed_tip_number();
    let (start_time, mut last_print) = (Instant::now(), Instant::now());
    while start_time.elapsed() <= Duration::from_secs(duration_time) {
        if last_print.elapsed() >= Duration::from_secs(60) {
            last_print = Instant::now();
            info!(
                "Bench progress ({:?}/{:?}) ...",
                start_time.elapsed(),
                Duration::from_secs(duration_time)
            );
        }
        sleep(Duration::from_secs(1));
    }
    let last_tip_number = net.get_confirmed_tip_number();
    Metrics::eval_blocks(net, first_tip_number, last_tip_number)
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
    pub fn eval_blocks(net: &Net, from_number: u64, end_number: u64) -> Self {
        let network_nodes = net.get_network_nodes();
        let bench_nodes = net.get_bench_nodes();

        let mut totaltxns: usize = 0;
        let mut total_transactions_size: u64 = 0;
        for number in from_number..=end_number {
            let block: BlockView = net.get_block_by_number(number).unwrap().into();
            totaltxns += block.transactions().len();
            total_transactions_size += eval_total_tx_size_in_block(&block);
        }

        let blocks_count: u64 = end_number - from_number + 1;
        let front: BlockView = net.get_block_by_number(from_number).unwrap().into();
        let back: BlockView = net.get_block_by_number(end_number).unwrap().into();
        let average_block_transactions = (totaltxns / blocks_count as usize) as u64;
        let elapsed_ms = back.timestamp().saturating_sub(front.timestamp());
        let average_block_time_ms = max(1, elapsed_ms / blocks_count);
        let tps = (totaltxns as f64 * 1000.0 / elapsed_ms as f64) as u64;
        Metrics {
            tps,
            average_block_time_ms,
            average_block_transactions,
            start_block_number: from_number,
            end_block_number: end_number,
            network_nodes,
            bench_nodes,
            total_transactions_size,
        }
    }
}

fn eval_total_tx_size_in_block(block: &BlockView) -> u64 {
    block
        .transactions()
        .iter()
        .map(|tx| tx.data().serialized_size_in_block() as u64)
        .sum()
}
