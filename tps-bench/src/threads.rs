use ckb_types::packed::{CellOutput, OutPoint};
use ckb_types::prelude::*;
use crossbeam_channel::{bounded, Receiver};
use log::info;
use std::collections::HashMap;
use std::thread::{sleep, spawn, JoinHandle};

use crate::account::Account;
use crate::config::Config;
use crate::miner::Miner;
use crate::net::Net;
use crate::transfer::{construct_unsigned_transaction, sign_transaction};
use crate::utxo::UTXO;

// TODO move inside Account
pub fn spawn_pull_utxos(
    config: &Config,
    account: &Account,
    miner: &Miner,
) -> (JoinHandle<()>, Receiver<UTXO>) {
    let net = Net::connect_all(config.rpc_urls());
    let current_header = net.get_confirmed_tip_header();
    let (mut utxoset, mut unmatureds) = account.pull_until(&net, &current_header);

    let mut total_capacity = get_total_capacity_from_utxo(&utxoset);

    while total_capacity < config.ensure_matured_capacity_greater_than {
        if let Some(block_number) = miner.generate_block() {
            account.pull_from_block_number(&net, block_number, &mut utxoset, &mut unmatureds);
            total_capacity = get_total_capacity_from_utxo(&utxoset);
        }
    }

    let (matureds, unmatureds) = account.construct_utxo_vec(utxoset, unmatureds);
    let (utxo_sender, utxo_receiver) = bounded(2000);
    let account = account.clone();
    let handler = spawn(move || {
        matureds.into_iter().for_each(|utxo| {
            utxo_sender.send(utxo).unwrap();
        });
        account.pull_forever(net, current_header, unmatureds, utxo_sender);
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
    let net = Net::connect_all(config.rpc_urls());
    let sender = sender.clone();
    let recipient = recipient.clone();
    spawn(move || {
        while let Ok(utxo) = utxo_receiver.recv() {
            let raw = construct_unsigned_transaction(&recipient, vec![utxo], 1);
            let signed = sign_transaction(&sender, raw);
            net.send_transaction(signed.data().into());
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

fn get_total_capacity_from_utxo(utxoset: &HashMap<OutPoint, CellOutput>) -> u64 {
    utxoset
        .iter()
        .map(|(_, output)| -> u64 { output.capacity().unpack() })
        .sum()
}
