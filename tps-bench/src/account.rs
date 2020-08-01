use crate::config::TransactionType;
use crate::rpc::Jsonrpc;
use crate::transfer::{construct_unsigned_transaction, sign_transaction};
use crate::util::estimate_fee;
use crate::utxo::UTXO;
use crate::{MIN_SECP_CELL_CAPACITY, SIGHASH_ALL_TYPE_HASH};
use ckb_crypto::secp::Privkey;
use ckb_hash::blake2b_256;
use ckb_types::core;
use ckb_types::core::{BlockNumber, ScriptHashType};
use ckb_types::packed::{Byte32, CellOutput, OutPoint, Script};
use ckb_types::prelude::*;
use ckb_types::H160;
use crossbeam_channel::{tick, unbounded, Receiver, Sender};
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

    // TODO improve monitor mechanism
    // TODO multiple rpcs
    // Search the blockchain `[from_number, to_number]` and return the live utxos owned by `privkey`
    pub fn pull_until(
        &self,
        rpc: &Jsonrpc,
        until_number: BlockNumber,
    ) -> (Vec<UTXO>, Vec<(BlockNumber, UTXO)>) {
        let mut unmatureds: HashMap<OutPoint, (BlockNumber, CellOutput)> = HashMap::default();
        let mut utxoset: HashMap<OutPoint, CellOutput> = HashMap::default();
        for number in 0..=until_number {
            let block: core::BlockView = rpc
                .get_block_by_number(number)
                .expect("get_block_by_number")
                .into();

            let (matured, unmatured) = self.get_owned_utxos(&block);
            // Add newly UTXOs
            for utxo in matured {
                utxoset.insert(utxo.out_point().clone(), utxo.output().clone());
            }
            // Remove spent UTXOs
            for transaction in block.transactions() {
                for input_out_point in transaction.input_pts_iter() {
                    utxoset.remove(&input_out_point);
                    unmatureds.remove(&input_out_point);
                }
            }
            for utxo in unmatured {
                unmatureds.insert(
                    utxo.out_point().clone(),
                    (block.number(), utxo.output().clone()),
                );
            }
        }

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
        rpc: Jsonrpc,
        from_number: BlockNumber,
        mut unmatureds: Vec<(BlockNumber, UTXO)>,
        utxo_sender: Sender<UTXO>,
    ) {
        let mut number = from_number;
        loop {
            let tip_number = rpc.get_tip_block_number();
            while number < tip_number {
                number += 1;
                let block: core::BlockView = rpc
                    .get_block_by_number(number)
                    .expect("get_block_by_number")
                    .into();

                let (matured, unmatured) = self.get_owned_utxos(&block);
                println!(
                    "pull_forever, block: {}, matured: {}, unmatured: {}, total_unmatured: {}",
                    number,
                    matured.len(),
                    unmatured.len(),
                    unmatureds.len()
                );

                for utxo in matured {
                    if utxo_sender.send(utxo).is_err() {
                        return;
                    }
                }
                while let Some(true) = unmatureds
                    .first()
                    .map(|number_and_utxo| is_matured(tip_number, number_and_utxo))
                {
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

            sleep(Duration::from_secs(1));
        }
    }

    pub fn transfer_forever(
        &self,
        recipient: Account,
        rpc: Jsonrpc,
        utxo_receiver: Receiver<UTXO>,
        transaction_type: TransactionType,
        duration: Option<Duration>,
    ) {
        let start_time = Instant::now();
        let outputs_count = transaction_type.required() as u64;
        let min_input_total_capacity =
            outputs_count * MIN_SECP_CELL_CAPACITY + estimate_fee(outputs_count);
        let (mut inputs, mut input_total_capacity) = (Vec::new(), 0);

        let mut sent = 0;
        let mut last_print = Instant::now();
        while let Ok(utxo) = utxo_receiver.recv() {
            input_total_capacity += utxo.capacity();
            inputs.push(utxo);

            if input_total_capacity < min_input_total_capacity {
                continue;
            }

            sent += 1;
            if last_print.elapsed() > Duration::from_secs(5) {
                last_print = Instant::now();
                println!("transfer_forever, sent: {}", sent);
            }

            input_total_capacity = 0;
            let raw_transaction =
                construct_unsigned_transaction(&recipient, inputs.split_off(0), outputs_count);
            let signed_transaction = sign_transaction(&self, raw_transaction);
            if let Err(err) = rpc.send_transaction_result(signed_transaction.data().into()) {
                let tip_number = rpc.get_tip_block_number();
                let message =
                    signed_transaction.input_pts_iter()
                        .map(|input| format!("input.tx_hash: {}, input.index: {}", input.tx_hash(), input.index()))
                        .collect::<Vec<_>>()
                        .join(";");

                let message = format!("rpc.send_transaction_result: message: {}, error: {:?}", message, err);
                panic!(message)
            }

            if duration.map(|d| start_time.elapsed() > d).unwrap_or(false) {
                break;
            }
        }
    }

    pub fn privkey(&self) -> &Privkey {
        &self.privkey
    }

    fn get_owned_utxos(&self, block: &core::BlockView) -> (Vec<UTXO>, Vec<UTXO>) {
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

fn is_matured(tip_number: BlockNumber, number_and_utxo: &(BlockNumber, UTXO)) -> bool {
    tip_number > number_and_utxo.0 + 1800 * 5
}
