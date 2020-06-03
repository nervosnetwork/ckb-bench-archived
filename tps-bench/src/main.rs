#[macro_use]
extern crate clap;

use crate::command::{commandline, CommandLine};
use crate::config::{Config, Url};
use crate::controller::Controller;
use crate::global_controller::GlobalController;
use crate::rpc::Jsonrpc;
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
use std::thread::{sleep, spawn};
use std::time::{Duration, Instant};

pub mod util;
pub mod command;
pub mod config;
pub mod controller;
pub mod global_controller;
pub mod local_controller;
pub mod rpc;

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
pub const MIN_SECP_CELL_CAPACITY: u64 = 60_0000_0000;
pub const INITIAL_CELL_CAPACITY: u64 = MIN_SECP_CELL_CAPACITY * 2 - 1;

/// Network Parameters
pub const MIN_FEE_RATE: u64 = 1_000; // shannons/KB
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

    static ref GENESIS_INFO: Mutex<GenesisInfo> = Mutex::new(GenesisInfo::default());
    static ref SIGHASH_ALL_DEP_GROUP_TX_HASH: Byte32 = {
        let genesis_info = GENESIS_INFO.lock().unwrap();
        genesis_info.dep_group_tx_hash()
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
        CommandLine::MineMode(config, blocks) => mine_mode(&config, blocks),
        CommandLine::BenchMode(config, duration) => {
            init_genesis_info(&config);
            spawn_miner(&config);
            bench_mode(&config, duration)
        }
        CommandLine::InitBenchAccount(config) => {
            init_genesis_info(&config);
            spawn_miner(&config);

            let miner_privkey = match Privkey::from_str(&config.private_key) {
                Ok(privkey) => {
                    let configured_miner_lock_hash = lock_hash(&privkey);
                    let block_assembler_lock_hash = CKB_BLOCK_ASSMEBLER_LOCK_HASH.lock().unwrap();
                    if configured_miner_lock_hash != *block_assembler_lock_hash {
                        println!(
                            "[WARN] configured miner privkey: {}, lock_hash: {}",
                            config.private_key, configured_miner_lock_hash
                        );
                        println!(
                            "[WARN] CKB_BLOCK_ASSMEBLER_LOCK_HASH: {}",
                            configured_miner_lock_hash
                        );
                    }
                    privkey
                }
                Err(err) => prompt_and_exit!("invalid privkey {}: {:?}", config.private_key, err),
            };

            let count = config.transaction_count as usize * config.transaction_type.required();
            let mut c = 0;
            while c < count {
                let delta = ::std::cmp::min(count - c, 1000);
                init_bench_account(&config, &miner_privkey, &BENCH_ACCOUNT_PRIVATE_KEY, delta);
                c += delta;
            }
        }
    }
}

// - Initialize the global variable `GENESIS_INFO`
// - Initialize the configured miner in ckb
fn init_genesis_info(config: &Config) {
    let url = config.node_urls.first().expect("checked");
    let rpc = match Jsonrpc::connect(url.as_str()) {
        Ok(rpc) => rpc,
        Err(err) => prompt_and_exit!("Jsonrpc::connect({}) error: {}", url.as_str(), err),
    };
    match rpc.get_block_by_number(0) {
        Some(genesis_block) => {
            let genesis_block: core::BlockView = genesis_block.into();
            let mut genesis_info = GENESIS_INFO.lock().unwrap();
            *genesis_info = GenesisInfo::from(genesis_block);
        }
        None => prompt_and_exit!(
            "Jsonrpc::get_block_by_number(0) from {} error: return None",
            url.as_str()
        ),
    }

    let mut block_assembler_lock_hash = CKB_BLOCK_ASSMEBLER_LOCK_HASH.lock().unwrap();
    *block_assembler_lock_hash = rpc
        .get_block_template(None, None, None)
        .cellbase
        .data
        .outputs
        .get(0)
        .unwrap()
        .lock
        .code_hash
        .0
        .pack();
}

fn mine_mode(config: &Config, blocks: u64) {
    let url = config.node_urls.first().expect("checked");
    let rpc = match Jsonrpc::connect(url.as_str()) {
        Ok(rpc) => rpc,
        Err(err) => prompt_and_exit!("Jsonrpc::connect({}) error: {}", url.as_str(), err),
    };
    for _ in 0..blocks {
        let template = rpc.get_block_template(None, None, None);
        let work_id = template.work_id.value();
        let block_number = template.number.value();
        let block: Block = template.into();
        if let Some(block_hash) = rpc.submit_block(work_id.to_string(), block.into()) {
            println!("submit block  #{} {:#x}", block_number, block_hash);
        } else {
            eprintln!("submit block  #{} None", block_number);
        }
    }
}

