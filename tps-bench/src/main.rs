#[macro_use]
extern crate clap;

use crate::account::Account;
use crate::command::{commandline, CommandLine};
use crate::config::{Config, TransactionType};
use crate::global::GENESIS_INFO;
use crate::miner::Miner;
use crate::rpc::Jsonrpc;
use crate::rpcs::Jsonrpcs;
use crate::tps_calculator::TPSCalculator;

use ckb_types::core::BlockView;
use crossbeam_channel::bounded;
use log::{info, LevelFilter};
use metrics_exporter_http::HttpExporter;
use metrics_observer_prometheus::PrometheusBuilder;
use metrics_runtime::Receiver;
use simplelog::WriteLogger;
use std::fs::{File, OpenOptions};
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
            miner.generate_blocks(blocks)
        }
        CommandLine::BenchMode(config) => {
            init_logger(&config);
            init_metrics(&config);
            init_global_genesis_info(&config);

            let miner = Miner::new(&config, &config.miner_private_key);
            let bencher = Account::new(&config.bencher_private_key);
            let rpcs = Jsonrpcs::connect_all(config.rpc_urls()).unwrap();

            if config.start_miner {
                miner.async_mine();
            }
            miner.wait_txpool_empty(config.start_miner);
            TPSCalculator::new(&config).async_run();

            if miner.lock_script() != bencher.lock_script() {
                let _ = run_account_threads(
                    miner.account().clone(),
                    bencher.clone(),
                    rpcs.clone(),
                    config.transaction_type,
                    config.seconds().map(|secs| Duration::from_secs(secs)),
                );
            }
            run_account_threads(
                bencher.clone(),
                bencher.clone(),
                rpcs.clone(),
                config.transaction_type,
                config.seconds().map(|secs| Duration::from_secs(secs)),
            )
            .join()
            .unwrap();
        }
    }
}

fn run_account_threads(
    sender: Account,
    recipient: Account,
    rpcs: Jsonrpcs,
    transaction_type: TransactionType,
    duration: Option<Duration>,
) -> JoinHandle<()> {
    let (utxo_sender, utxo_receiver) = bounded(2000);
    let cursor_number = rpcs.get_fixed_tip_number();
    info!("START account.pull_until");
    let (matureds, unmatureds) = sender.pull_until(&rpcs, cursor_number);
    info!("DONE account.pull_until");
    let sender_clone = sender.clone();
    let rpcs_clone = rpcs.clone();
    spawn(move || {
        matureds.into_iter().for_each(|utxo| {
            utxo_sender.send(utxo).unwrap();
        });
        sender_clone.pull_forever(rpcs_clone, cursor_number, unmatureds, utxo_sender);
    });
    spawn(move || {
        sender.transfer_forever(recipient, rpcs, utxo_receiver, transaction_type, duration)
    })
}

fn init_logger(config: &Config) {
    WriteLogger::init(
        LevelFilter::Info,
        Default::default(),
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(config.log_path())
            .unwrap(),
    )
    .unwrap();
    println!(
        "Log Path: {}",
        config.log_path().canonicalize().unwrap().to_string_lossy()
    );

    // dirty...
    File::create(config.metrics_path()).unwrap();
    println!(
        "Metrics Path: {}",
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
