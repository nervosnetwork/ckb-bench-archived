use crate::config::{Serial, Url};
use crate::utils::wait_until;
use ckb_core::transaction::Transaction;
use ckb_core::BlockNumber;
use ckb_logger::{debug, info};
use ckb_util::Mutex;
use crossbeam_channel::{bounded, Receiver, Sender};
use failure::Error;
use rpc_client::Jsonrpc;
use std::cmp::{max, min};
use std::collections::VecDeque;
use std::sync::Arc;
use std::thread::{sleep, spawn, JoinHandle};
use std::time::{Duration, Instant};

const DANGER_SEND_MS: u64 = 200; // ms

pub trait Bencher {
    fn adjust(&mut self, misbehavior: usize) -> Option<Duration>;

    fn wait_until_ready(&self, receiver: &Receiver<Transaction>);

    fn send_transaction(&self, transaction: Transaction);

    fn bench(&mut self, receiver: Receiver<Transaction>) {
        let mut counter = 0;
        let mut misbehavior = 0;
        let mut collector = Collector::default();
        let mut sleep_time = self.adjust(::std::usize::MAX).unwrap();
        self.wait_until_ready(&receiver);
        loop {
            let transaction = {
                if let Ok(transaction) = receiver.try_recv() {
                    transaction
                } else {
                    misbehavior += 30;
                    receiver.recv().unwrap()
                }
            };

            sleep(sleep_time);
            let send_start = Instant::now();
            self.send_transaction(transaction);
            let elapsed = send_start.elapsed();
            collector.add_one(elapsed);

            counter += 1;
            if counter == 100 {
                counter = 0;
                collector.stat(sleep_time, &mut misbehavior, receiver.len());
                if let Some(duration) = self.adjust(misbehavior) {
                    info!("Adjust! new sleep time: {:?}", duration);
                    self.wait_until_ready(&receiver);
                    misbehavior = 0;
                    sleep_time = duration;
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

    tx_forwarder: Sender<Transaction>,
    _handlers: Vec<JoinHandle<()>>,
}

impl DefaultBencher {
    pub fn init(
        serial: Serial,
        rpc_urls: Vec<Url>,
        current_number: Arc<Mutex<BlockNumber>>,
    ) -> Result<Self, Error> {
        let last_adjust_number = { *current_number.lock() };
        let sleep_time = serial.adjust_origin;
        let (tx_forwarder, tx_receiver) = bounded::<Transaction>(0);
        let jsonrpcs: Vec<Jsonrpc> = rpc_urls
            .iter()
            .map(|url| Jsonrpc::connect(url.as_str()))
            .collect::<Result<_, Error>>()?;
        let handlers: Vec<JoinHandle<()>> = jsonrpcs
            .into_iter()
            .map(|jsonrpc| {
                let tx_receiver = tx_receiver.clone();
                spawn(move || {
                    while let Ok(transaction) = tx_receiver.recv() {
                        jsonrpc.send_transaction((&transaction).into());
                    }
                })
            })
            .collect();

        Ok(Self {
            serial,
            last_adjust_number,
            current_number,
            sleep_time,
            sleep_coefficient: -1,
            tx_forwarder,
            _handlers: handlers,
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

    fn wait_until_ready(&self, receiver: &Receiver<Transaction>) {
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

    fn send_transaction(&self, transaction: Transaction) {
        self.tx_forwarder
            .send(transaction)
            .expect("push transaction");
    }
}

// FIXME Too dirty, remove it
#[derive(Default)]
pub struct Collector {
    inner: VecDeque<(Instant, Duration)>,
}

impl Collector {
    pub fn add_one(&mut self, elapsed: Duration) {
        self.inner.push_back((Instant::now(), elapsed))
    }

    pub fn stat(&mut self, sleep_time: Duration, misbehavior: &mut usize, chan_size: usize) {
        let duration = Duration::from_secs(5);
        self.inner
            .retain(|(instant, _)| instant.elapsed() <= duration);
        let last_tps = self.inner.len() as f64 / duration.as_secs() as f64;
        let elapseds = self
            .inner
            .iter()
            .fold(Duration::new(0, 0), |sum, (_, elapsed)| sum + *elapsed);
        let average_elapsed = elapseds / self.inner.len() as u32;

        if average_elapsed > Duration::from_millis(DANGER_SEND_MS) {
            *misbehavior += 5;
        }

        info!(
            "TPS: {}, Elapsed: {:?}, Sleep {:?}, Misbehavior: {}, ChanSize: {}",
            last_tps, average_elapsed, sleep_time, misbehavior, chan_size,
        );
    }
}
