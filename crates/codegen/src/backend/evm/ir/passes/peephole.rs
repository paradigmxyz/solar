//! Local peephole optimization over scheduled EVM IR.

use super::{EvmPass, compact_pushes::immediate_materialization_cost};
use crate::backend::evm::{
    ir::{Instruction, Module, PushValue, TerminatorKind},
    op,
    stack::{StackModel, StackOp as PhysicalStackOp},
};
use alloy_primitives::U256;
use solar_sema::Gcx;
use std::fmt;
use tracing::trace;

pub(super) struct Peephole;

/// Runs peephole cleanup only when the wrapped pass changes the module.
pub(super) struct Cleanup<T>(pub(super) T);

impl EvmPass for Peephole {
    fn name(&self) -> &'static str {
        "peephole"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        optimize_module(gcx, module)
    }
}

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
    let mut changed = false;
    let mut scratch = Vec::new();
    for block in &mut module.blocks {
        let mut rewrites = optimize(gcx, &mut block.instructions, &mut scratch, block.label);
        // `STOP` does not observe the stack, so trailing cleanup `POP`s only spend gas.
        if matches!(
            block.terminator.as_ref().map(|term| &term.kind),
            Some(TerminatorKind::Op(op::STOP))
        ) {
            while block
                .instructions
                .last()
                .is_some_and(|inst| inst.as_legacy_opcode() == Some(op::POP))
            {
                block.instructions.pop();
                rewrites += 1;
            }
        }
        changed |= rewrites != 0;
    }
    changed
}

fn optimize(
    gcx: Gcx<'_>,
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
        while try_peephole(gcx, instructions, block) {
            rewrites += 1;
        }
    }
    rewrites
}

