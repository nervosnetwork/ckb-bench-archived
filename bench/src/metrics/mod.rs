use failure::Error;
use numext_fixed_hash::H256;
use std::time::{Duration, SystemTime};

mod netdata;

pub use netdata::Netdata;

#[allow(dead_code)]
pub struct Metrics {
    pub start: SystemTime,
    pub end: SystemTime,
    pub hashes: Vec<(H256, Duration)>,
    pub errors: Vec<Error>,
}

#[allow(dead_code)]
impl Metrics {
    pub fn new() -> Self {
        Self {
            start: SystemTime::now(),
            end: SystemTime::now(),
            hashes: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn start(&mut self) {
        self.start = SystemTime::now();
    }

    pub fn end(&mut self) {
        self.end = SystemTime::now();
    }

    pub fn add_result(&mut self, result: Result<(H256, Duration), Error>) {
        match result {
            Ok((hash, elapsed)) => self.add_hash(hash, elapsed),
            Err(err) => self.add_error(err),
        }
    }

    pub fn add_hash(&mut self, hash: H256, elapsed: Duration) {
        self.hashes.push((hash, elapsed))
    }

    pub fn add_error(&mut self, error: Error) {
        self.errors.push(error)
    }

    pub fn elapsed(&self) -> Duration {
        // self.end.duration_since(self.start).unwrap()
        self.hashes.iter().map(|(_, elapsed)| *elapsed).sum()
    }

    pub fn tps(&self) -> u128 {
        if self.hashes.is_empty() {
            return 0;
        }

        let total = self.elapsed();
        let x = Duration::from_secs(1)
            .checked_mul(self.hashes.len() as u32)
            .unwrap()
            .as_millis();
        let y = total.as_millis();
        x / y
    }

    pub fn latency(&self) -> Duration {
        let total = self.elapsed();
        total
            .checked_div(self.hashes.len() as u32)
            .unwrap_or_else(|| Duration::new(0, 0))
    }
}
