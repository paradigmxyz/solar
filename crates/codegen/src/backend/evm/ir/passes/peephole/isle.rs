//! ISLE rules for the EVM IR peephole pass.
//!
//! The rules live in `isle/evm-ir/peephole.isle` and match on a view of the last few
//! instructions of a block. The opcode vocabulary they use is generated from
//! the opcode table into `isle/evm-ir/prelude.isle`. This module implements the
//! window extractors, instruction facets, and opcode classes the rules call.

use super::{
    super::utils::{is_split_point, symbolic_stack_op},
    Edit, Rules,
};
use crate::{
    backend::evm::{
        ir::{ImmediateMaterialization, Instruction, StackEffect, immediate_materialization_cost},
        op,
        op::*,
    },
    mir::utils::eval,
    target::{Cost, Target},
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_config::EvmVersion;

/// How far back a rule may simulate the block's stack.
const MAX_STACK_WINDOW: usize = 24;

/// Inverts a comparison against a protected constant without increasing stack usage.
pub(crate) fn invert_comparison(
    instructions: &[Instruction],
    evm_version: EvmVersion,
) -> Option<(usize, U256, u8)> {
    let end = instructions.len().checked_sub(1)?;
    let comparison = &instructions[end];
    let opcode = comparison.as_evm_opcode()?;
    let opposite = flipped_comparison(opcode)?;
    if comparison.keeps_with_next() {
        return None;
    }
    for start in (end.saturating_sub(MAX_STACK_WINDOW - 2)..end).rev() {
        let pushed = &instructions[start];
        if pushed.keeps_with_next() || !is_split_point(instructions, start) {
            break;
        }
        if let Some(value) = pushed.concrete_immediate()
            && let Some(depth @ 0..=1) = protected_word_depth(&instructions[start + 1..end])
        {
            // Bias signed bounds into unsigned order before checking for overflow.
            let bias = if matches!(opcode, SLT | SGT) { U256::ONE << 255 } else { U256::ZERO };
            let ordered = value ^ bias;
            let bound = if matches!(opcode, GT | SGT) == (depth == 1) {
                ordered.checked_add(U256::ONE)
            } else {
                ordered.checked_sub(U256::ONE)
            };
            // A shorter encoding may need an extra temporary stack word.
            if let Some(bound) = bound.map(|bound| bound ^ bias)
                && ImmediateMaterialization::new(evm_version, bound).stack_peak()
                    <= ImmediateMaterialization::new(evm_version, value).stack_peak()
                && op::push_len(evm_version, bound)
                    <= immediate_materialization_cost(evm_version, value).0 + 1
            {
                return Some((start, bound, opposite));
            }
        }
    }
    None
}

fn flipped_comparison(opcode: u8) -> Option<u8> {
    op::swapped_binary_opcode(opcode).filter(|&flipped| flipped != opcode)
}

/// Rewrite-rule name of the instruction tail under inspection.
#[derive(Clone, Copy)]
pub(super) struct Window;

/// Start index of a late window ending at the current prefix.
type LateWindow = usize;

/// An index into the immutable instruction slice being matched.
///
/// Keeping indices in generated matches avoids repeatedly copying and decoding
/// overlapping windows. Each extractor reads just the facet its rule needs.
type Inst = usize;

/// The result of a rule: how many trailing instructions it consumes and the edit.
#[derive(Clone, Copy)]
pub(super) struct Rewrite {
    pub(super) skip: u8,
    pub(super) edit: Edit,
}

#[allow(
    clippy::all,
    clippy::nursery,
    clippy::pedantic,
    dead_code,
    non_camel_case_types,
    non_snake_case,
    rust_2018_idioms,
    unnameable_types,
    unreachable_code,
    unreachable_pub,
    unused_imports,
    unused_mut,
    unused_variables
)]
mod generated {
    use crate::isle_log as log;

    include!(concat!(env!("OUT_DIR"), "/peephole.isle.rs"));
}

/// One word of the simulated stack: only a known zero is distinguished.
#[derive(Clone, Copy, PartialEq, Eq)]
enum KnownStackWord {
    Other,
    Zero,
}

