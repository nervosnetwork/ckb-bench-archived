use ckb_types::packed::{Block, Transaction};
use log::{error, info};
use serde_derive::{Deserialize, Serialize};
use std::ops::Deref;
use std::time::Duration;

use crate::account::Account;
use crate::prompt_and_exit;
use crate::rpcs::Jsonrpcs;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct MinerConfig {
    private_key: String,
    block_time: u64,
}

#[derive(Clone)]
pub struct Miner {
    rpcs: Jsonrpcs,
    account: Account,
    pub block_time: Duration,
}

impl Miner {
    pub fn new(miner_config: &MinerConfig, rpc_urls: Vec<&str>) -> Self {
        let rpcs = match Jsonrpcs::connect_all(rpc_urls) {
            Ok(rpcs) => rpcs,
            Err(err) => prompt_and_exit!("Jsonrpcs::connect_all() error: {}", err),
        };
        let account = Account::new(&miner_config.private_key);
        let block_time = Duration::from_millis(miner_config.block_time);
        Self {
            rpcs,
            account,
            block_time,
        }
    }

    // TODO multiple miners
    pub fn generate_block(&self) {
        let template = self.rpcs.get_block_template(None, None, None);
        let work_id = template.work_id.value();
        let block_number = template.number.value();
        let block: Block = template.into();

        if let Some(block_hash) = self.rpcs.submit_block(work_id.to_string(), block.into()) {
            info!("submit block  #{} {:#x}", block_number, block_hash);
        } else {
            error!("submit block  #{} None", block_number);
        }
    }

    /// Run a miner to generate the given number of blocks.
    pub fn generate_blocks(&self, n: u64) {
        (0..n).for_each(|_| self.generate_block());
    }

    pub fn assert_block_assembler(&self) {
        // Ensure the miner is matcher with block_assembler configured in ckb
        let configured_miner_lock_script = self.lock_script();
        let block_assembler_lock_script = {
            let cellbase: Transaction = self
                .rpcs
                .get_block_template(None, None, None)
                .cellbase
                .data
                .into();
            cellbase.into_view().output(0).unwrap().lock()
        };
        assert_eq!(configured_miner_lock_script, block_assembler_lock_script);
    }
}

impl Deref for Miner {
    type Target = Account;

    fn deref(&self) -> &Self::Target {
        &&self.account
    }
}
