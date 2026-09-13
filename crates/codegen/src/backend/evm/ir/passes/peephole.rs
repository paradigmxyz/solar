//! Local peephole optimization over scheduled EVM IR.
//!
//! The rewrite rules are written in ISLE in `isle/peephole.isle`; this module
//! drives them over each block and applies the edits they return. Matching uses
//! the same ordered rules on each successive prefix, retrying its tail after
//! every edit so newly adjacent operations can simplify immediately.
//!
//! Prefixes are inspected in place until the first rewrite. Only then is the
//! unvisited suffix moved to a scratch buffer for streaming cleanup. Unchanged
//! blocks require no instruction copies, which matters when later pipeline
//! passes expose few new opportunities. Rules never cross a block boundary;
//! target legality, push removability, and symbolic stack bounds stay in the
//! extractors, and edits preserve their existing metadata policy.
//! Literal unary expressions use the same evaluator as MIR and require a Pareto
//! improvement under the target's immediate materialization costs. A known-false
//! inline conditional jump then disappears with its two pushes. These rules
//! preserve custom stack effects and protected instruction boundaries, and never
//! treat symbolic label addresses or deferred values as literal constants.
//!
//! The separate `late-word` entry point runs only after structural cleanup. It
//! replaces a low-mask construction with a shorter complement/shift form. A closed
//! count window cannot read the surrounding stack; a protected-base window may
//! shuffle its base but cannot duplicate, consume, or inspect it. Neither window
//! can cross a terminator or observe gas/PC. Their instructions stay in place.
//! A target cost check requires a Pareto improvement, including the materialized
//! constants. Deferring this rewrite preserves earlier outlining opportunities;
//! doing it in MIR can turn a shareable run into two smaller inline copies that
//! occupy more bytes overall. Matching is bounded to 24 instructions per tail.

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
        optimize_module::<false>(gcx, module)
    }
}

/// Rewrites word windows after structural sharing and stack scheduling are fixed.
pub(super) struct LateWord;

impl EvmPass for LateWord {
    fn name(&self) -> &'static str {
        "late-word"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        optimize_module::<true>(gcx, module)
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

fn optimize_module<const LATE: bool>(gcx: Gcx<'_>, module: &mut Module) -> bool {
    let evm_version = gcx.sess.opts.evm_version;
    let mut changed = false;
    let mut scratch = Vec::new();
    for block in &mut module.blocks {
        // Dead stack traffic before a terminator that cannot observe it is dead-code
        // elimination's to remove; this pass only rewrites what it can see locally.
        let rewrites =
            optimize::<LATE>(evm_version, &mut block.instructions, &mut scratch, block.label);
        changed |= rewrites != 0;
    }
    changed
}

fn optimize<const LATE: bool>(
    evm_version: EvmVersion,
    instructions: &mut Vec<Instruction>,
    scratch: &mut Vec<Instruction>,
    block: u32,
) -> usize {
    // Inspect the original prefix without copying instructions. Until the first
    // rewrite, this is exactly the optimized prefix the streaming matcher sees.
    let first = (1..=instructions.len()).find_map(|end| {
        isle::PeepContext::new(&instructions[..end], evm_version)
            .select::<LATE>()
            .map(|rewrite| (end, rewrite))
    });
    let Some((end, isle::Rewrite { skip, edit })) = first else { return 0 };

    // unchanged prefix; matched suffix => unchanged prefix; replacement
    // Resume the streaming matcher at the first changed tail, including cascades.
    scratch.clear();
    scratch.extend(instructions.drain(end..));
    rewrite(evm_version, instructions, usize::from(skip), edit, block);
    let mut rewrites = 1;
    while try_peephole::<LATE>(evm_version, instructions, block) {
        rewrites += 1;
    }
    for inst in scratch.drain(..) {
        instructions.push(inst);
        while try_peephole::<LATE>(evm_version, instructions, block) {
            rewrites += 1;
        }
    }
    rewrites
}

fn try_peephole<const LATE: bool>(
    evm_version: EvmVersion,
    instructions: &mut Vec<Instruction>,
    block: u32,
) -> bool {
    let Some(isle::Rewrite { skip, edit }) =
        isle::PeepContext::new(instructions, evm_version).select::<LATE>()
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
    /// PUSH1 1; DUP1; closed_count; SHL; SUB => PUSH0; NOT; closed_count; SHL; NOT.
    LowMask,
    /// Replace a shuffled shift base and the trailing PUSH1 1; SWAP1; SUB.
    LowMaskWithSwap,
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
            Self::LowMaskWithSwap => {
                // push 0; not; protected_count; shl; not
                // NOTE: Changed constants and arithmetic have no exact source checkpoint.
                instructions.truncate(instructions.len() - 3);
                instructions[start] = Instruction::push_value(U256::ZERO).with_debug_info_dropped();
                instructions
                    .insert(start + 1, Instruction::opcode(op::NOT).with_debug_info_dropped());
                instructions.push(Instruction::opcode(op::NOT).with_debug_info_dropped());
            }
            Self::LowMask => {
                // push 0; not; closed_count; shl; not
                // NOTE: The changed constants and operations have different meanings;
                // their original source checkpoints cannot describe the replacement.
                instructions[start] = Instruction::push_value(U256::ZERO).with_debug_info_dropped();
                instructions[start + 1] = Instruction::opcode(op::NOT).with_debug_info_dropped();
                *instructions.last_mut().expect("matched subtraction") =
                    Instruction::opcode(op::NOT).with_debug_info_dropped();
            }
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
