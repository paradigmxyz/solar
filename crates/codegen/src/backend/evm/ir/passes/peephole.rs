//! Local peephole optimization over scheduled EVM IR.
//!
//! The rewrite rules are written in ISLE in `isle/peephole.isle`; this module
//! drives them over each block and applies the edits they return.

use super::{
    EvmPass,
    compact_pushes::{immediate_materialization_cost, materialize_immediate},
};
use crate::backend::evm::{
    ir::{Instruction, Module, PushValue},
    op,
};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_sema::Gcx;
use std::fmt;
use tracing::trace;

mod isle;

pub(super) struct Peephole;

impl EvmPass for Peephole {
    fn name(&self) -> &'static str {
        "peephole"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        optimize_module(gcx, module)
    }
}

/// Runs peephole cleanup only when the wrapped pass changes the module.
pub(super) struct Cleanup<T>(pub(super) T);

impl<T: EvmPass> EvmPass for Cleanup<T> {
    fn name(&self) -> &'static str {
        self.0.name()
    }

    fn is_enabled(&self, gcx: Gcx<'_>, module: &Module) -> bool {
        self.0.is_enabled(gcx, module)
    }

    fn is_required(&self) -> bool {
        self.0.is_required()
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let changed = self.0.run_pass(gcx, module);
        if changed {
            let _ = Peephole.run_pass(gcx, module);
        }
        changed
    }
}

const TRACE_TARGET: &str = "solar::codegen::evm_ir::peephole";

fn optimize_module(gcx: Gcx<'_>, module: &mut Module) -> bool {
    let evm_version = gcx.sess.opts.evm_version;
    let mut changed = false;
    let mut scratch = Vec::new();
    for block in &mut module.blocks {
        // Dead stack traffic before a terminator that cannot observe it is dead-code
        // elimination's to remove; this pass only rewrites what it can see locally.
        let rewrites = optimize(evm_version, &mut block.instructions, &mut scratch, block.label);
        changed |= rewrites != 0;
    }
    changed
}

fn optimize(
    evm_version: EvmVersion,
    instructions: &mut Vec<Instruction>,
    scratch: &mut Vec<Instruction>,
    block: u32,
) -> usize {
    scratch.clear();
    std::mem::swap(instructions, scratch);
    instructions.reserve(scratch.len());
    let mut rewrites = 0;
    for inst in scratch.drain(..) {
        instructions.push(inst);
        while try_peephole(evm_version, instructions, block) {
            rewrites += 1;
        }
    }
    rewrites
}

fn try_peephole(evm_version: EvmVersion, instructions: &mut Vec<Instruction>, block: u32) -> bool {
    let Some(isle::Rewrite { skip, edit }) =
        isle::PeepContext::new(instructions, evm_version).peep()
    else {
        return false;
    };
    rewrite(evm_version, instructions, usize::from(skip), edit, block)
}

// Keep trace formatting out of the hot matcher's stack frame.
#[inline(never)]
fn rewrite(
    evm_version: EvmVersion,
    instructions: &mut Vec<Instruction>,
    skip: usize,
    edit: Edit,
    block: u32,
) -> bool {
    let start = instructions.len() - skip;
    let input = tracing::enabled!(target: TRACE_TARGET, tracing::Level::TRACE)
        .then(|| instructions[start..].to_vec());
    edit.apply(evm_version, instructions, start);
    if let Some(input) = input {
        trace!(
            target: TRACE_TARGET,
            block,
            input = %format_args!("\"{}\"", InstructionSequence(&input)),
            output = %format_args!("\"{}\"", InstructionSequence(&instructions[start..])),
            "rewrite"
        );
    }
    true
}

/// An edit of the last instructions of a block, applied from `start`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edit {
    /// Keep the first `len` instructions of the window.
    Keep {
        len: u8,
    },
    /// Replace a duplicate of a known zero with `PUSH0`.
    OverwritePush0,
    RemoveFirstKeepOne,
    RemoveFirstKeepTwo,
    RemoveFirstOverwrite {
        opcode: u8,
    },
    SwapOverwrite {
        opcode: u8,
    },
    OverwriteOne {
        opcode: u8,
    },
    OverwriteTwo {
        opcode: u8,
    },
    /// Merge a swap-and-pop chain into one swap of `depth` and its pops.
    MergeSwapPop {
        depth: u8,
    },
    /// Drop a swap whose permuted words are all discarded by the pops that follow.
    DropDiscardedSwap,
    /// Replace an evaluated constant expression with its materialized result.
    FoldConstants {
        value: U256,
    },
    ReloadStoredValue,
    DropDoubleIszero,
    EqIszeroJumpi,
    StackOp {
        op: op::StackOp,
    },
    StackOps {
        first: op::StackOp,
        second: op::StackOp,
    },
}

