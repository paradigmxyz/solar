//! Local peephole optimization over scheduled EVM IR.
//!
//! The rewrite rules are written in ISLE in `isle/evm-ir/peephole.isle`; this module
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
//! Comparison inversion tracks a constant through up to 24 instructions that cannot observe
//! or copy it. Adjusting that bound and flipping LT/GT or SLT/SGT removes ISZERO after branch
//! layout chooses the taken edge. Operand computations stay in place; wrapping bounds,
//! protected boundaries, custom stack effects, and materializations that grow in size or stack
//! peak are rejected.
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
    utils::MachineInstKey,
};
use crate::backend::evm::{
    ir::{BlockId, Instruction, Module, PushValue, TerminatorKind},
    op,
};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_data_structures::{index::IndexVec, map::FxHasher};
use solar_sema::Gcx;
use std::{
    fmt,
    hash::{Hash, Hasher},
};
use tracing::trace;

mod isle;

pub(super) use isle::invert_comparison;

pub(super) struct Peephole {
    final_cleanup: bool,
}

impl Peephole {
    pub(super) const EARLY: Self = Self { final_cleanup: false };
    pub(super) const FINAL: Self = Self { final_cleanup: true };
}

impl EvmPass for Peephole {
    fn name(&self) -> &'static str {
        "peephole"
    }

    fn cache_config(&self) -> u64 {
        u64::from(self.final_cleanup)
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        optimize_module::<false>(gcx, module, self.final_cleanup)
    }
}

/// Rewrites word windows after structural sharing and stack scheduling are fixed.
pub(super) struct LateWord;

impl EvmPass for LateWord {
    fn name(&self) -> &'static str {
        "late-word"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        optimize_module::<true>(gcx, module, false)
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

    fn cache_config(&self) -> u64 {
        self.0.cache_config()
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let changed = self.0.run_pass(gcx, module);
        if changed {
            let _ = Peephole::EARLY.run_pass(gcx, module);
        }
        changed
    }
}

const TRACE_TARGET: &str = "solar::codegen::evm_ir::peephole";

/// Block contents on which a peephole run found no rewrite, so the same contents can be skipped.
///
/// Matching reads only each instruction's opcode, encoding, value, stack operation, stack effect,
/// and `keep_with_next` flag, never debug metadata, so equal keys produce the same result. The
/// final rules extend the early ones, so contents clean under them are clean under both.
/// Records hold a hash of the keys rather than a copy, which would retain every clean block's
/// contents a second time for the module's lifetime.
/// Module clones start without the cache, and it never affects module equality.
#[derive(Default)]
pub(crate) struct CleanBlocks(IndexVec<BlockId, Option<CleanBlock>>);

struct CleanBlock {
    final_cleanup: bool,
    len: u32,
    hash: u64,
}

fn clean_hash(instructions: &[Instruction]) -> u64 {
    let mut hasher = FxHasher::default();
    for inst in instructions {
        (MachineInstKey::new(inst), inst.metadata.stack).hash(&mut hasher);
    }
    hasher.finish()
}

impl CleanBlocks {
    /// Returns whether the block was recorded clean with exactly these contents, and if so,
    /// whether the final rules were included.
    fn recorded(&self, block: BlockId, instructions: &[Instruction]) -> Option<bool> {
        let clean = self.0.get(block)?.as_ref()?;
        (clean.len as usize == instructions.len() && clean.hash == clean_hash(instructions))
            .then_some(clean.final_cleanup)
    }
}

impl Clone for CleanBlocks {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl PartialEq for CleanBlocks {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for CleanBlocks {}

impl fmt::Debug for CleanBlocks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CleanBlocks")
    }
}

