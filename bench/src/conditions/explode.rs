use crate::types::{LiveCell, Personal, Secp, MIN_SECP_CELL_CAPACITY};
use ckb_core::transaction::{CellInput, CellOutput, OutPoint, Transaction, TransactionBuilder};
use ckb_core::{Bytes, Capacity};
use hash::blake2b_256;
use numext_fixed_hash::H256;
use std::vec::Vec;
// use occupied_capacity::OccupiedCapacity;

pub const MAX_EXPLODE_OUTPUTS: usize = 5000;

pub fn generate_transactions(
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
                output.capacity = output.occupied_capacity().unwrap();
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
                unreachable!();
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
