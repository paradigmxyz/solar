//! EVM word-level evaluation used by MIR folding passes.
//!
//! These helpers intentionally do not reuse `Gcx::eval_const`:
//! sema evaluates Solidity source constants and reports semantic errors, while
//! MIR folding must match 256-bit EVM wrapping and zero-divisor semantics.

use crate::{
    backend::evm::op,
    mir::{InstKind, ValueId},
};
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
    let Some(opcode) = kind.evm_opcode() else { return Ok(None) };
    let Some((inputs, 1)) = op::stack_io(opcode) else { return Ok(None) };
    if inputs > 3 {
        return Ok(None);
    }

    let mut values = [U256::ZERO; 3];
    let values = &mut values[..usize::from(inputs)];
    if eval_opcode(opcode, values).is_none() {
        return Ok(None);
    }
    let operands = kind.operands();
    if operands.len() != values.len() {
        return Ok(None);
    }
    for (value, operand) in values.iter_mut().zip(operands) {
        *value = get(operand)?;
    }
    Ok(eval_opcode(opcode, values))
}

/// Evaluates a pure EVM opcode with concrete operands in pop order.
pub(crate) fn eval_opcode(opcode: u8, operands: &[U256]) -> Option<U256> {
    Some(match (opcode, operands) {
        (op::ADD, &[a, b]) => a.wrapping_add(b),
        (op::SUB, &[a, b]) => a.wrapping_sub(b),
        (op::MUL, &[a, b]) => a.wrapping_mul(b),
        (op::DIV, &[a, b]) => div(a, b),
        (op::SDIV, &[a, b]) => i256_div(a, b),
        (op::MOD, &[a, b]) => rem(a, b),
        (op::SMOD, &[a, b]) => i256_mod(a, b),
        (op::EXP, &[a, b]) => a.wrapping_pow(b),
        (op::ADDMOD, &[a, b, n]) => a.add_mod(b, n),
        (op::MULMOD, &[a, b, n]) => a.mul_mod(b, n),
        (op::AND, &[a, b]) => a & b,
        (op::OR, &[a, b]) => a | b,
        (op::XOR, &[a, b]) => a ^ b,
        (op::NOT, &[a]) => !a,
        (op::CLZ, &[a]) => U256::from(a.leading_zeros()),
        (op::SHL, &[shift, value]) => shl(shift, value),
        (op::SHR, &[shift, value]) => shr(shift, value),
        (op::SAR, &[shift, value]) => sar(shift, value),
        (op::BYTE, &[index, value]) => byte(index, value),
        (op::SIGNEXTEND, &[size, value]) => signextend(size, value),
        (op::LT, &[a, b]) => U256::from(a < b),
        (op::GT, &[a, b]) => U256::from(a > b),
        (op::SLT, &[a, b]) => U256::from(i256_cmp(&a, &b) == Ordering::Less),
        (op::SGT, &[a, b]) => U256::from(i256_cmp(&a, &b) == Ordering::Greater),
        (op::EQ, &[a, b]) => U256::from(a == b),
        (op::ISZERO, &[a]) => U256::from(a.is_zero()),
        _ => return None,
    })
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
