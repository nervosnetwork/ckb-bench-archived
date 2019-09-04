use crate::types::{LiveCell, Personal, TaggedTransaction};
use bytes::Bytes;
use ckb_hash::blake2b_256;
use ckb_types::core::{BlockView as Block, Capacity, TransactionView as Transaction};
use ckb_types::packed::{CellInput, Witness};
use ckb_types::prelude::*;
use ckb_types::H256;
use crossbeam_channel::Sender;

mod in2out2;
mod random_fee;
mod unresolvable;

use crate::traits::PackedCapacityAsCapacity;
pub use in2out2::In2Out2;
pub use random_fee::RandomFee;
pub use unresolvable::Unresolvable;

pub trait Generator {
    fn generate(
        &self,
        input_cells: Vec<LiveCell>,
        sender: &Personal,
        receiver: &Personal,
    ) -> (Vec<LiveCell>, Vec<TaggedTransaction>);

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
        let (rest_unspent_cells, transactions) = self.generate(new_unspent_cells, alice, alice);

        // Transfer the transactions into channel
        for transaction in transactions.into_iter() {
            tx_sender.send(transaction).expect("insert into tx_sender")
        }

        rest_unspent_cells
    }
}

pub fn construct_inputs(live_cells: Vec<LiveCell>) -> (Vec<CellInput>, Capacity) {
    let input_capacities = live_cells.iter().fold(Capacity::zero(), |sum, c| {
        sum.safe_add(c.cell_output.capacity().as_capacity())
            .expect("sum input capacities")
    });
    let inputs: Vec<_> = live_cells
        .into_iter()
        .map(|c| CellInput::new(c.out_point, 0))
        .collect();
    (inputs, input_capacities)
}

pub fn sign_transaction(raw_transaction: Transaction, sender: &Personal) -> Transaction {
    let witness = {
        let message = H256::from(blake2b_256(raw_transaction.hash().as_slice()));
        let signature_bytes: Bytes = sender
            .privkey()
            .sign_recoverable(&message)
            .unwrap()
            .serialize()
            .into();
        Witness::new_builder().push(signature_bytes.pack()).build()
    };
    let witnesses = vec![witness.clone(), witness];
    raw_transaction
        .as_advanced_builder()
        .witnesses(witnesses)
        .build()
}
