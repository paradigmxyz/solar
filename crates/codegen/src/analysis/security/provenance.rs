//! Value-provenance helpers shared by the flow detectors.
//!
//! A minimal backward taint primitive: walk the SSA def graph from a value to
//! its origins and test whether any reached node is a source of interest. This
//! is enough for "is this value derived from attacker input" without a full
//! forward taint engine.

use crate::mir::{Function, InstKind, Value, ValueId};
use solar_data_structures::map::FxHashSet;

/// Whether `func` is directly reachable by an external caller, so its arguments
/// and calldata are attacker-controlled.
pub(super) fn is_externally_reachable(func: &Function) -> bool {
    func.selector.is_some() || func.attributes.is_fallback || func.attributes.is_receive
}

/// Whether `value`'s origin reaches an external argument or a calldata read —
/// i.e. it is derived from attacker-controlled input. Only meaningful inside an
/// externally reachable function (see [`is_externally_reachable`]), where every
/// argument is attacker-supplied.
pub(super) fn is_attacker_controlled(func: &Function, value: ValueId) -> bool {
    origin_reaches(func, value, &mut FxHashSet::default(), &|value, kind| {
        matches!(value, Value::Arg(_))
            || matches!(kind, Some(InstKind::CalldataLoad(_) | InstKind::CalldataCopy(..)))
    })
}

/// Walks the SSA def graph backward from `value`, returning whether any reached
/// node satisfies `is_source`. The visited set breaks phi cycles; any matching
/// operand taints the result, so an attacker who influences part of a
/// computation influences its result.
///
/// `is_source` receives the reached [`Value`] and, when it is an instruction
/// result, that instruction's [`InstKind`].
pub(super) fn origin_reaches(
    func: &Function,
    value: ValueId,
    visited: &mut FxHashSet<ValueId>,
    is_source: &impl Fn(&Value, Option<&InstKind>) -> bool,
) -> bool {
    if !visited.insert(value) {
        return false;
    }
    let value_ref = func.value(value);
    let kind = match value_ref {
        Value::Inst(inst_id) => Some(&func.inst(*inst_id).kind),
        _ => None,
    };
    if is_source(value_ref, kind) {
        return true;
    }
    match kind {
        Some(kind) => {
            kind.operands().into_iter().any(|op| origin_reaches(func, op, visited, is_source))
        }
        None => false,
    }
}
