//! Legalize opcodes that older EVM targets lack.
//!
//! Before Byzantium, this pass replaces `REVERT` with `INVALID`, the closest available terminating
//! behavior. Newer targets skip the pass entirely.
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

    fn is_enabled(&self, gcx: Gcx<'_>, _module: &Module) -> bool {
        !gcx.sess.opts.evm_version.supports_returndata()
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
    for block in &mut module.blocks {
        if let Some(terminator) = &mut block.terminator
            && matches!(terminator.kind, TerminatorKind::Op(op::REVERT))
        {
            terminator.kind = TerminatorKind::Op(op::INVALID);
            changed = true;
        }
    }
    changed
}
