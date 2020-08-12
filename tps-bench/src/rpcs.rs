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
        let mut tip_number = self.endpoints[0].get_tip_block_number();
        loop {
            if let Some(header) = self.is_fixed(tip_number) {
                return header;
            }
            tip_number -= 1;
        }
    }

    pub fn is_fixed(&self, number: BlockNumber) -> Option<Header> {
        let mut header0 = None;
        for jsonrpc in self.endpoints.iter() {
            if let Some(header) = jsonrpc.get_header_by_number(number) {
                if header0.is_none() {
                    header0 = Some(header);
                } else if header0.as_ref().map(|h| h != &header).unwrap() {
                    return None;
                }
            } else {
                return None;
            }
        }
        header0.map(|h| h.inner.into())
    }
}
