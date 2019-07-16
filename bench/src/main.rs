use crate::bench::prepare;
use crate::config::{setup, Command};
use crate::miner::DummyConfig;
use crate::notify::Notifier;
use crate::types::Personal;
use ckb_core::BlockNumber;
use ckb_logger::{debug, error};
use failure::Error;
use std::cmp::min;
use std::sync::Arc;
use std::time::Duration;
use types::PROPOSAL_WINDOW;
use utils::wait_until;

mod bench;
mod bencher;
mod client;
mod conditions;
mod config;
mod generator;
mod metrics;
mod miner;
mod notify;
mod run;
mod types;
mod utils;

fn main() {
    let (command, config) = expect_or_exit(setup(), "load config");
    let _logger_guard = ckb_logger::init(config.logger.clone()).expect("init logger");

    let _miner = match command {
        Command::Mine(target) => {
            miner::spawn_run(config.miner_configs.clone(), target)
                .into_iter()
                .for_each(|h| h.join().unwrap());
            exit();
            unreachable!()
        }
        _ => {
            let mut bootstrap_miner = config.miner_configs[0].clone();
            bootstrap_miner.dummy_config = DummyConfig::Constant { value: 100 };
            let _ = miner::spawn_run(vec![bootstrap_miner], PROPOSAL_WINDOW * 10)
                .into_iter()
                .for_each(|h| h.join().unwrap());
            miner::spawn_run(config.miner_configs.clone(), ::std::u64::MAX)
        }
    };

    let mut notifier = Notifier::init(&config).expect("init notifier");
    let tip = Arc::clone(notifier.tip());
    let block_receiver = notifier.subscribe();

    let bank = expect_or_exit(
        Personal::init(
            config.bank.as_str(),
            config.basedir.to_str().unwrap(),
            &mut notifier,
        ),
        "build bank personal",
    );
    let alice = expect_or_exit(
        Personal::init(
            config.alice.as_str(),
            config.basedir.to_str().unwrap(),
            &mut notifier,
        ),
        "build alice personal",
    );

    // Start notifier to watch the CKB node, and wait for synchronizing to the tip
    let _notifier_handler = {
        let start = min(alice.unspent().block_number, bank.unspent().block_number) + 1;
        let handle = notifier.spawn_watch(start);
        let current_tip = { *tip.lock() };
        let gap_number = current_tip + 1 - start;
        assert!(wait_until(
            Duration::from_millis(200) * gap_number as u32 + Duration::from_secs(10 * 60),
            || is_unspent_synced(&bank, current_tip + PROPOSAL_WINDOW)
        ));
        assert!(wait_until(
            Duration::from_millis(200) * gap_number as u32 + Duration::from_secs(10 * 60),
            || is_unspent_synced(&alice, current_tip + PROPOSAL_WINDOW)
        ));
        handle
    };

    match prepare(&config, &bank, &alice) {
        Ok(()) => {
            let current_tip = { *tip.lock() } + 1;
            let gap_number = current_tip - alice.unspent().block_number;
            assert!(wait_until(
                Duration::from_millis(200) * gap_number as u32 + Duration::from_secs(10 * 60),
                || is_unspent_synced(&alice, current_tip)
            ));
        }
        Err(err) => {
            error!("prepare error: {:?}", err);
            exit();
        }
    }

    debug!("running...");
    if let Err(err) = run::run(config, alice, block_receiver, tip) {
        error!("bench error: {:?}", err);
        exit();
    }
}

fn is_unspent_synced(personal: &Personal, target_tip: BlockNumber) -> bool {
    debug!(
        "syncing... current_number: {}, old_tip: {}",
        personal.unspent().block_number,
        target_tip,
    );
    personal.unspent().block_number >= target_tip
}

fn exit() {
    ckb_logger::flush();
    std::process::exit(1);
}

fn expect_or_exit<T>(r: Result<T, Error>, message: &str) -> T {
    match r {
        Err(err) => {
            eprintln!("{} error: {:?}", message, err);
            exit();
            unreachable!()
        }
        Ok(t) => t,
    }
}
