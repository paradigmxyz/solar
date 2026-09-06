//! Interprocedural MIR function compaction.
//!
//! This module removes unused internal parameters and results and combines equivalent internal
//! function bodies. These transforms preserve external ABI entry signatures: only direct MIR call
//! edges are rewritten.
//! Equivalent bodies merge their source origins, independently of structural
//! matching, so later lowering cannot attribute shared code to one arbitrary body.

use crate::{
    analysis::CallGraphInfo,
    memory::EvmMemoryLayout,
    mir::{
        ArgIdx, EffectKind, Function, FunctionId, Immediate, InstId, InstKind, MirType, Module,
        StorageAlias, Terminator, Value, ValueId,
    },
    pass::{MirPass, ModuleAnalyses},
};
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::IndexVec,
    map::{FxHashMap, FxHasher},
};
use std::hash::{Hash, Hasher};

type ArgDependents = IndexVec<FunctionId, IndexVec<ArgIdx, Vec<(FunctionId, ArgIdx)>>>;

/// Removes dead internal arguments and results together, in the shape of LLVM's dead-argument
/// elimination: removing a dead result can strand the arguments that only fed it, and removing a
/// dead argument can strand the call whose result kept another function's result live, so the two
/// analyses are iterated to a joint fixed point.
///
/// The component analyses stay distinct rather than sharing one lattice because MIR represents only
/// a single-word result as an SSA value — additional results travel through the ephemeral
/// multi-return buffer — so result pruning is restricted to one-word signatures and, like today,
/// runs only under `-Osize`. Under other objectives argument liveness is already an internal fixed
/// point, so a single pass suffices with no outer iteration.
pub(crate) struct DeadArgElim;

impl MirPass for DeadArgElim {
    fn name(&self) -> &'static str {
        "dead-arg-elim"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut ModuleAnalyses,
    ) -> bool {
        if !gcx.sess.opts.optimization.is_size() {
            return prune_unused_args(module) != 0;
        }
        let mut changed = false;
        loop {
            let pruned = prune_unused_args(module) + prune_unused_returns(module);
            if pruned == 0 {
                break;
            }
            changed = true;
        }
        changed
    }
}

/// Redirects calls to alpha-equivalent internal function bodies.
pub(crate) struct MergeEquivalentFunctions;

impl MirPass for MergeEquivalentFunctions {
    fn name(&self) -> &'static str {
        "merge-equivalent-functions"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut ModuleAnalyses,
    ) -> bool {
        merge_equivalent_functions(module) != 0
    }
}

/// Returns whether `func` has a private MIR signature that direct callers may rewrite.
fn is_internal_body(module: &Module, func_id: FunctionId, func: &Function) -> bool {
    func.selector.is_none()
        && !func.is_public()
        && !func.attributes.is_constructor
        && !func.attributes.is_fallback
        && !func.attributes.is_receive
        && module.dispatch_entry() != Some(func_id)
}

fn has_rewritable_signature(
    module: &Module,
    func_id: FunctionId,
    func: &Function,
    is_called: bool,
) -> bool {
    is_called
        && !func.params.is_empty()
        && func.params.len() == func.arg_indices().count()
        && is_internal_body(module, func_id, func)
        && frame_offsets_are_local(func)
}

/// Returns whether every baked frame address belongs to the local region that moves with the
/// signature prefix. Parsed MIR may explicitly address a parameter, result, or frame-header slot;
/// those accesses do not identify which signature component they refer to, so changing that
/// function's signature would be ambiguous.
fn frame_offsets_are_local(func: &Function) -> bool {
    let Some(signature_slots) = func.params.len().checked_add(func.returns.len()) else {
        return false;
    };
    let Some(signature_size) = u64::try_from(signature_slots)
        .ok()
        .and_then(|slots| slots.checked_mul(EvmMemoryLayout::WORD_SIZE))
    else {
        return false;
    };
    let Some(local_start) = EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE.checked_add(signature_size)
    else {
        return false;
    };
    let local_end = if func.internal_frame_size == 0 {
        None
    } else {
        let Some(end) = local_start.checked_add(func.internal_frame_size) else { return false };
        Some(end)
    };

    func.instructions().all(|inst_id| match func.inst(inst_id).kind {
        InstKind::InternalFrameAddr(offset) => {
            offset >= local_start && local_end.is_none_or(|end| offset < end)
        }
        _ => true,
    })
}

