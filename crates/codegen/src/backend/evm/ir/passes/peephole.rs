//! Local peephole optimization over scheduled EVM IR.
//!
//! This pass repeatedly applies ordered, bounded rewrites to the end of each block's instruction
//! prefix until it reaches a local fixed point. The rules cover constant arithmetic, comparison
//! canonicalization, redundant pushes and copies, target-aware `DUP`/`SWAP`/`EXCHANGE` identities,
//! and short symbolic stack sequences whose net effect is the identity.
//!
//! Rules match only canonical EVM IR instructions and preserve instruction metadata on retained or
//! replacement operations. Constant materializations use the same target-dependent cost model as
//! compact pushes. Stack rewrites check that replacement operations can be lowered for the selected
//! EVM version, and bounded symbolic simulation prevents the matcher from becoming a general or
//! unbounded stack optimizer. Rule order is intentional because one rewrite often exposes the next.
//!
//! Peephole runs at several cleanup points after transforms that delete, coalesce, or resynthesize
//! instructions. [`Cleanup`] couples such a pass with peephole only when the wrapped pass reports a
//! change, keeping the canonical pipeline at a local fixed point without adding optimization logic
//! to assembly.

use super::{
    EvmPass,
    compact_pushes::{immediate_materialization_cost, materialize_immediate},
};
use crate::{
    backend::evm::{
        ir::{Instruction, Module, PushValue},
        op,
        stack::StackOp as PhysicalStackOp,
    },
    utils::eval,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_config::EvmVersion;
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
        changed |= optimize(gcx, &mut block.instructions, &mut scratch, block.label);
    }
    changed
}

fn optimize(
    gcx: Gcx<'_>,
    instructions: &mut Vec<Instruction>,
    scratch: &mut Vec<Instruction>,
    block: u32,
) -> bool {
    scratch.clear();
    std::mem::swap(instructions, scratch);
    instructions.reserve(scratch.len());
    let mut changed = false;
    for inst in scratch.drain(..) {
        instructions.push(inst);
        while try_peephole(gcx, instructions, block) {
            changed = true;
        }
    }
    changed
}

