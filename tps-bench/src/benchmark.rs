use crate::account::Account;
use crate::config::TransactionType;
use crate::global::MIN_SECP_CELL_CAPACITY;
use crate::rpcs::Jsonrpcs;
use crate::transfer::{construct_unsigned_transaction, sign_transaction};
use crate::util::estimate_fee;
use crate::utxo::UTXO;
use crossbeam_channel::{bounded, select, Receiver};
use log::info;
use serde_derive::{Deserialize, Serialize};
use serde_json::json;
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
pub struct Benchmark {
    transaction_type: TransactionType,
    send_delay: Duration,
}

impl Benchmark {
    pub fn new(transaction_type: TransactionType, send_delay: Duration) -> Self {
        Self {
            transaction_type,
            send_delay,
        }
    }

    pub fn bench(
        &self,
        rpcs: &Jsonrpcs,
        sender: &Account,
        recipient: &Account,
        sender_utxo_rx: &Receiver<UTXO>,
    ) {
        let stabled_notifier = {
            let (notifier_sender, notifier_receiver) = bounded(0);
            let rpcs_ = rpcs.clone();
            wait_txpool_empty(&rpcs_);
            spawn(move || {
                wait_txpool_not_empty(&rpcs_);
                let metrics = wait_chain_stabled(&rpcs_);
                let _ = notifier_sender.send(metrics);
            });
            notifier_receiver
        };

        // info!(
        //     "Start benchmark: {}",
        //     json!(
        //         "transaction_type": format!("{:?}", self.transaction_type),
        //         "send_delay": self.send_delay.as_millis(),
        //     )
        // );
        let rpcs = rpcs.endpoints();
        let outputs_count = self.transaction_type.outputs_count() as u64;
        let min_input_total_capacity =
            outputs_count * MIN_SECP_CELL_CAPACITY + estimate_fee(outputs_count);
        let (mut inputs, mut input_total_capacity) = (Vec::new(), 0);
        let mut cursor = 0;
        let metrics: Metrics = loop {
            select! {
                recv(sender_utxo_rx) -> msg => {
                    match msg {
                        Ok(utxo) => {
                            input_total_capacity += utxo.capacity();
                            inputs.push(utxo);
                            if input_total_capacity < min_input_total_capacity {
                                continue;
                            }

                            input_total_capacity = 0;
                            let raw_transaction =
                                construct_unsigned_transaction(&recipient, inputs.split_off(0), outputs_count);
                            let signed_transaction = sign_transaction(sender, raw_transaction);

                            // TODO async Send transaction to random nodes
                            cursor = (cursor + 1) % rpcs.len();
                            rpcs[cursor].send_transaction( signed_transaction.data().into());

                            sleep(self.send_delay);
                        }
                        Err(err) => panic!(err),
                    }
                }
                recv(stabled_notifier) -> msg => {
                    match msg {
                        Ok(metrics) => break metrics,
                        Err(err) => panic!(err),
                    }
                }
            }
        };
        // // TODO 完善日志输出
        info!("Done benchmark: {}", json!(metrics));
    }
}

fn wait_txpool_empty(rpcs: &Jsonrpcs) {
    info!("START wait_txpool_empty");
    for rpc in rpcs.endpoints() {
        loop {
            let tx_pool_info = rpc.tx_pool_info();
            if tx_pool_info.pending.value() == 0 && tx_pool_info.proposed.value() == 0 {
                break;
            }
            sleep(Duration::from_secs(1));
        }
    }
    info!("DONE wait_txpool_empty");
}

fn wait_txpool_not_empty(rpcs: &Jsonrpcs) {
    info!("START wait_txpool_not_empty");
    for rpc in rpcs.endpoints() {
        loop {
            let tx_pool_info = rpc.tx_pool_info();
            if tx_pool_info.pending.value() != 0 || tx_pool_info.proposed.value() != 0 {
                break;
            }
            sleep(Duration::from_secs(1));
        }
    }
    info!("DONE wait_txpool_not_empty");
}

fn wait_chain_stabled(rpcs: &Jsonrpcs) -> Metrics {
    info!("START wait_chain_stabled");

    let window_size = 20;
    let window_margin = 10;

    let mut queue = VecDeque::with_capacity(window_size);
    queue.push_back(rpcs.get_fixed_tip_block());
    
    loop {
        loop {
            let tip_number = rpcs.get_fixed_tip_number();
            let back = queue.front().unwrap();
            if tip_number > back.number() {
                let next_block = rpcs.get_block_by_number(back.number() + 1).unwrap().into();
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
            let average_block_time_ms = elapsed_ms / (totaltxns as u64);
            let tps = (totaltxns as f64 * 1000.0 / elapsed_ms as f64) as u64;
            let metrics = Metrics {
                tps,
                average_block_time_ms,
                average_block_transactions,
                start_block_number: front.number(),
                end_block_number: back.number(),
            };

            info!("metrics: {}", json!(metrics).to_string());

            if maxtxns <= mintxns + window_margin {
                return metrics;
            }
        }
    }
}
