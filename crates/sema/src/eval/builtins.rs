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

/// The size of the largest `bytes` or `string` value that folds, so that doubling concatenations
/// cannot exhaust memory at compile time.
pub(super) const MAX_BYTES: usize = 1 << 16;

/// The largest number of values in an array or struct that folds. Every value takes at least an
/// ABI word.
pub(super) const MAX_VALUES: usize = MAX_BYTES / 32;

/// Returns a `bytes` value.
fn byte_array(data: Vec<u8>) -> Result<ConstValue, EE> {
    bytes_value(data, BytesKind::Bytes)
}

/// Appends `bytes` to the data of a `bytes` or `string` value, if the result is small enough to
/// fold.
fn append(data: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EE> {
    if data.len() + bytes.len() > MAX_BYTES {
        return Err(EE::ValueTooLarge);
    }
    data.extend_from_slice(bytes);
    Ok(())
}

/// Returns a `bytes` or `string` value, if it is small enough to fold.
pub(super) fn bytes_value(data: Vec<u8>, kind: BytesKind) -> Result<ConstValue, EE> {
    if data.len() > MAX_BYTES {
        return Err(EE::ValueTooLarge);
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

/// ABI-encodes the given values as a tuple, like `abi.encode`: the heads of all values, followed
/// by the tails of the dynamically encoded ones.
fn abi_encode(values: &[ConstValue]) -> Result<Vec<u8>, EE> {
    let head_len = values.iter().map(head_size).sum::<usize>();
    if head_len > MAX_BYTES {
        return Err(EE::ValueTooLarge);
    }
    let mut head = Vec::with_capacity(head_len);
    let mut tail = Vec::new();
    for value in values {
        if is_dynamic(value) {
            head.extend_from_slice(&word(head_len + tail.len()));
            tail.extend(abi_encode_value(value)?);
        } else {
            head.extend(abi_encode_value(value)?);
        }
        if head_len + tail.len() > MAX_BYTES {
            return Err(EE::ValueTooLarge);
        }
    }
    head.extend_from_slice(&tail);
    Ok(head)
}

/// ABI-encodes a single value.
fn abi_encode_value(value: &ConstValue) -> Result<Vec<u8>, EE> {
    Ok(match value {
        ConstValue::Bytes(data, _) => {
            let data = data.as_byte_str();
            let mut encoded = word(data.len()).to_vec();
            encoded.extend_from_slice(data);
            encoded.resize(encoded.len().next_multiple_of(32), 0);
            encoded
        }
        ConstValue::Struct(_, fields) => abi_encode(fields)?,
        ConstValue::Array { elements, dynamic } => {
            let mut encoded = if *dynamic { word(elements.len()).to_vec() } else { Vec::new() };
            encoded.extend(abi_encode(elements)?);
            encoded
        }
        ConstValue::Udvt(_, value) => abi_encode_value(value)?,
        value => {
            let word = value.clone().mobile()?.as_word().ok_or(EE::UnsupportedExpr)?;
            word.to_be_bytes::<32>().to_vec()
        }
    })
}

/// Returns whether a value is encoded in the tail of its tuple.
fn is_dynamic(value: &ConstValue) -> bool {
    match value {
        ConstValue::Bytes(..) | ConstValue::Array { dynamic: true, .. } => true,
        ConstValue::Struct(_, elements) | ConstValue::Array { elements, .. } => {
            elements.iter().any(is_dynamic)
        }
        ConstValue::Udvt(_, value) => is_dynamic(value),
        _ => false,
    }
}

/// Returns the size of the head of a value in its tuple.
fn head_size(value: &ConstValue) -> usize {
    match value {
        ConstValue::Struct(_, elements) | ConstValue::Array { elements, .. }
            if !is_dynamic(value) =>
        {
            elements.iter().map(head_size).sum()
        }
        _ => 32,
    }
}

fn word(value: usize) -> [u8; 32] {
    U256::from(value).to_be_bytes()
}

/// Encodes the given values without padding, like `abi.encodePacked`.
fn abi_encode_packed(values: &[ConstValue]) -> Result<Vec<u8>, EE> {
    let mut data = Vec::new();
    for value in values {
        packed(value, &mut data)?;
        if data.len() > MAX_BYTES {
            return Err(EE::ValueTooLarge);
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
        // Array elements are padded to words.
        ConstValue::Array { elements, .. } => {
            for element in elements {
                let word = element.as_word().ok_or(EE::UnsupportedExpr)?;
                data.extend_from_slice(&word.to_be_bytes::<32>());
            }
        }
        // Only codegen knows the bytecode of a contract, and type checking rejects packed structs.
        ConstValue::Code(..) | ConstValue::Struct(..) => return Err(EE::UnsupportedExpr),
        ConstValue::Rational(_) => return Err(EE::Fractional),
    }
    Ok(())
}