impl Edit {
    fn apply(self, evm_version: EvmVersion, instructions: &mut Vec<Instruction>, start: usize) {
        match self {
            Self::Keep { len } => instructions.truncate(start + usize::from(len)),
            Self::OverwritePush0 => {
                let metadata = std::mem::take(&mut instructions[start].metadata);
                instructions[start] = Instruction::push_value(U256::ZERO);
                instructions[start].metadata = metadata;
            }
            Self::DropDiscardedSwap => {
                instructions.remove(start);
            }
            Self::FoldConstants { value } => {
                instructions.truncate(start);
                materialize_immediate(instructions, evm_version, value);
            }
            Self::RemoveFirstKeepOne => {
                instructions.remove(start);
                instructions.truncate(start + 1);
            }
            Self::RemoveFirstKeepTwo => {
                instructions.remove(start);
                instructions.truncate(start + 2);
            }
            Self::RemoveFirstOverwrite { opcode } => {
                instructions.remove(start);
                overwrite_raw(&mut instructions[start], opcode);
            }
            Self::SwapOverwrite { opcode } => {
                instructions.swap(start, start + 1);
                overwrite_raw(&mut instructions[start], opcode);
            }
            Self::OverwriteOne { opcode } => {
                overwrite_raw(&mut instructions[start], opcode);
                instructions.truncate(start + 1);
            }
            Self::OverwriteTwo { opcode } => {
                overwrite_raw(&mut instructions[start], op::SWAP1);
                overwrite_raw(&mut instructions[start + 1], opcode);
                instructions.truncate(start + 2);
            }
            Self::MergeSwapPop { depth } => {
                let end = instructions.len();
                overwrite_stack_op(&mut instructions[start], op::StackOp::Swap(depth));
                overwrite_raw(&mut instructions[end - 2], op::POP);
                instructions.truncate(end - 1);
            }
            Self::ReloadStoredValue => {
                instructions.swap(start, start + 3);
                instructions.swap(start + 1, start + 2);
                overwrite_raw(&mut instructions[start], op::DUP1);
                instructions.truncate(start + 3);
            }
            Self::DropDoubleIszero => {
                instructions.drain(start..start + 2);
                overwrite_raw(&mut instructions[start + 1], op::JUMPI);
            }
            Self::EqIszeroJumpi => {
                overwrite_raw(&mut instructions[start], op::SUB);
                instructions.remove(start + 1);
                overwrite_raw(&mut instructions[start + 2], op::JUMPI);
            }
            Self::StackOp { op: stack_op } => {
                instructions[start] = Instruction::stack_op(stack_op).with_debug_info_dropped();
                instructions.truncate(start + 1);
            }
            Self::StackOps { first, second } => {
                instructions[start] = Instruction::stack_op(first).with_debug_info_dropped();
                instructions[start + 1] = Instruction::stack_op(second).with_debug_info_dropped();
                instructions.truncate(start + 2);
            }
        }
    }
}

fn overwrite_raw(inst: &mut Instruction, opcode: u8) {
    debug_assert!(raw_opcode(inst).is_some());
    let metadata = std::mem::take(&mut inst.metadata);
    *inst = Instruction::opcode(opcode);
    inst.metadata = metadata;
    inst.metadata.stack = None;
}

fn overwrite_stack_op(inst: &mut Instruction, stack_op: op::StackOp) {
    let metadata = std::mem::take(&mut inst.metadata);
    *inst = Instruction::stack_op(stack_op);
    inst.metadata = metadata;
    inst.metadata.stack = None;
}

/// Returns the byte length and static gas of the selected materialization of `value`.
pub(super) fn materialization_cost(evm_version: EvmVersion, value: U256) -> (usize, usize) {
    immediate_materialization_cost(evm_version, value)
}

fn raw_opcode(inst: &Instruction) -> Option<u8> {
    inst.as_evm_opcode()
}

pub(super) fn push_value(inst: &Instruction) -> Option<U256> {
    if !inst.is_encoded_push() || inst.deferred_push().is_some() || inst.immutable_push().is_some()
    {
        return None;
    }
    match &inst.value {
        Some(PushValue::Immediate(value)) => Some(*value),
        _ => None,
    }
}

fn is_block_push(inst: &Instruction) -> bool {
    inst.is_encoded_push() && matches!(inst.value, Some(PushValue::Block(_)))
}

fn is_removable_push(inst: &Instruction) -> bool {
    inst.is_encoded_push() && inst.deferred_push().is_none()
}

struct InstructionSequence<'a>(&'a [Instruction]);

impl fmt::Display for InstructionSequence<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, inst) in self.0.iter().enumerate() {
            if index != 0 {
                f.write_str(" ")?;
            }
            if inst.deferred_push().is_some() {
                f.write_str("push_deferred")?;
            } else if inst.immutable_push().is_some() {
                f.write_str("push_immutable")?;
            } else if let Some(value) = push_value(inst) {
                write!(f, "push {value:#x}")?;
            } else if inst.is_encoded_push() {
                f.write_str("push_ref")?;
            } else if let Some(mnemonic) = op::mnemonic(inst.opcode) {
                f.write_str(mnemonic)?;
            } else {
                write!(f, "0x{:02x}", inst.opcode)?;
            }
        }
        Ok(())
    }
}
