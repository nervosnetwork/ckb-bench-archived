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

    // TODO use constant variables
    let mut queue = FixedSizeQueue::new(20);
    queue.push(rpcs.get_fixed_tip_block());
    loop {
        loop {
            let tip_number = rpcs.get_fixed_tip_number();
            let front_number = queue.front().unwrap().number();
            if tip_number > front_number {
                queue.push(rpcs.get_block_by_number(front_number + 1).unwrap().into());
                break;
            }
        }

        if queue.len() >= 20 {
            let min_txns = queue.items().iter().fold(9999, |min, block| {
                if min <= block.transactions().len() {
                    min
                } else {
                    block.transactions().len()
                }
            });
            let max_txns = queue.items().iter().fold(0, |max, block| {
                if max >= block.transactions().len() {
                    max
                } else {
                    block.transactions().len()
                }
            });
            let total_txns: usize = queue
                .items()
                .iter()
                .map(|block| block.transactions().len())
                .sum();
            let front = queue.front().unwrap();
            let back = queue.back().unwrap();
            let average_block_transactions = (total_txns / queue.len()) as u64;
            let elapsed_ms = front.timestamp().saturating_sub(back.timestamp());
            let average_block_time_ms = elapsed_ms / (total_txns as u64);
            let tps = (total_txns as f64 * 1000.0 / elapsed_ms as f64) as u64;
            let metrics = Metrics {
                tps,
                average_block_time_ms,
                average_block_transactions,
                start_block_number: front.number(),
                end_block_number: back.number(),
            };

            info!("metrics: {}", json!(metrics).to_string());

            if max_txns > min_txns + 10 {
                return metrics;
            }
        }
    }
}

struct FixedSizeQueue<T> {
    inner: VecDeque<T>,
    size: usize,
}

impl<T> FixedSizeQueue<T> {
    fn new(size: usize) -> Self {
        Self {
            inner: VecDeque::with_capacity(size),
            size,
        }
    }

    fn push(&mut self, item: T) {
        while self.inner.len() >= self.size {
            self.inner.pop_front();
        }
        self.inner.push_back(item);
    }

    fn items(&self) -> &VecDeque<T> {
        &self.inner
    }

    fn len(&self) -> usize {
        self.inner.len()
    }

    fn front(&self) -> Option<&T> {
        self.inner.front()
    }

    fn back(&self) -> Option<&T> {
        self.inner.back()
    }
}
