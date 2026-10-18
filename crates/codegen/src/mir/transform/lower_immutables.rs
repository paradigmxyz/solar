//! Lower immutable assignments to ordinary constructor-memory stores.
//!
//! Keeping `storeimmutable` semantic through the optimization pipeline lets
//! immutable-aware passes reason about assignments without treating the
//! backend staging area as arbitrary memory. This pass expands assignments
//! only after those optimizations have finished.
//!
//! Each assignment becomes a word store to the immutable's constructor-memory home, which the
//! backend copies into the deployed code's placeholders. Only functions unreachable from the
//! runtime entry, selectors, receive, and fallback are rewritten: an assignment left in runtime
//! code stays semantic and fails the lowered-phase check rather than writing memory there.

use crate::mir::{
    EffectKind, FunctionBuilder, FunctionId, InstKind, MemoryRegion, Module, PtrLocation,
    analysis::CallGraphInfo,
    immutable::{immutable_staging_addr, immutable_staging_base},
    pass::MirPass,
};
use solar_data_structures::bit_set::DenseBitSet;

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
                        // store staging_address, word
                        let metadata = builder.func().inst(inst_id).metadata.debug_context();
                        builder.set_debug_context(&metadata);
                        let value = builder.cast_word(value);
                        let addr = builder.imm(immutable_staging_addr(staging_base, id));
                        let inst = builder.func_mut().inst_mut(inst_id);
                        inst.kind = InstKind::Store(addr, value, PtrLocation::Memory);
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
    let roots = module
        .dispatch_entry()
        .into_iter()
        .chain(module.functions.iter_enumerated().filter_map(|(id, func)| {
            (func.selector.is_some() || func.attributes.is_fallback || func.attributes.is_receive)
                .then_some(id)
        }))
        .collect::<Vec<_>>();
    let mut reachable = CallGraphInfo::new(module).reachable_callees_from(roots.iter().copied());
    for root in roots {
        reachable.insert(root);
    }
    reachable
}
