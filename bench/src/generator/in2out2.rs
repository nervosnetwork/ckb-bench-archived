use crate::config::Condition;
use crate::generator::{construct_inputs, sign_transaction, Generator};
use crate::traits::PackedCapacityAsCapacity;
use crate::types::{LiveCell, Personal, TaggedTransaction};
use bytes::Bytes;
use ckb_types::core::{Capacity, TransactionBuilder};
use ckb_types::packed::CellOutput;
use ckb_types::prelude::*;

pub struct In2Out2;

impl Generator for In2Out2 {
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
                let output2 = output
                    .clone()
                    .as_builder()
                    .capacity(
                        input_capacities
                            .safe_sub(output.capacity().as_capacity())
                            .expect("input capacity should be enough for 2-secp outputs")
                            .pack(),
                    )
                    .build();
                vec![output, output2]
            };
            let raw_transaction = TransactionBuilder::default()
                .inputs(inputs)
                .outputs(outputs)
                .outputs_data(vec![Bytes::new().pack(), Bytes::new().pack()])
                .cell_dep(sender.cell_dep().clone())
                .build();
            let transaction = sign_transaction(raw_transaction, sender);
            transactions.push(transaction);
        }
        let condition = Condition::In2Out2;
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
