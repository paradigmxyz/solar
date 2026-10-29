//! CFG Simplification and Normalization passes.
//!
//! This module provides optimization passes to clean up the Control Flow Graph:
//!
//! ## Block Merging
//! If block A unconditionally jumps to B, and B has only A as predecessor,
//! merge A and B into a single block. This reduces jump instructions (one `JUMP` each).
//!
//! ## Empty Block Elimination
//! Remove blocks that contain no instructions and only an unconditional jump,
//! redirecting predecessors to the target.
//!
//! A branch also folds when a chain of unique incoming edges establishes the same SSA condition.
//! The block keeps its instructions; only its terminator changes. Entry blocks and
//! predecessors whose two arms reach the block provide no such fact. The bounded walk stops
//! at the condition definition so loop iterations cannot reuse a prior iteration’s condition.
//! `branch-simplify` runs just this terminator cleanup after lowering, followed by
//! unreachable-block removal. It leaves block merging and terminal sharing to the backend to
//! preserve stack lifetimes.
//!
//! ## Dead Function Elimination
//! Remove functions that are never called, starting from entry points
//! (public/external functions, constructor, fallback, receive).
//!
//! ## Calls That Cannot Return
//! `noreturn-calls` ends each block at its first call to a bodied function that cannot return,
//! such as a shared revert helper, with `invalid` after the call. HIR lowering continues after
//! every call, so without it a failure path through `fail()` joins the code after it, and `if
//! (i >= n) fail();` leaves no `i < n` there for the check, threading, and loop passes. A check
//! or `require` whose condition folds to a failing constant ends its block the same way, so a
//! helper that only fails, like `return new uint256[](0)[0];`, cannot return. The pass runs
//! first in the semantic pipeline, again once CSE has forwarded the loads such checks read, and
//! `lower-evm-shaped` repeats it with tail calls.
//! Returnability follows explicit returns and tail-call chains conservatively: an unreachable
//! return keeps a body returning until CFG cleanup removes it, and a caller worklist propagates
//! the bodies that every path now leaves through such a call.
//!
//! Terminal-block equivalence compares every non-operand instruction field, including semantic
//! layouts and literal payloads, while comparing SSA operands by definition position. It ignores
//! source context. Shared instructions and
//! terminators retain the bounded union of their original locations instead.

use super::utils::{redirect_edge, redirect_successor_predecessors};
use crate::mir::{
    BlockId, Builtin, Callee, Function, FunctionId, Immediate, InstKind, InstructionMetadata,
    MirType, Module, Terminator, Value, ValueId,
    analysis::{CallGraphInfo, CfgInfo, direct_callees},
    pass::{MirPass, run_function_pass},
    utils::{eval, invalidate_unreachable_block, replace_terminator, retain_blocks},
};
use alloy_primitives::U256;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::FxHashMap,
};
use std::cell::OnceCell;

/// Function pass for CFG simplification.
pub(crate) struct CfgSimplify;

impl MirPass for CfgSimplify {
    fn name(&self) -> &'static str {
        "cfg-simplify"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| simplify_function(func))
    }
}

/// Folds known branches after representation lowering exposes repeated conditions.
pub(crate) struct BranchSimplify;

impl MirPass for BranchSimplify {
    fn name(&self) -> &'static str {
        "branch-simplify"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| {
            let changed = simplify_degenerate_terminators(func);
            if changed {
                let _ = remove_unreachable_blocks(func);
            }
            changed
        })
    }
}

/// Cleans a function after a local transform changes its control flow.
pub(super) fn simplify_function(func: &mut Function) -> bool {
    run_to_fixpoint(func)
}

/// Function pass that only replaces trivial phis by their unique incoming value.
///
/// HIR lowering leaves a phi for every local a loop could reassign, including
/// locals it never does. Those phis hide one object behind several SSA names,
/// which alias analysis joins to distinct symbolic pointers, so the loop and
/// memory passes cannot prove that element stores leave a length word alone.
/// Full CFG cleanup this early would also merge and remove the blocks that
/// jump threading later recognizes in short-circuit conditions, so this pass
/// touches nothing but the phis.
pub(crate) struct SimplifyTrivialPhis;

