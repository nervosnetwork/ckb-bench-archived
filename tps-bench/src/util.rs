#![macro_use]

use crate::config::Config;
use crate::SIGHASH_ALL_TYPE_HASH;
use ckb_crypto::secp::Privkey;
use ckb_hash::blake2b_256;
use ckb_types::core::{BlockNumber, ScriptHashType};
use ckb_types::packed::{Byte32, Script};
use ckb_types::prelude::*;
use ckb_types::H160;

#[macro_export]
macro_rules! prompt_and_exit {
    ($($arg:tt)*) => ({
        eprintln!($($arg)*);
        ::std::process::exit(1);
    })
}

/// TODO estimate fee MIN_FEE_RATE
pub(crate) fn estimate_fee(outputs_count: u64) -> u64 {
    1000 * outputs_count
}
