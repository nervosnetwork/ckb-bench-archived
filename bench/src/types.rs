use crate::config::Condition;
use crate::notify::Notifier;
use crate::utils::privkey_from;
use ckb_core::block::Block;
use ckb_core::script::{Script, ScriptHashType};
use ckb_core::transaction::{CellOutPoint, CellOutput, OutPoint, Transaction};
use ckb_core::{BlockNumber, Bytes};
use ckb_crypto::secp::{Privkey, Pubkey};
use ckb_hash::blake2b_256;
use ckb_util::{Mutex, MutexGuard};
use failure::Error;
use numext_fixed_hash::{H160, H256};
use rpc_client::Jsonrpc;
use serde_derive::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::thread::{spawn, JoinHandle};

pub const MIN_SECP_CELL_CAPACITY: u64 = 60_0000_0000;
pub const CELLBASE_MATURITY: u64 = 10;
pub const PROPOSAL_WINDOW: u64 = 10;

pub struct TaggedTransaction {
    pub condition: Condition,
    pub transaction: Transaction,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct LiveCell {
    pub cell_output: CellOutput,
    pub out_point: OutPoint,
    pub valid_since: BlockNumber,
}

// TODO recycle the sent failed cells (within `sent`, but the corresponding transactions are not found
#[derive(Default, Deserialize, Serialize)]
pub struct Unspent {
    // TODO move logic inside
    pub unsent: HashMap<CellOutPoint, LiveCell>,
    // TODO unnecessary to store the whole transaction
    pub sent: HashMap<CellOutPoint, Transaction>,
    pub block_hash: H256,
    pub block_number: BlockNumber,
}

impl Unspent {
    pub fn mark_sent(&mut self, transactions: &[Transaction]) {
        for transaction in transactions {
            for out_point in transaction.input_pts_iter() {
                let cell = out_point.cell.clone().unwrap();
                self.unsent.remove(&cell);
                self.sent.insert(cell, transaction.clone());
            }
        }
    }

    pub fn unsent_iter(&self) -> impl Iterator<Item = (&CellOutPoint, &LiveCell)> {
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
            let cell = &dead.cell.clone().unwrap();
            self.sent.remove(cell);
            self.unsent.remove(cell);
        }
        for live in live_cells.into_iter() {
            self.unsent
                .insert(live.out_point.clone().cell.unwrap(), live);
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
    dep_out_point: OutPoint,
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
            dep_out_point: self.dep_out_point().clone(),
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
        let address = H160::from_slice(&blake2b_256(pubkey.serialize())[0..20])
            .expect("failed to generate hash(H160) from pubkey");
        let secp = Secp::load(&notifier)?;
        let lock_script = Script {
            args: vec![Bytes::from(address.as_bytes())],
            code_hash: secp.code_hash(),
            hash_type: ScriptHashType::Data,
        };
        let dep_out_point = OutPoint {
            cell: Some(secp.out_point),
            block_hash: Some(secp.block_hash),
        };
        let mut this = Self {
            privkey_string,
            basedir: basedir.to_string(),
            privkey,
            pubkey,
            lock_script,
            dep_out_point,
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

    pub fn dep_out_point(&self) -> &OutPoint {
        &self.dep_out_point
    }

    fn ready_unspent(&self, jsonrpc: &Jsonrpc) {
        let unspent_path =
            Path::new(&self.basedir).join(format!("{:x}", self.lock_script().hash()));
        if let Ok(content) = ::std::fs::read(unspent_path) {
            if let Ok(unspent) = bincode::deserialize::<Unspent>(&content) {
                if jsonrpc.get_block(unspent.block_hash.clone()).is_some() {
                    *self.unspent() = unspent;
                    return;
                }
            }
        }

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
    pub fn mark_sent(&self, transactions: &[Transaction]) {
        self.unspent().mark_sent(transactions);
    }

    pub fn spawn_collect_unspent(&self, notifier: &mut Notifier) -> JoinHandle<()> {
        // Clone a Personal instant that share the same `unspent` field
        let that = self.clone();
        let subscriber = notifier.subscribe();
        spawn(move || {
            while let Ok(block) = subscriber.recv() {
                that.handle_block(&block);

                if block.header().number() % 103 == 0 {
                    that.save_unspent();
                }
            }
        })
    }

    fn handle_block(&self, block: &Block) {
        let dead_out_points = self.dead_out_points(block);
        let live_cells = self.live_cells(block);
        self.unspent().update(
            &dead_out_points,
            live_cells,
            block.header().hash().clone(),
            block.header().number(),
        );
    }

    pub fn save_unspent(&self) {
        let unspent_path =
            Path::new(&self.basedir).join(format!("{:x}", self.lock_script().hash()));
        let serialized = bincode::serialize(&*self.unspent()).expect("serialize unspent");
        ::std::fs::write(unspent_path, serialized).expect("open unspent");
    }

    fn dead_out_points(&self, block: &Block) -> Vec<OutPoint> {
        let mut deads = Vec::new();
        for transaction in block.transactions().iter().skip(1) {
            for input in transaction.input_pts_iter() {
                deads.push(input.clone());
            }
        }
        deads
    }

    // Return the owned output cells within the given block
    pub fn live_cells(&self, block: &Block) -> Vec<LiveCell> {
        let lock_hash = self.lock_script().hash();
        let mut lives = Vec::new();
        for (tx_index, transaction) in block.transactions().iter().enumerate() {
            for (index, cell_output) in transaction.outputs().iter().enumerate() {
                if lock_hash != cell_output.lock.hash() {
                    continue;
                }
                let valid_since = if tx_index == 0 {
                    block.header().number() + CELLBASE_MATURITY
                } else {
                    block.header().number()
                };
                let live_cell = LiveCell {
                    cell_output: cell_output.clone(),
                    out_point: OutPoint {
                        block_hash: None,
                        cell: Some(CellOutPoint {
                            tx_hash: transaction.hash().clone(),
                            index: index as u32,
                        }),
                    },
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
    cell_output: CellOutput,
    out_point: CellOutPoint,
    block_hash: H256,
}

impl Secp {
    pub fn load(jsonrpc: &Jsonrpc) -> Result<Self, Error> {
        let genesis = jsonrpc.get_block_by_number(0).unwrap().into();
        Secp::from_block(genesis)
    }

    pub fn code_hash(&self) -> H256 {
        self.cell_output.data_hash()
    }

    pub fn out_point(&self) -> &CellOutPoint {
        &self.out_point
    }

    pub fn from_block(block: Block) -> Result<Self, Error> {
        assert_eq!(block.header().number(), 0);
        assert_eq!(block.transactions().len(), 1);
        let transaction = &block.transactions()[0];
        let index = 1;
        let cell = transaction.outputs()[index].clone();
        Ok(Self {
            cell_output: cell,
            out_point: CellOutPoint {
                tx_hash: transaction.hash().clone(),
                index: index as u32,
            },
            block_hash: block.header().hash().clone(),
        })
    }
}
