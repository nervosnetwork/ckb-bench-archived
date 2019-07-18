use crate::bencher::{Bencher, DefaultBencher};
use crate::config::{Condition, Config};
use crate::generator::{Generator, In2Out2 as In2Out2Generator, RandomFee as RandomFeeGenerator};
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
    let Config {
        serial, rpc_urls, ..
    } = config;

    let (tx_sender, tx_receiver) = unbounded();
    let mut bencher = DefaultBencher::init(serial.clone(), rpc_urls, tip)?;
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

    spawn(move || {
        match serial.condition {
            Condition::In2Out2 => {
                In2Out2Generator.serve(&alice, live_cells, block_receiver, tx_sender)
            }
            Condition::RandomFee => {
                RandomFeeGenerator.serve(&alice, live_cells, block_receiver, tx_sender)
            }
        };
    });
    bencher.bench(tx_receiver);

    Ok(())
}