impl MirPass for SimplifyTrivialPhis {
    fn name(&self) -> &'static str {
        "simplify-phis"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| {
            let mut changed = false;
            // Replacing one phi can make the phi it fed trivial in turn.
            while simplify_trivial_phis(func) {
                changed = true;
            }
            changed
        })
    }
}

/// Module pass for dead internal function elimination.
pub(crate) struct FunctionDce;

impl MirPass for FunctionDce {
    fn name(&self) -> &'static str {
        "function-dce"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let call_graph = CallGraphInfo::new(module);
        let reachable = call_graph.reachable_from_entries();
        !reachable.is_empty() && retain_functions(module, reachable) != 0
    }
}

/// Module pass that ends each block at its first call to a function that cannot return.
pub(crate) struct NoreturnCalls;

impl MirPass for NoreturnCalls {
    fn name(&self) -> &'static str {
        "noreturn-calls"
    }

    fn is_contract_wide(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        end_nonreturning_calls(module, false)
    }
}

/// Alpha-equivalence key for a terminal block used by
/// [`deduplicate_terminal_blocks`].
#[derive(Debug, PartialEq, Eq, Hash)]
struct CanonBlock {
    insts: Vec<CanonInst>,
    term_mnemonic: &'static str,
    term_function: Option<FunctionId>,
    term_operands: Vec<CanonOperand>,
}

/// Alpha-equivalence key for one instruction of a terminal block.
#[derive(Debug, PartialEq, Eq, Hash)]
struct CanonInst {
    kind: InstKind,
    operands: Vec<CanonOperand>,
    result_ty: Option<MirType>,
    metadata: InstructionMetadata,
}

/// A canonicalized operand: block-local results compare by definition
/// position, immediates by value, and everything else by exact [`ValueId`].
#[derive(Debug, PartialEq, Eq, Hash)]
enum CanonOperand {
    Local(usize),
    Imm(Immediate),
    Outside(ValueId),
}

#[must_use]
pub(super) fn remove_unreachable_blocks(func: &mut Function) -> usize {
    let cfg = CfgInfo::new(func);
    let order = func.blocks.indices().filter(|&block| cfg.is_reachable(block)).collect::<Vec<_>>();
    let removed = func.blocks.len() - order.len();
    if removed != 0 {
        retain_blocks(func, &order);
    }
    removed
}

/// Runs one round of CFG simplification on a function and returns whether it rewrote anything.
fn run(func: &mut Function) -> bool {
    let mut changed = simplify_degenerate_terminators(func);
    changed |= merge_blocks(func);
    changed |= eliminate_empty_blocks(func);
    changed |= deduplicate_terminal_blocks(func);
    changed |= simplify_trivial_phis(func);
    changed
}

/// Merges identical terminal blocks (no phis, terminator without
/// successors, alpha-equivalent instructions) into one shared block and
/// redirects all predecessor edges to it.
///
/// Checked arithmetic materializes one panic block per check; this folds
/// them to one block per panic code (and shared revert-string blocks) per
/// function. The rewrite is phi-safe by construction: the kept block has
/// no phis and a terminal block has no successors, so no phi inputs
/// elsewhere can mention it.
fn deduplicate_terminal_blocks(func: &mut Function) -> bool {
    // Equal keys imply equal terminator mnemonics and block lengths, so only
    // blocks sharing that shape with another candidate need a key.
    let shape = |block_id: BlockId| {
        let block = &func.blocks[block_id];
        let term = block.terminator.as_ref()?;
        (!block.predecessors.is_empty()
            && !matches!(term, Terminator::Invalid)
            && !term.has_successors())
        .then(|| (term.mnemonic(), block.instructions.len()))
    };
    let mut shapes = FxHashMap::<_, usize>::default();
    for block_id in func.blocks.indices() {
        if let Some(shape) = shape(block_id) {
            *shapes.entry(shape).or_default() += 1;
        }
    }
    let mut kept = FxHashMap::default();
    let mut merges = Vec::new();
    for block_id in func.blocks.indices() {
        if shape(block_id).is_none_or(|shape| shapes[&shape] < 2) {
            continue;
        }
        let Some(canon) = canonicalize_terminal_block(func, block_id) else {
            continue;
        };
        let keep = *kept.entry(canon).or_insert(block_id);
        if keep != block_id {
            merges.push((block_id, keep));
        }
    }

    for &(dup, keep) in &merges {
        // duplicate instructions; terminal -> shared instructions; terminal
        // Merge debug origins without changing the structural equivalence key.
        let origins = func.blocks[keep]
            .instructions
            .iter()
            .zip(&func.blocks[dup].instructions)
            .map(|(&target, &source)| (target, func.inst(source).metadata.debug_context()))
            .collect::<Vec<_>>();
        for (target, metadata) in origins {
            func.inst_mut(target).metadata.merge_debug_context(&metadata);
        }
        let metadata = func.blocks[dup].terminator_metadata.debug_context();
        func.blocks[keep].terminator_metadata.merge_debug_context(&metadata);
        let predecessors = func.blocks[dup].predecessors.to_vec();
        for pred in predecessors {
            redirect_edge(func, pred, dup, keep);
        }
        func.blocks[dup].instructions.clear();
        func.blocks[dup].set_generated_terminator(Terminator::Invalid);
        func.blocks[dup].predecessors.clear();
    }
    !merges.is_empty()
}

