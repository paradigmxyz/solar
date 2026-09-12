//! Expand dynamic storage-array loads during builtin conversion.
//!
//! Allocate the word-strided memory array, then copy through a counted loop. Scalar element
//! types retain their storage width and signed or fixed-bytes encoding; enum elements retain
//! their range check. Bytes elements call one shared loader so their validation and copy loops
//! stay shared with direct bytes loads. The builtin pass creates that helper before walking
//! functions, preserving source context and successor phi edges when either expansion splits
//! a block. Nested struct and array layouts remain with their existing lowering paths.

use crate::mir::{
    AllocationSemantics, FunctionBuilder, FunctionId, MemoryObjectKind, MirType, ValueId,
};
use alloy_primitives::U256;

pub(super) fn load(
    builder: &mut FunctionBuilder<'_>,
    slot: ValueId,
    element: MirType,
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
    builder.counted_loop(length, |builder, index| {
        let value = if element == MirType::MemoryObject(MemoryObjectKind::Bytes) {
            // element_slot = data_slot + index; value = load_storage_bytes(element_slot)
            let element_slot = builder.add(data_slot, index);
            builder.icall(
                bytes_helper.expect("bytes array requires a bytes loader"),
                vec![element_slot],
                element,
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
    element: MirType,
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
    let mask = builder.imm((U256::from(1) << (bytes * 8)) - U256::from(1));
    let value = builder.and(shifted, mask);
    match element {
        MirType::Int(_) => {
            // value = signextend(bytes - 1, value)
            let index = builder.imm(bytes - 1);
            builder.signextend(index, value)
        }
        MirType::FixedBytes(_) => {
            // value = value << (32 - bytes) * 8
            let shift = builder.imm((32 - bytes) * 8);
            builder.shl(shift, value)
        }
        _ => value,
    }
}
