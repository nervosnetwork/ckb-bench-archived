use crate::config::Condition;
use crate::generator::{construct_inputs, sign_transaction, Generator};
use crate::types::{LiveCell, Personal, TaggedTransaction};
use ckb_core::transaction::{CellOutput, TransactionBuilder};
use ckb_core::Bytes;
use ckb_occupied_capacity::Capacity;

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
                let mut output = CellOutput::new(
                    Capacity::zero(),
                    Bytes::new(),
                    receiver.lock_script().clone(),
                    None,
                );
                let mut output2 = output.clone();
                output.capacity = output.occupied_capacity().unwrap();
                output2.capacity = input_capacities
                    .safe_sub(output.capacity)
                    .expect("input capacity is enough for 2 secp outputs");
                vec![output, output2]
            };
            let raw_transaction = TransactionBuilder::default()
                .inputs(inputs)
                .outputs(outputs)
                .dep(sender.dep_out_point().clone())
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
