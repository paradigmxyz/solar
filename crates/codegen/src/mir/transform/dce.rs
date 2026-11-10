//! Dead Code Elimination (DCE) optimization pass.
//!
//! Remove unused computations and internal calls whose summaries prove that they have no
//! observable behavior and terminate normally. Calls with missing bodies, recursive cycles,
//! checks, or external termination remain conservative; lowered multi-return buffer writes also
//! prevent removal. Run on either representation, sharing cached module summaries. A mark phase
//! from instructions that must execute and from terminator operands finds the live values in
//! linear time, so dead chains and dead phi cycles both go. Reachability is computed once.
//!
//! A counting loop with no effects goes too. Once dead chains are gone, a natural loop whose
//! blocks hold only instructions that need not execute, and whose values nobody outside reads,
//! runs to its exit and does nothing else, so its preheader jumps to the exit instead. The header
//! test must show that the loop ends: `i < n`, or `i != n`, over a counter that starts anywhere
//! and steps by one (up only, for `<`). An increment under `i < n` cannot wrap past `n`, and a unit
//! step meets every word under `!=`. An ABI decoder's validation loop over a word array is such a
//! loop once the loads of its elements are unused.
//!
//! The loop needs one latch, a dedicated preheader, the header's branch as its only exit, and a
//! body that is a chain of blocks; other shapes stay, as do loops with another step or a bound
//! that changes in the loop. Removing a loop that would run out of gas turns that revert into a
//! return.

use crate::mir::{
    BlockId, Callee, Function, InstId, InstKind, MirType, Module, Terminator, Value, ValueId,
    analysis::{CfgInfo, Loop, LoopAnalyzer, MemoryCallSummaries},
    pass::{MirPass, run_function_pass_with_cfg},
    utils::{invalidate_unreachable_block, invalidate_unreachable_blocks, replace_terminator},
};
use alloy_primitives::U256;
use solar_data_structures::bit_set::DenseBitSet;
use solar_evmir::debug::DebugLocKind;
use std::rc::Rc;

/// Function pass for dead code elimination.
///
/// Empties unreachable blocks and removes instructions whose results are unused and whose
/// execution `InstKind::has_side_effects` does not require, together with internal calls whose
/// summaries allow discarding them and counting loops that then do nothing.
pub(crate) struct Dce;

impl MirPass for Dce {
    fn name(&self) -> &'static str {
        "dce"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let summaries = analyses.call_summaries(module);
        let changed = run_function_pass_with_cfg(module, analyses, |func, analyses| {
            invalidate_unreachable_blocks(func, analyses.cfg().reachable())
                + remove_dead_loops(func, analyses.cfg())
                + remove_dead_chains(func, &summaries)
                != 0
        });
        // Removing operations and unreachable blocks cannot add call effects.
        analyses.preserve_call_summaries();
        changed
    }

    fn preserves_debugify_sites(&self) -> bool {
        true
    }
}

fn remove_dead_chains(func: &mut Function, summaries: &MemoryCallSummaries) -> usize {
    let mut discardable = DenseBitSet::new_empty(func.num_insts());
    let mut live = DenseBitSet::<InstId>::new_empty(func.num_insts());
    let mut pending = Vec::new();
    for inst_id in func.instructions() {
        let inst = func.inst(inst_id);
        let discardable_call = matches!(&inst.kind, InstKind::ICall { function: Callee::Function(function), .. }
            if summaries.get(*function).is_some_and(|summary| summary.can_discard_call()));
        if !inst.kind.has_side_effects() || discardable_call {
            discardable.insert(inst_id);
        } else if live.insert(inst_id) {
            pending.push(inst_id);
        }
    }

    let mut mark_operand = |func: &Function, operand, pending: &mut Vec<InstId>| {
        if let Value::Inst(producer) = *func.value(operand)
            && live.insert(producer)
        {
            pending.push(producer);
        }
    };
    for block in func.blocks.iter() {
        if let Some(term) = &block.terminator {
            term.visit_operands(|operand| mark_operand(func, operand, &mut pending));
        }
    }
    while let Some(inst) = pending.pop() {
        func.inst(inst).kind.visit_operands(|operand| mark_operand(func, operand, &mut pending));
    }

    // LLVM's dropLocation rule applies to discarded computations, not to their surviving users.
    // Keep that decision on the removed instruction for debugify; never transfer its location.
    for inst in discardable.iter() {
        if !live.contains(inst) {
            func.inst_mut(inst).metadata.drop_debug_loc(solar_evmir::debug::DebugLocKind::Dropped);
        }
    }

    let mut removed = 0;
    for block in &mut func.blocks {
        // unused, discardable instruction -> removed
        let len = block.instructions.len();
        block.instructions.retain(|&inst| live.contains(inst) || !discardable.contains(inst));
        removed += len - block.instructions.len();
    }
    removed
}

/// A loop that only counts to its exit.
struct DeadLoop {
    preheader: BlockId,
    header: BlockId,
    exit: BlockId,
    blocks: Vec<BlockId>,
}

