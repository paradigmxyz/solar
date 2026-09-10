//! Lower immutable assignments to constructor staging or an SSA exit payload.
//!
//! Keeping `storeimmutable` semantic through the optimization pipeline lets
//! immutable-aware passes reason about assignments without treating the
//! backend staging area as arbitrary memory. This pass expands assignments
//! only after those optimizations have finished.
//!
//! Constructors with unrestricted source memory cannot keep assignments in hidden staging
//! words. For single assignments that dominate every normal exit and constructor read, this
//! pass replaces reads with their SSA values and returns the final values as a constructor
//! exit payload, ordered by immutable ID. The backend stages that payload only after source
//! execution ends. Nonrecursive immutable-reading helpers are inlined into a temporary
//! constructor; the rewrite commits only when every access can be promoted. Helper writes,
//! recursive readers, and assignments requiring merge phis retain the guarded memory path.
//! Tail calls to proven aborting helpers need no exit payload; other tail calls remain unsupported.

use super::inline::inline_call;
use crate::mir::{
    EffectKind, FunctionId, Immediate, ImmutableId, InstKind, MemoryRegion, Module, Terminator,
    Value,
    analysis::{CallGraphInfo, CfgInfo},
    immutable::{immutable_staging_addr, immutable_staging_base},
    pass::MirPass,
};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
use std::collections::VecDeque;

/// Lowers immutable assignments to constructor staging or an SSA exit payload.
pub(crate) struct LowerImmutables;

impl MirPass for LowerImmutables {
    fn name(&self) -> &'static str {
        "lower-immutables"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let promoted = promote_constructor_immutables(module);
        let staging_base = immutable_staging_base(module);
        let runtime_reachable = runtime_reachable_functions(module);
        let mut changed = promoted;
        for (func_id, func) in module.functions.iter_mut_enumerated() {
            if runtime_reachable.contains(func_id) {
                continue;
            }
            let stores: Vec<_> = func
                .instructions()
                .filter_map(|inst_id| match func.inst(inst_id).kind {
                    InstKind::StoreImmutable(id, value) => Some((inst_id, id, value)),
                    _ => None,
                })
                .collect();

            // storeimmutable id, value
            //   -> mstore staging_address(id), value !metadata(compiler_memory)
            for &(inst_id, id, value) in &stores {
                let addr = func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(
                    immutable_staging_addr(staging_base, id),
                ))));
                let inst = func.inst_mut(inst_id);
                inst.kind = InstKind::MStore(addr, value);
                inst.metadata.set_effect(Some(EffectKind::MemoryWrite));
                inst.metadata.set_memory_region(Some(MemoryRegion::Unknown));
                inst.metadata.set_requires_private_memory();
            }
            changed |= !stores.is_empty();
        }
        changed
    }
}

