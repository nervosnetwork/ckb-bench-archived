use crate::account::Account;
use crate::global::SIGHASH_ALL_CELL_DEP;
use crate::util::estimate_fee;
use crate::utxo::UTXO;

use ckb_types::core;
use ckb_types::packed::{CellInput, CellOutput, WitnessArgs};
use ckb_types::prelude::*;
use ckb_types::{bytes::Bytes, H256};

/// Transfer all the `utxos` to `recipient` in `outputs_count` outputs.
/// The returned transaction is unsigned, it should be signed by sender before
/// sending to CKB.
pub fn construct_unsigned_transaction(
    recipient: &Account,
    utxos: Vec<UTXO>, // inputs
    outputs_count: u64,
) -> core::TransactionView {
    let fee = estimate_fee(outputs_count);
    let input_total_capacity = utxos.iter().map(|input| input.capacity()).sum::<u64>();
    let output_total_capacity = input_total_capacity - fee;
    let inputs = utxos
        .iter()
        .map(|utxo| {
            CellInput::new_builder()
                .previous_output(utxo.out_point().clone())
                .build()
        })
        .collect::<Vec<_>>();
    let outputs = (0..outputs_count)
        .map(|i| {
            let capacity = if (i as u64) < output_total_capacity % outputs_count {
                output_total_capacity / outputs_count + 1
            } else {
                output_total_capacity / outputs_count
            };
            CellOutput::new_builder()
                .lock(recipient.lock_script())
                .capacity(capacity.pack())
                .build()
        })
        .collect::<Vec<_>>();
    let outputs_data = (0..outputs_count)
        .map(|_| Default::default())
        .collect::<Vec<_>>();
    let raw_transaction = core::TransactionBuilder::default()
        .inputs(inputs)
        .outputs(outputs)
        .outputs_data(outputs_data)
        .cell_dep(SIGHASH_ALL_CELL_DEP.clone())
        .build();
    raw_transaction
}

pub fn sign_transaction(signer: &Account, tx: core::TransactionView) -> core::TransactionView {
    let privkey = signer.privkey();
    let tx_hash = tx.hash();

    let mut blake2b = ckb_hash::new_blake2b();
    let mut message = [0u8; 32];
    blake2b.update(&tx_hash.raw_data());
    let witness_for_digest = WitnessArgs::new_builder()
        .lock(Some(Bytes::from(vec![0u8; 65])).pack())
        .build();
    let witness_len = witness_for_digest.as_bytes().len() as u64;
    blake2b.update(&witness_len.to_le_bytes());
    blake2b.update(&witness_for_digest.as_bytes());
    blake2b.finalize(&mut message);
    let message = H256::from(message);
    let sig = privkey.sign_recoverable(&message).expect("sign");
    let signed_witness = WitnessArgs::new_builder()
        .lock(Some(Bytes::from(sig.serialize())).pack())
        .build()
        .as_bytes()
        .pack();

    // calculate message
    tx.as_advanced_builder()
        .set_witnesses(vec![signed_witness])
        .build()
}
