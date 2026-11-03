//! EVM word-level evaluation used by MIR folding passes.
//!
//! These helpers intentionally do not reuse `Gcx::const_eval`:
//! sema evaluates Solidity source constants and reports semantic errors, while
//! MIR folding must match 256-bit EVM wrapping and zero-divisor semantics.

use crate::{
    backend::evm::op,
    mir::{
        ArithmeticKind, Builtin, Callee, CheckedOp, Function, InstKind, MirType, ResultKind,
        Semantics, ValueId,
    },
};
use alloy_primitives::{I256, U256};

pub(crate) use solar_evmir::op::eval_opcode;
use solar_evmir::op::word::{i256_div, i256_mod, signextend};

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

/// Evaluates a pure instruction from the semantics its schema row declares.
///
/// A select reads its condition and then only the operand it chooses. Returns `Ok(None)` when
/// `kind` has no word-level evaluator. Operand lookup errors pass through unchanged.
pub(crate) fn eval_inst<E>(
    kind: &InstKind,
    mut get: impl FnMut(ValueId) -> Result<U256, E>,
) -> Result<Option<U256>, E> {
    Ok(match kind.semantics() {
        Some(Semantics::Word(value)) => Some(get(value)?),
        Some(Semantics::LowBits(value, bits @ 1..=256)) => {
            Some(get(value)? & (U256::MAX >> (256 - bits)))
        }
        Some(Semantics::SignExtend(value, from, to)) if 0 < from && from < to && to <= 256 => {
            Some(sign_extend(get(value)?, from) & (U256::MAX >> (256 - to)))
        }
        Some(Semantics::NotEqual(a, b)) => Some(U256::from(get(a)? != get(b)?)),
        Some(Semantics::Select(condition, if_true, if_false)) => {
            let chosen = if get(condition)?.is_zero() { if_false } else { if_true };
            Some(get(chosen)?)
        }
        Some(Semantics::Checked(op, arithmetic, lhs, rhs)) => {
            eval_checked(op, arithmetic, get(lhs)?, get(rhs)?)
        }
        Some(Semantics::Call(
            Callee::Builtin(builtin @ (Builtin::CheckedAddMod | Builtin::CheckedMulMod)),
            &[a, b, modulus],
        )) => {
            let modulus = get(modulus)?;
            let opcode =
                if matches!(builtin, Builtin::CheckedAddMod) { op::ADDMOD } else { op::MULMOD };
            if modulus.is_zero() { None } else { eval_opcode(opcode, &[get(a)?, get(b)?, modulus]) }
        }
        Some(Semantics::Opcode(name, operands)) => {
            let Some(opcode) = op::from_mnemonic(name) else { return Ok(None) };
            let mut values = [U256::ZERO; 3];
            let Some(values) = values.get_mut(..operands.len()) else { return Ok(None) };
            if eval_opcode(opcode, values).is_none() {
                return Ok(None);
            }
            for (value, operand) in values.iter_mut().zip(operands) {
                *value = get(operand)?;
            }
            eval_opcode(opcode, values)
        }
        _ => None,
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
