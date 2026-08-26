//! MIR-based security analysis.
//!
//! Read-only passes inspect freshly lowered (`Built`-phase) MIR before the
//! optimization pipeline folds guards, deletes writes, or lowers away
//! semantic structure. Findings are diagnostics that point at the originating
//! Solidity source.

use crate::mir::{InstKind, Module};
use solar_interface::diagnostics::DiagId;
use solar_sema::Gcx;

/// A security finding class: a stable code, a severity, and its help text.
struct Finding {
    /// Stable diagnostic code, e.g. `SEC-TX-ORIGIN`.
    code: &'static str,
    /// One-line description used as the primary diagnostic message.
    message: &'static str,
    /// Actionable remediation shown as a `help:` subdiagnostic.
    help: &'static str,
}

impl Finding {
    fn id(&self) -> DiagId {
        DiagId::new_str(self.code)
    }
}

/// `tx.origin` used anywhere in the contract.
const TX_ORIGIN: Finding = Finding {
    code: "SEC-TX-ORIGIN",
    message: "use of `tx.origin`",
    help: "use `msg.sender` for authorization; `tx.origin` can be spoofed by an intermediary contract",
};

/// Runs the security analysis over a freshly lowered module and emits findings.
pub(crate) fn analyze(gcx: Gcx<'_>, module: &Module) {
    for func in module.functions.iter() {
        check_tx_origin(gcx, func);
    }
}

/// Reports every `tx.origin` read.
fn check_tx_origin(gcx: Gcx<'_>, func: &crate::mir::Function) {
    for inst_id in func.instructions() {
        let inst = func.inst(inst_id);
        if !matches!(inst.kind, InstKind::Origin) {
            continue;
        }
        let diag = gcx.dcx().warn(TX_ORIGIN.message).code(TX_ORIGIN.id()).help(TX_ORIGIN.help);
        let diag = match inst.metadata.source_span() {
            Some(span) => diag.span(span),
            None => diag,
        };
        diag.emit();
    }
}
