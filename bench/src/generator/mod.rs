use crate::types::{LiveCell, Personal};
use ckb_core::block::Block;
use ckb_core::transaction::Transaction;
use crossbeam_channel::{Receiver, Sender};
use std::sync::Arc;

mod in2out2;
mod random_fee;

pub use in2out2::In2Out2;
pub use random_fee::RandomFee;

pub trait Generator {
    // TODO impl IntoIterator
    fn generate(
        &self,
        input_cells: Vec<LiveCell>,
        sender: &Personal,
        receiver: &Personal,
    ) -> (Vec<LiveCell>, Vec<Transaction>);

    fn serve(
        &self,
        alice: &Personal,
        mut unspent: Vec<LiveCell>,
        block_receiver: Receiver<Arc<Block>>,
        tx_sender: Sender<Transaction>,
    ) {
        while let Ok(block) = block_receiver.recv() {
            // Update live cell set based on new block
            let mut live_cells = alice.live_cells(&block);
            live_cells.extend(unspent.split_off(0));

            // Generate transactions based on live cell set
            let (rest_cells, transactions) = self.generate(live_cells, alice, alice);
            unspent = rest_cells;

            // Transfer the transactions into channel
            for transaction in transactions.into_iter() {
                tx_sender.send(transaction).expect("insert into tx_sender")
            }
        }
    }
}
