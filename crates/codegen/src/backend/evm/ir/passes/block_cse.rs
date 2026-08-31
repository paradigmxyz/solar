//! Block-local common-subexpression regeneration over scheduled EVM IR.

use super::{
    EvmPass,
    peephole::{Peephole, is_removable_copy},
};
use crate::backend::evm::{
    ir::{Instruction, Module, PushValue, default_instruction_stack_effect},
    op,
};
use smallvec::SmallVec;
use solar_data_structures::map::{FxHashMap, FxHasher};
use solar_sema::Gcx;
use std::hash::{Hash, Hasher};

pub(super) struct BlockCse;

/// Default-pipeline adapter that cleans up only when regeneration changed the module.
pub(super) struct BlockCseCleanup;

impl EvmPass for BlockCse {
    fn name(&self) -> &'static str {
        "block-cse"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let mut changed = false;
        let stack_access_limit = gcx.sess.opts.evm_version.reachable_stack_depth();
        for block in &mut module.blocks {
            changed |= regenerate_block(&mut block.instructions, stack_access_limit);
        }
        changed
    }
}

impl EvmPass for BlockCseCleanup {
    fn name(&self) -> &'static str {
        "block-cse"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        let changed = BlockCse.run_pass(gcx, module);
        if changed {
            let _ = Peephole.run_pass(gcx, module);
        }
        changed
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Expr {
    Push(u8, u8, PushValue),
    Op(u8, SmallVec<[usize; 3]>),
    Read(u8, u64, SmallVec<[usize; 3]>),
}

#[derive(Clone, Copy)]
struct StackValue {
    expr: usize,
    /// A closed instruction interval that produces only this value.
    span: Option<(usize, usize)>,
    /// The rebuilt instruction that physically pushed this entry.
    origin: Option<usize>,
}

#[derive(Clone, Copy)]
struct FingerprintValue {
    expr: u64,
    span: Option<(usize, usize)>,
}

fn regenerate_block(instructions: &mut Vec<Instruction>, stack_access_limit: usize) -> bool {
    if !may_regenerate(instructions, stack_access_limit)
        && !has_repeated_const_memory_addr(instructions)
    {
        return false;
    }

    let original = std::mem::take(instructions);
    instructions.reserve(original.len());

    let mut stack = Vec::<StackValue>::new();
    let mut expressions = FxHashMap::<Expr, usize>::default();
    let mut next_expr = 0usize;
    let mut memory_epoch = 0u64;
    let mut storage_epoch = 0u64;
    let mut changed = false;
    // Constant-address memory words with a known symbolic content, maintained
    // through the same walk: a store whose value is already present is a
    // no-op, and a load of a known word yields the stored expression.
    let mut known_stores = FxHashMap::<u64, usize>::default();
    let mut const_exprs = FxHashMap::<usize, u64>::default();

    for inst in original {
        let canonical_stack_effect = inst.has_canonical_stack_effect();
        if canonical_stack_effect && inst.is_encoded_push() {
            let Some(value) = inst.value else {
                append_unknown(inst, instructions, &mut stack, &mut next_expr);
                continue;
            };
            let expr = intern(
                Expr::Push(inst.opcode, inst.encoding, value),
                &mut expressions,
                &mut next_expr,
            );
            if let Some(immediate) = inst.concrete_immediate()
                && let Ok(address) = u64::try_from(immediate)
            {
                const_exprs.insert(expr, address);
            }
            if inst.deferred_push().is_none()
                && inst.pushed_value().is_some_and(|value| !value.is_zero())
                && let Some(depth) = stack.iter().rev().position(|value| value.expr == expr)
                && depth < stack_access_limit
            {
                let mut duplicate = Instruction::stack_op(op::StackOp::Dup((depth + 1) as u8));
                duplicate.metadata = inst.metadata;
                duplicate.metadata.stack = None;
                let origin = instructions.len();
                instructions.push(duplicate);
                stack.push(StackValue { expr, span: None, origin: Some(origin) });
                changed = true;
                continue;
            }
            let start = instructions.len();
            instructions.push(inst);
            stack.push(StackValue { expr, span: Some((start, start + 1)), origin: Some(start) });
            continue;
        }

        let opcode = inst.opcode;
        if canonical_stack_effect && let Some(stack_op) = inst.as_stack_op() {
            match stack_op {
                op::StackOp::Dup(depth) => {
                    let depth = usize::from(depth);
                    ensure_depth(&mut stack, depth, &mut next_expr);
                    let value = stack[stack.len() - depth];
                    let origin = instructions.len();
                    instructions.push(inst);
                    stack.push(StackValue { expr: value.expr, span: None, origin: Some(origin) });
                }
                op::StackOp::Swap(depth) => {
                    let depth = usize::from(depth);
                    ensure_depth(&mut stack, depth + 1, &mut next_expr);
                    let top = stack.len() - 1;
                    stack.swap(top, top - depth);
                    instructions.push(inst);
                }
                op::StackOp::Exchange(n, m) => {
                    ensure_depth(&mut stack, usize::from(m) + 1, &mut next_expr);
                    let top = stack.len() - 1;
                    stack.swap(top - usize::from(n), top - usize::from(m));
                    instructions.push(inst);
                }
                op::StackOp::Pop => {
                    ensure_depth(&mut stack, 1, &mut next_expr);
                    stack.pop();
                    instructions.push(inst);
                }
            }
            continue;
        }
        if canonical_stack_effect && matches!(opcode, op::SWAPN | op::EXCHANGE) {
            instructions.push(inst);
            stack.clear();
            continue;
        }

        if canonical_stack_effect && (opcode == op::MSTORE || opcode == op::MSTORE8) {
            ensure_depth(&mut stack, 2, &mut next_expr);
            let addr = stack[stack.len() - 1];
            let value = stack[stack.len() - 2];
            let const_addr = const_exprs.get(&addr.expr).copied();
            // Re-storing the word already known at this address is a no-op.
            // Remove it only when the two operands were pushed by the two
            // immediately preceding copies, so truncation drops exactly the
            // operand materialization and nothing else.
            if opcode == op::MSTORE
                && let Some(address) = const_addr
                && known_stores.get(&address) == Some(&value.expr)
                && instructions.len() >= 2
                && addr.origin == Some(instructions.len() - 1)
                && value.origin == Some(instructions.len() - 2)
                && is_removable_copy(&instructions[instructions.len() - 1])
                && is_removable_copy(&instructions[instructions.len() - 2])
            {
                instructions.truncate(instructions.len() - 2);
                stack.pop();
                stack.pop();
                changed = true;
                continue;
            }
            match const_addr {
                Some(address) if opcode == op::MSTORE => {
                    known_stores.retain(|&slot, _| slot.abs_diff(address) >= 32);
                    known_stores.insert(address, value.expr);
                }
                Some(address) => {
                    known_stores.retain(|&slot, _| slot.abs_diff(address) >= 32);
                }
                None => known_stores.clear(),
            }
            stack.pop();
            stack.pop();
            memory_epoch = memory_epoch.wrapping_add(1);
            instructions.push(inst);
            continue;
        }

        if canonical_stack_effect && opcode == op::MLOAD {
            ensure_depth(&mut stack, 1, &mut next_expr);
            let addr = stack[stack.len() - 1];
            if let Some(address) = const_exprs.get(&addr.expr).copied()
                && let Some(&known) = known_stores.get(&address)
            {
                stack.pop();
                // Forward the stored word: reuse a live stack copy when the
                // address was just pushed, otherwise keep the load but give
                // its result the stored expression for downstream reuse.
                if !instructions.is_empty()
                    && addr.origin == Some(instructions.len() - 1)
                    && is_removable_copy(&instructions[instructions.len() - 1])
                    && let Some(depth) = stack.iter().rev().position(|entry| entry.expr == known)
                    && depth < stack_access_limit
                {
                    instructions.truncate(instructions.len() - 1);
                    let mut duplicate = Instruction::stack_op(op::StackOp::Dup((depth + 1) as u8));
                    duplicate.metadata = inst.metadata;
                    duplicate.metadata.stack = None;
                    let origin = instructions.len();
                    instructions.push(duplicate);
                    stack.push(StackValue { expr: known, span: None, origin: Some(origin) });
                    changed = true;
                    continue;
                }
                let origin = instructions.len();
                instructions.push(inst);
                stack.push(StackValue { expr: known, span: None, origin: Some(origin) });
                continue;
            }
        }

        if canonical_stack_effect
            && let Some((inputs, read_epoch)) =
                expression_inputs(opcode, memory_epoch, storage_epoch)
        {
            ensure_depth(&mut stack, inputs, &mut next_expr);
            let mut operands = SmallVec::<[StackValue; 3]>::new();
            for _ in 0..inputs {
                operands.push(stack.pop().expect("stack depth was extended"));
            }
            let mut operand_exprs =
                operands.iter().map(|value| value.expr).collect::<SmallVec<[usize; 3]>>();
            if op::is_commutative(opcode) {
                operand_exprs.sort_unstable();
            }
            let expression = if let Some(epoch) = read_epoch {
                Expr::Read(opcode, epoch, operand_exprs)
            } else {
                Expr::Op(opcode, operand_exprs)
            };
            let expr = intern(expression, &mut expressions, &mut next_expr);

            let closed_span =
                closed_span(operands.iter().map(|value| value.span), instructions.len());
            if let Some((start, _)) = closed_span
                && instructions.len() + 1 - start > 1
                && let Some(depth) = stack.iter().rev().position(|value| value.expr == expr)
                && depth < stack_access_limit
            {
                // The removed interval contains at least one pure opcode (gas
                // >= DUPn) plus another instruction. It is therefore strictly
                // smaller and cannot cost more runtime gas than the DUPn.
                instructions.truncate(start);
                let mut duplicate = Instruction::stack_op(op::StackOp::Dup((depth + 1) as u8));
                duplicate.metadata = inst.metadata;
                duplicate.metadata.stack = None;
                let origin = instructions.len();
                instructions.push(duplicate);
                stack.push(StackValue { expr, span: None, origin: Some(origin) });
                changed = true;
            } else {
                let start = closed_span.map(|span| span.0);
                let origin = instructions.len();
                instructions.push(inst);
                stack.push(StackValue {
                    expr,
                    span: start.map(|start| (start, instructions.len())),
                    origin: Some(origin),
                });
            }
            continue;
        }

        let stack_effect =
            canonical_stack_effect.then(|| default_instruction_stack_effect(&inst)).flatten();
        let origin = instructions.len();
        instructions.push(inst);
        if op::writes_memory(opcode) {
            memory_epoch = memory_epoch.wrapping_add(1);
            known_stores.clear();
        }
        if op::writes_storage(opcode) {
            storage_epoch = storage_epoch.wrapping_add(1);
        }
        if !canonical_stack_effect {
            // An explicit override describes only net inputs and outputs, not how a future
            // instruction may permute surviving words. Forget every symbolic identity across it.
            stack.clear();
        } else if let Some(effect) = stack_effect {
            let inputs = usize::from(effect.inputs);
            ensure_depth(&mut stack, inputs, &mut next_expr);
            stack.truncate(stack.len() - inputs);
            for _ in 0..effect.outputs {
                stack.push(StackValue {
                    expr: fresh(&mut next_expr),
                    span: None,
                    origin: Some(origin),
                });
            }
        } else {
            // An unknown stack effect is a hard analysis boundary. Values
            // below it may still exist physically, but cannot safely satisfy
            // a later expression lookup.
            stack.clear();
            known_stores.clear();
        }
    }

    changed
}

/// Returns whether a block computes an expression while an equal expression remains within
/// the target's `DUP` reach. This lightweight symbolic execution avoids rebuilding blocks that
/// merely repeat opcodes with different operands, which is common in already-optimized EVM IR.
fn may_regenerate(instructions: &[Instruction], stack_access_limit: usize) -> bool {
    if !has_repeated_candidate_opcode(instructions) {
        return false;
    }

    let mut stack = Vec::<FingerprintValue>::new();
    let mut next_fresh = 0usize;
    let mut memory_epoch = 0u64;
    let mut storage_epoch = 0u64;

    for (inst_idx, inst) in instructions.iter().enumerate() {
        let canonical_stack_effect = inst.has_canonical_stack_effect();
        if canonical_stack_effect && inst.is_encoded_push() {
            let Some(value) = inst.value else {
                stack.push(FingerprintValue { expr: fresh_hash(&mut next_fresh), span: None });
                continue;
            };
            let expr = push_fingerprint(inst.opcode, inst.encoding, value);
            if inst.deferred_push().is_none()
                && inst.pushed_value().is_some_and(|value| !value.is_zero())
                && stack.iter().rev().take(stack_access_limit).any(|existing| existing.expr == expr)
            {
                return true;
            }
            stack.push(FingerprintValue { expr, span: Some((inst_idx, inst_idx + 1)) });
            continue;
        }

        let opcode = inst.opcode;
        if canonical_stack_effect && let Some(stack_op) = inst.as_stack_op() {
            match stack_op {
                op::StackOp::Dup(depth) => {
                    let depth = usize::from(depth);
                    ensure_hash_depth(&mut stack, depth, &mut next_fresh);
                    let value = stack[stack.len() - depth];
                    stack.push(FingerprintValue { expr: value.expr, span: None });
                }
                op::StackOp::Swap(depth) => {
                    let depth = usize::from(depth);
                    ensure_hash_depth(&mut stack, depth + 1, &mut next_fresh);
                    let top = stack.len() - 1;
                    stack.swap(top, top - depth);
                }
                op::StackOp::Exchange(n, m) => {
                    ensure_hash_depth(&mut stack, usize::from(m) + 1, &mut next_fresh);
                    let top = stack.len() - 1;
                    stack.swap(top - usize::from(n), top - usize::from(m));
                }
                op::StackOp::Pop => {
                    ensure_hash_depth(&mut stack, 1, &mut next_fresh);
                    stack.pop();
                }
            }
            continue;
        }
        if canonical_stack_effect && matches!(opcode, op::SWAPN | op::EXCHANGE) {
            stack.clear();
            continue;
        }

        if canonical_stack_effect
            && let Some((inputs, read_epoch)) =
                expression_inputs(opcode, memory_epoch, storage_epoch)
        {
            ensure_hash_depth(&mut stack, inputs, &mut next_fresh);
            let mut operands = SmallVec::<[FingerprintValue; 3]>::new();
            for _ in 0..inputs {
                operands.push(stack.pop().expect("stack depth was extended"));
            }
            let mut operand_exprs =
                operands.iter().map(|value| value.expr).collect::<SmallVec<[u64; 3]>>();
            if op::is_commutative(opcode) {
                operand_exprs.sort_unstable();
            }
            let expr = operation_fingerprint(opcode, read_epoch, &operand_exprs);
            let closed_span = closed_span(operands.iter().map(|value| value.span), inst_idx);
            if closed_span.is_some()
                && stack.iter().rev().take(stack_access_limit).any(|existing| existing.expr == expr)
            {
                return true;
            }
            stack.push(FingerprintValue {
                expr,
                span: closed_span.map(|(start, _)| (start, inst_idx + 1)),
            });
            continue;
        }

        if op::writes_memory(opcode) {
            memory_epoch = memory_epoch.wrapping_add(1);
        }
        if op::writes_storage(opcode) {
            storage_epoch = storage_epoch.wrapping_add(1);
        }
        if !canonical_stack_effect {
            stack.clear();
        } else if let Some(effect) = default_instruction_stack_effect(inst) {
            let inputs = usize::from(effect.inputs);
            ensure_hash_depth(&mut stack, inputs, &mut next_fresh);
            stack.truncate(stack.len() - inputs);
            for _ in 0..effect.outputs {
                stack.push(FingerprintValue { expr: fresh_hash(&mut next_fresh), span: None });
            }
        } else {
            stack.clear();
        }
    }
    false
}

/// Returns whether two instructions could possibly intern to the same expression. Equal
/// expressions necessarily have the same opcode, so this allocation-free screen rejects most
/// already-optimized blocks before symbolic execution.
fn has_repeated_candidate_opcode(instructions: &[Instruction]) -> bool {
    // An inline 256-bit set over the opcode domain, keeping the screen allocation-free.
    let mut seen = [0u64; 4];
    for inst in instructions {
        let canonical_stack_effect = inst.has_canonical_stack_effect();
        let candidate = if !canonical_stack_effect {
            false
        } else if inst.is_encoded_push() {
            inst.deferred_push().is_none()
                && inst.pushed_value().is_some_and(|value| !value.is_zero())
        } else {
            expression_inputs(inst.opcode, 0, 0).is_some()
        };
        if candidate {
            let opcode = usize::from(inst.opcode);
            let bit = 1u64 << (opcode % 64);
            if seen[opcode / 64] & bit != 0 {
                return true;
            }
            seen[opcode / 64] |= bit;
        }
    }
    false
}

fn push_fingerprint(opcode: u8, encoding: u8, value: PushValue) -> u64 {
    let mut hasher = FxHasher::default();
    (opcode, encoding, value).hash(&mut hasher);
    hasher.finish()
}

fn operation_fingerprint(opcode: u8, epoch: Option<u64>, operands: &[u64]) -> u64 {
    let mut hasher = FxHasher::default();
    (opcode, epoch, operands).hash(&mut hasher);
    hasher.finish()
}

fn fresh_hash(next_fresh: &mut usize) -> u64 {
    let id = *next_fresh;
    *next_fresh += 1;
    // A distinct namespace from `operation_fingerprint` so fresh values never collide with a real
    // expression fingerprint.
    let mut hasher = FxHasher::default();
    ("fresh", id).hash(&mut hasher);
    hasher.finish()
}

fn ensure_hash_depth(stack: &mut Vec<FingerprintValue>, depth: usize, next_fresh: &mut usize) {
    let missing = depth.saturating_sub(stack.len());
    if missing != 0 {
        stack.splice(
            0..0,
            (0..missing).map(|_| FingerprintValue { expr: fresh_hash(next_fresh), span: None }),
        );
    }
}

fn append_unknown(
    inst: Instruction,
    instructions: &mut Vec<Instruction>,
    stack: &mut Vec<StackValue>,
    next_expr: &mut usize,
) {
    let origin = instructions.len();
    instructions.push(inst);
    stack.push(StackValue { expr: fresh(next_expr), span: None, origin: Some(origin) });
}

/// Returns whether a block addresses the same constant memory word from more
/// than one store or from a store and a load, the shapes the known-store
/// model can simplify.
fn has_repeated_const_memory_addr(instructions: &[Instruction]) -> bool {
    let mut stores = SmallVec::<[u64; 8]>::new();
    let mut loads = SmallVec::<[u64; 8]>::new();
    for pair in instructions.windows(2) {
        let [producer, consumer] = pair else { continue };
        let Some(address) =
            producer.concrete_immediate().and_then(|value| u64::try_from(value).ok())
        else {
            continue;
        };
        match consumer.opcode {
            op::MSTORE => {
                if stores.contains(&address) || loads.contains(&address) {
                    return true;
                }
                stores.push(address);
            }
            op::MLOAD => {
                if stores.contains(&address) {
                    return true;
                }
                loads.push(address);
            }
            _ => {}
        }
    }
    false
}

fn closed_span(
    spans: impl IntoIterator<Item = Option<(usize, usize)>>,
    instruction_end: usize,
) -> Option<(usize, usize)> {
    let mut spans: SmallVec<[(usize, usize); 3]> = spans.into_iter().collect::<Option<_>>()?;
    spans.sort_unstable();
    let start = spans.first()?.0;
    let mut cursor = start;
    for (span_start, span_end) in spans {
        if span_start != cursor {
            return None;
        }
        cursor = span_end;
    }
    (cursor == instruction_end).then_some((start, instruction_end + 1))
}

fn ensure_depth(stack: &mut Vec<StackValue>, depth: usize, next_expr: &mut usize) {
    let missing = depth.saturating_sub(stack.len());
    if missing != 0 {
        stack.splice(
            0..0,
            (0..missing).map(|_| StackValue { expr: fresh(next_expr), span: None, origin: None }),
        );
    }
}

fn intern(expr: Expr, expressions: &mut FxHashMap<Expr, usize>, next_expr: &mut usize) -> usize {
    *expressions.entry(expr).or_insert_with(|| fresh(next_expr))
}

fn fresh(next_expr: &mut usize) -> usize {
    let id = *next_expr;
    *next_expr += 1;
    id
}

/// Returns the input count and cache epoch of an opcode this pass value-numbers, or `None` when
/// the opcode is not a tracked pure expression. The input count comes from [`op::stack_io`]; the
/// epoch keys a memory or storage read to the current clobber generation so it is only reused
/// until the next write. Pure arithmetic has no epoch.
const fn expression_inputs(
    opcode: u8,
    memory_epoch: u64,
    storage_epoch: u64,
) -> Option<(usize, Option<u64>)> {
    // Pure opcodes carry no epoch; the reads this pass value-numbers are keyed to the current
    // memory or storage clobber generation so they are only reused until the next write, and
    // calldata is immutable. Everything else is not a tracked expression.
    let epoch = if op::is_pure(opcode) {
        None
    } else {
        match opcode {
            op::MLOAD | op::KECCAK256 => Some(memory_epoch << 1),
            op::SLOAD | op::TLOAD => Some((storage_epoch << 1) | 1),
            op::CALLDATALOAD => Some(0),
            _ => return None,
        }
    };
    match op::stack_io(opcode) {
        Some((inputs, _)) => Some((inputs as usize, epoch)),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::evm::ir::StackEffect;
    use alloy_primitives::U256;

    #[test]
    fn explicit_stack_effect_is_an_analysis_boundary() {
        let mut overridden_dup = Instruction::stack_op(op::StackOp::Dup(1));
        overridden_dup.metadata.stack = Some(StackEffect::new(0, 0));
        let mut instructions = vec![
            Instruction::push_value(U256::from(1)),
            Instruction::push_value(U256::from(2)),
            Instruction::opcode(op::ADD),
            overridden_dup,
            Instruction::push_value(U256::from(1)),
            Instruction::push_value(U256::from(2)),
            Instruction::opcode(op::ADD),
        ];
        let original = instructions.clone();

        assert!(!regenerate_block(&mut instructions, 16));
        assert_eq!(instructions, original);
    }
}
