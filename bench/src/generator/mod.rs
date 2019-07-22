use crate::types::{LiveCell, Personal};
use ckb_core::block::Block;
use ckb_core::transaction::Transaction;
use crossbeam_channel::{Sender};

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
        unspent: Vec<LiveCell>,
        tx_sender: &Sender<Transaction>,
        block: &Block,
    ) -> Vec<LiveCell> {
        // Update live cell set based on new block
        let mut live_cells = alice.live_cells(block);
        live_cells.extend(unspent);

        // Generate transactions based on live cell set
        let (rest_cells, transactions) = self.generate(live_cells, alice, alice);

        // Transfer the transactions into channel
        for transaction in transactions.into_iter() {
            tx_sender.send(transaction).expect("insert into tx_sender")
        }

        rest_cells
    }
}
