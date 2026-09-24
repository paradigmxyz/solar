//! Interprocedural MIR function compaction.
//!
//! This module removes unused internal parameters and results and combines equivalent internal
//! function bodies. These transforms preserve external ABI entry signatures: only direct MIR call
//! edges are rewritten. Before pruning, direct callers reuse an argument or constant when every
//! explicit return in the callee returns that same value. Calls remain in place, including their
//! effects and failure paths. Tail calls, mixed return values, and baked signature-frame addresses
//! prevent forwarding; no recursive summary or control-flow fixed point is needed.
//!
//! Constants must reach a pure instruction or a non-return terminator. Direct stores and returns
//! alone do not justify discarding the call result and pushing the same constant again.
//!
//! A parameter that every direct call passes the same calldata word, an ABI wrapper's lazy
//! argument or a load at a constant offset, is read from calldata in the callee instead when the
//! callee keeps it across a loop, and pruning then drops it. Calldata does not change during a
//! call, so the callee reads the word its callers read. Such a parameter cannot stay a resident
//! stack argument, so every caller would stage it in the callee's static frame for the callee to
//! reread; other parameters already travel on the stack, where the reread would only move the
//! load into the callee.
//!
//! Structural buckets include canonical operand identities and constants, avoiding pairwise
//! comparisons between bodies with the same opcodes but different inputs. Hash collisions still
//! require the exact equivalence check.
//!
//! Equivalent bodies merge their source origins, independently of structural
//! matching, so later lowering cannot attribute shared code to one arbitrary body.
//! Recursive pairs need no call-graph analysis: corresponding calls must target the same
//! function or one of the two bodies being compared. Matching all other instructions, operands
//! and CFG edges closes that pairwise equivalence proof, including mutual recursion.

use crate::mir::{
    ArgIdx, BlockId, Callee, EffectKind, Function, FunctionId, Immediate, InstId, InstKind,
    Instruction, MirType, Module, StorageAlias, Terminator, Value, ValueId,
    analysis::{CfgInfo, Liveness},
    memory::EvmMemoryLayout,
    pass::{MirPass, ModuleAnalyses},
};
use alloy_primitives::U256;
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
/// Result pruning removes a complete SSA result, including a struct, and runs only under `-Osize`.
/// It leaves legacy multi-result signatures intact because their extra results use a shared buffer.
/// Under other objectives argument liveness is already an internal fixed point, so a single pass
/// suffices with no outer iteration.
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
        let forwarded = forward_returned_values(module) + forward_calldata_args(module);
        if !gcx.sess.opts.optimization.is_size() {
            return prune_unused_args(module) != 0 || forwarded != 0;
        }
        let mut changed = forwarded != 0;
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

fn forward_returned_values(module: &mut Module) -> usize {
    let returned = module
        .functions
        .iter_enumerated()
        .filter_map(|(id, func)| {
            let value = returned_value(func)?;
            (is_internal_body(module, id, func) && frame_offsets_are_local(func))
                .then_some((id, value))
        })
        .collect::<FxHashMap<_, _>>();
    if returned.is_empty() {
        return 0;
    }
    let mut forwarded = 0;
    for func in &mut module.functions {
        let calls = func
            .instructions()
            .filter(|&inst| {
                matches!(func.inst(inst).kind, InstKind::ICall { function: Callee::Function(function), .. }
                if returned.contains_key(&function))
            })
            .collect::<Vec<_>>();
        if calls.is_empty() {
            continue;
        }
        let mut used = DenseBitSet::new_empty(func.num_values());
        let mut constant_uses = DenseBitSet::new_empty(func.num_values());
        for block in &func.blocks {
            for &inst in &block.instructions {
                let kind = &func.inst(inst).kind;
                for value in kind.operands() {
                    used.insert(value);
                    if kind.effect_kind() == EffectKind::Pure {
                        constant_uses.insert(value);
                    }
                }
            }
            if let Some(term) = &block.terminator {
                for value in term.operands() {
                    used.insert(value);
                    if !matches!(term, Terminator::Return { .. }) {
                        constant_uses.insert(value);
                    }
                }
            }
        }
        let mut replacements = FxHashMap::default();
        for inst in calls {
            let Some(result) = func.inst_result_value(inst).filter(|&value| used.contains(value))
            else {
                continue;
            };
            let InstKind::ICall { function: Callee::Function(function), args } =
                &func.inst(inst).kind
            else {
                unreachable!()
            };
            let replacement = match &returned[function] {
                ReturnedValue::Argument(arg) => {
                    let Some(&value) = args.get(arg.index()) else { continue };
                    value
                }
                ReturnedValue::Constant(value) => {
                    if !constant_uses.contains(result) {
                        continue;
                    }
                    // callee.constant => caller.constant
                    func.alloc_value(Value::Immediate(value.clone()))
                }
            };
            replacements.insert(result, replacement);
        }
        forwarded += replacements.len();
        // result = icall callee, args; use result => icall callee, args; use returned_value
        func.replace_uses_canonicalized(&replacements);
    }
    forwarded
}

