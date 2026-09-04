//! Replace consecutive constant memory stores with program-data copies.
//!
//! The pass recognizes a contiguous run of literal word stores, appends its bytes to the program
//! data pool, and replaces the stores with `CODECOPY`. It charges both the new data and the copy
//! sequence against the original instructions, so it only accepts a byte saving. It skips modules
//! that observe program-data layout through `CODESIZE`, because appending data would change the
//! observed final byte.

use super::{EvmPass, data::literal_store_run, utils::instruction_size_lower_bound};
use crate::{
    backend::evm::{
        ir::{BlockId, Data, DataRef, Instruction, Metadata, Module},
        op::{self, WORD_BYTES},
    },
    lower::data_copy_cost,
};
use alloy_primitives::{Bytes, U256};
use solar_interface::sym;
use solar_sema::Gcx;

pub(super) struct ConstantData;

impl EvmPass for ConstantData {
    fn name(&self) -> &'static str {
        "constant-data"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        materialize_constant_data(gcx, module)
    }
}

struct Rewrite {
    block: BlockId,
    start: usize,
    end: usize,
    data: Bytes,
}

fn materialize_constant_data(gcx: Gcx<'_>, module: &mut Module) -> bool {
    if module.data_layout_is_observable() {
        return false;
    }

    let mut rewrites = Vec::new();
    for (block_id, block) in module.blocks.iter_enumerated() {
        let mut start = 0;
        while start < block.instructions.len() {
            let Some(rewrite) = find_run(gcx, block_id, &block.instructions, start) else {
                start += 1;
                continue;
            };
            start = rewrite.end;
            rewrites.push(rewrite);
        }
    }
    if rewrites.is_empty() {
        return false;
    }

    let mut prepared = Vec::with_capacity(rewrites.len());
    for rewrite in rewrites {
        let size = rewrite.data.len();
        let id = module.data.push(Data {
            bytes: rewrite.data,
            name: Some(sym::literal),
            emit_in_runtime: false,
        });
        let data = DataRef::new(id, 0);
        prepared.push((rewrite.block, rewrite.start, rewrite.end, size, data));
    }
    for (block, start, end, size, data) in prepared.into_iter().rev() {
        // The copy carries every origin of the stores it replaces; their function events land
        // on the copy itself, the last replacement.
        let mut metadata = Metadata::default();
        for inst in &module.blocks[block].instructions[start..end] {
            metadata.absorb_debug_info(&inst.metadata);
        }
        let mut replacement = [
            Instruction::push_value(U256::from(size)),
            Instruction::push_data(data),
            Instruction::stack_op(op::StackOp::Dup(3)),
            Instruction::opcode(op::CODECOPY),
        ];
        for inst in &mut replacement {
            inst.metadata.copy_source_debug_from(&metadata);
        }
        replacement[3].metadata.absorb_debug_info(&metadata);
        module.blocks[block].instructions.splice(start..end, replacement);
    }
    true
}

fn find_run(
    gcx: Gcx<'_>,
    block: BlockId,
    instructions: &[Instruction],
    start: usize,
) -> Option<Rewrite> {
    let (data, end) = literal_store_run(instructions, start)?;
    if data.len() < 2 * WORD_BYTES {
        return None;
    }

    let old_size = instructions[start..end]
        .iter()
        .map(|inst| instruction_size_lower_bound(gcx, inst))
        .sum::<usize>();
    // Account for PUSH3 conservatively so a selected rewrite cannot grow an
    // EIP-170-sized program when the data lands above the PUSH2 boundary.
    let new_size = data.len() + data_copy_cost(gcx.sess.opts.evm_version, data.len()).0;
    (new_size < old_size).then(|| Rewrite { block, start, end, data })
}
