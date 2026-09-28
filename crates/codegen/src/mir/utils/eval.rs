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
    mir::{ArithmeticKind, Builtin, Callee, CheckedOp, InstKind, Semantics, ValueId},
};
use alloy_primitives::{I256, U256};
use smallvec::SmallVec;

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
            let value = get(value)?;
            let value =
                if value.bit((from - 1) as usize) { value | (U256::MAX << from) } else { value };
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

#[cfg(test)]
mod tests {
    use super::*;

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
