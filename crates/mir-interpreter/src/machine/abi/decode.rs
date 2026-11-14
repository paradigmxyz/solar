//! Recursive ABI decoding in the source address space. Tuple heads and relative
//! offsets are checked before descent; allocation, copy, cleanup and validation
//! retain their observable order. Work limits produce unsupported executions,
//! while malformed encodings revert and allocation failures retain Panic(0x41).

use super::{canonical, limit};
use crate::machine::{Outcome, Run, RuntimeValue, calldata_byte, calldata_word, panic_data};
use alloy_primitives::U256;
use solar_mir::{AbiParamLayout, AbiParamType, AllocationSemantics, PtrLocation, ValueLayout};
use std::ops::ControlFlow;

fn empty() -> ControlFlow<Outcome, U256> {
    ControlFlow::Break(Outcome::Revert(Vec::new()))
}

fn scalar(ty: &AbiParamType) -> bool {
    matches!(ty, AbiParamType::Scalar(_) | AbiParamType::Enum { .. })
}

fn function(ty: &AbiParamType) -> bool {
    matches!(ty, AbiParamType::Scalar(ValueLayout::Function))
}

#[derive(Clone, Copy)]
struct DecodeContext {
    end: U256,
    location: PtrLocation,
    static_tuple: bool,
    depth: usize,
}

