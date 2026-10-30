//! Bounded unsigned interval proofs for typed scalar arithmetic.
//!
//! Every unknown value starts with its integer type's full range. Recursive
//! producer queries refine that range only when the operation cannot wrap.
//! Callers can supply loop induction bounds; cyclic or deep definitions fall
//! back to the type range. A shared query budget bounds work across phi inputs. Address and affine
//! analyses use the same no-wrap proof before treating modular arithmetic as an ordinary offset.

use crate::{
    backend::evm::{op, select::InstKindOpcode},
    mir::{
        Callee, Function, FunctionId, InstKind, MirType, Module, Terminator, Value, ValueId,
        memory::EvmMemoryLayout,
    },
};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashMap;

#[inline]
pub(crate) fn integer_bits(func: &Function, value: ValueId) -> u32 {
    func.value_ty(value).and_then(MirType::integer_bits).unwrap_or(256)
}

#[inline]
pub(crate) fn integer_max(func: &Function, value: ValueId) -> U256 {
    let bits = integer_bits(func, value);
    integer_mask(bits)
}

/// The low `bits` set in an EVM word.
#[inline]
pub(crate) fn integer_mask(bits: u32) -> U256 {
    U256::MAX >> (256 - bits)
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
        // Runtime floors fit in u64. `emit_constructor_heap_start` computes
        // align32(fixed_end + args_size) + heap_guard: three u64 values and at most 31 bytes
        // of padding fit in 66 bits. The argument copy must succeed before MIR executes.
        // This bounds the compiler's floor, not a free-memory-pointer slot that assembly writes.
        InstKind::HeapFloor => (U256::from(EvmMemoryLayout::HEAP_START), integer_mask(66)),
        InstKind::Zext(value) | InstKind::PtrToInt(value, 256) | InstKind::IntToPtr(value) => {
            bounds(*value)
        }
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

/// Like [`unsigned_bounds`], bounding call results by `returns`, as [`return_bounds`] computes.
pub(crate) fn unsigned_bounds_with_returns(
    func: &Function,
    value: ValueId,
    returns: &FxHashMap<FunctionId, U256>,
) -> (U256, U256) {
    let known = |value| {
        let Value::Inst(inst) = func.value(value) else { return None };
        let InstKind::ICall { function: Callee::Function(callee), .. } = func.inst(*inst).kind
        else {
            return None;
        };
        returns.get(&callee).map(|&max| (U256::ZERO, max))
    };
    bounds_with(func, value, 8, &mut 64, &known)
}

/// Like [`unsigned_bounds`], bounding the result of an operation whose opcode is declared narrower
/// than a word, as `calldatasize` is.
pub(crate) fn unsigned_bounds_with_opcodes(func: &Function, value: ValueId) -> (U256, U256) {
    let known = |value| {
        let Value::Inst(inst) = func.value(value) else { return None };
        let bits = op::definition(func.inst(*inst).kind.evm_opcode()?)?.result_bits;
        (bits < 256).then(|| (U256::ZERO, integer_mask(u32::from(bits))))
    };
    bounds_with(func, value, 8, &mut 64, &known)
}

/// The largest word each function can return, for functions that return one word on every
/// path and whose returned values have a proven bound below their type's maximum.
///
/// A call to a function in the map yields at most its bound. Bounds only depend on functions
/// already in the map, so a recursive call contributes its type's full range and the result is
/// sound without assuming anything about the function being bounded.
pub(crate) fn return_bounds(module: &Module) -> FxHashMap<FunctionId, U256> {
    let mut bounds = FxHashMap::default();
    // Each round can bound the callers of the functions bounded by the one before; a few rounds
    // reach the short call chains of value helpers.
    for _ in 0..4 {
        let mut changed = false;
        for (id, func) in module.functions.iter_enumerated() {
            if bounds.contains_key(&id) || func.return_components() != [MirType::I256] {
                continue;
            }
            let mut widest = Some(U256::ZERO);
            for block in &func.blocks {
                widest = match &block.terminator {
                    Some(Terminator::Return { values }) => match values.as_slice() {
                        &[value] => widest.map(|widest| {
                            widest.max(unsigned_bounds_with_returns(func, value, &bounds).1)
                        }),
                        _ => None,
                    },
                    Some(Terminator::TailCall { .. }) => None,
                    _ => widest,
                };
            }
            if let Some(widest) = widest.filter(|&widest| widest < U256::MAX) {
                bounds.insert(id, widest);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    bounds
}
