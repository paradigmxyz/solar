//! Packed arrays retain padded ABI leaves, declared memory strides and nested
//! object pointers. Plain contiguous words copy with snapshot semantics; cleaned
//! leaves read and write in order. Wrapped calldata arrays retain wordwise reads
//! except the explicitly unguarded full-word form. Limits reject whole executions.

use crate::machine::{Outcome, Run, RuntimeValue, calldata_word, limit, panic_data};
use alloy_primitives::U256;
use solar_mir::{AbiType, MemoryObjectLayout, PackedArraySource, PtrLocation};
use std::ops::ControlFlow;

pub(super) struct PackedArray {
    pub(super) element: AbiType,
    pub(super) data: U256,
    pub(super) length: usize,
    pub(super) bytes: U256,
    location: PtrLocation,
    stride: u64,
    object: bool,
    wordwise: bool,
}

fn width(element: &AbiType, depth: usize) -> Option<u64> {
    if depth == 0 {
        return None;
    }
    match element {
        AbiType::Word(_) | AbiType::Function => Some(32),
        AbiType::FixedArray { element, len, .. } => width(element, depth - 1)?.checked_mul(*len),
        _ => None,
    }
}

fn plain(element: &AbiType) -> bool {
    match element {
        AbiType::Word(None) => true,
        AbiType::FixedArray { element, .. } => plain(element),
        _ => false,
    }
}

impl Run<'_, '_, '_> {
    pub(super) fn prepare_packed_array(
        &mut self,
        input: RuntimeValue,
        element: &AbiType,
        source: PackedArraySource,
        wordwise: bool,
    ) -> ControlFlow<Outcome, PackedArray> {
        let Some(width) = width(element, 33) else {
            return ControlFlow::Break(Outcome::Unsupported("packed array layout depth or width"));
        };
        let (data, length, location, stride, object) = match (input, source) {
            (RuntimeValue::Word(pointer), PackedArraySource::Memory { layout }) => {
                let (length, stride, header) = match layout {
                    MemoryObjectLayout::DynamicArray { element_words } => {
                        (limit(self.memory.load(pointer))?, element_words, 32)
                    }
                    MemoryObjectLayout::FixedArray { len, element_words } => {
                        (U256::from(len), element_words, 0)
                    }
                    _ => return ControlFlow::Break(Outcome::Unsupported("packed memory array")),
                };
                (
                    pointer.wrapping_add(U256::from(header)),
                    length,
                    PtrLocation::Memory,
                    u64::from(stride),
                    true,
                )
            }
            (RuntimeValue::Aggregate(fields), PackedArraySource::Slice(location)) => {
                let [RuntimeValue::Word(data), RuntimeValue::Word(length)] = fields.as_slice()
                else {
                    return ControlFlow::Break(Outcome::Unsupported("packed array slice"));
                };
                (*data, *length, location, width / 32, false)
            }
            _ => return ControlFlow::Break(Outcome::Unsupported("packed array representation")),
        };
        let Some(bytes) = length.checked_mul(U256::from(width)) else {
            return ControlFlow::Break(Outcome::Revert(panic_data(U256::from(0x41))));
        };
        if length > U256::from(2048) || bytes > U256::from(65536) {
            return ControlFlow::Break(Outcome::Unsupported("packed array work limit"));
        }
        ControlFlow::Continue(PackedArray {
            element: element.clone(),
            data,
            length: length.to(),
            bytes,
            location,
            stride,
            object,
            wordwise,
        })
    }

    pub(super) fn write_packed_array(
        &mut self,
        array: &PackedArray,
        destination: U256,
        depth: usize,
        work: &mut usize,
        blocks: bool,
    ) -> ControlFlow<Outcome> {
        if depth == 0 || array.length > *work {
            return ControlFlow::Break(Outcome::Unsupported("packed array work limit"));
        }
        *work -= array.length;
        self.burn(array.length as u64)?;
        let contiguous = if array.object {
            matches!(array.element, AbiType::Word(None)) && array.stride == 1
        } else {
            plain(&array.element)
        };
        let unguarded = !array.wordwise && matches!(array.element, AbiType::Word(_));
        let wraps = array.data.overflowing_add(array.bytes).1;
        if blocks && contiguous && (array.location != PtrLocation::Calldata || unguarded || !wraps)
        {
            return self.abi_copy_source(array.location, destination, array.data, array.bytes);
        }
        let element_width = width(&array.element, depth).expect("prepared array element");
        for index in 0..array.length {
            let source = array.data.wrapping_add(
                U256::from(index).wrapping_mul(U256::from(array.stride) * U256::from(32)),
            );
            let destination =
                destination.wrapping_add(U256::from(index).wrapping_mul(U256::from(element_width)));
            match &array.element {
                AbiType::Word(cleanup) => {
                    let word = self.packed_array_word(source, array.location)?;
                    let word = self.abi_encode_clean(word, *cleanup)?;
                    limit(self.memory.store(destination, word))?;
                }
                AbiType::Function => {
                    let word = self.packed_array_word(source, array.location)?;
                    let word = if array.object { word & (U256::MAX << 64) } else { word };
                    limit(self.memory.store(destination, word))?;
                }
                AbiType::FixedArray { element, len, .. } => {
                    let nested_width = width(element, depth - 1).expect("prepared nested array");
                    let Some(length) = usize::try_from(*len).ok().filter(|n| *n <= 2048) else {
                        return ControlFlow::Break(Outcome::Unsupported("packed array work limit"));
                    };
                    let nested = PackedArray {
                        element: (**element).clone(),
                        data: if array.object { limit(self.memory.load(source))? } else { source },
                        length,
                        bytes: U256::from(*len) * U256::from(nested_width),
                        location: array.location,
                        stride: if array.object { 1 } else { nested_width / 32 },
                        object: array.object,
                        wordwise: array.wordwise,
                    };
                    self.write_packed_array(&nested, destination, depth - 1, work, array.object)?;
                }
                _ => return ControlFlow::Break(Outcome::Unsupported("packed array element")),
            }
        }
        ControlFlow::Continue(())
    }

    fn packed_array_word(
        &mut self,
        source: U256,
        location: PtrLocation,
    ) -> ControlFlow<Outcome, U256> {
        if location == PtrLocation::Memory {
            limit(self.memory.load(source))
        } else if location == PtrLocation::Calldata {
            let Some(transaction) = &self.transaction else {
                return ControlFlow::Break(Outcome::Unsupported("packed array calldata"));
            };
            ControlFlow::Continue(calldata_word(transaction.calldata, source))
        } else {
            ControlFlow::Break(Outcome::Unsupported("packed array location"))
        }
    }
}