fn try_peephole(gcx: Gcx<'_>, instructions: &mut Vec<Instruction>, block: u32) -> bool {
    // `PUSH x PUSH 0 OP -> PUSH 0`.
    // `PUSH x PUSH 1 EXP -> PUSH 1`.
    if let [.., lhs, pushed, instruction] = instructions.as_slice()
        && is_removable_push(lhs)
        && let Some(value) = push_value(pushed)
        && let Some(opcode) = instruction.as_legacy_opcode()
    {
        if value.is_zero()
            && matches!(
                opcode,
                op::MUL | op::DIV | op::SDIV | op::MOD | op::SMOD | op::AND | op::GT
            )
        {
            return rewrite(instructions, 3, Edit::RemoveFirstKeepOne, block);
        }
        if value == U256::ONE && opcode == op::EXP {
            return rewrite(instructions, 3, Edit::RemoveFirstKeepOne, block);
        }
    }

    // `PUSH 0 OP -> ∅`.
    // `PUSH 0 EQ -> ISZERO`.
    // `PUSH 0 OP -> POP PUSH 0`.
    // `PUSH 1 MUL -> ∅`.
    // `PUSH 1 EXP -> POP PUSH 1`.
    if let [.., pushed, instruction] = instructions.as_slice()
        && let Some(value) = push_value(pushed)
        && let Some(opcode) = instruction.as_legacy_opcode()
    {
        if value.is_zero() {
            match opcode {
                op::ADD | op::OR | op::XOR | op::SHL | op::SHR | op::SAR => {
                    return rewrite(instructions, 2, Edit::Keep(0), block);
                }
                op::EQ => {
                    return rewrite(instructions, 2, Edit::RemoveFirstOverwrite(op::ISZERO), block);
                }
                op::MUL | op::DIV | op::SDIV | op::MOD | op::SMOD | op::AND | op::GT => {
                    return rewrite(instructions, 2, Edit::SwapOverwrite(op::POP), block);
                }
                _ => {}
            }
        }
        if value == U256::ONE {
            match opcode {
                op::MUL => return rewrite(instructions, 2, Edit::Keep(0), block),
                op::EXP => return rewrite(instructions, 2, Edit::SwapOverwrite(op::POP), block),
                _ => {}
            }
        }
    }

    // Fold adjacent literal ADD/MUL expressions only when the compact result is no worse in
    // either encoded size or static gas.
    if let [.., lhs, rhs, instruction] = instructions.as_slice()
        && lhs.has_canonical_stack_effect()
        && rhs.has_canonical_stack_effect()
        && instruction.has_canonical_stack_effect()
        && let Some(lhs_value) = push_value(lhs)
        && let Some(rhs_value) = push_value(rhs)
        && let Some(opcode) = instruction.as_legacy_opcode()
        && let Some((result, opcode_gas)) = match opcode {
            op::ADD => Some((lhs_value.wrapping_add(rhs_value), 3)),
            op::MUL => Some((lhs_value.wrapping_mul(rhs_value), 5)),
            _ => None,
        }
    {
        let evm_version = gcx.sess.opts.evm_version;
        let (lhs_size, lhs_gas) = immediate_materialization_cost(evm_version, lhs_value);
        let (rhs_size, rhs_gas) = immediate_materialization_cost(evm_version, rhs_value);
        let (result_size, result_gas) = immediate_materialization_cost(evm_version, result);
        let input_size = lhs_size + rhs_size + 1;
        let input_gas = lhs_gas + rhs_gas + opcode_gas;
        if result_size <= input_size
            && result_gas <= input_gas
            && (result_size < input_size || result_gas < input_gas)
        {
            return rewrite(instructions, 3, Edit::FoldConstants(result), block);
        }
    }

    // `PUSH x POP -> ∅`.
    if let [.., pushed, pop] = instructions.as_slice()
        && is_removable_push(pushed)
        && pop.as_legacy_opcode() == Some(op::POP)
    {
        return rewrite(instructions, 2, Edit::Keep(0), block);
    }

    // `NOT NOT -> ∅`, `DUPn POP -> ∅`, or an involutive stack operation twice -> ∅.
    if let [.., first, second] = instructions.as_slice()
        && ((first.as_legacy_opcode(), second.as_legacy_opcode()) == (Some(op::NOT), Some(op::NOT))
            || (second.as_stack_op() == Some(op::StackOp::Pop)
                && matches!(first.as_stack_op(), Some(op::StackOp::Dup(_))))
            || (first.as_stack_op() == second.as_stack_op()
                && matches!(
                    first.as_stack_op(),
                    Some(op::StackOp::Swap(_) | op::StackOp::Exchange(_, _))
                )))
    {
        return rewrite(instructions, 2, Edit::Keep(0), block);
    }

    // `DUPn SWAPn -> DUPn` because the swapped values are equal.
    if let [.., dup, swap] = instructions.as_slice()
        && let Some(dup) = dup.as_legacy_opcode()
        && (op::DUP1..=op::DUP16).contains(&dup)
        && swap.as_legacy_opcode() == Some(op::swap(dup - op::DUP1 + 1))
    {
        return rewrite(instructions, 2, Edit::Keep(1), block);
    }

    // `ISZERO ISZERO ISZERO -> ISZERO`.
    if let [.., first, second, third] = instructions.as_slice()
        && first.as_legacy_opcode() == Some(op::ISZERO)
        && second.as_legacy_opcode() == Some(op::ISZERO)
        && third.as_legacy_opcode() == Some(op::ISZERO)
    {
        return rewrite(instructions, 3, Edit::OverwriteOne(op::ISZERO), block);
    }

    // `SWAP1 COMMUTATIVE_OP -> COMMUTATIVE_OP`.
    if let [.., swap, instruction] = instructions.as_slice()
        && swap.as_legacy_opcode() == Some(op::SWAP1)
        && let Some(opcode) = instruction.as_legacy_opcode()
        && is_commutative(opcode)
    {
        return rewrite(instructions, 2, Edit::RemoveFirstKeepOne, block);
    }

    // `SWAP1 LT -> GT`, `SWAP1 GT -> LT`, `SWAP1 SLT -> SGT`, or `SWAP1 SGT -> SLT`.
    if let [.., swap, comparison] = instructions.as_slice()
        && swap.as_legacy_opcode() == Some(op::SWAP1)
        && let Some(comparison) = comparison.as_legacy_opcode()
        && let Some(flipped) = flipped_comparison(comparison)
    {
        return rewrite(instructions, 2, Edit::RemoveFirstOverwrite(flipped), block);
    }

    // `DUP2 OP SWAP1 POP -> OP`.
    // `DUP2 OP SWAP1 POP -> SWAP1 OP`.
    if let [.., dup, binop, swap, pop] = instructions.as_slice()
        && dup.as_legacy_opcode() == Some(op::DUP2)
        && let Some(binop) = binop.as_legacy_opcode()
        && swap.as_legacy_opcode() == Some(op::SWAP1)
        && pop.as_legacy_opcode() == Some(op::POP)
    {
        if is_commutative(binop) {
            return rewrite(instructions, 4, Edit::OverwriteOne(binop), block);
        }
        if matches!(
            binop,
            op::SUB
                | op::DIV
                | op::SDIV
                | op::MOD
                | op::SMOD
                | op::EXP
                | op::SIGNEXTEND
                | op::LT
                | op::GT
                | op::SLT
                | op::SGT
                | op::BYTE
                | op::SHL
                | op::SHR
                | op::SAR
                | op::KECCAK256
        ) {
            return rewrite(instructions, 4, Edit::OverwriteTwo(binop), block);
        }
    }

    // `DUP2 SINK POP -> SWAP1 SINK`.
    if let [.., dup, sink, pop] = instructions.as_slice()
        && dup.as_legacy_opcode() == Some(op::DUP2)
        && let Some(opcode) = sink.as_legacy_opcode()
        && matches!(opcode, op::MSTORE | op::MSTORE8 | op::SSTORE | op::TSTORE | op::LOG0)
        && pop.as_legacy_opcode() == Some(op::POP)
    {
        return rewrite(instructions, 3, Edit::OverwriteTwo(opcode), block);
    }

    // `SWAP1 POP SWAP2 POP -> SWAP3 POP POP`.
    if let [.., first_swap, first_pop, second_swap, second_pop] = instructions.as_slice()
        && first_swap.as_legacy_opcode() == Some(op::SWAP1)
        && first_pop.as_legacy_opcode() == Some(op::POP)
        && second_swap.as_legacy_opcode() == Some(op::SWAP2)
        && second_pop.as_legacy_opcode() == Some(op::POP)
    {
        return rewrite(instructions, 4, Edit::MergeSwapPop(3), block);
    }

    // `SWAPn POP*n SWAP1 POP -> SWAP(n+1) POP*(n+1)`.
    for depth in 1..16 {
        let input_len = depth + 3;
        let Some(start) = instructions.len().checked_sub(input_len) else {
            break;
        };
        if instructions[start].as_legacy_opcode() == Some(op::swap(depth as u8))
            && instructions[start + 1..instructions.len() - 2]
                .iter()
                .all(|inst| inst.as_legacy_opcode() == Some(op::POP))
            && instructions[instructions.len() - 2].as_legacy_opcode() == Some(op::SWAP1)
            && instructions[instructions.len() - 1].as_legacy_opcode() == Some(op::POP)
        {
            let merged_depth = depth + 1;
            return rewrite(instructions, input_len, Edit::MergeSwapPop(merged_depth as u8), block);
        }
    }

    // `SWAPn POP*(n+1) -> POP*(n+1)` because every permuted value is discarded.
    for depth in 1..=16 {
        let input_len = depth + 2;
        let Some(start) = instructions.len().checked_sub(input_len) else {
            break;
        };
        if instructions[start].as_legacy_opcode() == Some(op::swap(depth as u8))
            && instructions[start + 1..].iter().all(|inst| inst.as_legacy_opcode() == Some(op::POP))
        {
            return rewrite(instructions, input_len, Edit::DropDiscardedSwap, block);
        }
    }

    // `DUP1 PUSH x MSTORE DUP1 PUSH x MSTORE -> DUP1 PUSH x MSTORE`.
    if let [.., dup_a, push_a, store_a, dup_b, push_b, store_b] = instructions.as_slice()
        && dup_a.as_legacy_opcode() == Some(op::DUP1)
        && let Some(a) = push_value(push_a)
        && store_a.as_legacy_opcode() == Some(op::MSTORE)
        && dup_b.as_legacy_opcode() == Some(op::DUP1)
        && let Some(b) = push_value(push_b)
        && store_b.as_legacy_opcode() == Some(op::MSTORE)
        && a == b
    {
        return rewrite(instructions, 6, Edit::Keep(3), block);
    }

    // `PUSH x MLOAD DUP1 PUSH x MSTORE -> PUSH x MLOAD`.
    if let [.., load_addr, load, dup, store_addr, store] = instructions.as_slice()
        && let Some(a) = push_value(load_addr)
        && load.as_legacy_opcode() == Some(op::MLOAD)
        && dup.as_legacy_opcode() == Some(op::DUP1)
        && let Some(b) = push_value(store_addr)
        && store.as_legacy_opcode() == Some(op::MSTORE)
        && a == b
    {
        return rewrite(instructions, 5, Edit::Keep(2), block);
    }

    // `DUP1 PUSH x MSTORE POP PUSH x MLOAD -> DUP1 PUSH x MSTORE`.
    if let [.., dup, pushed, store, pop, loaded, load] = instructions.as_slice()
        && dup.as_legacy_opcode() == Some(op::DUP1)
        && let Some(a) = push_value(pushed)
        && store.as_legacy_opcode() == Some(op::MSTORE)
        && pop.as_legacy_opcode() == Some(op::POP)
        && let Some(b) = push_value(loaded)
        && load.as_legacy_opcode() == Some(op::MLOAD)
        && a == b
    {
        return rewrite(instructions, 6, Edit::Keep(3), block);
    }

    // `PUSH x MSTORE PUSH x MLOAD -> DUP1 PUSH x MSTORE`.
    if let [.., store_addr, store, load_addr, load] = instructions.as_slice()
        && let Some(a) = push_value(store_addr)
        && store.as_legacy_opcode() == Some(op::MSTORE)
        && let Some(b) = push_value(load_addr)
        && load.as_legacy_opcode() == Some(op::MLOAD)
        && a == b
    {
        return rewrite(instructions, 4, Edit::ReloadStoredValue, block);
    }

    // `DUP1 PUSH x MSTORE POP -> PUSH x MSTORE`.
    if let [.., dup, pushed, store, pop] = instructions.as_slice()
        && dup.as_legacy_opcode() == Some(op::DUP1)
        && pushed.is_encoded_push()
        && store.as_legacy_opcode() == Some(op::MSTORE)
        && pop.as_legacy_opcode() == Some(op::POP)
    {
        return rewrite(instructions, 4, Edit::RemoveFirstKeepTwo, block);
    }

    // `ISZERO ISZERO PUSH_REF JUMPI -> PUSH_REF JUMPI`.
    if let [.., first, second, target, jump] = instructions.as_slice()
        && first.as_legacy_opcode() == Some(op::ISZERO)
        && second.as_legacy_opcode() == Some(op::ISZERO)
        && is_block_push(target)
        && jump.as_legacy_opcode() == Some(op::JUMPI)
    {
        return rewrite(instructions, 4, Edit::DropDoubleIszero, block);
    }

    // `EQ ISZERO PUSH_REF JUMPI -> SUB PUSH_REF JUMPI`.
    if let [.., eq, iszero, target, jump] = instructions.as_slice()
        && eq.as_legacy_opcode() == Some(op::EQ)
        && iszero.as_legacy_opcode() == Some(op::ISZERO)
        && is_block_push(target)
        && jump.as_legacy_opcode() == Some(op::JUMPI)
    {
        return rewrite(instructions, 4, Edit::EqIszeroJumpi, block);
    }

    if let Some(len) = noop_stack_suffix_len(instructions) {
        return rewrite(instructions, len, Edit::Keep(0), block);
    }

    // `EXCHANGE n, m SWAPn -> SWAPn SWAPm`.
    // `EXCHANGE n, m SWAPm -> SWAPm SWAPn`.
    // `SWAPn EXCHANGE n, m -> SWAPm SWAPn`.
    // `SWAPm EXCHANGE n, m -> SWAPn SWAPm`.
    if let [.., first, second] = instructions.as_slice()
        && let Some((first_depth, second_depth)) = match (first.as_stack_op(), second.as_stack_op())
        {
            (Some(op::StackOp::Exchange(n, m)), Some(op::StackOp::Swap(depth))) if n == depth => {
                Some((n, m))
            }
            (Some(op::StackOp::Exchange(n, m)), Some(op::StackOp::Swap(depth))) if m == depth => {
                Some((m, n))
            }
            (Some(op::StackOp::Swap(depth)), Some(op::StackOp::Exchange(n, m))) if n == depth => {
                Some((m, n))
            }
            (Some(op::StackOp::Swap(depth)), Some(op::StackOp::Exchange(n, m))) if m == depth => {
                Some((n, m))
            }
            _ => None,
        }
    {
        let start = instructions.len() - 2;
        instructions[start] = Instruction::stack_op(op::StackOp::Swap(first_depth));
        instructions[start + 1] = Instruction::stack_op(op::StackOp::Swap(second_depth));
        return true;
    }

    // `SWAPn SWAPm SWAPn -> EXCHANGE n, m`.
    if let [.., first, second, third] = instructions.as_slice()
        && let (Some(first), Some(second), Some(third)) =
            (swap_depth(first), swap_depth(second), swap_depth(third))
        && let Some(exchange) = op::StackOp::from_swaps(first, second, third)
    {
        let start = instructions.len() - 3;
        instructions[start] = Instruction::stack_op(exchange);
        instructions.truncate(start + 1);
        return true;
    }

    false
}

