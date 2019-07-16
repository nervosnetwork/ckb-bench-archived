use crate::conditions::explode::generate_transactions as explode;
use crate::config::Config;
use crate::types::{Personal, Secp};
use crate::utils::wait_until;
use ckb_jsonrpc_types::TxStatus;
use failure::{format_err, Error};
use numext_fixed_hash::H256;
use rpc_client::Jsonrpc;
use std::time::Duration;

pub fn prepare(config: &Config, bank: &Personal, alice: &Personal) -> Result<(), Error> {
    let mut hashes = Vec::new();
    let alice_ = alice.unspent().unsent.len();
    let need = config.serial.transactions * 2;
    let jsonrpc = Jsonrpc::connect(config.rpc_urls[0].as_str())?;
    let secp = Secp::load(&jsonrpc)?;
    let transactions = if need > alice_ {
        explode(bank, alice, secp, need - alice_)
    } else if need < alice_ {
        explode(alice, bank, secp, alice_ - need)
    } else {
        Vec::new()
    };

    for transaction in transactions.iter() {
        let hash = jsonrpc
            .send_transaction_result(transaction.into())
            .map_err(|err| format_err!("{:?}", err))?;
        hashes.push(hash);
    }

    let committed = TxStatus::committed(H256::zero()).status;
    for hash in hashes {
        assert!(
            wait_until(Duration::from_secs(10 * 60), || jsonrpc
                .get_transaction(hash.clone())
                .expect("sent transaction just now")
                .tx_status
                .status
                == committed),
            "timeout to wait. Not committed yet transaction: {:x}",
            hash,
        );
    }
    Ok(())
}
