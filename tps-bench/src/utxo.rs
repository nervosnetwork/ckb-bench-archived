use ckb_types::packed::{CellInput, CellOutput, OutPoint};
use ckb_types::prelude::*;

#[derive(Debug, Clone)]
pub struct UTXO {
    output: CellOutput,
    out_point: OutPoint,
}

impl UTXO {
    pub fn new(output: CellOutput, out_point: OutPoint) -> Self {
        Self { output, out_point }
    }

    pub fn output(&self) -> &CellOutput {
        &self.output
    }

    pub fn out_point(&self) -> &OutPoint {
        &self.out_point
    }

    pub fn capacity(&self) -> u64 {
        self.output.capacity().unpack()
    }

    pub fn as_previous_input(&self) -> CellInput {
        CellInput::new_builder()
            .previous_output(self.out_point().clone())
            .build()
    }
}
