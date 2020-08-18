use crate::global::CONFIRMATION_BLOCKS;
use crate::Jsonrpc;

use ckb_types::core::{BlockNumber, BlockView};
use ckb_types::packed::Header;
use std::ops::Deref;

#[derive(Clone)]
pub struct Net {
    endpoints: Vec<Jsonrpc>,
}

impl Deref for Net {
    type Target = Jsonrpc;
    fn deref(&self) -> &Self::Target {
        self.endpoints.first().unwrap()
    }
}

impl Net {
    pub fn connect_all(uris: Vec<&str>) -> Self {
        Self {
            endpoints: uris.into_iter().map(Jsonrpc::connect).collect(),
        }
    }

    pub fn endpoints(&self) -> &Vec<Jsonrpc> {
        &self.endpoints
    }

    pub fn get_confirmed_tip_number(&self) -> BlockNumber {
        self.get_confirmed_tip_header().into_view().number()
    }

    pub fn get_confirmed_tip_block(&self) -> BlockView {
        let header = self.get_confirmed_tip_header();
        let block = self.get_block(header.calc_header_hash()).unwrap();
        block.into()
    }

    pub fn get_confirmed_tip_header(&self) -> Header {
        let unconfirmed = self.get_unconfirmed_fixed_tip_header();
        let unconfirmed_number = unconfirmed.into_view().number();
        let confirmed_number =
            unconfirmed_number.saturating_sub(*CONFIRMATION_BLOCKS.lock().unwrap());
        self.get_header_by_number(confirmed_number)
            .expect(&format!(
                "rpc.get_header_by_number({}, unconfirmed={}, confirmed={})",
                self.endpoints[0].uri(),
                unconfirmed_number,
                confirmed_number
            ))
            .inner
            .into()
    }

    fn get_unconfirmed_fixed_tip_header(&self) -> Header {
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
