use crate::config::Condition;
use crate::generator::{construct_inputs, sign_transaction, Generator};
use crate::traits::PackedCapacityAsCapacity;
use crate::types::{LiveCell, Personal, TaggedTransaction};
use bytes::Bytes;
use ckb_types::core::{Capacity, TransactionBuilder};
use ckb_types::packed::CellOutput;
use ckb_types::prelude::*;
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
                let output = CellOutput::new_builder()
                    .lock(receiver.lock_script().clone())
                    .build_exact_capacity(Capacity::zero())
                    .unwrap();
                let mut output2 = output.clone();
                let fee = input_capacities
                    .safe_sub(output.capacity().as_capacity())
                    .expect("input capacity is enough for 2 secp outputs")
                    .safe_sub(output2.capacity().as_capacity())
                    .expect("input capacity is enough for 2 secp outputs");
                let mut rng = thread_rng();
                if fee != Capacity::zero() {
                    let capacity2 = output2
                        .capacity()
                        .as_capacity()
                        .safe_add(Capacity::shannons(rng.gen_range(0, min(5, fee.as_u64()))))
                        .unwrap();
                    output2 = output2.as_builder().capacity(capacity2.pack()).build();
                }
                vec![output, output2]
            };
            let raw_transaction = TransactionBuilder::default()
                .inputs(inputs)
                .outputs(outputs)
                .outputs_data(vec![Bytes::new().pack(); 2])
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
