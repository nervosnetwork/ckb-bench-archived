use crate::Jsonrpc;

use ckb_types::core::BlockNumber;
use ckb_types::packed::Header;
use std::ops::Deref;

#[derive(Clone)]
pub struct Jsonrpcs {
    endpoints: Vec<Jsonrpc>,
}

impl Deref for Jsonrpcs {
    type Target = Jsonrpc;
    fn deref(&self) -> &Self::Target {
        self.endpoints.first().unwrap()
    }
}

impl Jsonrpcs {
    pub fn connect_all(uris: Vec<&str>) -> Result<Self, String> {
        Ok(Self {
            endpoints: uris
                .into_iter()
                .map(Jsonrpc::connect)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    pub fn endpoints(&self) -> &Vec<Jsonrpc> {
        &self.endpoints
    }

    pub fn get_fixed_tip_number(&self) -> BlockNumber {
        self.get_fixed_tip_header().into_view().number()
    }

    pub fn get_fixed_tip_header(&self) -> Header {
        let tip_number = self.endpoints[0].get_tip_block_number();
        for number in (0..=tip_number).rev() {
            if let Some(header) = self.endpoints[0].get_header_by_number(number) {
                let is_fixed = self.endpoints[1..self.endpoints.len()].iter().all(|rpc| {
                    rpc.get_header_by_number(number)
                        .map(|h| h == header)
                        .unwrap_or(false)
                });
                if is_fixed {
                    return header.inner.into();
                }
            };
        }
        unreachable!()
    }
}