/// Returns the stack effect of an instruction that only pushes, permutes, or computes a pure word,
/// so it cannot inspect the word an earlier push added.
fn word_effect(inst: &Instruction) -> Option<StackEffect> {
    (inst.is_encoded_push()
        || inst.as_stack_op().is_some()
        || inst.as_evm_opcode().is_some_and(op::is_unaffected_by_preceding_push))
    .then(|| inst.stack_effect())
}

/// Returns whether the instructions from `start` on, and the boundary before them, may be replaced.
fn is_unprotected(instructions: &[Instruction], start: usize) -> bool {
    is_split_point(instructions, start)
        && !instructions[start..].iter().any(Instruction::keeps_with_next)
}

/// Counts the trailing `POP`s of `instructions`, up to `limit`, and returns them with the depth
/// of the `SWAP` before them.
fn swap_before_pops(instructions: &[Instruction], limit: usize) -> Option<(usize, u8)> {
    let pops = instructions
        .iter()
        .rev()
        .take(limit)
        .take_while(|inst| inst.as_stack_op() == Some(StackOp::Pop))
        .count();
    let swap = instructions.get(instructions.len().checked_sub(pops + 1)?)?;
    let StackOp::Swap(depth) = swap.as_stack_op()? else { return None };
    Some((pops, depth))
}

/// Returns the stack depth after `effect` from `depth`, unless it reads below the stack.
fn apply_effect(depth: usize, StackEffect { inputs, outputs }: StackEffect) -> Option<usize> {
    Some(depth.checked_sub(usize::from(inputs))? + usize::from(outputs))
}

/// Returns whether a sequence of only pushes and stack operations leaves the stack as it found
/// it.
fn is_noop_stack_sequence(instructions: &[Instruction]) -> bool {
    let depth = instructions
        .iter()
        .try_fold(0usize, |depth, inst| apply_effect(depth, inst.stack_effect()));
    depth == Some(0)
}

/// Tracks a word through instructions that cannot inspect, duplicate, or consume it.
fn protected_word_depth(instructions: &[Instruction]) -> Option<usize> {
    // Track the one word whose value changes. Every other word and operation
    // must be independent of it; only permutations may move the protected word.
    let mut above = 0usize;
    for inst in instructions {
        if inst.keeps_with_next() {
            return None;
        }
        match inst.as_stack_op() {
            Some(StackOp::Dup(depth)) => {
                if usize::from(depth) == above + 1 {
                    return None;
                }
                above += 1;
            }
            Some(StackOp::Swap(depth)) => {
                let depth = usize::from(depth);
                if above == 0 {
                    above = depth;
                } else if above == depth {
                    above = 0;
                }
            }
            Some(StackOp::Exchange(first, second)) => {
                let (first, second) = (usize::from(first), usize::from(second));
                if above == first {
                    above = second;
                } else if above == second {
                    above = first;
                }
            }
            Some(StackOp::Pop) => above = above.checked_sub(1)?,
            None => above = apply_effect(above, word_effect(inst)?)?,
        }
    }
    Some(above)
}

/// Context the rules run against: the instructions of one block so far.
pub(super) struct PeepContext<'a> {
    instructions: &'a [Instruction],
    target: Target,
    rules: Rules,
}

impl<'a> PeepContext<'a> {
    pub(super) fn new(instructions: &'a [Instruction], target: Target, rules: Rules) -> Self {
        Self { instructions, target, rules }
    }

    /// Target-only cleanup deferred until structural sharing has finished. Only the final rules
    /// include it.
    pub(super) fn final_rewrite(&self) -> Option<Rewrite> {
        // PUSH slot; SSTORE/TSTORE; PUSH slot; SLOAD/TLOAD
        // => DUP1; PUSH slot; SSTORE/TSTORE
        if let [.., address, store, loaded, load] = self.instructions
            && ![address, store, loaded, load].iter().any(|inst| inst.keeps_with_next())
            && matches!(
                (store.as_evm_opcode(), load.as_evm_opcode()),
                (Some(SSTORE), Some(SLOAD)) | (Some(TSTORE), Some(TLOAD))
            )
            && let Some(slot) = address.concrete_immediate()
            && loaded.concrete_immediate() == Some(slot)
        {
            return Some(Rewrite { skip: 4, edit: Edit::ReloadStoredValue });
        }
        // DUPn; PUSH address; MSTORE; SWAP(n-1); POP
        // => SWAP(n-1); PUSH address; MSTORE
        if let [.., dup, address, store, swap, pop] = self.instructions
            && let Some(StackOp::Dup(depth)) = dup.as_stack_op()
            && depth > 1
            && address.is_encoded_push()
            && store.as_evm_opcode() == Some(MSTORE)
            && swap.as_stack_op() == Some(StackOp::Swap(depth - 1))
            && pop.as_evm_opcode() == Some(POP)
            && ![dup, address, store, swap, pop].iter().any(|inst| inst.keeps_with_next())
        {
            return Some(Rewrite { skip: 5, edit: Edit::ConsumeStoredValue { depth: depth - 1 } });
        }
        None
    }