/// Rebases every static-frame address in `func` after `removed_slots` leading parameter or result
/// slots were removed from its signature.
///
/// `local_memory_addr` bakes each frame-local address as `header + (params + returns) * word +
/// local_offset`, so every `InternalFrameAddr` offset lands in the locals region above the
/// signature prefix. Shrinking the signature shifts that prefix — and with it every local — down by
/// one word per removed slot, so the baked offsets must move too or they would point at a stale
/// slot: the backend recomputes the frame layout and spill base from the current parameter and
/// result counts, so leaving the offsets high aliases the locals onto spill slots or a nested
/// callee frame. The removed parameters and results are dead, so their own slots (if any) never
/// remain referenced and `internal_frame_size` is unchanged; the shift is a uniform subtraction,
/// mirroring the inliner's frame-prefix rebasing.
fn rebase_frame_offsets(func: &mut Function, removed_slots: u64) {
    if removed_slots == 0 {
        return;
    }
    let shift = removed_slots * EvmMemoryLayout::WORD_SIZE;
    let local_start = EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
        .checked_add(((func.params.len() + func.returns.len()) as u64) * EvmMemoryLayout::WORD_SIZE)
        .expect("MIR frame prefix overflow");
    let old_local_start = local_start.checked_add(shift).expect("MIR frame prefix overflow");
    let local_end = (func.internal_frame_size != 0).then(|| {
        local_start.checked_add(func.internal_frame_size).expect("MIR local frame region overflow")
    });
    let name = func.name;
    func.for_each_instruction_mut(|_, inst| {
        if let InstKind::InternalFrameAddr(offset) = &mut inst.kind {
            assert!(
                *offset >= old_local_start,
                "frame-local offset precedes the old signature prefix: \
                 func=`{name}` offset={offset} old_local_start={old_local_start} \
                 removed={removed_slots}"
            );
            *offset = offset
                .checked_sub(shift)
                .expect("frame-local offset precedes the removed signature prefix");
            assert!(
                *offset >= local_start,
                "rebased frame-local offset precedes the locals region"
            );
            assert!(
                local_end.is_none_or(|end| *offset < end),
                "rebased frame-local offset exceeds the locals region"
            );
        }
    });
}

