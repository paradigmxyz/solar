//! Late dead-value elimination over scheduled EVM IR.

use super::EvmPass;
use crate::backend::evm::{
    ir::{Instruction, Module, default_instruction_stack_effect},
    op,
};
use solar_sema::Gcx;
use std::ops::Range;

pub(super) struct Dce;

impl EvmPass for Dce {
    fn name(&self) -> &'static str {
        "dce"
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        eliminate_dead_saved_values(module)
    }
}

/// Removes stack copies that only preserve a value until it is discarded.
///
/// Starting after a `DUP1`, `above` tracks the number of words above the
/// original value. A candidate is safe while every instruction operates only
/// on those words. Once they have all been consumed, a `POP` discards the
/// original and both bookend instructions can be removed. A `SWAPn POP`
/// cleanup with up to three live results can likewise be replaced by fewer,
/// shallower swaps.
fn eliminate_dead_saved_values(module: &mut Module) -> bool {
    let mut changed = false;
    let mut removals = Vec::new();
    let mut scratch = Vec::new();
    for block in &mut module.blocks {
        changed |= eliminate_in_block(&mut block.instructions, &mut removals, &mut scratch) != 0;
    }
    changed
}

fn eliminate_in_block(
    instructions: &mut Vec<Instruction>,
    removals: &mut Vec<usize>,
    scratch: &mut Vec<Instruction>,
) -> usize {
    let mut rewrites = 0;
    loop {
        removals.clear();
        let mut start = 0;
        while start < instructions.len() {
            if raw_opcode(&instructions[start]) != Some(op::DUP1) {
                start += 1;
                continue;
            }
            let Some(cleanup) = find_cleanup(instructions, start + 1) else {
                start += 1;
                continue;
            };

            removals.push(start);
            match cleanup.live_results {
                0 => removals.push(cleanup.range.start),
                1 => removals.extend(cleanup.range.clone()),
                2 => {
                    overwrite_raw(&mut instructions[cleanup.range.start], op::SWAP1);
                    removals.push(cleanup.range.end - 1);
                }
                3 => {
                    overwrite_raw(&mut instructions[cleanup.range.start], op::SWAP2);
                    overwrite_raw(&mut instructions[cleanup.range.start + 1], op::SWAP1);
                }
                _ => unreachable!("cleanup search only returns profitable result counts"),
            }
            start = cleanup.range.end;
            rewrites += 1;
        }
        if removals.is_empty() {
            return rewrites;
        }

        scratch.clear();
        std::mem::swap(instructions, scratch);
        instructions.reserve(scratch.len() - removals.len());
        let mut removals = removals.iter().copied().peekable();
        for (index, inst) in scratch.drain(..).enumerate() {
            if removals.peek() == Some(&index) {
                removals.next();
            } else {
                instructions.push(inst);
            }
        }
    }
}

struct Cleanup {
    range: Range<usize>,
    live_results: usize,
}

fn find_cleanup(instructions: &[Instruction], mut index: usize) -> Option<Cleanup> {
    // The duplicate itself is initially the only word above the saved original.
    let mut above = 1usize;
    while let Some(inst) = instructions.get(index) {
        let opcode = raw_opcode(inst);
        match opcode {
            Some(op::POP) if above == 0 => {
                return Some(Cleanup { range: index..index + 1, live_results: 0 });
            }
            Some(op::POP) => above -= 1,
            Some(opcode)
                if (1..=3).contains(&above)
                    && opcode == op::swap(above as u8)
                    && instructions
                        .get(index + 1)
                        .is_some_and(|inst| raw_opcode(inst) == Some(op::POP)) =>
            {
                return Some(Cleanup { range: index..index + 2, live_results: above });
            }
            Some(opcode) if is_control_flow(opcode) => return None,
            Some(opcode) if (op::DUP1..=op::DUP16).contains(&opcode) => {
                let depth = usize::from(opcode - op::DUP1 + 1);
                if depth > above {
                    return None;
                }
                above += 1;
            }
            Some(opcode) if (op::SWAP1..=op::SWAP16).contains(&opcode) => {
                let depth = usize::from(opcode - op::SWAP1 + 1);
                if depth >= above {
                    return None;
                }
            }
            _ => {
                let effect =
                    inst.metadata.stack.or_else(|| default_instruction_stack_effect(inst))?;
                let inputs = usize::from(effect.inputs);
                if inputs > above {
                    return None;
                }
                above = above - inputs + usize::from(effect.outputs);
            }
        }
        index += 1;
    }
    None
}

fn raw_opcode(inst: &Instruction) -> Option<u8> {
    (!inst.is_encoded_push()).then_some(inst.opcode)
}

fn overwrite_raw(inst: &mut Instruction, opcode: u8) {
    debug_assert!(raw_opcode(inst).is_some());
    inst.opcode = opcode;
    inst.metadata.stack = None;
}

const fn is_control_flow(opcode: u8) -> bool {
    op::is_terminal(opcode)
        || matches!(
            opcode,
            op::JUMPI
                | op::RJUMP
                | op::RJUMPI
                | op::RJUMPV
                | op::CALLF
                | op::RETF
                | op::JUMPF
                | op::RETURNCONTRACT
        )
}
