//! EVM word-level evaluation used by MIR folding passes.
//!
//! These helpers intentionally do not reuse `Gcx::eval_const`:
//! sema evaluates Solidity source constants and reports semantic errors, while
//! MIR folding must match 256-bit EVM wrapping and zero-divisor semantics.

use crate::mir::{InstKind, ValueId};
use alloy_primitives::U256;
use std::cmp::Ordering;

type Word = U256;

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
        InstKind::Div(a, b) => div(get(a)?, get(b)?),
        InstKind::SDiv(a, b) => i256_div(get(a)?, get(b)?),
        InstKind::Mod(a, b) => rem(get(a)?, get(b)?),
        InstKind::SMod(a, b) => i256_mod(get(a)?, get(b)?),
        InstKind::Exp(a, b) => get(a)?.wrapping_pow(get(b)?),
        InstKind::AddMod(a, b, n) => get(a)?.add_mod(get(b)?, get(n)?),
        InstKind::MulMod(a, b, n) => get(a)?.mul_mod(get(b)?, get(n)?),
        InstKind::And(a, b) => get(a)? & get(b)?,
        InstKind::Or(a, b) => get(a)? | get(b)?,
        InstKind::Xor(a, b) => get(a)? ^ get(b)?,
        InstKind::Not(a) => !get(a)?,
        InstKind::Clz(a) => U256::from(get(a)?.leading_zeros()),
        InstKind::Shl(shift, value) => shl(get(shift)?, get(value)?),
        InstKind::Shr(shift, value) => shr(get(shift)?, get(value)?),
        InstKind::Sar(shift, value) => sar(get(shift)?, get(value)?),
        InstKind::Byte(index, value) => byte(get(index)?, get(value)?),
        InstKind::SignExtend(size, value) => signextend(get(size)?, get(value)?),
        InstKind::Lt(a, b) => U256::from(get(a)? < get(b)?),
        InstKind::Gt(a, b) => U256::from(get(a)? > get(b)?),
        InstKind::SLt(a, b) => U256::from(i256_cmp(&get(a)?, &get(b)?) == Ordering::Less),
        InstKind::SGt(a, b) => U256::from(i256_cmp(&get(a)?, &get(b)?) == Ordering::Greater),
        InstKind::Eq(a, b) => U256::from(get(a)? == get(b)?),
        InstKind::IsZero(a) => U256::from(get(a)?.is_zero()),
        _ => return Ok(None),
    }))
}

fn div(a: Word, b: Word) -> Word {
    if b.is_zero() { Word::ZERO } else { a.wrapping_div(b) }
}

fn rem(a: Word, b: Word) -> Word {
    if b.is_zero() { Word::ZERO } else { a.wrapping_rem(b) }
}

fn signextend(ext: Word, value: Word) -> Word {
    if ext < Word::from(31) {
        let bit_index = (8 * ext.as_limbs()[0] + 7) as usize;
        let mask = (Word::ONE << bit_index) - Word::ONE;
        if value.bit(bit_index) { value | !mask } else { value & mask }
    } else {
        value
    }
}

fn byte(index: Word, value: Word) -> Word {
    let index = word_to_usize_saturated(index);
    if index < 32 { Word::from(value.byte(31 - index)) } else { Word::ZERO }
}

fn shl(shift: Word, value: Word) -> Word {
    let shift = word_to_usize_saturated(shift);
    if shift < 256 { value << shift } else { Word::ZERO }
}

fn shr(shift: Word, value: Word) -> Word {
    let shift = word_to_usize_saturated(shift);
    if shift < 256 { value >> shift } else { Word::ZERO }
}

fn sar(shift: Word, value: Word) -> Word {
    let shift = word_to_usize_saturated(shift);
    if shift < 256 {
        value.arithmetic_shr(shift)
    } else if value.bit(255) {
        Word::MAX
    } else {
        Word::ZERO
    }
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

#[inline]
fn i256_div(mut first: Word, mut second: Word) -> Word {
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

#[inline]
fn i256_mod(mut first: Word, mut second: Word) -> Word {
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
