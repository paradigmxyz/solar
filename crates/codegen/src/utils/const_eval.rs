//! Constant-evaluation helpers with exact EVM semantics.
//!
//! Shared by the folding passes (`inst_simplify`, `pure_eval`, `sccp`) so they
//! all agree on signed arithmetic, shifts, and byte-level operations.

use alloy_primitives::U256;

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
