use crate::genesis_info::GenesisInfo;

use ckb_types::core::DepType;
use ckb_types::packed::{Byte32, CellDep, OutPoint};
use ckb_types::prelude::*;
use ckb_types::{h256, H256};
use lazy_static::lazy_static;
use std::sync::Mutex;

pub const MIN_SECP_CELL_CAPACITY: u64 = 61_0000_0000;
pub const DEP_GROUP_TRANSACTION_INDEX: usize = 1;
pub const SIGHASH_ALL_DEP_GROUP_CELL_INDEX: usize = 0;
pub const SIGHASH_ALL_TYPE_HASH: H256 =
    h256!("0x9bd7e06f3ecf4be0f2fcd2188b23f1b9fcc88e5d4b65a8637b17723bbda3cce8");

lazy_static! {
    pub static ref SIGHASH_ALL_DEP_GROUP_TX_HASH: Byte32 = {
        let genesis_info = GENESIS_INFO.lock().unwrap();
        genesis_info.assert_initialized();
        genesis_info.dep_group_tx_hash()
    };
    pub static ref SIGHASH_ALL_CELL_DEP_OUT_POINT: OutPoint = OutPoint::new_builder()
        .tx_hash(SIGHASH_ALL_DEP_GROUP_TX_HASH.clone())
        .index(SIGHASH_ALL_DEP_GROUP_CELL_INDEX.pack())
        .build();
    pub static ref SIGHASH_ALL_CELL_DEP: CellDep = CellDep::new_builder()
        .out_point(SIGHASH_ALL_CELL_DEP_OUT_POINT.clone())
        .dep_type(DepType::DepGroup.into())
        .build();
}

lazy_static! {
    pub static ref GENESIS_INFO: Mutex<GenesisInfo> = Mutex::new(GenesisInfo::default());
    pub static ref CELLBASE_MATURITY: Mutex<u64> = Mutex::new(1);
    pub static ref CONFIRMATION_BLOCKS: Mutex<u64> = Mutex::new(0);
}
