use crate::account::Account;
use crate::config::TransactionType;
use crate::global::MIN_SECP_CELL_CAPACITY;
use crate::net::Net;
use crate::rpc::Jsonrpc;
use crate::transfer::{construct_unsigned_transaction, sign_transaction};
use crate::util::estimate_fee;
use crate::utxo::UTXO;
use ckb_types::core::TransactionView;
use crossbeam_channel::{bounded, Receiver, Sender};
use log::info;
use serde_derive::{Deserialize, Serialize};
use serde_json::json;
use std::cmp::max;
use std::collections::VecDeque;
use std::thread::{sleep, spawn};
use std::time::Duration;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Metrics {
    tps: u64,
    average_block_time_ms: u64,
    average_block_transactions: u64,
    start_block_number: u64,
    end_block_number: u64,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BenchmarkConfig {
    transaction_type: TransactionType,
    send_delay: u64, // millis
}

impl BenchmarkConfig {
    fn start_net_monitor(&self, net: &Net) -> Receiver<Metrics> {
        let (notifier_sender, notifier_receiver) = bounded(0);
        let net = net.clone();

        info!("[START] wait net.is_network_txpool_empty() == true");
        while !net.is_network_txpool_empty() {
            sleep(Duration::from_secs(1));
        }
        info!("[END] net.is_network_txpool_empty() == true");

        spawn(move || {
            // Wait benchmark starting
            while net.is_network_txpool_empty() {
                sleep(Duration::from_secs(1));
            }

            let metrics = wait_network_stabled(&net);
            let _ = notifier_sender.send(metrics);
        });
        notifier_receiver
    }

    pub fn bench(
        &self,
        net: &Net,
        sender: &Account,
        recipient: &Account,
        sender_utxo_rx: &Receiver<UTXO>,
    ) {
        let net_notifier = self.start_net_monitor(net);

        let txemitters = net
            .endpoints()
            .iter()
            .map(|rpc| spawn_transaction_emitter(rpc.clone()))
            .collect::<Vec<_>>();

        let outputs_count = self.transaction_type.outputs_count() as u64;
        let min_input_total_capacity =
            outputs_count * MIN_SECP_CELL_CAPACITY + estimate_fee(outputs_count);
        let (mut inputs, mut input_total_capacity) = (Vec::new(), 0);
        let mut cursor = 0;
        let current_confirmed_tip = net.get_confirmed_tip_number();
        info!(
            "[BENCHMARK] {}",
            json!({
                "benchmark": self,
                "current_confirmed_tip_number": current_confirmed_tip
            })
        );

        while let Ok(utxo) = sender_utxo_rx.recv() {
            input_total_capacity += utxo.capacity();
            inputs.push(utxo);
            if input_total_capacity < min_input_total_capacity {
                continue;
            }

            // Construct transaction
            input_total_capacity = 0;
            let raw_transaction =
                construct_unsigned_transaction(&recipient, inputs.split_off(0), outputs_count);
            let signed_transaction = sign_transaction(sender, raw_transaction);

            // Send transaction
            loop {
                cursor = (cursor + 1) % txemitters.len();
                if txemitters[cursor]
                    .try_send(signed_transaction.clone())
                    .is_ok()
                {
                    break;
                }
            }

            // Sleep every time sending transaction.
            sleep(Duration::from_millis(self.send_delay));

            if let Ok(metrics) = net_notifier.try_recv() {
                info!(
                    "[BENCHMARK RESULT] {}",
                    json!({
                        "benchmark": self,
                        "metrics": metrics,
                    })
                );
                break;
            }
        }
    }
}

fn wait_network_stabled(net: &Net) -> Metrics {
    info!("[START] wait the network become stable");

    let window_size = 21;
    let window_margin = 10;

    let mut queue = VecDeque::with_capacity(window_size);
    queue.push_back(net.get_confirmed_tip_block());

    loop {
        loop {
            let tip_number = net.get_confirmed_tip_number();
            let back = queue.back().unwrap();
            if tip_number > back.number() {
                let next_block = net.get_block_by_number(back.number() + 1).unwrap().into();
                while queue.len() >= window_size {
                    queue.pop_front();
                }
                queue.push_back(next_block);
                break;
            } else {
                sleep(Duration::from_secs(1));
            }
        }

        if queue.len() >= window_size {
            let mintxns = queue.iter().map(|b| b.transactions().len()).min().unwrap();
            let maxtxns = queue.iter().map(|b| b.transactions().len()).max().unwrap();
            let totaltxns: usize = queue.iter().map(|block| block.transactions().len()).sum();
            let front = queue.front().unwrap();
            let back = queue.back().unwrap();
            let average_block_transactions = (totaltxns / queue.len()) as u64;
            let elapsed_ms = front.timestamp().saturating_sub(back.timestamp());
            let average_block_time_ms = max(1, elapsed_ms / (queue.len() as u64));
            let tps = (totaltxns as f64 * 1000.0 / elapsed_ms as f64) as u64;
            let metrics = Metrics {
                tps,
                average_block_time_ms,
                average_block_transactions,
                start_block_number: front.number(),
                end_block_number: back.number(),
            };

            info!("[metrics] {}", json!(metrics));

            if maxtxns <= mintxns + window_margin {
                return metrics;
            }
        }
    }
}

fn spawn_transaction_emitter(rpc: Jsonrpc) -> Sender<TransactionView> {
    let (sender, receiver) = bounded(1000);
    spawn(move || {
        while let Ok(transaction) = receiver.recv() {
            let transaction: TransactionView = transaction;
            rpc.send_transaction(transaction.data().into());
        }
    });
    sender
}
