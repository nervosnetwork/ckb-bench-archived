#![macro_use]

use crate::SIGHASH_ALL_TYPE_HASH;
use ckb_crypto::secp::Privkey;
use ckb_hash::blake2b_256;
use ckb_types::core::ScriptHashType;
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

pub(crate) fn lock_hash(privkey: &Privkey) -> Byte32 {
    lock_script(privkey).calc_script_hash()
}

pub(crate) fn lock_script(privkey: &Privkey) -> Script {
    let pubkey = privkey.pubkey().unwrap();
    let address: H160 = H160::from_slice(&blake2b_256(pubkey.serialize())[0..20]).unwrap();
    Script::new_builder()
        .args(address.0.pack())
        .code_hash(SIGHASH_ALL_TYPE_HASH.pack())
        .hash_type(ScriptHashType::Type.into())
        .build()
}
