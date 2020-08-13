#[macro_use]
extern crate clap;

use crate::account::Account;
use crate::command::{commandline, CommandLine};
use crate::config::Config;
use crate::global::GENESIS_INFO;
use crate::miner::Miner;
use crate::rpc::Jsonrpc;
use crate::rpcs::Jsonrpcs;
use crate::tps_calculator::TPSCalculator;

use ckb_types::core::BlockView;
use crossbeam_channel::bounded;
use log::LevelFilter;
use metrics_exporter_http::HttpExporter;
use metrics_observer_prometheus::PrometheusBuilder;
use metrics_runtime::Receiver;
use simplelog::WriteLogger;
use std::fs::OpenOptions;
use std::net::SocketAddr;
use std::thread::{spawn, JoinHandle};
use std::time::Duration;

pub mod global;
pub mod miner;
pub mod rpcs;
pub mod transfer;
pub mod util;
pub mod account;
pub mod command;
pub mod config;
pub mod genesis_info;
pub mod rpc;
pub mod tps_calculator;
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
                miner.async_run();
            }
            miner.wait_txpool_empty();

            // TPSCalculator
            TPSCalculator::new(&config).async_run();

            // Benchmark
            let bencher = Account::new(&config.bencher_private_key);
            if miner.lock_script() != bencher.lock_script() {
                run_account_threads(miner.account(), &bencher, &config);
                run_account_threads(&bencher, &bencher, &config)
            } else {
                run_account_threads(&bencher, &bencher, &config)
            }
            .join()
            .unwrap();
        }
    }
}

fn run_account_threads(sender: &Account, recipient: &Account, config: &Config) -> JoinHandle<()> {
    let rpcs = Jsonrpcs::connect_all(config.rpc_urls()).unwrap();
    let cursor_number = rpcs.get_fixed_tip_number();
    let (matureds, unmatureds) = sender.pull_until(&rpcs, cursor_number);

    let (utxo_sender, utxo_receiver) = bounded(2000);
    let sender_ = sender.clone();
    let rpcs_ = rpcs.clone();
    spawn(move || {
        matureds.into_iter().for_each(|utxo| {
            utxo_sender.send(utxo).unwrap();
        });
        sender_.pull_forever(rpcs_, cursor_number, unmatureds, utxo_sender);
    });

    let sender_ = sender.clone();
    let recipient_ = recipient.clone();
    let transaction_type = config.transaction_type;
    let duration = config.seconds().map(Duration::from_secs);
    spawn(move || {
        sender_.transfer_forever(recipient_, rpcs, utxo_receiver, transaction_type, duration)
    })
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
