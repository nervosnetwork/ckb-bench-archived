use ckb_jsonrpc_types::{
    Block, BlockNumber, BlockTemplate, BlockView, CellWithStatus, ChainInfo, DryRunResult,
    HeaderView, LocalNode, OutPoint, PeerState, RemoteNode, Transaction, TransactionWithStatus,
    TxPoolInfo, Uint64, Version,
};
use ckb_types::{
    core::{BlockNumber as CoreBlockNumber, Version as CoreVersion},
    packed::Byte32,
    prelude::*,
    H256,
};
use ckb_util::Mutex;
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
    pub fn connect(uri: &str) -> Self {
        let transport = HttpTransport::new().standalone().unwrap();
        let mut transport = transport
            .handle(uri)
            .unwrap_or_else(|_| panic!("Jsonrpc::connect({})", uri));

        if !username().is_empty() && !password().is_empty() {
            transport.set_header(Authorization(Basic {
                username: username(),
                password: Some(password()),
            }));
        }
        Self {
            uri: uri.to_string(),
            inner: Arc::new(Mutex::new(Inner::new(transport))),
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
            .unwrap_or_else(|_| panic!("Jsonrpc::get_block({}, {})", self.uri(), hash))
    }

    pub fn get_block_by_number(&self, number: CoreBlockNumber) -> Option<BlockView> {
        self.inner
            .lock()
            .get_block_by_number(number.into())
            .call()
            .unwrap_or_else(|_| panic!("Jsonrpc::get_block_by_number({}, {})", self.uri(), number))
    }

    pub fn get_transaction(&self, hash: Byte32) -> Option<TransactionWithStatus> {
        self.inner
            .lock()
            .get_transaction(hash.unpack())
            .call()
            .unwrap_or_else(|_| panic!("Jsonrpc::get_transaction({}, {})", self.uri(), hash))
    }

    pub fn get_block_hash(&self, number: CoreBlockNumber) -> Option<H256> {
        self.inner
            .lock()
            .get_block_hash(number.into())
            .call()
            .unwrap_or_else(|_| panic!("Jsonrpc::get_block_hash({}, {})", self.uri(), number))
    }

    pub fn get_tip_header(&self) -> HeaderView {
        self.inner
            .lock()
            .get_tip_header()
            .call()
            .unwrap_or_else(|_| panic!("Jsonrpc::get_tip_header({})", self.uri()))
    }

    pub fn get_header_by_number(&self, number: CoreBlockNumber) -> Option<HeaderView> {
        self.inner
            .lock()
            .get_header_by_number(number.into())
            .call()
            .unwrap_or_else(|_| panic!("Jsonrpc::get_header_by_number({}, {})", self.uri(), number))
    }

    pub fn get_live_cell(&self, out_point: OutPoint) -> CellWithStatus {
        self.inner
            .lock()
            .get_live_cell(out_point.clone())
            .call()
            .unwrap_or_else(|_| panic!("Jsonrpc::get_live_cell({}, {:?})", self.uri(), out_point))
    }

    pub fn get_tip_block_number(&self) -> CoreBlockNumber {
        self.inner
            .lock()
            .get_tip_block_number()
            .call()
            .unwrap_or_else(|_| panic!("Jsonrpc::get_tip_block_number({})", self.uri()))
            .into()
    }

    pub fn local_node_info(&self) -> LocalNode {
        self.inner
            .lock()
            .local_node_info()
            .call()
            .unwrap_or_else(|_| panic!("Jsonrpc::local_node_info({})", self.uri()))
    }

    pub fn get_peers(&self) -> Vec<RemoteNode> {
        self.inner
            .lock()
            .get_peers()
            .call()
            .unwrap_or_else(|_| panic!("Jsonrpc::get_peers({})", self.uri()))
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
            .unwrap_or_else(|_| {
                panic!(
                    "Jsonrpc::get_block_template({}, {:?}, {:?}, {:?})",
                    self.uri(),
                    bytes_limit,
                    proposals_limit,
                    max_version
                )
            })
    }

    pub fn submit_block(&self, work_id: String, block: Block) -> Option<H256> {
        self.inner
            .lock()
            .submit_block(work_id, block.clone())
            .call()
            .unwrap_or_else(|_| panic!("Jsonrpc::submit_block({}, {:?})", self.uri(), block))
    }

    pub fn get_blockchain_info(&self) -> ChainInfo {
        self.inner
            .lock()
            .get_blockchain_info()
            .call()
            .unwrap_or_else(|_| panic!("Jsonrpc::get_blockchain_info({})", self.uri()))
    }

    pub fn send_transaction(&self, tx: Transaction) -> H256 {
        self.inner
            .lock()
            .send_transaction(tx.clone())
            .call()
            .unwrap_or_else(|_| panic!("Jsonrpc::send_transaction({}, {:?})", self.uri(), tx))
    }

    pub fn broadcast_transaction(&self, tx: Transaction) -> H256 {
        self.inner
            .lock()
            .broadcast_transaction(tx.clone())
            .call()
            .unwrap_or_else(|_| panic!("Jsonrpc::broadcast_transaction({}, {:?})", self.uri(), tx))
    }

    pub fn send_transaction_result(&self, tx: Transaction) -> JsonRpcResult<H256> {
        self.inner.lock().send_transaction(tx).call()
    }

    pub fn tx_pool_info(&self) -> TxPoolInfo {
        self.inner
            .lock()
            .tx_pool_info()
            .call()
            .unwrap_or_else(|_| panic!("Jsonrpc::tx_pool_info({})", self.uri()))
    }

    pub fn add_node(&self, peer_id: String, address: String) {
        self.inner
            .lock()
            .add_node(peer_id.clone(), address.clone())
            .call()
            .unwrap_or_else(|_| {
                panic!(
                    "Jsonrpc::add_node({}, {}, {})",
                    self.uri(),
                    peer_id,
                    address
                )
            });
    }

    pub fn remove_node(&self, peer_id: String) {
        self.inner
            .lock()
            .remove_node(peer_id.clone())
            .call()
            .unwrap_or_else(|_| panic!("Jsonrpc::remove_node({}, {})", self.uri(), peer_id))
    }

    pub fn process_block_without_verify(&self, block: Block) -> Option<H256> {
        self.inner
            .lock()
            .process_block_without_verify(block.clone())
            .call()
            .unwrap_or_else(|_| {
                panic!(
                    "Jsonrpc::process_block_without_verify({}, {:?})",
                    self.uri(),
                    block
                )
            })
    }
}

jsonrpc_client!(pub struct Inner {
    pub fn get_block(&mut self, _hash: H256) -> RpcRequest<Option<BlockView>>;
    pub fn get_block_by_number(&mut self, _number: BlockNumber) -> RpcRequest<Option<BlockView>>;
    pub fn get_header_by_number(&mut self, _number: BlockNumber) -> RpcRequest<Option<HeaderView>>;
    pub fn get_transaction(&mut self, _hash: H256) -> RpcRequest<Option<TransactionWithStatus>>;
    pub fn get_block_hash(&mut self, _number: BlockNumber) -> RpcRequest<Option<H256>>;
    pub fn get_tip_header(&mut self) -> RpcRequest<HeaderView>;
    pub fn get_live_cell(&mut self, _out_point: OutPoint) -> RpcRequest<CellWithStatus>;
    pub fn get_tip_block_number(&mut self) -> RpcRequest<BlockNumber>;
    pub fn local_node_info(&mut self) -> RpcRequest<LocalNode>;
    pub fn get_peers(&mut self) -> RpcRequest<Vec<RemoteNode>>;
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
