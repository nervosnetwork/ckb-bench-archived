use crate::config::{Condition, Serial, Url};
use crate::metrics::Metrics;
use crate::types::TaggedTransaction;
use crate::utils::wait_until;
use ckb_logger::{debug, info};
use ckb_types::core::BlockNumber;
use ckb_util::Mutex;
use crossbeam_channel::{bounded, Receiver, Sender};
use failure::Error;
use lazy_static::lazy_static;
use rpc_client::Jsonrpc;
use std::cmp::{max, min};
use std::sync::Arc;
use std::thread::{sleep, spawn};
use std::time::{Duration, Instant};

lazy_static! {
    static ref LONG_SENDING_LATENCY: Duration = Duration::from_millis(200);
    static ref ESTIMATE_PERIOD: Duration = Duration::from_secs(5);
    static ref LONG_SENDING_PUNISH: usize = 5;
    static ref EMPTY_SENDING_PUNISH: usize = 30;
}

pub trait Bencher {
    fn adjust(&mut self, misbehavior: usize) -> Option<Duration>;

    fn stat(&mut self, sleep_time: Duration, unsend: usize, misbehavior: usize) -> Duration;

    fn add_sample(&mut self, elapsed: Duration);

    fn wait_until_ready(&self, receiver: &Receiver<TaggedTransaction>);

    fn send_transaction(&self, transaction: TaggedTransaction);

    fn bench(&mut self, receiver: Receiver<TaggedTransaction>) {
        self.wait_until_ready(&receiver);
        let mut sleep_time = self.adjust(::std::usize::MAX).unwrap();
        let mut misbehavior = 0;
        let mut ticker = Instant::now();
        loop {
            let transaction = {
                if let Ok(transaction) = receiver.try_recv() {
                    transaction
                } else {
                    misbehavior += *EMPTY_SENDING_PUNISH;
                    receiver.recv().unwrap()
                }
            };

            let elapsed = {
                sleep(sleep_time);
                let send_start = Instant::now();
                self.send_transaction(transaction);
                send_start.elapsed()
            };
            self.add_sample(elapsed);

            if ticker.elapsed() >= *ESTIMATE_PERIOD {
                ticker = Instant::now();

                let latency = self.stat(sleep_time, receiver.len(), misbehavior);
                if latency > *LONG_SENDING_LATENCY {
                    misbehavior += *LONG_SENDING_PUNISH;
                }

                if let Some(new_sleep_time) = self.adjust(misbehavior) {
                    info!(
                        "Adjust sleep time from: {:?} to {:?}",
                        sleep_time, new_sleep_time
                    );
                    self.wait_until_ready(&receiver);
                    sleep_time = new_sleep_time;
                    misbehavior = 0;
                }
            }
        }
    }
}

pub struct DefaultBencher {
    serial: Serial,
    last_adjust_number: BlockNumber,
    current_number: Arc<Mutex<BlockNumber>>,

    sleep_time: Duration,
    sleep_coefficient: i32,

    tx_forwarder: Sender<TaggedTransaction>,
    metrics: Metrics,
}

impl DefaultBencher {
    pub fn init(
        serial: Serial,
        rpc_urls: Vec<Url>,
        current_number: Arc<Mutex<BlockNumber>>,
    ) -> Result<Self, Error> {
        let (tx_forwarder, tx_receiver) = bounded::<TaggedTransaction>(0);
        rpc_urls
            .iter()
            .map(|url| Jsonrpc::connect(url.as_str()))
            .collect::<Result<Vec<_>, Error>>()?
            .into_iter()
            .for_each(|jsonrpc| {
                let tx_receiver = tx_receiver.clone();
                spawn(move || {
                    while let Ok(tagged_transaction) = tx_receiver.recv() {
                        let TaggedTransaction {
                            condition,
                            transaction,
                        } = tagged_transaction;
                        match condition {
                            Condition::Unresolvable => {
                                jsonrpc.broadcast_transaction(transaction.data().into())
                            }
                            _ => jsonrpc.send_transaction(transaction.data().into()),
                        };
                    }
                });
            });

        let last_adjust_number = { *current_number.lock() };
        let sleep_time = serial.adjust_origin;
        let metrics = Metrics::new(rpc_urls[0].as_str(), serial.transactions);
        Ok(Self {
            serial,
            last_adjust_number,
            current_number,
            sleep_time,
            sleep_coefficient: -1,
            tx_forwarder,
            metrics,
        })
    }

    pub fn adjust_by_direction(&mut self, direction: i32) {
        if direction > 0 {
            // increase sleep time
            if self.sleep_coefficient > 0 {
                self.sleep_coefficient = min(64, self.sleep_coefficient * 2);
            } else {
                self.sleep_coefficient = 1;
            }
            self.sleep_time += self.serial.adjust_step * self.sleep_coefficient as u32;
            if self.sleep_time > Duration::from_secs(1) {
                self.sleep_time = Duration::from_secs(1);
            }
        } else {
            // decrease sleep time
            if self.sleep_coefficient < 0 {
                self.sleep_coefficient = max(-64, self.sleep_coefficient * 2);
            } else {
                self.sleep_coefficient = -1;
            }
            if self.sleep_time >= self.serial.adjust_step * self.sleep_coefficient.abs() as u32 {
                self.sleep_time -= self.serial.adjust_step * self.sleep_coefficient.abs() as u32;
            } else {
                self.sleep_time = Duration::from_secs(0);
            }
        }
    }
}

impl Bencher for DefaultBencher {
    fn adjust(&mut self, misbehavior: usize) -> Option<Duration> {
        if misbehavior == ::std::usize::MAX {
            return Some(self.serial.adjust_origin);
        }

        let current_number = { *self.current_number.lock() };
        if misbehavior >= self.serial.adjust_misbehavior {
            self.last_adjust_number = current_number;
            self.adjust_by_direction(1);
            Some(self.sleep_time)
        } else if current_number - self.last_adjust_number >= self.serial.adjust_cycle as u64 {
            self.last_adjust_number = current_number;
            self.adjust_by_direction(-1);
            Some(self.sleep_time)
        } else {
            None
        }
    }

    fn stat(&mut self, sleep_time: Duration, unsend: usize, misbehavior: usize) -> Duration {
        self.metrics.stat(sleep_time, unsend, misbehavior)
    }

    fn add_sample(&mut self, elapsed: Duration) {
        self.metrics.add_sample(elapsed)
    }

    fn wait_until_ready(&self, receiver: &Receiver<TaggedTransaction>) {
        let standard = self.serial.transactions;
        let current_number = { *self.current_number.lock() };
        let ready = wait_until(Duration::new(60 * 30, 0), || {
            debug!(
                "wait_until_ready expect {}, actual: {}",
                standard,
                receiver.len()
            );
            receiver.len() >= standard && *self.current_number.lock() >= current_number + 2
        });
        assert!(
            ready,
            "timeout to wait, not enough transactions to bench, expect {}, actual: {}",
            standard,
            receiver.len(),
        );
    }

    fn send_transaction(&self, transaction: TaggedTransaction) {
        self.tx_forwarder
            .send(transaction)
            .expect("push transaction");
    }
}
