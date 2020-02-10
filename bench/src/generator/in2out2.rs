use crate::config::Condition;
use crate::generator::{construct_inputs, sign_transaction, Generator};
use crate::types::{LiveCell, Personal, TaggedTransaction};
use ckb_types::{
    core::{Capacity, TransactionBuilder},
    packed::CellOutput,
    prelude::*,
};
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
                let builder1 = CellOutput::new_builder().lock(receiver.lock_script().clone());
                let builder2 = CellOutput::new_builder().lock(receiver.lock_script().clone());

                let output1 = builder1.build();
                let capacity1 = output1.occupied_capacity(Capacity::zero()).unwrap();
                let capacity2 = input_capacities
                    .safe_sub(capacity1)
                    .expect("input capacity is enough for 2 secp outputs");

                vec![
                    builder1.capacity(capacity1.pack()).build(),
                    builder2.capacity(capacity2.pack()).build(),
                ]
            };
            let raw_transaction = TransactionBuilder::default()
                .inputs(inputs)
                .outputs(outputs)
                .outputs_data(vec![Default::default(); 2])
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
