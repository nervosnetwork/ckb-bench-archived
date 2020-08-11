#[macro_use]
extern crate clap;

use crate::account::Account;
use crate::command::{commandline, CommandLine};
use crate::config::{Config, TransactionType, Url};
use crate::genesis_info::{global_genesis_info, init_global_genesis_info};
use crate::miner::Miner;
use crate::rpc::Jsonrpc;
use crate::tps_calculator::TPSCalculator;
use ckb_types::core::DepType;
use ckb_types::packed::{Byte32, CellDep, OutPoint};
use ckb_types::prelude::*;
use ckb_types::{h256, H256};
use crossbeam_channel::bounded;
use lazy_static::lazy_static;
use log::{info, LevelFilter};
use metrics_exporter_http::HttpExporter;
use metrics_observer_prometheus::PrometheusBuilder;
use metrics_runtime::Receiver;
use simplelog::WriteLogger;
use std::fs::{File, OpenOptions};
use std::net::SocketAddr;
use std::sync::Mutex;
use std::thread::{spawn, JoinHandle};
use std::time::Duration;

pub mod miner;
pub mod transfer;
pub mod util;
pub mod account;
pub mod command;
pub mod config;
pub mod genesis_info;
pub mod rpc;
pub mod tps_calculator;
pub mod utxo;

pub const MIN_SECP_CELL_CAPACITY: u64 = 61_0000_0000;
pub const SIGHASH_ALL_DEP_GROUP_CELL_INDEX: usize = 0;
pub const SIGHASH_ALL_TYPE_HASH: H256 =
    h256!("0x9bd7e06f3ecf4be0f2fcd2188b23f1b9fcc88e5d4b65a8637b17723bbda3cce8");

lazy_static! {
    static ref SIGHASH_ALL_DEP_GROUP_TX_HASH: Byte32 = global_genesis_info().dep_group_tx_hash();
    static ref SIGHASH_ALL_CELL_DEP_OUT_POINT: OutPoint = OutPoint::new_builder()
        .tx_hash(SIGHASH_ALL_DEP_GROUP_TX_HASH.clone())
        .index(SIGHASH_ALL_DEP_GROUP_CELL_INDEX.pack())
        .build();
    static ref SIGHASH_ALL_CELL_DEP: CellDep = CellDep::new_builder()
        .out_point(SIGHASH_ALL_CELL_DEP_OUT_POINT.clone())
        .dep_type(DepType::DepGroup.into())
        .build();
}

lazy_static! {
    pub static ref CELLBASE_MATURITY: Mutex<u64> = Mutex::new(0);
}

fn main() {
    match commandline() {
        CommandLine::MineMode(config, blocks) => {
            init_logger(&config);
            init_global_genesis_info(&config);

            let miner = Miner::new(config.clone(), &config.miner_private_key);
            miner.generate_blocks(blocks)
        }
        CommandLine::BenchMode(config, duration) => {
            init_logger(&config);
            init_metrics(&config);
            init_global_genesis_info(&config);

            let miner = Miner::new(config.clone(), &config.miner_private_key);
            let bencher = Account::new(&config.bencher_private_key);
            let rpcs = connect_jsonrpcs(&config.node_urls);

            if config.start_miner {
                miner.async_mine();
            }
            miner.wait_txpool_empty(config.start_miner);
            TPSCalculator::new(&config).async_run();

            if miner.lock_script() != bencher.lock_script() {
                let _ = run_account_threads(
                    miner.account().clone(),
                    bencher.clone(),
                    rpcs[0].clone(),
                    config.transaction_type,
                    duration,
                );
            }
            run_account_threads(
                bencher.clone(),
                bencher.clone(),
                rpcs[0].clone(),
                config.transaction_type,
                duration,
            )
            .join()
            .unwrap();
        }
    }
}

fn run_account_threads(
    sender: Account,
    recipient: Account,
    rpc: Jsonrpc,
    transaction_type: TransactionType,
    duration: Option<Duration>,
) -> JoinHandle<()> {
    let (utxo_sender, utxo_receiver) = bounded(2000);
    let cursor_number = rpc.get_tip_block_number();
    info!("START account.pull_until");
    let (matureds, unmatureds) = sender.pull_until(&rpc, cursor_number);
    info!("DONE account.pull_until");
    let sender_clone = sender.clone();
    let rpc_clone = rpc.clone();
    spawn(move || {
        matureds.into_iter().for_each(|utxo| {
            utxo_sender.send(utxo).unwrap();
        });
        sender_clone.pull_forever(rpc_clone, cursor_number, unmatureds, utxo_sender);
    });
    spawn(move || {
        sender.transfer_forever(recipient, rpc, utxo_receiver, transaction_type, duration)
    })
}

fn connect_jsonrpcs(urls: &[Url]) -> Vec<Jsonrpc> {
    let nnode = urls.len();
    let mut rpcs = Vec::with_capacity(nnode);
    for url in urls.iter() {
        match Jsonrpc::connect(url.as_str()) {
            Ok(rpc) => rpcs.push(rpc),
            Err(err) => prompt_and_exit!("Jsonrpc::connect({}) error: {}", url.as_str(), err),
        }
    }
    rpcs
}

fn init_logger(config: &Config) {
    WriteLogger::init(
        LevelFilter::Info,
        Default::default(),
        OpenOptions::new()
            .create(true)
            .write(true)
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
