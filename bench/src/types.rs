use crate::config::Condition;
use crate::notify::Notifier;
use crate::utils::privkey_from;
use ckb_chain_spec::{build_genesis_type_id_script, OUTPUT_INDEX_SECP256K1_BLAKE160_SIGHASH_ALL};
use ckb_crypto::secp::{Privkey, Pubkey};
use ckb_hash::blake2b_256;
use ckb_types::{
    core::{BlockNumber, BlockView, DepType, ScriptHashType, TransactionView},
    packed::{Byte32, CellDep, CellOutput, OutPoint, Script},
    prelude::*,
    H160, H256,
};
use ckb_util::{Mutex, MutexGuard};
use failure::Error;
use rpc_client::Jsonrpc;
use std::collections::HashMap;
use std::sync::Arc;
use std::thread::{spawn, JoinHandle};

pub const MIN_SECP_CELL_CAPACITY: u64 = 60_0000_0000;
pub const CELLBASE_MATURITY: u64 = 10;

pub struct TaggedTransaction {
    pub condition: Condition,
    pub transaction: TransactionView,
}

#[derive(Clone)]
pub struct LiveCell {
    pub cell_output: CellOutput,
    pub out_point: OutPoint,
    pub valid_since: BlockNumber,
}

// TODO recycle the sent failed cells (within `sent`, but the corresponding transactions are not found
#[derive(Default)]
pub struct Unspent {
    // TODO move logic inside
    pub unsent: HashMap<OutPoint, LiveCell>,
    // TODO unnecessary to store the whole transaction
    pub sent: HashMap<OutPoint, TransactionView>,
    pub block_hash: H256,
    pub block_number: BlockNumber,
}

impl Unspent {
    pub fn mark_sent(&mut self, transactions: &[TransactionView]) {
        for transaction in transactions {
            for out_point in transaction.input_pts_iter() {
                let cell = out_point.clone();
                self.unsent.remove(&cell);
                self.sent.insert(cell, transaction.clone());
            }
        }
    }

    pub fn unsent_iter(&self) -> impl Iterator<Item = (&OutPoint, &LiveCell)> {
        self.unsent
            .iter()
            .filter(move |(_, live_cell)| live_cell.valid_since < self.block_number)
    }

    pub fn update(
        &mut self,
        dead_out_points: &[OutPoint],
        live_cells: Vec<LiveCell>,
        block_hash: H256,
        block_number: BlockNumber,
    ) {
        for dead in dead_out_points.iter() {
            let cell = &dead.clone();
            self.sent.remove(cell);
            self.unsent.remove(cell);
        }
        for live in live_cells.into_iter() {
            self.unsent.insert(live.out_point.clone(), live);
        }
        self.block_hash = block_hash;
        self.block_number = block_number;
    }
}

pub struct Personal {
    privkey_string: String,
    basedir: String,
    privkey: Privkey,
    pubkey: Pubkey,
    lock_script: Script,
    cell_dep: CellDep,
    unspent: Arc<Mutex<Unspent>>,
    _handler: Option<JoinHandle<()>>,
}

impl Clone for Personal {
    fn clone(&self) -> Self {
        Self {
            privkey_string: self.privkey_string().clone(),
            basedir: self.basedir.clone(),
            privkey: privkey_from(self.privkey_string()).unwrap(),
            pubkey: self.pubkey().clone(),
            lock_script: self.lock_script().clone(),
            cell_dep: self.cell_dep().clone(),
            unspent: Arc::clone(&self.unspent),
            _handler: None,
        }
    }
}

impl Personal {
    pub fn init<S: ToString>(
        privkey_string: S,
        basedir: S,
        notifier: &mut Notifier,
    ) -> Result<Self, Error> {
        let privkey_string = privkey_string.to_string();
        let privkey = privkey_from(privkey_string.clone())?;
        let pubkey = privkey
            .pubkey()
            .expect("failed to generate pubkey from privkey");
        let address: H160 = H160::from_slice(&blake2b_256(pubkey.serialize())[0..20])
            .expect("failed to generate hash(H160) from pubkey");
        let secp = Secp::load(&notifier)?;
        let lock_script = Script::new_builder()
            .args(address.0.pack())
            .code_hash(secp.type_lock_script_code_hash())
            .hash_type(ScriptHashType::Type.into())
            .build();
        let secp_out_point = OutPoint::new(secp.dep_group_tx_hash().clone(), 0);
        let cell_dep = CellDep::new_builder()
            .out_point(secp_out_point)
            .dep_type(DepType::DepGroup.into())
            .build();
        let mut this = Self {
            privkey_string,
            basedir: basedir.to_string(),
            privkey,
            pubkey,
            lock_script,
            cell_dep,
            unspent: Arc::new(Mutex::new(Unspent::default())),
            _handler: None,
        };

        this.ready(notifier);
        Ok(this)
    }

    fn ready(&mut self, notifier: &mut Notifier) {
        self.ready_unspent(notifier);
        self._handler = Some(self.spawn_collect_unspent(notifier));
    }

