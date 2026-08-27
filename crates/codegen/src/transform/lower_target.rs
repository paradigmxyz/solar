//! Coordinated lowering of abstract and target-dependent MIR operations.

use crate::{
    mir::{MirPhase, Module},
    pass::{MirPass, ModuleAnalyses},
    transform::{
        lower_alloc::lower_alloc, lower_mcopy::lower_mcopy_module,
        lower_memory_zero::lower_memory_zero,
    },
};
use solar_sema::Gcx;

/// Lowers abstract allocation and target-dependent memory operations.
pub(crate) struct LowerTarget;

impl MirPass for LowerTarget {
    fn name(&self) -> &'static str {
        "lower-target"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, _analyses: &mut ModuleAnalyses) -> bool {
        let _ = lower_alloc(module);
        let _ = lower_memory_zero(module);
        if !gcx.sess.opts.evm_version.has_mcopy() {
            let _ = lower_mcopy_module(module);
        }
        module.advance_phase(MirPhase::TargetLowered);
        true
    }
}
