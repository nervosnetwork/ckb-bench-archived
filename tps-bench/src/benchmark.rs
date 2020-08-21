use crate::account::Account;
use crate::config::TransactionType;
use crate::global::{METRICS_RECORDER, MIN_SECP_CELL_CAPACITY};
use crate::net::Net;
use crate::net_monitor::wait_network_stabled;
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
    transaction_type: TransactionType,
    send_delay: u64, // millis
}

impl BenchmarkConfig {
    pub fn bench(
        &self,
        net: &Net,
        sender: &Account,
        recipient: &Account,
        sender_utxo_rx: &Receiver<UTXO>,
    ) {
        crate::net_monitor::wait_network_txpool_empty(&net);

        let current_confirmed_tip = net.get_confirmed_tip_number();
        info!(
            "[BENCHMARK] {}",
            json!({
                "benchmark": self,
                "current_confirmed_tip_number": current_confirmed_tip
            })
        );

        let net_notifier = {
            let net = net.clone();
            let (net_sender, net_notifier) = bounded(1);
            spawn(move || {
                let metrics = wait_network_stabled(&net);
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
            sleep(Duration::from_millis(self.send_delay));

            if let Ok(metrics) = net_notifier.try_recv() {
                let result = json!({
                    "benchmark": self,
                    "metrics": metrics,
                });

                let recorder = METRICS_RECORDER.lock().unwrap();
                recorder.as_ref().map(|mut recorder| {
                    let _ = recorder.write(result.to_string().as_bytes());
                    let _ = recorder.write("\n".as_bytes());
                    let _ = recorder.flush();
                });

                info!("[BENCHMARK RESULT] {}", result,);
                break;
            }
        }
    }
}

fn spawn_transaction_emitter(rpc: Jsonrpc) -> Sender<TransactionView> {
    let (sender, receiver) = bounded(1000);
    spawn(move || {
        while let Ok(transaction) = receiver.recv() {
            let transaction: TransactionView = transaction;
            loop {
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
