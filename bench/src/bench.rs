use crate::config::Config;
use crate::types::{LiveCell, Personal, Secp, MIN_SECP_CELL_CAPACITY};
use ckb_core::transaction::{CellInput, CellOutput, OutPoint, Transaction, TransactionBuilder};
use ckb_core::{Bytes, Capacity};
use ckb_hash::blake2b_256;
use failure::{format_err, Error};
use numext_fixed_hash::H256;
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
    for transaction in transactions.iter() {
        jsonrpc
            .send_transaction_result(transaction.into())
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
    let dep = { OutPoint::new_cell(secp.out_point().tx_hash.clone(), secp.out_point().index) };
    sender
        .unspent()
        .unsent_iter()
        .take(outputs_count)
        .map(|(_, previous)| {
            let input = CellInput::new(previous.out_point.clone(), 0);
            let output = CellOutput {
                capacity: previous.cell_output.capacity,
                data: Bytes::new(),
                lock: receiver.lock_script().clone(),
                type_: None,
            };
            let raw_transaction = TransactionBuilder::default()
                .input(input)
                .output(output)
                .dep(dep.clone())
                .build();
            let witness = {
                let hash = raw_transaction.hash();
                let message = H256::from(blake2b_256(hash));
                let signature_bytes = sender
                    .privkey()
                    .sign_recoverable(&message)
                    .unwrap()
                    .serialize();
                vec![Bytes::from(signature_bytes)]
            };
            TransactionBuilder::from_transaction(raw_transaction)
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
                let mut output = CellOutput {
                    capacity: Capacity::zero(),
                    data: Bytes::new(),
                    lock: receiver.lock_script().clone(),
                    type_: None,
                };
                // TODO refactor it.
                output.capacity = output
                    .occupied_capacity()
                    .unwrap()
                    .safe_mul(2 as u64)
                    .unwrap()
                    .safe_sub(1 as u64)
                    .unwrap();
                output
            })
            .collect()
    };
    let dep = { OutPoint::new_cell(secp.out_point().tx_hash.clone(), secp.out_point().index) };
    let mut transactions = Vec::new();
    // TODO refactor it
    for (_, previous) in sender.unspent().unsent_iter() {
        if targets.is_empty() {
            break;
        } else if !can_explode(previous) {
            continue;
        }

        let input = CellInput::new(previous.out_point.clone(), 0);
        let mut input_capacity = previous.cell_output.capacity;
        let mut outputs: Vec<CellOutput> = Vec::new();
        while let Some(mut output) = targets.pop() {
            if input_capacity.as_u64() >= output.capacity.as_u64() * 2 {
                input_capacity = input_capacity.safe_sub(output.capacity).unwrap();
                outputs.push(output);
            } else if input_capacity.as_u64() >= output.capacity.as_u64() {
                output.capacity = input_capacity;
                input_capacity = Capacity::zero();
                outputs.push(output);
                break;
            } else {
                targets.push(output);
            }

            if outputs.len() >= MAX_EXPLODE_OUTPUTS {
                break;
            }
        }
        if input_capacity != Capacity::zero() {
            outputs.push(CellOutput {
                capacity: input_capacity,
                data: Bytes::new(),
                lock: sender.lock_script().clone(),
                type_: None,
            });
        }

        let raw_transaction = TransactionBuilder::default()
            .input(input)
            .outputs(outputs)
            .dep(dep.clone())
            .build();
        let witness = {
            let hash = raw_transaction.hash();
            let message = H256::from(blake2b_256(hash));
            let signature_bytes = sender
                .privkey()
                .sign_recoverable(&message)
                .unwrap()
                .serialize();
            vec![Bytes::from(signature_bytes)]
        };
        let transaction = TransactionBuilder::from_transaction(raw_transaction)
            .witness(witness)
            .build();
        transactions.push(transaction);
    }
    assert_eq!(targets.len(), 0, "No enough balance");

    transactions
}

fn can_explode(cell: &LiveCell) -> bool {
    cell.cell_output.capacity.as_u64() >= MIN_SECP_CELL_CAPACITY
}