/// Builds the alpha-equivalence key of a terminal block, or `None` if the
/// block is not a dedup candidate.
fn canonicalize_terminal_block(func: &Function, block_id: BlockId) -> Option<CanonBlock> {
    let block = &func.blocks[block_id];
    let term = block.terminator.as_ref()?;
    if matches!(term, Terminator::Invalid) || term.has_successors() {
        return None;
    }

    let mut local_defs: FxHashMap<ValueId, usize> = FxHashMap::default();
    for (position, &inst_id) in block.instructions.iter().enumerate() {
        if let Some(result) = func.inst_result_value(inst_id) {
            local_defs.insert(result, position);
        }
    }

    let canon_operand = |value: ValueId| {
        if let Some(&position) = local_defs.get(&value) {
            return CanonOperand::Local(position);
        }
        match func.value(value) {
            Value::Immediate(imm) => CanonOperand::Imm(imm.clone()),
            _ => CanonOperand::Outside(value),
        }
    };

    let mut insts = Vec::with_capacity(block.instructions.len());
    for &inst_id in &block.instructions {
        let inst = func.inst(inst_id);
        if matches!(inst.kind, InstKind::Phi(_)) {
            return None;
        }
        let mut metadata = inst.metadata.clone();
        metadata.set_hir_expr(None);
        metadata.mark_debug_info_dropped();
        metadata.loop_depth = 0;
        insts.push(CanonInst {
            kind: inst.kind.clone_without_operands(),
            operands: inst.kind.operands().into_iter().map(canon_operand).collect(),
            result_ty: inst.result_ty,
            metadata,
        });
    }

    let term_function =
        if let Terminator::TailCall { function, .. } = term { Some(*function) } else { None };
    let term_operands = term.operands().into_iter().map(canon_operand).collect();
    Some(CanonBlock { insts, term_mnemonic: term.mnemonic(), term_function, term_operands })
}

fn simplify_trivial_phis(func: &mut Function) -> bool {
    let mut candidates = Vec::new();
    let mut raw = FxHashMap::default();

    for block_id in func.blocks.indices() {
        let mut same_block_phi_results = None;
        for &inst_id in &func.blocks[block_id].instructions {
            let InstKind::Phi(incoming) = &func.inst(inst_id).kind else {
                continue;
            };
            let Some(phi_value) = func.inst_result_value(inst_id) else {
                continue;
            };
            let same_block_phi_results =
                same_block_phi_results.get_or_insert_with(|| func.block_phi_results(block_id));
            let Some(replacement) =
                trivial_phi_replacement(incoming, phi_value, same_block_phi_results)
            else {
                continue;
            };
            candidates.push((inst_id, phi_value));
            raw.insert(phi_value, replacement);
        }
    }

    if raw.is_empty() {
        return false;
    }

    // A trivial phi may be replaced by another phi deleted in the same
    // batch (`v81 -> v82 -> v80`); uses must be rewritten to the end of
    // the chain or they dangle once the intermediate phi is removed.
    // Mutually-trivial cycles have no outside source; keep those phis.
    let mut replacements = FxHashMap::default();
    let mut dead = DenseBitSet::new_empty(func.num_insts());
    let mut seen = DenseBitSet::new_empty(func.num_values());
    for &(inst_id, phi_value) in &candidates {
        seen.clear();
        seen.insert(phi_value);
        let mut target = raw[&phi_value];
        let mut cyclic = false;
        while let Some(&next) = raw.get(&target) {
            if !seen.insert(target) {
                cyclic = true;
                break;
            }
            target = next;
        }
        if !cyclic {
            replacements.insert(phi_value, target);
            dead.insert(inst_id);
        }
    }

    if replacements.is_empty() {
        return false;
    }

    func.replace_uses(&replacements);
    for block in func.blocks.iter_mut() {
        block.instructions.retain(|&inst_id| !dead.contains(inst_id));
    }
    true
}

