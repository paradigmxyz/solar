//! Combines argument preparation and continuation insertion with callee entry order.
//!
//! Stack-argument callees normally rearrange reverse ABI argument order in a
//! shared entry block. For spill-free callees, the caller can instead reconcile
//! directly to that exact canonical stack and jump to the first MIR block. The
//! suspended caller, saved writer words and continuation remain unchanged; no
//! memory setup or restoration is bypassed. Other callers keep the ordinary
//! shared entry. Candidate schedules use the ordinary argument materializer.
//!
//! The candidate uses the existing physical scheduler, keeps the activation's
//! return label fixed and restores the exact original suspended caller prefix.
//! It must shrink caller instruction bytes without increasing executed stack gas
//! or the original combined input/peak requirement. Callee
//! entry construction is shared with the ordinary emitter, including duplicate
//! argument identities. The existing post-preparation entry fusion is evaluated
//! first; complete-boundary candidates must improve that incumbent or keep it.
//! One additional absent-argument order puts the final target word first, allowing
//! a single top swap to insert the continuation when all arguments are absent.
//! It runs only when its emitted materialization order differs, skips resident and
//! duplicate identities, and uses the same full schedule and profitability checks.
//! Spill-bearing callers retain their existing protocol. Final jump widths, layout and outlining
//! still require corpus measurement.

use super::{Context, FunctionLayout, Slot, materialize, prefix};
use crate::{
    backend::evm::{ir, scheduler::Stack},
    mir,
};
use solar_config::OptimizationMode;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};

/// Builds the shared stack-argument entry contract before emitting any stores.
pub(super) fn entry(
    function: &mir::Function,
    layout: &FunctionLayout,
) -> Option<(Stack<Slot>, Vec<Slot>)> {
    let mut incoming = vec![Slot::ReturnAddress];
    incoming.extend(function.params.indices().rev().map(Slot::Argument));
    let mut desired = Vec::new();
    for &slot in &layout.entries[mir::BlockId::ENTRY] {
        desired.push(match slot {
            Slot::Value(value) => {
                let mir::Value::Arg(index) = function.value(value) else { return None };
                Slot::Argument(*index)
            }
            slot => slot,
        });
    }
    Some((Stack::new(incoming), desired))
}

pub(super) fn choose(
    context: &Context<'_>,
    callee: mir::FunctionId,
    layout: &FunctionLayout,
    args: &[mir::ValueId],
    incoming: &Stack<Slot>,
    (continuation, caller): (ir::BlockId, &[Slot]),
    (original, rotation_start): (&[ir::Instruction], usize),
) -> Option<(Vec<ir::Instruction>, ir::BlockId)> {
    if matches!(context.optimization, OptimizationMode::None)
        || !context.plan.functions[callee].stack_arguments
        || layout.uses_spill_protocol()
        || context.layout.uses_spill_protocol()
    {
        return None;
    }
    let continuation_slot = Slot::CallLabel(continuation);
    let (mut entry_stack, entry_values) = entry(context.module.function(callee), layout)?;
    // <reverse argument order> -> <canonical callee entry>
    let mut combined = original.to_vec();
    combined.extend(entry_stack.reconcile(&entry_values, 1, context.version).ok()?);
    let mut desired = caller.to_vec();
    for slot in entry_values {
        desired.push(match slot {
            Slot::ReturnAddress => continuation_slot,
            Slot::Argument(index) => Slot::Value(*args.get(index.index())?),
            _ => return None,
        });
    }
    let improves = |candidate: &[ir::Instruction],
                    previous: &[ir::Instruction],
                    combined: &[ir::Instruction]| {
        let Some(old_usage) = ir::scheduling_usage(combined) else { return false };
        let Some(new_usage) = ir::scheduling_usage(candidate) else { return false };
        if new_usage.0 > old_usage.0 || new_usage.1 != old_usage.1 || new_usage.2 > old_usage.2 {
            return false;
        }
        let old_caller = ir::scheduling_cost(context.version, previous);
        // Combined is exactly the caller followed by its optional entry rotation.
        let old = if previous.len() == combined.len() {
            old_caller
        } else {
            ir::scheduling_cost(context.version, combined)
        };
        let new = ir::scheduling_cost(context.version, candidate);
        // Entry swaps can cancel with the first body shuffle. Require a caller-byte
        // saving instead of relying only on the estimated cost of that shared entry.
        new.1 < old_caller.1 && new.0 <= old.0
    };
    let mut prepared = caller.to_vec();
    prepared.extend(args.iter().rev().copied().map(Slot::Value));
    prepared.push(continuation_slot);
    // <already prepared arguments>; <continuation>
    // -> <same suspended caller>; <exact canonical callee entry>
    let mut incumbent = Stack::new(prepared)
        .reconcile(&desired, caller.len(), context.version)
        .ok()
        .filter(|candidate| {
            improves(candidate, &original[rotation_start..], &combined[rotation_start..])
        })
        .map(|candidate| {
            original[..rotation_start].iter().cloned().chain(candidate).collect::<Vec<_>>()
        });
    let operands = args.iter().copied().map(Slot::Value).collect::<Vec<_>>();
    let reordered =
        rotated_materialization(&desired[caller.len() + 1..], &operands, incoming.values());
    for operands in std::iter::once(operands.as_slice()).chain(reordered.as_deref()) {
        let complete = (|| {
            let mut stack = incoming.clone();
            let mut candidate = Vec::new();
            materialize(context, &mut stack, &mut candidate, operands).ok()?;
            // <original stack>; <materialized arguments>; push <continuation>
            // -> <same suspended caller>; <exact canonical callee entry>
            candidate.push(ir::InstKind::PushLabel(continuation).into());
            stack.push(continuation_slot);
            candidate.extend(stack.reconcile(&desired, prefix(context), context.version).ok()?);
            Some(candidate)
        })();
        let previous = incumbent.as_deref().unwrap_or(original);
        let combined = incumbent.as_deref().unwrap_or(&combined);
        if let Some(candidate) = complete
            && improves(&candidate, previous, combined)
        {
            incumbent = Some(candidate);
        }
    }
    incumbent.map(|candidate| (candidate, layout.blocks[mir::BlockId::ENTRY]))
}