const MAX_STACK_PEEPHOLE_WINDOW: usize = 24;

#[derive(Clone, Copy)]
enum SymbolicStackOp {
    Push,
    Physical(PhysicalStackOp),
}

fn noop_stack_suffix_len(instructions: &[Instruction]) -> Option<usize> {
    let end = instructions.len();
    let last = stack_op(instructions.last()?)?;
    if end < 2
        || matches!(
            last,
            SymbolicStackOp::Push | SymbolicStackOp::Physical(PhysicalStackOp::Dup(_))
        )
    {
        return None;
    }
    let floor = end.saturating_sub(MAX_STACK_PEEPHOLE_WINDOW);
    let start = instructions[floor..end - 1]
        .iter()
        .rposition(|inst| stack_op(inst).is_none())
        .map_or(floor, |index| floor + index + 1);
    (start..end - 1)
        .find(|&start| is_noop_stack_sequence(&instructions[start..]))
        .map(|start| end - start)
}

fn is_noop_stack_sequence(instructions: &[Instruction]) -> bool {
    let mut stack = StackModel::new();
    let mut next_push = 0;
    for inst in instructions {
        match stack_op(inst) {
            Some(SymbolicStackOp::Push) => {
                stack.push(crate::mir::ValueId::from_usize(next_push));
                next_push += 1;
            }
            Some(SymbolicStackOp::Physical(op)) => {
                let required = match op {
                    PhysicalStackOp::Dup(depth) => usize::from(depth),
                    PhysicalStackOp::Swap(depth) => usize::from(depth) + 1,
                    PhysicalStackOp::Exchange(first, second) => usize::from(first.max(second)) + 1,
                    PhysicalStackOp::Pop => 1,
                };
                if stack.depth() < required {
                    return false;
                }
                stack.apply(op);
            }
            None => return false,
        }
    }
    stack.depth() == 0
}

