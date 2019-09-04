use crate::config::Config;
use crate::traits::{PackedCapacityAsCapacity, PackedCapacityAsU64};
use crate::types::{LiveCell, Personal, Secp, MIN_SECP_CELL_CAPACITY};
use bytes::Bytes;
use ckb_hash::blake2b_256;
use ckb_types::core::{Capacity, TransactionBuilder, TransactionView as Transaction};
use ckb_types::packed::{CellInput, CellOutput, Witness};
use ckb_types::prelude::*;
use ckb_types::H256;
use failure::{format_err, Error};
use rpc_client::Jsonrpc;
use std::vec::Vec;

pub const MAX_EXPLODE_OUTPUTS: usize = 5000;

pub fn prepare(config: &Config, bank: &Personal, alice: &Personal) -> Result<(), Error> {
    let alice_ = alice.unspent().unsent.len();
    let need = config.serial.transactions * 2;
    let jsonrpc = Jsonrpc::connect(config.rpc_urls[0].as_str())?;
    let secp = Secp::load(&jsonrpc)?;
    let transactions = if need > alice_ {
        issue(bank, alice, secp, need - alice_)
    } else if need < alice_ {
        burn(alice, bank, secp, alice_ - need)
    } else {
        Vec::new()
    };
    for transaction in transactions.into_iter() {
        jsonrpc
            .send_transaction_result(transaction.data().into())
            .map_err(|err| format_err!("{:?}", err))?;
    }
    Ok(())
}

fn burn(
    sender: &Personal,
    receiver: &Personal,
    secp: Secp,
    outputs_count: usize,
) -> Vec<Transaction> {
    let dep = secp.unlock_me_cell_dep();
    sender
        .unspent()
        .unsent_iter()
        .take(outputs_count)
        .map(|(_, previous)| {
            let input = CellInput::new(previous.out_point.clone(), 0);
            let output = CellOutput::new_builder()
                .capacity(previous.cell_output.capacity())
                .lock(receiver.lock_script().clone())
                .build();
            let raw_transaction = TransactionBuilder::default()
                .input(input)
                .output(output)
                .output_data(Bytes::new().pack())
                .cell_dep(dep.clone())
                .build();
            let witness = {
                let hash = raw_transaction.hash();
                let message = H256::from(blake2b_256(hash.as_slice()));
                let signature_bytes = sender
                    .privkey()
                    .sign_recoverable(&message)
                    .unwrap()
                    .serialize();
                Witness::new_builder()
                    .push(Bytes::from(signature_bytes).pack())
                    .build()
            };
            raw_transaction
                .as_advanced_builder()
                .witness(witness)
                .build()
        })
        .collect()
}

fn issue(
    sender: &Personal,
    receiver: &Personal,
    secp: Secp,
    outputs_count: usize,
) -> Vec<Transaction> {
    let mut targets: Vec<CellOutput> = {
        (0..outputs_count)
            .map(|_| {
                let output = CellOutput::new_builder()
                    .lock(receiver.lock_script().clone())
                    .build_exact_capacity(Capacity::zero())
                    .unwrap();
                // TODO refactor it.
                let capacity: Capacity = output.capacity().unpack();
                let capacity = capacity
                    .safe_mul(2 as u64)
                    .unwrap()
                    .safe_sub(1 as u64)
                    .unwrap();
                let output = CellOutput::new_builder()
                    .capacity(capacity.pack())
                    .lock(output.lock())
                    .build();
                let capacity = output
                    .occupied_capacity(Capacity::zero())
                    .unwrap()
                    .safe_mul(2u64)
                    .unwrap()
                    .safe_sub(1u64)
                    .unwrap();
                output.as_builder().capacity(capacity.pack()).build()
            })
            .collect()
    };
    let dep = secp.unlock_me_cell_dep();
    let mut transactions = Vec::new();
    // TODO refactor it
    for (_, previous) in sender.unspent().unsent_iter() {
        if targets.is_empty() {
            break;
        } else if !can_explode(previous) {
            continue;
        }

        let input = CellInput::new(previous.out_point.clone(), 0);
        let mut input_capacity: Capacity = previous.cell_output.capacity().unpack();
        let mut outputs: Vec<CellOutput> = Vec::new();
        while let Some(output) = targets.pop() {
            if input_capacity.as_u64() >= output.capacity().as_u64_capacity() * 2 {
                input_capacity = input_capacity
                    .safe_sub(output.capacity().as_capacity())
                    .unwrap();
                outputs.push(output);
            } else if input_capacity.as_u64() >= output.capacity().as_u64_capacity() {
                let new_output = output.as_builder().capacity(input_capacity.pack()).build();
                input_capacity = Capacity::zero();
                outputs.push(new_output);
                break;
            } else {
                targets.push(output);
            }

            if outputs.len() >= MAX_EXPLODE_OUTPUTS {
                break;
            }
        }
        if input_capacity != Capacity::zero() {
            outputs.push(
                CellOutput::new_builder()
                    .capacity(input_capacity.pack())
                    .lock(sender.lock_script().clone())
                    .build(),
            );
        }

        let raw_transaction = TransactionBuilder::default()
            .input(input)
            .outputs_data((0..outputs.len()).map(|_| Bytes::new().pack()))
            .outputs(outputs)
            .cell_dep(dep.clone())
            .build();
        let witness = {
            let hash = raw_transaction.hash();
            let message = H256::from(blake2b_256(hash.as_slice()));
            let signature_bytes = sender
                .privkey()
                .sign_recoverable(&message)
                .unwrap()
                .serialize();
            Witness::new_builder().push(signature_bytes.pack()).build()
        };
        let transaction = raw_transaction
            .as_advanced_builder()
            .witness(witness)
            .build();
        transactions.push(transaction);
    }
    assert_eq!(targets.len(), 0, "No enough balance");

    transactions
}

fn can_explode(cell: &LiveCell) -> bool {
    cell.cell_output.capacity().as_u64_capacity() >= MIN_SECP_CELL_CAPACITY
}
