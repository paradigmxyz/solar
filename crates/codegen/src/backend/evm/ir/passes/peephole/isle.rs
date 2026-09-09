//! ISLE rules for the EVM IR peephole pass.
//!
//! The rules live in `isle/peephole.isle` and match on a view of the last few
//! instructions of a block. The opcode vocabulary they use is generated from
//! the opcode table into `isle/evm_prelude.isle`. This module implements the
//! window extractors, instruction facets, and opcode classes the rules call.

use super::{Edit, is_block_push, is_removable_push, materialization_cost, push_value, raw_opcode};
use crate::{
    backend::evm::{ir::Instruction, op, op::*},
    mir::utils::eval,
    target::Target,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_config::{EvmVersion, OptimizationMode};

/// How far back a rule may simulate the block's stack.
const MAX_STACK_WINDOW: usize = 24;

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
    include!(concat!(env!("OUT_DIR"), "/peephole.isle.rs"));
}

/// One word of the simulated stack: only a known zero is distinguished.
#[derive(Clone, Copy, PartialEq, Eq)]
enum KnownStackWord {
    Other,
    Zero,
}

/// A stack operation as the symbolic simulation sees it. A removable push is a
/// stack operation for this purpose: deleting it deletes the word it pushes.
#[derive(Clone, Copy)]
enum SymbolicStackOp {
    Push,
    Physical(StackOp),
}

fn symbolic_stack_op(inst: &Instruction) -> Option<SymbolicStackOp> {
    if is_removable_push(inst) {
        return Some(SymbolicStackOp::Push);
    }
    inst.as_stack_op().map(SymbolicStackOp::Physical)
}

/// Returns whether the sequence leaves the stack exactly as it found it.
fn is_noop_stack_sequence(instructions: &[Instruction]) -> bool {
    let mut depth = 0usize;
    for inst in instructions {
        match symbolic_stack_op(inst) {
            Some(SymbolicStackOp::Push) => depth += 1,
            Some(SymbolicStackOp::Physical(op)) => {
                if depth < op.required_depth() {
                    return false;
                }
                let Some(next) = depth.checked_add_signed(op.net_growth()) else { return false };
                depth = next;
            }
            None => return false,
        }
    }
    depth == 0
}

/// Context the rules run against: the instructions of one block so far.
pub(super) struct PeepContext<'a> {
    instructions: &'a [Instruction],
    evm_version: EvmVersion,
}

impl<'a> PeepContext<'a> {
    pub(super) fn new(instructions: &'a [Instruction], evm_version: EvmVersion) -> Self {
        Self { instructions, evm_version }
    }

    /// Returns the edit to apply to the tail of the block, when a rule matches.
    pub(super) fn select<const LATE: bool>(&mut self) -> Option<Rewrite> {
        if !LATE {
            return generated::constructor_peep(self, Window);
        }
        if self.instructions.len() < 5 || raw_opcode(self.instructions.last()?) != Some(SUB) {
            return None;
        }
        let first = self.instructions.len().saturating_sub(MAX_STACK_WINDOW);
        (first..=self.instructions.len() - 5)
            .find_map(|start| generated::constructor_late_peep(self, start))
    }

    /// A canonical instruction suffix whose boundaries permit replacement.
    fn unprotected_tail<const N: usize>(&self) -> Option<[Inst; N]> {
        let start = self.instructions.len().checked_sub(N)?;
        if self.instructions[start..]
            .iter()
            .any(|inst| inst.keeps_with_next() || !inst.has_canonical_stack_effect())
            || start > 0 && self.instructions[start - 1].keeps_with_next()
        {
            return None;
        }
        self.tail()
    }

    fn tail<const N: usize>(&self) -> Option<[Inst; N]> {
        let start = self.instructions.len().checked_sub(N)?;
        Some(std::array::from_fn(|index| start + index))
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
        let target = Target::with(
            self.evm_version,
            OptimizationMode::Gas,
            Target::DEFAULT_EXPECTED_EXECUTIONS,
        );
        let before = target.push(U256::ONE) + target.dup() + target.opcode(SUB);
        let after = target.push(U256::ZERO) + target.opcode(NOT) + target.opcode(NOT);
        after.gas <= before.gas && after.bytes <= before.bytes && after != before
    }

