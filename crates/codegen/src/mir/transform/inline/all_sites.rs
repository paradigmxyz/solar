//! Callees that `-Osize` inlines at every call site and deletes.
//!
//! A callee with one call site is consumed when its protocol and body outweigh the lost
//! sharing. A callee with several sites needs the judgment for the whole set, because each
//! inline lowers the count that the next site sees: three calls of a small helper look like
//! two, then one, and no single site pays for the copies. The set is priced once, before a
//! round, as `Target::helper_sharing` prices a shared helper: the call protocol of every
//! site and one body, against one body per site plus the code the backend stops sharing with
//! other functions. A callee with a tail call or an unreachable caller keeps its body and is
//! left out.
//!
//! The model prices a body from its operations alone, so only arithmetic leaves and forwarders
//! qualify: bodies of operations that each lower to one opcode, with branches but no phis,
//! selects, or switches, whose joins cost stack moves the model omits, and bodies that only call
//! another function. For bodies that move memory or storage,
//! or return a memory object, the estimate omits the stack moves and the expansion of later
//! lowerings, and the copies grew the corpus's bytecode. Copies of a leaf also fold against the
//! constants and ranges of their sites, which the model does not credit. Callees with more than
//! [`MAX_SITES`] sites stay shared: the estimate's error grows with every copy. Inlining a body
//! drops its price, as the callee it became no longer matches the body that was priced.

use super::{MirInlineSummary, MirInliner, is_terminal_call, surviving_call_words};
use crate::{
    mir::{
        Callee, EffectKind, Function, FunctionId, InstKind, MirType, Module, Terminator,
        analysis::Liveness,
    },
    target::Cost,
};
use solar_data_structures::map::{FxHashMap, FxHashSet};

/// The most call sites of a callee that is inlined everywhere.
const MAX_SITES: usize = 8;

/// The callees whose inlining at every call site leaves the bytecode smaller. Empty unless the
/// objective is size.
pub(super) fn consumed_everywhere(
    inliner: &MirInliner,
    module: &Module,
    summaries: &FxHashMap<FunctionId, MirInlineSummary>,
    call_counts: &FxHashMap<FunctionId, usize>,
) -> FxHashSet<FunctionId> {
    let Some(shared_runs) = &inliner.shared_runs else { return FxHashSet::default() };
    let internal_calls = internal_call_counts(module);
    let mut consumed = call_counts
        .iter()
        .filter(|&(&callee, &sites)| {
            let func = module.function(callee);
            let Some(summary) = summaries.get(&callee) else { return false };
            if !(2..=MAX_SITES).contains(&sites)
                || internal_calls.get(&callee) != Some(&sites)
                || func
                    .return_components()
                    .iter()
                    .any(|&ty| matches!(ty, MirType::MEMPTR | MirType::Slice(_)))
                || !(is_arithmetic_leaf(module, func) || summary.is_transparent_forwarder)
            {
                return false;
            }
            let (shared, expanded) = inliner.target.helper_sharing(
                body_bytes(inliner, module, func),
                func.params.len(),
                func.return_components().len(),
                sites as u32,
            );
            let lost = shared_runs.shared_bytes(&inliner.target, module, callee) as u32;
            inliner.target.cmp(expanded + Cost::new(0, lost), shared).is_le()
        })
        .map(|(&callee, _)| callee)
        .collect::<FxHashSet<_>>();
    drop_stack_bound(module, summaries, &mut consumed);
    consumed
}

/// Removes the callees that overflow the stack budget at any of their sites. The inline loop
/// skips such a site alone, which would leave copies beside the callee the price assumed gone.
fn drop_stack_bound(
    module: &Module,
    summaries: &FxHashMap<FunctionId, MirInlineSummary>,
    consumed: &mut FxHashSet<FunctionId>,
) {
    if consumed.is_empty() {
        return;
    }
    for func in &module.functions {
        let mut liveness = None;
        for block in func.blocks.indices() {
            for (index, &inst) in func.blocks[block].instructions.iter().enumerate() {
                if let InstKind::ICall { function: Callee::Function(callee), .. } =
                    func.inst(inst).kind
                    && consumed.contains(&callee)
                    && let Some(peak) = summaries[&callee].phi_stack_peak
                {
                    let liveness =
                        liveness.get_or_insert_with(|| Liveness::compute_live_sets(func));
                    if surviving_call_words(func, liveness, block, index) + peak
                        > MirInliner::STACK_BUDGET
                    {
                        consumed.remove(&callee);
                    }
                }
            }
        }
    }
}

/// The number of `icall` sites of each function in the module. It differs from the call count
/// of a function that is also tail-called, or called from an unreachable function.
fn internal_call_counts(module: &Module) -> FxHashMap<FunctionId, usize> {
    let mut counts = FxHashMap::<FunctionId, usize>::default();
    for func in &module.functions {
        for inst in func.instructions() {
            if let InstKind::ICall { function: Callee::Function(callee), .. } = func.inst(inst).kind
            {
                *counts.entry(callee).or_default() += 1;
            }
        }
    }
    counts
}

/// Whether every operation of `func` is pure or a call that never returns, and no block joins
/// values, switches, or returns more than one word along more than one path.
fn is_arithmetic_leaf(module: &Module, func: &Function) -> bool {
    func.return_components().iter().all(|ty| ty.is_word())
        && func.return_components().len() <= 1
        && func
            .blocks
            .iter()
            .filter(|block| matches!(block.terminator, Some(Terminator::Return { .. })))
            .count()
            == 1
        && func.instructions().all(|inst| {
            let kind = &func.inst(inst).kind;
            is_terminal_call(module, kind)
                || (!matches!(
                    kind,
                    InstKind::ICall { .. } | InstKind::Phi(_) | InstKind::Select(..)
                ) && kind.effect_kind() == EffectKind::Pure)
        })
        && func
            .blocks
            .iter()
            .all(|block| !matches!(block.terminator, Some(Terminator::Switch { .. })))
}

/// The bytes of one copy of `func`: its operations with their immediates, the jump of a call
/// that never returns, and the jumps between its blocks. A conditional branch costs one jump,
/// the other edge falling through, and a return costs none, since the code after the call
/// follows the copy.
fn body_bytes(inliner: &MirInliner, module: &Module, func: &Function) -> u32 {
    let target = &inliner.target;
    let mut bytes = 0;
    for block in func.blocks.indices() {
        bytes += target.block_instructions_estimate(func, block).bytes;
        bytes += match &func.blocks[block].terminator {
            Some(Terminator::Jump(_)) => target.jump().bytes,
            Some(Terminator::Branch { .. }) => target.branch().bytes,
            Some(terminator @ Terminator::Switch { .. }) => {
                target.branch().bytes * terminator.successors().len() as u32
            }
            _ => 0,
        };
    }
    for inst in func.instructions() {
        let kind = &func.inst(inst).kind;
        if let InstKind::ICall { function: Callee::Function(callee), args } = kind {
            let moves = target.dup().times(args.len() as u32);
            bytes += if is_terminal_call(module, kind) {
                (target.jump() + moves).bytes
            } else {
                let results = module.function(*callee).return_components().len() as u32;
                (target.label_push() + target.jump() + moves + target.dup().times(results)).bytes
            };
        }
    }
    bytes
}
