//! Jump Threading optimization pass.
//!
//! This pass eliminates unnecessary jumps by threading through blocks that only contain
//! an unconditional jump. Each eliminated `JUMP` saves its static gas.
//!
//! ## Optimizations performed:
//!
//! 1. **JUMP to JUMP threading**: If block A jumps to block B, and B is an empty forwarder that
//!    only jumps to C, rewrite A to jump directly to C.
//!
//! 2. **JUMPI to JUMP threading**: If a conditional branch or switch targets an empty forwarder,
//!    thread through to the final target.
//!
//! A chain of forwarders resolves to its last block, stopping at the first block it visits twice.
//! Threading never enters a target with phis, which would need new phi inputs. The forwarders
//! themselves stay for later CFG cleanup to remove.
//!
//! Threading can leave a phi-only branch block with one predecessor. When that predecessor
//! jumps unconditionally and no phi result escapes, move the branch onto the predecessor and
//! substitute the selected inputs. This exposes nested short-circuit checks within the same
//! fixpoint, without waiting for another CFG cleanup pass. Targets with phis and cyclic phi
//! inputs stay unchanged. Constant incoming edges also bypass a phi followed by
//! an equality or inequality with zero, provided no result escapes the block, including through
//! the block's own phis, as a loop header's loop-carried values do. Phis that only feed other
//! dead phis of the block do not count and go with the threaded edge.

use super::{cfg_simplify::empty_forwarder_target, utils::redirect_edge};
use crate::mir::{
    BlockId, Function, InstId, InstKind, Module, Terminator, Value, ValueId,
    pass::{MirPass, run_function_pass},
    utils::replace_terminator,
};
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};

/// Function pass for jump threading.
pub(crate) struct JumpThreading;

impl MirPass for JumpThreading {
    fn name(&self) -> &'static str {
        "jump-threading"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| run_to_fixpoint(func))
    }
}

/// Runs jump threading until no more changes and returns whether anything changed.
fn run_to_fixpoint(func: &mut Function) -> bool {
    let mut changed = false;
    while run(func) {
        changed = true;
    }
    changed
}

/// Runs one round of jump threading on a function.
fn run(func: &mut Function) -> bool {
    let mut changed = thread_jumps(func);
    changed |= thread_phi_constant_edges(func);
    changed |= fold_single_predecessor_phi_branches(func);
    changed
}

/// Rewrites every edge into a forwarder chain to enter the end of the chain instead.
fn thread_jumps(func: &mut Function) -> bool {
    let final_targets = forwarder_final_targets(func);
    let mut threaded = false;
    for block_id in func.blocks.indices() {
        let Some(term) = &func.blocks[block_id].terminator else { continue };
        let mut changes = false;
        term.for_each_successor(|target| {
            changes |= threaded_target(func, &final_targets, target).is_some();
        });
        if !changes {
            continue;
        }
        let mut term = term.clone();
        term.for_each_successor_mut(|target| {
            if let Some(final_target) = threaded_target(func, &final_targets, *target) {
                *target = final_target;
            }
        });
        // jump/branch/switch through forwarders -> their final targets
        replace_terminator(func, block_id, term);
        threaded = true;
    }
    threaded
}

/// Resolves each block that forwards to another block to the end of its forwarder chain.
///
/// A block that is not a forwarder, or whose chain ends where it began, has no entry.
fn forwarder_final_targets(func: &Function) -> IndexVec<BlockId, Option<BlockId>> {
    // Only a block with predecessors forwards: bypassing a block that contains a phi would sever
    // the phi's incoming edges, so only fully empty blocks qualify.
    let next = func
        .blocks
        .iter_enumerated()
        .map(|(block_id, block)| {
            empty_forwarder_target(func, block_id).filter(|_| !block.predecessors.is_empty())
        })
        .collect::<IndexVec<_, _>>();
    let mut visited = DenseBitSet::new_empty(func.blocks.len());
    next.indices()
        .map(|start| {
            next[start]?;
            visited.clear();
            let mut current = start;
            while let Some(target) = next[current] {
                if !visited.insert(current) {
                    break;
                }
                current = target;
            }
            (current != start).then_some(current)
        })
        .collect()
}

fn threaded_target(
    func: &Function,
    final_targets: &IndexVec<BlockId, Option<BlockId>>,
    target: BlockId,
) -> Option<BlockId> {
    final_targets[target].filter(|&final_target| !func.block_has_phi(final_target))
}

