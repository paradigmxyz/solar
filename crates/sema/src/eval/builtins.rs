//! Evaluation of pure builtin functions.

use super::{
    EvalErrorKind as EE, erc7201_slot,
    value::{BytesKind, ConstValue, FixedBytes, IntScalar, IntTy, ValueTy},
};
use crate::{builtins::Builtin, ty::Gcx};
use alloy_primitives::{U256, keccak256};
use sha2::Digest;
use solar_interface::ByteSymbol;

/// Evaluates a call to `builtin` with the given argument values.
pub(super) fn call(
    gcx: Gcx<'_>,
    builtin: Builtin,
    args: Vec<ConstValue>,
) -> Result<ConstValue, EE> {
    Ok(match (builtin, &args[..]) {
        (Builtin::Keccak256, [data]) => fixed_bytes(&keccak256(bytes(data)?).0),
        (Builtin::Sha256, [data]) => fixed_bytes(&sha2::Sha256::digest(bytes(data)?)),
        (Builtin::Ripemd160, [data]) => fixed_bytes(&ripemd::Ripemd160::digest(bytes(data)?)),
        (Builtin::AddMod | Builtin::MulMod, [a, b, modulus]) => {
            let [a, b, modulus] = [a, b, modulus].map(|value| uint256(gcx, value.clone()));
            let (a, b, modulus) = (a?, b?, modulus?);
            if modulus.is_zero() {
                return Err(EE::DivisionByZero { literal: false });
            }
            let value = if builtin == Builtin::AddMod {
                a.add_mod(b, modulus)
            } else {
                a.mul_mod(b, modulus)
            };
            uint256_value(value)
        }
        (Builtin::Erc7201, [ConstValue::Bytes(data, BytesKind::Literal | BytesKind::String)]) => {
            uint256_value(erc7201_slot(data.as_byte_str()).into())
        }
        (Builtin::AbiEncode, args) => byte_array(abi_encode(args)?)?,
        (Builtin::AbiEncodePacked, args) => byte_array(abi_encode_packed(args)?)?,
        (Builtin::AbiEncodeWithSelector, [selector, args @ ..]) => {
            let ConstValue::FixedBytes(selector) =
                selector.clone().implicit_to(gcx, ValueTy::FixedBytes(4))?
            else {
                unreachable!()
            };
            byte_array([selector.as_slice(), &abi_encode(args)?].concat())?
        }
        (
            Builtin::AbiEncodeWithSignature,
            [ConstValue::Bytes(signature, BytesKind::Literal | BytesKind::String), args @ ..],
        ) => byte_array([&keccak256(signature.as_byte_str())[..4], &abi_encode(args)?].concat())?,
        (Builtin::BytesConcat, args) => {
            let mut data = Vec::new();
            for arg in args {
                match arg {
                    ConstValue::FixedBytes(value) => append(&mut data, value.as_slice())?,
                    ConstValue::Bytes(bytes, BytesKind::Literal | BytesKind::Bytes) => {
                        append(&mut data, bytes.as_byte_str())?;
                    }
                    _ => return Err(EE::UnsupportedExpr),
                }
            }
            byte_array(data)?
        }
        (Builtin::StringConcat, args) => {
            let mut data = Vec::new();
            for arg in args {
                let ConstValue::Bytes(bytes, BytesKind::Literal | BytesKind::String) = arg else {
                    return Err(EE::UnsupportedExpr);
                };
                append(&mut data, bytes.as_byte_str())?;
            }
            bytes_value(data, BytesKind::String)?
        }
        _ => return Err(EE::UnsupportedExpr),
    })
}

/// Returns a `bytesN` value holding all of the given bytes.
pub(super) fn fixed_bytes(bytes: &[u8]) -> ConstValue {
    ConstValue::FixedBytes(FixedBytes::new(bytes.len() as u8, bytes))
}

/// The size of the largest `bytes` or `string` value that folds. Larger values are computed at
/// runtime, so that doubling concatenations cannot exhaust memory at compile time.
const MAX_BYTES: usize = 1 << 16;

