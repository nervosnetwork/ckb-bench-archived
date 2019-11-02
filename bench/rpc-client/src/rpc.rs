use ckb_jsonrpc_types::{
    Block, BlockNumber, BlockTemplate, BlockView, CellOutputWithOutPoint, CellWithStatus,
    ChainInfo, DryRunResult, HeaderView, Node, OutPoint, PeerState, Transaction,
    TransactionWithStatus, TxPoolInfo, Uint64, Version,
};
use ckb_types::{
    core::{BlockNumber as CoreBlockNumber, Version as CoreVersion},
    packed::Byte32,
    prelude::*,
    H256,
};
use ckb_util::Mutex;
use failure::{format_err, Error};
use hyper::header::{Authorization, Basic};
use jsonrpc_client_core::{expand_params, jsonrpc_client, Result as JsonRpcResult};
use jsonrpc_client_http::{HttpHandle, HttpTransport};
use std::env::var;
use std::sync::Arc;

#[derive(Clone)]
pub struct Jsonrpc {
    uri: String,
    inner: Arc<Mutex<Inner<HttpHandle>>>,
}

pub fn username() -> String {
    var("CKB_STAGING_USERNAME").unwrap_or_else(|_| "".to_owned())
}

pub fn password() -> String {
    var("CKB_STAGING_PASSWORD").unwrap_or_else(|_| "".to_owned())
}

impl Jsonrpc {
    pub fn connect(uri: &str) -> Result<Self, Error> {
        let transport = HttpTransport::new().standalone().unwrap();
        match transport.handle(uri) {
            Ok(mut transport) => {
                if !username().is_empty() && !password().is_empty() {
                    transport.set_header(Authorization(Basic {
                        username: username(),
                        password: Some(password()),
                    }));
                }
                Ok(Self {
                    uri: uri.to_string(),
                    inner: Arc::new(Mutex::new(Inner::new(transport))),
                })
            }
            Err(err) => Err(format_err!("{}", err)),
        }
    }

    pub fn uri(&self) -> &String {
        &self.uri
    }

    pub fn inner(&self) -> &Mutex<Inner<HttpHandle>> {
        &self.inner
    }

    pub fn get_block(&self, hash: Byte32) -> Option<BlockView> {
        self.inner
            .lock()
            .get_block(hash.unpack())
            .call()
            .expect("rpc call get_block")
    }

    pub fn get_block_by_number(&self, number: CoreBlockNumber) -> Option<BlockView> {
        self.inner
            .lock()
            .get_block_by_number(number.into())
            .call()
            .expect("rpc call get_block_by_number")
    }

    pub fn get_transaction(&self, hash: Byte32) -> Option<TransactionWithStatus> {
        self.inner
            .lock()
            .get_transaction(hash.unpack())
            .call()
            .expect("rpc call get_transaction")
    }

    pub fn get_block_hash(&self, number: CoreBlockNumber) -> Option<H256> {
        self.inner
            .lock()
            .get_block_hash(number.into())
            .call()
            .expect("rpc call get_block_hash")
    }

    pub fn get_tip_header(&self) -> HeaderView {
        self.inner
            .lock()
            .get_tip_header()
            .call()
            .expect("rpc call get_block_hash")
    }

    pub fn get_header_by_number(&self, number: CoreBlockNumber) -> Option<HeaderView> {
        self.inner
            .lock()
            .get_header_by_number(number.into())
            .call()
            .expect("rpc call get_header_by_number")
    }

    pub fn get_cells_by_lock_hash(
        &self,
        lock_hash: Byte32,
        from: CoreBlockNumber,
        to: CoreBlockNumber,
    ) -> Vec<CellOutputWithOutPoint> {
        self.inner
            .lock()
            .get_cells_by_lock_hash(lock_hash.unpack(), from.into(), to.into())
            .call()
            .expect("rpc call get_cells_by_lock_hash")
    }

    pub fn get_live_cell(&self, out_point: OutPoint) -> CellWithStatus {
        self.inner
            .lock()
            .get_live_cell(out_point)
            .call()
            .expect("rpc call get_live_cell")
    }

    pub fn get_tip_block_number(&self) -> CoreBlockNumber {
        self.inner
            .lock()
            .get_tip_block_number()
            .call()
            .expect("rpc call get_tip_block_number")
            .into()
    }

    pub fn local_node_info(&self) -> Node {
        self.inner
            .lock()
            .local_node_info()
            .call()
            .expect("rpc call local_node_info")
    }

    pub fn get_peers(&self) -> Vec<Node> {
        self.inner
            .lock()
            .get_peers()
            .call()
            .expect("rpc call get_peers")
    }

