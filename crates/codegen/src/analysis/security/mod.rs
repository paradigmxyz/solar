//! MIR-based security analysis.
//!
//! Read-only detectors inspect freshly lowered (`Built`-phase) MIR before the
//! optimization pipeline folds guards, deletes writes, or lowers away
//! semantic structure. Findings are diagnostics that point at the originating
//! Solidity source.
//!
//! Each detector lives in its own module and describes its findings through the
//! shared [`finding::Finding`] vocabulary. [`analyze`] dispatches to every
//! detector.

mod finding;
mod provenance;

mod arbitrary_send;
mod controlled_delegatecall;
mod tx_origin;

use crate::mir::Module;
use solar_sema::Gcx;

/// Runs every security detector over a freshly lowered module.
pub(crate) fn analyze(gcx: Gcx<'_>, module: &Module) {
    for func in module.functions.iter() {
        tx_origin::check(gcx, func);
        controlled_delegatecall::check(gcx, func);
        arbitrary_send::check(gcx, func);
    }
}
