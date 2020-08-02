#[macro_use]
extern crate clap;

use crate::account::Account;
use crate::command::{commandline, CommandLine};
use crate::config::{Config, TransactionType, Url};
use crate::controller::Controller;
use crate::genesis_info::{global_genesis_info, init_global_genesis_info, GenesisInfo};
use crate::global_controller::GlobalController;
use crate::miner::Miner;
use crate::rpc::Jsonrpc;
use crate::transfer::sign_transaction;
use crate::util::estimate_fee;
use crate::utxo::UTXO;
use ckb_crypto::secp::{Privkey, Pubkey};
use ckb_hash::blake2b_256;
use ckb_jsonrpc_types::Status;
use ckb_types::core::{self, DepType, ScriptHashType};
use ckb_types::packed::{
    Block, Byte32, CellDep, CellInput, CellOutput, OutPoint, Script, WitnessArgs,
};
use ckb_types::prelude::*;
use ckb_types::{bytes::Bytes, h160, h256, H160, H256};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::mem;
use std::str::FromStr;
use std::sync::Mutex;
use std::thread::{sleep, spawn, JoinHandle};
use std::time::{Duration, Instant};

pub mod miner;
pub mod transfer;
pub mod util;
pub mod account;
pub mod command;
pub mod config;
pub mod controller;
pub mod genesis_info;
pub mod global_controller;
pub mod rpc;
pub mod utxo;

/// Bench Account Info, type `ckb-cli util key-info <privkey-path>` to generate the account info,
///
/// ```ignore
/// $ cat privkey.txt
/// 1111111111111111111111111111111111111111111111111111111111111111
///
/// $ ckb-cli util key-info --privkey-path privkey.txt
/// Put this config in < ckb.toml >:
///
/// [block_assembler]
/// code_hash = "0x9bd7e06f3ecf4be0f2fcd2188b23f1b9fcc88e5d4b65a8637b17723bbda3cce8"
/// hash_type = "type"
/// args = "0xf949a9cc83edefcd580eb3f0f3bae187c4d008db"
/// message = "0x"
///
/// address:
///   mainnet: ckb1qyq0jjdfejp7mm7dtq8t8u8nhtsc03xsprdsqk6hek
///   testnet: ckt1qyq0jjdfejp7mm7dtq8t8u8nhtsc03xsprdsanyg42
/// lock_arg: 0xf949a9cc83edefcd580eb3f0f3bae187c4d008db
/// lock_hash: 0x827da7c1bd9514ed493a6e9c54cb614865d474d49a6e8f753ce4a472cf8c5fe8
/// pubkey: 034f355bdcb7cc0af728ef3cceb9615d90684bb5b2ca5f859ab0f0b704075871aa
/// ```
pub const BENCH_ACCOUNT_PRIVATE_KEY_STR: &str =
    "1111111111111111111111111111111111111111111111111111111111111111";
pub const BENCH_ACCOUNT_PUBLIC_KEY_STR: &str =
    "034f355bdcb7cc0af728ef3cceb9615d90684bb5b2ca5f859ab0f0b704075871aa";
pub const BENCH_ACCOUNT_ADDRESS_STR: &str = "ckt1qyq0jjdfejp7mm7dtq8t8u8nhtsc03xsprdsanyg42";
pub const BENCH_ACCOUNT_LOCK_ARG: H160 = h160!("0xf949a9cc83edefcd580eb3f0f3bae187c4d008db");
lazy_static! {
    static ref BENCH_ACCOUNT_PRIVATE_KEY: Privkey =
        Privkey::from_str(BENCH_ACCOUNT_PRIVATE_KEY_STR).unwrap();
    static ref BENCH_ACCOUNT_PUBLIC_KEY: Pubkey = BENCH_ACCOUNT_PRIVATE_KEY.pubkey().unwrap();
    static ref BENCH_ACCOUNT_LOCK_SCRIPT: Script = Script::new_builder()
        .args(BENCH_ACCOUNT_LOCK_ARG.0.pack())
        .code_hash(SIGHASH_ALL_TYPE_HASH.pack())
        .hash_type(ScriptHashType::Type.into())
        .build();
    static ref BENCH_ACCOUNT_LOCK_HASH: Byte32 = BENCH_ACCOUNT_LOCK_SCRIPT.calc_script_hash();
}
pub const MIN_SECP_CELL_CAPACITY: u64 = 61_0000_0000;

/// Network Parameters
pub const BLOCK_TIME: Duration = Duration::from_secs(2);
pub const SYSTEM_TRANSACTION_INDEX: usize = 0;
pub const DEP_GROUP_TRANSACTION_INDEX: usize = 1;
pub const SIGHASH_ALL_SYSTEM_CELL_INDEX: usize = 1;
pub const SIGHASH_ALL_DEP_GROUP_CELL_INDEX: usize = 0;
pub const SIGHASH_ALL_TYPE_HASH: H256 =
    h256!("0x9bd7e06f3ecf4be0f2fcd2188b23f1b9fcc88e5d4b65a8637b17723bbda3cce8");
lazy_static! {
    // The `[block_assembler]` configured in ckb node ckb.toml
    static ref CKB_BLOCK_ASSMEBLER_LOCK_HASH: Mutex<Byte32> = Mutex::new(Default::default());

    static ref SIGHASH_ALL_DEP_GROUP_TX_HASH: Byte32 = {
        global_genesis_info().dep_group_tx_hash()
    };
    static ref SIGHASH_ALL_CELL_DEP_OUT_POINT: OutPoint = OutPoint::new_builder()
        .tx_hash(SIGHASH_ALL_DEP_GROUP_TX_HASH.clone())
        .index(SIGHASH_ALL_DEP_GROUP_CELL_INDEX.pack())
        .build();
    static ref SIGHASH_ALL_CELL_DEP: CellDep = CellDep::new_builder()
        .out_point(SIGHASH_ALL_CELL_DEP_OUT_POINT.clone())
        .dep_type(DepType::DepGroup.into())
        .build();
}

fn main() {
    match commandline() {
        CommandLine::MineMode(config, blocks) => {
            init_global_genesis_info(&config);

            let miner = Miner::new(config.clone(), &config.miner_private_key);
            miner.generate_blocks(blocks)
        }
        CommandLine::BenchMode(config, duration) => {
            init_global_genesis_info(&config);

            let miner = Miner::new(config.clone(), &config.miner_private_key);
            let bencher = Account::new(&config.bencher_private_key);
            let rpcs = connect_jsonrpcs(&config.node_urls);

            if config.start_miner {
                miner.async_mine();
            }
            miner.wait_txpool_empty(config.start_miner);

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

    println!("start pull_until");
    let (matureds, unmatureds) = sender.pull_until(&rpc, cursor_number);
    println!(
        "end pull_until, matured: {}, unmatured: {}",
        matureds.len(),
        unmatureds.len()
    );

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
