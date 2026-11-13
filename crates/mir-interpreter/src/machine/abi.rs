//! Direct scalar ABI semantics, independent of encoder/decoder expansion.
//!
//! Static word/function tuples retain allocation, selector and header ordering.
//! Packed literals/scalars retain their individual word stores and trailing zero
//! word through the packed submodule. Scalar decoders check
//! the signed tuple length and canonical encodings before returning a value.
//! Validation markers require a decoder-established obligation. Recursive shapes
//! use the encode/decode submodules without delegating to compiler lowering.

use super::{Outcome, Run, RuntimeValue, limit};
use alloy_primitives::U256;
use solar_mir::{
    AbiEncodeMode, AbiLayout, AbiParamLayout, AbiParamType, AbiType, AbiWordValidator,
    AllocationSemantics, Function, PtrLocation, Semantics, Value, ValueId, ValueLayout,
};
use std::ops::ControlFlow;

mod decode;
mod encode;
mod packed;
mod packed_refs;
mod require;

pub(super) use require::literal_error;

fn scalar_layout(layout: &AbiParamLayout) -> Option<(Option<AbiWordValidator>, bool)> {
    let [ty] = layout.types.as_ref() else {
        return None;
    };
    ty.is_scalar_word()
        .then(|| (ty.word_validator(), matches!(ty, AbiParamType::Scalar(ValueLayout::Function))))
}

fn canonical(value: U256, validator: Option<AbiWordValidator>) -> bool {
    match validator {
        None => true,
        Some(AbiWordValidator::Unsigned(bits)) => {
            bits == 256 || value >> usize::from(bits) == U256::ZERO
        }
        Some(AbiWordValidator::LeftAligned(bits)) => {
            bits == 256 || value & (U256::MAX >> usize::from(bits)) == U256::ZERO
        }
        Some(AbiWordValidator::SignExtend(index)) => {
            super::sign_extend(value, ((index + 1) * 8) as u32) == value
        }
        Some(AbiWordValidator::Bool) => value <= U256::ONE,
        Some(AbiWordValidator::EnumRange(bound)) => value < U256::from(bound),
    }
}

impl Run<'_, '_, '_> {
    pub(super) fn semantic_abi_encode(
        &mut self,
        mode: AbiEncodeMode,
        base: Option<U256>,
        selector: Option<U256>,
        words: &[U256],
        layout: &AbiLayout,
    ) -> ControlFlow<Outcome, RuntimeValue> {
        if words.len() != layout.types.len()
            || layout.types.iter().any(|ty| !matches!(ty, AbiType::Word(_) | AbiType::Function))
        {
            return ControlFlow::Break(Outcome::Unsupported(
                "ABI encoding requires a static scalar tuple",
            ));
        }
        let size = U256::from(words.len() * 32 + if selector.is_some() { 4 } else { 0 });
        let header =
            mode == AbiEncodeMode::Bytes || mode == AbiEncodeMode::Slice && selector.is_none();
        let pointer = if mode == AbiEncodeMode::Scratch {
            if let Some(base) = base { base } else { limit(self.memory.load(U256::from(64)))? }
        } else {
            self.semantic_allocate(
                ((size + U256::from(31)) & !U256::from(31))
                    + if header { U256::from(32) } else { U256::ZERO },
                AllocationSemantics::INTERNAL,
            )?
        };
        let data = pointer.wrapping_add(if header { U256::from(32) } else { U256::ZERO });
        if header {
            limit(self.memory.store(pointer, size))?;
        }
        if let Some(selector) = selector {
            limit(self.memory.store(data, selector))?;
        }
        let start = data.wrapping_add(if selector.is_some() { U256::from(4) } else { U256::ZERO });
        for (index, (&value, ty)) in words.iter().zip(&layout.types).enumerate() {
            let value = if matches!(ty, AbiType::Function) { value << 64 } else { value };
            limit(self.memory.store(start.wrapping_add(U256::from(index * 32)), value))?;
        }
        ControlFlow::Continue(if mode == AbiEncodeMode::Bytes {
            RuntimeValue::Word(pointer)
        } else {
            RuntimeValue::Aggregate(vec![RuntimeValue::Word(data), RuntimeValue::Word(size)])
        })
    }

    pub(super) fn semantic_abi_decode(
        &mut self,
        input: RuntimeValue,
        location: PtrLocation,
        layout: &AbiParamLayout,
    ) -> ControlFlow<Outcome, Option<U256>> {
        let rule =
            if layout.types.is_empty() {
                None
            } else {
                Some(scalar_layout(layout).ok_or_else(|| {
                    Outcome::Unsupported("ABI decoding requires zero or one scalar")
                }))
            };
        let rule = match rule {
            Some(Ok(rule)) => Some(rule),
            Some(Err(error)) => return ControlFlow::Break(error),
            None => None,
        };
        let (base, length) = match input {
            RuntimeValue::Word(object) if location == PtrLocation::Memory => {
                (object.wrapping_add(U256::from(32)), limit(self.memory.load(object))?)
            }
            RuntimeValue::Aggregate(fields) => {
                let [RuntimeValue::Word(base), RuntimeValue::Word(length)] = fields.as_slice()
                else {
                    return ControlFlow::Break(Outcome::Unsupported("undefined ABI decode input"));
                };
                (*base, *length)
            }
            _ => return ControlFlow::Break(Outcome::Unsupported("undefined ABI decode input")),
        };
        let head = if rule.is_some() { 32 } else { 0 };
        if length.bit(255) || length < U256::from(head) {
            return ControlFlow::Break(Outcome::Revert(Vec::new()));
        }
        let Some((validator, function)) = rule else {
            return ControlFlow::Continue(None);
        };
        let word = self.abi_read_source(location, base)?;
        if !canonical(word, validator) {
            return ControlFlow::Break(Outcome::Revert(Vec::new()));
        }
        ControlFlow::Continue(Some(if function { word >> 64 } else { word }))
    }

    pub(super) fn semantic_validate_abi(
        &mut self,
        body: &Function,
        value: ValueId,
        word: U256,
    ) -> ControlFlow<Outcome> {
        let Value::Inst(origin) = *body.value(value) else {
            return ControlFlow::Break(Outcome::Unsupported("ABI marker lacks decoder evidence"));
        };
        // validate_abi (shr raw, 64) retains the raw function word's obligation.
        // Never turn this evidence check into a synthetic runtime revert.
        if let Some(Semantics::Opcode("shr", operands)) = body.inst(origin).kind.semantics()
            && let [shift, raw] = operands.as_slice()
            && body.value(*shift).as_immediate().and_then(|v| v.as_u256()) == Some(U256::from(64))
        {
            let raw = self.frame().read(*raw)?;
            if !canonical(raw, Some(AbiWordValidator::LeftAligned(192))) {
                return ControlFlow::Break(Outcome::Unsupported("invalid decoder ABI evidence"));
            }
            return ControlFlow::Continue(());
        }
        let Some(Semantics::AbiDecode(_, layout)) = body.inst(origin).kind.semantics() else {
            return ControlFlow::Break(Outcome::Unsupported("ABI marker lacks decoder evidence"));
        };
        let Some((rule, function)) = scalar_layout(layout) else {
            return ControlFlow::Break(Outcome::Unsupported(
                "ABI marker requires a scalar decoder",
            ));
        };
        let rule = if function { Some(AbiWordValidator::Unsigned(192)) } else { rule };
        if !canonical(word, rule) {
            return ControlFlow::Break(Outcome::Unsupported("invalid decoder ABI evidence"));
        }
        ControlFlow::Continue(())
    }
}
