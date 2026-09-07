//! Expand Solidity storage bytes reads, writes, and data-word clearing during builtin conversion.
//!
//! Validate the short/long header before exposing its length or allocating memory. Short values
//! copy their packed header word; long values copy the hashed data area through a counted loop.
//! Allocation keeps Solidity's checked, rounded, uninitialized policy. The builtin pass supplies
//! source context and repairs the continuation edge, and revert outlining shares failure payloads.
//! Clearing carries both the word index and storage address through the loop. The index controls
//! termination independently of wrapping storage addresses. Address hashing stays abstract until
//! memory lowering, as it does for ordinary array accesses. Writes share a clear helper, validate
//! the old header, clear truncated words, and mask partial words before storing new data. They
//! leave allocation and memory layout decisions to the later conversion passes.

use crate::mir::{
    AllocationSemantics, Function, FunctionBuilder, FunctionId, MemoryObjectKind, MirType, Module,
    PanicCode, SliceLocation, ValueId,
};
use alloy_primitives::U256;
use solar_interface::{Ident, sym};

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

pub(super) fn clear_words(
    builder: &mut FunctionBuilder<'_>,
    slot: ValueId,
    first_word: ValueId,
    words: ValueId,
) {
    // data_slot = storage_array_data_slot(slot)
    // address = data_slot + first_word
    // for i in first_word..words { sstore(address, 0); address += 1 }
    let zero = builder.imm(0);
    let data_slot = builder.storage_array_data_slot(slot);
    let first_slot = builder.add(data_slot, first_word);
    let preheader = builder.current_block();
    let header = builder.create_block();
    let body = builder.create_block();
    let exit = builder.create_block();
    builder.jump(header);
    builder.switch_to_block(header);
    let index = builder.phi(vec![(preheader, first_word)]);
    let element_slot = builder.phi(vec![(preheader, first_slot)]);
    let condition = builder.lt(index, words);
    builder.branch(condition, body, exit);
    builder.switch_to_block(body);
    builder.sstore(element_slot, zero);
    let next = builder.add_u64_offset(index, 1);
    let next_slot = builder.add_u64_offset(element_slot, 1);
    let backedge = builder.current_block();
    builder.jump(header);
    builder.add_phi_incoming(index, backedge, next);
    builder.add_phi_incoming(element_slot, backedge, next_slot);
    builder.switch_to_block(exit);
}

pub(super) fn add_clear_helper(module: &mut Module) -> FunctionId {
    // fn clear_storage_words(slot, first, end) { clear_storage_words slot, first, end; ret }
    let mut function = Function::new(Ident::with_dummy_span(sym::clear_storage_words));
    let mut builder = FunctionBuilder::new_semantic(&mut function);
    let slot = builder.add_param(MirType::uint256());
    let first = builder.add_param(MirType::uint256());
    let end = builder.add_param(MirType::uint256());
    builder.clear_storage_words(slot, first, end);
    builder.ret([]);
    module.add_function(function)
}

pub(super) fn store(
    builder: &mut FunctionBuilder<'_>,
    slot: ValueId,
    object: ValueId,
    clear_helper: FunctionId,
) {
    // old_is_long, old_length = decode_storage_bytes_header(slot)
    // length, data = bytes(object)
    let header = builder.sload(slot);
    let (old_is_long, old_length) = validate(builder, header);
    let length = builder.memory_object_len(object, MemoryObjectKind::Bytes);
    let data_ptr = builder.memory_object_data(object, MemoryObjectKind::Bytes);
    let data = builder.make_slice(data_ptr, length, SliceLocation::Memory);
    let word_size = builder.imm(32);
    let thirty_one = builder.imm(31);
    let old_rounded = builder.add(old_length, thirty_one);
    let old_words = builder.div(old_rounded, word_size);
    let rounded = builder.checked_add(length, thirty_one);
    let words = builder.div(rounded, word_size);
    let zero = builder.imm(0);
    let short = builder.lt(length, word_size);
    let new_words = builder.select(short, zero, words);
    let shrunk = builder.gt(old_length, length);
    let needs_cleanup = builder.and(old_is_long, shrunk);
    let cleanup_block = builder.create_block();
    let write_block = builder.create_block();
    builder.branch(needs_cleanup, cleanup_block, write_block);

    // if old_is_long && old_length > length {
    //     clear_storage_words(slot, new_words, old_words)
    // }
    builder.switch_to_block(cleanup_block);
    builder.icall_void(clear_helper, vec![slot, new_words, old_words]);
    builder.jump(write_block);

    builder.switch_to_block(write_block);
    let short_block = builder.create_block();
    let long_block = builder.create_block();
    let merge_block = builder.create_block();
    builder.branch(short, short_block, long_block);

    // header = mask(mload(data), length) | length * 2
    // sstore(slot, header)
    builder.switch_to_block(short_block);
    let data_word = builder.memory_slice_load_word(data, zero);
    let unused_bytes = builder.sub(word_size, length);
    let bits = builder.imm(8);
    let shift = builder.mul(unused_bytes, bits);
    let one = builder.imm(1);
    let high_bit = builder.shl(shift, one);
    let low_mask = builder.sub(high_bit, one);
    let data_mask = builder.not(low_mask);
    let data_word = builder.and(data_word, data_mask);
    let two = builder.imm(2);
    let tag = builder.mul(length, two);
    let header = builder.or(data_word, tag);
    builder.sstore(slot, header);
    builder.jump(merge_block);

    // sstore(slot, length << 1 | 1)
    // data_slot = storage_array_data_slot(slot)
    builder.switch_to_block(long_block);
    let one = builder.imm(1);
    let shifted = builder.shl(one, length);
    let tag = builder.or(shifted, one);
    builder.sstore(slot, tag);
    let data_slot = builder.storage_array_data_slot(slot);

    // for i in 0..length / 32 {
    //     sstore(data_slot + i, mload(data + i * 32))
    // }
    let full_words = builder.div(length, word_size);
    builder.counted_loop(full_words, |builder, index| {
        let byte_offset = builder.mul(index, word_size);
        let value = builder.memory_slice_load_word(data, byte_offset);
        let element_slot = builder.add(data_slot, index);
        builder.sstore(element_slot, value);
    });
    // The final memory word can contain dirty padding bytes, so mask it before storage,
    // matching solc's `copy_byte_array_to_storage`.
    let partial_block = builder.create_block();
    let remainder = builder.and(length, thirty_one);
    let has_partial = builder.iszero(remainder);
    builder.branch(has_partial, merge_block, partial_block);

    // if length % 32 != 0 {
    //     sstore(data_slot + full_words, mask(mload(data + full_words * 32), remainder))
    // }
    builder.switch_to_block(partial_block);
    let partial_offset = builder.mul(full_words, word_size);
    let partial_word = builder.memory_slice_load_word(data, partial_offset);
    let unused_bytes = builder.sub(word_size, remainder);
    let bits = builder.imm(8);
    let shift = builder.mul(unused_bytes, bits);
    let high_bit = builder.shl(shift, one);
    let low_mask = builder.sub(high_bit, one);
    let data_mask = builder.not(low_mask);
    let partial_word = builder.and(partial_word, data_mask);
    let partial_slot = builder.add(data_slot, full_words);
    builder.sstore(partial_slot, partial_word);
    builder.jump(merge_block);

    builder.switch_to_block(merge_block);
}