    pub fn cell_dep(&self) -> &CellDep {
        &self.cell_dep
    }

    fn ready_unspent(&self, jsonrpc: &Jsonrpc) {
        // let unspent_path =
        //     Path::new(&self.basedir).join(format!("{:x}", self.lock_script().hash()));
        // if let Ok(content) = ::std::fs::read(unspent_path) {
        //     if let Ok(unspent) = bincode::deserialize::<Unspent>(&content) {
        //         if jsonrpc.get_block(unspent.block_hash.clone()).is_some() {
        //             *self.unspent() = unspent;
        //             return;
        //         }
        //     }
        // }

        let genesis = jsonrpc.get_block_by_number(0).expect("get genesis block");
        self.handle_block(&genesis.into());
    }

    pub fn privkey_string(&self) -> &String {
        &self.privkey_string
    }

    pub fn privkey(&self) -> &Privkey {
        &self.privkey
    }

    pub fn pubkey(&self) -> &Pubkey {
        &self.pubkey
    }

    pub fn unspent(&self) -> MutexGuard<Unspent> {
        self.unspent.lock()
    }

    pub fn lock_script(&self) -> &Script {
        &self.lock_script
    }

    #[allow(dead_code)]
    pub fn mark_sent(&self, transactions: &[TransactionView]) {
        self.unspent().mark_sent(transactions);
    }

    pub fn spawn_collect_unspent(&self, notifier: &mut Notifier) -> JoinHandle<()> {
        // Clone a Personal instant that share the same `unspent` field
        let that = self.clone();
        let subscriber = notifier.subscribe();
        spawn(move || {
            while let Ok(block) = subscriber.recv() {
                that.handle_block(&block);

                // if block.header().number() % 103 == 0 {
                //     that.save_unspent();
                // }
            }
        })
    }

    fn handle_block(&self, block: &BlockView) {
        let dead_out_points = self.dead_out_points(block);
        let live_cells = self.live_cells(block);
        self.unspent().update(
            &dead_out_points,
            live_cells,
            block.header().hash().unpack(),
            block.header().number(),
        );
    }

    // pub fn save_unspent(&self) {
    //     let unspent_path =
    //         Path::new(&self.basedir).join(format!("{:x}", self.lock_script().hash()));
    //     let serialized = bincode::serialize(&*self.unspent()).expect("serialize unspent");
    //     ::std::fs::write(unspent_path, serialized).expect("open unspent");
    // }

    fn dead_out_points(&self, block: &BlockView) -> Vec<OutPoint> {
        let mut deads = Vec::new();
        for transaction in block.transactions().iter().skip(1) {
            for input in transaction.input_pts_iter() {
                deads.push(input.clone());
            }
        }
        deads
    }

    // Return the owned output cells within the given block
    pub fn live_cells(&self, block: &BlockView) -> Vec<LiveCell> {
        let lock_hash = self.lock_script().calc_script_hash();
        let mut lives = Vec::new();
        for (tx_index, transaction) in block.transactions().into_iter().enumerate() {
            for (index, cell_output) in transaction.outputs().into_iter().enumerate() {
                if lock_hash != cell_output.lock().calc_script_hash() {
                    continue;
                }
                let valid_since = if tx_index == 0 {
                    block.header().number() + CELLBASE_MATURITY
                } else {
                    block.header().number()
                };
                let live_cell = LiveCell {
                    cell_output: cell_output.clone(),
                    out_point: OutPoint::new(transaction.hash().clone(), index as u32),
                    valid_since,
                };
                lives.push(live_cell);
            }
        }
        lives
    }
}

#[derive(Clone)]
pub struct Secp {
    out_point: OutPoint,
    block_hash: Byte32,
    dep_group_tx_hash: Byte32,
    type_lock_script_code_hash: Byte32,
}

impl Secp {
    pub fn load(jsonrpc: &Jsonrpc) -> Result<Self, Error> {
        let genesis = jsonrpc.get_block_by_number(0).unwrap().into();
        Secp::from_block(genesis)
    }

    pub fn dep_group_tx_hash(&self) -> Byte32 {
        self.dep_group_tx_hash.clone()
    }

    pub fn type_lock_script_code_hash(&self) -> Byte32 {
        self.type_lock_script_code_hash.clone()
    }

    pub fn out_point(&self) -> &OutPoint {
        &self.out_point
    }

    pub fn from_block(block: BlockView) -> Result<Self, Error> {
        assert_eq!(block.header().number(), 0);
        let transaction = &block.transactions()[0];
        Ok(Self {
            out_point: OutPoint::new(transaction.hash().clone(), 1u32),
            block_hash: block.header().hash().clone(),
            dep_group_tx_hash: block.transactions()[1].hash().clone(),
            type_lock_script_code_hash: build_genesis_type_id_script(
                OUTPUT_INDEX_SECP256K1_BLAKE160_SIGHASH_ALL,
            )
            .calc_script_hash(),
        })
    }
}
