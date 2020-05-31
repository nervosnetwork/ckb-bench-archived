use crate::bencher::{Bencher, DefaultBencher};
use crate::config::Config;
use crate::generator::{Generator, In2Out2};
use crate::types::{LiveCell, Personal};
use ckb_types::core::{BlockNumber, BlockView};
use ckb_util::Mutex;
use crossbeam_channel::{unbounded, Receiver};
use failure::Error;
use rand::{thread_rng, Rng};
use std::sync::Arc;
use std::thread::spawn;

pub fn run(
    config: Config,
    alice: Personal,
    block_receiver: Receiver<BlockView>,
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
        let sum = conditions.iter().map(|(_, sg)| sg.end).max().unwrap();
        assert!(sum > 0);
        let mut rng = thread_rng();
        while let Ok(block) = block_receiver.recv() {
            let random = rng.gen_range(0, sum);
            let _condition = conditions
                .iter()
                .filter_map(|(c, sg)| if sg.contains(&random) { Some(c) } else { None })
                .collect::<Vec<_>>()[0];
            live_cells = In2Out2.serve(&alice, live_cells, &tx_sender, &block);
            // live_cells = match condition {
            //     Condition::In2Out2 => In2Out2.serve(&alice, live_cells, &tx_sender, &block),
            //     Condition::RandomFee => RandomFee.serve(&alice, live_cells, &tx_sender, &block),
            //     Condition::Unresolvable => {
            //         Unresolvable.serve(&alice, live_cells, &tx_sender, &block)
            //     }
            // };
        }
    });
    bencher.bench(tx_receiver);

    Ok(())
}