fn trivial_phi_replacement(
    incoming: &[(BlockId, ValueId)],
    phi_value: ValueId,
    same_block_phi_results: &DenseBitSet<ValueId>,
) -> Option<ValueId> {
    let mut incoming_values = incoming.iter().map(|(_, value)| *value);
    let first = incoming_values.find(|value| *value != phi_value)?;
    if same_block_phi_results.contains(first) {
        return None;
    }
    incoming_values.all(|value| value == phi_value || value == first).then_some(first)
}

fn simplify_degenerate_terminators(func: &mut Function) -> bool {
    let mut changed = false;
    for block_id in func.blocks.indices() {
        let Some(term) = &func.blocks[block_id].terminator else { continue };
        let terminator = if let Some(target) = known_branch_target(func, block_id) {
            Terminator::Jump(target)
        } else {
            match term {
                Terminator::Branch { then_block, else_block, .. } if then_block == else_block => {
                    Terminator::Jump(*then_block)
                }
                Terminator::Switch { value, default, cases }
                    if cases.last().is_some_and(|(_, target)| target == default) =>
                {
                    let len = cases
                        .iter()
                        .rposition(|(_, target)| target != default)
                        .map_or(0, |i| i + 1);
                    if len == 0 {
                        Terminator::Jump(*default)
                    } else {
                        Terminator::Switch {
                            value: *value,
                            default: *default,
                            cases: cases[..len].to_vec(),
                        }
                    }
                }
                _ => continue,
            }
        };
        // branch/switch -> simplified terminator
        replace_terminator(func, block_id, terminator);
        changed = true;
    }
    changed
}

fn known_branch_target(func: &Function, block: BlockId) -> Option<BlockId> {
    let Terminator::Branch { condition, then_block, else_block } =
        func.blocks[block].terminator.as_ref()?
    else {
        return None;
    };
    let taken = if let Some(value) = func.value_u256(*condition) {
        !value.is_zero()
    } else {
        let definition = match func.value(*condition) {
            Value::Inst(id) => Some(*id),
            _ => None,
        };
        let mut cursor = block;
        let mut known = None;
        for _ in 0..16 {
            if cursor == BlockId::ENTRY
                || definition.is_some_and(|id| func.blocks[cursor].instructions.contains(&id))
            {
                break;
            }
            let [pred] = func.blocks[cursor].predecessors.as_slice() else { break };
            if let Some(Terminator::Branch {
                condition: incoming,
                then_block: incoming_then,
                else_block: incoming_else,
            }) = func.blocks[*pred].terminator.as_ref()
                && incoming == condition
                && incoming_then != incoming_else
            {
                known = Some(*incoming_then == cursor);
                break;
            }
            cursor = *pred;
        }
        known?
    };
    Some(if taken { *then_block } else { *else_block })
}

/// Runs CFG simplification iteratively until no more changes.
fn run_to_fixpoint(func: &mut Function) -> bool {
    if func.blocks.len() == 1
        && !func.block_has_phi(BlockId::ENTRY)
        && !matches!(
            func.blocks[BlockId::ENTRY].terminator,
            Some(Terminator::Branch { .. } | Terminator::Switch { .. })
        )
    {
        return false;
    }

    let mut changed = false;
    while run(func) {
        changed = true;
    }
    changed | (remove_unreachable_blocks(func) != 0)
}

