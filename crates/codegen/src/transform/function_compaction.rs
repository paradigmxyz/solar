//! Interprocedural MIR function compaction.
//!
//! This module removes unused internal parameters and combines equivalent internal function
//! bodies. Both transforms preserve external ABI entry signatures: only direct MIR call edges are
//! rewritten.

use crate::{
    analysis::CallGraphInfo,
    mir::{
        ArgIdx, Function, FunctionId, Immediate, InstId, InstKind, MirType, Module, StorageAlias,
        Terminator, Value, ValueId,
    },
    pass::{MirPass, ModuleAnalyses},
};
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};

/// Removes parameters that are unused throughout the direct internal-call graph.
pub(crate) struct PruneUnusedArgs;

impl MirPass for PruneUnusedArgs {
    fn name(&self) -> &'static str {
        "prune-unused-args"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut ModuleAnalyses,
    ) -> bool {
        prune_unused_args(module) != 0
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
fn is_internal_body(func: &Function) -> bool {
    func.selector.is_none()
        && !func.is_public()
        && !func.attributes.is_constructor
        && !func.attributes.is_fallback
        && !func.attributes.is_receive
        && !func.attributes.is_dispatch_entry
}

fn has_rewritable_signature(func: &Function, is_called: bool) -> bool {
    is_called
        && !func.params.is_empty()
        && func.params.len() == func.arg_indices().count()
        && is_internal_body(func)
}

/// Computes the least fixed point of argument liveness and removes every argument outside it.
fn prune_unused_args(module: &mut Module) -> usize {
    let mut called = DenseBitSet::new_empty(module.functions.len());
    for func in &module.functions {
        for inst_id in func.instructions() {
            if let InstKind::InternalCall { function, .. } = func.inst(inst_id).kind {
                called.insert(function);
            }
        }
        for block in &func.blocks {
            if let Some(Terminator::TailCall { function, .. }) = &block.terminator {
                called.insert(*function);
            }
        }
    }
    let mut live = module
        .functions
        .iter_enumerated()
        .map(|(func_id, func)| {
            vec![!has_rewritable_signature(func, called.contains(func_id)); func.params.len()]
        })
        .collect::<IndexVec<FunctionId, _>>();

    // An argument is live when it participates in a non-call operation, or when it is forwarded
    // into a live callee argument. Starting candidate arguments dead computes transitive deadness,
    // including arguments forwarded around an otherwise-unused recursive cycle.
    loop {
        let mut changed = false;
        for (func_id, func) in module.functions.iter_enumerated() {
            for inst_id in func.instructions() {
                let kind = &func.inst(inst_id).kind;
                if let InstKind::InternalCall { function, args, .. } = kind {
                    for (index, &arg) in args.iter().enumerate() {
                        if live[*function].get(index).copied().unwrap_or(true) {
                            changed |= mark_arg_live(func, arg, &mut live[func_id]);
                        }
                    }
                } else {
                    for operand in kind.operands() {
                        changed |= mark_arg_live(func, operand, &mut live[func_id]);
                    }
                }
            }
            for block in &func.blocks {
                let Some(term) = &block.terminator else { continue };
                if let Terminator::TailCall { function, args } = term {
                    for (index, &arg) in args.iter().enumerate() {
                        if live[*function].get(index).copied().unwrap_or(true) {
                            changed |= mark_arg_live(func, arg, &mut live[func_id]);
                        }
                    }
                } else {
                    for operand in term.operands() {
                        changed |= mark_arg_live(func, operand, &mut live[func_id]);
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }

    let removed = live.iter().map(|args| args.iter().filter(|&&live| !live).count()).sum();
    if removed == 0 {
        return 0;
    }

    // Rewrite every edge before rewriting argument identities in the callees. Any computation
    // that fed a removed argument remains in the caller; ordinary DCE may remove it only when its
    // effects permit that.
    for func in &mut module.functions {
        func.for_each_instruction_mut(|_, inst| {
            if let InstKind::InternalCall { function, args, .. } = &mut inst.kind {
                *args = args
                    .iter()
                    .zip(&live[*function])
                    .filter_map(|(&arg, &keep)| keep.then_some(arg))
                    .collect();
            }
        });
        for block in &mut func.blocks {
            if let Some(Terminator::TailCall { function, args }) = &mut block.terminator {
                *args = args
                    .iter()
                    .zip(&live[*function])
                    .filter_map(|(&arg, &keep)| keep.then_some(arg))
                    .collect();
            }
        }
    }

    let function_ids = module.functions.indices().collect::<Vec<_>>();
    for func_id in function_ids {
        if live[func_id].iter().all(|&keep| keep) {
            continue;
        }
        let func = module.function_mut(func_id);

        let old_params = func.params.clone();
        let mut remap = IndexVec::<ArgIdx, Option<ArgIdx>>::with_capacity(old_params.len());
        let mut new_params = IndexVec::with_capacity(old_params.len());
        for (index, &ty) in old_params.iter_enumerated() {
            remap.push(live[func_id][index.index()].then(|| new_params.push(ty)));
        }

        for index in 0..func.num_values() {
            let value = ValueId::from_usize(index);
            let Value::Arg(old_index) = func.value(value) else { continue };
            let old_index = old_index.to_owned();
            *func.value_mut(value) =
                remap[old_index].map(Value::Arg).unwrap_or(Value::Undef(old_params[old_index]));
        }
        func.set_params(new_params);
    }

    removed
}

fn mark_arg_live(func: &Function, value: ValueId, live: &mut [bool]) -> bool {
    let Value::Arg(index) = func.value(value) else { return false };
    let Some(is_live) = live.get_mut(index.index()) else { return false };
    let changed = !*is_live;
    *is_live = true;
    changed
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
    let recursive = CallGraphInfo::new(module);
    let mut merged = vec![false; module.functions.len()];
    let mut total = 0;

    loop {
        let mut groups = FxHashMap::<String, Vec<FunctionId>>::default();
        for (func_id, func) in module.functions.iter_enumerated() {
            if !merged[func_id.index()] && is_merge_candidate(func_id, func, &recursive) {
                groups.entry(equivalence_bucket(func)).or_default().push(func_id);
            }
        }

        let mut replacements = FxHashMap::default();
        for candidates in groups.values() {
            let mut representatives = Vec::new();
            for &candidate in candidates {
                if let Some(&representative) = representatives.iter().find(|&&representative| {
                    equivalent_functions(
                        module.function(representative),
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
        for &duplicate in replacements.keys() {
            merged[duplicate.index()] = true;
        }
        redirect_calls(module, &replacements);
    }

    total
}

fn is_merge_candidate(func_id: FunctionId, func: &Function, calls: &CallGraphInfo) -> bool {
    !func.blocks.is_empty() && is_internal_body(func) && !calls.is_recursive(func_id)
}

/// Cheaply partitions functions before the exact pairwise alpha-equivalence check.
fn equivalence_bucket(func: &Function) -> String {
    let mut key = format!(
        "{:?}|{:?}|{}|{}|{}",
        func.params,
        func.returns,
        func.internal_frame_size,
        func.external_static_return_size,
        func.blocks.len()
    );
    for block in &func.blocks {
        key.push('|');
        for &inst_id in &block.instructions {
            let inst = func.inst(inst_id);
            key.push_str(inst.kind.mnemonic());
            key.push(':');
            key.push_str(&format!("{:?};", inst.result_ty));
        }
        key.push_str(block.terminator.as_ref().map_or("none", Terminator::mnemonic));
    }
    key
}

fn equivalent_functions(lhs: &Function, rhs: &Function) -> bool {
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
                || !equivalent_inst_payload(&lhs_inst.kind, &rhs_inst.kind)
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
            || !equivalent_terminator_payload(lhs_term, rhs_term)
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
        && lhs.attributes.is_dispatch_entry == rhs.attributes.is_dispatch_entry
        && lhs.attributes.no_inline == rhs.attributes.no_inline
}

/// Compares the non-operand fields of two instructions. Operands are zeroed because their
/// alpha-equivalent identities were compared separately.
fn equivalent_inst_payload(lhs: &InstKind, rhs: &InstKind) -> bool {
    let mut lhs = lhs.clone();
    let mut rhs = rhs.clone();
    let zero = ValueId::from_usize(0);
    lhs.visit_operands_mut(|value| *value = zero);
    rhs.visit_operands_mut(|value| *value = zero);
    lhs == rhs
}

/// Compares CFG targets and other non-operand terminator fields.
fn equivalent_terminator_payload(lhs: &Terminator, rhs: &Terminator) -> bool {
    let mut lhs = lhs.clone();
    let mut rhs = rhs.clone();
    let zero = ValueId::from_usize(0);
    let lhs_replacements = lhs.operands().into_iter().map(|value| (value, zero)).collect();
    let rhs_replacements = rhs.operands().into_iter().map(|value| (value, zero)).collect();
    crate::mir::utils::replace_terminator_uses(&mut lhs, &lhs_replacements);
    crate::mir::utils::replace_terminator_uses(&mut rhs, &rhs_replacements);
    lhs == rhs
}

fn redirect_calls(module: &mut Module, replacements: &FxHashMap<FunctionId, FunctionId>) {
    for func in &mut module.functions {
        func.for_each_instruction_mut(|_, inst| {
            if let InstKind::InternalCall { function, .. } = &mut inst.kind
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
