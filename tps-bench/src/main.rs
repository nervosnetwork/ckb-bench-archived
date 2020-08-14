#[macro_use]
extern crate clap;

use crate::threads::{spawn_miner, spawn_pull_utxos, spawn_transfer_utxos};
use ckb_types::core::BlockView;
use log::LevelFilter;
use metrics_exporter_http::HttpExporter;
use metrics_observer_prometheus::PrometheusBuilder;
use metrics_runtime::Receiver;
use simplelog::WriteLogger;
use std::fs::OpenOptions;
use std::net::SocketAddr;

use crate::account::Account;
use crate::command::{commandline, CommandLine};
use crate::config::Config;
use crate::global::GENESIS_INFO;
use crate::miner::Miner;
use crate::rpc::Jsonrpc;

pub mod benchmark;
pub mod global;
pub mod miner;
pub mod rpcs;
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
            init_logger(&config);
            init_global_genesis_info(&config);

            let miner = Miner::new(&config, &config.miner_private_key);
            miner.generate_blocks(blocks);
        }
        CommandLine::BenchMode(config) => {
            init_logger(&config);
            init_metrics(&config);
            init_global_genesis_info(&config);

            // Miner
            let miner = Miner::new(&config, &config.miner_private_key);
            if config.start_miner {
                let _ = spawn_miner(&miner);
            }

            // Benchmark
            let bencher = Account::new(&config.bencher_private_key);
            let handler = if miner.lock_script() != bencher.lock_script() {
                let (_, miner_utxo_r) = spawn_pull_utxos(&config, &miner);
                let (_, bencher_utxo_r) = spawn_pull_utxos(&config, &bencher);
                spawn_transfer_utxos(&config, &miner, &bencher, miner_utxo_r);
                spawn_transfer_utxos(&config, &bencher, &bencher, bencher_utxo_r)
            } else {
                let (_, bencher_utxo_r) = spawn_pull_utxos(&config, &bencher);
                spawn_transfer_utxos(&config, &bencher, &bencher, bencher_utxo_r)
            };
            handler.join().unwrap();
        }
    }
}

fn init_logger(config: &Config) {
    let mut options = OpenOptions::new();
    let options = options.create(true).append(true);
    let logs = options.open(config.log_path()).unwrap();
    let _metrics = options.open(config.metrics_path()).unwrap();

    WriteLogger::init(LevelFilter::Info, Default::default(), logs).unwrap();
    println!(
        "LogPath: {}",
        config.log_path().canonicalize().unwrap().to_string_lossy()
    );
    println!(
        "MetricsPath: {}",
        config
            .metrics_path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
    );
}

// TODO It's just draft version, I don't really know how to init metrics service
fn init_metrics(config: &Config) {
    if config.metrics_url.is_none() {
        println!("No start metrics service");
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

    // println!("Metrics URL: {}", metrics_url);
}

/// Initialize the global `GENESIS_INFO` with the genesis block
pub fn init_global_genesis_info(config: &Config) {
    let url = config.rpc_urls()[0];
    let rpc = match Jsonrpc::connect(url) {
        Ok(rpc) => rpc,
        Err(err) => prompt_and_exit!("Jsonrpc::connect({}) error: {}", url, err),
    };
    let genesis_block: BlockView = match rpc.get_block_by_number(0) {
        Some(genesis_block) => genesis_block.into(),
        None => prompt_and_exit!(
            "Jsonrpc::get_block_by_number(0) from {} error: return None",
            url
        ),
    };
    *GENESIS_INFO.lock().unwrap() = genesis_block.into();
}
