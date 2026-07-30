//! Peephole and dead terminal-stack optimization over scheduled EVM IR.
//!
//! Local rewrites canonicalize instruction windows. A separate backward CFG
//! analysis removes trailing `POP`s when every continuation terminates without
//! reading the discarded words.

use super::EvmPass;
use crate::backend::evm::{
    ir::{
        BlockId, Instruction, Module, PushValue, StackEffect, TerminatorKind,
        default_instruction_stack_effect,
    },
    op,
    stack::MAX_STACK_DEPTH,
};
use alloy_primitives::U256;
use solar_data_structures::index::{IndexVec, index_vec};
use solar_sema::Gcx;
use std::{collections::VecDeque, fmt};
use tracing::trace;

pub(super) struct Peephole;

impl EvmPass for Peephole {
    fn name(&self) -> &'static str {
        "peephole"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        optimize_module(gcx, module)
    }
}

const TRACE_TARGET: &str = "solar::codegen::evm_ir::peephole";

fn optimize_module(_gcx: Gcx<'_>, module: &mut Module) -> bool {
    let mut changed = false;
    let mut scratch = Vec::new();
    for block in &mut module.blocks {
        let rewrites = optimize(&mut block.instructions, &mut scratch, block.label);
        changed |= rewrites != 0;
    }
    remove_dead_terminal_pops(module) != 0 || changed
}

#[derive(Clone, Copy)]
struct StackSummary {
    /// Minimum number of words read from the incoming stack.
    required: usize,
    /// Maximum additional stack depth reached relative to block entry.
    max_growth: usize,
}

// Process the terminating region from exits to predecessors. Cycles never become
// ready, which is intentional: carrying extra words around a loop could grow the
// stack on every iteration.
fn remove_dead_terminal_pops(module: &mut Module) -> usize {
    let mut summaries = index_vec![None; module.blocks.len()];
    let mut dependents = index_vec![Vec::new(); module.blocks.len()];
    let mut unresolved = index_vec![0usize; module.blocks.len()];
    for (block_id, block) in module.blocks.iter_enumerated() {
        let Some(term) = &block.terminator else { continue };
        match &term.kind {
            TerminatorKind::Jump(target) => {
                dependents[*target].push(block_id);
                unresolved[block_id] = 1;
            }
            TerminatorKind::JumpI { then_block, else_block } => {
                dependents[*then_block].push(block_id);
                unresolved[block_id] = 1;
                if then_block != else_block {
                    dependents[*else_block].push(block_id);
                    unresolved[block_id] += 1;
                }
            }
            TerminatorKind::Op(_) => {}
        }
    }

    let entry_depths = max_entry_depths(module);
    let mut pending =
        module.blocks.indices().filter(|&block| unresolved[block] == 0).collect::<VecDeque<_>>();
    let mut rewrites = 0;
    while let Some(block_id) = pending.pop_front() {
        let block = &mut module.blocks[block_id];
        let Some(term) = &block.terminator else { continue };
        let Some(continuation) = continuation_summary(&term.kind, &summaries) else {
            continue;
        };
        if continuation.required == 0 {
            rewrites += remove_trailing_pops(
                &mut block.instructions,
                continuation.max_growth,
                entry_depths[block_id],
            );
        }
        let Some(summary) = summarize_instructions(&block.instructions, continuation) else {
            continue;
        };
        summaries[block_id] = Some(summary);
        for &dependent in &dependents[block_id] {
            unresolved[dependent] -= 1;
            if unresolved[dependent] == 0 {
                pending.push_back(dependent);
            }
        }
    }
    rewrites
}

fn remove_trailing_pops(
    instructions: &mut Vec<Instruction>,
    continuation_growth: usize,
    entry_depth: Option<usize>,
) -> usize {
    let pops =
        instructions.iter().rev().take_while(|inst| raw_opcode(inst) == Some(op::POP)).count();
    if pops == 0 {
        return 0;
    }

    let removable = if continuation_growth == 0 {
        pops
    } else if let Some(entry_depth) = entry_depth
        && let Some(exit_depth) = apply_instructions(entry_depth, instructions)
    {
        let peak = exit_depth.saturating_add(continuation_growth);
        pops.min(MAX_STACK_DEPTH.saturating_sub(peak))
    } else {
        0
    };
    instructions.truncate(instructions.len() - removable);
    removable
}

