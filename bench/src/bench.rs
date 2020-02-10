use crate::config::Config;
use crate::types::{LiveCell, Personal, Secp, MIN_SECP_CELL_CAPACITY};
use crate::generator::sign_transaction;
use ckb_hash::new_blake2b;
use ckb_types::{
    bytes::Bytes,
    core::{Capacity, DepType, TransactionBuilder, TransactionView},
    packed::{self, CellDep, CellInput, CellOutput, OutPoint, WitnessArgs},
    prelude::*,
    H256,
};
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
        // println!("{}", transaction);
        // for (output, data) in transaction.raw().outputs().into_iter().zip(transaction.raw().outputs_data().into_iter()) {
        //     println!("{}", output.occupied_capacity(Capacity::bytes(data.len()).unwrap()).unwrap());
        // }
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
) -> Vec<TransactionView> {
    let dep = CellDep::new_builder()
        .out_point(OutPoint::new(
            secp.out_point().tx_hash().clone(),
            secp.out_point().index().unpack(),
        ))
        .build();
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
            let tx = TransactionBuilder::default()
                .input(input)
                .output(output)
                .output_data(Default::default())
                .cell_dep(dep.clone())
                .build();

            sign_transaction(tx, sender)
        })
        .collect()
}

fn issue(
    sender: &Personal,
    receiver: &Personal,
    secp: Secp,
    outputs_count: usize,
) -> Vec<TransactionView> {
    let mut targets: Vec<CellOutput> = {
        (0..outputs_count)
            .map(|_| {
                let output = CellOutput::new_builder()
                    .lock(receiver.lock_script().clone())
                    .build();
                let capacity = output
                    .occupied_capacity(Capacity::zero())
                    .unwrap()
                    .safe_mul(2 as u64)
                    .unwrap()
                    .safe_sub(1 as u64)
                    .unwrap();
                CellOutput::new_builder()
                    .lock(receiver.lock_script().clone())
                    .capacity(capacity.pack())
                    .build()
            })
            .collect()
    };
    let secp_out_point = OutPoint::new(secp.dep_group_tx_hash().clone(), 0);
    let dep = CellDep::new_builder()
        .out_point(secp_out_point)
        .dep_type(DepType::DepGroup.into())
        .build();
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
            let output_capacity: Capacity = output.capacity().unpack();
            if input_capacity.as_u64() >= output_capacity.as_u64() * 2 {
                input_capacity = input_capacity.safe_sub(output_capacity).unwrap();
                outputs.push(output);
            } else if input_capacity.as_u64() >= output_capacity.as_u64() {
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
            outputs.push(
                CellOutput::new_builder()
                    .capacity(input_capacity.pack())
                    .lock(sender.lock_script().clone())
                    .build(),
            );
        }

        let tx = TransactionBuilder::default()
            .input(input)
            .outputs_data((0..outputs.len()).map(|_| Default::default()))
            .outputs(outputs)
            .cell_dep(dep.clone())
            .build();

        transactions.push(sign_transaction(tx, sender));
    }
    assert_eq!(targets.len(), 0, "No enough balance");

    transactions
}

fn can_explode(cell: &LiveCell) -> bool {
    let output_capacity: Capacity = cell.cell_output.capacity().unpack();
    output_capacity.as_u64() >= MIN_SECP_CELL_CAPACITY
}
