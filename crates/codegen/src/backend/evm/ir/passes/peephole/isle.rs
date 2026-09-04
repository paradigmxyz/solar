//! ISLE rules for the EVM IR peephole pass.
//!
//! The rules live in `isle/peephole.isle` and match on a view of the last few
//! instructions of a block. The opcode vocabulary they use is generated from
//! the opcode table into `isle/evm_prelude.isle`. This module implements the
//! window extractors, instruction facets, and opcode classes the rules call.

use super::{Edit, is_block_push, is_removable_push, materialization_cost, push_value, raw_opcode};
use crate::{
    backend::evm::{ir::Instruction, op, op::*},
    utils::eval,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_config::EvmVersion;

/// How far back a rule may simulate the block's stack.
const MAX_STACK_WINDOW: usize = 24;

/// Rewrite-rule name of the instruction tail under inspection.
#[derive(Clone, Copy)]
pub(super) struct Window;

/// One instruction as the rules see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Inst {
    /// A removable push of an immediate.
    Push { value: U256 },
    /// A push of a block label.
    PushBlock { removable: bool },
    /// Any other push.
    PushOther { removable: bool },
    /// An instruction with a legacy opcode, and the stack operation it was built as.
    Op { opcode: u8, stack: Option<StackOp> },
    /// A stack operation without a legacy encoding.
    StackOnly { stack: StackOp },
}

impl Inst {
    fn of(inst: &Instruction) -> Self {
        if let Some(value) = push_value(inst) {
            Self::Push { value }
        } else if is_block_push(inst) {
            Self::PushBlock { removable: is_removable_push(inst) }
        } else if inst.is_encoded_push() {
            Self::PushOther { removable: is_removable_push(inst) }
        } else if let Some(opcode) = inst.as_evm_opcode() {
            Self::Op { opcode, stack: inst.as_stack_op() }
        } else {
            let stack = inst.as_stack_op().expect("non-push instruction without a legacy opcode");
            Self::StackOnly { stack }
        }
    }

    const fn stack(self) -> Option<StackOp> {
        match self {
            Self::Op { stack, .. } => stack,
            Self::StackOnly { stack } => Some(stack),
            Self::Push { .. } | Self::PushBlock { .. } | Self::PushOther { .. } => None,
        }
    }
}

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
    pub(super) fn peep(&mut self) -> Option<Rewrite> {
        generated::constructor_peep(self, Window)
    }

    fn tail<const N: usize>(&self) -> Option<[Inst; N]> {
        let start = self.instructions.len().checked_sub(N)?;
        Some(std::array::from_fn(|index| Inst::of(&self.instructions[start + index])))
    }
}

impl generated::Context for PeepContext<'_> {
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
        let start = instructions[..end]
            .iter()
            .rposition(|inst| !(inst.is_encoded_push() || inst.as_stack_op().is_some()))
            .map_or(floor, |index| index + 1)
            .max(floor);
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
        // TODO: Include the evaluated opcode's gas once opcode metadata exposes it.
        let input_gas = lhs_gas + rhs_gas;
        (result_size <= input_size
            && result_gas <= input_gas
            && (result_size < input_size || result_gas < input_gas))
            .then_some(result)
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

    fn dup(&mut self, inst: &Inst) -> Option<u8> {
        match inst.stack()? {
            StackOp::Dup(depth) => Some(depth),
            _ => None,
        }
    }

    fn swap(&mut self, inst: &Inst) -> Option<u8> {
        match inst.stack()? {
            StackOp::Swap(depth) => Some(depth),
            _ => None,
        }
    }

    fn exchange(&mut self, inst: &Inst) -> Option<(u8, u8)> {
        match inst.stack()? {
            StackOp::Exchange(n, m) => Some((n, m)),
            _ => None,
        }
    }

    fn pop(&mut self, inst: &Inst) -> Option<()> {
        matches!(inst.stack(), Some(StackOp::Pop)).then_some(())
    }

    fn removable_push(&mut self, inst: &Inst) -> Option<()> {
        match *inst {
            Inst::Push { .. } => Some(()),
            Inst::PushBlock { removable } | Inst::PushOther { removable } => {
                removable.then_some(())
            }
            Inst::Op { .. } | Inst::StackOnly { .. } => None,
        }
    }

    fn any_push(&mut self, inst: &Inst) -> Option<()> {
        matches!(*inst, Inst::Push { .. } | Inst::PushBlock { .. } | Inst::PushOther { .. })
            .then_some(())
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
