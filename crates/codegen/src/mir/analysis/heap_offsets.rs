//! Which operands of a pointer computation keep its result above the pointer, and which selects
//! raise a word to the heap floor.
//!
//! A heap pointer plus a word is a heap pointer only when the sum cannot wrap the address space
//! around to low memory, which takes a word of 2^256 minus an address. The offsets accepted here
//! are words whose bounds prove they cannot: constants and sizes below 2^64, sums, products, and
//! shifts of such words, whether the arithmetic wraps or reverts on overflow (unsigned only), and a
//! counter that starts bounded and only adds bounded steps to itself, in one addition or a chain of
//! them, which memory expansion keeps below 2^64 because each step costs gas. Anything else, such
//! as an argument, a loaded word, a call result, or a difference of words, may be any word, as the
//! negative step `mul(i, not(31))` and `sub(0x80, pointer)` show. The offsets of a memory object's
//! elements and fields are the exception: the compiler checks them against the object's bounds,
//! so a `ptradd` keeps its pointer, and `lower-memory-objects` marks the additions it makes for
//! them.
//!
//! Stepping back from a pointer by a constant stays above the heap prefix the frames reserve, but
//! a loop that steps back on every iteration walks down through the frames without limit.

use crate::mir::{ArithmeticKind, CheckedOp, Function, InstKind, Value, ValueId};
use smallvec::SmallVec;
use solar_data_structures::bit_set::DenseBitSet;

/// Returns the operands of the computation of `value` that a pointer stays above itself through:
/// the pointer that a bounded word is added to, or from which a constant of at most `max_step_back`
/// is subtracted outside a loop that subtracts on every iteration.
pub(crate) fn pointer_bases(
    func: &Function,
    value: ValueId,
    max_step_back: u64,
) -> SmallVec<[ValueId; 2]> {
    let Value::Inst(inst) = func.value(value) else { return SmallVec::new() };
    let mut bases = SmallVec::new();
    match func.inst(*inst).kind {
        // The compiler checked a memory object's offset against its bounds.
        InstKind::Add(a, b) if func.inst(*inst).metadata.checked_object_offset() => {
            bases.push(a);
            bases.push(b);
        }
        InstKind::Add(a, b) => {
            if is_bounded_offset(func, b) {
                bases.push(a);
            }
            if is_bounded_offset(func, a) {
                bases.push(b);
            }
        }
        InstKind::PtrAdd(base, _) => bases.push(base),
        InstKind::Sub(base, delta)
            if func.value_u64(delta).is_some_and(|step| step <= max_step_back)
                && !steps_back_in_loop(func, value) =>
        {
            bases.push(base);
        }
        _ => {}
    }
    bases
}

/// Whether `offset` is a word that cannot wrap a pointer around the address space.
fn is_bounded_offset(func: &Function, offset: ValueId) -> bool {
    let mut visiting = DenseBitSet::new_empty(func.num_values());
    let mut steps = OFFSET_BOUND_STEPS;
    offset_bound(func, offset, &mut visiting, &mut steps).is_some()
}

/// The words [`offset_bound`] looks at before it gives up, so that shared operands, as in a chain
/// of `add(n, n)`, cannot make the walk exponential. A word it gives up on is not bounded.
const OFFSET_BOUND_STEPS: u32 = 256;

