//! Replace consecutive constant memory stores with program-data copies.

use super::EvmPass;
use crate::backend::evm::{
    ir::{BlockId, Instruction, Module, PushValue},
    op,
};
use alloy_primitives::{Bytes, U256};
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
        let data = module.intern_data(rewrite.data);
        prepared.push((rewrite.block, rewrite.start, rewrite.end, size, data));
    }
    for (block, start, end, size, data) in prepared.into_iter().rev() {
        module.blocks[block].instructions.splice(
            start..end,
            [
                Instruction::push_value(U256::from(size)),
                Instruction::push_data(data),
                Instruction::opcode(op::DUP3),
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
    if raw_opcode(dup) != Some(op::DUP2) || raw_opcode(store) != Some(op::MSTORE) {
        return None;
    }

    let mut data = Vec::from(first.to_be_bytes::<32>());
    let mut end = start + 3;
    let mut words = 1usize;
    while let Some(window) = instructions.get(end..end + 6) {
        let [offset, dup, add, value, swap, store] = window else { unreachable!() };
        if immediate(offset) != Some(U256::from(words * 32))
            || raw_opcode(dup) != Some(op::DUP2)
            || raw_opcode(add) != Some(op::ADD)
            || raw_opcode(swap) != Some(op::SWAP1)
            || raw_opcode(store) != Some(op::MSTORE)
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
    let new_size = data.len() + push_len(gcx, U256::from(data.len())) + 4 + 2;
    (new_size < old_size).then(|| Rewrite { block, start, end, data: data.into() })
}

fn encoded_len(gcx: Gcx<'_>, inst: &Instruction) -> usize {
    immediate(inst).map_or(1, |value| super::compact_pushes::selected_len(gcx, value))
}

fn push_len(gcx: Gcx<'_>, value: U256) -> usize {
    let width = value.byte_len();
    if width == 0 && !gcx.sess.opts.evm_version.has_push0() { 2 } else { width + 1 }
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

fn raw_opcode(inst: &Instruction) -> Option<u8> {
    (!inst.is_encoded_push()).then_some(inst.opcode)
}
