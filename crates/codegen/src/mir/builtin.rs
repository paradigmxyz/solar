//! Opaque builtin call targets. Arguments use the ordinary internal-call operand list.

use std::sync::Arc;

use super::{AbiLayoutRef, ConcatPart, FunctionId, InstKind, MirType, RevertPayload, ValueId};

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
    /// A thin shared pointer keeps ordinary call instructions compact.
    Concat(Arc<Vec<MirType>>),
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
        Self::ICall { function: Callee::Builtin(Builtin::Require(kind)), args: args.into() }
    }

    /// Builds an opaque concat call with its specialized parameter types.
    pub(crate) fn concat(parts: Vec<ConcatPart>) -> Self {
        let types = parts
            .iter()
            .map(|part| match part {
                ConcatPart::Bytes(_) => MirType::MemoryObject(super::MemoryObjectKind::Bytes),
                ConcatPart::Fixed { size, .. } => MirType::FixedBytes(*size),
            })
            .collect();
        let args = parts.iter().map(ConcatPart::value).collect();
        // result = icall builtin concat<types>, arguments
        Self::ICall { function: Callee::Builtin(Builtin::Concat(Arc::new(types))), args }
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
