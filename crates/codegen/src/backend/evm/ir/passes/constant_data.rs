//! Replace consecutive constant memory stores with program-data copies.
//!
//! The pass appends each copied run to program data and replaces its stores
//! with `CODECOPY`. It skips modules that observe program-data layout through
//! `CODESIZE`, because appending data would change the observed final byte.

use super::{
    EvmPass,
    utils::{StackDepths, relative_stack_depths},
};
use crate::backend::evm::{
    ir::{BlockId, Data, DataRef, Instruction, Module, PushValue},
    op,
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

    let mut depths = None;
    let mut rewrites = Vec::new();
    for (block_id, block) in module.blocks.iter_enumerated() {
        let relative_depths = relative_stack_depths(&block.instructions);
        let high_water = relative_depths.as_ref().and_then(|depths| depths.iter().copied().max());
        let mut start = 0;
        while start < block.instructions.len() {
            let Some(rewrite) = find_run(gcx, block_id, &block.instructions, start) else {
                start += 1;
                continue;
            };
            start = rewrite.end;
            let fits_existing_peak = relative_depths.as_ref().is_some_and(|relative_depths| {
                high_water
                    .is_some_and(|high_water| relative_depths[rewrite.start] + 3 <= high_water)
            });
            let fits = fits_existing_peak
                || depths
                    .get_or_insert_with(|| StackDepths::new(module))
                    .as_ref()
                    .is_some_and(|depths| depths.has_headroom(block_id, rewrite.start, 3));
            if fits {
                rewrites.push(rewrite);
            }
        }
    }
    if rewrites.is_empty() {
        return false;
    }

    let mut prepared = Vec::with_capacity(rewrites.len());
    for rewrite in rewrites {
        let size = rewrite.data.len();
        let id = module.data.push(Data { bytes: rewrite.data, name: Some(sym::literal) });
        let data = DataRef::new(id, 0);
        prepared.push((rewrite.block, rewrite.start, rewrite.end, size, data));
    }
    for (block, start, end, size, data) in prepared.into_iter().rev() {
        module.blocks[block].instructions.splice(
            start..end,
            [
                Instruction::push_value(U256::from(size)),
                Instruction::push_data(data),
                Instruction::stack_op(op::StackOp::Dup(3)),
                Instruction::opcode(op::CODECOPY),
            ],
        );
    }
    true
}

fn find_run(
    gcx: Gcx<'_>,
    block: BlockId,
    instructions: &[Instruction],
    start: usize,
) -> Option<Rewrite> {
    let [value, dup, store, ..] = instructions.get(start..)? else { return None };
    let first = immediate(value)?;
    if dup.as_legacy_opcode() != Some(op::DUP2) || store.as_legacy_opcode() != Some(op::MSTORE) {
        return None;
    }

    let mut data = Vec::from(first.to_be_bytes::<32>());
    let mut end = start + 3;
    let mut words = 1usize;
    while let Some(window) = instructions.get(end..end + 6) {
        let [offset, dup, add, value, swap, store] = window else { unreachable!() };
        if immediate(offset) != Some(U256::from(words * 32))
            || dup.as_legacy_opcode() != Some(op::DUP2)
            || add.as_legacy_opcode() != Some(op::ADD)
            || swap.as_legacy_opcode() != Some(op::SWAP1)
            || store.as_legacy_opcode() != Some(op::MSTORE)
        {
            break;
        }
        let Some(value) = immediate(value) else { break };
        data.extend_from_slice(&value.to_be_bytes::<32>());
        words += 1;
        end += 6;
    }
    if words < 2 {
        return None;
    }

    let old_size =
        instructions[start..end].iter().map(|inst| encoded_len(gcx, inst)).sum::<usize>();
    // Account for PUSH3 conservatively so a selected rewrite cannot grow an
    // EIP-170-sized program when the data lands above the PUSH2 boundary.
    let new_size =
        data.len() + op::push_len(gcx.sess.opts.evm_version, U256::from(data.len())) + 4 + 2;
    (new_size < old_size).then(|| Rewrite { block, start, end, data: data.into() })
}

fn encoded_len(gcx: Gcx<'_>, inst: &Instruction) -> usize {
    immediate(inst).map_or(1, |value| super::compact_pushes::selected_len(gcx, value))
}

fn immediate(inst: &Instruction) -> Option<U256> {
    if !inst.is_encoded_push() || inst.deferred_push().is_some() || inst.immutable_push().is_some()
    {
        return None;
    }
    match inst.value {
        Some(PushValue::Immediate(value)) => Some(value),
        _ => None,
    }
}