fn continuation_summary(
    kind: &TerminatorKind,
    summaries: &IndexVec<BlockId, Option<StackSummary>>,
) -> Option<StackSummary> {
    match kind {
        TerminatorKind::Jump(target) => summaries[*target].map(|summary| StackSummary {
            required: summary.required,
            max_growth: summary.max_growth.max(1),
        }),
        TerminatorKind::JumpI { then_block, else_block } => {
            let then_summary = summaries[*then_block]?;
            let else_summary = summaries[*else_block]?;
            Some(StackSummary {
                required: 1 + then_summary.required.max(else_summary.required),
                max_growth: 1
                    .max(then_summary.max_growth.max(else_summary.max_growth).saturating_sub(1)),
            })
        }
        TerminatorKind::Op(opcode) => {
            let (inputs, _) = op::stack_io(*opcode)?;
            Some(StackSummary { required: usize::from(inputs), max_growth: 0 })
        }
    }
}

fn summarize_instructions(
    instructions: &[Instruction],
    continuation: StackSummary,
) -> Option<StackSummary> {
    let mut required = 0;
    let mut delta = 0isize;
    let mut max_growth = 0;
    for inst in instructions {
        let effect = instruction_stack_effect(inst)?;
        required = required.max(positive(isize::from(effect.inputs) - delta));
        delta += isize::from(effect.outputs) - isize::from(effect.inputs);
        max_growth = max_growth.max(positive(delta));
    }
    required = required.max(positive(isize::try_from(continuation.required).ok()? - delta));
    max_growth = max_growth.max(positive(delta + isize::try_from(continuation.max_growth).ok()?));
    Some(StackSummary { required, max_growth })
}

fn positive(value: isize) -> usize {
    usize::try_from(value).unwrap_or(0)
}

fn max_entry_depths(module: &Module) -> IndexVec<BlockId, Option<usize>> {
    // Dead words remain on the physical stack, so preserve enough headroom for
    // the largest downstream stack growth as well as an encoded jump target.
    let mut entry_depths = index_vec![None; module.blocks.len()];
    if module.blocks.is_empty() {
        return entry_depths;
    }
    entry_depths[BlockId::ENTRY] = Some(0);
    let mut pending = VecDeque::from([BlockId::ENTRY]);
    while let Some(block_id) = pending.pop_front() {
        let block = &module.blocks[block_id];
        let Some(entry_depth) = entry_depths[block_id] else { continue };
        let mut depth = entry_depth;
        let mut valid = true;
        for (index, inst) in block.instructions.iter().enumerate() {
            let Some(next_depth) = apply_stack_effect(depth, instruction_stack_effect(inst)) else {
                valid = false;
                break;
            };
            depth = next_depth;
            if raw_opcode(inst) == Some(op::JUMPI)
                && let Some(target) =
                    index.checked_sub(1).and_then(|index| block.instructions[index].pushed_block())
            {
                propagate_entry_depth(target, depth, &mut entry_depths, &mut pending);
            }
        }
        if !valid {
            continue;
        }
        let Some(term) = &block.terminator else { continue };
        let Some(depth) = apply_stack_effect(depth, terminator_stack_effect(&term.kind)) else {
            continue;
        };
        term.kind.visit_targets(|target| {
            propagate_entry_depth(target, depth, &mut entry_depths, &mut pending);
        });
    }
    entry_depths
}

fn propagate_entry_depth(
    block: BlockId,
    depth: usize,
    entry_depths: &mut IndexVec<BlockId, Option<usize>>,
    pending: &mut VecDeque<BlockId>,
) {
    let entry_depth = &mut entry_depths[block];
    let next = entry_depth.map_or(depth, |current| current.max(depth));
    if *entry_depth != Some(next) {
        *entry_depth = Some(next);
        pending.push_back(block);
    }
}

fn apply_instructions(mut depth: usize, instructions: &[Instruction]) -> Option<usize> {
    for inst in instructions {
        depth = apply_stack_effect(depth, instruction_stack_effect(inst))?;
    }
    Some(depth)
}

fn apply_stack_effect(depth: usize, effect: Option<StackEffect>) -> Option<usize> {
    let effect = effect?;
    depth
        .checked_sub(usize::from(effect.inputs))?
        .checked_add(usize::from(effect.outputs))
        .map(|depth| depth.min(MAX_STACK_DEPTH + 1))
}

fn instruction_stack_effect(inst: &Instruction) -> Option<StackEffect> {
    inst.metadata.stack.or_else(|| default_instruction_stack_effect(inst))
}

