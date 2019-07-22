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
use rand::{thread_rng, Rng};

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
    let (mut live_cells, start): (Vec<LiveCell>, BlockNumber) = {
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
        let conditions = serial.conditions();
        let sum = conditions.iter().fold(0, |sum, (_, rate)| sum + *rate);
        assert!(sum > 0);
        let mut rng = thread_rng();
        while let Ok(block) = block_receiver.recv() {
            let random = rng.gen_range(0, sum);
            let condition = conditions.iter().filter_map(|(c, rate)| if *rate >= random {
                Some(c)
            } else { None }).collect::<Vec<_>>()[0].clone();
            match condition {
                Condition::In2Out2 => {
                    live_cells = In2Out2Generator.serve(&alice, live_cells, &tx_sender, &block);
                }
                Condition::RandomFee => {
                    live_cells = RandomFeeGenerator.serve(&alice, live_cells, &tx_sender, &block);
                }
            };
        }
    });
    bencher.bench(tx_receiver);

    Ok(())
}