/// Reads each parameter that every direct call passes the same calldata word from calldata in
/// the callee, leaving the parameter unused for pruning.
fn forward_calldata_args(module: &mut Module) -> usize {
    // words[callee][arg] = None before any call, Some(Some(offset)) while every call passes the
    // word at `offset`, and Some(None) once two calls disagree or one passes something else.
    let mut words = module
        .functions
        .iter()
        .map(|func| {
            IndexVec::<ArgIdx, Option<Option<u64>>>::from_vec(vec![None; func.params.len()])
        })
        .collect::<IndexVec<FunctionId, _>>();
    let mut called = DenseBitSet::new_empty(module.functions.len());
    for func in &module.functions {
        let calls = func
            .instructions()
            .filter_map(|inst| match &func.inst(inst).kind {
                InstKind::ICall { function: Callee::Function(function), args } => {
                    Some((*function, &args[..]))
                }
                _ => None,
            })
            .chain(func.blocks.iter().filter_map(|block| match &block.terminator {
                Some(Terminator::TailCall { function, args }) => Some((*function, &args[..])),
                _ => None,
            }));
        for (callee, args) in calls {
            called.insert(callee);
            for (index, &arg) in args.iter().enumerate() {
                let Some(word) = words[callee].get_mut(ArgIdx::new(index)) else { continue };
                let offset = calldata_word(func, arg);
                *word = match *word {
                    None => Some(offset),
                    Some(previous) if previous == offset => Some(previous),
                    Some(_) => Some(None),
                };
            }
        }
    }

    let mut forwarded = 0;
    for func_id in module.functions.indices() {
        let func = module.function(func_id);
        if !has_rewritable_signature(module, func_id, func, called.contains(func_id)) {
            continue;
        }
        let mut offsets = words[func_id]
            .iter_enumerated()
            .filter_map(|(index, word)| {
                let offset = (*word)??;
                (func.params[index] == MirType::I256).then_some((index, offset))
            })
            .collect::<Vec<_>>();
        if offsets.is_empty() {
            continue;
        }
        // Profitability: a parameter used on both sides of a loop but not carried by it stays in
        // the callee's static frame, which every caller fills and the callee rereads; any other
        // parameter already travels on the stack, where rereading calldata saves nothing.
        let loop_carried = loop_carried_args(func);
        offsets.retain(|&(index, _)| loop_carried.contains(index));
        if offsets.is_empty() {
            continue;
        }
        let func = module.function_mut(func_id);
        let mut replacements = FxHashMap::default();
        let mut loads = Vec::new();
        for (index, offset) in offsets {
            // arg => calldataload offset
            let offset = func.alloc_value(Value::Immediate(Immediate::I256(U256::from(offset))));
            let (load, word) = func.alloc_value_inst(
                Instruction::new(InstKind::CalldataLoad(offset), Some(MirType::I256))
                    .with_debug_info_dropped(),
            );
            loads.push(load);
            for value in (0..func.num_values()).map(ValueId::from_usize) {
                if matches!(func.value(value), Value::Arg(arg) if *arg == index) {
                    replacements.insert(value, word);
                }
            }
        }
        forwarded += loads.len();
        func.replace_uses(&replacements);
        func.blocks[BlockId::ENTRY].instructions.splice(0..0, loads);
    }
    forwarded
}

/// The arguments live into some loop header of `func`.
fn loop_carried_args(func: &Function) -> DenseBitSet<ArgIdx> {
    let mut carried = DenseBitSet::new_empty(func.params.len());
    let cfg = CfgInfo::new(func);
    let liveness = Liveness::compute(func);
    for (block, body) in func.blocks.iter_enumerated() {
        if !body.predecessors.iter().any(|&latch| cfg.dominators().dominates(block, latch)) {
            continue;
        }
        for value in liveness.live_in(block).iter() {
            if let Value::Arg(index) = *func.value(value)
                && index.index() < carried.domain_size()
            {
                carried.insert(index);
            }
        }
    }
    carried
}

