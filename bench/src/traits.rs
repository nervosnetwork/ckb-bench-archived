use ckb_occupied_capacity::Capacity;
use ckb_types::packed;
use ckb_types::prelude::*;

pub trait PackedCapacityAsU64 {
    fn as_u64_capacity(self) -> u64;
}

impl PackedCapacityAsU64 for packed::Uint64 {
    fn as_u64_capacity(self) -> u64 {
        self.as_capacity().as_u64()
    }
}

pub trait PackedCapacityAsCapacity {
    fn as_capacity(self) -> Capacity;
}

impl PackedCapacityAsCapacity for packed::Uint64 {
    fn as_capacity(self) -> Capacity {
        self.unpack()
    }
}
