//! Lower immutable assignments to ordinary constructor-memory stores.
//!
//! Keeping `storeimmutable` semantic through the optimization pipeline lets
//! immutable-aware passes reason about assignments without treating the
//! backend staging area as arbitrary memory. This pass expands assignments
//! only after those optimizations have finished.

use crate::{
    immutable::{immutable_staging_addr, immutable_staging_base},
    mir::{EffectKind, FunctionId, Immediate, InstKind, MemoryRegion, Module, Terminator, Value},
    pass::MirPass,
};
use alloy_primitives::U256;
use solar_data_structures::bit_set::DenseBitSet;
use solar_interface::sym;
use std::collections::VecDeque;

/// Lowers immutable assignments to memory stores in the deployment staging area.
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
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        let staging_base = immutable_staging_base(module);
        let runtime_reachable = runtime_reachable_functions(module);
        let mut constructor_only = DenseBitSet::new_filled(module.functions.len());
        constructor_only.subtract(&runtime_reachable);
        let mut changed = false;
        for func_id in constructor_only.iter() {
            let func = module.function_mut(func_id);
            let stores: Vec<_> = func
                .instructions()
                .filter_map(|inst_id| match func.inst(inst_id).kind {
                    InstKind::StoreImmutable(id, value) => Some((inst_id, id, value)),
                    _ => None,
                })
                .collect();

            for &(inst_id, id, value) in &stores {
                let addr = func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(
                    immutable_staging_addr(staging_base, id),
                ))));
                let inst = func.inst_mut(inst_id);
                inst.kind = InstKind::MStore(addr, value);
                inst.metadata.set_effect(Some(EffectKind::MemoryWrite));
                inst.metadata.set_memory_region(Some(MemoryRegion::Unknown));
            }
            changed |= !stores.is_empty();
        }
        changed
    }
}

fn runtime_reachable_functions(module: &Module) -> DenseBitSet<FunctionId> {
    let mut reachable = DenseBitSet::new_empty(module.functions.len());
    let mut worklist = VecDeque::new();
    for (func_id, func) in module.functions.iter_enumerated() {
        if (func.name.symbol == sym::entry
            || func.selector.is_some()
            || func.attributes.is_fallback
            || func.attributes.is_receive)
            && reachable.insert(func_id)
        {
            worklist.push_back(func_id);
        }
    }

    while let Some(func_id) = worklist.pop_front() {
        let func = module.function(func_id);
        for inst_id in func.instructions() {
            if let InstKind::InternalCall { function, .. } = func.inst(inst_id).kind
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