/// Merges blocks where A unconditionally jumps to B and B has only A as predecessor.
///
/// A merge only changes the merged block's terminator and the predecessors of its new successors,
/// so it cannot make an earlier block mergeable, and one forward sweep reaches the fixpoint.
fn merge_blocks(func: &mut Function) -> bool {
    let mut merged = false;
    for block_id in func.blocks.indices() {
        while let Some(target) = can_merge(func, block_id) {
            do_merge(func, block_id, target);
            merged = true;
        }
    }
    merged
}

/// Checks if block_id can be merged with its successor.
/// Returns the target block if merge is possible.
fn can_merge(func: &Function, block_id: BlockId) -> Option<BlockId> {
    let block = &func.blocks[block_id];

    let Terminator::Jump(target) = block.terminator.as_ref()? else {
        return None;
    };

    if *target == block_id {
        return None;
    }

    let target_block = &func.blocks[*target];
    if target_block.predecessors.len() != 1 {
        return None;
    }

    if target_block.predecessors[0] != block_id {
        return None;
    }

    for &inst_id in &target_block.instructions {
        let InstKind::Phi(incoming) = &func.inst(inst_id).kind else {
            continue;
        };
        if !incoming.iter().any(|(pred, _)| *pred == block_id) {
            return None;
        }
    }

    Some(*target)
}

