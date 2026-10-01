//! Expand dynamic storage-array loads during builtin conversion.
//!
//! Allocate the word-strided memory array, then copy through a counted loop. Scalar element
//! types retain their storage width and signed or fixed-bytes encoding; enum elements retain
//! their range check. Elements narrow enough for several to share a storage word are read one
//! word at a time: an outer loop loads each word of the data area once, and an inner loop
//! unpacks the elements it holds, the last word only as many as the length leaves. A word of
//! `uint8` elements then costs one `SLOAD` instead of 32. Bytes elements call one shared loader
//! so their validation and copy loops stay shared with direct bytes loads. The builtin pass
//! creates that helper before walking functions, preserving source context and successor phi
//! edges when either expansion splits a block. Nested struct and array layouts remain with their
//! existing lowering paths.

use crate::mir::{
    AllocationSemantics, FunctionBuilder, FunctionId, MemoryObjectKind, ValueId, ValueLayout,
};
use alloy_primitives::U256;

pub(super) fn load(
    builder: &mut FunctionBuilder<'_>,
    slot: ValueId,
    element: ValueLayout,
    enum_variants: Option<u64>,
    bytes_helper: Option<FunctionId>,
) -> ValueId {
    // length = sload(slot)
    // array = alloc_dynamic_array(length); set_length(array, length)
    // data_slot = storage_array_data_slot(slot)
    // for i in 0..length { array[i] = load/unpack(data_slot, i) }
    let length = builder.sload(slot);
    let (object, layout) =
        builder.alloc_dynamic_word_array(length, AllocationSemantics::SOLIDITY_UNINITIALIZED);
    let data_slot = builder.storage_array_data_slot(slot);
    if let Some(bytes) = shared_word_bytes(element) {
        // words = (length + per_slot - 1) / per_slot
        // for w in 0..words {
        //     word = sload(data_slot + w)
        //     first = w * per_slot
        //     count = min(per_slot, length - first)
        //     for j in 0..count { array[first + j] = unpack(word >> j * bytes * 8) }
        // }
        let per_slot = 32 / bytes;
        let per_slot_value = builder.imm(per_slot);
        let rounded = builder.add_u64_offset(length, u64::from(per_slot - 1));
        let words = builder.div(rounded, per_slot_value);
        builder.counted_loop(words, |builder, word_index| {
            let word_slot = builder.add(data_slot, word_index);
            let word = builder.sload(word_slot);
            let first = builder.mul(word_index, per_slot_value);
            let rest = builder.sub(length, first);
            let short = builder.lt(rest, per_slot_value);
            let count = builder.select(short, rest, per_slot_value);
            builder.counted_loop(count, |builder, index_in_word| {
                let bits = builder.imm(bytes * 8);
                let shift = builder.mul(index_in_word, bits);
                let shifted = builder.shr(shift, word);
                let value = unpack(builder, shifted, element, bytes);
                if let Some(variants) = enum_variants {
                    // panic if value >= variants
                    builder.validate_enum_value(variants, value);
                }
                // array[first + j] = value
                let index = builder.add(first, index_in_word);
                builder.memory_object_store_element(object, layout, index, value);
            });
        });
        return object;
    }
    builder.counted_loop(length, |builder, index| {
        let value = if element == ValueLayout::MemoryObject(MemoryObjectKind::Bytes) {
            // element_slot = data_slot + index; value = load_storage_bytes(element_slot)
            let element_slot = builder.add(data_slot, index);
            builder.icall(
                bytes_helper.expect("bytes array requires a bytes loader"),
                vec![element_slot],
                element.mir_type(),
            )
        } else {
            load_scalar(builder, data_slot, index, element)
        };
        if let Some(variants) = enum_variants {
            // panic if value >= variants
            builder.validate_enum_value(variants, value);
        }
        // array[index] = value
        builder.memory_object_store_element(object, layout, index, value);
    });
    object
}

fn load_scalar(
    builder: &mut FunctionBuilder<'_>,
    data_slot: ValueId,
    index: ValueId,
    element: ValueLayout,
) -> ValueId {
    // slot = data_slot + index / (32 / bytes)
    // word = sload(slot)
    // value = (word >> (index % (32 / bytes)) * bytes * 8) & mask
    let bytes = u32::from(element.type_size().expect("validated scalar array element").bytes());
    let per_slot = builder.imm(32 / bytes);
    let slot_index = builder.div(index, per_slot);
    let slot = builder.add(data_slot, slot_index);
    let word = builder.sload(slot);
    if bytes == 32 {
        return word;
    }
    let index_in_slot = builder.mod_(index, per_slot);
    let bits = builder.imm(bytes * 8);
    let shift = builder.mul(index_in_slot, bits);
    let shifted = builder.shr(shift, word);
    unpack(builder, shifted, element, bytes)
}

/// The width in bytes of a scalar element that shares its storage word with at least one other,
/// or `None` for one that fills a word alone.
fn shared_word_bytes(element: ValueLayout) -> Option<u32> {
    if matches!(element, ValueLayout::MemoryObject(_)) {
        return None;
    }
    let bytes = u32::from(element.type_size()?.bytes());
    (32 / bytes >= 2).then_some(bytes)
}

/// The element of `bytes` bytes in the low bits of `shifted`, in its value encoding.
fn unpack(
    builder: &mut FunctionBuilder<'_>,
    shifted: ValueId,
    element: ValueLayout,
    bytes: u32,
) -> ValueId {
    // value = shifted & mask
    let mask = builder.imm((U256::from(1) << (bytes * 8)) - U256::from(1));
    let value = builder.and(shifted, mask);
    match element {
        ValueLayout::Int(_) => {
            // value = signextend(bytes - 1, value)
            let index = builder.imm(bytes - 1);
            builder.signextend(index, value)
        }
        ValueLayout::FixedBytes(_) => {
            // value = value << (32 - bytes) * 8
            let shift = builder.imm((32 - bytes) * 8);
            builder.shl(shift, value)
        }
        _ => value,
    }
}
