//! Evaluated require payloads. A failed require preserves the exact ABI bytes;
//! an empty string remains Error(string), distinct from a message-free revert.
//! Dynamic/custom payloads use direct ABI semantics, not a compiler-lowered body.

use crate::machine::{Outcome, Run, RuntimeValue, limit};
use alloy_primitives::U256;
use solar_mir::{AbiEncodeMode, AbiLayout, AbiType, Function, PtrLocation, RequireKind, ValueId};
use std::ops::ControlFlow;

fn word(value: &RuntimeValue) -> ControlFlow<Outcome, U256> {
    if let RuntimeValue::Word(value) = value {
        ControlFlow::Continue(*value)
    } else {
        ControlFlow::Break(Outcome::Unsupported("require scalar representation"))
    }
}

pub(in crate::machine) fn literal_error(bytes: &[u8]) -> Vec<u8> {
    let mut data = vec![0x08, 0xc3, 0x79, 0xa0];
    data.extend(U256::from(32).to_be_bytes::<32>());
    data.extend(U256::from(bytes.len()).to_be_bytes::<32>());
    data.extend(bytes);
    data.resize(68 + bytes.len().next_multiple_of(32), 0);
    data
}

impl Run<'_, '_, '_> {
    pub(in crate::machine) fn semantic_require(
        &mut self,
        body: &Function,
        kind: &RequireKind,
        args: &[ValueId],
    ) -> ControlFlow<Outcome> {
        let values = self.frame().read_values(args)?.into_iter().collect::<Vec<_>>();
        let [condition, rest @ ..] = values.as_slice() else {
            return ControlFlow::Break(Outcome::Unsupported("require argument count"));
        };
        if !word(condition)?.is_zero() {
            return ControlFlow::Continue(());
        }
        let payload = match kind {
            RequireKind::EmptyString => literal_error(&[]),
            RequireKind::ShortString => {
                let [length, data] = rest else {
                    return ControlFlow::Break(Outcome::Unsupported(
                        "require short argument count",
                    ));
                };
                let mut payload = vec![0x08, 0xc3, 0x79, 0xa0];
                payload.extend(U256::from(32).to_be_bytes::<32>());
                payload.extend(word(length)?.to_be_bytes::<32>());
                payload.extend(word(data)?.to_be_bytes::<32>());
                payload
            }
            RequireKind::LongString(bytes) => {
                let bytes = bytes.as_byte_str();
                if bytes.len() > 65536 {
                    return ControlFlow::Break(Outcome::Unsupported(
                        "require literal exceeds 64 KiB",
                    ));
                }
                literal_error(bytes)
            }
            RequireKind::ErrorString | RequireKind::CustomError(_) => {
                let (selector, values, operands, types) = match kind {
                    RequireKind::ErrorString => (
                        U256::from(0x08c379a0_u32) << 224,
                        rest,
                        &args[1..],
                        vec![AbiType::Bytes(PtrLocation::Memory)],
                    ),
                    RequireKind::CustomError(layout) => {
                        let [selector, values @ ..] = rest else {
                            return ControlFlow::Break(Outcome::Unsupported(
                                "require selector missing",
                            ));
                        };
                        (word(selector)?, values, &args[2..], layout.types.to_vec())
                    }
                    _ => unreachable!(),
                };
                let mut sources = Vec::new();
                let mut effective = Vec::new();
                for ((value, ty), &arg) in values.iter().zip(types).zip(operands) {
                    let location = body.value_slice_location(arg);
                    let memory_slice = location == Some(PtrLocation::Memory)
                        && matches!(ty, AbiType::FixedArray { .. } | AbiType::Tuple(_));
                    sources.push((value.clone(), memory_slice));
                    effective.push(if let Some(location) = location {
                        ty.at_location(location)
                    } else {
                        ty
                    });
                }
                let layout = AbiLayout::new(effective);
                let encoded = self.semantic_abi_encode_values(
                    AbiEncodeMode::Scratch,
                    None,
                    Some(selector),
                    &sources,
                    &layout,
                )?;
                let RuntimeValue::Aggregate(fields) = encoded else {
                    return ControlFlow::Break(Outcome::Unsupported("require ABI slice missing"));
                };
                let [RuntimeValue::Word(base), RuntimeValue::Word(length)] = fields.as_slice()
                else {
                    return ControlFlow::Break(Outcome::Unsupported("require ABI slice shape"));
                };
                self.burn_words(*length)?;
                limit(self.memory.read(*base, *length))?
            }
        };
        ControlFlow::Break(Outcome::Revert(payload))
    }
}