    /// Tries the rules whose last instruction has the class of the block's last instruction.
    fn select_by_tail(&mut self) -> Option<Rewrite> {
        let last = self.instructions.last()?;
        // No rule ends at a push.
        if last.is_encoded_push() {
            return None;
        }
        // The verifier rejects raw stack opcodes, so the logical form classifies every tail.
        match last.as_stack_op() {
            Some(StackOp::Dup(_)) => generated::constructor_peep_dup(self, Window),
            Some(StackOp::Swap(_)) => generated::constructor_peep_swap(self, Window),
            Some(StackOp::Exchange(..)) => generated::constructor_peep_exchange(self, Window),
            Some(StackOp::Pop) => generated::constructor_peep_pop(self, Window),
            None => generated::constructor_peep_op(self, Window),
        }
    }

    /// Returns the edit to apply to the tail of the block, when a rule matches.
    pub(super) fn select(&mut self) -> Option<Rewrite> {
        match self.rules {
            Rules::Early => self.select_by_tail(),
            Rules::Final => self.final_rewrite().or_else(|| self.select_by_tail()),
            Rules::Late => {
                if self.instructions.last()?.as_evm_opcode() != Some(SUB) {
                    return None;
                }
                let end = self.instructions.len();
                let first = end.saturating_sub(MAX_STACK_WINDOW);
                let masked = end.checked_sub(5).and_then(|last| {
                    (first..=last).find_map(|start| generated::constructor_late_peep(self, start))
                });
                masked.or_else(|| generated::constructor_late_sub(self, Window))
            }
        }
    }

    /// An instruction suffix whose boundaries permit replacement.
    fn unprotected_tail<const N: usize>(&self) -> Option<[Inst; N]> {
        let tail = self.tail::<N>()?;
        is_unprotected(self.instructions, tail[0]).then_some(tail)
    }

    fn tail<const N: usize>(&self) -> Option<[Inst; N]> {
        let start = self.instructions.len().checked_sub(N)?;
        Some(std::array::from_fn(|index| start + index))
    }

    /// Returns whether `PUSH0; NOT; NOT` dominates the removed instructions of a low-mask rewrite.
    fn mask_profitable(&self, before: Cost) -> bool {
        let target = self.target;
        let after = target.push(U256::ZERO) + target.opcode(NOT) + target.opcode(NOT);
        target.dominates(after, before)
    }
}