fn try_peephole(gcx: Gcx<'_>, instructions: &mut Vec<Instruction>, block: u32) -> bool {
    macro_rules! rewrite {
        ($skip:expr, $edit:expr) => {
            rewrite(instructions, $skip, $edit, block)
        };
    }

    // `... PUSH0 ... DUPn -> ... PUSH0 ... PUSH0` when the duplicate reads that zero.
    //
    // `PUSH0` is one byte and costs two gas, so it dominates every `DUPn` on targets that support
    // it.
    if gcx.sess.opts.evm_version.has_push0() && duplicates_known_zero(instructions) {
        return rewrite!(1, Edit::OverwritePush0);
    }

    // `PUSH x PUSH 0 OP -> PUSH 0`.
    // `PUSH x PUSH 1 EXP -> PUSH 1`.
    if let [.., lhs, pushed, instruction] = instructions.as_slice()
        && is_removable_push(lhs)
        && let Some(value) = pushed.concrete_immediate()
        && let Some(opcode) = instruction.as_evm_opcode()
    {
        if value.is_zero()
            && matches!(
                opcode,
                op::MUL | op::DIV | op::SDIV | op::MOD | op::SMOD | op::AND | op::GT
            )
        {
            return rewrite!(3, Edit::RemoveFirstKeep(1));
        }
        if value == U256::ONE && opcode == op::EXP {
            return rewrite!(3, Edit::RemoveFirstKeep(1));
        }
    }

    // `PUSH 0 OP -> ∅`.
    // `PUSH 0 EQ -> ISZERO`.
    // `PUSH 0 OP -> POP PUSH 0`.
    // `PUSH 1 MUL -> ∅`.
    // `PUSH 1 EXP -> POP PUSH 1`.
    if let [.., pushed, instruction] = instructions.as_slice()
        && let Some(value) = pushed.concrete_immediate()
        && let Some(opcode) = instruction.as_evm_opcode()
    {
        if value.is_zero() {
            match opcode {
                op::ADD | op::OR | op::XOR | op::SHL | op::SHR | op::SAR => {
                    return rewrite!(2, Edit::Keep(0));
                }
                op::EQ => {
                    return rewrite!(2, Edit::RemoveFirstOverwrite(op::ISZERO));
                }
                op::MUL | op::DIV | op::SDIV | op::MOD | op::SMOD | op::AND | op::GT => {
                    return rewrite!(2, Edit::SwapOverwrite(op::POP));
                }
                _ => {}
            }
        }
        if value == U256::ONE {
            match opcode {
                op::MUL => return rewrite!(2, Edit::Keep(0)),
                op::EXP => return rewrite!(2, Edit::SwapOverwrite(op::POP)),
                _ => {}
            }
        }
    }

    // Fold adjacent constants when the selected result materialization is a Pareto improvement.
    if let [.., lhs, rhs, instruction] = instructions.as_slice()
        && lhs.has_canonical_stack_effect()
        && rhs.has_canonical_stack_effect()
        && instruction.has_canonical_stack_effect()
        && let Some(lhs_value) = lhs.concrete_immediate()
        && let Some(rhs_value) = rhs.concrete_immediate()
        && let Some(opcode) = instruction.as_evm_opcode()
        && let Some(result) = eval::eval_opcode(opcode, &[rhs_value, lhs_value])
    {
        let evm_version = gcx.sess.opts.evm_version;
        let (lhs_size, lhs_gas) = immediate_materialization_cost(evm_version, lhs_value);
        let (rhs_size, rhs_gas) = immediate_materialization_cost(evm_version, rhs_value);
        let (result_size, result_gas) = immediate_materialization_cost(evm_version, result);
        let input_size = lhs_size + rhs_size + 1;
        // TODO: Include the evaluated opcode's gas once opcode metadata exposes it.
        let input_gas = lhs_gas + rhs_gas;
        if result_size <= input_size
            && result_gas <= input_gas
            && (result_size < input_size || result_gas < input_gas)
        {
            return rewrite!(3, Edit::FoldConstants(result, evm_version));
        }
    }

    // `PUSH x POP -> ∅`.
    if let [.., pushed, pop] = instructions.as_slice()
        && is_removable_push(pushed)
        && pop.as_evm_opcode() == Some(op::POP)
    {
        return rewrite!(2, Edit::Keep(0));
    }

    // `NOT NOT -> ∅`, `DUPn POP -> ∅`, or an involutive stack operation twice -> ∅.
    if let [.., first, second] = instructions.as_slice()
        && ((first.as_evm_opcode(), second.as_evm_opcode()) == (Some(op::NOT), Some(op::NOT))
            || (second.as_stack_op() == Some(op::StackOp::Pop)
                && matches!(first.as_stack_op(), Some(op::StackOp::Dup(_))))
            || (first.as_stack_op() == second.as_stack_op()
                && matches!(
                    first.as_stack_op(),
                    Some(op::StackOp::Swap(_) | op::StackOp::Exchange(_, _))
                )))
    {
        return rewrite!(2, Edit::Keep(0));
    }

    // `DUPn SWAPn -> DUPn` because the swapped values are equal.
    if let [.., dup, swap] = instructions.as_slice()
        && let (Some(PhysicalStackOp::Dup(depth)), Some(PhysicalStackOp::Swap(swap_depth))) =
            (dup.as_stack_op(), swap.as_stack_op())
        && depth == swap_depth
    {
        return rewrite!(2, Edit::Keep(1));
    }

    // `ISZERO ISZERO ISZERO -> ISZERO`.
    if let [.., first, second, third] = instructions.as_slice()
        && first.as_evm_opcode() == Some(op::ISZERO)
        && second.as_evm_opcode() == Some(op::ISZERO)
        && third.as_evm_opcode() == Some(op::ISZERO)
    {
        return rewrite!(3, Edit::OverwriteOne(op::ISZERO));
    }

    // `SWAP1 OP -> OP'` when the binary operation accepts reversed operands.
    if let [.., swap, comparison] = instructions.as_slice()
        && swap.as_evm_opcode() == Some(op::SWAP1)
        && let Some(comparison) = comparison.as_evm_opcode()
        && let Some(swapped) = op::swapped_binary_opcode(comparison)
    {
        let edit = if swapped == comparison {
            Edit::RemoveFirstKeep(1)
        } else {
            Edit::RemoveFirstOverwrite(swapped)
        };
        return rewrite!(2, edit);
    }

    // `DUP2 OP SWAP1 POP -> OP`.
    // `DUP2 OP SWAP1 POP -> SWAP1 OP`.
    if let [.., dup, binop, swap, pop] = instructions.as_slice()
        && dup.as_evm_opcode() == Some(op::DUP2)
        && let Some(binop) = binop.as_evm_opcode()
        && swap.as_evm_opcode() == Some(op::SWAP1)
        && pop.as_evm_opcode() == Some(op::POP)
    {
        if op::is_commutative(binop) {
            return rewrite!(4, Edit::OverwriteOne(binop));
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
            return rewrite!(4, Edit::OverwriteTwo(binop));
        }
    }

    // `DUP2 SINK POP -> SWAP1 SINK`.
    if let [.., dup, sink, pop] = instructions.as_slice()
        && dup.as_evm_opcode() == Some(op::DUP2)
        && let Some(opcode) = sink.as_evm_opcode()
        && matches!(opcode, op::MSTORE | op::MSTORE8 | op::SSTORE | op::TSTORE | op::LOG0)
        && pop.as_evm_opcode() == Some(op::POP)
    {
        return rewrite!(3, Edit::OverwriteTwo(opcode));
    }

    if instructions.last().and_then(Instruction::as_stack_op) == Some(PhysicalStackOp::Pop) {
        let max_stack_access = gcx.sess.opts.evm_version.reachable_stack_depth();

        // `SWAPn POP*n SWAP1 POP -> SWAP(n+1) POP*(n+1)`.
        if instructions.len() >= 2
            && instructions[instructions.len() - 2].as_stack_op() == Some(PhysicalStackOp::Swap(1))
        {
            let middle_pops = instructions[..instructions.len() - 2]
                .iter()
                .rev()
                .take(max_stack_access - 1)
                .take_while(|inst| inst.as_stack_op() == Some(PhysicalStackOp::Pop))
                .count();
            let middle_start = instructions.len() - 2 - middle_pops;
            if let Some(PhysicalStackOp::Swap(depth)) =
                middle_start.checked_sub(1).and_then(|index| instructions[index].as_stack_op())
                && usize::from(depth) == middle_pops
                && let Some(merged_depth) = depth.checked_add(1)
                && PhysicalStackOp::Swap(merged_depth).metrics(gcx.sess.opts.evm_version).is_some()
            {
                return rewrite!(middle_pops + 3, Edit::MergeSwapPop(merged_depth));
            }
        }

        // `SWAPn POP*(n+1) -> POP*(n+1)` because every permuted value is discarded.
        let pops = instructions
            .iter()
            .rev()
            .take(max_stack_access + 1)
            .take_while(|inst| inst.as_stack_op() == Some(PhysicalStackOp::Pop))
            .count();
        let first_pop = instructions.len() - pops;
        if let Some(PhysicalStackOp::Swap(depth)) =
            first_pop.checked_sub(1).and_then(|index| instructions[index].as_stack_op())
            && usize::from(depth) + 1 == pops
        {
            return rewrite!(pops + 1, Edit::DropDiscardedSwap);
        }
    }

    // `DUP1 PUSH x MSTORE DUP1 PUSH x MSTORE -> DUP1 PUSH x MSTORE`.
    if let [.., dup_a, push_a, store_a, dup_b, push_b, store_b] = instructions.as_slice()
        && dup_a.as_evm_opcode() == Some(op::DUP1)
        && let Some(a) = push_a.concrete_immediate()
        && store_a.as_evm_opcode() == Some(op::MSTORE)
        && dup_b.as_evm_opcode() == Some(op::DUP1)
        && let Some(b) = push_b.concrete_immediate()
        && store_b.as_evm_opcode() == Some(op::MSTORE)
        && a == b
    {
        return rewrite!(6, Edit::Keep(3));
    }

    // `PUSH x MLOAD DUP1 PUSH x MSTORE -> PUSH x MLOAD`.
    if let [.., load_addr, load, dup, store_addr, store] = instructions.as_slice()
        && let Some(a) = load_addr.concrete_immediate()
        && load.as_evm_opcode() == Some(op::MLOAD)
        && dup.as_evm_opcode() == Some(op::DUP1)
        && let Some(b) = store_addr.concrete_immediate()
        && store.as_evm_opcode() == Some(op::MSTORE)
        && a == b
    {
        return rewrite!(5, Edit::Keep(2));
    }

    // `DUP1 PUSH x MSTORE POP PUSH x MLOAD -> DUP1 PUSH x MSTORE`.
    if let [.., dup, pushed, store, pop, loaded, load] = instructions.as_slice()
        && dup.as_evm_opcode() == Some(op::DUP1)
        && let Some(a) = pushed.concrete_immediate()
        && store.as_evm_opcode() == Some(op::MSTORE)
        && pop.as_evm_opcode() == Some(op::POP)
        && let Some(b) = loaded.concrete_immediate()
        && load.as_evm_opcode() == Some(op::MLOAD)
        && a == b
    {
        return rewrite!(6, Edit::Keep(3));
    }

    // `PUSH value PUSH x MSTORE PUSH x MLOAD -> PUSH value DUP1 PUSH x MSTORE`.
    //
    // Keeping the stored value saves a push and MLOAD.
    if let [.., store_addr, store, load_addr, load] = instructions.as_slice()
        && store_addr.has_canonical_stack_effect()
        && store.has_canonical_stack_effect()
        && load_addr.has_canonical_stack_effect()
        && load.has_canonical_stack_effect()
        && let Some(store_addr) = store_addr.concrete_immediate()
        && store.as_evm_opcode() == Some(op::MSTORE)
        && let Some(load_addr) = load_addr.concrete_immediate()
        && load.as_evm_opcode() == Some(op::MLOAD)
        && store_addr == load_addr
    {
        return rewrite!(4, Edit::ReloadStoredValue);
    }

    // `DUP1 PUSH x MSTORE POP -> PUSH x MSTORE`.
    if let [.., dup, pushed, store, pop] = instructions.as_slice()
        && dup.as_evm_opcode() == Some(op::DUP1)
        && pushed.is_encoded_push()
        && store.as_evm_opcode() == Some(op::MSTORE)
        && pop.as_evm_opcode() == Some(op::POP)
    {
        return rewrite!(4, Edit::RemoveFirstKeep(2));
    }

    // `ISZERO ISZERO PUSH_REF JUMPI -> PUSH_REF JUMPI`.
    if let [.., first, second, target, jump] = instructions.as_slice()
        && first.as_evm_opcode() == Some(op::ISZERO)
        && second.as_evm_opcode() == Some(op::ISZERO)
        && is_block_push(target)
        && jump.as_evm_opcode() == Some(op::JUMPI)
    {
        return rewrite!(4, Edit::DropDoubleIszero);
    }

    // `EQ ISZERO PUSH_REF JUMPI -> SUB PUSH_REF JUMPI`.
    if let [.., eq, iszero, target, jump] = instructions.as_slice()
        && eq.as_evm_opcode() == Some(op::EQ)
        && iszero.as_evm_opcode() == Some(op::ISZERO)
        && is_block_push(target)
        && jump.as_evm_opcode() == Some(op::JUMPI)
    {
        return rewrite!(4, Edit::EqIszeroJumpi);
    }

    if let Some(len) = noop_stack_suffix_len(instructions) {
        return rewrite!(len, Edit::Keep(0));
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
        return rewrite!(
            2,
            Edit::StackOps(op::StackOp::Swap(first_depth), op::StackOp::Swap(second_depth))
        );
    }

    // `SWAPn SWAPm SWAPn -> EXCHANGE n, m`.
    if let [.., first, second, third] = instructions.as_slice()
        && let (
            Some(op::StackOp::Swap(first)),
            Some(op::StackOp::Swap(second)),
            Some(op::StackOp::Swap(third)),
        ) = (first.as_stack_op(), second.as_stack_op(), third.as_stack_op())
        && let Some(exchange) = op::StackOp::from_swaps(first, second, third)
    {
        return rewrite!(3, Edit::StackOp(exchange));
    }

    false
}

