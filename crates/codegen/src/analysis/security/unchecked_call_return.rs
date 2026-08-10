//! Unchecked low-level call-return detector.
//!
//! `.call`, `.send`, `.staticcall`, and `.delegatecall` return a boolean
//! success flag that the caller must check; a failed call otherwise proceeds as
//! if it succeeded (a silently dropped transfer, an ignored external failure).
//! High-level calls and `.transfer` are safe because the compiler emits their
//! own revert-on-failure check, which consumes the result — so they are not
//! flagged.
//!
//! Detection reports a call whose result value is produced but never used as an
//! operand anywhere in the function. This is sound with respect to the compiler
//! output: an unused success flag is genuinely never inspected.

use super::finding::Finding;
use crate::mir::{Function, InstKind};
use solar_data_structures::map::FxHashSet;
use solar_sema::Gcx;

const UNCHECKED_CALL: Finding = Finding {
    code: "SEC-UNCHECKED-CALL",
    message: "return value of low-level call is ignored",
    help: "check the boolean success value and revert on failure; a failed `.call`/`.send`/`.staticcall`/`.delegatecall` otherwise looks like success",
};

/// Reports every low-level call in `func` whose success result is never used.
pub(super) fn check(gcx: Gcx<'_>, func: &Function) {
    // Collect every value used as an operand by an instruction or a terminator.
    let mut used = FxHashSet::default();
    for inst_id in func.instructions() {
        used.extend(func.inst(inst_id).kind.operands());
    }
    for block in func.blocks.iter() {
        if let Some(term) = &block.terminator {
            used.extend(term.operands());
        }
    }

    for inst_id in func.instructions() {
        let inst = func.inst(inst_id);
        if !matches!(
            inst.kind,
            InstKind::Call { .. }
                | InstKind::CallCode { .. }
                | InstKind::StaticCall { .. }
                | InstKind::DelegateCall { .. }
        ) {
            continue;
        }
        let Some(result) = func.inst_result_value(inst_id) else { continue };
        if !used.contains(&result) {
            UNCHECKED_CALL.emit(gcx, inst.metadata.source_span());
        }
    }
}
