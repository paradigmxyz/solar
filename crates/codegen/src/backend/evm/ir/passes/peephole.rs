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
//! protected boundaries, and materializations that grow in size or stack peak are rejected.
//! Literal unary expressions use the same evaluator as MIR and require a Pareto
//! improvement under the target's immediate materialization costs. A known-false
//! inline conditional jump then disappears with its two pushes. These rules
//! preserve protected instruction boundaries, and never treat symbolic label
//! addresses or immutable placeholders as literal constants.
//!
//! A pure binary operation with a constant operand moves through a `SWAPn SWAP1 SWAPn`
//! permutation, which removes two swaps.
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
//! A mask that compact pushes built as `PUSH0 NOT PUSH s SHR` before an `AND` becomes the
//! shift pair `PUSH s SHL PUSH s SHR` in a fixed five-instruction window, when the pair
//! dominates the mask in gas and bytes. A literal mask never matches, so a loop that kept one
//! keeps it; the MIR word-sequence pass only fuses such a mask with a neighbouring shift.
//!
//! When no low-mask window matches, the late rules rewrite a trailing `PUSH c; SWAP1; SUB` to
//! `materialize(-c); ADD` if the target's cheapest materialization of `-c` beats the push and the
//! swap, as `PUSH0; NOT` does for the decrement `x - 1`. The low-mask window ends in the same
//! tail, so this rewrite waits for it.
//!
//! `DUP1 ADD PUSH1 1 OR` computes `(x + x) | 1`, which is `x - ~x`: `DUP1 NOT SWAP1 SUB` has the
//! same gas and fewer bytes, and each of its instructions keeps the origin of the `OR`.
//!
//! Equivalent replacements keep the root operation's source context on their result-producing
//! instruction. Removed operand locations are dropped, and function events are moved once.

use super::{
    EvmPass, block_layout::terminator_kind, compact_pushes::materialize_immediate,
    utils::MachineInstKey,
};
use crate::{
    backend::evm::{
        ir::{Instruction, Metadata, Module, ModuleTarget, TerminatorKind},
        op,
    },
    target::Target,
};
use alloy_primitives::U256;
use solar_data_structures::map::FxHasher;
use solar_sema::Gcx;
use std::{
    fmt,
    hash::{Hash, Hasher},
};
use tracing::trace;

mod isle;

pub(super) use isle::invert_comparison;

/// The rule set a run applies. The final rules extend the early ones; the late rules are separate.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rules {
    Early,
    Final,
    Late,
}

pub(super) struct Peephole(Rules);

impl Peephole {
    pub(super) const EARLY: Self = Self(Rules::Early);
    pub(super) const FINAL: Self = Self(Rules::Final);
}

impl EvmPass for Peephole {
    fn name(&self) -> &'static str {
        if self.0 == Rules::Early { "peephole-early" } else { "peephole" }
    }

    fn cache_config(&self) -> u64 {
        u64::from(self.0 == Rules::Final)
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        optimize_module(gcx, module, self.0)
    }
}

/// Rewrites word windows after structural sharing and stack scheduling are fixed.
pub(super) struct LateWord;

