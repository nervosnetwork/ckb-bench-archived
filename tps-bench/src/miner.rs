use crate::account::Account;
use crate::config::Config;
use crate::prompt_and_exit;
use crate::rpc::Jsonrpc;
use ckb_crypto::secp::Privkey;
use ckb_types::core::TransactionView;
use ckb_types::packed::{self, Block, Byte32, Script};
use failure::_core::time::Duration;
use std::ops::Deref;
use std::str::FromStr;
use std::thread::{sleep, spawn};

#[derive(Clone)]
pub struct Miner {
    config: Config,
    rpc: Jsonrpc,
    account: Account,
}

impl Miner {
    pub fn new(config: Config, private_key: &str) -> Self {
        let url = &config.node_urls[0];
        let rpc = match Jsonrpc::connect(url.as_str()) {
            Ok(rpc) => rpc,
            Err(err) => prompt_and_exit!("Jsonrpc::connect({}) error: {}", url.as_str(), err),
        };
        let account = Account::new(private_key);

        // Ensure the miner is matcher with block_assembler configured in ckb
        let configured_miner_lock_script = account.lock_script();
        let block_assembler_lock_script = get_block_assembler_lock_script(&rpc);
        assert_eq!(configured_miner_lock_script, block_assembler_lock_script);

        Self {
            config,
            rpc,
            account,
        }
    }

    pub fn generate_block(&self) {
        let template = self.rpc.get_block_template(None, None, None);
        let work_id = template.work_id.value();
        let block_number = template.number.value();
        let block: Block = template.into();

        if let Some(block_hash) = self.rpc.submit_block(work_id.to_string(), block.into()) {
            println!("submit block  #{} {:#x}", block_number, block_hash);
        } else {
            eprintln!("submit block  #{} None", block_number);
        }
    }

    /// Run a miner to generate the given number of blocks.
    pub fn generate_blocks(&self, n: u64) {
        (0..n).for_each(|_| self.generate_block());
    }

    /// Run a miner to generate new blocks until the tx-pool be empty.
    pub fn generate_blocks_until_tx_pool_empty(&self) {
        let rpc = self.rpc.clone();

        println!("Miner.generate_blocks_until_tx_pool_empty");
        loop {
            let tx_pool_info = rpc.tx_pool_info();
            if tx_pool_info.pending.value() == 0 && tx_pool_info.proposed.value() == 0 {
                break;
            }
            self.generate_block();
            sleep(Duration::from_secs(1));
        }
    }

    /// Run a miner background to generate blocks forever, in the configured frequency.
    pub fn async_mine(&self) {
        let block_time = Duration::from_millis(self.config.block_time);

        println!("Miner.async_run");
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
