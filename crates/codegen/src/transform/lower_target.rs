//! Coordinated lowering of abstract and target-dependent MIR operations.

use crate::{
    mir::{InstKind, MirPhase, Module},
    pass::{MirPass, ModuleAnalyses},
    transform::{
        lower_alloc::lower_alloc, lower_immutables::lower_immutables,
        lower_mcopy::lower_mcopy_module, lower_memory_zero::lower_memory_zero,
    },
};
use solar_sema::Gcx;

/// Lowers abstract allocation and target-dependent memory operations.
pub(crate) struct LowerTarget;

impl MirPass for LowerTarget {
    fn name(&self) -> &'static str {
        "lower-target"
    }

    fn is_enabled(&self, _gcx: Gcx<'_>, module: &Module) -> bool {
        module.phase == MirPhase::IntrinsicsLowered
    }

    fn is_required(&self) -> bool {
        true
    }

    fn output_phase(&self) -> Option<MirPhase> {
        Some(MirPhase::TargetLowered)
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, _analyses: &mut ModuleAnalyses) -> bool {
        if module.phase != MirPhase::IntrinsicsLowered {
            return false;
        }

        let mut changed = false;
        if module.functions.iter().any(|func| {
            func.instructions()
                .any(|inst_id| matches!(func.inst(inst_id).kind, InstKind::StoreImmutable(..)))
        }) {
            changed |= lower_immutables(module);
        }
        changed |= lower_alloc(module);
        changed |= lower_memory_zero(module);
        if !gcx.sess.opts.evm_version.has_mcopy() {
            changed |= lower_mcopy_module(module);
        }
        if target_operations_are_lowered(module, gcx.sess.opts.evm_version.has_mcopy()) {
            module.advance_phase(MirPhase::TargetLowered);
            changed = true;
        }
        changed
    }
}

/// Returns whether no abstract target-lowering operation remains.
pub(crate) fn target_operations_are_lowered(module: &Module, target_has_mcopy: bool) -> bool {
    super::static_alloc::deferred_allocations_are_valid(module)
        && module.functions.iter().all(|func| {
            func.instructions().all(|inst_id| {
                let inst = func.inst(inst_id);
                !matches!(
                    inst.kind,
                    InstKind::Fmp
                        | InstKind::SetFmp(_)
                        | InstKind::MemoryZero(..)
                        | InstKind::StoreImmutable(..)
                ) && !matches!(inst.kind, InstKind::Alloc { .. } if !inst.metadata.deferred_alloc())
                    && !matches!(inst.kind, InstKind::MCopy(..) if !target_has_mcopy)
            })
        })
}
