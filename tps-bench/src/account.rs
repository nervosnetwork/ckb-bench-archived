#![allow(clippy::mutable_key_type)]
use crate::config::TransactionType;
use crate::global::{CELLBASE_MATURITY, MIN_SECP_CELL_CAPACITY, SIGHASH_ALL_TYPE_HASH};
use crate::net::Net;
use crate::prompt_and_exit;
use crate::rpc::Jsonrpc;
use crate::transfer::{construct_unsigned_transaction, sign_transaction};
use crate::util::estimate_fee;
use crate::utxo::UTXO;

use ckb_crypto::secp::Privkey;
use ckb_hash::blake2b_256;
use ckb_types::core;
use ckb_types::core::{BlockNumber, BlockView, HeaderView, ScriptHashType};
use ckb_types::packed::{Byte32, CellOutput, OutPoint, Script};
use ckb_types::prelude::*;
use ckb_types::H160;
use crossbeam_channel::{bounded, Receiver, Sender};
use log::info;
use std::collections::HashMap;
use std::str::FromStr;
use std::thread::{sleep, spawn};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct Account {
    privkey: Privkey,
}

impl Account {
    pub fn new(private_key: &str) -> Self {
        let privkey = match Privkey::from_str(&private_key) {
            Ok(privkey) => privkey,
            Err(err) => prompt_and_exit!("Privkey::from_str({}): {:?}", private_key, err),
        };
        Self { privkey }
    }

    pub fn lock_hash(&self) -> Byte32 {
        self.lock_script().calc_script_hash()
    }

    pub fn lock_script(&self) -> Script {
        let pubkey = self.privkey.pubkey().unwrap();
        let address: H160 = H160::from_slice(&blake2b_256(pubkey.serialize())[0..20]).unwrap();
        Script::new_builder()
            .args(address.0.pack())
            .code_hash(SIGHASH_ALL_TYPE_HASH.pack())
            .hash_type(ScriptHashType::Type.into())
            .build()
    }

    // TODO multiple net
    // Search the blockchain `[from_number, to_number]` and return the live utxos owned by `privkey`
    //
    // This process will not care about chain re-organize problem.
    pub fn pull_until(
        &self,
        rpc: &Jsonrpc,
        until_header: &HeaderView,
    ) -> (
        HashMap<OutPoint, CellOutput>,
        HashMap<OutPoint, (BlockNumber, CellOutput)>,
    ) {
        info!(
            "[START] Account::pull_until({}, {})",
            rpc.uri(),
            until_header.number()
        );
        let mut unmatureds: HashMap<OutPoint, (BlockNumber, CellOutput)> = HashMap::default();
        let mut utxoset: HashMap<OutPoint, CellOutput> = HashMap::default();

        let start_time = Instant::now();
        let mut last_print = Instant::now();
        for number in 0..=until_header.number() {
            if last_print.elapsed() > Duration::from_secs(10) {
                last_print = Instant::now();
                info!(
                    "synchronization progress ({}/{}) ...",
                    number,
                    until_header.number()
                );
            }

            let block: BlockView = rpc
                .get_block_by_number(number)
                .expect("get_block_by_number")
                .into();

            let (matured, unmatured) = self.get_owned_utxos(&block);
            // Add newly UTXOs
            for utxo in matured {
                utxoset.insert(utxo.out_point().clone(), utxo.output().clone());
            }

            for utxo in unmatured {
                if is_matured(until_header.number(), number) {
                    utxoset.insert(utxo.out_point().clone(), utxo.output().clone());
                } else {
                    unmatureds.insert(
                        utxo.out_point().clone(),
                        (block.number(), utxo.output().clone()),
                    );
                }
            }
            // Remove spent UTXOs
            for transaction in block.transactions() {
                for input_out_point in transaction.input_pts_iter() {
                    utxoset.remove(&input_out_point);
                    unmatureds.remove(&input_out_point);
                }
            }
        }
        info!("complete synchronization, took {:?}", start_time.elapsed());
        info!(
            "[END] Account::pull_until({}, {}) matureds: {}, unmatureds: {}",
            rpc.uri(),
            until_header.number(),
            utxoset.len(),
            unmatureds.len()
        );

        (utxoset, unmatureds)
    }