fn stack_op(inst: &Instruction) -> Option<SymbolicStackOp> {
    if is_removable_push(inst) {
        return Some(SymbolicStackOp::Push);
    }
    inst.as_stack_op().map(SymbolicStackOp::Physical)
}

// Keep trace formatting out of the hot matcher's stack frame.
#[inline(never)]
fn rewrite(instructions: &mut Vec<Instruction>, skip: usize, edit: Edit, block: u32) -> bool {
    let start = instructions.len() - skip;
    let input = tracing::enabled!(target: TRACE_TARGET, tracing::Level::TRACE)
        .then(|| instructions[start..].to_vec());
    edit.apply(instructions, start);
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

#[derive(Clone, Copy)]
enum Edit {
    Keep(u8),
    RemoveFirstKeepOne,
    RemoveFirstKeepTwo,
    RemoveFirstOverwrite(u8),
    SwapOverwrite(u8),
    OverwriteOne(u8),
    OverwriteTwo(u8),
    MergeSwapPop(u8),
    DropDiscardedSwap,
    ReloadStoredValue,
    DropDoubleIszero,
    EqIszeroJumpi,
    FoldConstants(U256),
}

impl Edit {
    fn apply(self, instructions: &mut Vec<Instruction>, start: usize) {
        match self {
            Self::Keep(len) => instructions.truncate(start + usize::from(len)),
            Self::RemoveFirstKeepOne => {
                instructions.remove(start);
                instructions.truncate(start + 1);
            }
            Self::RemoveFirstKeepTwo => {
                instructions.remove(start);
                instructions.truncate(start + 2);
            }
            Self::RemoveFirstOverwrite(opcode) => {
                instructions.remove(start);
                overwrite_raw(&mut instructions[start], opcode);
            }
            Self::SwapOverwrite(opcode) => {
                instructions.swap(start, start + 1);
                overwrite_raw(&mut instructions[start], opcode);
            }
            Self::OverwriteOne(opcode) => {
                overwrite_raw(&mut instructions[start], opcode);
                instructions.truncate(start + 1);
            }
            Self::OverwriteTwo(opcode) => {
                overwrite_raw(&mut instructions[start], op::SWAP1);
                overwrite_raw(&mut instructions[start + 1], opcode);
                instructions.truncate(start + 2);
            }
            Self::MergeSwapPop(depth) => {
                let end = instructions.len();
                overwrite_raw(&mut instructions[start], op::swap(depth));
                overwrite_raw(&mut instructions[end - 2], op::POP);
                instructions.truncate(end - 1);
            }
            Self::DropDiscardedSwap => {
                let end = instructions.len();
                overwrite_raw(&mut instructions[start], op::POP);
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
            Self::FoldConstants(value) => {
                instructions[start] = Instruction::push_value(value);
                instructions.truncate(start + 1);
            }
        }
    }
}

fn overwrite_raw(inst: &mut Instruction, opcode: u8) {
    debug_assert!(inst.as_legacy_opcode().is_some());
    let metadata = std::mem::take(&mut inst.metadata);
    *inst = Instruction::opcode(opcode);
    inst.metadata = metadata;
    inst.metadata.stack = None;
}

fn swap_depth(inst: &Instruction) -> Option<u8> {
    let op::StackOp::Swap(depth) = inst.as_stack_op()? else { return None };
    Some(depth)
}

const fn is_commutative(opcode: u8) -> bool {
    matches!(opcode, op::ADD | op::MUL | op::AND | op::OR | op::XOR | op::EQ)
}

const fn flipped_comparison(opcode: u8) -> Option<u8> {
    match opcode {
        op::LT => Some(op::GT),
        op::GT => Some(op::LT),
        op::SLT => Some(op::SGT),
        op::SGT => Some(op::SLT),
        _ => None,
    }
}

fn push_value(inst: &Instruction) -> Option<U256> {
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