/// Keep constructor assignments in SSA until the constructor returns its staging payload.
/// Immutable-reading helpers are inlined before checking dominance. Helper writes and
/// assignments needing merge phis retain the guarded memory lowering.
fn promote_constructor_immutables(module: &mut Module) -> bool {
    if module.immutable_count() == 0 {
        return false;
    }
    let Some((ctor_id, ctor)) =
        module.functions.iter_enumerated().find(|(_, f)| f.attributes.is_constructor)
    else {
        return false;
    };
    if !ctor.returns.is_empty() {
        return false;
    }
    let call_graph = CallGraphInfo::new(module);
    let reachable = call_graph.reachable_callees_from([ctor_id]);
    if !std::iter::once(ctor_id)
        .chain(reachable.iter())
        .any(|id| module.functions[id].attributes.unrestricted_memory)
    {
        return false;
    }
    let mut readers = DenseBitSet::new_empty(module.functions.len());
    for id in reachable.iter() {
        for inst in module.functions[id].instructions() {
            match module.functions[id].inst(inst).kind {
                InstKind::StoreImmutable(..) => return false,
                InstKind::LoadImmutable(_) => {
                    readers.insert(id);
                }
                _ => {}
            }
        }
    }
    let mut inline_targets = readers.clone();
    for id in reachable.iter() {
        if call_graph.reachable_callees_from([id]).iter().any(|callee| readers.contains(callee)) {
            inline_targets.insert(id);
        }
    }
    if inline_targets
        .iter()
        .any(|id| call_graph.is_recursive(id) || module.functions[id].returns.len() > 1)
    {
        return false;
    }
    let mut ctor = ctor.clone();
    loop {
        let call = ctor.blocks.iter_enumerated().find_map(|(block_id, block)| {
            block.instructions.iter().enumerate().find_map(|(index, &inst)| {
                if let InstKind::ICall { function, .. } = ctor.inst(inst).kind
                    && inline_targets.contains(function)
                {
                    Some((block_id, index, function))
                } else {
                    None
                }
            })
        });
        let Some((block, index, target)) = call else { break };
        // icall @immutable_reader(args) -> cloned reader CFG with SSA arguments
        if !inline_call(&mut ctor, block, index, &module.functions[target]) {
            return false;
        }
    }
    if ctor.blocks.iter().any(|block| matches!(block.terminator, Some(Terminator::TailCall { function, .. }) if inline_targets.contains(function))) {
        return false;
    }
    let cold_functions = CallGraphInfo::collect_cold_functions(module);
    let cfg = CfgInfo::new(&ctor);
    let mut assignments =
        IndexVec::<ImmutableId, _>::from_vec(vec![None; module.immutable_count()]);
    let mut loads = Vec::new();
    let mut exits = Vec::new();
    for (block_id, block) in ctor.blocks.iter_enumerated() {
        if !cfg.is_reachable(block_id) {
            continue;
        }
        for (index, &inst_id) in block.instructions.iter().enumerate() {
            match ctor.inst(inst_id).kind {
                InstKind::StoreImmutable(id, value) => {
                    if assignments[id].replace((block_id, index, inst_id, value)).is_some() {
                        return false;
                    }
                }
                InstKind::LoadImmutable(id) => loads.push((block_id, index, inst_id, id)),
                _ => {}
            }
        }
        match &block.terminator {
            Some(Terminator::Return { values }) if values.is_empty() => exits.push(block_id),
            Some(Terminator::Stop) => exits.push(block_id),
            Some(Terminator::TailCall { function, .. }) if cold_functions.contains(*function) => {}
            Some(Terminator::Return { .. } | Terminator::TailCall { .. }) => return false,
            _ => {}
        }
    }
    if exits.is_empty() || assignments.iter().any(Option::is_none) {
        return false;
    }
    let assignments =
        assignments.into_iter().map(Option::unwrap).collect::<IndexVec<ImmutableId, _>>();
    if assignments
        .iter()
        .any(|&(block, _, _, _)| exits.iter().any(|&exit| !cfg.dominators().dominates(block, exit)))
    {
        return false;
    }
    let mut replacements = FxHashMap::default();
    for &(block, index, inst, id) in &loads {
        let (store_block, store_index, _, value) = assignments[id];
        if !cfg.dominators().dominates(store_block, block)
            || (store_block == block && store_index >= index)
        {
            return false;
        }
        replacements
            .insert(ctor.inst_result_value(inst).expect("immutable load has a result"), value);
    }
    let types = module.iter_immutables().map(|(_, immutable)| immutable.ty).collect();
    let mut removed = DenseBitSet::new_empty(ctor.num_insts());
    for &(_, _, inst, _) in &assignments {
        removed.insert(inst);
    }
    for &(_, _, inst, _) in &loads {
        removed.insert(inst);
    }
    // storeimmutable id, value; ...; ret
    //   -> ...; ret [immutable0, immutable1, ...]
    // loadimmutable id -> assigned_value(id)
    for block in &mut ctor.blocks {
        block.instructions.retain(|&inst| !removed.contains(inst));
    }
    // ret -> ret [immutable0, immutable1, ...]
    for block in exits {
        ctor.blocks[block].terminator = Some(Terminator::Return {
            values: assignments.iter().map(|&(_, _, _, value)| value).collect(),
        });
    }
    ctor.returns = types;
    ctor.replace_uses_canonicalized(&replacements);
    module.functions[ctor_id] = ctor;
    true
}

fn runtime_reachable_functions(module: &Module) -> DenseBitSet<FunctionId> {
    let mut reachable = DenseBitSet::new_empty(module.functions.len());
    let mut worklist = VecDeque::new();
    if let Some(entry) = module.dispatch_entry() {
        reachable.insert(entry);
        worklist.push_back(entry);
    }
    for (func_id, func) in module.functions.iter_enumerated() {
        if (func.selector.is_some() || func.attributes.is_fallback || func.attributes.is_receive)
            && reachable.insert(func_id)
        {
            worklist.push_back(func_id);
        }
    }

    while let Some(func_id) = worklist.pop_front() {
        let func = module.function(func_id);
        for inst_id in func.instructions() {
            if let InstKind::ICall { function, .. } = func.inst(inst_id).kind
                && reachable.insert(function)
            {
                worklist.push_back(function);
            }
        }
        for block in &func.blocks {
            if let Some(Terminator::TailCall { function, .. }) = &block.terminator
                && reachable.insert(*function)
            {
                worklist.push_back(*function);
            }
        }
    }
    reachable
}