/// The offset of the calldata word `value` holds: a lazy argument of a runtime ABI wrapper,
/// which the backend loads from the argument's head word after the selector, or a load at a
/// constant offset.
fn calldata_word(func: &Function, value: ValueId) -> Option<u64> {
    match *func.value(value) {
        Value::Arg(index)
            if func.attributes.is_abi_wrapper
                && func.selector.is_some()
                && !func.attributes.is_constructor
                && func.params.is_empty() =>
        {
            u64::try_from(index.index())
                .ok()?
                .checked_mul(EvmMemoryLayout::WORD_SIZE)?
                .checked_add(4)
        }
        Value::Inst(inst) => match func.inst(inst).kind {
            InstKind::CalldataLoad(offset) => {
                u64::try_from(func.value(offset).as_immediate()?.as_u256()?).ok()
            }
            _ => None,
        },
        _ => None,
    }
}

#[derive(PartialEq, Eq)]
enum ReturnedValue {
    Argument(ArgIdx),
    Constant(Immediate),
}

fn returned_value(func: &Function) -> Option<ReturnedValue> {
    if func.return_components().len() != 1 {
        return None;
    }
    let mut returned = None;
    for block in &func.blocks {
        match &block.terminator {
            Some(Terminator::Return { values }) => {
                let [value] = values.as_slice() else { return None };
                let value = match func.value(*value) {
                    Value::Arg(arg) => ReturnedValue::Argument(*arg),
                    Value::Immediate(value) => ReturnedValue::Constant(value.clone()),
                    _ => return None,
                };
                if returned.as_ref().is_some_and(|previous| *previous != value) {
                    return None;
                }
                returned = Some(value);
            }
            Some(Terminator::TailCall { .. }) => return None,
            _ => {}
        }
    }
    returned
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
    let Some(signature_slots) = func.params.len().checked_add(func.return_components().len())
    else {
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
        .checked_add(
            ((func.params.len() + func.return_components().len()) as u64)
                * EvmMemoryLayout::WORD_SIZE,
        )
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
            if let InstKind::ICall { function: Callee::Function(function), args, .. } = kind {
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
            if let InstKind::ICall { function: Callee::Function(function), args, .. } =
                &mut inst.kind
            {
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

/// Removes a complete SSA result when every direct caller discards it.
///
/// Scalar and struct results both have one SSA value. Legacy multi-result signatures still carry
/// extra results through a shared buffer and remain unchanged. Preserve the memory-return flag
/// before erasing the signature, including when a nested struct field refers to memory.
fn prune_unused_returns(module: &mut Module) -> usize {
    let mut called = DenseBitSet::new_empty(module.functions.len());
    for func in &module.functions {
        for inst_id in func.instructions() {
            if let InstKind::ICall { function: Callee::Function(function), .. } =
                func.inst(inst_id).kind
            {
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
            && func.return_components().len() == 1
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
                let InstKind::ICall { function: Callee::Function(function), .. } =
                    caller.inst(inst_id).kind
                else {
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
                    InstKind::ICall { function: Callee::Function(function), .. } if removed_set.contains(function)
                )
            })
            .collect::<Vec<_>>();
        for inst_id in calls {
            func.remove_inst_result(inst_id);
        }
    }

    for func_id in removed_set.iter() {
        let may_return_memory =
            module.type_may_reference_memory(module.function(func_id).return_components()[0]);
        let func = module.function_mut(func_id);
        // Candidates carry exactly one result, so clearing it removes one signature slot.
        func.attributes.may_return_memory |= may_return_memory;
        let removed_slots = func.return_components().len() as u64;
        func.set_return_type(MirType::Void);
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

/// Returns whether discarding `value` can expose only pure instructions and calldata reads to later
/// DCE.
///
/// Other reads are deliberately retained even when their loaded value is otherwise unused: memory
/// reads can expand memory as observed by `msize`, state reads affect warm/cold access costs, and
/// environment reads such as `gas` are directly observable. A calldata read is none of these.
/// Argument leaves are safe here because argument pruning applies this same proof to every
/// concrete call operand and propagates caller argument dependencies to a fixed point.
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
                let removable = inst.kind.effect_kind() == EffectKind::Pure
                    || matches!(inst.kind, InstKind::CalldataLoad(_));
                if !removable
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
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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
        let mut groups = FxHashMap::<u64, Vec<FunctionId>>::default();
        for (func_id, func) in module.functions.iter_enumerated() {
            if !merged.contains(func_id) && is_merge_candidate(module, func_id, func) {
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

fn is_merge_candidate(module: &Module, func_id: FunctionId, func: &Function) -> bool {
    !func.blocks.is_empty() && is_internal_body(module, func_id, func)
}

/// Cheaply partitions functions before the exact pairwise alpha-equivalence check.
fn equivalence_bucket(func: &Function) -> u64 {
    let mut key = FxHasher::default();
    let values = CanonValues::new(func);
    func.params.hash(&mut key);
    func.return_components().hash(&mut key);
    func.internal_frame_size.hash(&mut key);
    func.external_static_return_size.hash(&mut key);
    func.blocks.len().hash(&mut key);
    for block in &func.blocks {
        block.instructions.len().hash(&mut key);
        for &inst_id in &block.instructions {
            let inst = func.inst(inst_id);
            inst.kind.mnemonic().hash(&mut key);
            inst.result_ty.hash(&mut key);
            for operand in inst.kind.operands() {
                values.value(operand).hash(&mut key);
            }
        }
        block.terminator.as_ref().map_or("none", Terminator::mnemonic).hash(&mut key);
        if let Some(term) = &block.terminator {
            for operand in term.operands() {
                values.value(operand).hash(&mut key);
            }
        }
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
        || lhs.return_components() != rhs.return_components()
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
                || lhs_inst.metadata.preserves_fmp() != rhs_inst.metadata.preserves_fmp()
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
    let mut rhs_operands = rhs_operands.into_iter();
    for operand in lhs_operands {
        if !matches!(
            (lhs.value(operand), rhs_operands.next().and_then(|value| rhs.value(value))),
            (Some(lhs), Some(rhs)) if lhs == rhs
        ) {
            return false;
        }
    }
    rhs_operands.next().is_none()
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
        // Assembly is what can break the object-length bound, so a merge keeps it visible.
        && lhs.attributes.inline_assembly == rhs.attributes.inline_assembly
        && lhs.attributes.is_function_pointer_dispatcher
            == rhs.attributes.is_function_pointer_dispatcher
        && lhs.attributes.no_inline == rhs.attributes.no_inline
        && lhs.attributes.preserves_array_elements == rhs.attributes.preserves_array_elements
        && lhs.attributes.returns_param_elements == rhs.attributes.returns_param_elements
        // A proved element width is part of what callers rely on: merging a body
        // whose address array is proved canonical into one that is not would make
        // its callers re-clean every returned element.
        && lhs.attributes.array_element_bits == rhs.attributes.array_element_bits
        && lhs.attributes.array_return_element_bits == rhs.attributes.array_return_element_bits
}

/// Compares the non-operand fields of two instructions. Operands are zeroed because their
/// alpha-equivalent identities were compared separately.
fn equivalent_inst_payload(
    lhs_id: FunctionId,
    lhs: &InstKind,
    rhs_id: FunctionId,
    rhs: &InstKind,
) -> bool {
    let lhs = lhs.clone_without_operands();
    let mut rhs = rhs.clone_without_operands();
    if let (
        InstKind::ICall { function: Callee::Function(lhs_target), .. },
        InstKind::ICall { function: Callee::Function(rhs_target), .. },
    ) = (&lhs, &mut rhs)
        && (*lhs_target == lhs_id || *lhs_target == rhs_id)
        && (*rhs_target == lhs_id || *rhs_target == rhs_id)
    {
        *rhs_target = *lhs_target;
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
        && (*lhs_target == lhs_id || *lhs_target == rhs_id)
        && (*rhs_target == lhs_id || *rhs_target == rhs_id)
    {
        *rhs_target = *lhs_target;
    }
    lhs == rhs
}

fn redirect_calls(module: &mut Module, replacements: &FxHashMap<FunctionId, FunctionId>) {
    for func in &mut module.functions {
        func.for_each_instruction_mut(|_, inst| {
            if let InstKind::ICall { function: Callee::Function(function), .. } = &mut inst.kind
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