/// Computes the least fixed point of argument liveness and removes every argument outside it.
fn prune_unused_args(module: &mut Module) -> usize {
    let mut called = DenseBitSet::new_empty(module.functions.len());
    let mut live = module
        .functions
        .iter()
        .map(|func| DenseBitSet::<ArgIdx>::new_empty(func.params.len()))
        .collect::<IndexVec<FunctionId, _>>();
    let mut dependents = module
        .functions
        .iter()
        .map(|func| {
            let mut args = IndexVec::with_capacity(func.params.len());
            for _ in &func.params {
                args.push(Vec::<(FunctionId, ArgIdx)>::new());
            }
            args
        })
        .collect::<IndexVec<FunctionId, IndexVec<ArgIdx, _>>>();

    // Record direct uses and argument-forwarding dependencies in one scan. A dependency points
    // from the callee argument whose liveness is required to the caller argument that supplies it.
    for (func_id, func) in module.functions.iter_enumerated() {
        for inst_id in func.instructions() {
            let kind = &func.inst(inst_id).kind;
            if let InstKind::ICall { function, args, .. } = kind {
                called.insert(*function);
                record_arg_dependencies(func_id, func, *function, args, &mut live, &mut dependents);
            } else {
                for operand in kind.operands() {
                    mark_arg_live(func, operand, &mut live[func_id]);
                }
            }
        }
        for block in &func.blocks {
            let Some(term) = &block.terminator else { continue };
            if let Terminator::TailCall { function, args } = term {
                called.insert(*function);
                record_arg_dependencies(func_id, func, *function, args, &mut live, &mut dependents);
            } else {
                for operand in term.operands() {
                    mark_arg_live(func, operand, &mut live[func_id]);
                }
            }
        }
    }

    // An argument is live when it participates in a non-call operation, or when it is forwarded
    // into a live callee argument. Starting candidate arguments dead computes transitive deadness,
    // including arguments forwarded around an otherwise-unused recursive cycle.
    for (func_id, func) in module.functions.iter_enumerated() {
        if !has_rewritable_signature(module, func_id, func, called.contains(func_id)) {
            live[func_id].insert_all();
        }
    }
    let mut worklist = Vec::new();
    for (func_id, args) in live.iter_enumerated() {
        for index in args.iter() {
            worklist.push((func_id, index));
        }
    }
    while let Some((func_id, index)) = worklist.pop() {
        for &(dependent_func, dependent_arg) in &dependents[func_id][index] {
            if dependent_arg.index() < live[dependent_func].domain_size()
                && live[dependent_func].insert(dependent_arg)
            {
                worklist.push((dependent_func, dependent_arg));
            }
        }
    }
    if live.iter().all(|args| args.count() == args.domain_size()) {
        return 0;
    }

    let removed = live.iter().map(|args| args.domain_size() - args.count()).sum();
    if removed == 0 {
        return 0;
    }

    // Rewrite every edge before rewriting argument identities in the callees. Any computation
    // that fed a removed argument remains in the caller; ordinary DCE may remove it only when its
    // effects permit that.
    let mut removed_call_operands = 0usize;
    for func in &mut module.functions {
        func.for_each_instruction_mut(|_, inst| {
            if let InstKind::ICall { function, args, .. } = &mut inst.kind {
                let old_len = args.len();
                *args = args
                    .iter()
                    .enumerate()
                    .filter_map(|(index, &arg)| {
                        live[*function].contains(ArgIdx::new(index)).then_some(arg)
                    })
                    .collect();
                removed_call_operands += old_len - args.len();
            }
        });
        for block in &mut func.blocks {
            if let Some(Terminator::TailCall { function, args }) = &mut block.terminator {
                let old_len = args.len();
                *args = args
                    .iter()
                    .enumerate()
                    .filter_map(|(index, &arg)| {
                        live[*function].contains(ArgIdx::new(index)).then_some(arg)
                    })
                    .collect();
                removed_call_operands += old_len - args.len();
            }
        }
    }

    let function_ids = module.functions.indices();
    for func_id in function_ids {
        if live[func_id].count() == live[func_id].domain_size() {
            continue;
        }
        let func = module.function_mut(func_id);

        let old_params = func.params.clone();
        let mut remap = IndexVec::<ArgIdx, Option<ArgIdx>>::with_capacity(old_params.len());
        let mut new_params = IndexVec::with_capacity(old_params.len());
        for (index, &ty) in old_params.iter_enumerated() {
            remap.push(live[func_id].contains(index).then(|| new_params.push(ty)));
        }

        for index in 0..func.num_values() {
            let value = ValueId::from_usize(index);
            let Value::Arg(old_index) = func.value(value) else { continue };
            let old_index = old_index.to_owned();
            *func.value_mut(value) =
                remap[old_index].map(Value::Arg).unwrap_or(Value::Undef(old_params[old_index]));
        }
        let removed_slots = (old_params.len() - new_params.len()) as u64;
        func.set_params(new_params);
        rebase_frame_offsets(func, removed_slots);
    }

    tracing::debug!(
        target: "solar::codegen::function_compaction",
        removed_parameters = removed,
        removed_call_operands,
        "pruned unused internal arguments"
    );
    removed
}

fn mark_arg_live(func: &Function, value: ValueId, live: &mut DenseBitSet<ArgIdx>) -> bool {
    let Value::Arg(index) = func.value(value) else { return false };
    if index.index() >= live.domain_size() {
        return false;
    }
    live.insert(*index)
}

