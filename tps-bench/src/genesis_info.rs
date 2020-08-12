use crate::config::Config;
use crate::rpc::Jsonrpc;
use ckb_types::core;
use ckb_types::packed::Byte32;
use lazy_static::lazy_static;
use std::sync::Mutex;

const DEP_GROUP_TRANSACTION_INDEX: usize = 1;

lazy_static! {
    pub static ref GENESIS_INFO: Mutex<GenesisInfo> = Mutex::new(GenesisInfo::default());
}

#[derive(Debug, Clone)]
pub struct GenesisInfo {
    block: core::BlockView,
}

/// Initialize the global `GENESIS_INFO` with the genesis block
pub fn init_global_genesis_info(config: &Config) {
    let url = &config.rpc_urls()[0];
    let rpc = match Jsonrpc::connect(url.as_str()) {
        Ok(rpc) => rpc,
        Err(err) => prompt_and_exit!("Jsonrpc::connect({}) error: {}", url.as_str(), err),
    };
    let genesis_block: core::BlockView = match rpc.get_block_by_number(0) {
        Some(genesis_block) => genesis_block.into(),
        None => prompt_and_exit!(
            "Jsonrpc::get_block_by_number(0) from {} error: return None",
            url.as_str()
        ),
    };
    let genesis_info = GenesisInfo::from(genesis_block);

    *GENESIS_INFO.lock().unwrap() = genesis_info;
}

pub fn global_genesis_info() -> GenesisInfo {
    let genesis_info = GENESIS_INFO.lock().unwrap();
    genesis_info.assert_initialized();
    genesis_info.clone()
}

impl GenesisInfo {
    pub fn assert_initialized(&self) {
        assert!(self.block.transactions().len() > 0);
    }

    pub fn dep_group_tx_hash(&self) -> Byte32 {
        let dep_group_tx = self
            .block
            .transaction(DEP_GROUP_TRANSACTION_INDEX)
            .expect("genesis block should have transactions[DEP_GROUP_TRANSACTION_INDEX]");
        dep_group_tx.hash()
    }
}

impl From<core::BlockView> for GenesisInfo {
    fn from(block: core::BlockView) -> Self {
        assert_eq!(block.number(), 0);
        Self { block }
    }
}

impl Default for GenesisInfo {
    fn default() -> Self {
        Self {
            block: core::BlockBuilder::default().build(),
        }
    }
}