fn terminator_stack_effect(kind: &TerminatorKind) -> Option<StackEffect> {
    match kind {
        TerminatorKind::Jump(_) => Some(StackEffect::new(0, 0)),
        TerminatorKind::JumpI { .. } => Some(StackEffect::new(1, 0)),
        TerminatorKind::Op(opcode) => {
            op::stack_io(*opcode).map(|(inputs, outputs)| StackEffect::new(inputs, outputs))
        }
    }
}

fn optimize(
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
        while try_peephole(instructions, block) {
            rewrites += 1;
        }
    }
    rewrites
}

fn try_peephole(instructions: &mut Vec<Instruction>, block: u32) -> bool {
    // `PUSH x PUSH 0 OP -> PUSH 0`.
    // `PUSH x PUSH 1 EXP -> PUSH 1`.
    if let [.., lhs, pushed, instruction] = instructions.as_slice()
        && is_removable_push(lhs)
        && let Some(value) = push_value(pushed)
        && let Some(opcode) = raw_opcode(instruction)
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
        && let Some(opcode) = raw_opcode(instruction)
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

    // `PUSH x POP -> ∅`.
    if let [.., pushed, pop] = instructions.as_slice()
        && is_removable_push(pushed)
        && raw_opcode(pop) == Some(op::POP)
    {
        return rewrite(instructions, 2, Edit::Keep(0), block);
    }

    // `NOT NOT -> ∅`, `DUPn POP -> ∅`, or `SWAPn SWAPn -> ∅`.
    if let [.., first, second] = instructions.as_slice()
        && let Some(a) = raw_opcode(first)
        && let Some(b) = raw_opcode(second)
        && ((a, b) == (op::NOT, op::NOT)
            || (b == op::POP && (op::DUP1..=op::DUP16).contains(&a))
            || (a == b && (op::SWAP1..=op::SWAP16).contains(&a)))
    {
        return rewrite(instructions, 2, Edit::Keep(0), block);
    }

    // `ISZERO ISZERO ISZERO -> ISZERO`.
    if let [.., first, second, third] = instructions.as_slice()
        && raw_opcode(first) == Some(op::ISZERO)
        && raw_opcode(second) == Some(op::ISZERO)
        && raw_opcode(third) == Some(op::ISZERO)
    {
        return rewrite(instructions, 3, Edit::OverwriteOne(op::ISZERO), block);
    }

    // `SWAP1 COMMUTATIVE_OP -> COMMUTATIVE_OP`.
    if let [.., swap, instruction] = instructions.as_slice()
        && raw_opcode(swap) == Some(op::SWAP1)
        && let Some(opcode) = raw_opcode(instruction)
        && is_commutative(opcode)
    {
        return rewrite(instructions, 2, Edit::RemoveFirstKeepOne, block);
    }

    // `SWAP1 LT -> GT`, `SWAP1 GT -> LT`, `SWAP1 SLT -> SGT`, or `SWAP1 SGT -> SLT`.
    if let [.., swap, comparison] = instructions.as_slice()
        && raw_opcode(swap) == Some(op::SWAP1)
        && let Some(comparison) = raw_opcode(comparison)
        && let Some(flipped) = flipped_comparison(comparison)
    {
        return rewrite(instructions, 2, Edit::RemoveFirstOverwrite(flipped), block);
    }

    // `DUP2 OP SWAP1 POP -> OP`.
    // `DUP2 OP SWAP1 POP -> SWAP1 OP`.
    if let [.., dup, binop, swap, pop] = instructions.as_slice()
        && raw_opcode(dup) == Some(op::DUP2)
        && let Some(binop) = raw_opcode(binop)
        && raw_opcode(swap) == Some(op::SWAP1)
        && raw_opcode(pop) == Some(op::POP)
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
        && raw_opcode(dup) == Some(op::DUP2)
        && let Some(opcode) = raw_opcode(sink)
        && matches!(opcode, op::MSTORE | op::MSTORE8 | op::SSTORE | op::TSTORE | op::LOG0)
        && raw_opcode(pop) == Some(op::POP)
    {
        return rewrite(instructions, 3, Edit::OverwriteTwo(opcode), block);
    }

    // `SWAPn POP*n SWAP1 POP -> SWAP(n+1) POP*(n+1)`.
    for depth in 1..16 {
        let input_len = depth + 3;
        let Some(start) = instructions.len().checked_sub(input_len) else {
            break;
        };
        if raw_opcode(&instructions[start]) == Some(op::swap(depth as u8))
            && instructions[start + 1..instructions.len() - 2]
                .iter()
                .all(|inst| raw_opcode(inst) == Some(op::POP))
            && raw_opcode(&instructions[instructions.len() - 2]) == Some(op::SWAP1)
            && raw_opcode(&instructions[instructions.len() - 1]) == Some(op::POP)
        {
            let merged_depth = depth + 1;
            return rewrite(instructions, input_len, Edit::MergeSwapPop(merged_depth as u8), block);
        }
    }

    // `DUP1 PUSH x MSTORE DUP1 PUSH x MSTORE -> DUP1 PUSH x MSTORE`.
    if let [.., dup_a, push_a, store_a, dup_b, push_b, store_b] = instructions.as_slice()
        && raw_opcode(dup_a) == Some(op::DUP1)
        && let Some(a) = push_value(push_a)
        && raw_opcode(store_a) == Some(op::MSTORE)
        && raw_opcode(dup_b) == Some(op::DUP1)
        && let Some(b) = push_value(push_b)
        && raw_opcode(store_b) == Some(op::MSTORE)
        && a == b
    {
        return rewrite(instructions, 6, Edit::Keep(3), block);
    }

    // `PUSH x MLOAD DUP1 PUSH x MSTORE -> PUSH x MLOAD`.
    if let [.., load_addr, load, dup, store_addr, store] = instructions.as_slice()
        && let Some(a) = push_value(load_addr)
        && raw_opcode(load) == Some(op::MLOAD)
        && raw_opcode(dup) == Some(op::DUP1)
        && let Some(b) = push_value(store_addr)
        && raw_opcode(store) == Some(op::MSTORE)
        && a == b
    {
        return rewrite(instructions, 5, Edit::Keep(2), block);
    }

    // `DUP1 PUSH x MSTORE POP PUSH x MLOAD -> DUP1 PUSH x MSTORE`.
    if let [.., dup, pushed, store, pop, loaded, load] = instructions.as_slice()
        && raw_opcode(dup) == Some(op::DUP1)
        && let Some(a) = push_value(pushed)
        && raw_opcode(store) == Some(op::MSTORE)
        && raw_opcode(pop) == Some(op::POP)
        && let Some(b) = push_value(loaded)
        && raw_opcode(load) == Some(op::MLOAD)
        && a == b
    {
        return rewrite(instructions, 6, Edit::Keep(3), block);
    }

    // `ISZERO ISZERO PUSH_REF JUMPI -> PUSH_REF JUMPI`.
    if let [.., first, second, target, jump] = instructions.as_slice()
        && raw_opcode(first) == Some(op::ISZERO)
        && raw_opcode(second) == Some(op::ISZERO)
        && is_block_push(target)
        && raw_opcode(jump) == Some(op::JUMPI)
    {
        return rewrite(instructions, 4, Edit::DropDoubleIszero, block);
    }

    // `EQ ISZERO PUSH_REF JUMPI -> SUB PUSH_REF JUMPI`.
    if let [.., eq, iszero, target, jump] = instructions.as_slice()
        && raw_opcode(eq) == Some(op::EQ)
        && raw_opcode(iszero) == Some(op::ISZERO)
        && is_block_push(target)
        && raw_opcode(jump) == Some(op::JUMPI)
    {
        return rewrite(instructions, 4, Edit::EqIszeroJumpi, block);
    }

    false
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
    RemoveFirstOverwrite(u8),
    SwapOverwrite(u8),
    OverwriteOne(u8),
    OverwriteTwo(u8),
    MergeSwapPop(u8),
    DropDoubleIszero,
    EqIszeroJumpi,
}

impl Edit {
    fn apply(self, instructions: &mut Vec<Instruction>, start: usize) {
        match self {
            Self::Keep(len) => instructions.truncate(start + usize::from(len)),
            Self::RemoveFirstKeepOne => {
                instructions.remove(start);
                instructions.truncate(start + 1);
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
            Self::DropDoubleIszero => {
                instructions.drain(start..start + 2);
                overwrite_raw(&mut instructions[start + 1], op::JUMPI);
            }
            Self::EqIszeroJumpi => {
                overwrite_raw(&mut instructions[start], op::SUB);
                instructions.remove(start + 1);
                overwrite_raw(&mut instructions[start + 2], op::JUMPI);
            }
        }
    }
}

fn overwrite_raw(inst: &mut Instruction, opcode: u8) {
    debug_assert!(raw_opcode(inst).is_some());
    inst.opcode = opcode;
    inst.metadata.stack = None;
}

fn raw_opcode(inst: &Instruction) -> Option<u8> {
    (!inst.is_encoded_push()).then_some(inst.opcode)
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
