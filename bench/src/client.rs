use crate::config::Config;
use ckb_types::{core::BlockNumber, packed::Header};
use failure::Error;
use rpc_client::Jsonrpc;
use std::ops::Deref;

// We use the first ckb_node as the baseline node.
pub struct Client {
    ckb_nodes: Vec<Jsonrpc>,
}

impl Deref for Client {
    type Target = Jsonrpc;

    fn deref(&self) -> &Self::Target {
        &self.ckb_nodes[0]
    }
}

impl Client {
    pub fn init(config: &Config) -> Result<Self, Error> {
        let ckb_nodes = config
            .rpc_urls
            .iter()
            .map(|uri| Jsonrpc::connect(uri.as_str()))
            .collect::<Result<Vec<Jsonrpc>, Error>>()?;
        Ok(Self { ckb_nodes })
    }

    pub fn get_safe_tip_header(&self) -> Header {
        let mut tip_number = self.ckb_nodes[0].get_tip_block_number();
        loop {
            if let Some(header) = self.get_safe_block(tip_number) {
                return header;
            }
            tip_number -= 1;
        }
    }

    pub fn get_safe_block(&self, block_number: BlockNumber) -> Option<Header> {
        let mut safe = None;
        for jsonrpc in self.ckb_nodes.iter() {
            if let Some(header) = jsonrpc.get_header_by_number(block_number) {
                if safe.is_none() {
                    safe = Some(header);
                } else if safe.as_ref().map(|b| b != &header).unwrap() {
                    return None;
                }
            } else {
                return None;
            }
        }
        safe.map(|h| h.inner.into())
    }

    pub fn get_max_tip(&self) -> BlockNumber {
        self.ckb_nodes
            .iter()
            .map(Jsonrpc::get_tip_block_number)
            .max()
            .unwrap()
    }
}