fn optimize_module<const LATE: bool>(
    gcx: Gcx<'_>,
    module: &mut Module,
    final_cleanup: bool,
) -> bool {
    let evm_version = gcx.sess.opts.evm_version;
    let mut changed = false;
    let mut scratch = Vec::new();
    let mut clean = std::mem::take(&mut module.peephole_clean);
    clean.0.resize_with(module.blocks.len(), || None);
    for (block_id, block) in module.blocks.iter_mut_enumerated() {
        // The late rules are separate from the cached early and final ones.
        let recorded = if LATE { None } else { clean.recorded(block_id, &block.instructions) };
        let skip = recorded.is_some_and(|recorded_final| recorded_final || !final_cleanup);
        let early_clean = final_cleanup && recorded == Some(false);
        // Dead stack traffic before a terminator that cannot observe it is dead-code
        // elimination's to remove; this pass only rewrites what it can see locally.
        let rewrites = if skip {
            0
        } else {
            optimize::<LATE>(
                evm_version,
                &mut block.instructions,
                &mut scratch,
                block.label,
                final_cleanup,
                early_clean,
            )
        };
        changed |= rewrites != 0;
        let mut returned_zero = false;
        // mstore(offset, value); return(offset, 32)
        // -> mstore(0, value); return(0, 32)
        if final_cleanup
            && matches!(
                block.terminator.as_ref().map(|term| &term.kind),
                Some(TerminatorKind::Op(op::RETURN))
            )
            && let [prefix @ .., offset, store, size, returned] = block.instructions.as_mut_slice()
            && store.as_evm_opcode() == Some(op::MSTORE)
            && size.concrete_immediate() == Some(U256::from(32))
            && let Some(address) = offset.concrete_immediate()
            && !address.is_zero()
            && returned.concrete_immediate() == Some(address)
            && prefix
                .windows(2)
                .rev()
                .take_while(|pair| pair[1].as_evm_opcode() != Some(op::JUMPDEST))
                .any(|pair| {
                    pair[1].as_evm_opcode() == Some(op::MSTORE)
                        && pair[0].concrete_immediate().is_some_and(|previous| previous >= address)
                })
        {
            offset.replace_preserving_metadata(Instruction::push_value(U256::ZERO));
            returned.replace_preserving_metadata(Instruction::push_value(U256::ZERO));
            changed = true;
            returned_zero = true;
        }
        if !LATE && !skip {
            if rewrites != 0 || returned_zero {
                clean.0[block_id] = None;
            } else if recorded.is_some() {
                // The same contents are now clean under the final rules as well.
                clean.0[block_id].as_mut().unwrap().final_cleanup |= final_cleanup;
            } else {
                clean.0[block_id] = Some(CleanBlock {
                    final_cleanup,
                    len: block.instructions.len() as u32,
                    hash: clean_hash(&block.instructions),
                });
            }
        }
    }
    module.peephole_clean = clean;
    changed
}

fn optimize<const LATE: bool>(
    evm_version: EvmVersion,
    instructions: &mut Vec<Instruction>,
    scratch: &mut Vec<Instruction>,
    block: u32,
    final_cleanup: bool,
    early_clean: bool,
) -> usize {
    // Inspect the original prefix without copying instructions. Until the first
    // rewrite, this is exactly the optimized prefix the streaming matcher sees.
    // The early rules match no prefix of an early-clean block, so only the final
    // rules can supply its first rewrite.
    let first = (1..=instructions.len()).find_map(|end| {
        let mut context = isle::PeepContext::new(&instructions[..end], evm_version)
            .with_final_cleanup(final_cleanup);
        if early_clean { context.select_final() } else { context.select::<LATE>() }
            .map(|rewrite| (end, rewrite))
    });
    let Some((end, isle::Rewrite { skip, edit })) = first else { return 0 };

    // unchanged prefix; matched suffix => unchanged prefix; replacement
    // Resume the streaming matcher at the first changed tail, including cascades.
    scratch.clear();
    scratch.extend(instructions.drain(end..));
    rewrite(evm_version, instructions, usize::from(skip), edit, block);
    let mut rewrites = 1;
    while try_peephole::<LATE>(evm_version, instructions, block, final_cleanup) {
        rewrites += 1;
    }
    for inst in scratch.drain(..) {
        instructions.push(inst);
        while try_peephole::<LATE>(evm_version, instructions, block, final_cleanup) {
            rewrites += 1;
        }
    }
    rewrites
}

fn try_peephole<const LATE: bool>(
    evm_version: EvmVersion,
    instructions: &mut Vec<Instruction>,
    block: u32,
    final_cleanup: bool,
) -> bool {
    let Some(isle::Rewrite { skip, edit }) = isle::PeepContext::new(instructions, evm_version)
        .with_final_cleanup(final_cleanup)
        .select::<LATE>()
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
    InvertComparison {
        value: U256,
        opcode: u8,
    },
    ReloadStoredValue,
    ConsumeStoredValue {
        depth: u8,
    },
    DropNonzeroTest,
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
            Self::ConsumeStoredValue { depth } => {
                // SWAP(n-1); PUSH address; MSTORE
                overwrite_stack_op(&mut instructions[start], op::StackOp::Swap(depth));
                let (retained, removed) = instructions[start..].split_at_mut(3);
                for inst in removed {
                    retained[2].metadata.absorb_debug_info(&inst.metadata);
                }
                instructions.truncate(start + 3);
            }
            Self::InvertComparison { value, opcode } => {
                // PUSH c; independent operands; compare; ISZERO
                // => PUSH adjusted; independent operands; opposite compare
                instructions[start].replace_preserving_metadata(Instruction::push_value(value));
                let iszero = instructions.pop().expect("matched ISZERO");
                let comparison = instructions.last_mut().expect("matched comparison");
                overwrite_raw(comparison, opcode);
                comparison.metadata.absorb_debug_info(&iszero.metadata);
            }
            Self::ReloadStoredValue => {
                instructions.swap(start, start + 3);
                instructions.swap(start + 1, start + 2);
                overwrite_raw(&mut instructions[start], op::DUP1);
                instructions.truncate(start + 3);
            }
            Self::DropNonzeroTest => {
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
