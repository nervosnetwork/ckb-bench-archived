use rpc_client::Jsonrpc;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub struct Metrics {
    total_txs_count: usize,
    jsonrpc: Jsonrpc,
    elapseds: VecDeque<(Instant, Duration)>,
}

impl Metrics {
    pub fn new(uri: &str, total_txs_count: usize) -> Self {
        let jsonrpc = Jsonrpc::connect(uri).expect("connect jsonrpc");
        Self {
            total_txs_count,
            jsonrpc,
            elapseds: VecDeque::new(),
        }
    }

    pub fn add_sample(&mut self, elapsed: Duration) {
        self.elapseds.push_back((Instant::now(), elapsed))
    }

    pub fn stat(&mut self, sleep_time: Duration, unsend: usize, misbehavior: usize) -> Duration {
        let (pending, proposal) = self.tx_pool_info();
        let committed = self
            .total_txs_count
            .saturating_sub(pending)
            .saturating_sub(proposal);
        ckb_logger::info!(
            "Ready: {}, Pending: {}, Proposals: {}, Committed: {}",
            unsend,
            pending,
            proposal,
            committed,
        );

        self.prune_staled();
        let tps = self.average_tps();
        let latency = self.average_elapsed();
        ckb_logger::info!(
            "TPS: {}, Misbehavior: {}, Latency: {:?}, Sleep {:?}",
            tps,
            misbehavior,
            latency,
            sleep_time,
        );

        latency
    }

    fn tx_pool_info(&self) -> (usize, usize) {
        let tx_pool_info = self.jsonrpc.tx_pool_info();
        (
            tx_pool_info.pending.0 as usize,
            tx_pool_info.proposed.0 as usize,
        )
    }

    fn duration(&self) -> Duration {
        Duration::from_secs(5)
    }

    fn prune_staled(&mut self) {
        let duration = self.duration();
        self.elapseds
            .retain(|(instant, _)| instant.elapsed() <= duration);
    }

    fn average_elapsed(&self) -> Duration {
        let elapseds = self
            .elapseds
            .iter()
            .fold(Duration::new(0, 0), |sum, (_, elapsed)| sum + *elapsed);
        elapseds / self.elapseds.len() as u32
    }

    fn average_tps(&self) -> f64 {
        self.elapseds.len() as f64 / self.duration().as_secs() as f64
    }
}
