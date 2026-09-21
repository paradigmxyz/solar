//! EVM word-level evaluation used by MIR folding passes.
//!
//! These helpers intentionally do not reuse `Gcx::eval_const`:
//! sema evaluates Solidity source constants and reports semantic errors, while
//! MIR folding must match 256-bit EVM wrapping and zero-divisor semantics.

use crate::{
    backend::evm::op,
    mir::{
        ArithmeticKind, Builtin, Callee, CheckedOp, Function, InstKind, MirType, ResultKind,
        ValueId,
    },
};
use alloy_primitives::{I256, U256};
use std::cmp::Ordering;

type Word = U256;

/// Evaluates integer operations in their declared width, with EVM's total semantics.
pub(crate) fn eval_typed_inst<E>(
    func: &Function,
    kind: &InstKind,
    mut get: impl FnMut(ValueId) -> Result<U256, E>,
) -> Result<Option<U256>, E> {
    let operands = kind.operands();
    let bits = operands
        .first()
        .and_then(|&value| func.value_ty(value))
        .and_then(MirType::integer_bits)
        .unwrap_or(256);
    if bits > 256 {
        return Ok(None);
    }
    let signed = matches!(
        kind,
        InstKind::SDiv(..)
            | InstKind::SMod(..)
            | InstKind::SLt(..)
            | InstKind::SGt(..)
            | InstKind::Sar(..)
    );
    let mut index = 0;
    let value = eval_inst(kind, |value| {
        let word = get(value)?;
        let extend = signed && !(matches!(kind, InstKind::Sar(..)) && index == 0);
        index += 1;
        Ok(if extend { sign_extend(word, bits) } else { word })
    })?;
    Ok(value.map(|mut value| {
        if kind.op_def().result == ResultKind::Integer {
            if matches!(kind, InstKind::Clz(..)) {
                value -= U256::from(256 - bits);
            }
            value &= U256::MAX >> (256 - bits);
        }
        value
    }))
}

/// Sign-extends a zero-clean integer bit pattern to an EVM word.
pub(crate) fn sign_extend(value: U256, bits: u32) -> U256 {
    if bits < 256 && value.bit((bits - 1) as usize) { value | (U256::MAX << bits) } else { value }
}

