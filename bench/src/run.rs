use crate::bencher::{Bencher, DefaultBencher};
use crate::config::{Condition, Config};
use crate::generator::{Generator, In2Out2 as In2Out2Generator};
use crate::types::{LiveCell, Personal};
use ckb_core::block::Block;
use ckb_core::BlockNumber;
use ckb_util::Mutex;
use crossbeam_channel::{unbounded, Receiver};
use failure::Error;
use std::sync::Arc;
use std::thread::spawn;

pub fn run(
    config: Config,
    alice: Personal,
    block_receiver: Receiver<Arc<Block>>,
    tip: Arc<Mutex<BlockNumber>>,
) -> Result<(), Error> {
    let (tx_sender, tx_receiver) = unbounded();
    match config.serial.condition {
        Condition::In2Out2 => {
            let generator = In2Out2Generator {};
            let mut bencher = DefaultBencher::init(config.serial, config.rpc_urls, tip)?;
            let (live_cells, start): (Vec<LiveCell>, BlockNumber) = {
                let unspent = alice.unspent();
                let live_cells = unspent.unsent.values().cloned().collect();
                let start = unspent.block_number;
                (live_cells, start)
            };

            while let Ok(block) = block_receiver.recv() {
                if block.header().number() >= start {
                    break;
                }
            }

            let _ = spawn(move || generator.serve(&alice, live_cells, block_receiver, tx_sender));
            bencher.bench(tx_receiver)
        }
    }
    Ok(())
}