const MAX_STACK_PEEPHOLE_WINDOW: usize = 24;

#[derive(Clone, Copy)]
enum SymbolicStackOp {
    Push,
    Physical(PhysicalStackOp),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum KnownStackWord {
    Other,
    Zero,
}

fn duplicates_known_zero(instructions: &[Instruction]) -> bool {
    let Some(op::StackOp::Dup(depth)) = instructions.last().and_then(Instruction::as_stack_op)
    else {
        return false;
    };
    let end = instructions.len() - 1;
    let start = instructions[..end]
        .iter()
        .rposition(|inst| !(inst.is_encoded_push() || inst.as_stack_op().is_some()))
        .map_or(end.saturating_sub(MAX_STACK_PEEPHOLE_WINDOW), |index| index + 1)
        .max(end.saturating_sub(MAX_STACK_PEEPHOLE_WINDOW));
    if !instructions[start..end].iter().any(|inst| inst.concrete_immediate() == Some(U256::ZERO)) {
        return false;
    }
    let mut stack = SmallVec::<[KnownStackWord; MAX_STACK_PEEPHOLE_WINDOW + 16]>::from_elem(
        KnownStackWord::Other,
        usize::from(depth),
    );
    for inst in &instructions[start..end] {
        if !inst.has_canonical_stack_effect() {
            return false;
        }
        if inst.is_encoded_push() {
            stack.push(if inst.concrete_immediate() == Some(U256::ZERO) {
                KnownStackWord::Zero
            } else {
                KnownStackWord::Other
            });
            continue;
        }
        let Some(op) = inst.as_stack_op() else { return false };
        let required = op.required_depth();
        if stack.len() < required {
            for _ in stack.len()..required {
                stack.insert(0, KnownStackWord::Other);
            }
        }
        let top = stack.len() - 1;
        match op {
            op::StackOp::Dup(depth) => stack.push(stack[top - usize::from(depth - 1)]),
            op::StackOp::Swap(depth) => stack.swap(top, top - usize::from(depth)),
            op::StackOp::Exchange(n, m) => {
                stack.swap(top - usize::from(n), top - usize::from(m));
            }
            op::StackOp::Pop => {
                stack.pop();
            }
        }
    }
    let depth = usize::from(depth);
    if stack.len() < depth {
        return false;
    }
    stack[stack.len() - depth] == KnownStackWord::Zero
}

fn noop_stack_suffix_len(instructions: &[Instruction]) -> Option<usize> {
    let end = instructions.len();
    let last = stack_op(instructions.last()?)?;
    if end < 2 || !matches!(last, SymbolicStackOp::Physical(PhysicalStackOp::Pop)) {
        return None;
    }
    let floor = end.saturating_sub(MAX_STACK_PEEPHOLE_WINDOW);
    let start = instructions[floor..end - 1]
        .iter()
        .rposition(|inst| stack_op(inst).is_none())
        .map_or(floor, |index| floor + index + 1);
    let last_push =
        instructions[start..end].iter().rposition(is_removable_push).map(|index| start + index)?;
    (start..=last_push)
        .find(|&start| is_noop_stack_sequence(&instructions[start..]))
        .map(|start| end - start)
}

fn is_noop_stack_sequence(instructions: &[Instruction]) -> bool {
    let mut depth = 0usize;
    for inst in instructions {
        match stack_op(inst) {
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
    OverwritePush0,
    RemoveFirstKeep(u8),
    RemoveFirstOverwrite(u8),
    SwapOverwrite(u8),
    OverwriteOne(u8),
    OverwriteTwo(u8),
    MergeSwapPop(u8),
    DropDiscardedSwap,
    ReloadStoredValue,
    DropDoubleIszero,
    EqIszeroJumpi,
    FoldConstants(U256, EvmVersion),
    StackOp(op::StackOp),
    StackOps(op::StackOp, op::StackOp),
}

impl Edit {
    fn apply(self, instructions: &mut Vec<Instruction>, start: usize) {
        match self {
            Self::Keep(len) => instructions.truncate(start + usize::from(len)),
            Self::OverwritePush0 => {
                let metadata = std::mem::take(&mut instructions[start].metadata);
                instructions[start] = Instruction::push_value(U256::ZERO);
                instructions[start].metadata = metadata;
            }
            Self::RemoveFirstKeep(len) => {
                instructions.remove(start);
                instructions.truncate(start + usize::from(len));
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
                overwrite_stack_op(&mut instructions[start], PhysicalStackOp::Swap(depth));
                overwrite_raw(&mut instructions[end - 2], op::POP);
                instructions.truncate(end - 1);
            }
            Self::DropDiscardedSwap => {
                instructions.remove(start);
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
            Self::FoldConstants(value, evm_version) => {
                instructions.truncate(start);
                materialize_immediate(instructions, evm_version, value);
            }
            Self::StackOp(stack_op) => {
                instructions[start] = Instruction::stack_op(stack_op);
                instructions.truncate(start + 1);
            }
            Self::StackOps(first, second) => {
                instructions[start] = Instruction::stack_op(first);
                instructions[start + 1] = Instruction::stack_op(second);
                instructions.truncate(start + 2);
            }
        }
    }
}

fn overwrite_raw(inst: &mut Instruction, opcode: u8) {
    debug_assert!(inst.as_evm_opcode().is_some());
    let metadata = std::mem::take(&mut inst.metadata);
    *inst = Instruction::opcode(opcode);
    inst.metadata = metadata;
    inst.metadata.stack = None;
}

fn overwrite_stack_op(inst: &mut Instruction, stack_op: PhysicalStackOp) {
    let metadata = std::mem::take(&mut inst.metadata);
    *inst = Instruction::stack_op(stack_op);
    inst.metadata = metadata;
    inst.metadata.stack = None;
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
            } else if let Some(value) = inst.concrete_immediate() {
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
