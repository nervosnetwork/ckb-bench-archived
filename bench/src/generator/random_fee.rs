use crate::config::Condition;
use crate::generator::{construct_inputs, sign_transaction, Generator};
use crate::types::{LiveCell, Personal, TaggedTransaction};
use ckb_core::transaction::{CellOutput, TransactionBuilder};
use ckb_core::Bytes;
use ckb_occupied_capacity::Capacity;
use numext_fixed_hash::H256;
use rand::{thread_rng, Rng};
use std::cmp::min;

pub struct RandomFee;

impl Generator for RandomFee {
    fn generate(
        &self,
        mut live_cells: Vec<LiveCell>,
        sender: &Personal,
        receiver: &Personal,
    ) -> (Vec<LiveCell>, Vec<TaggedTransaction>) {
        let mut transactions = Vec::new();
        while live_cells.len() >= 2 {
            let input_cells: Vec<_> = (0..2).map(|_| live_cells.pop().unwrap()).collect();
            let (inputs, input_capacities) = construct_inputs(input_cells);
            let outputs = {
                let mut output = CellOutput::new(
                    Capacity::zero(),
                    H256::zero(),
                    receiver.lock_script().clone(),
                    None,
                );
                output.capacity = output.occupied_capacity(Capacity::zero()).unwrap();
                let mut output2 = output.clone();
                let fee = input_capacities
                    .safe_sub(output.capacity)
                    .expect("input capacity is enough for 2 secp outputs")
                    .safe_sub(output2.capacity)
                    .expect("input capacity is enough for 2 secp outputs");
                let mut rng = thread_rng();
                if fee != Capacity::zero() {
                    output2.capacity = output2
                        .capacity
                        .safe_add(Capacity::shannons(rng.gen_range(0, min(5, fee.as_u64()))))
                        .unwrap();
                }
                vec![output, output2]
            };
            let raw_transaction = TransactionBuilder::default()
                .inputs(inputs)
                .outputs(outputs)
                .outputs_data(vec![Bytes::new(), Bytes::new()])
                .cell_dep(sender.cell_dep().clone())
                .build();
            let transaction = sign_transaction(raw_transaction, sender);
            transactions.push(transaction);
        }
        let condition = Condition::RandomFee;
        let transactions = transactions
            .into_iter()
            .map(|transaction| TaggedTransaction {
                condition,
                transaction,
            })
            .collect();

        (live_cells, transactions)
    }
}