    fn closed_count(&mut self, start: LateWindow) -> bool {
        let end = self.instructions.len();
        // These edits change only the two prefix instructions and final SUB.
        // Keep constrained instruction boundaries and custom stack effects intact.
        if [start, start + 1, end - 2, end - 1].iter().any(|&i| {
            self.instructions[i].keeps_with_next()
                || !self.instructions[i].has_canonical_stack_effect()
        }) || start > 0 && self.instructions[start - 1].keeps_with_next()
        {
            return false;
        }
        let mut depth = 0usize;
        for inst in &self.instructions[start + 2..end - 2] {
            if !inst.has_canonical_stack_effect() || inst.keeps_with_next() {
                return false;
            }
            if inst.is_encoded_push() {
                depth += 1;
            } else if let Some(op) = inst.as_stack_op() {
                if depth < op.required_depth() {
                    return false;
                }
                depth = depth.checked_add_signed(op.net_growth()).expect("sufficient stack");
            } else if let Some(opcode) = raw_opcode(inst)
                && op::is_unaffected_by_preceding_push(opcode)
                && let Some((inputs, outputs)) = op::stack_io(opcode)
            {
                if depth < usize::from(inputs) {
                    return false;
                }
                depth = depth - usize::from(inputs) + usize::from(outputs);
            } else {
                return false;
            }
        }
        depth == 1
    }

    fn protected_window(&mut self, start: LateWindow) -> Option<(Inst, Inst, Inst, Inst, Inst)> {
        let end = self.instructions.len();
        Some((start, end - 4, end - 3, end - 2, end - 1))
    }

    fn protected_mask_profitable(&mut self) -> bool {
        let target = Target::with(
            self.evm_version,
            OptimizationMode::Gas,
            Target::DEFAULT_EXPECTED_EXECUTIONS,
        );
        let before = target.push(U256::ONE)
            + target.push(U256::ONE)
            + target.opcode(SWAP1)
            + target.opcode(SUB);
        let after = target.push(U256::ZERO) + target.opcode(NOT) + target.opcode(NOT);
        after.gas <= before.gas && after.bytes <= before.bytes && after != before
    }

    fn protected_count(&mut self, start: LateWindow) -> bool {
        let end = self.instructions.len();
        if self.instructions[start..]
            .iter()
            .any(|inst| inst.keeps_with_next() || !inst.has_canonical_stack_effect())
            || start > 0 && self.instructions[start - 1].keeps_with_next()
        {
            return false;
        }
        // Track the one word whose value changes. Every other word and operation
        // must be independent of it; only permutations may move the protected word.
        let mut above = 0usize;
        for inst in &self.instructions[start + 1..end - 4] {
            if inst.is_encoded_push() {
                above += 1;
            } else if let Some(stack_op) = inst.as_stack_op() {
                match stack_op {
                    StackOp::Dup(depth) => {
                        if usize::from(depth) == above + 1 {
                            return false;
                        }
                        above += 1;
                    }
                    StackOp::Swap(depth) => {
                        let depth = usize::from(depth);
                        if above == 0 {
                            above = depth;
                        } else if above == depth {
                            above = 0;
                        }
                    }
                    StackOp::Exchange(first, second) => {
                        let (first, second) = (usize::from(first), usize::from(second));
                        if above == first {
                            above = second;
                        } else if above == second {
                            above = first;
                        }
                    }
                    StackOp::Pop => {
                        if above == 0 {
                            return false;
                        }
                        above -= 1;
                    }
                }
            } else if let Some(opcode) = raw_opcode(inst)
                && op::is_unaffected_by_preceding_push(opcode)
                && let Some((inputs, outputs)) = op::stack_io(opcode)
            {
                if above < usize::from(inputs) {
                    return false;
                }
                above = above - usize::from(inputs) + usize::from(outputs);
            } else {
                return false;
            }
        }
        above == 1
    }

    fn nonpush_tail(&mut self, _: Window) -> Option<()> {
        (!self.instructions.last()?.is_encoded_push()).then_some(())
    }

    fn last2(&mut self, _: Window) -> Option<(Inst, Inst)> {
        self.tail().map(|[a, b]| (a, b))
    }

    fn last3(&mut self, _: Window) -> Option<(Inst, Inst, Inst)> {
        self.tail().map(|[a, b, c]| (a, b, c))
    }

    fn last4(&mut self, _: Window) -> Option<(Inst, Inst, Inst, Inst)> {
        self.tail().map(|[a, b, c, d]| (a, b, c, d))
    }

    fn last5(&mut self, _: Window) -> Option<(Inst, Inst, Inst, Inst, Inst)> {
        self.tail().map(|[a, b, c, d, e]| (a, b, c, d, e))
    }