/// The phis of `block_id` that nothing reads except other dead phis of `block_id`.
///
/// A loop header's phis can feed each other around the back edge without any other reader, such
/// as a swapped pair that the loop never uses afterwards.
fn dead_phis(func: &Function, block_id: BlockId) -> Vec<InstId> {
    let mut phis = func.blocks[block_id]
        .instructions
        .iter()
        .copied()
        .filter(|&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)))
        .collect::<Vec<_>>();
    let mut phi_results = DenseBitSet::new_empty(func.num_values());
    for &phi in &phis {
        if let Some(result) = func.inst_result_value(phi) {
            phi_results.insert(result);
        }
    }

    // live phi results: read outside the block's own phis
    let mut live = DenseBitSet::new_empty(func.num_values());
    let mut mark = |operand| {
        if phi_results.contains(operand) {
            live.insert(operand);
        }
    };
    for (other_block, block) in func.blocks.iter_enumerated() {
        for &inst in &block.instructions {
            if other_block != block_id || !phis.contains(&inst) {
                func.inst(inst).kind.visit_operands(&mut mark);
            }
        }
        if let Some(term) = &block.terminator {
            term.visit_operands(&mut mark);
        }
    }
    // a live phi keeps the phis it reads live
    let mut changed = true;
    while changed {
        changed = false;
        for &phi in &phis {
            if func.inst_result_value(phi).is_some_and(|result| live.contains(result)) {
                func.inst(phi).kind.visit_operands(|operand| {
                    changed |= phi_results.contains(operand) && live.insert(operand);
                });
            }
        }
    }
    phis.retain(|&phi| func.inst_result_value(phi).is_some_and(|result| !live.contains(result)));
    phis
}

/// Whether a result of `block_id` is used outside it. `results` is scratch space.
///
/// A phi input is used at the end of its incoming edge, not in the phi's block. A result that
/// feeds one of the block's own phis, as a loop-carried value does, is therefore used by a block
/// that can be entered without passing through `block_id`, unless that phi is one of `dead_phis`.
fn block_results_have_external_uses(
    func: &Function,
    block_id: BlockId,
    dead_phis: &[InstId],
    results: &mut DenseBitSet<ValueId>,
) -> bool {
    results.clear();
    for &inst in &func.blocks[block_id].instructions {
        if let Some(result) = func.inst_result_value(inst) {
            results.insert(result);
        }
    }
    if results.is_empty() {
        return false;
    }

    let external = |operand| results.contains(operand);
    func.blocks.iter_enumerated().any(|(other_block, block)| {
        let own = other_block == block_id;
        block.instructions.iter().any(|&inst_id| {
            let kind = &func.inst(inst_id).kind;
            (!own || matches!(kind, InstKind::Phi(_)) && !dead_phis.contains(&inst_id))
                && kind.any_operand(external)
        }) || (!own && block.terminator.as_ref().is_some_and(|term| term.any_operand(external)))
    })
}

fn thread_phi_constant_edges(func: &mut Function) -> bool {
    let mut rewrites = Vec::new();
    let mut dead_phis_by_block = Vec::new();
    let mut results = DenseBitSet::new_empty(func.num_values());
    let block_ids = func.blocks.indices();

    for block_id in block_ids {
        if !func.block_has_only_phis(block_id) {
            let Some(Terminator::Branch { condition, .. }) = func.blocks[block_id].terminator
            else {
                continue;
            };
            if branch_zero_test(func, condition).is_none()
                || !func.blocks[block_id].instructions.iter().all(|&inst| {
                    matches!(func.inst(inst).kind, InstKind::Phi(_))
                        || func.inst_result_value(inst) == Some(condition)
                })
            {
                continue;
            }
        }
        let Some(term) = &func.blocks[block_id].terminator else {
            continue;
        };
        let predecessors = &func.blocks[block_id].predecessors;
        if predecessors.is_empty() {
            continue;
        }

        let start = rewrites.len();
        for &pred in predecessors {
            if pred == block_id || successor_count(func, pred, block_id) != 1 {
                continue;
            }
            let Some(target) = phi_constant_target_for_pred(func, block_id, term, pred) else {
                continue;
            };
            if target == block_id || func.block_has_phi(target) {
                continue;
            }
            rewrites.push((pred, block_id, target));
        }
        // Only a block with a threadable edge needs the whole-function use scans.
        if rewrites.len() == start {
            continue;
        }
        let dead = dead_phis(func, block_id);
        if block_results_have_external_uses(func, block_id, &dead, &mut results) {
            rewrites.truncate(start);
        } else if !dead.is_empty() {
            dead_phis_by_block.push((block_id, dead));
        }
    }

    let mut threaded = false;
    for (pred, old_target, new_target) in rewrites {
        // pred -> old_target -> new_target => pred -> new_target
        threaded |= redirect_edge(func, pred, old_target, new_target);
    }
    for (block_id, dead) in dead_phis_by_block {
        // dead loop-carried phi -> removed, so the latch no longer reads a value the threaded edge
        // skips
        func.blocks[block_id].instructions.retain(|inst| !dead.contains(inst));
    }
    threaded
}

