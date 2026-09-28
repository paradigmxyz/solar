//! EVM word-level evaluation used by MIR folding passes.
//!
//! [`eval_inst`] evaluates an instruction's declared [`Semantics`] when they compute a word from
//! operand words alone: pure opcodes through the opcode table's word semantics, casts,
//! comparisons, and checked arithmetic that succeeds.
//!
//! These helpers intentionally do not reuse `Gcx::eval_const`:
//! sema evaluates Solidity source constants and reports semantic errors, while
//! MIR folding must match 256-bit EVM wrapping and zero-divisor semantics.

use crate::{
    backend::evm::op::{
        self,
        word::{i256_div, i256_mod, signextend},
    },
    mir::{
        ArithmeticKind, Builtin, Callee, CheckedOp, Function, InstKind, MirType, ResultKind,
        Semantics, ValueId,
    },
};
use alloy_primitives::{I256, U256};
use smallvec::SmallVec;

/// Evaluates integer operations in their declared width, with EVM's total semantics.
pub(crate) fn eval_typed_inst<E>(
    func: &Function,
    kind: &InstKind,
    mut get: impl FnMut(ValueId) -> Result<U256, E>,
) -> Result<Option<U256>, E> {
    if kind.op_def().result != ResultKind::Integer
        && !matches!(kind, InstKind::SLt(..) | InstKind::SGt(..) | InstKind::CheckedBinary { .. })
    {
        return eval_inst(kind, get);
    }
    let bits = kind
        .op()
        .first_operand()
        .and_then(|value| func.value_ty(value))
        .and_then(MirType::integer_bits)
        .unwrap_or(256);
    if bits == 256 {
        return eval_inst(kind, get);
    }
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
            | InstKind::CheckedBinary { arithmetic: ArithmeticKind::Signed(_), .. }
    );
    let mut index = 0;
    let value = eval_inst(kind, |value| {
        let word = get(value)?;
        let extend = signed
            && !(matches!(kind, InstKind::Sar(..)) && index == 0)
            && !(matches!(kind, InstKind::CheckedBinary { op: CheckedOp::Pow, .. }) && index == 1);
        index += 1;
        Ok(if extend { sign_extend(word, bits) } else { word })
    })?;
    Ok(value.map(|mut value| {
        if kind.op_def().result == ResultKind::Integer
            || matches!(kind, InstKind::CheckedBinary { .. })
        {
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

/// Evaluates an instruction whose declared semantics compute a word.
///
/// Returns `Ok(None)` when the instruction declares no semantics; see [`eval_semantics`].
pub(crate) fn eval_inst<E>(
    kind: &InstKind,
    get: impl FnMut(ValueId) -> Result<U256, E>,
) -> Result<Option<U256>, E> {
    match kind.semantics() {
        Some(semantics) => eval_semantics(semantics, get),
        None => Ok(None),
    }
}

/// Evaluates declared semantics that compute a word from operand words alone.
///
/// Returns `Ok(None)` before reading any operand when the semantics are not a word computation,
/// and `Ok(None)` when this instance computes no value: a cast with invalid widths, checked
/// arithmetic that panics, or a zero checked modulus. Operand lookup errors pass through
/// unchanged.
pub(crate) fn eval_semantics<E>(
    semantics: Semantics<'_>,
    mut get: impl FnMut(ValueId) -> Result<U256, E>,
) -> Result<Option<U256>, E> {
    Ok(match semantics {
        Semantics::Opcode(opcode, operands) => {
            // Other opcodes read memory, storage, or the environment.
            if !op::is_pure(opcode) {
                return Ok(None);
            }
            let mut words = SmallVec::<[U256; 3]>::new();
            for operand in operands {
                words.push(get(operand)?);
            }
            op::eval(opcode, &words)
        }
        Semantics::Word(value) => Some(get(value)?),
        Semantics::LowBits(value, bits) => {
            if bits == 0 || bits > 256 {
                return Ok(None);
            }
            Some(get(value)? & (U256::MAX >> (256 - bits)))
        }
        Semantics::SignExtend(value, from, to) => {
            if from == 0 || from >= to || to > 256 {
                return Ok(None);
            }
            let value = sign_extend(get(value)?, from);
            Some(value & (U256::MAX >> (256 - to)))
        }
        Semantics::NotEqual(a, b) => Some(U256::from(get(a)? != get(b)?)),
        Semantics::Checked(op, arithmetic, lhs, rhs) => {
            eval_checked(op, arithmetic, get(lhs)?, get(rhs)?)
        }
        Semantics::Call(
            Callee::Builtin(builtin @ (Builtin::CheckedAddMod | Builtin::CheckedMulMod)),
            &[a, b, modulus],
        ) => {
            let modulus = get(modulus)?;
            if modulus.is_zero() {
                return Ok(None);
            }
            let opcode =
                if matches!(builtin, Builtin::CheckedAddMod) { op::ADDMOD } else { op::MULMOD };
            op::eval(opcode, &[get(a)?, get(b)?, modulus])
        }
        Semantics::Select(..) | Semantics::Phi(_) | Semantics::Call(..) => None,
    })
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
