//! Coalesce adjacent constant word copies into `MCOPY`.

use super::{EvmPass, utils::StackDepths};
use crate::backend::evm::{
    ir::{Instruction, Module},
    op,
};
use alloy_primitives::U256;
use solar_config::OptimizationMode;
use solar_sema::Gcx;

pub(super) struct CoalesceCopies;

impl EvmPass for CoalesceCopies {
    fn name(&self) -> &'static str {
        "coalesce-copies"
    }

    fn is_enabled(&self, gcx: Gcx<'_>, _module: &Module) -> bool {
        gcx.sess.opts.evm_version.has_mcopy()
            && !matches!(gcx.sess.opts.optimization, OptimizationMode::None)
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        coalesce_copies(gcx, module)
    }
}

const WORD_BYTES: usize = 32;
const COPY_INSTRUCTIONS: usize = 4;

fn coalesce_copies(gcx: Gcx<'_>, module: &mut Module) -> bool {
    if !module.blocks.iter().any(|block| has_candidate(gcx, &block.instructions)) {
        return false;
    }
    let Some(depths) = StackDepths::new(module) else { return false };
    let mut groups = 0usize;
    let mut words = 0usize;
    for block_index in 0..module.blocks.len() {
        let block_id = crate::backend::evm::ir::BlockId::from_usize(block_index);
        let block = &mut module.blocks[block_id];
        let mut edits = Vec::new();
        let mut index = 0;
        while index + COPY_INSTRUCTIONS <= block.instructions.len() {
            let Some((source, destination)) = word_copy(&block.instructions[index..]) else {
                index += 1;
                continue;
            };

            let mut count = 1usize;
            while let Some(next) = index.checked_add(count * COPY_INSTRUCTIONS)
                && let Some((next_source, next_destination)) =
                    block.instructions.get(next..).and_then(word_copy)
                && let Some(expected_source) = source.checked_add(U256::from(count * WORD_BYTES))
                && let Some(expected_destination) =
                    destination.checked_add(U256::from(count * WORD_BYTES))
                && next_source == expected_source
                && next_destination == expected_destination
            {
                count += 1;
            }
            if count < 2
                || !ranges_disjoint(source, destination, count)
                || !depths.has_headroom(block_id, index, 3)
            {
                index += COPY_INSTRUCTIONS;
                continue;
            }

            let length = U256::from(count * WORD_BYTES);
            if !profitable(gcx, source, destination, length, count) {
                index += COPY_INSTRUCTIONS;
                continue;
            }

            let replacement = [
                Instruction::push_value(length),
                Instruction::push_value(source),
                Instruction::push_value(destination),
                Instruction::opcode(op::MCOPY),
            ];
            edits.push((index, count * COPY_INSTRUCTIONS, replacement));
            groups += 1;
            words += count;
            index += count * COPY_INSTRUCTIONS;
        }
        for (start, len, replacement) in edits.into_iter().rev() {
            block.instructions.splice(start..start + len, replacement);
        }
    }

    if groups != 0 {
        tracing::debug!(
            target: "solar::codegen::evm_ir::coalesce_copies",
            groups,
            words,
            "coalesced constant memory copies"
        );
    }
    groups != 0
}

fn has_candidate(gcx: Gcx<'_>, instructions: &[Instruction]) -> bool {
    let mut index = 0;
    while index + COPY_INSTRUCTIONS <= instructions.len() {
        let Some((source, destination)) = word_copy(&instructions[index..]) else {
            index += 1;
            continue;
        };
        let mut count = 1usize;
        while let Some(next) = index.checked_add(count * COPY_INSTRUCTIONS)
            && let Some((next_source, next_destination)) =
                instructions.get(next..).and_then(word_copy)
            && let Some(expected_source) = source.checked_add(U256::from(count * WORD_BYTES))
            && let Some(expected_destination) =
                destination.checked_add(U256::from(count * WORD_BYTES))
            && next_source == expected_source
            && next_destination == expected_destination
        {
            count += 1;
        }
        if count >= 2
            && ranges_disjoint(source, destination, count)
            && profitable(gcx, source, destination, U256::from(count * WORD_BYTES), count)
        {
            return true;
        }
        index += COPY_INSTRUCTIONS;
    }
    false
}

fn word_copy(instructions: &[Instruction]) -> Option<(U256, U256)> {
    let [source, load, destination, store, ..] = instructions else { return None };
    if load.opcode != op::MLOAD || store.opcode != op::MSTORE {
        return None;
    }
    Some((immediate(source)?, immediate(destination)?))
}

fn immediate(inst: &Instruction) -> Option<U256> {
    if inst.deferred_push().is_some() || inst.immutable_push().is_some() {
        return None;
    }
    inst.pushed_value()
}

fn ranges_disjoint(source: U256, destination: U256, words: usize) -> bool {
    let length = U256::from(words * WORD_BYTES);
    let Some(source_end) = source.checked_add(length) else { return false };
    let Some(destination_end) = destination.checked_add(length) else { return false };
    source_end <= destination || destination_end <= source
}

fn profitable(gcx: Gcx<'_>, source: U256, destination: U256, length: U256, words: usize) -> bool {
    let evm_version = gcx.sess.opts.evm_version;
    let mut old_size = words * 2;
    let mut old_gas = words * 6;
    for index in 0..words {
        let offset = U256::from(index * WORD_BYTES);
        for value in [source + offset, destination + offset] {
            let (size, gas) =
                super::compact_pushes::immediate_materialization_cost(evm_version, value);
            old_size += size;
            old_gas += gas;
        }
    }

    let mut new_size = 1;
    let mut new_gas = 3 + 3 * words;
    for value in [length, source, destination] {
        let (size, gas) = super::compact_pushes::immediate_materialization_cost(evm_version, value);
        new_size += size;
        new_gas += gas;
    }
    new_size < old_size && new_gas <= old_gas
}