    pub fn pull_from_block_number(
        &self,
        net: &Net,
        block_number: BlockNumber,
        utxoset: &mut HashMap<OutPoint, CellOutput>,
        unmatureds: &mut HashMap<OutPoint, (BlockNumber, CellOutput)>,
    ) {
        let block: BlockView = net.get_block_by_number(block_number).unwrap().into();
        let (matured, unmatured) = self.get_owned_utxos(&block);
        for utxo in matured {
            utxoset.insert(utxo.out_point().clone(), utxo.output().clone());
        }
        for tx in block.transactions() {
            for input_output_point in tx.input_pts_iter() {
                utxoset.remove(&input_output_point);
                unmatureds.remove(&input_output_point);
            }
        }
        for utxo in unmatured {
            unmatureds.insert(
                utxo.out_point().clone(),
                (block_number, utxo.output().clone()),
            );
        }
        let mut unmatured_utxo: Vec<_> = unmatureds
            .iter_mut()
            .map(|(out_point, (number, output))| {
                (*number, UTXO::new(output.clone(), out_point.clone()))
            })
            .collect();
        unmatured_utxo.sort_by_key(|(number, _)| *number);
        while let Some(true) = unmatured_utxo
            .first()
            .map(|number_and_utxo| is_matured(block_number, number_and_utxo.0))
        {
            let (_, utxo) = unmatured_utxo.remove(0);
            utxoset.insert(utxo.out_point().clone(), utxo.output().clone());
            unmatureds.remove(utxo.out_point());
        }
    }

    pub fn construct_utxo_vec(
        &self,
        utxoset: HashMap<OutPoint, CellOutput>,
        unmatureds: HashMap<OutPoint, (BlockNumber, CellOutput)>,
    ) -> (Vec<UTXO>, Vec<(BlockNumber, UTXO)>) {
        let mut unmatureds: Vec<_> = unmatureds
            .into_iter()
            .map(|(out_point, (number, output))| (number, UTXO::new(output, out_point)))
            .collect();
        unmatureds.sort_by_key(|(number, _)| *number);
        (
            utxoset
                .into_iter()
                .map(|(out_point, output)| UTXO::new(output, out_point))
                .collect(),
            unmatureds,
        )
    }

    /// Search (from_number, infinity)
    pub fn pull_forever(
        &self,
        net: Net,
        from_header: HeaderView,
        mut unmatureds: Vec<(BlockNumber, UTXO)>,
        utxo_sender: Sender<UTXO>,
    ) {
        let mut current_header = from_header;
        loop {
            if let Some(header) = net.get_fixed_header(current_header.number() + 1) {
                // Chain has been re-organized! Rollback to the fixed point!
                if header.parent_hash() != current_header.hash() {
                    current_header = rollback_for_reorg(&net, &current_header);
                    continue;
                }

                // net.get_block return None when re-organize
                if let Some(block) = net.get_block(header.hash()) {
                    current_header = header;
                    let block: BlockView = block.into();
                    let (matured, unmatured) = self.get_owned_utxos(&block);
                    for utxo in matured {
                        if utxo_sender.send(utxo).is_err() {
                            return;
                        }
                    }
                    while let Some(true) = unmatureds.first().map(|number_and_utxo| {
                        is_matured(current_header.number(), number_and_utxo.0)
                    }) {
                        let (_, utxo) = unmatureds.remove(0);
                        if utxo_sender.send(utxo).is_err() {
                            return;
                        }
                    }

                    // Collect the un-matured utxos in vector
                    for utxo in unmatured {
                        unmatureds.push((block.number(), utxo));
                    }
                }
            } else {
                sleep(Duration::from_millis(500));
            }
        }
    }

