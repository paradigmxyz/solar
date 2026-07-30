//! EVM word-level constant operations used by MIR folding passes.
//!
//! These helpers intentionally do not reuse `Gcx::eval_const`:
//! sema evaluates Solidity source constants and reports semantic errors, while
//! MIR folding must match 256-bit EVM wrapping and zero-divisor semantics.

use crate::mir::{InstKind, ValueId};
use alloy_primitives::U256;

/// Evaluates a pure EVM word instruction.
///
/// Returns `Ok(None)` when `kind` has no word-level evaluator. Operand lookup
/// errors pass through unchanged.
pub(crate) fn eval_inst<E>(
    kind: &InstKind,
    mut get: impl FnMut(ValueId) -> Result<U256, E>,
) -> Result<Option<U256>, E> {
    Ok(Some(match *kind {
        InstKind::Add(a, b) => get(a)?.wrapping_add(get(b)?),
        InstKind::Sub(a, b) => get(a)?.wrapping_sub(get(b)?),
        InstKind::Mul(a, b) => get(a)?.wrapping_mul(get(b)?),
        InstKind::Div(a, b) => {
            let (a, b) = (get(a)?, get(b)?);
            if b.is_zero() { U256::ZERO } else { a / b }
        }
        InstKind::SDiv(a, b) => signed_div(get(a)?, get(b)?),
        InstKind::Mod(a, b) => {
            let (a, b) = (get(a)?, get(b)?);
            if b.is_zero() { U256::ZERO } else { a % b }
        }
        InstKind::SMod(a, b) => signed_mod(get(a)?, get(b)?),
        InstKind::Exp(a, b) => get(a)?.wrapping_pow(get(b)?),
        InstKind::AddMod(a, b, n) => {
            let (a, b, n) = (get(a)?, get(b)?, get(n)?);
            if n.is_zero() { U256::ZERO } else { a.add_mod(b, n) }
        }
        InstKind::MulMod(a, b, n) => {
            let (a, b, n) = (get(a)?, get(b)?, get(n)?);
            if n.is_zero() { U256::ZERO } else { a.mul_mod(b, n) }
        }
        InstKind::And(a, b) => get(a)? & get(b)?,
        InstKind::Or(a, b) => get(a)? | get(b)?,
        InstKind::Xor(a, b) => get(a)? ^ get(b)?,
        InstKind::Not(a) => !get(a)?,
        InstKind::Clz(a) => U256::from(get(a)?.leading_zeros() as u64),
        InstKind::Shl(shift, value) => shift_left(get(value)?, get(shift)?),
        InstKind::Shr(shift, value) => shift_right(get(value)?, get(shift)?),
        InstKind::Sar(shift, value) => sar(get(value)?, get(shift)?),
        InstKind::Byte(index, value) => byte(get(index)?, get(value)?),
        InstKind::SignExtend(size, value) => signextend(get(size)?, get(value)?),
        InstKind::Lt(a, b) => U256::from(get(a)? < get(b)?),
        InstKind::Gt(a, b) => U256::from(get(a)? > get(b)?),
        InstKind::SLt(a, b) => U256::from(signed_lt(get(a)?, get(b)?)),
        InstKind::SGt(a, b) => U256::from(signed_gt(get(a)?, get(b)?)),
        InstKind::Eq(a, b) => U256::from(get(a)? == get(b)?),
        InstKind::IsZero(a) => U256::from(get(a)?.is_zero()),
        _ => return Ok(None),
    }))
}

fn shift_left(value: U256, shift: U256) -> U256 {
    if shift >= U256::from(256) { U256::ZERO } else { value << shift.to::<usize>() }
}

fn shift_right(value: U256, shift: U256) -> U256 {
    if shift >= U256::from(256) { U256::ZERO } else { value >> shift.to::<usize>() }
}

/// EVM `SDIV`: two's-complement division. `x / 0 == 0` and `MIN / -1 == MIN`.
pub(crate) fn signed_div(a: U256, b: U256) -> U256 {
    if b.is_zero() {
        return U256::ZERO;
    }
    let negative = is_negative(a) != is_negative(b);
    let quotient = signed_abs(a) / signed_abs(b);
    if negative { U256::ZERO.wrapping_sub(quotient) } else { quotient }
}

/// EVM `SMOD`: the result takes the dividend's sign. `x % 0 == 0`.
pub(crate) fn signed_mod(a: U256, b: U256) -> U256 {
    if b.is_zero() {
        return U256::ZERO;
    }
    let remainder = signed_abs(a) % signed_abs(b);
    if is_negative(a) { U256::ZERO.wrapping_sub(remainder) } else { remainder }
}

/// EVM `SLT`: two's-complement signed less-than.
pub(crate) fn signed_lt(a: U256, b: U256) -> bool {
    match (is_negative(a), is_negative(b)) {
        (true, false) => true,
        (false, true) => false,
        _ => a < b,
    }
}

/// EVM `SGT`: two's-complement signed greater-than.
pub(crate) fn signed_gt(a: U256, b: U256) -> bool {
    signed_lt(b, a)
}

/// EVM `SAR`: shifts of 256 or more produce 0 for non-negative values and all
/// ones for negative values.
pub(crate) fn sar(value: U256, shift: U256) -> U256 {
    let negative = is_negative(value);
    if shift >= U256::from(256) {
        return if negative { U256::MAX } else { U256::ZERO };
    }

    let shift = shift.to::<usize>();
    if shift == 0 || !negative {
        return value >> shift;
    }

    let low_mask = (U256::from(1) << (256 - shift)) - U256::from(1);
    (value >> shift) | !low_mask
}

/// EVM `BYTE`: big-endian byte `index` of `value`; indices of 32 or more
/// produce 0.
pub(crate) fn byte(index: U256, value: U256) -> U256 {
    if index >= U256::from(32) {
        U256::ZERO
    } else {
        let shift = 8 * (31 - index.to::<usize>());
        (value >> shift) & U256::from(0xff)
    }
}

/// EVM `SIGNEXTEND`: extends the sign bit of byte `size`; sizes of 31 or more
/// are the identity.
pub(crate) fn signextend(size: U256, value: U256) -> U256 {
    if size >= U256::from(31) {
        return value;
    }
    let bit = size.to::<usize>() * 8 + 7;
    let sign_bit = U256::from(1) << bit;
    let mask = sign_bit - U256::from(1);
    if (value & sign_bit).is_zero() { value & mask } else { value | !mask }
}

fn is_negative(value: U256) -> bool {
    value.bit(255)
}

fn signed_abs(value: U256) -> U256 {
    if is_negative(value) { U256::ZERO.wrapping_sub(value) } else { value }
}