impl EvmPass for LateWord {
    fn name(&self) -> &'static str {
        "late-word"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        optimize_module(gcx, module, Rules::Late)
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

/// Returns the length and hash of the keys of `instructions`, which key the module's records of
/// block contents on which a peephole run found no rewrite, so the same contents can be skipped.
///
/// Matching reads only each instruction's opcode, encoding, value, stack operation, and
/// `keep_with_next` flag, and no other metadata, so equal keys produce the same result. The final
/// rules extend the early ones, so contents clean under them are clean under both. Records are
/// keyed by contents rather than by block, so they survive block reordering and removal and cover
/// every block with equal contents.
///
/// NOTE: Records hold a hash of the keys rather than a copy, which would retain every clean
/// block's contents a second time for the module's lifetime. A collision between recorded and
/// current contents of equal length would skip a rewrite; the result stays deterministic.
fn clean_key(instructions: &[Instruction]) -> (u32, u64) {
    let mut hasher = FxHasher::default();
    for inst in instructions {
        MachineInstKey::new(inst).hash(&mut hasher);
    }
    (instructions.len() as u32, hasher.finish())
}

fn optimize_module(gcx: Gcx<'_>, module: &mut Module, rules: Rules) -> bool {
    let target = module.runtime_target(gcx);
    let final_cleanup = rules == Rules::Final;
    let mut changed = false;
    let mut scratch = Vec::new();
    let clean = &mut module.peephole_clean;
    for block in &mut module.blocks {
        // The late rules are separate from the cached early and final ones.
        let key = (rules != Rules::Late).then(|| clean_key(&block.instructions));
        let recorded = key.and_then(|key| clean.get(key));
        let skip = recorded.is_some_and(|recorded_final| recorded_final || !final_cleanup);
        let early_clean = final_cleanup && recorded == Some(false);
        // Dead stack traffic before a terminator that cannot observe it is dead-code
        // elimination's to remove; this pass only rewrites what it can see locally.
        let rewrites = if skip {
            0
        } else {
            Driver { target, rules, block: block.label }.run(
                &mut block.instructions,
                &mut scratch,
                early_clean,
            )
        };
        // The unchanged contents are clean under these rules, and under the final rules as well
        // when they were already clean under the early ones.
        if let Some(key) = key
            && !skip
            && rewrites == 0
        {
            clean.record(key, final_cleanup);
        }
        changed |= rewrites != 0;
        // mstore(offset, value); return(offset, 32)
        // -> mstore(0, value); return(0, 32)
        if final_cleanup
            && matches!(terminator_kind(block), Some(TerminatorKind::Op(op::RETURN)))
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
        }
    }
    changed
}

/// Applies the rules of one run to the blocks it visits.
#[derive(Clone, Copy)]
struct Driver {
    target: Target,
    rules: Rules,
    block: u32,
}

impl Driver {
    fn run(
        self,
        instructions: &mut Vec<Instruction>,
        scratch: &mut Vec<Instruction>,
        early_clean: bool,
    ) -> usize {
        // Inspect the original prefix without copying instructions. Until the first
        // rewrite, this is exactly the optimized prefix the streaming matcher sees.
        // The early rules match no prefix of an early-clean block, so only the final
        // rules can supply its first rewrite.
        let first = (1..=instructions.len()).find_map(|end| {
            let mut context = isle::PeepContext::new(&instructions[..end], self.target, self.rules);
            if early_clean { context.final_rewrite() } else { context.select() }
                .map(|rewrite| (end, rewrite))
        });
        let Some((end, first)) = first else { return 0 };

        // unchanged prefix; matched suffix => unchanged prefix; replacement
        // Resume the streaming matcher at the first changed tail, including cascades.
        scratch.clear();
        scratch.extend(instructions.drain(end..));
        self.rewrite(instructions, first);
        let mut rewrites = 1;
        while self.step(instructions) {
            rewrites += 1;
        }
        for inst in scratch.drain(..) {
            instructions.push(inst);
            while self.step(instructions) {
                rewrites += 1;
            }
        }
        rewrites
    }

    fn step(self, instructions: &mut Vec<Instruction>) -> bool {
        let Some(rewrite) = isle::PeepContext::new(instructions, self.target, self.rules).select()
        else {
            return false;
        };
        self.rewrite(instructions, rewrite);
        true
    }

    // Keep trace formatting out of the hot matcher's stack frame.
    #[inline(never)]
    fn rewrite(self, instructions: &mut Vec<Instruction>, rewrite: isle::Rewrite) {
        let start = instructions.len() - usize::from(rewrite.skip);
        let input = tracing::enabled!(target: TRACE_TARGET, tracing::Level::TRACE)
            .then(|| instructions[start..].to_vec());
        rewrite.edit.apply(self.target, instructions, start);
        if let Some(input) = input {
            trace!(
                target: TRACE_TARGET,
                block = self.block,
                input = %format_args!("\"{}\"", instruction_sequence(&input)),
                output = %format_args!("\"{}\"", instruction_sequence(&instructions[start..])),
                "rewrite"
            );
        }
    }
}

