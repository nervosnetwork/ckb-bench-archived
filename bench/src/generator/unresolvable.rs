use crate::config::Condition;
use crate::generator::{construct_inputs, sign_transaction, Generator};
use crate::traits::PackedCapacityAsCapacity;
use crate::types::{LiveCell, Personal, TaggedTransaction};
use ckb_types::core::{BlockView as Block, Capacity, TransactionBuilder};
use ckb_types::packed::CellOutput;
use ckb_types::prelude::*;
use crossbeam_channel::Sender;

pub struct Unresolvable;

impl Generator for Unresolvable {
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
                            .expect("input capacity is enough for 2 secp outputs")
                            .pack(),
                    )
                    .build();
                vec![output, output2]
            };

            // Construct a transaction with duplicated input-cells
            let raw_transaction = TransactionBuilder::default()
                .inputs(inputs)
                .outputs(outputs)
                .cell_dep(sender.cell_dep().clone())
                .build();
            let transaction = sign_transaction(raw_transaction, sender);
            transactions.push(transaction);
        }
        let condition = Condition::Unresolvable;
        let transactions = transactions
            .into_iter()
            .map(|transaction| TaggedTransaction {
                condition,
                transaction,
            })
            .collect();

        (live_cells, transactions)
    }

    fn serve(
        &self,
        alice: &Personal,
        unspent: Vec<LiveCell>,
        tx_sender: &Sender<TaggedTransaction>,
        block: &Block,
    ) -> Vec<LiveCell> {
        // Update live cell set based on new block
        let mut new_unspent_cells = alice.live_cells(block);
        new_unspent_cells.extend(unspent);

        // Generate transactions based on live cell set
        let (_, transactions) = self.generate(new_unspent_cells.clone(), alice, alice);

        // Transfer the transactions into channel
        for transaction in transactions.into_iter() {
            tx_sender.send(transaction).expect("insert into tx_sender")
        }

        new_unspent_cells
    }
}