/// Evaluates a pure EVM word instruction.
///
/// Returns `Ok(None)` when `kind` has no word-level evaluator. Operand lookup
/// errors pass through unchanged.
pub(crate) fn eval_inst<E>(
    kind: &InstKind,
    mut get: impl FnMut(ValueId) -> Result<U256, E>,
) -> Result<Option<U256>, E> {
    match *kind {
        InstKind::Trunc(value, bits) | InstKind::PtrToInt(value, bits) => {
            if bits == 0 || bits > 256 {
                return Ok(None);
            }
            return Ok(Some(get(value)? & (U256::MAX >> (256 - bits))));
        }
        InstKind::Zext(value) | InstKind::IntToPtr(value) | InstKind::Bitcast(value) => {
            return Ok(Some(get(value)?));
        }
        InstKind::Sext(value, from, to) => {
            if from == 0 || from >= to || to > 256 {
                return Ok(None);
            }
            let value = get(value)?;
            let value =
                if value.bit((from - 1) as usize) { value | (U256::MAX << from) } else { value };
            return Ok(Some(value & (U256::MAX >> (256 - to))));
        }
        _ => {}
    }
    if let InstKind::Ne(a, b) = *kind {
        return Ok(Some(U256::from(get(a)? != get(b)?)));
    }
    if let InstKind::CheckedBinary { op, arithmetic, lhs, rhs } = *kind {
        return Ok(eval_checked(op, arithmetic, get(lhs)?, get(rhs)?));
    }
    if let InstKind::ICall {
        function: Callee::Builtin(builtin @ (Builtin::CheckedAddMod | Builtin::CheckedMulMod)),
        args,
    } = kind
        && let &[a, b, modulus] = args.as_ref()
    {
        let modulus = get(modulus)?;
        if modulus.is_zero() {
            return Ok(None);
        }
        let opcode =
            if matches!(builtin, Builtin::CheckedAddMod) { op::ADDMOD } else { op::MULMOD };
        return Ok(eval_opcode(opcode, &[get(a)?, get(b)?, modulus]));
    }
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

/// Returns a value only when the complete checked operation succeeds.
fn eval_checked(op: CheckedOp, kind: ArithmeticKind, lhs: U256, rhs: U256) -> Option<U256> {
    let fits = |value| match kind {
        ArithmeticKind::Unsigned(bits) => bits == 256 || value >> bits == U256::ZERO,
        ArithmeticKind::Signed(bits) => signextend(U256::from(bits / 8 - 1), value) == value,
    };
    if !fits(lhs) || op != CheckedOp::Pow && !fits(rhs) {
        return None;
    }
    if op == CheckedOp::Pow {
        let mut exponent = rhs;
        let mut base = lhs;
        let mut power = U256::ONE;
        while !exponent.is_zero() {
            if exponent.bit(0) {
                power = eval_checked(CheckedOp::Mul, kind, power, base)?;
            }
            exponent >>= 1;
            if !exponent.is_zero() {
                base = eval_checked(CheckedOp::Mul, kind, base, base)?;
            }
        }
        return Some(power);
    }
    let result = match kind {
        ArithmeticKind::Unsigned(_) => match op {
            CheckedOp::Add => lhs.checked_add(rhs)?,
            CheckedOp::Sub => lhs.checked_sub(rhs)?,
            CheckedOp::Mul => lhs.checked_mul(rhs)?,
            CheckedOp::Div | CheckedOp::WrappingDiv => lhs.checked_div(rhs)?,
            CheckedOp::Rem => lhs.checked_rem(rhs)?,
            CheckedOp::Pow => unreachable!(),
        },
        ArithmeticKind::Signed(bits) => {
            let a = I256::from_raw(lhs);
            let b = I256::from_raw(rhs);
            match op {
                CheckedOp::Add => a.checked_add(b)?.into_raw(),
                CheckedOp::Sub => a.checked_sub(b)?.into_raw(),
                CheckedOp::Mul => a.checked_mul(b)?.into_raw(),
                CheckedOp::Div => a.checked_div(b)?.into_raw(),
                CheckedOp::WrappingDiv if !rhs.is_zero() => {
                    signextend(U256::from(bits / 8 - 1), i256_div(lhs, rhs))
                }
                CheckedOp::Rem if !rhs.is_zero() => i256_mod(lhs, rhs),
                CheckedOp::WrappingDiv | CheckedOp::Rem => return None,
                CheckedOp::Pow => unreachable!(),
            }
        }
    };
    fits(result).then_some(result)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{FunctionBuilder, Immediate, Value};
    use solar_interface::Ident;

    #[test]
    fn integer_arithmetic_at_every_width() {
        for bits in 1..=256 {
            let ty = MirType::Int(std::num::NonZeroU32::new(bits).unwrap());
            let mut function = Function::new(Ident::DUMMY);
            let mut builder = FunctionBuilder::new(&mut function);
            let a = builder.add_param(ty);
            let b = builder.add_param(ty);
            let mask = U256::MAX >> (256 - bits);
            let sign = U256::ONE << (bits - 1);
            for (kind, lhs, rhs, expected) in [
                (InstKind::Add(a, b), mask, U256::ONE, U256::ZERO),
                (InstKind::Sub(a, b), U256::ZERO, U256::ONE, mask),
                (InstKind::Mul(a, b), mask, mask, U256::ONE),
                (InstKind::Not(a), U256::ZERO, U256::ZERO, mask),
                (InstKind::SDiv(a, b), sign, mask, sign),
                (InstKind::SDiv(a, b), mask, U256::ZERO, U256::ZERO),
                (InstKind::SMod(a, b), mask, mask, U256::ZERO),
                (InstKind::SLt(a, b), sign, U256::ZERO, U256::ONE),
                (InstKind::SGt(a, b), U256::ZERO, sign, U256::ONE),
                (InstKind::Shl(a, b), U256::from(bits), mask, U256::ZERO),
                (InstKind::Shr(a, b), U256::from(bits), mask, U256::ZERO),
                (InstKind::Sar(a, b), U256::from(bits), sign, mask),
                (InstKind::Clz(a), U256::ZERO, U256::ZERO, U256::from(bits)),
                (InstKind::Clz(a), U256::ONE, U256::ZERO, U256::from(bits - 1)),
            ] {
                assert_eq!(
                    eval_typed_inst(&function, &kind, |value| Ok::<_, ()>(if value == a {
                        lhs
                    } else {
                        rhs
                    })),
                    Ok(Some(expected)),
                    "i{bits} {kind:?}"
                );
            }
            let constant =
                function.alloc_value(Value::Immediate(Immediate::for_type(Some(ty), mask)));
            assert_eq!(function.value_ty(constant), Some(ty));
        }
    }

    #[test]
    fn llvm_integer_and_pointer_casts() {
        let value = ValueId::new(0);
        let address_mask = U256::MAX >> 96;
        for (kind, input, expected) in [
            (InstKind::Trunc(value, 1), U256::from(2), U256::ZERO),
            (InstKind::Trunc(value, 1), U256::from(3), U256::ONE),
            (InstKind::Trunc(value, 160), U256::MAX, address_mask),
            (InstKind::Zext(value), U256::ONE, U256::ONE),
            (InstKind::Sext(value, 1, 256), U256::ONE, U256::MAX),
            (InstKind::Sext(value, 1, 160), U256::ONE, address_mask),
            (InstKind::Sext(value, 160, 256), address_mask, U256::MAX),
            (InstKind::Sext(value, 160, 256), U256::ONE, U256::ONE),
            (InstKind::PtrToInt(value, 160), U256::MAX, address_mask),
            (InstKind::IntToPtr(value), U256::MAX, U256::MAX),
            (InstKind::Bitcast(value), U256::MAX, U256::MAX),
        ] {
            assert_eq!(eval_inst(&kind, |_| Ok::<_, ()>(input)), Ok(Some(expected)), "{kind:?}");
        }
    }
}