fn record_arg_dependencies(
    caller_id: FunctionId,
    caller: &Function,
    callee_id: FunctionId,
    args: &[ValueId],
    live: &mut IndexVec<FunctionId, DenseBitSet<ArgIdx>>,
    dependents: &mut ArgDependents,
) {
    for (index, &arg) in args.iter().enumerate() {
        let callee_arg = ArgIdx::new(index);
        if !value_dependencies_are_pure(caller, arg) {
            if callee_arg.index() < live[callee_id].domain_size() {
                live[callee_id].insert(callee_arg);
            } else {
                // A malformed extra call operand is diagnosed by validation, but retaining the
                // caller dependency keeps this optional transform conservative before that point.
                mark_arg_live(caller, arg, &mut live[caller_id]);
            }
            continue;
        }
        let Value::Arg(caller_arg) = caller.value(arg) else { continue };
        if let Some(callee_dependents) = dependents[callee_id].get_mut(callee_arg) {
            callee_dependents.push((caller_id, *caller_arg));
        } else {
            // A malformed extra call operand was conservatively treated as live by the fixed-point
            // implementation. Validation rejects it later, but preserve that behavior here.
            mark_arg_live(caller, arg, &mut live[caller_id]);
        }
    }
}

/// Removes a one-word result when every direct caller discards it.
///
/// MIR represents only the first internal-call result as an SSA value. Additional results travel
/// through the ephemeral multi-return buffer, so changing an arbitrary component would require
/// making that buffer protocol explicit in the IR. Restricting this transform to one-word
/// signatures keeps the proof local and still removes the complete return slot and call-result
/// protocol for common effect-only helpers.
fn prune_unused_returns(module: &mut Module) -> usize {
    let mut called = DenseBitSet::new_empty(module.functions.len());
    for func in &module.functions {
        for inst_id in func.instructions() {
            if let InstKind::ICall { function, .. } = func.inst(inst_id).kind {
                called.insert(function);
            }
        }
        for block in &func.blocks {
            if let Some(Terminator::TailCall { function, .. }) = &block.terminator {
                called.insert(*function);
            }
        }
    }

    let mut candidates = DenseBitSet::new_empty(module.functions.len());
    for (func_id, func) in module.functions.iter_enumerated() {
        if called.contains(func_id)
            && func.returns.len() == 1
            && is_internal_body(module, func_id, func)
            && frame_offsets_are_local(func)
            && returned_value_dependencies_are_pure(func)
        {
            candidates.insert(func_id);
        }
    }
    // A result is required (`live`) for every function except the candidates whose result the
    // fixed point may still discard.
    let mut live = DenseBitSet::new_empty(module.functions.len());
    live.insert_all();
    for func_id in candidates.iter() {
        live.remove(func_id);
    }

    // Tail calls forward their caller's result contract. Direct calls require a result only when
    // its SSA value has a material use. A return in a candidate whose own result is still dead is
    // not a material use, which lets discarded forwarding chains collapse to a fixed point.
    loop {
        let mut changed = false;
        for (caller_id, caller) in module.functions.iter_enumerated() {
            for inst_id in caller.instructions() {
                let InstKind::ICall { function, .. } = caller.inst(inst_id).kind else {
                    continue;
                };
                if !candidates.contains(function) || live.contains(function) {
                    continue;
                }
                let Some(result) = caller.inst_result_value(inst_id) else { continue };
                if has_material_use(caller, result, live.contains(caller_id)) {
                    live.insert(function);
                    changed = true;
                }
            }
            for block in &caller.blocks {
                if let Some(Terminator::TailCall { function, .. }) = &block.terminator {
                    if candidates.contains(*function)
                        && !live.contains(*function)
                        && live.contains(caller_id)
                    {
                        live.insert(*function);
                        changed = true;
                    }
                    // The reverse direction: a candidate tail-caller cannot drop its
                    // return contract while the callee still delivers a result, or the
                    // rewritten call sites would disagree with the value the tail
                    // callee leaves behind.
                    if live.contains(*function)
                        && candidates.contains(caller_id)
                        && !live.contains(caller_id)
                    {
                        live.insert(caller_id);
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }

    let mut removed_set = DenseBitSet::new_empty(module.functions.len());
    for func_id in candidates.iter() {
        if !live.contains(func_id) {
            removed_set.insert(func_id);
        }
    }
    if removed_set.is_empty() {
        return 0;
    }

    for func in &mut module.functions {
        let calls = func
            .instructions()
            .filter(|&inst_id| {
                matches!(
                    func.inst(inst_id).kind,
                    InstKind::ICall { function, .. } if removed_set.contains(function)
                )
            })
            .collect::<Vec<_>>();
        for inst_id in calls {
            let InstKind::ICall { returns, .. } = &mut func.inst_mut(inst_id).kind else {
                unreachable!()
            };
            *returns = 0;
            func.remove_inst_result(inst_id);
        }
    }

    for func_id in removed_set.iter() {
        let func = module.function_mut(func_id);
        // Candidates carry exactly one result, so clearing it removes one signature slot.
        func.attributes.may_return_memory |= func.returns[0].is_memory_reference();
        let removed_slots = func.returns.len() as u64;
        func.returns.clear();
        for block in &mut func.blocks {
            if let Some(Terminator::Return { values }) = &mut block.terminator {
                values.clear();
            }
        }
        rebase_frame_offsets(func, removed_slots);
    }

    let removed = removed_set.count();
    tracing::debug!(
        target: "solar::codegen::function_compaction",
        removed_returns = removed,
        "pruned unused internal results"
    );
    removed
}

/// Returns whether discarding `value` can expose only pure instructions to later DCE.
///
/// Reads are deliberately retained even when their loaded value is otherwise unused: memory reads
/// can expand memory as observed by `msize`, state reads affect warm/cold access costs, and
/// environment reads such as `gas` are directly observable. Argument leaves are safe here because
/// argument pruning applies this same proof to every concrete call operand and propagates caller
/// argument dependencies to a fixed point.
fn value_dependencies_are_pure(func: &Function, value: ValueId) -> bool {
    let mut seen = DenseBitSet::new_empty(func.num_values());
    let mut worklist = vec![value];
    while let Some(value) = worklist.pop() {
        if !seen.insert(value) {
            continue;
        }
        match func.value(value) {
            Value::Arg(_) | Value::Immediate(_) | Value::Undef(_) => {}
            Value::Error(_) => return false,
            Value::Inst(inst_id) => {
                let inst = func.inst(*inst_id);
                if inst.kind.effect_kind() != EffectKind::Pure
                    || inst.metadata.effect().is_some_and(|effect| effect != EffectKind::Pure)
                {
                    return false;
                }
                worklist.extend(inst.kind.operands());
            }
        }
    }
    true
}

fn returned_value_dependencies_are_pure(func: &Function) -> bool {
    func.blocks.iter().all(|block| {
        let Some(Terminator::Return { values }) = &block.terminator else { return true };
        values.iter().all(|&value| value_dependencies_are_pure(func, value))
    })
}

fn has_material_use(func: &Function, value: ValueId, function_result_live: bool) -> bool {
    if func.instructions().any(|inst_id| func.inst(inst_id).kind.operands().contains(&value)) {
        return true;
    }
    func.blocks.iter().any(|block| {
        block.terminator.as_ref().is_some_and(|term| match term {
            Terminator::Return { values } if !function_result_live => {
                values.contains(&value) && values.len() != 1
            }
            _ => term.operands().contains(&value),
        })
    })
}

/// A source-independent operand identity used for alpha-equivalence.
#[derive(Clone, Debug, PartialEq, Eq)]
enum CanonValue {
    Arg(ArgIdx),
    Inst(usize),
    Immediate(Immediate),
    Undef(MirType),
}

/// A source-independent storage alias identity.
#[derive(Clone, Debug, PartialEq, Eq)]
enum CanonStorageAlias {
    Slot(alloy_primitives::U256),
    Symbolic(CanonValue),
    Offset { base: CanonValue, offset: alloy_primitives::U256 },
}

struct CanonValues<'a> {
    func: &'a Function,
    active_insts: FxHashMap<InstId, usize>,
}

impl<'a> CanonValues<'a> {
    fn new(func: &'a Function) -> Self {
        let active_insts =
            func.instructions().enumerate().map(|(index, inst)| (inst, index)).collect();
        Self { func, active_insts }
    }

    fn value(&self, value: ValueId) -> Option<CanonValue> {
        Some(match self.func.value(value) {
            Value::Arg(index) => CanonValue::Arg(*index),
            Value::Inst(inst) => CanonValue::Inst(*self.active_insts.get(inst)?),
            Value::Immediate(immediate) => CanonValue::Immediate(immediate.clone()),
            Value::Undef(ty) => CanonValue::Undef(*ty),
            Value::Error(_) => return None,
        })
    }

    fn operands(&self, operands: impl IntoIterator<Item = ValueId>) -> Option<Vec<CanonValue>> {
        operands.into_iter().map(|value| self.value(value)).collect()
    }

    fn storage_alias(&self, alias: StorageAlias) -> Option<CanonStorageAlias> {
        Some(match alias {
            StorageAlias::Slot(slot) => CanonStorageAlias::Slot(slot),
            StorageAlias::Symbolic(value) => CanonStorageAlias::Symbolic(self.value(value)?),
            StorageAlias::Offset { base, offset } => {
                CanonStorageAlias::Offset { base: self.value(base)?, offset }
            }
        })
    }
}

/// Redirects one wave of equivalent functions, then repeats because merging leaf callees can make
/// their callers equivalent on the next wave. Dead-function elimination follows this pass in the
/// canonical pipeline and removes redirected bodies.
fn merge_equivalent_functions(module: &mut Module) -> usize {
    let mut merged = DenseBitSet::new_empty(module.functions.len());
    let mut total = 0;
    let mut merged_instructions = 0usize;

    loop {
        // Redirecting one merge wave changes the graph observed by the next wave. Recompute SCC
        // information so recursion eligibility never relies on the pre-redirect call graph.
        let recursive = CallGraphInfo::new(module);
        let mut groups = FxHashMap::<u64, Vec<FunctionId>>::default();
        for (func_id, func) in module.functions.iter_enumerated() {
            if !merged.contains(func_id) && is_merge_candidate(module, func_id, func, &recursive) {
                groups.entry(equivalence_bucket(func)).or_default().push(func_id);
            }
        }

        let mut replacements = FxHashMap::default();
        for candidates in groups.values() {
            let mut representatives = Vec::new();
            for &candidate in candidates {
                if let Some(&representative) = representatives.iter().find(|&&representative| {
                    equivalent_functions(
                        representative,
                        module.function(representative),
                        candidate,
                        module.function(candidate),
                    )
                }) {
                    replacements.insert(candidate, representative);
                } else {
                    representatives.push(candidate);
                }
            }
        }
        if replacements.is_empty() {
            break;
        }

        total += replacements.len();
        merged_instructions += replacements
            .keys()
            .map(|&duplicate| module.function(duplicate).instructions().count())
            .sum::<usize>();
        for (&duplicate, &representative) in &replacements {
            merge_function_debug_origins(module, duplicate, representative);
            merged.insert(duplicate);
        }
        redirect_calls(module, &replacements);
    }

    if total != 0 {
        tracing::debug!(
            target: "solar::codegen::function_compaction",
            merged_functions = total,
            merged_instructions,
            "merged equivalent internal functions"
        );
    }
    total
}

/// Unions metadata of bodies already proven instruction-for-instruction equivalent.
fn merge_function_debug_origins(
    module: &mut Module,
    duplicate: FunctionId,
    representative: FunctionId,
) {
    let source = module.function(duplicate);
    let origins = source
        .blocks
        .iter()
        .map(|block| {
            (
                block
                    .instructions
                    .iter()
                    .map(|&id| source.inst(id).metadata.debug_context())
                    .collect::<Vec<_>>(),
                block.terminator_metadata.debug_context(),
            )
        })
        .collect::<Vec<_>>();
    let target = module.function_mut(representative);
    // duplicate body -> representative body !metadata(union of body origins)
    for (block, (instructions, terminator)) in target.blocks.indices().zip(origins) {
        for (index, metadata) in instructions.into_iter().enumerate() {
            let inst = target.blocks[block].instructions[index];
            target.inst_mut(inst).metadata.merge_debug_context(&metadata);
        }
        target.blocks[block].terminator_metadata.merge_debug_context(&terminator);
    }
}

fn is_merge_candidate(
    module: &Module,
    func_id: FunctionId,
    func: &Function,
    calls: &CallGraphInfo,
) -> bool {
    !func.blocks.is_empty()
        && is_internal_body(module, func_id, func)
        && (!calls.is_recursive(func_id) || has_only_direct_self_recursion(func_id, func, calls))
}

/// Recursive equivalence is local when every recursive edge is a direct self edge. Mutual SCCs
/// need a whole-component isomorphism proof and remain conservatively excluded.
fn has_only_direct_self_recursion(
    func_id: FunctionId,
    func: &Function,
    calls: &CallGraphInfo,
) -> bool {
    let mut saw_self = false;
    for inst_id in func.instructions() {
        if let InstKind::ICall { function, .. } = func.inst(inst_id).kind {
            if function == func_id {
                saw_self = true;
            } else if calls.is_recursive(function) {
                return false;
            }
        }
    }
    for block in &func.blocks {
        if let Some(Terminator::TailCall { function, .. }) = &block.terminator {
            if *function == func_id {
                saw_self = true;
            } else if calls.is_recursive(*function) {
                return false;
            }
        }
    }
    saw_self
}

/// Cheaply partitions functions before the exact pairwise alpha-equivalence check.
fn equivalence_bucket(func: &Function) -> u64 {
    let mut key = FxHasher::default();
    func.params.hash(&mut key);
    func.returns.hash(&mut key);
    func.internal_frame_size.hash(&mut key);
    func.external_static_return_size.hash(&mut key);
    func.blocks.len().hash(&mut key);
    for block in &func.blocks {
        for &inst_id in &block.instructions {
            let inst = func.inst(inst_id);
            inst.kind.mnemonic().hash(&mut key);
            inst.result_ty.hash(&mut key);
        }
        block.terminator.as_ref().map_or("none", Terminator::mnemonic).hash(&mut key);
    }
    key.finish()
}

fn equivalent_functions(
    lhs_id: FunctionId,
    lhs: &Function,
    rhs_id: FunctionId,
    rhs: &Function,
) -> bool {
    if lhs.params != rhs.params
        || lhs.returns != rhs.returns
        || lhs.abi_returns != rhs.abi_returns
        || lhs.internal_frame_size != rhs.internal_frame_size
        || lhs.external_static_return_size != rhs.external_static_return_size
        || lhs.blocks.len() != rhs.blocks.len()
        || !equivalent_attributes(lhs, rhs)
        || !lhs
            .arg_indices()
            .map(|index| lhs.arg_ty(index))
            .eq(rhs.arg_indices().map(|index| rhs.arg_ty(index)))
    {
        return false;
    }

    let lhs_values = CanonValues::new(lhs);
    let rhs_values = CanonValues::new(rhs);
    for (lhs_block, rhs_block) in lhs.blocks.iter().zip(&rhs.blocks) {
        if lhs_block.instructions.len() != rhs_block.instructions.len() {
            return false;
        }
        for (&lhs_inst, &rhs_inst) in lhs_block.instructions.iter().zip(&rhs_block.instructions) {
            let lhs_inst = lhs.inst(lhs_inst);
            let rhs_inst = rhs.inst(rhs_inst);
            if lhs_inst.result_ty != rhs_inst.result_ty
                || !equivalent_operands(
                    &lhs_values,
                    lhs_inst.kind.operands(),
                    &rhs_values,
                    rhs_inst.kind.operands(),
                )
                || !equivalent_inst_payload(lhs_id, &lhs_inst.kind, rhs_id, &rhs_inst.kind)
                || lhs_inst.metadata.memory_region() != rhs_inst.metadata.memory_region()
                || lhs_inst.metadata.effect() != rhs_inst.metadata.effect()
                || lhs_inst.metadata.unchecked() != rhs_inst.metadata.unchecked()
                || lhs_inst.metadata.deferred_alloc() != rhs_inst.metadata.deferred_alloc()
                || !equivalent_storage_aliases(
                    &lhs_values,
                    lhs_inst.metadata.storage_alias(),
                    &rhs_values,
                    rhs_inst.metadata.storage_alias(),
                )
            {
                return false;
            }
        }

        let (lhs_term, rhs_term) = match (&lhs_block.terminator, &rhs_block.terminator) {
            (None, None) => continue,
            (Some(lhs), Some(rhs)) => (lhs, rhs),
            _ => return false,
        };
        if !equivalent_operands(&lhs_values, lhs_term.operands(), &rhs_values, rhs_term.operands())
            || !equivalent_terminator_payload(lhs_id, lhs_term, rhs_id, rhs_term)
        {
            return false;
        }
    }
    true
}

fn equivalent_operands(
    lhs: &CanonValues<'_>,
    lhs_operands: impl IntoIterator<Item = ValueId>,
    rhs: &CanonValues<'_>,
    rhs_operands: impl IntoIterator<Item = ValueId>,
) -> bool {
    matches!(
        (lhs.operands(lhs_operands), rhs.operands(rhs_operands)),
        (Some(lhs), Some(rhs)) if lhs == rhs
    )
}

fn equivalent_storage_aliases(
    lhs: &CanonValues<'_>,
    lhs_alias: Option<StorageAlias>,
    rhs: &CanonValues<'_>,
    rhs_alias: Option<StorageAlias>,
) -> bool {
    match (lhs_alias, rhs_alias) {
        (None, None) => true,
        (Some(lhs_alias), Some(rhs_alias)) => matches!(
            (lhs.storage_alias(lhs_alias), rhs.storage_alias(rhs_alias)),
            (Some(lhs_alias), Some(rhs_alias)) if lhs_alias == rhs_alias
        ),
        _ => false,
    }
}

fn equivalent_attributes(lhs: &Function, rhs: &Function) -> bool {
    lhs.attributes.visibility == rhs.attributes.visibility
        && lhs.attributes.state_mutability == rhs.attributes.state_mutability
        && lhs.attributes.is_constructor == rhs.attributes.is_constructor
        && lhs.attributes.is_fallback == rhs.attributes.is_fallback
        && lhs.attributes.is_receive == rhs.attributes.is_receive
        && lhs.attributes.may_return_memory == rhs.attributes.may_return_memory
        && lhs.attributes.is_function_pointer_dispatcher
            == rhs.attributes.is_function_pointer_dispatcher
        && lhs.attributes.no_inline == rhs.attributes.no_inline
}

/// Compares the non-operand fields of two instructions. Operands are zeroed because their
/// alpha-equivalent identities were compared separately.
fn equivalent_inst_payload(
    lhs_id: FunctionId,
    lhs: &InstKind,
    rhs_id: FunctionId,
    rhs: &InstKind,
) -> bool {
    let mut lhs = lhs.clone();
    let mut rhs = rhs.clone();
    let zero = ValueId::from_usize(0);
    lhs.visit_operands_mut(|value| *value = zero);
    rhs.visit_operands_mut(|value| *value = zero);
    if let (
        InstKind::ICall { function: lhs_target, .. },
        InstKind::ICall { function: rhs_target, .. },
    ) = (&lhs, &mut rhs)
        && *lhs_target == lhs_id
        && *rhs_target == rhs_id
    {
        *rhs_target = lhs_id;
    }
    lhs == rhs
}

/// Compares CFG targets and other non-operand terminator fields.
fn equivalent_terminator_payload(
    lhs_id: FunctionId,
    lhs: &Terminator,
    rhs_id: FunctionId,
    rhs: &Terminator,
) -> bool {
    let mut lhs = lhs.clone();
    let mut rhs = rhs.clone();
    let zero = ValueId::from_usize(0);
    let lhs_replacements = lhs.operands().into_iter().map(|value| (value, zero)).collect();
    let rhs_replacements = rhs.operands().into_iter().map(|value| (value, zero)).collect();
    crate::mir::utils::replace_terminator_uses(&mut lhs, &lhs_replacements);
    crate::mir::utils::replace_terminator_uses(&mut rhs, &rhs_replacements);
    if let (
        Terminator::TailCall { function: lhs_target, .. },
        Terminator::TailCall { function: rhs_target, .. },
    ) = (&lhs, &mut rhs)
        && *lhs_target == lhs_id
        && *rhs_target == rhs_id
    {
        *rhs_target = lhs_id;
    }
    lhs == rhs
}

fn redirect_calls(module: &mut Module, replacements: &FxHashMap<FunctionId, FunctionId>) {
    for func in &mut module.functions {
        func.for_each_instruction_mut(|_, inst| {
            if let InstKind::ICall { function, .. } = &mut inst.kind
                && let Some(&replacement) = replacements.get(function)
            {
                *function = replacement;
            }
        });
        for block in &mut func.blocks {
            if let Some(Terminator::TailCall { function, .. }) = &mut block.terminator
                && let Some(&replacement) = replacements.get(function)
            {
                *function = replacement;
            }
        }
    }
}