/// Returns a `bytes` value.
fn byte_array(data: Vec<u8>) -> Result<ConstValue, EE> {
    bytes_value(data, BytesKind::Bytes)
}

/// Appends `bytes` to the data of a `bytes` or `string` value, if the result is small enough to
/// fold.
fn append(data: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EE> {
    if data.len() + bytes.len() > MAX_BYTES {
        return Err(EE::BytesTooLarge);
    }
    data.extend_from_slice(bytes);
    Ok(())
}

/// Returns a `bytes` or `string` value, if it is small enough to fold.
fn bytes_value(data: Vec<u8>, kind: BytesKind) -> Result<ConstValue, EE> {
    if data.len() > MAX_BYTES {
        return Err(EE::BytesTooLarge);
    }
    Ok(ConstValue::Bytes(ByteSymbol::intern(&data), kind))
}

fn uint256_value(value: U256) -> ConstValue {
    ConstValue::Int(IntScalar::uint256(value))
}

/// Converts an argument to `uint256`.
fn uint256(gcx: Gcx<'_>, value: ConstValue) -> Result<U256, EE> {
    value.implicit_to(gcx, ValueTy::Int(IntTy::UINT256))?.as_u256().ok_or(EE::OutOfRange)
}

/// Returns the bytes of a `bytes` or `string` argument.
fn bytes(value: &ConstValue) -> Result<&[u8], EE> {
    match value {
        ConstValue::Bytes(data, _) => Ok(data.as_byte_str()),
        _ => Err(EE::UnsupportedExpr),
    }
}

/// ABI-encodes the given values, like `abi.encode`.
fn abi_encode(values: &[ConstValue]) -> Result<Vec<u8>, EE> {
    let mut head = Vec::with_capacity(32 * values.len());
    let mut tail = Vec::new();
    for value in values {
        if let ConstValue::Bytes(data, _) = value {
            let data = data.as_byte_str();
            head.extend_from_slice(&U256::from(32 * values.len() + tail.len()).to_be_bytes::<32>());
            tail.extend_from_slice(&U256::from(data.len()).to_be_bytes::<32>());
            tail.extend_from_slice(data);
            tail.resize(tail.len().next_multiple_of(32), 0);
            if 32 * values.len() + tail.len() > MAX_BYTES {
                return Err(EE::BytesTooLarge);
            }
        } else {
            head.extend_from_slice(&abi_word(value)?.to_be_bytes::<32>());
        }
    }
    head.extend_from_slice(&tail);
    Ok(head)
}

/// Returns the ABI head word of a statically encoded value.
fn abi_word(value: &ConstValue) -> Result<U256, EE> {
    value.clone().mobile()?.as_word().ok_or(EE::UnsupportedExpr)
}

/// Encodes the given values without padding, like `abi.encodePacked`.
fn abi_encode_packed(values: &[ConstValue]) -> Result<Vec<u8>, EE> {
    let mut data = Vec::new();
    for value in values {
        packed(value, &mut data)?;
        if data.len() > MAX_BYTES {
            return Err(EE::BytesTooLarge);
        }
    }
    Ok(data)
}

fn packed(value: &ConstValue, data: &mut Vec<u8>) -> Result<(), EE> {
    match value {
        // The type checker rejects packed literals, whose width is unclear.
        ConstValue::Int(value) => data.extend(value.to_be_bytes().ok_or(EE::UnsupportedExpr)?),
        ConstValue::Bool(value) => data.push(*value as u8),
        ConstValue::FixedBytes(value) => data.extend_from_slice(value.as_slice()),
        ConstValue::Address(address) | ConstValue::Contract(_, address) => {
            data.extend_from_slice(address.as_slice());
        }
        ConstValue::Bytes(bytes, _) => data.extend_from_slice(bytes.as_byte_str()),
        ConstValue::Enum(_, index) => data.push(u8::try_from(*index).map_err(|_| EE::OutOfRange)?),
        ConstValue::Udvt(_, value) => packed(value, data)?,
        ConstValue::Rational(_) => return Err(EE::Fractional),
    }
    Ok(())
}
