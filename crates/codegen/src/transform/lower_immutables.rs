//! Lower immutable assignments to ordinary constructor-memory stores.
//!
//! Keeping `storeimmutable` semantic through the optimization pipeline lets
//! immutable-aware passes reason about assignments without treating the
//! backend staging area as arbitrary memory. This pass expands assignments
//! only after those optimizations have finished.

use crate::{
    immutable::immutable_staging_addr,
    mir::{EffectKind, Function, Immediate, InstKind, MemoryRegion, Value},
    pass::FunctionPass,
};
use alloy_primitives::U256;

/// Lowers immutable assignments to memory stores in the deployment staging area.
pub(crate) struct LowerImmutablesPass;

impl FunctionPass for LowerImmutablesPass {
    fn run_on_function(&mut self, func: &mut Function) -> bool {
        let stores: Vec<_> = func
            .instructions
            .iter_enumerated()
            .filter_map(|(inst_id, inst)| match inst.kind {
                InstKind::StoreImmutable { id, value } => Some((inst_id, id, value)),
                _ => None,
            })
            .collect();

        for &(inst_id, id, value) in &stores {
            let addr = func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(
                immutable_staging_addr(id),
            ))));
            let inst = &mut func.instructions[inst_id];
            inst.kind = InstKind::MStore(addr, value);
            inst.metadata.set_effect(Some(EffectKind::MemoryWrite));
            inst.metadata.set_memory_region(Some(MemoryRegion::Unknown));
        }
        !stores.is_empty()
    }
}