/// Merges block_id with target, appending target's instructions and terminator to block_id.
fn do_merge(func: &mut Function, block_id: BlockId, target: BlockId) {
    let phi_replacements = fold_target_phis_for_merge(func, block_id, target);
    let target_instructions = func.blocks[target]
        .instructions
        .iter()
        .copied()
        .filter(|&inst_id| !matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
        .collect::<Vec<_>>();
    let (terminator, metadata) = func.blocks[target].take_terminator();
    let terminator = terminator.expect("merged block must have a terminator");

    // block: prefix; target_instructions; target_terminator
    func.blocks[block_id].instructions.extend(target_instructions);
    func.blocks[block_id].set_terminator(terminator, metadata);
    redirect_successor_predecessors(func, target, block_id);

    func.blocks[target].instructions.clear();
    func.blocks[target].terminator = Some(Terminator::Invalid);
    func.blocks[target].predecessors.clear();

    func.replace_uses(&phi_replacements);
}

fn fold_target_phis_for_merge(
    func: &Function,
    pred: BlockId,
    target: BlockId,
) -> FxHashMap<ValueId, ValueId> {
    let mut replacements = FxHashMap::default();
    for &inst_id in &func.blocks[target].instructions {
        let InstKind::Phi(incoming) = &func.inst(inst_id).kind else {
            continue;
        };
        let Some(phi_value) = func.inst_result_value(inst_id) else {
            continue;
        };
        let Some((_, incoming_value)) =
            incoming.iter().find(|(incoming_pred, _)| *incoming_pred == pred)
        else {
            continue;
        };
        replacements.insert(phi_value, *incoming_value);
    }
    replacements
}

/// Eliminates empty blocks that only contain an unconditional jump.
fn eliminate_empty_blocks(func: &mut Function) -> bool {
    // Only a forwarder consults the CFG. Eliminating one contracts it into its target, which
    // changes neither reachability nor dominance among the other blocks, so one snapshot
    // answers every round; the eliminated block is never a forwarder again.
    let snapshot = OnceCell::new();
    let mut changed = false;
    let mut eliminated = true;
    while eliminated {
        eliminated = false;

        let cfg = || snapshot.get_or_init(|| CfgInfo::new(func));
        for block_id in func.blocks.indices() {
            if empty_forwarder_target(func, block_id).is_none()
                || (func.blocks[block_id].predecessors.is_empty() && cfg().is_reachable(block_id))
            {
                continue;
            }

            if !is_loop_preheader_forwarder(func, block_id, cfg())
                && forwarder_elimination_preserves_phis(func, block_id)
            {
                eliminate_forwarder(func, block_id);
                eliminated = true;
                changed = true;
                break;
            }
        }
    }
    changed
}

/// Returns the target of an empty block that only jumps to another block.
pub(super) fn empty_forwarder_target(func: &Function, block_id: BlockId) -> Option<BlockId> {
    let block = &func.blocks[block_id];
    match block.terminator {
        Some(Terminator::Jump(target)) if block.instructions.is_empty() && target != block_id => {
            Some(target)
        }
        _ => None,
    }
}

fn is_loop_preheader_forwarder(func: &Function, block_id: BlockId, cfg: &CfgInfo) -> bool {
    let Some(Terminator::Jump(target)) = func.blocks[block_id].terminator else {
        return false;
    };
    if !matches!(
        func.blocks[target].instructions.first(),
        Some(&inst) if matches!(func.inst(inst).kind, InstKind::Phi(_))
    ) {
        return false;
    }

    func.blocks[target]
        .predecessors
        .iter()
        .copied()
        .any(|pred| pred != block_id && cfg.dominators().dominates(target, pred))
}

/// Checks that redirecting the forwarder's predecessors into its target
/// keeps the target's phis well formed: a predecessor must not end up with
/// two incoming entries carrying different values (e.g. both arms of one
/// branch being forwarders into the same join), since phi incoming lists
/// are keyed per predecessor block, not per CFG edge.
fn forwarder_elimination_preserves_phis(func: &Function, block_id: BlockId) -> bool {
    let Some(Terminator::Jump(target)) = func.blocks[block_id].terminator else {
        return false;
    };
    let predecessors = &func.blocks[block_id].predecessors;
    for &inst_id in &func.blocks[target].instructions {
        let InstKind::Phi(incoming) = &func.inst(inst_id).kind else {
            continue;
        };
        let Some(&(_, forwarded)) = incoming.iter().find(|(pred, _)| *pred == block_id) else {
            continue;
        };
        for &pred in predecessors {
            if incoming.iter().any(|&(other, value)| other == pred && value != forwarded) {
                return false;
            }
        }
    }
    true
}

/// Eliminates an empty forwarder block by redirecting its predecessors.
fn eliminate_forwarder(func: &mut Function, block_id: BlockId) {
    let Some(Terminator::Jump(target)) = func.blocks[block_id].terminator else { return };

    let predecessors = func.blocks[block_id].predecessors.to_vec();
    redirect_target_phi_incoming(func, block_id, target, &predecessors);
    for pred in predecessors {
        redirect_edge(func, pred, block_id, target);
    }
    let _ = invalidate_unreachable_block(func, block_id);
}

fn redirect_target_phi_incoming(
    func: &mut Function,
    old_pred: BlockId,
    target: BlockId,
    new_preds: &[BlockId],
) {
    let instruction_count = func.blocks[target].instructions.len();
    for index in 0..instruction_count {
        let inst_id = func.blocks[target].instructions[index];
        let InstKind::Phi(incoming) = &mut func.inst_mut(inst_id).kind else {
            continue;
        };

        let mut rewritten = Vec::with_capacity(incoming.len() + new_preds.len());
        for &(pred, value) in incoming.iter() {
            if pred == old_pred {
                rewritten.extend(new_preds.iter().map(|&new_pred| (new_pred, value)));
            } else {
                rewritten.push((pred, value));
            }
        }
        // The safety check guarantees colliding entries carry equal values;
        // keep one entry per predecessor block.
        let mut seen = Vec::with_capacity(rewritten.len());
        rewritten.retain(|&(pred, _)| {
            if seen.contains(&pred) {
                false
            } else {
                seen.push(pred);
                true
            }
        });
        *incoming = rewritten;
    }
}

/// Removes the `candidates` that no remaining function calls or tail-calls and
/// that are not entry points themselves, renumbering the survivors.
///
/// Unlike [`FunctionDce`], this never touches a function outside `candidates`,
/// so an uncalled function that was never reachable survives. Returns the functions kept when
/// any is removed.
pub(super) fn remove_unreferenced_functions(
    module: &mut Module,
    candidates: &[FunctionId],
) -> Option<DenseBitSet<FunctionId>> {
    let count = module.functions.len();
    let mut referenced = DenseBitSet::new_empty(count);
    if let Some(entry) = module.dispatch_entry() {
        referenced.insert(entry);
    }
    for (id, func) in module.functions.iter_enumerated() {
        if func.is_external_entry() {
            referenced.insert(id);
        }
        for callee in direct_callees(func) {
            referenced.insert(callee);
        }
    }
    let mut keep = DenseBitSet::new_filled(count);
    for &candidate in candidates {
        if !referenced.contains(candidate) {
            keep.remove(candidate);
        }
    }
    (retain_functions(module, &keep) != 0).then_some(keep)
}

/// Keeps exactly the functions in `keep`, renumbering call and tail-call
/// targets. Returns the number of removed functions.
fn retain_functions(module: &mut Module, keep: &DenseBitSet<FunctionId>) -> usize {
    let removed = module.functions.len() - keep.count();
    if removed == 0 {
        return 0;
    }
    let mut remap = index_vec![None; module.functions.len()];
    let mut old_functions = std::mem::take(&mut module.functions)
        .into_iter()
        .map(Some)
        .collect::<IndexVec<FunctionId, _>>();
    let mut functions = IndexVec::with_capacity(keep.count());
    for old_id in keep {
        let function = old_functions[old_id].take().expect("kept function must exist");
        let new_id = functions.push(function);
        remap[old_id] = Some(new_id);
    }
    module.functions = functions;
    module.remap_dispatch_entry(
        module
            .dispatch_entry()
            .map(|entry| remap[entry].expect("kept function cannot be the dispatch entry")),
    );
    module.function_name_index.clear();
    module.function_name_index.extend(
        module.functions.iter_enumerated().map(|(id, function)| (function.name.symbol, id)),
    );

    for func in &mut module.functions {
        func.for_each_instruction_mut(|_, inst| {
            if let InstKind::ICall { function: crate::mir::Callee::Function(function), .. } =
                &mut inst.kind
            {
                *function =
                    remap[*function].expect("kept function cannot call an eliminated function");
            }
        });
        for block in &mut func.blocks {
            if let Some(Terminator::TailCall { function, .. }) = &mut block.terminator {
                *function = remap[*function]
                    .expect("kept function cannot tail-call an eliminated function");
            }
        }
    }

    removed
}

/// Ends each block at its first call to a bodied callee proven unable to return, including a
/// call with results, or at its first check that always fails. With `tail_calls`, a call to a
/// selectorless, non-recursive callee becomes a [`Terminator::TailCall`]; any other call or
/// check stays, followed by an unreachable `invalid`. Returns whether any block changed.
pub(super) fn end_nonreturning_calls(module: &mut Module, tail_calls: bool) -> bool {
    let mut changed = false;
    for func in &mut module.functions {
        changed |= end_failing_checks(func);
    }
    // Skip call-graph analysis when no internal calls remain.
    let has_candidate = module.functions.iter().any(|func| {
        func.instructions().any(|inst_id| {
            matches!(func.inst(inst_id).kind, InstKind::ICall { function: Callee::Function(_), .. })
        })
    });
    if !has_candidate {
        return changed;
    }
    let call_graph = CallGraphInfo::new(module);
    let returning = module.returning_functions();
    let mut nonreturning = DenseBitSet::new_empty(module.functions.len());
    let mut tail_callable = DenseBitSet::new_empty(module.functions.len());
    for (func_id, func) in module.functions.iter_enumerated() {
        if func.blocks.is_empty() {
            continue;
        }
        if !returning.contains(func_id) {
            nonreturning.insert(func_id);
        }
        if tail_calls
            && func.selector.is_none()
            && !func.attributes.is_receive
            && !func.attributes.is_fallback
            && !call_graph.is_recursive(func_id)
        {
            tail_callable.insert(func_id);
        }
    }

    // Keep reverse edges even when cleanup removes a call: stale edges only
    // cause a redundant visit, and function IDs stay stable through block cleanup.
    let mut callers = index_vec![Vec::new(); module.functions.len()];
    for caller in module.functions.indices() {
        for callee in call_graph.callees(caller) {
            callers[callee].push(caller);
        }
    }
    let mut worklist = Vec::new();
    let mut queued = DenseBitSet::new_empty(module.functions.len());
    for callee in &nonreturning {
        for &caller in &callers[callee] {
            if queued.insert(caller) {
                worklist.push(caller);
            }
        }
    }
    while let Some(func_id) = worklist.pop() {
        queued.remove(func_id);
        let func = &mut module.functions[func_id];
        let mut function_changed = false;
        for block_id in func.blocks.indices() {
            let insts = &func.blocks[block_id].instructions;
            let Some((position, function)) =
                insts.iter().enumerate().find_map(|(position, &inst_id)| {
                    let inst = func.inst(inst_id);
                    if let InstKind::ICall { function: Callee::Function(function), .. } = &inst.kind
                        && nonreturning.contains(*function)
                    {
                        Some((position, *function))
                    } else {
                        None
                    }
                })
            else {
                continue;
            };

            let inst = func.inst(insts[position]);
            let metadata = inst.metadata.debug_context();
            let InstKind::ICall { args, .. } = &inst.kind else { unreachable!() };
            if tail_callable.contains(function) {
                // result = icall callee, args -> tail_call callee, args
                let terminator =
                    Terminator::TailCall { function, args: args.iter().copied().collect() };
                func.blocks[block_id].instructions.truncate(position);
                replace_terminator(func, block_id, terminator);
                func.blocks[block_id].terminator_metadata = metadata;
            } else if !end_block_after(func, block_id, position) {
                continue;
            }
            function_changed = true;
        }
        if function_changed {
            let _ = remove_unreachable_blocks(func);
            changed = true;
        }
        if !nonreturning.contains(func_id)
            && !func.blocks.is_empty()
            && !func.blocks.iter().any(|block| match &block.terminator {
                Some(Terminator::Return { .. }) => true,
                Some(Terminator::TailCall { function, .. }) => !nonreturning.contains(*function),
                _ => false,
            })
        {
            nonreturning.insert(func_id);
            for &caller in &callers[func_id] {
                if queued.insert(caller) {
                    worklist.push(caller);
                }
            }
        }
    }
    changed
}

/// Ends each block at its first check or `require` whose condition always fails, with `invalid`
/// after it, so a body that only fails, such as `return new uint256[](0)[0];`, cannot return.
fn end_failing_checks(func: &mut Function) -> bool {
    let mut changed = false;
    for block in func.blocks.indices() {
        let insts = &func.blocks[block].instructions;
        let Some(position) =
            insts.iter().position(|&inst| always_fails(func, &func.inst(inst).kind))
        else {
            continue;
        };
        changed |= end_block_after(func, block, position);
    }
    if changed {
        let _ = remove_unreachable_blocks(func);
    }
    changed
}

/// Ends `block` with `invalid` right after the instruction at `position`. Returns whether the
/// block changed.
fn end_block_after(func: &mut Function, block: BlockId, position: usize) -> bool {
    if position + 1 == func.blocks[block].instructions.len()
        && matches!(func.blocks[block].terminator, Some(Terminator::Invalid))
    {
        return false;
    }
    // call or check
    // invalid
    func.blocks[block].instructions.truncate(position + 1);
    replace_terminator(func, block, Terminator::Invalid);
    // NOTE: The unreachable terminator has no source checkpoint.
    func.blocks[block].terminator_metadata.mark_debug_info_dropped();
    true
}

/// Whether `kind` is a check or `require` whose condition folds to a failing constant.
fn always_fails(func: &Function, kind: &InstKind) -> bool {
    match kind {
        InstKind::ICall { function: Callee::Builtin(Builtin::Check { is_zero, .. }), args } => {
            constant(func, args[0], MAX_FOLD_DEPTH).is_some_and(|value| value.is_zero() == *is_zero)
        }
        InstKind::ICall { function: Callee::Builtin(Builtin::Require(_)), args } => {
            constant(func, args[0], MAX_FOLD_DEPTH).is_some_and(|value| value.is_zero())
        }
        _ => false,
    }
}

/// How many pure instructions [`constant`] folds through.
const MAX_FOLD_DEPTH: u32 = 4;

/// The constant `value` folds to through pure instructions over immediates.
fn constant(func: &Function, value: ValueId, depth: u32) -> Option<U256> {
    if let Some(value) = func.value_u256(value) {
        return Some(value);
    }
    let Value::Inst(inst) = *func.value(value) else { return None };
    if depth == 0 {
        return None;
    }
    eval::eval_typed_inst(func, &func.inst(inst).kind, |operand| {
        constant(func, operand, depth - 1).ok_or(())
    })
    .ok()
    .flatten()
}
