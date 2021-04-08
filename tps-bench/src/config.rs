use crate::global::{CELLBASE_MATURITY, CONFIRMATION_BLOCKS, METHOD_TO_EVAL_NET_STABLE};

use crate::benchmark::BenchmarkConfig;
use crate::miner::MinerConfig;
use crate::net_monitor::MethodToEvalNetStable;
use serde_derive::{Deserialize, Serialize};
use std::fs::create_dir_all;
use std::ops::Deref;
use std::path::PathBuf;

pub const STAGING_SPEC: &str = include_str!("../specs/staging.toml");
pub const DEV_SPEC: &str = include_str!("../specs/dev.toml");
pub const RELEASE_SPEC: &str = include_str!("../specs/release.toml");

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Config {
    spec: Spec,
    rpc_urls: Vec<Url>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Spec {
    pub data_dir: String,
    pub bencher_private_key: String,

    pub miner: MinerConfig,
    pub benchmarks: Vec<BenchmarkConfig>,

    pub consensus_cellbase_maturity: u64,
    pub confirmation_blocks: u64,
    pub ensure_matured_capacity_greater_than: u64,

    pub method_to_eval_network_stable: MethodToEvalNetStable,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
pub enum TransactionType {
    In1Out1,
    In2Out2,
    In3Out3,
}

#[derive(Deserialize, Serialize, Debug, Clone, Eq, Ord, PartialOrd, PartialEq)]
pub struct Url(#[serde(with = "url_serde")] pub url::Url);

impl Deref for Config {
    type Target = Spec;
    fn deref(&self) -> &Self::Target {
        &self.spec
    }
}

impl Config {
    pub fn new(spec: Spec, mut rpc_urls: Vec<Url>) -> Self {
        rpc_urls.sort();
        rpc_urls.dedup();
        Self { spec, rpc_urls }
    }

    pub fn rpc_urls(&self) -> Vec<&str> {
        self.rpc_urls.iter().map(|url| url.as_str()).collect()
    }

    pub fn spec(&self) -> &Spec {
        &self.spec
    }
}

impl Spec {
    pub fn load(filepath: &str) -> Result<Self, String> {
        let spec = match filepath {
            "staging" => toml::from_str(STAGING_SPEC).map_err(|err| err.to_string())?,
            "dev" => toml::from_str(DEV_SPEC).map_err(|err| err.to_string())?,
            "release" => toml::from_str(RELEASE_SPEC).map_err(|err| err.to_string())?,
            _ => {
                let content = std::fs::read_to_string(filepath).map_err(|err| err.to_string())?;
                let spec: Self = toml::from_str(&content).map_err(|err| err.to_string())?;
                spec
            }
        };

        create_dir_all(&spec.data_dir).unwrap();
        *CELLBASE_MATURITY.lock().unwrap() = spec.consensus_cellbase_maturity;
        *CONFIRMATION_BLOCKS.lock().unwrap() = spec.confirmation_blocks;
        *METHOD_TO_EVAL_NET_STABLE.lock().unwrap() = spec.method_to_eval_network_stable;

        Ok(spec)
    }

    pub fn log_path(&self) -> PathBuf {
        PathBuf::from(&self.data_dir).join("bench.log")
    }

    pub fn metrics_path(&self) -> PathBuf {
        PathBuf::from(&self.data_dir).join("metrics.json")
    }
}

impl TransactionType {
    pub fn outputs_count(self) -> usize {
        match self {
            TransactionType::In1Out1 => 1,
            TransactionType::In2Out2 => 2,
            TransactionType::In3Out3 => 3,
        }
    }
}

impl Deref for Url {
    type Target = url::Url;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Url {
    pub fn parse(input: &str) -> Result<Url, url::ParseError> {
        let url = url::Url::parse(input)?;
        Ok(Url(url))
    }
}
