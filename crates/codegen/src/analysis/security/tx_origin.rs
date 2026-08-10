//! `tx.origin` usage detector.
//!
//! Using `tx.origin` for authorization lets a malicious intermediary contract
//! impersonate the transaction origin; `msg.sender` is almost always the
//! correct check. Lowering maps `tx.origin` to exactly [`InstKind::Origin`], and
//! the instruction carries the source span of the access, so the finding points
//! at the offending line.

use super::finding::Finding;
use crate::mir::{Function, InstKind};
use solar_sema::Gcx;

const TX_ORIGIN: Finding = Finding {
    code: "SEC-TX-ORIGIN",
    message: "use of `tx.origin`",
    help: "use `msg.sender` for authorization; `tx.origin` can be spoofed by an intermediary contract",
};

/// Reports every `tx.origin` read in `func`.
pub(super) fn check(gcx: Gcx<'_>, func: &Function) {
    for inst_id in func.instructions() {
        let inst = func.inst(inst_id);
        if matches!(inst.kind, InstKind::Origin) {
            TX_ORIGIN.emit(gcx, inst.metadata.source_span());
        }
    }
}