#[derive(Debug)]
pub struct UTXO {
    output: CellOutput,
    out_point: OutPoint,
}

impl UTXO {
    pub fn new(output: CellOutput, out_point: OutPoint) -> Self {
        Self { output, out_point }
    }

    pub fn output(&self) -> &CellOutput {
        &self.output
    }

    pub fn out_point(&self) -> &OutPoint {
        &self.out_point
    }

    pub fn capacity(&self) -> u64 {
        self.output.capacity().unpack()
    }

    pub fn as_previous_input(&self) -> CellInput {
        CellInput::new_builder()
            .previous_output(self.out_point().clone())
            .build()
    }
}

// TODO handle un-matured utxos
fn filter_utxos_from_block(lock_hash: &Byte32, block: &core::BlockView) -> Vec<UTXO> {
    let mut utxos = Vec::new();
    for (_tx_index, transaction) in block.transactions().into_iter().enumerate() {
        for (index, output) in transaction.outputs().into_iter().enumerate() {
            let output: CellOutput = output;
            if lock_hash != &output.lock().calc_script_hash() {
                continue;
            }

            let out_point = OutPoint::new_builder()
                .tx_hash(transaction.hash())
                .index(index.pack())
                .build();
            let utxo = UTXO { output, out_point };
            utxos.push(utxo);
        }
    }
    utxos
}

// Search the blockchain `[from_number, to_number]` and return the live utxos owned by `privkey`
fn filter_utxos_from_chain(
    lock_hash: &Byte32,
    rpc: &Jsonrpc,
    from_number: u64,
    to_number: u64,
) -> Vec<UTXO> {
    #[allow(clippy::mutable_key_type)]
    let mut utxos: HashMap<OutPoint, CellOutput> = HashMap::default();
    for number in from_number..=to_number {
        let block: core::BlockView = rpc
            .get_block_by_number(number)
            .expect("get_block_by_number")
            .into();
        for utxo in filter_utxos_from_block(lock_hash, &block) {
            utxos.insert(utxo.out_point, utxo.output);
        }
        for transaction in block.transactions() {
            for input_out_point in transaction.input_pts_iter() {
                utxos.remove(&input_out_point);
            }
        }
    }
    utxos
        .into_iter()
        .map(|(out_point, output)| UTXO { out_point, output })
        .collect()
}

fn spawn_miner(config: &Config) {
    let url = &config.node_urls[0];
    let rpc = match Jsonrpc::connect(url.as_str()) {
        Ok(rpc) => rpc,
        Err(err) => prompt_and_exit!("Jsonrpc::connect({}) error: {}", url.as_str(), err),
    };

    // Ensure the pending and proposed transactions be committed
    for _ in 0..20 {
        mine_block(&rpc);
    }
    spawn(move || loop {
        sleep(BLOCK_TIME);
        mine_block(&rpc);
    });
}

fn mine_block(rpc: &Jsonrpc) {
    let template = rpc.get_block_template(None, None, None);
    let work_id = template.work_id.value();
    let _block_number = template.number.value();
    let block: Block = template.into();

    if let Some(_block_hash) = rpc.submit_block(work_id.to_string(), block.into()) {
        // TODO println!("submit_block  #{} {:#x}", block_number, block_hash);
    }
}

