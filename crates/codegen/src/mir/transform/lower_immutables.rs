//! Lower immutable assignments to constructor staging or an SSA exit payload.
//!
//! Keeping `storeimmutable` semantic through the optimization pipeline lets
//! immutable-aware passes reason about assignments without treating the
//! backend staging area as arbitrary memory. This pass expands assignments
//! only after those optimizations have finished.
//!
//! Constructors with unrestricted source memory cannot keep assignments in hidden staging
//! words. Pruned dominance-frontier phis merge assignments across branches and loops. Renaming
//! starts with each immutable's default zero, replaces reads in statement order, and returns
//! each normal exit's final values as a constructor payload, ordered by immutable ID. The
//! backend stages that payload only after source execution ends. Nonrecursive immutable-reading
//! helpers are inlined into a temporary constructor; the rewrite commits only when every access can
//! be promoted. Helper writes and recursive readers retain the guarded memory path.
//! Tail calls to proven aborting helpers need no exit payload; other tail calls remain unsupported.

use super::inline::inline_call;
use crate::mir::{
    BlockId, EffectKind, FunctionId, Immediate, InstKind, Instruction, MemoryRegion, Module,
    Terminator, Value,
    analysis::{CallGraphInfo, CfgInfo},
    immutable::{immutable_staging_addr, immutable_staging_base},
    pass::MirPass,
};
use alloy_primitives::U256;
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    index::IndexVec,
    map::FxHashMap,
};
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
/// Immutable-reading helpers are inlined before SSA construction. Helper writes and
/// recursive readers retain the guarded memory lowering.
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
    let mut exits = DenseBitSet::new_empty(ctor.blocks.len());
    for (block_id, block) in ctor.blocks.iter_enumerated() {
        if !cfg.is_reachable(block_id) {
            continue;
        }
        match &block.terminator {
            Some(Terminator::Return { values }) if values.is_empty() => {
                exits.insert(block_id);
            }
            Some(Terminator::Stop) => {
                exits.insert(block_id);
            }
            Some(Terminator::TailCall { function, .. }) if cold_functions.contains(*function) => {}
            Some(Terminator::Return { .. } | Terminator::TailCall { .. }) => return false,
            _ => {}
        }
    }
    if exits.is_empty() {
        return false;
    }
    let mut frontiers =
        IndexVec::<BlockId, Vec<BlockId>>::from_vec(vec![Vec::new(); ctor.blocks.len()]);
    for &block in cfg.rpo() {
        let Some(idom) = cfg.dominators().idom(block) else { continue };
        for &pred in &ctor.blocks[block].predecessors {
            if !cfg.is_reachable(pred) {
                continue;
            }
            let mut runner = pred;
            while runner != idom {
                if !frontiers[runner].contains(&block) {
                    frontiers[runner].push(block);
                }
                let Some(parent) = cfg.dominators().idom(runner) else { break };
                if parent == runner {
                    break;
                }
                runner = parent;
            }
        }
    }
    let types = module.iter_immutables().map(|(_, immutable)| immutable.ty).collect::<Vec<_>>();
    let mut replacements = FxHashMap::default();
    let mut removed = GrowableBitSet::with_capacity(ctor.num_insts());
    let mut payloads = exits.iter().map(|block| (block, Vec::new())).collect::<FxHashMap<_, _>>();
    for (id, immutable) in module.iter_immutables() {
        let mut defs = DenseBitSet::new_empty(ctor.blocks.len());
        let mut live_in = DenseBitSet::new_empty(ctor.blocks.len());
        for (block, data) in ctor.blocks.iter_enumerated() {
            if !cfg.is_reachable(block) {
                continue;
            }
            for &inst in &data.instructions {
                match ctor.inst(inst).kind {
                    InstKind::StoreImmutable(stored, _) if stored == id => {
                        defs.insert(block);
                    }
                    InstKind::LoadImmutable(loaded) if loaded == id && !defs.contains(block) => {
                        live_in.insert(block);
                    }
                    _ => {}
                }
            }
            if exits.contains(block) && !defs.contains(block) {
                live_in.insert(block);
            }
        }
        loop {
            let mut changed = false;
            for &block in cfg.rpo() {
                if !defs.contains(block)
                    && cfg.successors(block).iter().any(|&succ| live_in.contains(succ))
                {
                    changed |= live_in.insert(block);
                }
            }
            if !changed {
                break;
            }
        }
        let mut phi_blocks = DenseBitSet::new_empty(ctor.blocks.len());
        let mut worklist = defs.iter().collect::<Vec<_>>();
        while let Some(block) = worklist.pop() {
            for &frontier in &frontiers[block] {
                if live_in.contains(frontier) && phi_blocks.insert(frontier) {
                    worklist.push(frontier);
                }
            }
        }
        let mut phis = FxHashMap::default();
        // incoming immutable values -> phi at each live iterated dominance frontier
        for block in phi_blocks.iter() {
            let (inst, value) = ctor.alloc_value_inst(
                Instruction::new(InstKind::Phi(Vec::new()), Some(immutable.ty))
                    .with_debug_info_dropped(),
            );
            let position = ctor.blocks[block]
                .instructions
                .iter()
                .take_while(|&&inst| matches!(ctor.inst(inst).kind, InstKind::Phi(_)))
                .count();
            ctor.blocks[block].instructions.insert(position, inst);
            phis.insert(block, (inst, value));
        }
        // immutable entry value = zero
        let zero =
            ctor.alloc_value(Value::Immediate(Immediate::for_type(Some(immutable.ty), U256::ZERO)));
        let mut worklist = vec![(BlockId::ENTRY, zero)];
        // storeimmutable id, value -> current = value
        // loadimmutable id -> current
        // ret -> ret [..., current]
        while let Some((block, mut current)) = worklist.pop() {
            if let Some(&(_, value)) = phis.get(&block) {
                current = value;
            }
            for &inst in &ctor.blocks[block].instructions {
                match ctor.inst(inst).kind {
                    InstKind::StoreImmutable(stored, value) if stored == id => {
                        current = value;
                        removed.insert(inst);
                    }
                    InstKind::LoadImmutable(loaded) if loaded == id => {
                        replacements.insert(
                            ctor.inst_result_value(inst).expect("immutable load has a result"),
                            current,
                        );
                        removed.insert(inst);
                    }
                    _ => {}
                }
            }
            if let Some(payload) = payloads.get_mut(&block) {
                payload.push(current);
            }
            // successor phi += [block: current]
            for &succ in cfg.successors(block) {
                if let Some(&(inst, _)) = phis.get(&succ) {
                    let InstKind::Phi(incoming) = &mut ctor.inst_mut(inst).kind else {
                        unreachable!()
                    };
                    incoming.push((block, current));
                }
            }
            for &child in cfg.dominators().children(block) {
                worklist.push((child, current));
            }
        }
        // phi inputs = reachable reaching values plus zero on unreachable edges
        for (&block, &(inst, _)) in &phis {
            let unreachable = ctor.blocks[block]
                .predecessors
                .iter()
                .copied()
                .filter(|&pred| !cfg.is_reachable(pred))
                .collect::<Vec<_>>();
            let InstKind::Phi(incoming) = &mut ctor.inst_mut(inst).kind else { unreachable!() };
            incoming.extend(unreachable.into_iter().map(|pred| (pred, zero)));
            incoming.sort_by_key(|(block, _)| *block);
        }
    }
    // storeimmutable and loadimmutable -> SSA values and edge phis
    for block in &mut ctor.blocks {
        block.instructions.retain(|&inst| !removed.contains(inst));
    }
    // ret -> ret [immutable0, immutable1, ...]
    for (block, values) in payloads {
        ctor.blocks[block].terminator = Some(Terminator::Return { values: values.into() });
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
