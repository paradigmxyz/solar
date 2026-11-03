//! The word functions that pure EVM opcodes compute.
//!
//! Each opcode row in `ops/opcodes.ops` names, with `eval`, the function of this module it
//! computes, with the EVM's wrapping arithmetic, zero divisors, and saturating shifts.
//! [`eval_opcode`](super::eval_opcode), generated from those rows, applies it to concrete
//! operands. Constant folding and the interpreters share it, so every consumer has one
//! definition of each opcode.

use alloy_primitives::U256;
use std::cmp::Ordering;

type Word = U256;

/// `a + b`, wrapping.
pub fn wrapping_add(a: Word, b: Word) -> Word {
    a.wrapping_add(b)
}

/// `a - b`, wrapping.
pub fn wrapping_sub(a: Word, b: Word) -> Word {
    a.wrapping_sub(b)
}

/// `a * b`, wrapping.
pub fn wrapping_mul(a: Word, b: Word) -> Word {
    a.wrapping_mul(b)
}

/// `base ** exponent`, wrapping.
pub fn wrapping_pow(base: Word, exponent: Word) -> Word {
    base.wrapping_pow(exponent)
}

/// `(a + b) % n`, zero when `n` is zero.
pub fn addmod(a: Word, b: Word, n: Word) -> Word {
    a.add_mod(b, n)
}

/// `(a * b) % n`, zero when `n` is zero.
pub fn mulmod(a: Word, b: Word, n: Word) -> Word {
    a.mul_mod(b, n)
}

/// Bitwise AND.
pub fn and(a: Word, b: Word) -> Word {
    a & b
}

/// Bitwise OR.
pub fn or(a: Word, b: Word) -> Word {
    a | b
}

/// Bitwise XOR.
pub fn xor(a: Word, b: Word) -> Word {
    a ^ b
}

/// Bitwise NOT.
pub fn not(a: Word) -> Word {
    !a
}

/// The number of leading zero bits.
pub fn clz(a: Word) -> Word {
    Word::from(a.leading_zeros())
}

/// Unsigned `a < b`.
pub fn lt(a: Word, b: Word) -> Word {
    Word::from(a < b)
}

/// Unsigned `a > b`.
pub fn gt(a: Word, b: Word) -> Word {
    Word::from(a > b)
}

/// Signed `a < b`.
pub fn slt(a: Word, b: Word) -> Word {
    Word::from(i256_cmp(&a, &b) == Ordering::Less)
}

/// Signed `a > b`.
pub fn sgt(a: Word, b: Word) -> Word {
    Word::from(i256_cmp(&a, &b) == Ordering::Greater)
}

/// `a == b`.
pub fn eq(a: Word, b: Word) -> Word {
    Word::from(a == b)
}

/// `a == 0`.
pub fn iszero(a: Word) -> Word {
    Word::from(a.is_zero())
}

/// Signed `a % b`, as `SMOD` computes it.
pub fn smod(a: Word, b: Word) -> Word {
    i256_mod(a, b)
}

/// Signed `a / b`, as `SDIV` computes it.
pub fn sdiv(a: Word, b: Word) -> Word {
    i256_div(a, b)
}

/// Unsigned `a / b`, zero when `b` is zero.
pub fn div(a: Word, b: Word) -> Word {
    if b.is_zero() { Word::ZERO } else { a.wrapping_div(b) }
}

/// Unsigned `a % b`, zero when `b` is zero.
pub fn rem(a: Word, b: Word) -> Word {
    if b.is_zero() { Word::ZERO } else { a.wrapping_rem(b) }
}

/// Sign-extends `value` from its byte `ext`, counted from the least significant byte, as
/// `SIGNEXTEND` does.
pub fn signextend(ext: Word, value: Word) -> Word {
    if ext < Word::from(31) {
        let bit_index = (8 * ext.wrapping_to::<u64>() + 7) as usize;
        let mask = (Word::ONE << bit_index) - Word::ONE;
        if value.bit(bit_index) { value | !mask } else { value & mask }
    } else {
        value
    }
}

