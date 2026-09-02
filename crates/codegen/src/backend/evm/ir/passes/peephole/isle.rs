//! ISLE rules for the EVM IR peephole pass.
//!
//! The rules live in `isle/peephole.isle` and match on a view of the last few
//! instructions of a block. The opcode vocabulary they use is generated from
//! the opcode table into `isle/evm_prelude.isle`. This module implements the
//! window extractors, instruction facets, and opcode classes the rules call.

use super::{Edit, is_block_push, is_removable_push, push_value, raw_opcode};
use crate::backend::evm::{ir::Instruction, op, op::*};
use alloy_primitives::U256;

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

/// Context the rules run against: the instructions of one block so far.
pub(super) struct PeepContext<'a> {
    instructions: &'a [Instruction],
}

impl<'a> PeepContext<'a> {
    pub(super) fn new(instructions: &'a [Instruction]) -> Self {
        Self { instructions }
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
        for depth in 1..16u8 {
            let input_len = usize::from(depth) + 3;
            let start = self.instructions.len().checked_sub(input_len)?;
            let window = &self.instructions[start..];
            if raw_opcode(&window[0]) == Some(op::swap(depth))
                && window[1..input_len - 2].iter().all(|inst| raw_opcode(inst) == Some(POP))
                && raw_opcode(&window[input_len - 2]) == Some(SWAP1)
                && raw_opcode(&window[input_len - 1]) == Some(POP)
            {
                return Some(depth);
            }
        }
        None
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
