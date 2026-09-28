//! Word semantics of the pure EVM opcodes.
//!
//! Each function computes one opcode's result from its operands in pop order: 256-bit wrapping
//! arithmetic, zero for a zero divisor or modulus, saturating shifts, and two's-complement
//! signed operations. The opcode table names the function of every pure opcode in its `eval`
//! column, and [`super::eval`] dispatches through the table, so constant folding, the MIR
//! interpreter, and EVM IR peepholes share one definition of each opcode.

use alloy_primitives::U256;
use std::cmp::Ordering;

type Word = U256;

pub(super) fn add(a: Word, b: Word) -> Word {
    a.wrapping_add(b)
}

pub(super) fn mul(a: Word, b: Word) -> Word {
    a.wrapping_mul(b)
}

pub(super) fn sub(a: Word, b: Word) -> Word {
    a.wrapping_sub(b)
}

pub(super) fn div(a: Word, b: Word) -> Word {
    if b.is_zero() { Word::ZERO } else { a.wrapping_div(b) }
}

pub(super) fn sdiv(a: Word, b: Word) -> Word {
    i256_div(a, b)
}

pub(super) fn rem(a: Word, b: Word) -> Word {
    if b.is_zero() { Word::ZERO } else { a.wrapping_rem(b) }
}

pub(super) fn smod(a: Word, b: Word) -> Word {
    i256_mod(a, b)
}

pub(super) fn addmod(a: Word, b: Word, n: Word) -> Word {
    a.add_mod(b, n)
}

pub(super) fn mulmod(a: Word, b: Word, n: Word) -> Word {
    a.mul_mod(b, n)
}

pub(super) fn exp(a: Word, b: Word) -> Word {
    a.wrapping_pow(b)
}

/// Sign-extends `value` from byte `ext`, counted from the least significant byte.
pub(crate) fn signextend(ext: Word, value: Word) -> Word {
    if ext < Word::from(31) {
        let bit_index = (8 * ext.as_limbs()[0] + 7) as usize;
        let mask = (Word::ONE << bit_index) - Word::ONE;
        if value.bit(bit_index) { value | !mask } else { value & mask }
    } else {
        value
    }
}

pub(super) fn lt(a: Word, b: Word) -> Word {
    Word::from(a < b)
}

pub(super) fn gt(a: Word, b: Word) -> Word {
    Word::from(a > b)
}

pub(super) fn slt(a: Word, b: Word) -> Word {
    Word::from(i256_cmp(&a, &b) == Ordering::Less)
}

pub(super) fn sgt(a: Word, b: Word) -> Word {
    Word::from(i256_cmp(&a, &b) == Ordering::Greater)
}

pub(super) fn eq(a: Word, b: Word) -> Word {
    Word::from(a == b)
}

pub(super) fn iszero(a: Word) -> Word {
    Word::from(a.is_zero())
}

pub(super) fn and(a: Word, b: Word) -> Word {
    a & b
}

pub(super) fn or(a: Word, b: Word) -> Word {
    a | b
}

pub(super) fn xor(a: Word, b: Word) -> Word {
    a ^ b
}

pub(super) fn not(a: Word) -> Word {
    !a
}

pub(super) fn byte(index: Word, value: Word) -> Word {
    let index = word_to_usize_saturated(index);
    if index < 32 { Word::from(value.byte(31 - index)) } else { Word::ZERO }
}

pub(super) fn shl(shift: Word, value: Word) -> Word {
    let shift = word_to_usize_saturated(shift);
    if shift < 256 { value << shift } else { Word::ZERO }
}

pub(super) fn shr(shift: Word, value: Word) -> Word {
    let shift = word_to_usize_saturated(shift);
    if shift < 256 { value >> shift } else { Word::ZERO }
}

pub(super) fn sar(shift: Word, value: Word) -> Word {
    let shift = word_to_usize_saturated(shift);
    if shift < 256 {
        value.arithmetic_shr(shift)
    } else if value.bit(255) {
        Word::MAX
    } else {
        Word::ZERO
    }
}

pub(super) fn clz(a: Word) -> Word {
    Word::from(a.leading_zeros())
}

#[inline]
fn word_to_usize_saturated(value: Word) -> usize {
    value.try_into().unwrap_or(usize::MAX)
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

/// Signed division, zero for a zero divisor, wrapping `MIN / -1` to `MIN`.
#[inline]
pub(crate) fn i256_div(mut first: Word, mut second: Word) -> Word {
    let second_sign = i256_sign_compl(&mut second);
    if second_sign == Sign::Zero {
        return Word::ZERO;
    }

    let first_sign = i256_sign_compl(&mut first);
    if first == MIN_NEGATIVE_VALUE && second == Word::from(1) {
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

/// Signed remainder with the dividend's sign, zero for a zero divisor.
#[inline]
pub(crate) fn i256_mod(mut first: Word, mut second: Word) -> Word {
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
