//! Bounded unsigned interval proofs for typed scalar arithmetic.
//!
//! Every unknown value starts with its integer type's full range. Recursive
//! producer queries refine that range only when the operation cannot wrap.
//! Callers can supply loop induction bounds; cyclic or deep definitions fall
//! back to the type range. A shared query budget bounds work across phi inputs. Address and affine
//! analyses use the same no-wrap proof before treating modular arithmetic as an ordinary offset.

use crate::mir::{Function, InstKind, MirType, Value, ValueId};
use alloy_primitives::U256;

#[inline]
pub(crate) fn integer_bits(func: &Function, value: ValueId) -> u32 {
    func.value_ty(value).and_then(MirType::integer_bits).unwrap_or(256)
}

#[inline]
pub(crate) fn integer_max(func: &Function, value: ValueId) -> U256 {
    let bits = integer_bits(func, value);
    if bits == 256 { U256::MAX } else { U256::MAX >> (256 - bits) }
}

pub(crate) fn unsigned_bounds(func: &Function, value: ValueId) -> (U256, U256) {
    bounds_with(func, value, 8, &mut 64, &|_| None)
}

fn bounds_with(
    func: &Function,
    value: ValueId,
    depth: u32,
    budget: &mut u32,
    known: &impl Fn(ValueId) -> Option<(U256, U256)>,
) -> (U256, U256) {
    if let Some(value) = func.value_u256(value) {
        return (value, value);
    }
    if let Some(range) = known(value) {
        return range;
    }
    let max = integer_max(func, value);
    let full = (U256::ZERO, max);
    let Some(depth) = depth.checked_sub(1) else { return full };
    let Some(remaining) = budget.checked_sub(1) else { return full };
    *budget = remaining;
    let Value::Inst(id) = func.value(value) else { return full };
    let mut bounds = |value| bounds_with(func, value, depth, budget, known);
    match &func.inst(*id).kind {
        InstKind::Zext(value) | InstKind::Bitcast(value) => bounds(*value),
        InstKind::Trunc(value, _) => {
            let range = bounds(*value);
            if range.1 <= max { range } else { full }
        }
        InstKind::And(a, b) => (U256::ZERO, bounds(*a).1.min(bounds(*b).1)),
        InstKind::Div(a, b) => {
            let divisor = bounds(*b).0;
            (U256::ZERO, if divisor.is_zero() { max } else { bounds(*a).1 / divisor })
        }
        InstKind::Shr(shift, value) => {
            let count = bounds(*shift).0;
            if count >= U256::from(256) {
                (U256::ZERO, U256::ZERO)
            } else {
                (U256::ZERO, bounds(*value).1 >> count.to::<usize>())
            }
        }
        InstKind::Select(_, a, b) => {
            let a = bounds(*a);
            let b = bounds(*b);
            (a.0.min(b.0), a.1.max(b.1))
        }
        InstKind::Phi(incoming) if !incoming.is_empty() => incoming
            .iter()
            .map(|&(_, value)| bounds(value))
            .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
            .unwrap(),
        kind => arithmetic_bounds(kind, max, bounds).unwrap_or(full),
    }
}

pub(crate) fn arithmetic_no_wrap(func: &Function, value: ValueId) -> bool {
    no_wrap_with(func, value, &|_| None)
}

pub(crate) fn no_wrap_with(
    func: &Function,
    value: ValueId,
    known: &impl Fn(ValueId) -> Option<(U256, U256)>,
) -> bool {
    let Value::Inst(id) = func.value(value) else { return true };
    let mut budget = 64;
    arithmetic_bounds(&func.inst(*id).kind, integer_max(func, value), |operand| {
        bounds_with(func, operand, 8, &mut budget, known)
    })
    .is_some()
}

fn arithmetic_bounds(
    kind: &InstKind,
    max: U256,
    mut bounds: impl FnMut(ValueId) -> (U256, U256),
) -> Option<(U256, U256)> {
    let range = match *kind {
        InstKind::Add(a, b) => {
            let (a, b) = (bounds(a), bounds(b));
            (a.0.checked_add(b.0)?, a.1.checked_add(b.1)?)
        }
        InstKind::Sub(a, b) => {
            let (a, b) = (bounds(a), bounds(b));
            (a.0.checked_sub(b.1)?, a.1.checked_sub(b.0)?)
        }
        InstKind::Mul(a, b) => {
            let (a, b) = (bounds(a), bounds(b));
            (a.0.checked_mul(b.0)?, a.1.checked_mul(b.1)?)
        }
        InstKind::Shl(count, value) => {
            let (count, value) = (bounds(count), bounds(value));
            if count.1 >= U256::from(256) {
                return None;
            }
            let hi = count.1.to::<usize>();
            if value.1 > (max >> hi) {
                return None;
            }
            (value.0 << count.0.to::<usize>(), value.1 << hi)
        }
        _ => return None,
    };
    (range.1 <= max).then_some(range)
}