pub fn byte(index: Word, value: Word) -> Word {
    let index = index.saturating_to::<usize>();
    if index < 32 { Word::from(value.byte(31 - index)) } else { Word::ZERO }
}

pub fn shl(shift: Word, value: Word) -> Word {
    let shift = shift.saturating_to::<usize>();
    if shift < 256 { value << shift } else { Word::ZERO }
}

pub fn shr(shift: Word, value: Word) -> Word {
    let shift = shift.saturating_to::<usize>();
    if shift < 256 { value >> shift } else { Word::ZERO }
}

pub fn sar(shift: Word, value: Word) -> Word {
    let shift = shift.saturating_to::<usize>();
    if shift < 256 {
        value.arithmetic_shr(shift)
    } else if value.bit(255) {
        Word::MAX
    } else {
        Word::ZERO
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(i8)]
enum Sign {
    Minus = -1,
    Zero = 0,
    Plus = 1,
}

const MIN_NEGATIVE_VALUE: Word = Word::from_limbs([
    0x0000000000000000,
    0x0000000000000000,
    0x0000000000000000,
    0x8000000000000000,
]);

const FLIPH_BITMASK_U64: u64 = 0x7fff_ffff_ffff_ffff;

#[inline]
fn i256_sign(value: &Word) -> Sign {
    if value.bit(Word::BITS - 1) {
        Sign::Minus
    } else if value.is_zero() {
        Sign::Zero
    } else {
        Sign::Plus
    }
}

#[inline]
fn i256_sign_compl(value: &mut Word) -> Sign {
    let sign = i256_sign(value);
    if sign == Sign::Minus {
        two_compl_mut(value);
    }
    sign
}

#[inline]
fn u256_remove_sign(value: &mut Word) {
    // SAFETY: A 256-bit word always has four limbs.
    unsafe {
        value.as_limbs_mut()[3] &= FLIPH_BITMASK_U64;
    }
}

#[inline]
fn two_compl_mut(value: &mut Word) {
    *value = two_compl(*value);
}

#[inline]
fn two_compl(value: Word) -> Word {
    value.wrapping_neg()
}

#[inline]
fn i256_cmp(first: &Word, second: &Word) -> Ordering {
    let first_sign = i256_sign(first);
    let second_sign = i256_sign(second);
    match first_sign.cmp(&second_sign) {
        Ordering::Equal => first.cmp(second),
        ordering => ordering,
    }
}

/// Divides two signed words, rounding towards zero, as `SDIV` does.
#[inline]
pub fn i256_div(mut first: Word, mut second: Word) -> Word {
    let second_sign = i256_sign_compl(&mut second);
    if second_sign == Sign::Zero {
        return Word::ZERO;
    }

    let first_sign = i256_sign_compl(&mut first);
    if first == MIN_NEGATIVE_VALUE && second == Word::ONE {
        return two_compl(MIN_NEGATIVE_VALUE);
    }

    let mut quotient = first / second;
    u256_remove_sign(&mut quotient);

    if (first_sign == Sign::Minus && second_sign != Sign::Minus)
        || (second_sign == Sign::Minus && first_sign != Sign::Minus)
    {
        two_compl(quotient)
    } else {
        quotient
    }
}

/// Returns the signed remainder of two words, with the sign of the dividend, as `SMOD` does.
#[inline]
pub fn i256_mod(mut first: Word, mut second: Word) -> Word {
    let first_sign = i256_sign_compl(&mut first);
    if first_sign == Sign::Zero {
        return Word::ZERO;
    }

    let second_sign = i256_sign_compl(&mut second);
    if second_sign == Sign::Zero {
        return Word::ZERO;
    }

    let mut remainder = first % second;
    u256_remove_sign(&mut remainder);

    if first_sign == Sign::Minus { two_compl(remainder) } else { remainder }
}
