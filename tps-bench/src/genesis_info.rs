use crate::global::DEP_GROUP_TRANSACTION_INDEX;

use ckb_types::core;
use ckb_types::packed::Byte32;

#[derive(Debug, Clone)]
pub struct GenesisInfo {
    block: core::BlockView,
}

impl GenesisInfo {
    pub fn assert_initialized(&self) {
        assert!(!self.block.transactions().is_empty());
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
