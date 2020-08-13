use ckb_types::packed::{Block, Transaction};
use log::{error, info};
use std::ops::Deref;
use std::thread::sleep;
use std::time::Duration;

use crate::account::Account;
use crate::config::Config;
use crate::prompt_and_exit;
use crate::rpcs::Jsonrpcs;

#[derive(Clone)]
pub struct Miner {
    rpcs: Jsonrpcs,
    account: Account,
    pub block_time: u64,
}

impl Miner {
    pub fn new(config: &Config, private_key: &str) -> Self {
        let rpcs = match Jsonrpcs::connect_all(config.rpc_urls()) {
            Ok(rpc) => rpc,
            Err(err) => prompt_and_exit!(
                "Jsonrpcs::connect_all({:?}) error: {}",
                config.rpc_urls(),
                err
            ),
        };
        let account = Account::new(private_key);
        Self {
            rpcs,
            account,
            block_time: config.block_time,
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

    /// Run a miner to generate new blocks until the tx-pool be empty.
    pub fn wait_txpool_empty(&self) {
        info!("miner wait txpool empty");
        for rpc in self.rpcs.endpoints() {
            loop {
                let tx_pool_info = rpc.tx_pool_info();
                if tx_pool_info.pending.value() == 0 && tx_pool_info.proposed.value() == 0 {
                    break;
                }
                sleep(Duration::from_secs(1));
            }
        }
        info!("miner txpool is empty now");
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