impl generated::Context for PeepContext<'_> {
    fn late_window(&mut self, start: LateWindow) -> Option<(Inst, Inst, Inst, Inst)> {
        let end = self.instructions.len();
        Some((start, start + 1, end - 2, end - 1))
    }

    fn late_length(&mut self, start: LateWindow) -> u8 {
        (self.instructions.len() - start) as u8
    }

    fn low_mask_profitable(&mut self) -> bool {
        let target = self.target;
        self.mask_profitable(target.push(U256::ONE) + target.dup() + target.opcode(SUB))
    }

    fn closed_count(&mut self, start: LateWindow) -> bool {
        let instructions = self.instructions;
        let end = instructions.len();
        // These edits change only the two prefix instructions and final SUB.
        // Keep constrained instruction boundaries intact.
        if [start, start + 1, end - 2, end - 1].iter().any(|&i| instructions[i].keeps_with_next())
            || !is_split_point(instructions, start)
        {
            return false;
        }
        let depth = instructions[start + 2..end - 2].iter().try_fold(0usize, |depth, inst| {
            if inst.keeps_with_next() {
                return None;
            }
            apply_effect(depth, word_effect(inst)?)
        });
        depth == Some(1)
    }

    fn protected_window(&mut self, start: LateWindow) -> Option<(Inst, Inst, Inst, Inst, Inst)> {
        let end = self.instructions.len();
        Some((start, end - 4, end - 3, end - 2, end - 1))
    }

    fn protected_mask_profitable(&mut self) -> bool {
        let target = self.target;
        self.mask_profitable(
            target.push(U256::ONE)
                + target.push(U256::ONE)
                + target.opcode(SWAP1)
                + target.opcode(SUB),
        )
    }

    fn protected_count(&mut self, start: LateWindow) -> bool {
        let end = self.instructions.len();
        is_unprotected(self.instructions, start)
            && protected_word_depth(&self.instructions[start + 1..end - 4]) == Some(1)
    }

    fn last2(&mut self, _: Window) -> Option<(Inst, Inst)> {
        self.tail().map(Into::into)
    }

    fn last3(&mut self, _: Window) -> Option<(Inst, Inst, Inst)> {
        self.tail().map(Into::into)
    }

    fn last4(&mut self, _: Window) -> Option<(Inst, Inst, Inst, Inst)> {
        self.tail().map(Into::into)
    }

    fn unprotected_last4(&mut self, _: Window) -> Option<(Inst, Inst, Inst, Inst)> {
        self.unprotected_tail().map(Into::into)
    }

    fn last5(&mut self, _: Window) -> Option<(Inst, Inst, Inst, Inst, Inst)> {
        self.tail().map(Into::into)
    }

    fn unprotected_last5(&mut self, _: Window) -> Option<(Inst, Inst, Inst, Inst, Inst)> {
        self.unprotected_tail().map(Into::into)
    }

    fn last6(&mut self, _: Window) -> Option<(Inst, Inst, Inst, Inst, Inst, Inst)> {
        self.tail().map(Into::into)
    }

    fn unprotected_last6(&mut self, _: Window) -> Option<(Inst, Inst, Inst, Inst, Inst, Inst)> {
        self.unprotected_tail().map(Into::into)
    }

    fn swap_pop_chain(&mut self, _: Window) -> Option<u8> {
        let [rest @ .., swap, pop] = self.instructions else { return None };
        if swap.as_stack_op() != Some(StackOp::Swap(1)) || pop.as_stack_op() != Some(StackOp::Pop) {
            return None;
        }
        let version = self.target.evm_version();
        let (pops, depth) = swap_before_pops(rest, version.reachable_stack_depth() - 1)?;
        if usize::from(depth) != pops {
            return None;
        }
        // The merged swap must still be expressible on this target.
        StackOp::Swap(depth.checked_add(1)?).metrics(version)?;
        // SWAPn POP*n SWAP1 POP
        u8::try_from(pops + 3).ok()
    }

    fn swap_discard_chain(&mut self, _: Window) -> Option<u8> {
        let (pops, depth) = swap_before_pops(
            self.instructions,
            self.target.evm_version().reachable_stack_depth() + 1,
        )?;
        if usize::from(depth) + 1 != pops {
            return None;
        }
        // SWAPn POP*(n+1)
        u8::try_from(pops + 1).ok()
    }

    /// A `DUPn` whose duplicated word this block just pushed as a literal zero.
    ///
    /// The simulation follows only pushes and stack operations, starting after the last other
    /// operation, and keeps only "is this word a known zero".
    fn duplicates_known_zero(&mut self, _: Window) -> Option<()> {
        if !self.target.evm_version().has_push0() {
            return None;
        }
        let instructions = self.instructions;
        let StackOp::Dup(depth) = instructions.last()?.as_stack_op()? else { return None };
        let end = instructions.len() - 1;
        let floor = end.saturating_sub(MAX_STACK_WINDOW);
        let start = instructions[floor..end]
            .iter()
            .rposition(|inst| !(inst.is_encoded_push() || inst.as_stack_op().is_some()))
            .map_or(floor, |index| floor + index + 1);
        if !instructions[start..end]
            .iter()
            .any(|inst| inst.concrete_immediate() == Some(U256::ZERO))
        {
            return None;
        }
        let mut stack = SmallVec::<[KnownStackWord; MAX_STACK_WINDOW + 16]>::from_elem(
            KnownStackWord::Other,
            usize::from(depth),
        );
        for inst in &instructions[start..end] {
            if inst.is_encoded_push() {
                stack.push(if inst.concrete_immediate() == Some(U256::ZERO) {
                    KnownStackWord::Zero
                } else {
                    KnownStackWord::Other
                });
                continue;
            }
            let op = inst.as_stack_op()?;
            for _ in stack.len()..op.required_depth() {
                stack.insert(0, KnownStackWord::Other);
            }
            let top = stack.len() - 1;
            match op {
                StackOp::Dup(depth) => stack.push(stack[top - usize::from(depth - 1)]),
                StackOp::Swap(depth) => stack.swap(top, top - usize::from(depth)),
                StackOp::Exchange(n, m) => stack.swap(top - usize::from(n), top - usize::from(m)),
                StackOp::Pop => {
                    stack.pop();
                }
            }
        }
        let depth = usize::from(depth);
        (stack.len() >= depth && stack[stack.len() - depth] == KnownStackWord::Zero).then_some(())
    }

    /// Two adjacent constants and the operation over them, when materializing the
    /// evaluated result is no worse in both bytes and gas and better in one.
    fn fold_constants(&mut self, _: Window) -> Option<U256> {
        let [.., lhs, rhs, instruction] = self.instructions else { return None };
        let lhs_value = lhs.concrete_immediate()?;
        let rhs_value = rhs.concrete_immediate()?;
        let opcode = instruction.as_evm_opcode()?;
        let result = eval::eval_opcode(opcode, &[rhs_value, lhs_value])?;
        let target = self.target;
        // PUSH lhs; PUSH rhs; opcode => materialize evaluated word
        // Price the operation too: compact wide constants may need several
        // instructions yet cost less than the operation they replace.
        let input = target.push(lhs_value)
            + target.push(rhs_value)
            + target.opcode_with_immediates(opcode, &[Some(rhs_value), Some(lhs_value)]);
        target.dominates(target.push(result), input).then_some(result)
    }

    /// Folds a literal unary expression only when its materialization is Pareto better.
    fn fold_unary_constant(&mut self, _: Window) -> Option<U256> {
        let [value, instruction] = self.unprotected_tail()?;
        let value = self.instructions[value].concrete_immediate()?;
        let opcode = self.instructions[instruction].as_evm_opcode()?;
        let result = eval::eval_opcode(opcode, &[value])?;
        let target = self.target;
        // PUSH value; unary_opcode => materialize evaluated word
        let input = target.push(value) + target.opcode_with_immediates(opcode, &[Some(value)]);
        target.dominates(target.push(result), input).then_some(result)
    }

    /// The negated constant of `PUSH c; SWAP1; SUB`, when `materialize(-c); ADD` improves the
    /// objective without raising the transient stack use above the push's.
    fn negated_subtrahend(&mut self, _: Window) -> Option<U256> {
        let [push, swap, sub] = self.unprotected_tail()?;
        let instructions = self.instructions;
        let value = instructions[push].concrete_immediate()?;
        if instructions[swap].as_stack_op() != Some(StackOp::Swap(1))
            || instructions[sub].as_evm_opcode() != Some(SUB)
        {
            return None;
        }
        let negated = U256::ZERO.wrapping_sub(value);
        let target = self.target;
        // The matched push is a literal, which compact pushes keep when its recipe does not repay.
        // PUSH c; SWAP1; SUB => materialize(-c); ADD
        let before = target.literal_push(value) + target.opcode(SWAP1) + target.opcode(SUB);
        let after = target.push(negated) + target.opcode(ADD);
        let gas_saving =
            i64::from(target.runtime_gas(before)) - i64::from(target.runtime_gas(after));
        let byte_saving = i64::from(before.bytes) - i64::from(after.bytes);
        (target.improves(gas_saving, byte_saving)
            && ImmediateMaterialization::new(target.evm_version(), negated).stack_peak() <= 1)
            .then_some(negated)
    }

    /// Removes a closed conditional jump whose literal condition is false.
    fn untaken_jump(&mut self, _: Window) -> Option<()> {
        let [condition, destination, instruction] = self.unprotected_tail()?;
        (self.instructions[condition].concrete_immediate()?.is_zero()
            && self.instructions[destination].pushed_block().is_some()
            && self.instructions[instruction].as_evm_opcode() == Some(JUMPI))
        .then_some(())
    }

    /// The length of a trailing run of pushes and stack operations that together
    /// leave the stack unchanged, searched from the earliest such start.
    fn noop_stack_suffix(&mut self, _: Window) -> Option<u8> {
        let instructions = self.instructions;
        let end = instructions.len();
        if end < 2 || instructions.last()?.as_stack_op() != Some(StackOp::Pop) {
            return None;
        }
        let floor = end.saturating_sub(MAX_STACK_WINDOW);
        let start = instructions[floor..end - 1]
            .iter()
            .rposition(|inst| symbolic_stack_op(inst).is_none())
            .map_or(floor, |index| floor + index + 1);
        let last_push = instructions[start..end]
            .iter()
            .rposition(Instruction::is_encoded_push)
            .map(|index| start + index)?;
        (start..=last_push)
            .find(|&start| is_noop_stack_sequence(&instructions[start..]))
            .and_then(|start| u8::try_from(end - start).ok())
    }

    fn dup(&mut self, inst: Inst) -> Option<u8> {
        match self.instructions[inst].as_stack_op()? {
            StackOp::Dup(depth) => Some(depth),
            _ => None,
        }
    }

    fn swap(&mut self, inst: Inst) -> Option<u8> {
        match self.instructions[inst].as_stack_op()? {
            StackOp::Swap(depth) => Some(depth),
            _ => None,
        }
    }

    fn exchange(&mut self, inst: Inst) -> Option<(u8, u8)> {
        match self.instructions[inst].as_stack_op()? {
            StackOp::Exchange(n, m) => Some((n, m)),
            _ => None,
        }
    }

    fn pop(&mut self, inst: Inst) -> Option<()> {
        matches!(self.instructions[inst].as_stack_op(), Some(StackOp::Pop)).then_some(())
    }

    fn push(&mut self, inst: Inst) -> Option<U256> {
        self.instructions[inst].concrete_immediate()
    }

    fn push_block(&mut self, inst: Inst) -> Option<()> {
        self.instructions[inst].pushed_block().map(drop)
    }

    fn opcode(&mut self, inst: Inst) -> Option<u8> {
        self.instructions[inst].as_evm_opcode()
    }

    fn any_push(&mut self, inst: Inst) -> Option<()> {
        self.instructions[inst].is_encoded_push().then_some(())
    }

    fn absorbs_zero(&mut self, opcode: u8) -> bool {
        matches!(opcode, MUL | DIV | SDIV | MOD | SMOD | AND | GT)
    }

    fn zero_identity(&mut self, opcode: u8) -> bool {
        matches!(opcode, ADD | OR | XOR | SHL | SHR | SAR)
    }

    fn is_commutative(&mut self, opcode: u8) -> bool {
        op::is_commutative(opcode)
    }

    fn flipped_comparison(&mut self, opcode: u8) -> Option<u8> {
        flipped_comparison(opcode)
    }

    fn is_noncommutative_binop(&mut self, opcode: u8) -> bool {
        op::stack_io(opcode) == Some((2, 1)) && !op::is_commutative(opcode)
    }

    fn is_sink(&mut self, opcode: u8) -> bool {
        // A taken `JUMPI` leaves the stack to its target, which may read the word below.
        op::stack_io(opcode) == Some((2, 0)) && !op::is_terminal(opcode) && opcode != JUMPI
    }

    fn is_pure_binop(&mut self, opcode: u8) -> bool {
        op::is_pure(opcode) && op::stack_io(opcode) == Some((2, 1))
    }

    fn is_deep_swap(&mut self, depth: u8) -> bool {
        depth > 1
    }

    fn u256_is_zero(&mut self, value: U256) -> bool {
        value.is_zero()
    }

    fn u256_is_one(&mut self, value: U256) -> bool {
        value == U256::ONE
    }

    fn invert_comparison(&mut self, _: Window) -> Option<(u8, U256, u8)> {
        let (iszero, comparison) = self.instructions.split_last()?;
        if iszero.as_evm_opcode() != Some(ISZERO) || iszero.keeps_with_next() {
            return None;
        }
        let (start, bound, opposite) = invert_comparison(comparison, self.target.evm_version())?;
        Some(((self.instructions.len() - start) as u8, bound, opposite))
    }

    fn rewrite(&mut self, skip: u8, edit: &Edit) -> Rewrite {
        Rewrite { skip, edit: *edit }
    }
}