    fn last6(&mut self, _: Window) -> Option<(Inst, Inst, Inst, Inst, Inst, Inst)> {
        self.tail().map(|[a, b, c, d, e, f]| (a, b, c, d, e, f))
    }

    fn swap_pop_chain(&mut self, _: Window) -> Option<u8> {
        let instructions = self.instructions;
        if instructions.last()?.as_stack_op() != Some(StackOp::Pop) || instructions.len() < 2 {
            return None;
        }
        let end = instructions.len();
        if instructions[end - 2].as_stack_op() != Some(StackOp::Swap(1)) {
            return None;
        }
        let middle_pops = instructions[..end - 2]
            .iter()
            .rev()
            .take(self.evm_version.reachable_stack_depth() - 1)
            .take_while(|inst| inst.as_stack_op() == Some(StackOp::Pop))
            .count();
        let StackOp::Swap(depth) = (end - 2 - middle_pops)
            .checked_sub(1)
            .and_then(|index| instructions[index].as_stack_op())?
        else {
            return None;
        };
        if usize::from(depth) != middle_pops {
            return None;
        }
        // The merged swap must still be expressible on this target.
        let merged = depth.checked_add(1)?;
        StackOp::Swap(merged).metrics(self.evm_version)?;
        u8::try_from(middle_pops).ok()
    }

    fn swap_discard_chain(&mut self, _: Window) -> Option<u8> {
        let instructions = self.instructions;
        if instructions.last()?.as_stack_op() != Some(StackOp::Pop) {
            return None;
        }
        let pops = instructions
            .iter()
            .rev()
            .take(self.evm_version.reachable_stack_depth() + 1)
            .take_while(|inst| inst.as_stack_op() == Some(StackOp::Pop))
            .count();
        let StackOp::Swap(depth) = (instructions.len() - pops)
            .checked_sub(1)
            .and_then(|index| instructions[index].as_stack_op())?
        else {
            return None;
        };
        (usize::from(depth) + 1 == pops).then_some(())?;
        u8::try_from(pops).ok()
    }

