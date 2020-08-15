use crossbeam_channel::{bounded, Receiver};
use log::info;
use std::thread::{sleep, spawn, JoinHandle};

use crate::account::Account;
use crate::config::Config;
use crate::miner::Miner;
use crate::rpcs::Jsonrpcs;
use crate::transfer::{construct_unsigned_transaction, sign_transaction};
use crate::utxo::UTXO;

pub fn spawn_pull_utxos(config: &Config, account: &Account) -> (JoinHandle<()>, Receiver<UTXO>) {
    info!("threads::spawn_pull_utxos");
    let rpcs = Jsonrpcs::connect_all(config.rpc_urls()).unwrap();
    let current_number = rpcs.get_fixed_tip_number();
    let (matureds, unmatureds) = account.pull_until(&rpcs, current_number);

    let (utxo_sender, utxo_receiver) = bounded(2000);
    let account = account.clone();
    let handler = spawn(move || {
        matureds.into_iter().for_each(|utxo| {
            utxo_sender.send(utxo).unwrap();
        });
        account.pull_forever(rpcs, current_number, unmatureds, utxo_sender);
    });

    (handler, utxo_receiver)
}

pub fn spawn_transfer_utxos(
    config: &Config,
    sender: &Account,
    recipient: &Account,
    utxo_receiver: Receiver<UTXO>,
) -> JoinHandle<()> {
    info!("threads::spawn_transfer_utxos");
    let rpcs = Jsonrpcs::connect_all(config.rpc_urls()).unwrap();
    let sender = sender.clone();
    let recipient = recipient.clone();
    spawn(move || {
        while let Ok(utxo) = utxo_receiver.recv() {
            let raw = construct_unsigned_transaction(&recipient, vec![utxo], 1);
            let signed = sign_transaction(&sender, raw);
            rpcs.send_transaction(signed.data().into());
        }
    })
}

pub fn spawn_miner(miner: &Miner) {
    info!("threads::spawn_miner");
    miner.assert_block_assembler();
    let miner = miner.clone();
    spawn(move || loop {
        sleep(miner.block_time);
        miner.generate_block();
    });
}
