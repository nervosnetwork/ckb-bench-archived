use crate::account::Account;
use crate::config::Config;
use crate::prompt_and_exit;
use crate::rpc::Jsonrpc;
use crate::rpcs::Jsonrpcs;
use ckb_types::packed::{self, Block, Script};
use failure::_core::time::Duration;
use log::{error, info};
use std::ops::Deref;
use std::thread::{sleep, spawn};

#[derive(Clone)]
pub struct Miner {
    config: Config,
    rpcs: Jsonrpcs,
    account: Account,
}

impl Miner {
    pub fn new(config: Config, private_key: &str) -> Self {
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
            config,
            rpcs,
            account,
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
    pub fn wait_txpool_empty(&self, start_miner: bool) {
        info!("START miner.wait_txpool_empty");
        for rpc in self.rpcs.endpoints() {
            loop {
                let tx_pool_info = rpc.tx_pool_info();
                if tx_pool_info.pending.value() == 0 && tx_pool_info.proposed.value() == 0 {
                    break;
                }
                if start_miner {
                    self.generate_block();
                }
                sleep(Duration::from_secs(1));
            }
        }
        info!("DONE miner.wait_txpool_empty");
    }

    /// Run a miner background to generate blocks forever, in the configured frequency.
    pub fn async_mine(&self) {
        // Ensure the miner is matcher with block_assembler configured in ckb
        let configured_miner_lock_script = self.account.lock_script();
        let block_assembler_lock_script = get_block_assembler_lock_script(&self.rpcs);
        assert_eq!(configured_miner_lock_script, block_assembler_lock_script);

        info!("miner.async_run");
        let block_time = Duration::from_millis(self.config.block_time);
        let miner = self.clone();
        spawn(move || loop {
            sleep(block_time);
            miner.generate_block();
        });
    }

    pub fn account(&self) -> &Account {
        &self.account
    }
}

impl Deref for Miner {
    type Target = Account;

    fn deref(&self) -> &Self::Target {
        &&self.account
    }
}

fn get_block_assembler_lock_script(rpc: &Jsonrpc) -> Script {
    let cellbase: packed::Transaction = rpc
        .get_block_template(None, None, None)
        .cellbase
        .data
        .into();
    cellbase.into_view().output(0).unwrap().lock()
}