fn offset_bound(
    func: &Function,
    value: ValueId,
    visiting: &mut DenseBitSet<ValueId>,
    steps: &mut u32,
) -> Option<u128> {
    // Far below 2^255, so that sums and products of bounded words cannot wrap.
    const LIMIT: u128 = 1 << 100;
    let limited = |bound: Option<u128>| bound.filter(|&bound| bound <= LIMIT);
    if let Some(value) = func.value_u64(value) {
        return Some(value.into());
    }
    let Value::Inst(inst_id) = func.value(value) else { return None };
    if !visiting.insert(value) {
        return None;
    }
    *steps = steps.checked_sub(1)?;
    let mut bound = |value| offset_bound(func, value, visiting, steps);
    let result = match &func.inst(*inst_id).kind {
        InstKind::CalldataSize
        | InstKind::ReturnDataSize
        | InstKind::CodeSize
        | InstKind::MSize
        | InstKind::DataSize(_) => Some(u64::MAX.into()),
        InstKind::Add(a, b)
        | InstKind::CheckedBinary {
            op: CheckedOp::Add,
            arithmetic: ArithmeticKind::Unsigned(_),
            lhs: a,
            rhs: b,
        } => limited(bound(*a)?.checked_add(bound(*b)?)),
        InstKind::Mul(a, b)
        | InstKind::CheckedBinary {
            op: CheckedOp::Mul,
            arithmetic: ArithmeticKind::Unsigned(_),
            lhs: a,
            rhs: b,
        } => limited(bound(*a)?.checked_mul(bound(*b)?)),
        InstKind::Shl(shift, word) => {
            let shift = u32::try_from(func.value_u64(*shift)?).ok()?;
            limited(bound(*word)?.checked_mul(1u128.checked_shl(shift)?))
        }
        InstKind::Shr(_, word) | InstKind::Div(word, _) | InstKind::Mod(_, word) => bound(*word),
        InstKind::Zext(inner) => bound(*inner),
        InstKind::Select(_, a, b) => Some(bound(*a)?.max(bound(*b)?)),
        InstKind::And(a, b) => match (bound(*a), bound(*b)) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(bound), None) | (None, Some(bound)) => Some(bound),
            (None, None) => None,
        },
        InstKind::Phi(incoming) => {
            // A counter that starts bounded and only adds bounded steps to itself.
            let mut start = 0;
            let mut step = 0;
            for &(_, edge) in incoming {
                match advance(func, edge, value, visiting, steps) {
                    Some(delta) => step = step.max(delta),
                    None => start = start.max(offset_bound(func, edge, visiting, steps)?),
                }
            }
            limited(start.checked_add(step.checked_mul(u64::MAX.into())?))
        }
        _ => None,
    };
    visiting.remove(value);
    result
}

/// The bound of what `value` adds to the counter `phi`, when `value` is the counter plus a chain of
/// bounded steps, as the two additions of a loop that advances by `offset += 2; offset += 32`.
fn advance(
    func: &Function,
    value: ValueId,
    phi: ValueId,
    visiting: &mut DenseBitSet<ValueId>,
    steps: &mut u32,
) -> Option<u128> {
    if value == phi {
        return Some(0);
    }
    let Value::Inst(inst) = func.value(value) else { return None };
    let (InstKind::Add(a, b)
    | InstKind::CheckedBinary {
        op: CheckedOp::Add,
        arithmetic: ArithmeticKind::Unsigned(_),
        lhs: a,
        rhs: b,
    }) = func.inst(*inst).kind
    else {
        return None;
    };
    *steps = steps.checked_sub(1)?;
    [(a, b), (b, a)].into_iter().find_map(|(counter, step)| {
        let before = advance(func, counter, phi, visiting, steps)?;
        before.checked_add(offset_bound(func, step, visiting, steps)?)
    })
}

/// Whether the subtraction `value` computes from a word that, through phis and arithmetic,
/// depends on `value` itself, as a pointer that a loop steps back on every iteration does. A
/// computation too large to search counts as one.
fn steps_back_in_loop(func: &Function, value: ValueId) -> bool {
    let mut seen = DenseBitSet::new_empty(func.num_values());
    let mut pending = vec![value];
    let mut budget = 256;
    while let Some(next) = pending.pop() {
        let Value::Inst(inst) = func.value(next) else { continue };
        budget -= 1;
        if budget == 0 {
            return true;
        }
        let mut cycle = false;
        func.inst(*inst).kind.visit_operands(|operand| {
            cycle |= operand == value;
            if seen.insert(operand) {
                pending.push(operand);
            }
        });
        if cycle {
            return true;
        }
    }
    false
}

/// Whether `select cond, then_value, else_value` is `max(word, heap_floor)` as `heap-floor` makes
/// it: the choice between the floor and a word that is below it. The result lies at or above the
/// floor whatever the word.
pub(crate) fn is_floor_clamp(
    func: &Function,
    cond: ValueId,
    then_value: ValueId,
    else_value: ValueId,
) -> bool {
    let is_floor = |value: ValueId| matches!(func.value(value), Value::Inst(inst) if func.inst(*inst).kind == InstKind::HeapFloor);
    let Value::Inst(cond) = func.value(cond) else { return false };
    matches!(func.inst(*cond).kind, InstKind::Lt(word, floor)
        if is_floor(floor) && is_floor(then_value) && word == else_value)
}