/// Skips every effect-free counting loop, innermost first, and returns how many it skipped.
fn remove_dead_loops(func: &mut Function, cfg: &Rc<CfgInfo>) -> usize {
    let mut removed = 0;
    let mut loops = LoopAnalyzer::new().analyze_structure_with_cfg(func, Rc::clone(cfg));
    loop {
        let dead = loops.all_loops().find_map(|l| dead_loop(func, l));
        let Some(dead) = dead else { return removed };
        // exit: phi [header: v] -> phi [preheader: v]
        for &inst in &func.blocks[dead.exit].instructions.clone() {
            if let InstKind::Phi(incoming) = &mut func.inst_mut(inst).kind {
                for (block, _) in incoming {
                    if *block == dead.header {
                        *block = dead.preheader;
                    }
                }
            }
        }
        // preheader: jump header -> jump exit
        replace_terminator(func, dead.preheader, Terminator::Jump(dead.exit));
        for block in dead.blocks {
            // LLVM's dropLocation rule, as for any discarded computation.
            for inst in func.blocks[block].instructions.clone() {
                func.inst_mut(inst).metadata.drop_debug_loc(DebugLocKind::Dropped);
            }
            let _ = invalidate_unreachable_block(func, block);
        }
        removed += 1;
        loops = LoopAnalyzer::new().analyze_structure(func);
    }
}

fn dead_loop(func: &Function, l: &Loop) -> Option<DeadLoop> {
    let [latch] = l.back_edges.as_slice() else { return None };
    let preheader = l.preheader?;
    let [exit] = l.exit_blocks.as_slice() else { return None };
    let Some(Terminator::Branch { condition, then_block, else_block }) =
        func.blocks[l.header].terminator
    else {
        return None;
    };
    if else_block != *exit || !l.blocks.contains(then_block) {
        return None;
    }
    // Every block after the header has one predecessor and jumps on inside the loop, so each
    // iteration that passes the header test runs the whole body.
    for block in l.blocks.iter().filter(|&block| block != l.header) {
        if func.blocks[block].predecessors.len() != 1
            || !matches!(func.blocks[block].terminator,
                Some(Terminator::Jump(next)) if l.blocks.contains(next))
        {
            return None;
        }
    }
    let defined = l.instructions(func);
    if defined.iter().any(|inst| func.inst(inst).kind.has_side_effects()) {
        return None;
    }
    let defined_in_loop =
        |value| matches!(func.value(value), Value::Inst(inst) if defined.contains(*inst));
    let mut escapes = false;
    for (block, contents) in func.blocks.iter_enumerated() {
        if l.blocks.contains(block) {
            continue;
        }
        for &inst in &contents.instructions {
            func.inst(inst).kind.visit_operands(|operand| escapes |= defined_in_loop(operand));
        }
        if let Some(term) = &contents.terminator {
            term.visit_operands(|operand| escapes |= defined_in_loop(operand));
        }
    }
    if escapes || !counts_to_exit(func, l, *latch, preheader, condition, &defined_in_loop) {
        return None;
    }
    Some(DeadLoop { preheader, header: l.header, exit: *exit, blocks: l.blocks.iter().collect() })
}

/// Whether the header test `condition` stops a counter that steps by one: `i < n` over an
/// increment, or `i != n` over an increment or decrement, with `n` fixed in the loop.
fn counts_to_exit(
    func: &Function,
    l: &Loop,
    latch: BlockId,
    preheader: BlockId,
    condition: ValueId,
    defined_in_loop: &impl Fn(ValueId) -> bool,
) -> bool {
    let Value::Inst(test) = *func.value(condition) else { return false };
    if !func.blocks[l.header].instructions.contains(&test) {
        return false;
    }
    let (counter, bound, ascending_only) = match func.inst(test).kind {
        InstKind::Lt(counter, bound) => (counter, bound, true),
        InstKind::Ne(a, b) if defined_in_loop(a) => (a, b, false),
        InstKind::Ne(a, b) => (b, a, false),
        _ => return false,
    };
    let Value::Inst(phi) = *func.value(counter) else { return false };
    let InstKind::Phi(incoming) = &func.inst(phi).kind else { return false };
    if defined_in_loop(bound)
        || func.value_ty(counter) != Some(MirType::I256)
        || !func.blocks[l.header].instructions.contains(&phi)
        || incoming.len() != 2
        || !incoming.iter().any(|&(block, _)| block == preheader)
    {
        return false;
    }
    let Some(&(_, next)) = incoming.iter().find(|&&(block, _)| block == latch) else {
        return false;
    };
    let Value::Inst(update) = *func.value(next) else { return false };
    let (step, subtract) = match func.inst(update).kind {
        InstKind::Add(a, b) if a == counter => (b, false),
        InstKind::Add(a, b) if b == counter => (a, false),
        InstKind::Sub(a, b) if a == counter => (b, true),
        _ => return false,
    };
    let Some(step) = func.value_u256(step) else { return false };
    if !defined_in_loop(next) {
        return false;
    }
    let increment = !subtract && step == U256::ONE;
    let decrement = if subtract { step == U256::ONE } else { step == U256::MAX };
    increment || (decrement && !ascending_only)
}
