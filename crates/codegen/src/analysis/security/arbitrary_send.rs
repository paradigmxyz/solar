//! Arbitrary ether send detector.
//!
//! An externally reachable function that sends ether to an address derived from
//! calldata or an argument lets anyone choose the recipient and drain the
//! contract. Sending to `msg.sender` — the ordinary withdraw pattern — is not
//! flagged, because the recipient is `CALLER`, not attacker-controlled input.
//!
//! `.transfer`, `.send`, and `.call{value: …}` all lower to a value-carrying
//! `Call`; this detector reports one whose recipient is attacker-controlled and
//! whose value is not statically zero.
//!
//! Heuristic: like the delegatecall detector, it does not yet prove the absence
//! of a dominating access check, so a guarded payout to an argument address is
//! still reported.

use super::{finding::Finding, provenance};
use crate::mir::{Function, InstKind};
use solar_sema::Gcx;

const ARBITRARY_SEND_ETH: Finding = Finding {
    code: "SEC-ARBITRARY-SEND-ETH",
    message: "ether sent to an attacker-controlled address",
    help: "restrict who can call this or fix the recipient; sending to a calldata or argument address lets anyone drain the contract",
};

/// Reports every value-carrying `Call` in `func` whose recipient is derived
/// from attacker-controlled input.
pub(super) fn check(gcx: Gcx<'_>, func: &Function) {
    if !provenance::is_externally_reachable(func) {
        return;
    }
    for inst_id in func.instructions() {
        let inst = func.inst(inst_id);
        let InstKind::Call { addr, value, .. } = inst.kind else { continue };
        // A statically-zero value moves no ether — an ordinary external call.
        if func.value_u64(value) == Some(0) {
            continue;
        }
        if provenance::is_attacker_controlled(func, addr) {
            ARBITRARY_SEND_ETH.emit(gcx, inst.metadata.source_span());
        }
    }
}