    pub fn transfer_forever(
        &self,
        recipient: Account,
        net: Net,
        utxo_receiver: Receiver<UTXO>,
        transaction_type: TransactionType,
        duration: Option<Duration>,
    ) {
        let start_time = Instant::now();
        let outputs_count = transaction_type.outputs_count() as u64;
        let min_input_total_capacity =
            outputs_count * MIN_SECP_CELL_CAPACITY + estimate_fee(outputs_count);
        let (mut inputs, mut input_total_capacity) = (Vec::new(), 0);

        let senders = net
            .endpoints()
            .iter()
            .map(|rpc| {
                let rpc = rpc.clone();
                let (sender, receiver) = bounded(1000);
                spawn(move || {
                    while let Ok(transaction) = receiver.recv() {
                        if let Err(err) = retry_send(&rpc, &transaction) {
                            panic!(err)
                        }
                    }
                });
                sender
            })
            .collect::<Vec<_>>();

        info!("START account.transfer_forever");
        let mut cursor = 0;
        while let Ok(utxo) = utxo_receiver.recv() {
            input_total_capacity += utxo.capacity();
            inputs.push(utxo);
            if input_total_capacity < min_input_total_capacity {
                continue;
            }

            input_total_capacity = 0;
            let raw_transaction =
                construct_unsigned_transaction(&recipient, inputs.split_off(0), outputs_count);
            let signed_transaction = sign_transaction(&self, raw_transaction);

            cursor = (cursor + 1) % senders.len();
            if senders[cursor].send(signed_transaction).is_err() {
                break;
            }
            if duration.map(|d| start_time.elapsed() > d).unwrap_or(false) {
                break;
            }
        }
        info!("START account.transfer_forever");
    }

    pub fn privkey(&self) -> &Privkey {
        &self.privkey
    }

    pub fn get_owned_utxos(&self, block: &BlockView) -> (Vec<UTXO>, Vec<UTXO>) {
        let lock_script = self.lock_script();
        let (mut unmatured, mut matured) = (Vec::new(), Vec::new());
        for (tx_index, transaction) in block.transactions().into_iter().enumerate() {
            for (index, output) in transaction.outputs().into_iter().enumerate() {
                let output: CellOutput = output;
                if lock_script != output.lock() {
                    continue;
                }

                let out_point = OutPoint::new_builder()
                    .tx_hash(transaction.hash())
                    .index(index.pack())
                    .build();
                let utxo = UTXO::new(output, out_point);

                if tx_index == 0 {
                    unmatured.push(utxo)
                } else {
                    matured.push(utxo);
                }
            }
        }
        (matured, unmatured)
    }
}

fn is_matured(tip_number: BlockNumber, number: BlockNumber) -> bool {
    tip_number > number + 1800 * *CELLBASE_MATURITY.lock().unwrap()
}

fn retry_send(rpc: &Jsonrpc, transaction: &core::TransactionView) -> Result<(), String> {
    loop {
        match rpc.send_transaction_result(transaction.data().into()) {
            Err(err) => {
                let err_str = err.to_string();
                if err_str.contains("TransactionPoolFull") || err_str.contains("PoolIsFull") {
                    sleep(Duration::from_secs(1));
                    continue;
                }
                return Err(err.to_string());
            }
            Ok(_) => {
                return Ok(());
            }
        }
    }
}

fn rollback_for_reorg(net: &Net, old_header: &HeaderView) -> HeaderView {
    // NOTE: We cannot find the exactly fixed point of old_header and new tip header based on ckb
    // rpc interfaces.

    net.get_header_by_number(old_header.number().saturating_sub(1000))
        .unwrap_or_else(|| panic!("rollback_for_org(old_header={:?})", old_header))
        .into()
}
