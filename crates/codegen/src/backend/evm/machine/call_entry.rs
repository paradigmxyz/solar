//! Combines a call's argument rotation with its callee's canonical entry order.
//!
//! Stack-argument callees normally rearrange reverse ABI argument order in a
//! shared entry block. For spill-free callees, the caller can instead reconcile
//! directly to that exact canonical stack and jump to the first MIR block. The
//! suspended caller, saved writer words and continuation remain unchanged; no
//! memory setup or restoration is bypassed. Other callers keep the ordinary
//! shared entry. Arguments have already been materialized by the actual emitter.
//!
//! The candidate uses the existing physical scheduler, with the complete caller
//! prefix fixed. It must shrink caller instruction bytes without increasing
//! executed stack gas or the original combined input/peak requirement. Callee
//! entry construction is shared with the ordinary emitter, including duplicate
//! argument identities. Spill-bearing callers retain their existing protocol.
//! Final jump widths, layout and outlining still require corpus measurement.

use super::{Context, FunctionLayout, Slot};
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
    caller_len: usize,
    original: &[ir::Instruction],
) -> Option<(Vec<ir::Instruction>, ir::BlockId)> {
    if matches!(context.optimization, OptimizationMode::None)
        || !context.plan.functions[callee].stack_arguments
        || !layout.spills.homes.is_empty()
        || !context.layout.spills.homes.is_empty()
    {
        return None;
    }
    let continuation = *incoming.values().last()?;
    if !matches!(continuation, Slot::CallLabel(_)) {
        return None;
    }
    let (mut entry_stack, entry_values) = entry(context.module.function(callee), layout)?;
    // <reverse argument order> -> <canonical callee entry>
    let entry_insts = entry_stack.reconcile(&entry_values, 1, context.version).ok()?;
    let mut desired = incoming.values().get(..caller_len)?.to_vec();
    for slot in entry_values {
        desired.push(match slot {
            Slot::ReturnAddress => continuation,
            Slot::Argument(index) => Slot::Value(*args.get(index.index())?),
            _ => return None,
        });
    }
    // <suspended caller>; <args>; <continuation>
    // -> <same suspended caller>; <exact canonical callee entry>
    let candidate = incoming.clone().reconcile(&desired, caller_len, context.version).ok()?;
    let mut combined = original.to_vec();
    combined.extend(entry_insts);
    let old_usage = ir::scheduling_usage(&combined)?;
    let new_usage = ir::scheduling_usage(&candidate)?;
    if new_usage.0 > old_usage.0 || new_usage.1 != old_usage.1 || new_usage.2 > old_usage.2 {
        return None;
    }
    let old_caller = ir::scheduling_cost(context.version, original);
    let old = ir::scheduling_cost(context.version, &combined);
    let new = ir::scheduling_cost(context.version, &candidate);
    // Entry swaps can cancel with the first body shuffle. Require a caller-byte
    // saving instead of relying only on the estimated cost of that shared entry.
    (new.1 < old_caller.1 && new.0 <= old.0)
        .then_some((candidate, layout.blocks[mir::BlockId::ENTRY]))
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
