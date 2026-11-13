//! Recursive ABI encoding from live memory and calldata objects. Offset heads,
//! tails, scalar cleanup and storage byte strings retain their ordered effects.
//! Dynamic outputs are reserved only after encoding; static outputs reserve first.
//! Limits reject work rather than truncating arrays or manufacturing valid bytes.

use super::canonical;
use crate::machine::{Outcome, Run, RuntimeValue, limit, panic_data};
use alloy_primitives::U256;
use solar_mir::{
    AbiEncodeMode, AbiLayout, AbiType, AbiWordValidator, AllocationSemantics, PtrLocation,
};
use std::ops::ControlFlow;

#[derive(Clone)]
enum Source {
    Value(RuntimeValue, bool),
    Calldata { head: U256, base: U256, validate: bool },
    Memory { head: U256 },
}

#[derive(Clone, Copy)]
struct Destination {
    head: U256,
    base: U256,
    tail: U256,
    depth: usize,
}

fn reject<T>() -> ControlFlow<Outcome, T> {
    ControlFlow::Break(Outcome::Revert(Vec::new()))
}

fn word(value: &RuntimeValue) -> ControlFlow<Outcome, U256> {
    if let RuntimeValue::Word(word) = value {
        ControlFlow::Continue(*word)
    } else {
        ControlFlow::Break(Outcome::Unsupported("ABI scalar/pointer representation"))
    }
}

fn slice(value: &RuntimeValue) -> ControlFlow<Outcome, (U256, U256)> {
    if let RuntimeValue::Aggregate(fields) = value
        && let [RuntimeValue::Word(base), RuntimeValue::Word(length)] = fields.as_slice()
    {
        return ControlFlow::Continue((*base, *length));
    }
    ControlFlow::Break(Outcome::Unsupported("ABI slice representation"))
}

fn signed_lt(left: U256, right: U256) -> bool {
    (left ^ (U256::ONE << 255)) < (right ^ (U256::ONE << 255))
}