    pub fn get_block_template(
        &self,
        bytes_limit: Option<u64>,
        proposals_limit: Option<u64>,
        max_version: Option<CoreVersion>,
    ) -> BlockTemplate {
        let bytes_limit = bytes_limit.map(Into::into);
        let proposals_limit = proposals_limit.map(Into::into);
        let max_version = max_version.map(Into::into);
        self.inner
            .lock()
            .get_block_template(bytes_limit, proposals_limit, max_version)
            .call()
            .expect("rpc call get_block_template")
    }

    pub fn submit_block(&self, work_id: String, block: Block) -> Option<H256> {
        self.inner
            .lock()
            .submit_block(work_id, block)
            .call()
            .expect("rpc call submit_block")
    }

    pub fn get_blockchain_info(&self) -> ChainInfo {
        self.inner
            .lock()
            .get_blockchain_info()
            .call()
            .expect("rpc call get_blockchain_info")
    }

    pub fn send_transaction(&self, tx: Transaction) -> H256 {
        self.inner
            .lock()
            .send_transaction(tx)
            .call()
            .expect("rpc call send_transaction")
    }

    pub fn broadcast_transaction(&self, tx: Transaction) -> H256 {
        self.inner
            .lock()
            .broadcast_transaction(tx)
            .call()
            .expect("rpc call send_transaction")
    }

    pub fn send_transaction_result(&self, tx: Transaction) -> JsonRpcResult<H256> {
        self.inner.lock().send_transaction(tx).call()
    }

    pub fn tx_pool_info(&self) -> TxPoolInfo {
        self.inner
            .lock()
            .tx_pool_info()
            .call()
            .expect("rpc call tx_pool_info")
    }

    pub fn add_node(&self, peer_id: String, address: String) {
        self.inner
            .lock()
            .add_node(peer_id, address)
            .call()
            .expect("rpc call add_node");
    }

    pub fn remove_node(&self, peer_id: String) {
        self.inner
            .lock()
            .remove_node(peer_id)
            .call()
            .expect("rpc call remove_node")
    }

    pub fn process_block_without_verify(&self, block: Block) -> Option<H256> {
        self.inner
            .lock()
            .process_block_without_verify(block)
            .call()
            .expect("rpc call process_block_without verify")
    }
}

jsonrpc_client!(pub struct Inner {
    pub fn get_block(&mut self, _hash: H256) -> RpcRequest<Option<BlockView>>;
    pub fn get_block_by_number(&mut self, _number: BlockNumber) -> RpcRequest<Option<BlockView>>;
    pub fn get_header_by_number(&mut self, _number: BlockNumber) -> RpcRequest<Option<HeaderView>>;
    pub fn get_transaction(&mut self, _hash: H256) -> RpcRequest<Option<TransactionWithStatus>>;
    pub fn get_block_hash(&mut self, _number: BlockNumber) -> RpcRequest<Option<H256>>;
    pub fn get_tip_header(&mut self) -> RpcRequest<HeaderView>;
    pub fn get_cells_by_lock_hash(
        &mut self,
        _lock_hash: H256,
        _from: BlockNumber,
        _to: BlockNumber
    ) -> RpcRequest<Vec<CellOutputWithOutPoint>>;
    pub fn get_live_cell(&mut self, _out_point: OutPoint) -> RpcRequest<CellWithStatus>;
    pub fn get_tip_block_number(&mut self) -> RpcRequest<BlockNumber>;
    pub fn local_node_info(&mut self) -> RpcRequest<Node>;
    pub fn get_peers(&mut self) -> RpcRequest<Vec<Node>>;
    pub fn get_block_template(
        &mut self,
        bytes_limit: Option<Uint64>,
        proposals_limit: Option<Uint64>,
        max_version: Option<Version>
    ) -> RpcRequest<BlockTemplate>;
    pub fn submit_block(&mut self, _work_id: String, _data: Block) -> RpcRequest<Option<H256>>;
    pub fn get_blockchain_info(&mut self) -> RpcRequest<ChainInfo>;
    pub fn get_peers_state(&mut self) -> RpcRequest<Vec<PeerState>>;
    pub fn compute_transaction_hash(&mut self, tx: Transaction) -> RpcRequest<H256>;
    pub fn dry_run_transaction(&mut self, _tx: Transaction) -> RpcRequest<DryRunResult>;
    pub fn send_transaction(&mut self, tx: Transaction) -> RpcRequest<H256>;
    pub fn broadcast_transaction(&mut self, tx: Transaction) -> RpcRequest<H256>;
    pub fn tx_pool_info(&mut self) -> RpcRequest<TxPoolInfo>;

    pub fn add_node(&mut self, peer_id: String, address: String) -> RpcRequest<()>;
    pub fn remove_node(&mut self, peer_id: String) -> RpcRequest<()>;
    pub fn process_block_without_verify(&mut self, _data: Block) -> RpcRequest<Option<H256>>;
});
