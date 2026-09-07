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
//! Runtime gas-mode tail edges may also bypass a static memory-argument entry
//! for a single fixed-range reverting block. Only pure arithmetic and literal
//! word stores are admitted; the revert range must miss every omitted argument
//! word. Both activations must have no spill protocol, and dynamic frames and
//! returning activations and module code observations decline. Returning calls
//! can leave physical stack heights unknown; retaining their argument stores
//! preserves the later compact-literal pass's temporary stack budget. This uses
//! the existing reachability analysis. Frame reservations and other callers stay
//! unchanged. A checked reconciliation must save caller bytes without adding gas
//! or stack peak; no saving from the shared entry is credited. Nonempty parameters
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
    layout: &FunctionLayout,
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
    let mut target = layout.entry;
    if context.tail_entry_scope.get() != Some(false)
        && let Some((candidate, body)) =
            choose_tail(context, callee, layout, args, incoming, &setup)
    {
        // <reverse arguments> -> <canonical callee entry>
        // jump <fixed reverting body>, omitting only unobserved argument stores
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
    args: &[mir::ValueId],
    incoming: &Stack<Slot>,
    setup: &calls::CallSetup,
) -> Option<(Vec<ir::Instruction>, ir::BlockId)> {
    let storage = &context.plan.functions[callee];
    let function = context.module.function(callee);
    if !context.optimization.is_gas()
        || context.deployment
        || context.tail_entry_scope.get() == Some(false)
        || context.layout.uses_spill_protocol()
        || layout.uses_spill_protocol()
        || context.plan.max_dynamic_frame_size != 0
        || layout.returning
        || storage.is_entry
        || storage.stack_arguments
        || storage.address_exposed
        || !storage.deferred_allocations.is_empty()
        || function.attributes.is_constructor
        || external_argument(function)
        || function.blocks.len() != 1
        || !(1..=12).contains(&args.len())
        || function.params.len() != args.len()
        || !setup.guard.is_empty()
    {
        return None;
    }
    let FrameBase::Static(base) = storage.base else { return None };
    let start = base.checked_add(storage.argument_offset)?;
    let end = base.checked_add(storage.return_offset)?;
    let mir::Terminator::Revert { offset, size } =
        function.blocks[mir::BlockId::ENTRY].terminator.as_ref()?
    else {
        return None;
    };
    let offset = function.value_u64(*offset)?;
    let size = function.value_u64(*size)?;
    let revert_end = offset.checked_add(size)?;
    if size != 0 && offset < end && start < revert_end {
        return None;
    }
    for id in function.instructions() {
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