impl Run<'_, '_, '_> {
    pub(in crate::machine) fn semantic_abi_encode_values(
        &mut self,
        mode: AbiEncodeMode,
        base: Option<U256>,
        selector: Option<U256>,
        values: &[(RuntimeValue, bool)],
        layout: &AbiLayout,
    ) -> ControlFlow<Outcome, RuntimeValue> {
        if layout.types.iter().all(|ty| matches!(ty, AbiType::Word(_) | AbiType::Function)) {
            let mut words = Vec::new();
            for (value, _) in values {
                words.push(word(value)?);
            }
            return self.semantic_abi_encode(mode, base, selector, &words, layout);
        }
        let dynamic = layout.types.iter().any(AbiType::is_dynamic);
        let prefix = usize::from(selector.is_some()) * 4;
        let header = mode == AbiEncodeMode::Bytes
            || !dynamic && mode == AbiEncodeMode::Slice && selector.is_none();
        let pointer = if dynamic || mode == AbiEncodeMode::Scratch {
            if let Some(base) = base { base } else { limit(self.memory.load(U256::from(64)))? }
        } else {
            let total = layout.head_size() + prefix as u64;
            self.semantic_allocate(
                U256::from(total.next_multiple_of(32) + if header { 32 } else { 0 }),
                AllocationSemantics::INTERNAL,
            )?
        };
        let data = pointer.wrapping_add(U256::from(if header { 32 } else { 0 }));
        if !dynamic && header {
            limit(self.memory.store(pointer, U256::from(layout.head_size() + prefix as u64)))?;
        }
        if let Some(selector) = selector {
            limit(self.memory.store(data, selector))?;
        }
        let start = data.wrapping_add(U256::from(prefix));
        let mut work = 2048;
        let mut sources = Vec::new();
        for ((value, memory_slice), ty) in values.iter().zip(&layout.types) {
            sources.push(if *memory_slice {
                let (head, _) = slice(value)?;
                if ty.is_dynamic() {
                    Source::Value(RuntimeValue::Word(head), false)
                } else {
                    Source::Memory { head }
                }
            } else {
                Source::Value(value.clone(), false)
            });
        }
        let end = self.abi_encode_fields(&layout.types, sources, start, 33, &mut work)?;
        let total = end.wrapping_sub(start).wrapping_add(U256::from(prefix));
        let result_pointer = if dynamic && mode != AbiEncodeMode::Scratch {
            let size =
                end.wrapping_sub(start).wrapping_add(U256::from(if prefix == 0 { 0 } else { 32 }));
            let size = if header {
                let Some(size) = total.checked_add(U256::from(63)) else {
                    return ControlFlow::Break(Outcome::Revert(panic_data(U256::from(0x41))));
                };
                size & !U256::from(31)
            } else {
                size
            };
            let allocated = self.semantic_allocate(size, AllocationSemantics::INTERNAL)?;
            if header {
                limit(self.memory.store(allocated, total))?;
            }
            allocated
        } else {
            pointer
        };
        ControlFlow::Continue(if mode == AbiEncodeMode::Bytes {
            RuntimeValue::Word(result_pointer)
        } else {
            RuntimeValue::Aggregate(vec![
                RuntimeValue::Word(result_pointer.wrapping_add(U256::from(if header {
                    32
                } else {
                    0
                }))),
                RuntimeValue::Word(total),
            ])
        })
    }

    fn abi_encode_fields(
        &mut self,
        fields: &[AbiType],
        values: Vec<Source>,
        base: U256,
        depth: usize,
        work: &mut usize,
    ) -> ControlFlow<Outcome, U256> {
        if fields.len() != values.len() {
            return ControlFlow::Break(Outcome::Unsupported("ABI tuple arity"));
        }
        let mut destination = Destination {
            head: base,
            base,
            tail: base.wrapping_add(U256::from(fields.iter().map(AbiType::head_size).sum::<u64>())),
            depth,
        };
        for (field, value) in fields.iter().zip(values) {
            destination.tail = self.abi_encode_node(field, value, destination, work)?;
            destination.head = destination.head.wrapping_add(U256::from(field.head_size()));
        }
        ControlFlow::Continue(destination.tail)
    }

    fn abi_encode_node(
        &mut self,
        ty: &AbiType,
        mut source: Source,
        mut destination: Destination,
        work: &mut usize,
    ) -> ControlFlow<Outcome, U256> {
        if destination.depth == 0 || *work == 0 {
            return ControlFlow::Break(Outcome::Unsupported("ABI encoding work limit"));
        }
        *work -= 1;
        self.burn(1)?;
        destination.depth -= 1;
        if !ty.is_dynamic() {
            self.abi_encode_static(ty, source, destination, work)?;
            return ControlFlow::Continue(destination.tail);
        }
        let from_calldata = matches!(source, Source::Calldata { .. });
        if let Source::Calldata { head, base, .. } = source {
            let value = self.abi_encode_calldata_tail(ty, head, base)?;
            if let AbiType::DynamicArray { element, .. } = ty
                && let AbiType::Word(Some(validator)) = element.as_ref()
            {
                let (data, length) = slice(&value)?;
                if length > U256::from(*work) {
                    return ControlFlow::Break(Outcome::Unsupported("ABI encoding work limit"));
                }
                for index in 0..length.to::<usize>() {
                    let value = self.abi_read_source(
                        PtrLocation::Calldata,
                        data.wrapping_add(U256::from(index * 32)),
                    )?;
                    if !canonical(value, Some(*validator)) {
                        return reject();
                    }
                }
                *work -= length.to::<usize>();
            }
            source = Source::Value(value, false);
        }
        limit(
            self.memory.store(destination.head, destination.tail.wrapping_sub(destination.base)),
        )?;
        let Source::Value(value, _) = source else {
            return ControlFlow::Break(Outcome::Unsupported("ABI dynamic source representation"));
        };
        let base = destination.tail;
        match ty {
            AbiType::Bytes(PtrLocation::Storage) => {
                let slot = word(&value)?;
                let header = self.storage_read(slot)?;
                let (long, length) = Self::bytes_header(header)?;
                limit(self.memory.store(base, length))?;
                let count = length.wrapping_add(U256::from(31)) / U256::from(32);
                if count > U256::from(*work) {
                    return ControlFlow::Break(Outcome::Unsupported("ABI encoding work limit"));
                }
                *work -= count.to::<usize>();
                let first = if long { self.fixed_slot(&[slot])? } else { U256::ZERO };
                for index in 0..if long { count.to::<usize>() } else { 1 } {
                    let data = if long {
                        self.storage_read(first.wrapping_add(U256::from(index)))?
                    } else {
                        header & !U256::from(255)
                    };
                    limit(self.memory.store(base.wrapping_add(U256::from(32 + index * 32)), data))?;
                }
                ControlFlow::Continue(
                    base.wrapping_add(U256::from(32)).wrapping_add(count * U256::from(32)),
                )
            }
            AbiType::Bytes(declared) => {
                let (location, data, length) = self.abi_encode_range(
                    &value,
                    if from_calldata { PtrLocation::Calldata } else { *declared },
                    true,
                )?;
                let padded = length.wrapping_add(U256::from(31)) & !U256::from(31);
                limit(self.memory.store(base.wrapping_add(padded), U256::ZERO))?;
                limit(self.memory.store(base, length))?;
                self.abi_copy_source(location, base.wrapping_add(U256::from(32)), data, length)?;
                ControlFlow::Continue(base.wrapping_add(U256::from(32)).wrapping_add(padded))
            }
            AbiType::DynamicArray { element, location } => {
                let (location, data, length) = self.abi_encode_range(
                    &value,
                    if from_calldata { PtrLocation::Calldata } else { *location },
                    true,
                )?;
                limit(self.memory.store(base, length))?;
                self.abi_encode_array(
                    element,
                    length,
                    data,
                    location,
                    Destination { head: base.wrapping_add(U256::from(32)), ..destination },
                    work,
                )
            }
            AbiType::FixedArray { element, len, .. } => {
                let (location, data, _) = self.abi_encode_range(
                    &value,
                    if matches!(value, RuntimeValue::Aggregate(_)) {
                        PtrLocation::Calldata
                    } else {
                        PtrLocation::Memory
                    },
                    false,
                )?;
                self.abi_encode_array(
                    element,
                    U256::from(*len),
                    data,
                    location,
                    Destination { head: base, ..destination },
                    work,
                )
            }
            AbiType::Tuple(fields) => {
                let sources = match value {
                    RuntimeValue::Word(pointer) => {
                        let mut values = Vec::new();
                        for index in 0..fields.len() {
                            values.push(Source::Value(
                                RuntimeValue::Word(limit(
                                    self.memory.load(pointer.wrapping_add(U256::from(index * 32))),
                                )?),
                                true,
                            ));
                        }
                        values
                    }
                    _ => {
                        let (pointer, _) = slice(&value)?;
                        let mut head = pointer;
                        fields
                            .iter()
                            .map(|field| {
                                let source =
                                    Source::Calldata { head, base: pointer, validate: true };
                                head = head.wrapping_add(U256::from(field.head_size()));
                                source
                            })
                            .collect()
                    }
                };
                self.abi_encode_fields(fields, sources, base, destination.depth, work)
            }
            _ => unreachable!(),
        }
    }

    fn abi_encode_range(
        &mut self,
        value: &RuntimeValue,
        declared: PtrLocation,
        header: bool,
    ) -> ControlFlow<Outcome, (PtrLocation, U256, U256)> {
        if let RuntimeValue::Word(pointer) = value {
            let length = if header { limit(self.memory.load(*pointer))? } else { U256::ZERO };
            ControlFlow::Continue((
                PtrLocation::Memory,
                pointer.wrapping_add(U256::from(if header { 32 } else { 0 })),
                length,
            ))
        } else {
            let (data, length) = slice(value)?;
            ControlFlow::Continue((declared, data, length))
        }
    }

    fn abi_encode_static(
        &mut self,
        ty: &AbiType,
        source: Source,
        destination: Destination,
        work: &mut usize,
    ) -> ControlFlow<Outcome> {
        match ty {
            AbiType::Word(_) | AbiType::Function => {
                let cleanup = match ty {
                    AbiType::Word(cleanup) => *cleanup,
                    _ => None,
                };
                let (mut value, memory) = match source {
                    Source::Value(value, memory) => (word(&value)?, memory),
                    Source::Memory { head } => {
                        (self.abi_read_source(PtrLocation::Memory, head)?, true)
                    }
                    Source::Calldata { head, validate, .. } => {
                        let value = self.abi_read_source(PtrLocation::Calldata, head)?;
                        let cleanup = if matches!(ty, AbiType::Function) {
                            Some(AbiWordValidator::LeftAligned(192))
                        } else {
                            cleanup
                        };
                        if validate && !canonical(value, cleanup) {
                            return reject();
                        }
                        limit(self.memory.store(destination.head, value))?;
                        return ControlFlow::Continue(());
                    }
                };
                if matches!(ty, AbiType::Function) {
                    value = if memory { value & (U256::MAX << 64) } else { value << 64 };
                } else if memory {
                    value = self.abi_encode_clean(value, cleanup)?;
                }
                limit(self.memory.store(destination.head, value))?;
            }
            AbiType::Tuple(fields) => {
                let mut head = destination.head;
                match source {
                    Source::Value(RuntimeValue::Word(pointer), _) => {
                        for (index, field) in fields.iter().enumerate() {
                            let value = limit(
                                self.memory.load(pointer.wrapping_add(U256::from(index * 32))),
                            )?;
                            self.abi_encode_node(
                                field,
                                Source::Value(RuntimeValue::Word(value), true),
                                Destination { head, ..destination },
                                work,
                            )?;
                            head = head.wrapping_add(U256::from(field.head_size()));
                        }
                    }
                    Source::Value(value, _) => {
                        let (pointer, _) = slice(&value)?;
                        self.abi_encode_static(
                            ty,
                            Source::Calldata { head: pointer, base: pointer, validate: false },
                            destination,
                            work,
                        )?;
                    }
                    Source::Calldata { head: mut position, base, validate } => {
                        for field in fields {
                            self.abi_encode_node(
                                field,
                                Source::Calldata { head: position, base, validate },
                                Destination { head, ..destination },
                                work,
                            )?;
                            position = position.wrapping_add(U256::from(field.head_size()));
                            head = head.wrapping_add(U256::from(field.head_size()));
                        }
                    }
                    Source::Memory { head: mut position } => {
                        for field in fields {
                            self.abi_encode_node(
                                field,
                                Source::Memory { head: position },
                                Destination { head, ..destination },
                                work,
                            )?;
                            position = position.wrapping_add(U256::from(field.head_size()));
                            head = head.wrapping_add(U256::from(field.head_size()));
                        }
                    }
                }
            }
            AbiType::FixedArray { element, len, block_copy } => {
                let (position, location, validate, encoded) = match source {
                    Source::Value(RuntimeValue::Word(pointer), _) => {
                        (pointer, PtrLocation::Memory, false, false)
                    }
                    Source::Value(value, _) => {
                        (slice(&value)?.0, PtrLocation::Calldata, false, true)
                    }
                    Source::Calldata { head, validate, .. } => {
                        (head, PtrLocation::Calldata, validate, true)
                    }
                    Source::Memory { head } => (head, PtrLocation::Memory, false, true),
                };
                if *block_copy && location == PtrLocation::Calldata {
                    self.abi_copy_source(
                        location,
                        destination.head,
                        position,
                        U256::from(ty.head_size()),
                    )?;
                } else {
                    if U256::from(*len) > U256::from(*work) {
                        return ControlFlow::Break(Outcome::Unsupported("ABI encoding work limit"));
                    }
                    for index in 0..*len as usize {
                        let source = if location == PtrLocation::Memory && encoded {
                            Source::Memory {
                                head: position.wrapping_add(
                                    U256::from(index) * U256::from(element.head_size()),
                                ),
                            }
                        } else if location == PtrLocation::Memory {
                            let value = limit(
                                self.memory.load(position.wrapping_add(U256::from(index * 32))),
                            )?;
                            Source::Value(RuntimeValue::Word(value), true)
                        } else {
                            Source::Calldata {
                                head: position.wrapping_add(
                                    U256::from(index) * U256::from(element.head_size()),
                                ),
                                base: position,
                                validate,
                            }
                        };
                        self.abi_encode_node(
                            element,
                            source,
                            Destination {
                                head: destination.head.wrapping_add(
                                    U256::from(index) * U256::from(element.head_size()),
                                ),
                                ..destination
                            },
                            work,
                        )?;
                    }
                }
            }
            _ => unreachable!(),
        }
        ControlFlow::Continue(())
    }

    fn abi_encode_clean(
        &self,
        value: U256,
        cleanup: Option<AbiWordValidator>,
    ) -> ControlFlow<Outcome, U256> {
        ControlFlow::Continue(match cleanup {
            None => value,
            Some(AbiWordValidator::Unsigned(bits)) => {
                value & (U256::MAX >> (256 - usize::from(bits)))
            }
            Some(AbiWordValidator::LeftAligned(bits)) => {
                value & (U256::MAX << (256 - usize::from(bits)))
            }
            Some(AbiWordValidator::SignExtend(index)) => {
                crate::machine::sign_extend(value, ((index + 1) * 8) as u32)
            }
            Some(AbiWordValidator::Bool) => U256::from(!value.is_zero()),
            Some(AbiWordValidator::EnumRange(bound)) => {
                if value >= U256::from(bound) {
                    return ControlFlow::Break(Outcome::Revert(panic_data(U256::from(0x21))));
                }
                value
            }
        })
    }

    fn abi_encode_array(
        &mut self,
        element: &AbiType,
        length: U256,
        source: U256,
        location: PtrLocation,
        destination: Destination,
        work: &mut usize,
    ) -> ControlFlow<Outcome, U256> {
        if length > U256::from(*work) {
            return ControlFlow::Break(Outcome::Unsupported("ABI encoding work limit"));
        }
        let base = destination.head;
        let size = length.wrapping_mul(U256::from(element.head_size()));
        let mut dest = Destination { base, tail: base.wrapping_add(size), ..destination };
        if matches!(element, AbiType::Word(None))
            || location != PtrLocation::Memory && matches!(element, AbiType::Word(_))
            || location == PtrLocation::Returndata && matches!(element, AbiType::Function)
        {
            *work -= length.to::<usize>();
            self.abi_copy_source(location, base, source, size)?;
            return ControlFlow::Continue(dest.tail);
        }
        for index in 0..length.to::<usize>() {
            let value = if location == PtrLocation::Memory {
                Source::Value(
                    RuntimeValue::Word(limit(
                        self.memory.load(source.wrapping_add(U256::from(index * 32))),
                    )?),
                    true,
                )
            } else {
                Source::Calldata {
                    head: source.wrapping_add(U256::from(index) * U256::from(element.head_size())),
                    base: source,
                    validate: true,
                }
            };
            dest.tail = self.abi_encode_node(element, value, dest, work)?;
            dest.head = dest.head.wrapping_add(U256::from(element.head_size()));
        }
        ControlFlow::Continue(dest.tail)
    }

    fn abi_encode_calldata_tail(
        &mut self,
        ty: &AbiType,
        head: U256,
        base: U256,
    ) -> ControlFlow<Outcome, RuntimeValue> {
        let Some(transaction) = &self.transaction else {
            return ControlFlow::Break(Outcome::Unsupported(
                "ABI calldata requires an input buffer",
            ));
        };
        let end = U256::from(transaction.calldata.len());
        let offset = self.abi_read_source(PtrLocation::Calldata, head)?;
        let Some(slack) = ty.tail_size().checked_sub(1) else {
            return ControlFlow::Break(Outcome::Unsupported("ABI zero-sized calldata tail"));
        };
        let bound = end.wrapping_sub(base).wrapping_sub(U256::from(slack));
        if !signed_lt(offset, bound) {
            return reject();
        }
        let position = base.wrapping_add(offset);
        let (data, length) = match ty {
            AbiType::FixedArray { len, .. } => (position, U256::from(*len)),
            AbiType::Tuple(_) => (position, U256::from(ty.head_size())),
            _ => {
                let length = self.abi_read_source(PtrLocation::Calldata, position)?;
                if length > U256::from(u64::MAX) {
                    return reject();
                }
                let stride = match ty {
                    AbiType::DynamicArray { element, .. } => element.head_size(),
                    _ => 1,
                };
                let data = position.wrapping_add(U256::from(32));
                if signed_lt(end.wrapping_sub(length.wrapping_mul(U256::from(stride))), data) {
                    return reject();
                }
                (data, length)
            }
        };
        ControlFlow::Continue(RuntimeValue::Aggregate(vec![
            RuntimeValue::Word(data),
            RuntimeValue::Word(length),
        ]))
    }
}