/// Returns a distinct pop-order materialization request for the rotated target.
fn rotated_materialization<T: Copy + Eq>(
    desired: &[T],
    original: &[T],
    resident: &[T],
) -> Option<Vec<T>> {
    let (&last, rest) = desired.split_last()?;
    let mut emitted = Vec::new();
    // <last target argument>; <preceding target arguments>; <continuation>
    // swap N -> <continuation>; <target arguments>, when all arguments are absent
    for &value in std::iter::once(&last).chain(rest) {
        if !resident.contains(&value) && !emitted.contains(&value) {
            emitted.push(value);
        }
    }
    let original = original.iter().enumerate().rev().filter_map(|(index, &value)| {
        (!resident.contains(&value) && !original[index + 1..].contains(&value)).then_some(value)
    });
    if emitted.iter().copied().eq(original) {
        return None;
    }
    emitted.reverse();
    Some(emitted)
}

/// Omits generated entry stubs whose addresses were never used during lowering.
/// All machine block-address producers are explicit labels or terminator targets;
/// numeric MIR function identities and data/program relocations are not entries.
/// This construction-time fact does not weaken generic computed-jump analysis.
pub(super) fn prune_unused(
    module: &mut ir::Module,
    layouts: &FxHashMap<mir::FunctionId, FunctionLayout>,
) {
    if !module.private_control_labels {
        return;
    }
    let mut referenced = DenseBitSet::new_empty(module.blocks.len());
    if let Some(entry) = module.block_ids().next() {
        referenced.insert(entry);
    }
    for id in module.block_ids() {
        let block = &module.blocks[id];
        for inst in &block.insts {
            if let ir::InstKind::PushLabel(target) = inst.kind {
                referenced.insert(target);
            }
        }
        match &block.terminator.kind {
            ir::TerminatorKind::Jump(target) => {
                referenced.insert(*target);
            }
            ir::TerminatorKind::JumpI(yes, no) => {
                referenced.insert(*yes);
                referenced.insert(*no);
            }
            ir::TerminatorKind::IndexedJump(targets) => {
                for &target in targets {
                    referenced.insert(target);
                }
            }
            _ => {}
        }
    }
    let mut unused = DenseBitSet::new_empty(module.blocks.len());
    for layout in layouts.values() {
        if !referenced.contains(layout.entry) {
            unused.insert(layout.entry);
        }
    }
    if !unused.is_empty() {
        // <existing block order, excluding never-addressed generated entry stubs>
        module.layout = Some(module.block_ids().filter(|&id| !unused.contains(id)).collect());
    }
}

#[cfg(test)]
mod tests {
    use super::rotated_materialization;

    #[test]
    fn rotated_absent_arguments_and_identical_orders() {
        assert_eq!(rotated_materialization(&[0, 1, 2], &[0, 1, 2], &[]), Some(vec![1, 0, 2]));
        assert_eq!(rotated_materialization(&[0, 1, 2], &[0, 1, 2], &[0]), None);
        assert_eq!(rotated_materialization(&[0, 1, 2], &[0, 1, 2], &[0, 1, 2]), None);
        assert_eq!(rotated_materialization(&[0, 1], &[0, 1], &[]), None);
        assert_eq!(rotated_materialization(&[0, 0, 1], &[0, 0, 1], &[]), None);
        assert_eq!(rotated_materialization(&[0, 1, 0], &[0, 1, 0], &[]), None);
        assert_eq!(rotated_materialization(&[0], &[0, 1], &[]), Some(vec![0]));
        assert_eq!(rotated_materialization(&[0], &[0, 1], &[1]), None);
        assert_eq!(rotated_materialization::<u8>(&[], &[], &[]), None);
    }
}
