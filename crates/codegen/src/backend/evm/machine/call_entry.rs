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
//!
//! Optimized runtime tail edges may also bypass a static memory-argument entry
//! when every path through a bounded tail-call closure ends in a fixed-range revert.
//! Only direct arithmetic and literal word stores are admitted; memory reads and
//! other observations decline. The omitted argument interval must miss the envelope
//! of all nonempty descendant revert ranges, including ranges in later helpers.
//! Ordinary entries, frame reservations and other incoming edges remain unchanged.
//! Both activations and every descendant must be static and free of spill protocols.
//! Returning activations and module code observations retain the existing global gate.
//! Cached summaries charge at most 256 uncached nonleaf function/block/instruction units
//! per request. Terminal single-block leaves retain the previous source-linear scan once,
//! without spending expansion budget. Cycles use the existing lazy CFG facts. Negative and
//! in-progress entries share a refusal; budget-dependent refusals stay conservative.
//! A checked reconciliation must save bytes without adding gas or stack peak.
//! Size outlining and final layout still require measurement. Nonempty arguments
//! exclude the zero-parameter entry where FMP initialization can be relocated.

use super::{Context, FunctionLayout, Slot, external_argument, materialize, prefix};
use crate::{
    backend::evm::{calls, ir, op, scheduler::Stack, storage::FrameBase},
    mir,
};
use solar_config::{EvmVersion, OptimizationMode};
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

/// Emits a tail edge after argument preparation, keeping setup temporaries out of the scheduler.
pub(super) fn lower_tail(
    context: &Context<'_>,
    callee: mir::FunctionId,
    layouts: &FxHashMap<mir::FunctionId, FunctionLayout>,
    args: &[mir::ValueId],
    incoming: &Stack<Slot>,
    (current, insts): (ir::BlockId, &mut Vec<ir::Instruction>),
    output: &mut ir::Module,
) -> Result<(), String> {
    let mut setup = calls::enter(
        &context.plan.functions[callee],
        context.storage,
        args.len(),
        context.plan.fixed_memory_end,
        context.plan.max_dynamic_frame_size,
        context.version,
    )?;
    let layout = &layouts[&callee];
    let mut target = layout.entry;
    if context.tail_entry_scope.get() != Some(false)
        && let Some((candidate, body)) =
            choose_tail(context, callee, layout, layouts, args, incoming, &setup)
    {
        // <reverse arguments> -> <canonical callee entry>
        // jump <terminal tail body>, omitting only unobserved argument stores
        setup.setup = candidate;
        target = body;
    }
    // <frame argument stores or checked canonical entry schedule>
    // jump <callee entry or fixed reverting body>
    super::emit_call_setup(setup, current, insts, target, output);
    Ok(())
}

/// Reuses the ordinary body's stack contract on an independently safe terminal edge.
/// `setup` has already validated the full argument count and all frame addresses.
fn choose_tail(
    context: &Context<'_>,
    callee: mir::FunctionId,
    layout: &FunctionLayout,
    layouts: &FxHashMap<mir::FunctionId, FunctionLayout>,
    args: &[mir::ValueId],
    incoming: &Stack<Slot>,
    setup: &calls::CallSetup,
) -> Option<(Vec<ir::Instruction>, ir::BlockId)> {
    let storage = &context.plan.functions[callee];
    let function = context.module.function(callee);
    if !matches!(context.optimization, OptimizationMode::Gas | OptimizationMode::Size)
        || context.deployment
        || context.tail_entry_scope.get() == Some(false)
        || context.layout.uses_spill_protocol()
        || context.plan.max_dynamic_frame_size != 0
        || !(1..=12).contains(&args.len())
        || function.params.len() != args.len()
        || !setup.guard.is_empty()
    {
        return None;
    }
    let FrameBase::Static(base) = storage.base else { return None };
    let start = base.checked_add(storage.argument_offset)?;
    let end = base.checked_add(storage.return_offset)?;
    let mut budget = 256;
    let reverts = terminal_reverts(
        context,
        layouts,
        callee,
        &mut context.tail_reverts.borrow_mut(),
        &mut budget,
    )?;
    if reverts.range.is_some_and(|(offset, revert_end)| offset < end && start < revert_end) {
        return None;
    }
    let mut desired = Vec::new();
    for &slot in &layout.entries[mir::BlockId::ENTRY] {
        let Slot::Value(value) = slot else { return None };
        let mir::Value::Arg(index) = function.value(value) else { return None };
        desired.push(Slot::Value(*args.get(index.index())?));
    }
    // <all reversed actual arguments> -> <exact live arguments of the body>
    let candidate = incoming.clone().reconcile(&desired, 0, context.version).ok()?;
    let (required, delta, peak) = ir::scheduling_usage(&candidate)?;
    let count = args.len() as i64;
    if incoming.values().len() != args.len()
        || required > count
        || delta != desired.len() as i64 - count
        || peak > 1
        || count + peak > 1024
    {
        return None;
    }
    let old = tail_cost(context.version, &setup.setup)?;
    let new = tail_cost(context.version, &candidate)?;
    if new.0 > old.0 || new.1 >= old.1 {
        return None;
    }
    let eligible = context.tail_entry_scope.get().unwrap_or_else(|| {
        let observes = context.module.iter_functions().any(|(_, function)| {
            function.instructions().any(|id| {
                matches!(
                    function.inst(id).kind,
                    mir::InstKind::CodeSize
                        | mir::InstKind::CodeCopy(..)
                        | mir::InstKind::ExtCodeSize(_)
                        | mir::InstKind::ExtCodeCopy(..)
                        | mir::InstKind::ExtCodeHash(_)
                        | mir::InstKind::DataCopy(..)
                )
            })
        });
        context.tail_entry_scope.set(Some(!observes));
        !observes
    });
    eligible.then_some((candidate, layout.blocks[mir::BlockId::ENTRY]))
}

