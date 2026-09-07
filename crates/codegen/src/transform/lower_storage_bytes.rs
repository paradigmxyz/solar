//! Expand Solidity storage bytes reads during builtin conversion.
//!
//! Validate the short/long header before exposing its length or allocating memory. Short values
//! copy their packed header word; long values copy the hashed data area through a counted loop.
//! Allocation keeps Solidity's checked, rounded, uninitialized policy. The builtin pass supplies
//! source context and repairs the continuation edge, and revert outlining shares failure payloads.

use crate::mir::{AllocationSemantics, FunctionBuilder, PanicCode, ValueId};
use alloy_primitives::U256;

pub(super) fn validate(builder: &mut FunctionBuilder<'_>, header: ValueId) -> (ValueId, ValueId) {
    // flag = header & 1; is_long = (flag == 1)
    // half = header >> 1
    // length = is_long ? half : (half & 0x7f)
    // if invalid_short_long_encoding { panic(StorageEncoding) }
    let one = builder.imm(1);
    let flag = builder.and(header, one);
    let is_long = builder.eq(flag, one);
    let shift = builder.imm(1);
    let half = builder.shr(shift, header);
    let short_mask = builder.imm(0x7f);
    let short_len = builder.and(half, short_mask);
    let length = builder.select(is_long, half, short_len);
    let thirty_two = builder.imm(32);
    let short_length = builder.lt(length, thirty_two);
    let invalid_encoding = builder.eq(is_long, short_length);
    builder.panic_if(invalid_encoding, PanicCode::StorageEncoding);
    (is_long, length)
}

pub(super) fn load(builder: &mut FunctionBuilder<'_>, slot: ValueId) -> ValueId {
    // header = sload(slot)
    // (is_long, length) = validate_storage_bytes(header)
    // words = (length + 31) / 32
    // object = bytes_uninitialized(length)
    // branch is_long, long, short
    let header = builder.sload(slot);
    let (is_long, length) = validate(builder, header);
    let thirty_two = builder.imm(32);
    let thirty_one = builder.imm(31);
    let rounded = builder.add(length, thirty_one);
    let words = builder.div(rounded, thirty_two);
    let object = builder.alloc_bytes_object(length, AllocationSemantics::SOLIDITY_UNINITIALIZED);

    let short_block = builder.create_block();
    let long_block = builder.create_block();
    let merge_block = builder.create_block();
    builder.branch(is_long, long_block, short_block);

    // short: store_word(object, 0, header & ~0xff); jump merge
    builder.switch_to_block(short_block);
    let zero = builder.imm(0);
    let short_mask = builder.imm(U256::MAX << 8);
    let short_data = builder.and(header, short_mask);
    builder.memory_object_store_word(object, zero, short_data);
    builder.jump(merge_block);

    // long: data_slot = storage_array_data_slot(slot)
    // for index < words { store_word(object, index * 32, sload(data_slot + index)) }
    // jump merge
    builder.switch_to_block(long_block);
    let data_slot = builder.storage_array_data_slot(slot);
    builder.counted_loop(words, |builder, index| {
        let element_slot = builder.add(data_slot, index);
        let value = builder.sload(element_slot);
        let byte_offset = builder.mul(index, thirty_two);
        builder.memory_object_store_word(object, byte_offset, value);
    });
    builder.jump(merge_block);

    builder.switch_to_block(merge_block);
    object
}
