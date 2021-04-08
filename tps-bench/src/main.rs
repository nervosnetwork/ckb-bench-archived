#[macro_use]
extern crate clap;

use ckb_types::core::BlockView;
use log::{info, LevelFilter};
use serde_json::json;
use simplelog::{CombinedLogger, WriteLogger};
use std::fs::OpenOptions;

use crate::account::Account;
use crate::benchmark::BenchmarkConfig;
use crate::command::{commandline, CommandLine};
use crate::config::{Config, TransactionType};
use crate::global::{GENESIS_INFO, METRICS_RECORDER};
use crate::miner::Miner;
use crate::net::Net;
use crate::net_monitor::Metrics;
use crate::rpc::Jsonrpc;
use crate::threads::{spawn_miner, spawn_pull_utxos, spawn_transfer_utxos};

pub mod account;
pub mod benchmark;
pub mod command;
pub mod config;
pub mod genesis_info;
pub mod global;
pub mod miner;
pub mod net;
pub mod net_monitor;
pub mod rpc;
pub mod threads;
pub mod transfer;
#[macro_use]
pub mod util;
pub mod utxo;

fn main() {
    match commandline() {
        CommandLine::MineMode(config, blocks) => {
            info!("\nTPSBench start with configuration: {}", json!(config));
            init_logger(&config);
            init_global_genesis_info(&config);

            let miner_config = &config.miner;
            let rpc_urls = config.rpc_urls();
            let miner = Miner::new(miner_config, rpc_urls);
            miner.generate_blocks(blocks);
        }
        CommandLine::BenchMode(config, skip_best_tps_caculation) => {
            info!("\nTPSBench start with configuration: {}", json!(config));
            init_logger(&config);
            init_metrics_recorder(&config);
            init_global_genesis_info(&config);

            let rpc_urls = config.rpc_urls();
            let net = Net::connect_all(config.rpc_urls());

            // Bencher
            let bencher = Account::new(&config.bencher_private_key);

            // Miner
            let miner_config = &config.miner;
            let miner = Miner::new(&miner_config, rpc_urls);
            spawn_miner(&miner);

            // Transfer all miner's utxo to bencher
            if miner.lock_script() != bencher.lock_script() {
                let (_, miner_utxo_r) = spawn_pull_utxos(&config, &miner, &miner);
                spawn_transfer_utxos(&config, &miner, &bencher, miner_utxo_r);
            }

            let (_, bencher_utxo_r) = spawn_pull_utxos(&config, &bencher, &miner);

            // Benchmark
            for benchmark in config.benchmarks.iter() {
                benchmark.bench(
                    &net,
                    &bencher,
                    &bencher,
                    &bencher_utxo_r,
                    benchmark.send_delay,
                );
            }

            if !skip_best_tps_caculation {
                let benchmark = BenchmarkConfig {
                    transaction_type: TransactionType::In2Out2,
                    send_delay: 0,
                    method_to_eval_net_stable: None,
                };
                let best_tps = benchmark.find_best_bench(&net, &bencher, &bencher, &bencher_utxo_r);
                info!("Best TPS: {}", best_tps);
                println!("TPS: {}", best_tps);
            }
        }
        CommandLine::MetricMode(rpc_urls) => {
            info!("\n Caculate TPS");

            let rpc_urls = rpc_urls.iter().map(|url| url.as_str()).collect();
            let net = Net::connect_all(rpc_urls);

            let tip_block_number = net.get_confirmed_tip_number();
            let result = Metrics::eval_blocks(&net, 1, tip_block_number);
            println!("{:?}", result);
        }
    }
}

fn init_logger(config: &Config) {
    let mut options = OpenOptions::new();
    let options = options.create(true).append(true);
    let path = config.log_path();
    let file = options.open(&path).unwrap();
    CombinedLogger::init(vec![
        // SimpleLogger::new(LevelFilter::Info, Default::default()),
        WriteLogger::new(LevelFilter::Info, Default::default(), file),
    ])
    .unwrap();
    let abs_path = path.canonicalize().unwrap();
    info!(
        "TPSBench appends logs into {}",
        abs_path.canonicalize().unwrap().to_string_lossy()
    );
}

fn init_metrics_recorder(config: &Config) {
    let mut options = OpenOptions::new();
    let options = options.create(true).append(true);
    let path = config.metrics_path();
    let file = options.open(&path).unwrap();
    *METRICS_RECORDER.lock().unwrap() = Some(file);
    let abs_path = path.canonicalize().unwrap();
    info!(
        "TPSBench appends benchmark results into {}",
        abs_path.to_string_lossy()
    );
}

/// Initialize the global `GENESIS_INFO` with the genesis block
pub fn init_global_genesis_info(config: &Config) {
    info!("[START] init_global_genesis_info");
    let url = config.rpc_urls()[0];
    let rpc = Jsonrpc::connect(url);
    let genesis_block: BlockView = rpc
        .get_block_by_number(0)
        .unwrap_or_else(|| {
            panic!(
                "Jsonrpc::get_block_by_number({}, 0), error: return None",
                url
            )
        })
        .into();
    info!("[END] init_global_genesis_info {}", genesis_block.hash());
    *GENESIS_INFO.lock().unwrap() = genesis_block.into();
}