/// An edit of the last instructions of a block, applied from `start`.
#[derive(Clone, Copy)]
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
    /// PUSH0; NOT; PUSH s; SHR; AND => PUSH s; SHL; PUSH s; SHR.
    ShiftPairMask,
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
    /// Merge a swap-and-pop chain into one swap that is one deeper, and its pops.
    MergeSwapPop,
    /// Drop a swap whose permuted words are all discarded by the pops that follow.
    DropDiscardedSwap,
    /// Replace an evaluated constant expression with its materialized result.
    FoldConstants {
        value: U256,
    },
    /// PUSH c; SWAP1; SUB => materialize(-c); ADD.
    AddNegated {
        value: U256,
    },
    InvertComparison {
        value: U256,
        opcode: u8,
    },
    ZeroLessIszero,
    ReloadStoredValue,
    ConsumeStoredValue {
        depth: u8,
    },
    DropNonzeroTest,
    EqIszeroJumpi,
    SwapPair {
        first: u8,
        second: u8,
    },
    MoveConstantThroughSwaps,
    /// DUP1; NOT; SWAP1; SUB.
    OddWord,
}

impl Edit {
    fn apply(self, target: Target, instructions: &mut Vec<Instruction>, start: usize) {
        // Every instruction a replacement creates takes the source context of its root, the
        // instruction whose output the replacement produces, as InstCombine does:
        // `Builder.SetCurrentDebugLocation(I->getDebugLoc())`
        // (llvm/lib/Transforms/InstCombine/InstructionCombining.cpp#L5974-L5999 at llvm-project
        // e98b16a2710b). LLVM's guide gives an expansion the location of the replaced instruction,
        // `(add X X) => (shl X 1)`, and solc's peephole optimizer gives each new item the
        // location of the matched item it replaces (libevmasm/PeepholeOptimiser.cpp).
        let not = |root: &Metadata| Instruction::opcode(op::NOT).with_debug_loc(root);
        match self {
            Self::LowMaskWithSwap => {
                // push 0; not; protected_count; shl; not
                let root = instructions.last().unwrap().metadata.clone();
                let entry = instructions[start].metadata.clone();
                instructions.truncate(instructions.len() - 3);
                instructions[start] = Instruction::push_value(U256::ZERO).with_debug_loc(&root);
                instructions[start].metadata.move_function_events(&entry);
                instructions.insert(start + 1, not(&root));
                instructions.push(not(&root));
                instructions.last_mut().unwrap().metadata.move_function_events(&root);
            }
            Self::LowMask => {
                // push 0; not; closed_count; shl; not
                let root = instructions.last().unwrap().metadata.clone();
                let entry = instructions[start].metadata.clone();
                instructions[start] = Instruction::push_value(U256::ZERO).with_debug_loc(&root);
                instructions[start].metadata.move_function_events(&entry);
                instructions[start + 1] = not(&root);
                *instructions.last_mut().expect("matched subtraction") = not(&root);
                instructions.last_mut().unwrap().metadata.move_function_events(&root);
            }
            Self::ShiftPairMask => {
                // push s; shl; push s; shr
                // The root is the `and`, which the last shift replaces. The copied shift count and
                // the first shift take its source context.
                let root = instructions.last().unwrap().metadata.clone();
                instructions[start] = instructions[start + 2].clone().with_debug_loc(&root);
                overwrite(&mut instructions[start + 1], op::SHL);
                instructions[start + 1].metadata.preserve_debug_loc(&root);
                instructions.remove(start + 3);
                overwrite(&mut instructions[start + 3], op::SHR);
            }
            Self::Keep { len } => instructions.truncate(start + usize::from(len)),
            Self::OverwritePush0 => {
                instructions[start]
                    .replace_preserving_metadata(Instruction::push_value(U256::ZERO));
            }
            Self::DropDiscardedSwap => {
                instructions.remove(start);
            }
            Self::FoldConstants { value } => {
                // NOTE: The materialization computes the old root's value on the same path, so
                // every instruction of it takes the root's source context. Operand locations
                // disappear.
                let context =
                    replacement_debug_info(&instructions[start..], instructions.len() - start - 1);
                instructions.truncate(start);
                materialize_immediate(instructions, target.evm_version(), value, &context);
                let last = instructions.last_mut().unwrap();
                if let Some(exit) = context.function_exit() {
                    last.metadata.set_function_exit(exit);
                }
                if let Some(function) = context.function_invoke() {
                    instructions[start].metadata.set_function_invoke(function);
                }
            }
            Self::AddNegated { value } => {
                // push -c; add
                // NOTE: The materialized constant takes the source context of the subtraction, the
                // root. The `ADD` replaces the subtraction and keeps its context; the removed push
                // and swap take their origins with them, and only their function events move.
                let mut add = instructions.pop().expect("matched SUB");
                overwrite(&mut add, op::ADD);
                let root = add.metadata.clone();
                for removed in instructions.drain(start..) {
                    add.metadata.move_function_events(&removed.metadata);
                }
                materialize_immediate(instructions, target.evm_version(), value, &root);
                instructions.push(add);
            }
            Self::RemoveFirstKeepOne => {
                // push x; push 0; absorbing_op => push 0
                // push x; push 1; exp => push 1
                let context = (instructions.len() - start == 3)
                    .then(|| replacement_debug_info(&instructions[start..], 2));
                instructions.remove(start);
                instructions.truncate(start + 1);
                if let Some(context) = context {
                    // NOTE: The survivor keeps no event of its own: the window's agreed events
                    // replace them.
                    let metadata = &mut instructions[start].metadata;
                    metadata.drop_function_events();
                    metadata.preserve_debug_loc(&context);
                    metadata.move_function_events(&context);
                }
            }
            Self::RemoveFirstKeepTwo => {
                instructions.remove(start);
                instructions.truncate(start + 2);
            }
            Self::RemoveFirstOverwrite { opcode } => {
                instructions.remove(start);
                overwrite(&mut instructions[start], opcode);
            }
            Self::SwapOverwrite { opcode } => {
                instructions.swap(start, start + 1);
                overwrite(&mut instructions[start], opcode);
            }
            Self::OverwriteOne { opcode } => {
                // dup 2; binop; swap 1; pop => binop
                let context = replacement_debug_info(&instructions[start..], 1);
                overwrite(&mut instructions[start], opcode);
                // NOTE: The survivor keeps no event of its own: the window's agreed events
                // replace them.
                let metadata = &mut instructions[start].metadata;
                metadata.drop_function_events();
                metadata.preserve_debug_loc(&context);
                metadata.move_function_events(&context);
                instructions.truncate(start + 1);
            }
            Self::OverwriteTwo { opcode } => {
                overwrite(&mut instructions[start], op::SWAP1);
                overwrite(&mut instructions[start + 1], opcode);
                instructions.truncate(start + 2);
            }
            Self::MergeSwapPop => {
                // SWAPn POP*n SWAP1 POP has n + 3 instructions.
                let end = instructions.len();
                let depth = (end - start - 2) as u8;
                instructions[start]
                    .replace_preserving_metadata(Instruction::stack_op(op::StackOp::Swap(depth)));
                overwrite(&mut instructions[end - 2], op::POP);
                instructions.truncate(end - 1);
            }
            Self::ConsumeStoredValue { depth } => {
                // SWAP(n-1); PUSH address; MSTORE
                instructions[start]
                    .replace_preserving_metadata(Instruction::stack_op(op::StackOp::Swap(depth)));
                // The removed swap and pop take their origins with them.
                let (retained, removed) = instructions[start..].split_at_mut(3);
                for inst in removed {
                    retained[2].metadata.move_function_events(&inst.metadata);
                }
                instructions.truncate(start + 3);
            }
            Self::InvertComparison { value, opcode } => {
                // PUSH c; independent operands; compare; ISZERO
                // => PUSH adjusted; independent operands; opposite compare
                // NOTE: The opposite compare keeps the comparison's context, as solc's `sub` keeps
                // the `eq`'s when `eq; iszero; push; jumpi` fuses (`_eq.debugData()`,
                // libevmasm/PeepholeOptimiser.cpp#L349). The removed `iszero` takes its origins
                // with it; only its function events move.
                instructions[start].replace_preserving_metadata(Instruction::push_value(value));
                let iszero = instructions.pop().expect("matched ISZERO");
                let comparison = instructions.last_mut().expect("matched comparison");
                overwrite(comparison, opcode);
                comparison.metadata.move_function_events(&iszero.metadata);
            }
            Self::ZeroLessIszero => {
                // PUSH 0; LT; ISZERO => ISZERO
                // NOTE: The comparison is removed and the surviving `iszero` keeps its own
                // context. The removed push and `lt` take their origins with them; only their
                // function events move.
                let pushed = instructions.remove(start);
                let comparison = instructions.remove(start);
                let iszero = instructions.last_mut().expect("matched ISZERO");
                iszero.metadata.move_function_events(&pushed.metadata);
                iszero.metadata.move_function_events(&comparison.metadata);
            }
            Self::ReloadStoredValue => {
                instructions.swap(start, start + 3);
                instructions.swap(start + 1, start + 2);
                overwrite(&mut instructions[start], op::DUP1);
                instructions.truncate(start + 3);
            }
            Self::DropNonzeroTest => {
                // The removed test takes its origins with it; its function events move to the
                // branch's label.
                let predicate = replacement_debug_info(&instructions[start..start + 2], 1);
                instructions.drain(start..start + 2);
                instructions[start].metadata.move_function_events(&predicate);
                overwrite(&mut instructions[start + 1], op::JUMPI);
            }
            Self::EqIszeroJumpi => {
                // The `sub` keeps the `eq`'s context, as solc's peephole optimizer gives it
                // (`_eq.debugData()`, libevmasm/PeepholeOptimiser.cpp#L349). The removed `iszero`
                // takes its origins with it; its function events move to the branch's label.
                overwrite(&mut instructions[start], op::SUB);
                let removed = instructions.remove(start + 1);
                instructions[start + 1].metadata.move_function_events(&removed.metadata);
                overwrite(&mut instructions[start + 2], op::JUMPI);
            }
            Self::SwapPair { first, second } => {
                let context = replacement_debug_info(&instructions[start..], 1);
                for (inst, depth) in instructions[start..].iter_mut().zip([first, second]) {
                    *inst =
                        Instruction::stack_op(op::StackOp::Swap(depth)).with_debug_loc(&context);
                }
                instructions.truncate(start + 2);
                // NOTE: Only the final stack state agrees with the old window.
                instructions[start + 1].metadata.move_function_events(&context);
            }
            Self::MoveConstantThroughSwaps => {
                // SWAPn; SWAP1; PUSH c; binop
                let window = &mut instructions[start..start + 6];
                let root = window[2].metadata.clone();
                // The swaps take their origins with them.
                for swap in [0, 3, 4, 5] {
                    let metadata = std::mem::take(&mut window[swap].metadata);
                    window[2].metadata.move_function_events(&metadata);
                }
                window[..4].rotate_right(1);
                // NOTE: The intermediate stack layouts differ even though the final one agrees.
                for swap in &mut window[..2] {
                    swap.metadata.preserve_debug_loc(&root);
                }
                instructions.truncate(start + 4);
            }
            Self::OddWord => {
                // NOTE: Every instruction of the replacement carries the source origin of the
                // `OR` it replaces, as LLVM keeps the location of an expanded instruction, and
                // the `SUB` takes its function events. Origins of the `ADD` and the push drop,
                // as those of any removed operand do.
                let context = replacement_debug_info(&instructions[start..], 3);
                instructions.truncate(start);
                instructions.extend([
                    Instruction::stack_op(op::StackOp::Dup(1)),
                    Instruction::opcode(op::NOT),
                    Instruction::stack_op(op::StackOp::Swap(1)),
                    Instruction::opcode(op::SUB),
                ]);
                for inst in &mut instructions[start..start + 3] {
                    inst.metadata.preserve_debug_loc(&context);
                }
                instructions[start + 3].metadata.preserve_debug_loc(&context);
                instructions[start + 3].metadata.move_function_events(&context);
            }
        }
    }
}

fn overwrite(inst: &mut Instruction, opcode: u8) {
    debug_assert!(inst.as_evm_opcode().is_some());
    inst.replace_preserving_metadata(Instruction::opcode(opcode));
}

fn instruction_sequence(instructions: &[Instruction]) -> impl fmt::Display + '_ {
    fmt::from_fn(move |f| {
        for (index, inst) in instructions.iter().enumerate() {
            if index != 0 {
                f.write_str(" ")?;
            }
            if inst.immutable_push().is_some() {
                f.write_str("push_immutable")?;
            } else if let Some(value) = inst.concrete_immediate() {
                write!(f, "push {value:#x}")?;
            } else if inst.is_encoded_push() {
                f.write_str("push_ref")?;
            } else if let Some(mnemonic) = inst.as_evm_opcode().and_then(op::mnemonic) {
                f.write_str(mnemonic)?;
            } else {
                write!(f, "{}", inst.mnemonic())?;
            }
        }
        Ok(())
    })
}

/// Keeps the replaced result's source context and the window's compatible function events.
fn replacement_debug_info(instructions: &[Instruction], root: usize) -> Metadata {
    let mut context = instructions[root].metadata.clone();
    context.merge_function_events(instructions.iter().map(|inst| &inst.metadata));
    context
}
