//! Shared vocabulary for security findings.
//!
//! Every detector describes its findings with a [`Finding`] constant (a stable
//! code, a message, and remediation help) and emits them through
//! [`Finding::emit`], which handles the diagnostic plumbing and the
//! present-or-missing source span uniformly.

use solar_interface::{Span, diagnostics::DiagId};
use solar_sema::Gcx;

/// A security finding class: a stable code, a message, and its remediation help.
pub(super) struct Finding {
    /// Stable diagnostic code, e.g. `SEC-TX-ORIGIN`. Lets CI allowlist and diff.
    pub(super) code: &'static str,
    /// One-line description used as the primary diagnostic message.
    pub(super) message: &'static str,
    /// Actionable remediation shown as a `help:` subdiagnostic.
    pub(super) help: &'static str,
}

impl Finding {
    fn id(&self) -> DiagId {
        DiagId::new_str(self.code)
    }

    /// Emits this finding as a warning located at `span`, or without a location
    /// when the originating instruction carries no source span.
    pub(super) fn emit(&self, gcx: Gcx<'_>, span: Option<Span>) {
        let diag = gcx.dcx().warn(self.message).code(self.id()).help(self.help);
        let diag = match span {
            Some(span) => diag.span(span),
            None => diag,
        };
        diag.emit();
    }
}
