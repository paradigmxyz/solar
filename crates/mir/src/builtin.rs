//! Opaque builtin call targets. Arguments use the ordinary internal-call operand list.

use super::{AbiLayoutRef, FunctionId, InstKind, RevertPayload, ValueId};
use solar_interface::ByteSymbol;

/// An internal call targets either a MIR definition or a builtin specialization.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Callee {
    Function(FunctionId),
    Builtin(Builtin),
}

/// Builtin identity and static type arguments, without value operands.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Builtin {
    Require(RequireKind),
    /// Conditional failure; the condition is the sole value argument.
    Check {
        is_zero: bool,
        failure: super::RevertKind,
    },
    /// SHA-256 of a bytes object, read from scratch memory; a failed precompile call reverts.
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
}

/// Error payload type for a require call.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RequireKind {
    ShortString,
    LongString(ByteSymbol),
    EmptyString,
    ErrorString,
    CustomError(AbiLayoutRef),
}

impl InstKind {
    /// Builds a builtin call with evaluated values in argument order.
    pub fn builtin(builtin: Builtin, args: impl Into<Box<[ValueId]>>) -> Self {
        // icall builtin, arguments
        Self::ICall { function: Callee::Builtin(builtin), args: args.into() }
    }

    /// Builds an opaque require call with evaluated values in argument order.
    pub fn require(condition: ValueId, payload: RevertPayload) -> Self {
        let mut args = Vec::new();
        args.push(condition);
        let kind = match payload {
            RevertPayload::ShortString { length, data } => {
                args.extend([length, data]);
                RequireKind::ShortString
            }
            RevertPayload::LongString(bytes) => RequireKind::LongString(bytes),
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
}

impl RequireKind {
    /// Recovers an evaluated payload when expanding a well-formed call.
    pub fn payload(&self, args: &[ValueId]) -> Option<RevertPayload> {
        Some(match (self, args) {
            (Self::ShortString, &[length, data]) => RevertPayload::ShortString { length, data },
            (Self::LongString(bytes), []) => RevertPayload::LongString(*bytes),
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
            Self::Require(_) | Self::Check { .. } => None,
        }
    }
}
