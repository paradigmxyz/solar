//! Lower immutable assignments to ordinary constructor-memory stores.
//!
//! Keeping `storeimmutable` semantic through the optimization pipeline lets
//! immutable-aware passes reason about assignments without treating the
//! backend staging area as arbitrary memory. This pass expands assignments
//! only after those optimizations have finished.

use crate::{
    immutable::{immutable_staging_addr, immutable_staging_base},
    mir::{EffectKind, Immediate, InstKind, MemoryRegion, Module, Value},
    pass::MirPass,
};
use alloy_primitives::U256;

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
        let mut changed = false;
        for func in &mut module.functions {
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
