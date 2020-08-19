#[macro_use]
extern crate clap;

use ckb_types::core::BlockView;
use log::{info, LevelFilter};
use metrics_exporter_http::HttpExporter;
use metrics_observer_prometheus::PrometheusBuilder;
use metrics_runtime::Receiver;
use serde_json::json;
use simplelog::{CombinedLogger, WriteLogger};
use std::fs::OpenOptions;
use std::net::SocketAddr;

use crate::account::Account;
use crate::command::{commandline, CommandLine};
use crate::config::Config;
use crate::global::{GENESIS_INFO, METRICS_RECORDER};
use crate::miner::Miner;
use crate::net::Net;
use crate::rpc::Jsonrpc;
use crate::threads::{spawn_miner, spawn_pull_utxos, spawn_transfer_utxos};

pub mod benchmark;
pub mod global;
pub mod miner;
pub mod net;
pub mod net_monitor;
pub mod threads;
pub mod transfer;
pub mod util;
pub mod account;
pub mod command;
pub mod config;
pub mod genesis_info;
pub mod rpc;
pub mod utxo;

fn main() {
    match commandline() {
        CommandLine::MineMode(config, blocks) => {
            info!("\nTPSBench start with configuration: {}", json!(config));
            init_logger(&config);
            init_global_genesis_info(&config);

            let miner_config = config.miner.as_ref().expect("config [miner] on mine-mode");
            let rpc_urls = config.rpc_urls();
            let miner = Miner::new(miner_config, rpc_urls);
            miner.generate_blocks(blocks);
        }
        CommandLine::BenchMode(config) => {
            info!("\nTPSBench start with configuration: {}", json!(config));
            init_logger(&config);
            init_metrics_recorder(&config);
            init_metrics(&config);
            init_global_genesis_info(&config);

            // Bencher
            let bencher = Account::new(&config.bencher_private_key);
            let (_, bencher_utxo_r) = spawn_pull_utxos(&config, &bencher);

            // Miner
            if let Some(ref miner_config) = config.miner {
                let rpc_urls = config.rpc_urls();
                let miner = Miner::new(miner_config, rpc_urls);

                let _ = spawn_miner(&miner);

                // Transfer all miner's utxo to bencher
                if miner.lock_script() != bencher.lock_script() {
                    let (_, miner_utxo_r) = spawn_pull_utxos(&config, &miner);
                    spawn_transfer_utxos(&config, &miner, &bencher, miner_utxo_r);
                }
            }

            // Benchmark
            for benchmark in config.benchmarks.iter() {
                let net = Net::connect_all(config.rpc_urls());
                benchmark.bench(&net, &bencher, &bencher, &bencher_utxo_r);
            }
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

// TODO It's just draft version, I don't really know how to init metrics service
fn init_metrics(config: &Config) {
    if config.metrics_url.is_none() {
        info!("No start metrics service");
        return;
    }

    let metrics_url = config.metrics_url.as_ref().unwrap();
    let listen = metrics_url.parse::<SocketAddr>().unwrap();
    let receiver = Receiver::builder().build().unwrap();
    let controller = receiver.controller();
    let builder = PrometheusBuilder::new();
    let exporter = HttpExporter::new(controller, builder, listen);

    let runtime = tokio::runtime::Builder::default()
        .threaded_scheduler()
        .enable_all()
        .build()
        .unwrap();
    runtime.handle().spawn(async {
        tokio::spawn(exporter.async_run());
    });

    // info!("Metrics URL: {}", metrics_url);
}

/// Initialize the global `GENESIS_INFO` with the genesis block
pub fn init_global_genesis_info(config: &Config) {
    info!("[START] init_global_genesis_info");
    let url = config.rpc_urls()[0];
    let rpc = Jsonrpc::connect(url);
    let genesis_block: BlockView = rpc
        .get_block_by_number(0)
        .expect(&format!(
            "Jsonrpc::get_block_by_number({}, 0), error: return None",
            url
        ))
        .into();
    info!("[END] init_global_genesis_info {}", genesis_block.hash());
    *GENESIS_INFO.lock().unwrap() = genesis_block.into();
}
