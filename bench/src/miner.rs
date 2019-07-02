use crate::config;
use ckb_core::block::BlockBuilder;
use rand::{
    distributions::{self as dist, Distribution as _},
    thread_rng,
};
use rpc_client::Jsonrpc;
use serde_derive::{Deserialize, Serialize};
use std::thread::{sleep, spawn, JoinHandle};
use std::time::Duration;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MinerConfig {
    pub rpc_url: config::Url,
    #[serde(rename = "delay_type")]
    pub dummy_config: DummyConfig,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DummyConfig {
    Constant { value: u64 },
    Uniform { low: u64, high: u64 },
    Normal { mean: f64, std_dev: f64 },
    Poisson { lambda: f64 },
}

pub enum Delay {
    Constant(u64),
    Uniform(dist::Uniform<u64>),
    Normal(dist::Normal),
    Poisson(dist::Poisson),
}

impl From<DummyConfig> for Delay {
    fn from(config: DummyConfig) -> Self {
        match config {
            DummyConfig::Constant { value } => Delay::Constant(value),
            DummyConfig::Uniform { low, high } => Delay::Uniform(dist::Uniform::new(low, high)),
            DummyConfig::Normal { mean, std_dev } => {
                Delay::Normal(dist::Normal::new(mean, std_dev))
            }
            DummyConfig::Poisson { lambda } => Delay::Poisson(dist::Poisson::new(lambda)),
        }
    }
}

impl Default for Delay {
    fn default() -> Self {
        Delay::Constant(5000)
    }
}

impl Delay {
    fn duration(&self) -> Duration {
        let mut rng = thread_rng();
        let millis = match self {
            Delay::Constant(v) => *v,
            Delay::Uniform(ref d) => d.sample(&mut rng),
            Delay::Normal(ref d) => d.sample(&mut rng) as u64,
            Delay::Poisson(ref d) => d.sample(&mut rng),
        };
        Duration::from_millis(millis)
    }
}

pub fn spawn_run(miner_configs: Vec<MinerConfig>, target: u64) -> Vec<JoinHandle<()>> {
    miner_configs
        .into_iter()
        .map(|miner_config| {
            let MinerConfig {
                rpc_url,
                dummy_config,
            } = miner_config;
            let jsonrpc = Jsonrpc::connect(rpc_url.as_str()).expect("init miner client");
            let delay: Delay = dummy_config.into();
            spawn(move || {
                let mut count = 0;
                loop {
                    sleep(delay.duration());
                    solve(&jsonrpc);
                    count += 1;
                    if target <= count {
                        break;
                    }
                }
            })
        })
        .collect()
}

fn solve(jsonrpc: &Jsonrpc) {
    let template = jsonrpc.get_block_template(None, None, None);
    let work_id = template.work_id.0;
    let block_number = template.number.0;
    let block_builder: BlockBuilder = template.into();
    let block = block_builder.build();
    if let Some(block_hash) = jsonrpc.submit_block(work_id.to_string(), (&block).into()) {
        ckb_logger::debug!("submit block #{} {:#x}", block_number, block_hash);
    } else {
        ckb_logger::debug!("submit block #{} None", block_number);
    }
}