fn bench_mode(config: &Config, duration: Duration) {
    let start_time = Instant::now();
    let (block_sender, block_receiver) = unbounded();
    let (utxo_sender, utxo_receiver) = unbounded();
    let (transaction_sender, transaction_receiver) = unbounded();

    // Walk through the chain from genesis to tip, filter the owned utxos
    let rpcs = connect_jsonrpcs(&config.node_urls);
    let tip_number = rpcs
        .iter()
        .map(|rpc| rpc.get_tip_block_number())
        .max()
        .unwrap();
    let utxos = filter_utxos_from_chain(&BENCH_ACCOUNT_LOCK_HASH, &rpcs[0], 0, tip_number);
    utxos
        .into_iter()
        .for_each(|utxo| utxo_sender.send(utxo).unwrap());

    // A thread for monitoring new blocks
    let mut current_number = tip_number;
    let config_clone = config.clone();
    spawn(move || {
        let config = config_clone;
        let rpcs = connect_jsonrpcs(&config.node_urls);

        loop {
            let zero_hash: Byte32 = h256!("0x0").0.pack();
            let mut new_hash = zero_hash.clone();
            for rpc in rpcs.iter() {
                match rpc.get_block_by_number(current_number + 1) {
                    Some(block) => {
                        let block: core::BlockView = block.into();
                        let block_hash = block.hash();
                        if new_hash == zero_hash {
                            new_hash = block_hash;
                        } else if new_hash != block.hash() {
                            // inconsistent chain state, break and wait a minute
                            new_hash = zero_hash.clone();
                            break;
                        }
                    }
                    None => {
                        // inconsistent chain state, break and wait a minute
                        new_hash = zero_hash.clone();
                        break;
                    }
                }
            }

            if new_hash != zero_hash {
                let block: core::BlockView = rpcs[0]
                    .get_block(new_hash)
                    .expect("block existence checked")
                    .into();
                if block_sender.send(block).is_err() {
                    return;
                }

                current_number += 1;
            }
        }
    });

    // A thread for receiving blocks and extract owned utxos by bench account
    spawn(move || {
        while let Ok(block) = block_receiver.recv() {
            let utxos = filter_utxos_from_block(&BENCH_ACCOUNT_LOCK_HASH, &block);
            for utxo in utxos {
                if utxo_sender.send(utxo).is_err() {
                    return;
                }
            }
        }
    });

    // A thread for receiving utxos and construct new transactions
    let config_clone = config.clone();
    spawn(move || {
        let config = config_clone;
        let mut utxos: Vec<UTXO> = Vec::new();
        while let Ok(utxo) = utxo_receiver.recv() {
            utxos.push(utxo);
            if config.transaction_type.required() == utxos.len() {
                let mut inputs = Vec::new();
                mem::swap(&mut inputs, &mut utxos);
                let transaction = construct_bench_transaction(&config, inputs);
                if transaction_sender.send(transaction).is_err() {
                    break;
                }
            }
        }
    });

    // Multiple threads for sending transaction
    let rpcs = connect_jsonrpcs(&config.node_urls);
    let rpc_transaction_senders = rpcs
        .into_iter()
        .map(|rpc| {
            let (rpc_transaction_sender, rpc_transaction_receiver): (
                Sender<core::TransactionView>,
                Receiver<core::TransactionView>,
            ) = bounded(1);
            spawn(move || {
                while let Ok(transaction) = rpc_transaction_receiver.recv() {
                    rpc.send_transaction_result(transaction.data().into())
                        .expect("there is bug, let it crash early");
                }
            });
            rpc_transaction_sender
        })
        .collect::<Vec<_>>();

    let nnode = config.node_urls.len();
    let mut inode = 0;
    let mut controller = GlobalController::new(&config);
    while start_time.elapsed() < duration {
        match transaction_receiver.recv() {
            Ok(transaction) => {
                if rpc_transaction_senders[inode].send(transaction).is_err() {
                    break;
                }
                inode = (inode + 1) % nnode;

                let sleep_time = controller.add();
                sleep(sleep_time);
            }
            Err(_err) => {
                break;
            }
        }
    }
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

fn construct_bench_transaction(config: &Config, utxos: Vec<UTXO>) -> core::TransactionView {
    let required = config.transaction_type.required();
    assert_eq!(required, utxos.len());

    let input_total_capacity = utxos.iter().map(|input| input.capacity()).sum::<u64>();
    let fee = estimate_fee(config);
    let output_total_capacity = input_total_capacity - fee;

    let inputs = utxos
        .iter()
        .map(|input| {
            CellInput::new_builder()
                .previous_output(input.out_point().clone())
                .build()
        })
        .collect::<Vec<_>>();
    let outputs = (0..required)
        .map(|i| {
            let capacity = if (i as u64) < output_total_capacity % (required as u64) {
                output_total_capacity / required as u64 + 1
            } else {
                output_total_capacity / required as u64
            };
            CellOutput::new_builder()
                .lock(BENCH_ACCOUNT_LOCK_SCRIPT.clone())
                .capacity(capacity.pack())
                .build()
        })
        .collect::<Vec<_>>();
    let outputs_data = (0..required)
        .map(|_| Default::default())
        .collect::<Vec<_>>();
    let raw_transaction = core::TransactionBuilder::default()
        .inputs(inputs)
        .outputs(outputs)
        .outputs_data(outputs_data)
        .cell_dep(SIGHASH_ALL_CELL_DEP.clone())
        .build();
    sign_transaction(&BENCH_ACCOUNT_PRIVATE_KEY, raw_transaction)
}

fn lock_hash(privkey: &Privkey) -> Byte32 {
    lock_script(privkey).calc_script_hash()
}

fn lock_script(privkey: &Privkey) -> Script {
    let pubkey = privkey.pubkey().unwrap();
    let address: H160 = H160::from_slice(&blake2b_256(pubkey.serialize())[0..20]).unwrap();
    Script::new_builder()
        .args(address.0.pack())
        .code_hash(SIGHASH_ALL_TYPE_HASH.pack())
        .hash_type(ScriptHashType::Type.into())
        .build()
}

// fn transfer_all(config: &Config, sender: &Privkey, receiver: &Privkey) {
// TODO complete this function
// }

// TODO if count > MAX_COUNT then transfer_initial_cells multiple times
fn init_bench_account(config: &Config, sender: &Privkey, receiver: &Privkey, count: usize) {
    let rpcs = connect_jsonrpcs(&config.node_urls);
    let tip_number = rpcs
        .iter()
        .map(|rpc| rpc.get_tip_block_number())
        .max()
        .unwrap();

    // Construct outputs
    let mut outputs = (0..count)
        .map(|_| {
            CellOutput::new_builder()
                .lock(lock_script(receiver))
                .capacity(INITIAL_CELL_CAPACITY.pack())
                .build()
        })
        .collect::<Vec<_>>();
    let output_total_capacity = count as u64 * INITIAL_CELL_CAPACITY;

    // Construct inputs, inputs' total capacity greater than outputs' total capacity adding estimated fee
    let estimated_fee = 10000 * outputs.len() as u64; // FIXME
    let sender_lock_hash = lock_hash(sender);
    let mut inputs = Vec::new();
    let mut input_total_capacity = 0;
    let utxos = filter_utxos_from_chain(&sender_lock_hash, &rpcs[0], 0, tip_number);
    assert!(!utxos.is_empty());

    for utxo in utxos.into_iter() {
        inputs.push(utxo.as_previous_input());
        input_total_capacity += utxo.capacity();
        if input_total_capacity >= estimated_fee + output_total_capacity {
            break;
        }
    }
    assert!(input_total_capacity >= estimated_fee + output_total_capacity);

    // Handle last output
    let last_output = outputs.pop().unwrap();
    let last_output_capacity: u64 = last_output.capacity().unpack();
    let last_output = last_output
        .as_builder()
        .capacity(
            (last_output_capacity + input_total_capacity - estimated_fee - output_total_capacity)
                .pack(),
        )
        .build();
    outputs.push(last_output);

    // Construct transaction
    let outputs_data = (0..outputs.len())
        .map(|_| Default::default())
        .collect::<Vec<_>>();
    let raw_transaction = core::TransactionBuilder::default()
        .inputs(inputs)
        .outputs(outputs)
        .outputs_data(outputs_data)
        .cell_dep(SIGHASH_ALL_CELL_DEP.clone())
        .build();
    let transaction = sign_transaction(&sender, raw_transaction);
    let tx_hash = rpcs[0].send_transaction(transaction.data().into()).pack();

    wait_until_committed(&rpcs[0], &tx_hash);
}

fn wait_until_committed(rpc: &Jsonrpc, tx_hash: &Byte32) {
    loop {
        if let Some(tx_result) = rpc.get_transaction(tx_hash.clone()) {
            if tx_result.tx_status.status == Status::Committed {
                return;
            }
        }
    }
}

fn sign_transaction(privkey: &Privkey, tx: core::TransactionView) -> core::TransactionView {
    let tx_hash = tx.hash();

    let mut blake2b = ckb_hash::new_blake2b();
    let mut message = [0u8; 32];
    blake2b.update(&tx_hash.raw_data());
    let witness_for_digest = WitnessArgs::new_builder()
        .lock(Some(Bytes::from(vec![0u8; 65])).pack())
        .build();
    let witness_len = witness_for_digest.as_bytes().len() as u64;
    blake2b.update(&witness_len.to_le_bytes());
    blake2b.update(&witness_for_digest.as_bytes());
    blake2b.finalize(&mut message);
    let message = H256::from(message);
    let sig = privkey.sign_recoverable(&message).expect("sign");
    let signed_witness = WitnessArgs::new_builder()
        .lock(Some(Bytes::from(sig.serialize())).pack())
        .build()
        .as_bytes()
        .pack();

    // calculate message
    tx.as_advanced_builder()
        .set_witnesses(vec![signed_witness])
        .build()
}

// TODO estimate fee MIN_FEE_RATE
fn estimate_fee(config: &Config) -> u64 {
    1000 * config.transaction_type.required() as u64
}

pub struct GenesisInfo {
    block: core::BlockView,
}

impl From<core::BlockView> for GenesisInfo {
    fn from(block: core::BlockView) -> Self {
        assert_eq!(block.number(), 0);
        Self { block }
    }
}

impl Default for GenesisInfo {
    fn default() -> Self {
        Self {
            block: core::BlockBuilder::default().build(),
        }
    }
}

impl GenesisInfo {
    pub fn dep_group_tx_hash(&self) -> Byte32 {
        let dep_group_tx = self
            .block
            .transaction(DEP_GROUP_TRANSACTION_INDEX)
            .expect("genesis block should contain at least 2 transactions");
        dep_group_tx.hash()
    }
}

// - The structure of transaction
// - The number of nodes in the network
fn print_parameters(config: &Config) {
    let tx_type = config.transaction_type;
}
