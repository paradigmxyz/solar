//! Legalize opcodes that older EVM targets lack.
//!
//! Before Byzantium, this pass replaces `REVERT` with `INVALID`, the closest available terminating
//! behavior. Newer targets keep their `REVERT`s.
//!
//! Codegen runs this pass after the whole EVM IR pipeline, so no peephole sees its output. Shifts
//! need no legalization here: `lower-shifts` removes them from MIR on targets without them.

use super::EvmPass;
use crate::backend::evm::{
    ir::{Module, TerminatorKind},
    op,
};
use solar_sema::Gcx;

pub(super) struct LegalizeRevert;

impl EvmPass for LegalizeRevert {
    fn name(&self) -> &'static str {
        "legalize-revert"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        legalize_revert(gcx, module)
    }
}

pub(in crate::backend) fn legalize_revert(gcx: Gcx<'_>, module: &mut Module) -> bool {
    if gcx.sess.opts.evm_version.supports_returndata() {
        return false;
    }

    let mut changed = false;
    for terminator in module.blocks.iter_mut().filter_map(|block| block.terminator.as_mut()) {
        if matches!(terminator.kind, TerminatorKind::Op(op::REVERT)) {
            terminator.kind = TerminatorKind::Op(op::INVALID);
            changed = true;
        }
    }
    changed
}
