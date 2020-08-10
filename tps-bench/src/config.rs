use serde_derive::{Deserialize, Serialize};
use std::ops::Deref;

#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
pub enum TransactionType {
    In1Out1,
    In2Out2,
    In3Out3,
}

impl TransactionType {
    pub fn required(self) -> usize {
        match self {
            TransactionType::In1Out1 => 1,
            TransactionType::In2Out2 => 2,
            TransactionType::In3Out3 => 3,
        }
    }
}

impl Default for TransactionType {
    fn default() -> Self {
        TransactionType::In2Out2
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Url(#[serde(with = "url_serde")] pub url::Url);

impl Deref for Url {
    type Target = url::Url;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Config {
    pub logpath: String,
    pub bencher_private_key: String,
    pub miner_private_key: String,
    pub node_urls: Vec<Url>,
    pub block_time: u64, // in milliseconds
    pub transaction_type: TransactionType,
    pub start_miner: bool,
}

impl Config {
    pub fn load(filepath: &str) -> Result<Self, String> {
        let content = std::fs::read_to_string(filepath).map_err(|err| err.to_string())?;
        let config: Self = toml::from_str(&content).map_err(|err| err.to_string())?;

        // TODO create logfile if not exists
        // TODO return error if node rpc urls is empty

        Ok(config)
    }
}
