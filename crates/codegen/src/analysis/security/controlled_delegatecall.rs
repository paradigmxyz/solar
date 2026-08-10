//! Controlled `delegatecall` detector.
//!
//! A `delegatecall` (or `callcode`) runs the target's code in this contract's
//! own storage and balance context. If the target address is derived from
//! calldata or an external function argument, an attacker chooses the code that
//! executes as this contract — a full takeover. Legitimate proxies delegate to
//! a target held in storage or an immutable, which this detector does not flag.
//!
//! Detection traces the call's target value backward to its origins; a target
//! reaching an external argument or a calldata read is reported. This is
//! heuristic: it does not yet prove the absence of a dominating validation of
//! the target, so a delegatecall to a checked argument is still reported.

use super::{finding::Finding, provenance};
use crate::mir::{Function, InstKind};
use solar_sema::Gcx;

const CONTROLLED_DELEGATECALL: Finding = Finding {
    code: "SEC-CONTROLLED-DELEGATECALL",
    message: "`delegatecall` to an attacker-controlled address",
    help: "delegatecall runs the target's code in this contract's context; never derive its target from calldata or an external argument",
};

/// Reports every `delegatecall`/`callcode` in `func` whose target is derived
/// from attacker-controlled input.
pub(super) fn check(gcx: Gcx<'_>, func: &Function) {
    // Arguments and calldata are attacker-controlled only for externally
    // reachable entries. An internal helper's arguments may be fixed by its
    // callers, so restrict the report to external functions to stay quiet.
    if !provenance::is_externally_reachable(func) {
        return;
    }
    for inst_id in func.instructions() {
        let inst = func.inst(inst_id);
        let addr = match inst.kind {
            InstKind::DelegateCall { addr, .. } | InstKind::CallCode { addr, .. } => addr,
            _ => continue,
        };
        if provenance::is_attacker_controlled(func, addr) {
            CONTROLLED_DELEGATECALL.emit(gcx, inst.metadata.source_span());
        }
    }
}
