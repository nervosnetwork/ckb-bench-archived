use crate::bench::prepare;
use crate::client::Client;
use crate::config::{setup, Command, Config};
use crate::miner::DummyConfig;
use crate::notify::Notifier;
use crate::types::Personal;
use failure::Error;
use std::cmp::min;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;
use types::PROPOSAL_WINDOW;

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
    if let Command::Mine(target) = command {
        mine_by(&config, target);
        exit();
    }

    let mut notifier = expect_or_exit(Notifier::init(&config), "init notifier");
    let block_receiver = notifier.subscribe();
    let notifier_tip = Arc::clone(notifier.tip());
    let bank = expect_or_exit(
        Personal::init(
            config.bank.as_str(),
            config.basedir.to_str().unwrap(),
            &mut notifier,
        ),
        "build bank",
    );
    let alice = expect_or_exit(
        Personal::init(
            config.alice.as_str(),
            config.basedir.to_str().unwrap(),
            &mut notifier,
        ),
        "build alice",
    );

    // Start notifier to watch the CKB node
    {
        let start = min(alice.unspent().block_number, bank.unspent().block_number) + 1;
        notifier.spawn_watch(start);
    }

    ckb_logger::info!("\n\nStart preparing...");
    let wait_and_mine = || {
        let client = expect_or_exit(Client::init(&config), "init client");
        let target_tip = client.get_max_tip() + PROPOSAL_WINDOW;
        loop {
            mine_by(&config, PROPOSAL_WINDOW);
            sleep(Duration::from_secs(2));
            if bank.unspent().block_number < target_tip || alice.unspent().block_number < target_tip
            {
                continue;
            }
            let tx_pool_info = client.tx_pool_info();
            if (tx_pool_info.pending.0, tx_pool_info.proposed.0) == (0, 0) {
                break;
            }
        }
    };
    wait_and_mine();
    expect_or_exit(prepare(&config, &bank, &alice), "prepare");
    wait_and_mine();

    ckb_logger::info!("\n\nStart running...");
    miner::spawn_run(config.miner_configs.clone(), ::std::u64::MAX);
    expect_or_exit(
        run::run(config, alice, block_receiver, notifier_tip),
        "bench",
    );
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

fn mine_by(config: &Config, count: u64) {
    let bootstrap_miner = {
        let mut bootstrap_miner = config.miner_configs[0].clone();
        bootstrap_miner.dummy_config = DummyConfig::Constant { value: 10 };
        bootstrap_miner
    };
    miner::spawn_run(vec![bootstrap_miner], count)
        .into_iter()
        .for_each(|h| h.join().unwrap());
}