fn fold_single_predecessor_phi_branches(func: &mut Function) -> bool {
    let mut folded = false;
    let mut replacements = FxHashMap::default();
    let mut results = DenseBitSet::new_empty(func.num_values());
    for block in func.blocks.indices() {
        if !func.block_has_phi(block) || !func.block_has_only_phis(block) {
            continue;
        }
        let [pred] = func.blocks[block].predecessors.as_slice() else { continue };
        let pred = *pred;
        if pred == block
            || !matches!(func.blocks[pred].terminator, Some(Terminator::Jump(target)) if target == block)
        {
            continue;
        }
        let Some(mut term @ Terminator::Branch { .. }) = func.blocks[block].terminator.clone()
        else {
            continue;
        };
        if term.successors().iter().any(|&target| func.block_has_phi(target)) {
            continue;
        }
        replacements.clear();
        for &id in &func.blocks[block].instructions {
            let InstKind::Phi(incoming) = &func.inst(id).kind else { unreachable!() };
            if let [(incoming_pred, value)] = incoming.as_slice()
                && *incoming_pred == pred
                && let Some(result) = func.inst_result_value(id)
            {
                replacements.insert(result, *value);
            }
        }
        if replacements.len() != func.blocks[block].instructions.len()
            || replacements.values().any(|value| replacements.contains_key(value))
            || block_results_have_external_uses(func, block, &[], &mut results)
        {
            continue;
        }
        // pred: jump block -> branch selected_phi_input, then, else
        // block: phi inputs; branch -> invalid
        let Terminator::Branch { condition, .. } = &mut term else { unreachable!() };
        if let Some(&replacement) = replacements.get(condition) {
            *condition = replacement;
        }
        let context = func.blocks[block].terminator_metadata.debug_context();
        func.replace_uses(&replacements);
        func.blocks[block].instructions.clear();
        replace_terminator(func, block, Terminator::Invalid);
        replace_terminator(func, pred, term);
        func.blocks[pred].terminator_metadata.merge_debug_context(&context);
        folded = true;
    }
    folded
}

fn phi_constant_target_for_pred(
    func: &Function,
    block_id: BlockId,
    term: &Terminator,
    pred: BlockId,
) -> Option<BlockId> {
    match term {
        Terminator::Branch { condition, then_block, else_block } => {
            let (value, invert) = branch_zero_test(func, *condition).unwrap_or((*condition, false));
            let incoming = incoming_value_for_pred(func, block_id, value, pred)?;
            let truth = !func.value_u256(incoming)?.is_zero() ^ invert;
            Some(if truth { *then_block } else { *else_block })
        }
        Terminator::Switch { value, default, cases } => {
            let incoming = incoming_value_for_pred(func, block_id, *value, pred)?;
            let value = func.value_u256(incoming)?;
            for (case, target) in cases {
                if func.value_u256(*case)? == value {
                    return Some(*target);
                }
            }
            Some(*default)
        }
        _ => None,
    }
}

fn branch_zero_test(func: &Function, condition: ValueId) -> Option<(ValueId, bool)> {
    let Value::Inst(inst) = func.value(condition) else { return None };
    let instruction = func.inst(*inst);
    if instruction.metadata.effect().is_some_and(|effect| effect != instruction.kind.effect_kind())
    {
        return None;
    }
    let (InstKind::Eq(value, zero) | InstKind::Ne(value, zero)) = instruction.kind else {
        return None;
    };
    (func.value_u64(zero) == Some(0))
        .then_some((value, matches!(instruction.kind, InstKind::Eq(..))))
}

fn incoming_value_for_pred(
    func: &Function,
    block_id: BlockId,
    value: ValueId,
    pred: BlockId,
) -> Option<ValueId> {
    let Value::Inst(inst_id) = func.value(value) else {
        return Some(value);
    };
    if !func.blocks[block_id].instructions.contains(inst_id) {
        return None;
    }
    let InstKind::Phi(incoming) = &func.inst(*inst_id).kind else {
        return None;
    };
    incoming.iter().find_map(|(incoming_block, incoming_value)| {
        (*incoming_block == pred).then_some(*incoming_value)
    })
}

fn successor_count(func: &Function, pred: BlockId, target: BlockId) -> usize {
    func.blocks[pred]
        .terminator
        .as_ref()
        .map(|term| term.successors().into_iter().filter(|&succ| succ == target).count())
        .unwrap_or_default()
}
