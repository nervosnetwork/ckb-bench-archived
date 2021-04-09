use crate::account::Account;
use crate::config::TransactionType;
use crate::global::{METHOD_TO_EVAL_NET_STABLE, METRICS_RECORDER, MIN_SECP_CELL_CAPACITY};
use crate::net::Net;
use crate::net_monitor::{wait_network_stabled, MethodToEvalNetStable};
use crate::rpc::Jsonrpc;
use crate::transfer::{construct_unsigned_transaction, sign_transaction};
use crate::util::estimate_fee;
use crate::utxo::UTXO;
use ckb_types::core::TransactionView;
use crossbeam_channel::{bounded, Receiver, Sender};
use log::info;
use serde_derive::{Deserialize, Serialize};
use serde_json::json;
use std::io::Write;
use std::thread::{sleep, spawn};
use std::time::{Duration, Instant};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BenchmarkConfig {
    pub transaction_type: TransactionType,
    pub send_delay: u64, // micros
    pub method_to_eval_net_stable: Option<MethodToEvalNetStable>,
}

impl BenchmarkConfig {
    pub fn bench(
        &self,
        net: &Net,
        sender: &Account,
        recipient: &Account,
        sender_utxo_rx: &Receiver<UTXO>,
        send_delay: u64,
    ) -> u64 {
        crate::net_monitor::wait_network_txpool_empty(&net);

        let current_confirmed_tip = net.get_confirmed_tip_number();
        info!(
            "[BENCHMARK] {}",
            json!({
                "benchmark": {
                    "send_delay": send_delay,
                    "transaction_type": self.transaction_type,
                },
                "current_confirmed_tip_number": current_confirmed_tip
            })
        );

        let net_notifier = {
            let net = net.clone();
            let (net_sender, net_notifier) = bounded(1);
            let evaluation = self
                .method_to_eval_net_stable
                .unwrap_or_else(|| *METHOD_TO_EVAL_NET_STABLE.lock().unwrap());
            spawn(move || {
                let metrics = wait_network_stabled(&net, evaluation);
                let _ = net_sender.send(metrics);
            });
            net_notifier
        };

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
        let (mut sent, mut last_print_sent) = (0, Instant::now());

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

            sent += 1;
            if last_print_sent.elapsed() > Duration::from_secs(60) {
                last_print_sent = Instant::now();
                info!("benched {} transactions", sent);
            }

            // Sleep every time sending transaction.
            sleep(Duration::from_micros(send_delay));

            if let Ok(metrics) = net_notifier.try_recv() {
                let ckb_version = net.local_node_info().version;
                let time = chrono::Utc::now();
                let result = json!({
                    "time": time.to_string(),
                    "ckb_version": ckb_version,
                    "send_delay": send_delay,
                    "transaction_type": self.transaction_type,
                    "tps": metrics.tps,
                    "average_block_time_ms": metrics.average_block_time_ms,
                    "average_block_transactions": metrics.average_block_transactions,
                    "start_block_number": metrics.start_block_number,
                    "end_block_number": metrics.end_block_number,
                    "network_nodes": metrics.network_nodes,
                    "instances_count": metrics.network_nodes,
                    "bench_nodes": metrics.bench_nodes,
                    "total_transactions_size": metrics.total_transactions_size,
                });

                let recorder = METRICS_RECORDER.lock().unwrap();
                if let Some(mut recorder) = recorder.as_ref() {
                    let _ = recorder.write(result.to_string().as_bytes());
                    let _ = recorder.write(b"\n");
                    let _ = recorder.flush();
                };

                info!("[BENCHMARK RESULT] {}", result,);
                return metrics.tps;
            }
        }
        0
    }

    pub fn find_best_bench(
        &self,
        net: &Net,
        sender: &Account,
        recipient: &Account,
        sender_utxo_rx: &Receiver<UTXO>,
    ) -> u64 {
        let mut min_send_delay = self.send_delay;
        let mut min_send_delay_tps =
            self.bench(net, sender, recipient, sender_utxo_rx, self.send_delay);

        let mut max_send_delay = 1_000_000 / min_send_delay_tps;
        let mut max_send_delay_tps =
            self.bench(net, sender, recipient, sender_utxo_rx, max_send_delay);

        let mut nearly_send_delay_tps: Vec<u64> = Vec::new();

        while min_send_delay < max_send_delay - 1 {
            let mid_send_delay = (min_send_delay + max_send_delay) / 2;
            let mid_send_delay_tps =
                self.bench(net, sender, recipient, sender_utxo_rx, mid_send_delay);
            if max_send_delay - min_send_delay < 200 {
                nearly_send_delay_tps.push(mid_send_delay_tps);
            }
            if min_send_delay_tps < max_send_delay_tps {
                min_send_delay = mid_send_delay;
                min_send_delay_tps = mid_send_delay_tps;
            } else {
                max_send_delay = mid_send_delay;
                max_send_delay_tps = mid_send_delay_tps
            }
        }
        nearly_send_delay_tps.iter().sum::<u64>() / nearly_send_delay_tps.len() as u64
    }
}

fn spawn_transaction_emitter(rpc: Jsonrpc) -> Sender<TransactionView> {
    let (sender, receiver) = bounded(1000);
    spawn(move || {
        while let Ok(transaction) = receiver.recv() {
            let transaction: TransactionView = transaction;
            loop {
                // Chain reorg will cause many double-spent problem. Just ignore it. The chain
                // monitor will solve it.
                if let Err(err) = rpc.send_transaction_result(transaction.data().into()) {
                    let errs = err.to_string();
                    if errs.contains("PoolIsFull") || errs.contains("TransactionPoolFull") {
                        sleep(Duration::from_secs(1));
                        continue;
                    }
                }
                break;
            }
        }
    });
    sender
}
