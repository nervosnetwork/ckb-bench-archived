use crate::global::CELLBASE_MATURITY;

use crate::benchmark::BenchmarkConfig;
use crate::miner::MinerConfig;
use serde_derive::{Deserialize, Serialize};
use std::fs::create_dir_all;
use std::ops::Deref;
use std::path::PathBuf;

pub const STAGING_SPEC: &str = include_str!("../specs/staging.toml");
pub const DEV_SPEC: &str = include_str!("../specs/dev.toml");

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Config {
    spec: Spec,
    rpc_urls: Vec<Url>,
    seconds: Option<u64>, // The last time of bench process
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Spec {
    pub data_dir: String,
    pub bencher_private_key: String,

    pub miner: Option<MinerConfig>,
    pub benchmarks: Vec<BenchmarkConfig>,

    #[serde(default)]
    pub metrics_url: Option<String>,
    pub consensus_cellbase_maturity: u64,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
pub enum TransactionType {
    In1Out1,
    In2Out2,
    In3Out3,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Url(#[serde(with = "url_serde")] pub url::Url);

impl Deref for Config {
    type Target = Spec;
    fn deref(&self) -> &Self::Target {
        &self.spec
    }
}

impl Config {
    pub fn new(spec: Spec, rpc_urls: Vec<Url>, seconds: Option<u64>) -> Self {
        Self {
            spec,
            rpc_urls,
            seconds,
        }
    }

    pub fn rpc_urls(&self) -> Vec<&str> {
        self.rpc_urls.iter().map(|url| url.as_str()).collect()
    }

    pub fn spec(&self) -> &Spec {
        &self.spec
    }

    pub fn seconds(&self) -> Option<u64> {
        self.seconds
    }
}

impl Spec {
    pub fn load(filepath: &str) -> Result<Self, String> {
        let spec = match filepath {
            "staging" => toml::from_str(STAGING_SPEC).map_err(|err| err.to_string())?,
            "dev" => toml::from_str(DEV_SPEC).map_err(|err| err.to_string())?,
            _ => {
                let content = std::fs::read_to_string(filepath).map_err(|err| err.to_string())?;
                let spec: Self = toml::from_str(&content).map_err(|err| err.to_string())?;
                spec
            }
        };

        create_dir_all(&spec.data_dir).unwrap();
        *CELLBASE_MATURITY.lock().unwrap() = spec.consensus_cellbase_maturity;

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
