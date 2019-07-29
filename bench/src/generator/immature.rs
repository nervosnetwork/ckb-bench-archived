use crate::generator::{construct_inputs, sign_transaction, Generator};
use crate::types::{LiveCell, Personal};
use ckb_core::block::Block;
use ckb_core::transaction::{CellOutput, Transaction, TransactionBuilder};
use ckb_core::Bytes;
use ckb_occupied_capacity::Capacity;
use crossbeam_channel::Sender;

pub struct Immature;

impl Generator for Immature {
    fn generate(
        &self,
        mut live_cells: Vec<LiveCell>,
        sender: &Personal,
        receiver: &Personal,
    ) -> (Vec<LiveCell>, Vec<Transaction>) {
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

        (live_cells, transactions)
    }

    fn serve(
        &self,
        alice: &Personal,
        unspent: Vec<LiveCell>,
        tx_sender: &Sender<Transaction>,
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
