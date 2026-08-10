//! Weak-randomness (weak PRNG) detector.
//!
//! `block.prevrandao`, `blockhash(...)`, and `block.coinbase` are observable to
//! callers and influenceable by block producers. Reducing one of them modulo a
//! bound — the `blockValue % n` idiom — to pick a winner or an index is
//! predictable randomness an attacker can game.
//!
//! Detection anchors on a `Mod` whose either operand's origin reaches one of
//! those block values. `block.timestamp` and `block.number` are intentionally
//! excluded: they have legitimate modulo uses (time-of-day, epochs) and would
//! raise false positives.
//!
//! Limitation: the walk follows SSA values, so a block value hashed through
//! `keccak256(abi.encode(...))` before the modulo — a common wrapped form — is
//! not yet traced (it flows through memory). Direct `blockValue % n` is caught.

use super::{finding::Finding, provenance};
use crate::mir::{Function, InstKind, Value};
use solar_data_structures::map::FxHashSet;
use solar_sema::Gcx;

const WEAK_PRNG: Finding = Finding {
    code: "SEC-WEAK-PRNG",
    message: "randomness derived from a block value",
    help: "block.prevrandao, blockhash, and block.coinbase are observable and block-producer-influenceable; use a commit-reveal scheme or a VRF",
};

/// Reports every `Mod` in `func` whose operands derive from a block-randomness
/// value.
pub(super) fn check(gcx: Gcx<'_>, func: &Function) {
    for inst_id in func.instructions() {
        let inst = func.inst(inst_id);
        let InstKind::Mod(dividend, divisor) = inst.kind else { continue };
        if reaches_block_randomness(func, dividend) || reaches_block_randomness(func, divisor) {
            WEAK_PRNG.emit(gcx, inst.metadata.source_span());
        }
    }
}

/// Whether `value`'s origin reaches `block.prevrandao`, `blockhash`, or
/// `block.coinbase`.
fn reaches_block_randomness(func: &Function, value: crate::mir::ValueId) -> bool {
    provenance::origin_reaches(func, value, &mut FxHashSet::default(), &is_block_randomness)
}

fn is_block_randomness(_value: &Value, kind: Option<&InstKind>) -> bool {
    matches!(kind, Some(InstKind::PrevRandao | InstKind::BlockHash(_) | InstKind::Coinbase))
}
