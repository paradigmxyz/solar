//! Lower immutable assignments to ordinary constructor-memory stores.
//!
//! Keeping `storeimmutable` semantic through the optimization pipeline lets
//! immutable-aware passes reason about assignments without treating the
//! backend staging area as arbitrary memory. This pass expands assignments
//! only after those optimizations have finished.

use crate::mir::{
    Callee, EffectKind, FunctionBuilder, FunctionId, InstKind, MemoryRegion, Module, Terminator,
    immutable::{immutable_staging_addr, immutable_staging_base},
    pass::MirPass,
};
use solar_data_structures::bit_set::DenseBitSet;
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
        _analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let staging_base = immutable_staging_base(module);
        let runtime_reachable = runtime_reachable_functions(module);
        let mut changed = false;
        for (func_id, func) in module.functions.iter_mut_enumerated() {
            if runtime_reachable.contains(func_id) {
                continue;
            }
            for block in func.blocks.indices() {
                let instructions = std::mem::take(&mut func.blocks[block].instructions);
                let mut builder = FunctionBuilder::new(func);
                builder.switch_to_block(block);
                for inst_id in instructions {
                    if let InstKind::StoreImmutable(id, value) = builder.func().inst(inst_id).kind {
                        // word = zext integer or ptrtoint pointer to i256
                        // mstore staging_address, word
                        let metadata = builder.func().inst(inst_id).metadata.debug_context();
                        builder.set_debug_context(&metadata);
                        let value = builder.cast_word(value);
                        let addr = builder.imm(immutable_staging_addr(staging_base, id));
                        let inst = builder.func_mut().inst_mut(inst_id);
                        inst.kind = InstKind::MStore(addr, value);
                        inst.metadata.set_effect(Some(EffectKind::MemoryWrite));
                        inst.metadata.set_memory_region(Some(MemoryRegion::Unknown));
                        changed = true;
                    }
                    builder.func_mut().blocks[block].instructions.push(inst_id);
                }
            }
        }
        changed
    }
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
            if let InstKind::ICall { function: Callee::Function(function), .. } =
                func.inst(inst_id).kind
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
