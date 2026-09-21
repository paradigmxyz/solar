//! Opaque builtin call targets. Arguments use the ordinary internal-call operand list.

use std::sync::Arc;

use super::{AbiLayoutRef, ConcatPart, FunctionId, InstKind, RevertPayload, ValueId, ValueLayout};

/// An internal call targets either a MIR definition or a builtin specialization.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Callee {
    Function(FunctionId),
    Builtin(Builtin),
}

/// Builtin identity and static type arguments, without value operands.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Builtin {
    Require(RequireKind),
    /// Conditional failure; the condition is the sole value argument.
    Check {
        is_zero: bool,
        failure: super::RevertKind,
    },
    /// SHA-256 of a bytes object, including precompile output allocation and returndata effects.
    Sha256,
    /// Left-aligned RIPEMD-160 of a bytes object, with the same effects as `sha256`.
    Ripemd160,
    /// Recover an address from hash, recovery ID, and signature words.
    EcRecover,
    /// ERC-7201 namespace slot derived from a bytes object.
    Erc7201,
    /// Solidity modular addition, which panics for a zero modulus.
    CheckedAddMod,
    /// Solidity modular multiplication, which panics for a zero modulus.
    CheckedMulMod,
    /// Send value to an address with a 2300 gas stipend, returning success.
    Send,
    /// Transfer value with a 2300 gas stipend, reverting with returndata on failure.
    Transfer,
    /// Copy the current returndata into a fresh bytes object (empty before Byzantium).
    ReturndataBytes,
    /// A thin shared pointer keeps ordinary call instructions compact.
    Concat(Arc<Vec<ValueLayout>>),
}

/// Error payload type for a require call.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RequireKind {
    ShortString,
    EmptyString,
    ErrorString,
    CustomError(AbiLayoutRef),
}

impl InstKind {
    /// Builds a builtin call with evaluated values in argument order.
    pub(crate) fn builtin(builtin: Builtin, args: impl Into<Box<[ValueId]>>) -> Self {
        // icall builtin, arguments
        Self::ICall { function: Callee::Builtin(builtin), args: args.into() }
    }

    /// Builds an opaque require call with evaluated values in argument order.
    pub(crate) fn require(condition: ValueId, payload: RevertPayload) -> Self {
        let mut args = vec![condition];
        let kind = match payload {
            RevertPayload::ShortString { length, data } => {
                args.extend([length, data]);
                RequireKind::ShortString
            }
            RevertPayload::EmptyString => RequireKind::EmptyString,
            RevertPayload::ErrorString(value) => {
                args.push(value);
                RequireKind::ErrorString
            }
            RevertPayload::CustomError { selector, layout, values } => {
                args.push(selector);
                args.extend(values);
                RequireKind::CustomError(layout)
            }
        };
        // icall builtin require<kind>, condition, payload_arguments
        Self::builtin(Builtin::Require(kind), args)
    }

    /// Builds an opaque concat call with its specialized parameter types.
    pub(crate) fn concat(parts: Vec<ConcatPart>) -> Self {
        let types = parts
            .iter()
            .map(|part| match part {
                ConcatPart::Bytes(_) => ValueLayout::MemoryObject(super::MemoryObjectKind::Bytes),
                ConcatPart::Fixed { size, .. } => ValueLayout::FixedBytes(*size),
            })
            .collect();
        let args = parts.iter().map(ConcatPart::value).collect::<Box<[_]>>();
        // result = icall builtin concat<types>, arguments
        Self::builtin(Builtin::Concat(Arc::new(types)), args)
    }
}

impl RequireKind {
    /// Recovers an evaluated payload when expanding a well-formed call.
    pub(crate) fn payload(&self, args: &[ValueId]) -> Option<RevertPayload> {
        Some(match (self, args) {
            (Self::ShortString, &[length, data]) => RevertPayload::ShortString { length, data },
            (Self::EmptyString, []) => RevertPayload::EmptyString,
            (Self::ErrorString, &[value]) => RevertPayload::ErrorString(value),
            (Self::CustomError(layout), [selector, values @ ..])
                if values.len() == layout.types.len() =>
            {
                RevertPayload::CustomError {
                    selector: *selector,
                    layout: layout.clone(),
                    values: values.into(),
                }
            }
            _ => return None,
        })
    }
}

impl Builtin {
    /// Operand count for builtins with a fixed signature.
    pub(crate) const fn fixed_arity(&self) -> Option<usize> {
        match self {
            Self::Sha256 | Self::Ripemd160 | Self::Erc7201 => Some(1),
            Self::EcRecover => Some(4),
            Self::CheckedAddMod | Self::CheckedMulMod => Some(3),
            Self::Send | Self::Transfer => Some(2),
            Self::ReturndataBytes => Some(0),
            Self::Require(_) | Self::Check { .. } | Self::Concat(_) => None,
        }
    }
}
