//! Coordinated lowering of semantic MIR intrinsics and aggregate representations.

use crate::{
    mir::{MirPhase, Module},
    pass::{MirPass, ModuleAnalyses},
    transform::lower_memory_objects::lower_memory_objects,
};
use solar_sema::Gcx;

/// Lowers semantic intrinsics behind one representation boundary.
pub(crate) struct LowerIntrinsics;

impl MirPass for LowerIntrinsics {
    fn name(&self) -> &'static str {
        "lower-intrinsics"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        if lower_memory_objects(module) {
            analyses.invalidate();
        }
        module.advance_phase(MirPhase::IntrinsicsLowered);
        true
    }
}
