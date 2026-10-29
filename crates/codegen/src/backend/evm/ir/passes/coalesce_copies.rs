//! Coalesce adjacent constant word copies into `MCOPY`.
//!
//! The pass recognizes consecutive `MLOAD`/`MSTORE` pairs with constant, contiguous source and
//! destination addresses. It replaces a non-overlapping run with one `MCOPY` when the target
//! supports it and the replacement is smaller without using more static gas. Overlapping ranges
//! remain unchanged because repeated word copies have memmove-like ordering that `MCOPY` does not
//! necessarily preserve.

use super::EvmPass;
use crate::{
    backend::evm::{
        ir::{Instruction, Module, ModuleTarget},
        op::{self, WORD_BYTES},
    },
    target::Target,
};
use alloy_primitives::U256;
use solar_config::OptimizationMode;
use solar_sema::Gcx;

pub(super) struct CoalesceCopies;

impl EvmPass for CoalesceCopies {
    fn name(&self) -> &'static str {
        "coalesce-copies"
    }

    fn is_enabled(&self, gcx: Gcx<'_>, module: &Module) -> bool {
        gcx.sess.opts.evm_version.has_mcopy()
            && !matches!(module.optimization, OptimizationMode::None)
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        coalesce_copies(gcx, module)
    }
}

const COPY_INSTRUCTIONS: usize = 4;

fn coalesce_copies(gcx: Gcx<'_>, module: &mut Module) -> bool {
    let target = module.runtime_target(gcx);
    let mut groups = 0usize;
    let mut words = 0usize;
    let mut edits = Vec::new();
    for block in &mut module.blocks {
        edits.clear();
        let mut index = 0;
        while index + COPY_INSTRUCTIONS <= block.instructions.len() {
            let Some((source, destination)) = word_copy(&block.instructions, index) else {
                index += 1;
                continue;
            };

            let count = 1
                + (1..)
                    .take_while(|&next| {
                        let offset = U256::from(next * WORD_BYTES);
                        source.checked_add(offset).zip(destination.checked_add(offset)).is_some_and(
                            |expected| {
                                word_copy(&block.instructions, index + next * COPY_INSTRUCTIONS)
                                    == Some(expected)
                            },
                        )
                    })
                    .count();
            let length = U256::from(count * WORD_BYTES);
            if count < 2
                || !ranges_disjoint(source, destination, length)
                || !profitable(target, source, destination, length, count)
            {
                index += COPY_INSTRUCTIONS;
                continue;
            }

            let metadata = &block.instructions[index].metadata;
            let replacement = [
                Instruction::push_value(length),
                Instruction::push_value(source),
                Instruction::push_value(destination),
                Instruction::opcode(op::MCOPY),
            ]
            .map(|inst| inst.with_source_debug(metadata));
            edits.push((index, count * COPY_INSTRUCTIONS, replacement));
            groups += 1;
            words += count;
            index += count * COPY_INSTRUCTIONS;
        }
        for (start, len, replacement) in edits.drain(..).rev() {
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

fn word_copy(instructions: &[Instruction], at: usize) -> Option<(U256, U256)> {
    let [source, load, destination, store, ..] = instructions.get(at..)? else { return None };
    if load.opcode != op::MLOAD || store.opcode != op::MSTORE {
        return None;
    }
    Some((source.concrete_immediate()?, destination.concrete_immediate()?))
}

fn ranges_disjoint(source: U256, destination: U256, length: U256) -> bool {
    source.checked_add(length).zip(destination.checked_add(length)).is_some_and(
        |(source_end, destination_end)| source_end <= destination || destination_end <= source,
    )
}

fn profitable(target: Target, source: U256, destination: U256, length: U256, words: usize) -> bool {
    // push source; mload; push destination; mstore, for each word
    let mut old = (target.opcode(op::MLOAD) + target.opcode(op::MSTORE)).times(words as u32);
    for index in 0..words {
        let offset = U256::from(index * WORD_BYTES);
        old += target.push(source + offset) + target.push(destination + offset);
    }
    // push length; push source; push destination; mcopy
    let mut new = target.opcode(op::MCOPY);
    new.gas += target.copy_word_gas() * words as u32;
    for value in [length, source, destination] {
        new += target.push(value);
    }
    new.bytes < old.bytes && target.runtime_gas(new) <= target.runtime_gas(old)
}