/// An envelope of every nonempty REVERT range in a certified terminal closure.
/// `None` denotes only empty reverts, which do not observe memory bytes.
#[derive(Clone, Copy, Default)]
pub(super) struct TailReverts {
    range: Option<(u64, u64)>,
}

impl TailReverts {
    fn include(&mut self, other: Self) {
        if let Some((start, end)) = other.range {
            self.range = Some(self.range.map_or((start, end), |(a, b)| (a.min(start), b.max(end))));
        }
    }
}

/// Certifies effects and terminal observations without changing MIR or frame ownership.
/// A positive closure cannot contain the current Phi trial caller: its queried edge would
/// complete a refused tail cycle. Other spill-free layouts cannot enter a later trial.
fn terminal_reverts(
    context: &Context<'_>,
    layouts: &FxHashMap<mir::FunctionId, FunctionLayout>,
    id: mir::FunctionId,
    cache: &mut FxHashMap<mir::FunctionId, Option<TailReverts>>,
    budget: &mut usize,
) -> Option<TailReverts> {
    if let Some(result) = cache.get(&id) {
        return *result;
    }
    // A recursive tail edge sees the in-progress refusal instead of recursing.
    cache.insert(id, None);
    let result = (|| {
        let function = context.module.function(id);
        let layout = layouts.get(&id)?;
        let storage = &context.plan.functions[id];
        if layout.uses_spill_protocol()
            || layout.returning
            || storage.is_entry
            || storage.stack_arguments
            || storage.address_exposed
            || !matches!(storage.base, FrameBase::Static(_))
            || !storage.deferred_allocations.is_empty()
            || function.attributes.is_constructor
            || external_argument(function)
            || function.blocks.is_empty()
            || function.blocks.len() > 64
        {
            return None;
        }
        // Preserve the existing single-block leaf eligibility without a second scan.
        // Cached leaves are source-linear once, including when reached as descendants.
        let leaf = function.blocks.len() == 1
            && matches!(
                function.blocks[mir::BlockId::ENTRY].terminator,
                Some(mir::Terminator::Revert { .. })
            );
        if !leaf {
            let cost =
                function.blocks.iter().try_fold(1 + function.blocks.len(), |cost, block| {
                    cost.checked_add(block.instructions.len())
                })?;
            *budget = budget.checked_sub(cost)?;
        }
        let mut result = TailReverts::default();
        for block in &function.blocks {
            for &id in &block.instructions {
                let instruction = function.inst(id);
                if instruction
                    .metadata
                    .effect()
                    .is_some_and(|effect| effect != instruction.kind.effect_kind())
                {
                    return None;
                }
                match instruction.kind {
                    mir::InstKind::MStore(address, _) => {
                        function.value_u64(address)?.checked_add(32)?;
                    }
                    _ => {
                        let opcode = instruction.kind.evm_opcode()?;
                        if !matches!(opcode, op::ADD..=op::SIGNEXTEND | op::LT..=op::CLZ) {
                            return None;
                        }
                    }
                }
            }
            match block.terminator.as_ref()? {
                mir::Terminator::Jump(_) | mir::Terminator::Branch { .. } => {}
                mir::Terminator::Revert { offset, size } => {
                    let start = function.value_u64(*offset)?;
                    let size = function.value_u64(*size)?;
                    let end = start.checked_add(size)?;
                    result.include(TailReverts { range: (size != 0).then_some((start, end)) });
                }
                mir::Terminator::TailCall { function, .. } => {
                    result.include(terminal_reverts(context, layouts, *function, cache, budget)?);
                }
                _ => return None,
            }
        }
        // Refused terminators cannot force a CFG traversal through large Switch tables.
        if !leaf && !layout.cfg.cyclic_blocks().is_empty() {
            return None;
        }
        Some(result)
    })();
    cache.insert(id, result);
    result
}

/// Counts the emitted instructions, without simplifying a different trial sequence.
/// Omitting memory expansion from the original stores is conservative.
fn tail_cost(version: EvmVersion, instructions: &[ir::Instruction]) -> Option<(usize, usize)> {
    let mut cost = (0, 0);
    for instruction in instructions {
        let (gas, bytes) = match instruction.kind {
            ir::InstKind::Push(value) => {
                let bytes = op::push_len(version, value);
                (if bytes == 1 { 2 } else { 3 }, bytes)
            }
            ir::InstKind::Dup(1..=16) | ir::InstKind::Swap(1..=16) => (3, 1),
            ir::InstKind::Op(op::POP) => (2, 1),
            ir::InstKind::Op(op::MSTORE) => (3, 1),
            _ => return None,
        };
        cost.0 += gas;
        cost.1 += bytes;
    }
    Some(cost)
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