    /// A `DUPn` whose duplicated word this block just pushed as a literal zero.
    ///
    /// The simulation starts after the last operation it cannot follow, keeps only
    /// "is this word a known zero", and gives up on any operation with an overridden
    /// stack effect.
    fn duplicates_known_zero(&mut self, _: Window) -> Option<()> {
        if !self.evm_version.has_push0() {
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
        if !instructions[start..end].iter().any(|inst| push_value(inst) == Some(U256::ZERO)) {
            return None;
        }
        let mut stack = SmallVec::<[KnownStackWord; MAX_STACK_WINDOW + 16]>::from_elem(
            KnownStackWord::Other,
            usize::from(depth),
        );
        for inst in &instructions[start..end] {
            if !inst.has_canonical_stack_effect() {
                return None;
            }
            if inst.is_encoded_push() {
                stack.push(if push_value(inst) == Some(U256::ZERO) {
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
        if !lhs.has_canonical_stack_effect()
            || !rhs.has_canonical_stack_effect()
            || !instruction.has_canonical_stack_effect()
        {
            return None;
        }
        let lhs_value = push_value(lhs)?;
        let rhs_value = push_value(rhs)?;
        let opcode = raw_opcode(instruction)?;
        let result = eval::eval_opcode(opcode, &[rhs_value, lhs_value])?;
        let (lhs_size, lhs_gas) = materialization_cost(self.evm_version, lhs_value);
        let (rhs_size, rhs_gas) = materialization_cost(self.evm_version, rhs_value);
        let (result_size, result_gas) = materialization_cost(self.evm_version, result);
        let input_size = lhs_size + rhs_size + 1;
        let target = Target::with(
            self.evm_version,
            OptimizationMode::Gas,
            Target::DEFAULT_EXPECTED_EXECUTIONS,
        );
        // PUSH lhs; PUSH rhs; opcode => materialize evaluated word
        // Price the operation too: compact wide constants may need several
        // instructions yet cost less than the operation they replace.
        let input_gas = lhs_gas
            + rhs_gas
            + target.opcode_with_immediates(opcode, &[Some(rhs_value), Some(lhs_value)]).gas
                as usize;
        (result_size <= input_size
            && result_gas <= input_gas
            && (result_size < input_size || result_gas < input_gas))
            .then_some(result)
    }

    /// Folds a literal unary expression only when its materialization is Pareto better.
    fn fold_unary_constant(&mut self, _: Window) -> Option<U256> {
        let [value, instruction] = self.unprotected_tail()?;
        let value = push_value(&self.instructions[value])?;
        let opcode = raw_opcode(&self.instructions[instruction])?;
        let result = eval::eval_opcode(opcode, &[value])?;
        let target = Target::with(
            self.evm_version,
            OptimizationMode::Gas,
            Target::DEFAULT_EXPECTED_EXECUTIONS,
        );
        let (input_size, input_gas) = materialization_cost(self.evm_version, value);
        let (result_size, result_gas) = materialization_cost(self.evm_version, result);
        let input_size = input_size + 1;
        let input_gas =
            input_gas + target.opcode_with_immediates(opcode, &[Some(value)]).gas as usize;
        // PUSH value; unary_opcode => materialize evaluated word
        (result_size <= input_size
            && result_gas <= input_gas
            && (result_size < input_size || result_gas < input_gas))
            .then_some(result)
    }

    /// Removes a closed conditional jump whose literal condition is false.
    fn untaken_jump(&mut self, _: Window) -> Option<()> {
        let [condition, destination, instruction] = self.unprotected_tail()?;
        (push_value(&self.instructions[condition])?.is_zero()
            && is_block_push(&self.instructions[destination])
            && raw_opcode(&self.instructions[instruction]) == Some(JUMPI))
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
            .rposition(is_removable_push)
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
        push_value(&self.instructions[inst])
    }

    fn push_block(&mut self, inst: Inst) -> Option<()> {
        is_block_push(&self.instructions[inst]).then_some(())
    }

    fn opcode(&mut self, inst: Inst) -> Option<u8> {
        self.instructions[inst].as_evm_opcode()
    }

    fn removable_push(&mut self, inst: Inst) -> Option<()> {
        is_removable_push(&self.instructions[inst]).then_some(())
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
        match opcode {
            LT => Some(GT),
            GT => Some(LT),
            SLT => Some(SGT),
            SGT => Some(SLT),
            _ => None,
        }
    }

    fn is_noncommutative_binop(&mut self, opcode: u8) -> bool {
        matches!(
            opcode,
            SUB | DIV
                | SDIV
                | MOD
                | SMOD
                | EXP
                | SIGNEXTEND
                | LT
                | GT
                | SLT
                | SGT
                | BYTE
                | SHL
                | SHR
                | SAR
                | KECCAK256
        )
    }

    fn is_sink(&mut self, opcode: u8) -> bool {
        matches!(opcode, MSTORE | MSTORE8 | SSTORE | TSTORE | LOG0)
    }

    fn u8_add(&mut self, a: u8, b: u8) -> u8 {
        a + b
    }

    fn swap_op(&mut self, depth: u8) -> StackOp {
        StackOp::Swap(depth)
    }

    fn exchange_of_swaps(&mut self, first: u8, second: u8, third: u8) -> Option<StackOp> {
        StackOp::from_swaps(first, second, third)
    }

    fn u256_is_zero(&mut self, value: U256) -> bool {
        value.is_zero()
    }

    fn u256_is_one(&mut self, value: U256) -> bool {
        value == U256::ONE
    }

    fn rewrite(&mut self, skip: u8, edit: &Edit) -> Rewrite {
        Rewrite { skip, edit: *edit }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::evm::ir::StackEffect;

    #[test]
    fn suffix_boundaries_remain_protected() {
        let mut instructions = [
            Instruction::opcode(GAS),
            Instruction::push_value(U256::from(2)),
            Instruction::opcode(ISZERO),
        ];
        assert!(
            PeepContext::new(&instructions, EvmVersion::Osaka).unprotected_tail::<2>().is_some()
        );
        for boundary in 0..instructions.len() {
            instructions[boundary].metadata.keep_with_next = true;
            assert!(
                PeepContext::new(&instructions, EvmVersion::Osaka)
                    .unprotected_tail::<2>()
                    .is_none()
            );
            instructions[boundary].metadata.keep_with_next = false;
        }
    }

    #[test]
    fn suffix_requires_canonical_stack_effects() {
        let mut instructions = [Instruction::push_value(U256::ONE), Instruction::opcode(ISZERO)];
        instructions[0].metadata.stack = Some(StackEffect::new(0, 1));
        instructions[1].metadata.stack = Some(StackEffect::new(1, 1));
        assert!(
            PeepContext::new(&instructions, EvmVersion::Osaka).unprotected_tail::<2>().is_some()
        );
        instructions[0].metadata.stack = Some(StackEffect::new(0, 2));
        assert!(
            PeepContext::new(&instructions, EvmVersion::Osaka).unprotected_tail::<2>().is_none()
        );
    }
}