impl Run<'_, '_, '_> {
    pub(in crate::machine) fn semantic_abi_values(
        &mut self,
        input: RuntimeValue,
        location: PtrLocation,
        layout: &AbiParamLayout,
    ) -> ControlFlow<Outcome, Option<RuntimeValue>> {
        if layout.types.len() <= 1 && layout.types.iter().all(scalar) {
            let value = self.semantic_abi_decode(input, location, layout)?;
            return ControlFlow::Continue(value.map(|word| {
                let bits = layout.types[0].mir_type().integer_bits().unwrap_or(256);
                RuntimeValue::Word(word & (U256::MAX >> (256 - bits)))
            }));
        }
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
        let head_size = layout.checked_head_size().expect("validated ABI layout");
        if length.bit(255) || length < U256::from(head_size) {
            return ControlFlow::Break(Outcome::Revert(Vec::new()));
        }
        let end = base.wrapping_add(length);
        let static_tuple = layout.types.iter().all(|ty| !ty.is_dynamic());
        let mut head = base;
        let mut values = Vec::new();
        let mut work = 2048;
        let context = DecodeContext { end, location, static_tuple, depth: 33 };
        for ty in &layout.types {
            let word = self.abi_decode_node(ty, head, base, context, &mut work)?;
            let bits = ty.mir_type().integer_bits().unwrap_or(256);
            values.push(RuntimeValue::Word(word & (U256::MAX >> (256 - bits))));
            head =
                head.wrapping_add(U256::from(ty.checked_head_size().expect("validated ABI head")));
        }
        ControlFlow::Continue(match values.len() {
            0 => None,
            1 => values.pop(),
            _ => Some(RuntimeValue::Aggregate(values)),
        })
    }

    pub(super) fn abi_read_source(
        &mut self,
        location: PtrLocation,
        position: U256,
    ) -> ControlFlow<Outcome, U256> {
        match location {
            PtrLocation::Memory => limit(self.memory.load(position)),
            PtrLocation::Calldata => {
                let Some(transaction) = &self.transaction else {
                    return ControlFlow::Break(Outcome::Unsupported(
                        "ABI calldata requires an input buffer",
                    ));
                };
                ControlFlow::Continue(calldata_word(transaction.calldata, position))
            }
            _ => ControlFlow::Break(Outcome::Unsupported("ABI input address space")),
        }
    }

    pub(super) fn abi_copy_source(
        &mut self,
        location: PtrLocation,
        destination: U256,
        source: U256,
        size: U256,
    ) -> ControlFlow<Outcome> {
        if location == PtrLocation::Returndata {
            if self.lacks(solar_evmir::op::RETURNDATACOPY) {
                return ControlFlow::Break(Outcome::Invalid);
            }
            let Some(world) = &self.world else {
                return ControlFlow::Break(Outcome::Unsupported(
                    "ABI returndata requires a live buffer",
                ));
            };
            let data = &world.effects.return_data;
            if source > U256::from(data.len()) || size > U256::from(data.len()) - source {
                return ControlFlow::Break(Outcome::ReturndataOutOfBounds);
            }
            let bytes = data[source.to::<usize>()..(source + size).to::<usize>()].to_vec();
            self.burn_words(size)?;
            return limit(self.memory.write(destination, &bytes));
        }
        self.burn_words(size)?;
        let bytes = if location == PtrLocation::Memory {
            limit(self.memory.read(source, size))?
        } else if location == PtrLocation::Calldata {
            let Some(transaction) = &self.transaction else {
                return ControlFlow::Break(Outcome::Unsupported(
                    "ABI calldata requires an input buffer",
                ));
            };
            (0..size.to::<u64>()).map(|i| calldata_byte(transaction.calldata, source, i)).collect()
        } else {
            return ControlFlow::Break(Outcome::Unsupported("ABI input address space"));
        };
        limit(self.memory.write(destination, &bytes))
    }

    fn abi_decode_node(
        &mut self,
        ty: &AbiParamType,
        head: U256,
        tuple_base: U256,
        context: DecodeContext,
        work: &mut usize,
    ) -> ControlFlow<Outcome, U256> {
        let DecodeContext { end, location, static_tuple, depth } = context;
        if depth == 0 || *work == 0 {
            return ControlFlow::Break(Outcome::Unsupported("ABI decode work limit"));
        }
        *work -= 1;
        self.burn(1)?;
        if scalar(ty) {
            let word = self.abi_read_source(location, head)?;
            if !canonical(word, ty.word_validator()) {
                return empty();
            }
            return ControlFlow::Continue(if function(ty) { word >> 64 } else { word });
        }
        let dynamic = ty.is_dynamic();
        let static_tuple = static_tuple || location == PtrLocation::Calldata && !dynamic;
        let base = if dynamic {
            let offset = self.abi_read_source(location, head)?;
            let target = tuple_base.wrapping_add(offset);
            let invalid = if matches!(ty, AbiParamType::Bytes | AbiParamType::DynamicArray(_)) {
                offset > U256::from(u64::MAX) || target.wrapping_add(U256::from(32)) > end
            } else {
                offset > end.wrapping_sub(tuple_base)
            };
            if invalid {
                return empty();
            }
            target
        } else {
            head
        };
        match ty {
            AbiParamType::Bytes => {
                let length = self.abi_read_source(location, base)?;
                let Some(rounded) = length.checked_add(U256::from(63)) else {
                    return ControlFlow::Break(Outcome::Revert(panic_data(U256::from(0x41))));
                };
                let object = self.semantic_allocate(
                    rounded & !U256::from(31),
                    AllocationSemantics::SOLIDITY_UNINITIALIZED,
                )?;
                limit(self.memory.store(object, length))?;
                let data = base.wrapping_add(U256::from(32));
                if length > end.wrapping_sub(data) {
                    return empty();
                }
                let destination = object.wrapping_add(U256::from(32));
                self.abi_copy_source(location, destination, data, length)?;
                let padding = length.wrapping_neg() & U256::from(31);
                limit(
                    self.memory
                        .write(destination.wrapping_add(length), &vec![0; padding.to::<usize>()]),
                )?;
                ControlFlow::Continue(object)
            }
            AbiParamType::DynamicArray(element) => {
                let length = self.abi_read_source(location, base)?;
                if length > U256::from(u64::MAX / 32) {
                    return ControlFlow::Break(Outcome::Revert(panic_data(U256::from(0x41))));
                }
                let object = self.semantic_allocate(
                    (length + U256::ONE) * U256::from(32),
                    AllocationSemantics::SOLIDITY_UNINITIALIZED,
                )?;
                limit(self.memory.store(object, length))?;
                let data = base.wrapping_add(U256::from(32));
                let stride = element.checked_head_size().expect("validated ABI stride");
                let maximum = if stride == 0 {
                    U256::ZERO
                } else {
                    end.wrapping_sub(data) / U256::from(stride)
                };
                if length > maximum {
                    return empty();
                }
                if length > U256::from(*work) {
                    return ControlFlow::Break(Outcome::Unsupported("ABI decode work limit"));
                }
                let count = length.to::<usize>();
                let destination = object.wrapping_add(U256::from(32));
                if scalar(element)
                    && (element.word_validator().is_none() || location == PtrLocation::Calldata)
                {
                    for index in 0..count {
                        self.abi_decode_node(
                            element,
                            data.wrapping_add(U256::from(index * 32)),
                            data,
                            DecodeContext { static_tuple: false, depth: depth - 1, ..context },
                            work,
                        )?;
                    }
                    self.abi_copy_source(location, destination, data, length * U256::from(32))?;
                } else {
                    self.abi_decode_elements(
                        element,
                        count,
                        data,
                        destination,
                        DecodeContext { static_tuple: false, depth: depth - 1, ..context },
                        work,
                    )?;
                }
                ControlFlow::Continue(object)
            }
            AbiParamType::FixedArray { element, len } => {
                if !static_tuple
                    && dynamic
                    && U256::from(ty.data_head_size()) > end.wrapping_sub(base)
                {
                    return empty();
                }
                if *len > *work as u64 {
                    return ControlFlow::Break(Outcome::Unsupported("ABI decode work limit"));
                }
                let object = self.semantic_allocate(
                    U256::from(*len) * U256::from(32),
                    AllocationSemantics::INTERNAL,
                )?;
                if static_tuple && scalar(element) && !function(element) {
                    if element.word_validator().is_some() {
                        for index in 0..*len as usize {
                            self.abi_decode_node(
                                element,
                                base.wrapping_add(U256::from(index * 32)),
                                base,
                                DecodeContext { static_tuple: true, depth: depth - 1, ..context },
                                work,
                            )?;
                        }
                    } else {
                        *work -= *len as usize;
                    }
                    self.abi_copy_source(
                        location,
                        object,
                        base,
                        U256::from(*len) * U256::from(32),
                    )?;
                } else {
                    self.abi_decode_elements(
                        element,
                        *len as usize,
                        base,
                        object,
                        DecodeContext { static_tuple, depth: depth - 1, ..context },
                        work,
                    )?;
                }
                ControlFlow::Continue(object)
            }
            AbiParamType::Tuple(fields) => {
                if !static_tuple
                    && dynamic
                    && U256::from(ty.data_head_size()) > end.wrapping_sub(base)
                {
                    return empty();
                }
                let carries_base =
                    !static_tuple && location == PtrLocation::Calldata && ty.has_dynamic_child();
                let object = self.semantic_allocate(
                    U256::from((fields.len() + usize::from(carries_base)) * 32),
                    AllocationSemantics::INTERNAL,
                )?;
                let mut source = base;
                for (index, field) in fields.iter().enumerate() {
                    let word = self.abi_decode_node(
                        field,
                        source,
                        base,
                        DecodeContext { static_tuple, depth: depth - 1, ..context },
                        work,
                    )?;
                    let word = if function(field) { word << 64 } else { word };
                    limit(self.memory.store(object.wrapping_add(U256::from(index * 32)), word))?;
                    source = source.wrapping_add(U256::from(
                        field.checked_head_size().expect("validated ABI field"),
                    ));
                }
                if carries_base {
                    limit(
                        self.memory.store(object.wrapping_add(U256::from(fields.len() * 32)), base),
                    )?;
                }
                ControlFlow::Continue(object)
            }
            _ => ControlFlow::Break(Outcome::Unsupported("ABI aggregate layout")),
        }
    }

    fn abi_decode_elements(
        &mut self,
        element: &AbiParamType,
        count: usize,
        source: U256,
        destination: U256,
        context: DecodeContext,
        work: &mut usize,
    ) -> ControlFlow<Outcome> {
        let stride = element.checked_head_size().expect("validated ABI element");
        for index in 0..count {
            let head = source.wrapping_add(U256::from(index) * U256::from(stride));
            let value = self.abi_decode_node(element, head, source, context, work)?;
            let value = if function(element) { value << 64 } else { value };
            limit(self.memory.store(destination.wrapping_add(U256::from(index * 32)), value))?;
        }
        ControlFlow::Continue(())
    }
}
