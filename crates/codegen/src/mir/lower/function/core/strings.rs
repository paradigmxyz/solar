//! String lowering for compiler-owned core operations.
//!
//! Replacement scans once and copies unmatched runs in bulk. String search
//! streams non-overlapping match offsets into one exact array. Splitting asks
//! that search for one spare word, appends the subject end, then replaces each
//! offset in place with a bulk-copied string. Short needles use one masked word
//! comparison; long needles use that comparison as a prefix filter before
//! hashing. Growing replacements validate a checked upper bound. Streamed
//! outputs reserve their exact objects only after the scan, so conservative
//! bounds do not inflate later memory costs. The checked Solidity bodies
//! remain the reference under `-Zno-core-intrinsics`.

use super::*;

impl FunctionLowerer<'_, '_> {
    /// Keep replacement in one shared body instead of cloning it into callers.
    pub(super) fn lower_core_string_replace_call(
        &mut self,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        let [subject, needle, replacement] = *operands else { return None };
        let helper =
            self.lazy_helper(Symbol::intern("core_string_replace"), |this, function| {
                function.attributes.no_inline = true;
                let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
                let ty = MirType::MemoryObject(MemoryObjectKind::Bytes);
                let subject = lowerer.builder.add_param(ty);
                let needle = lowerer.builder.add_param(ty);
                let replacement = lowerer.builder.add_param(ty);
                lowerer.builder.set_return_type(ty);
                lowerer.lower_core_string_replace(subject, needle, replacement);
                Some(())
            })?;
        Some(self.builder.icall(
            helper,
            vec![subject, needle, replacement],
            MirType::MemoryObject(MemoryObjectKind::Bytes),
        ))
    }

    /// Lower both minimal-hex spellings directly into the caller, avoiding an
    /// internal-call round trip on every conversion.
    pub(super) fn lower_core_string_minimal_hex_call(
        &mut self,
        operands: &[ValueId],
        prefixed: bool,
    ) -> Option<ValueId> {
        let [value] = *operands else { return None };
        let prefix_length = self.builder.imm(if prefixed { 2 } else { 0 });
        Some(self.lower_core_string_minimal_hex(value, prefix_length))
    }

    fn lower_core_string_minimal_hex(&mut self, value: ValueId, prefix_length: ValueId) -> ValueId {
        // Reserve one fixed region and fill it backwards two digits at a time.
        // The returned bytes header may start inside the region; the allocation
        // still owns every possible header, payload, and trailing padding word.
        let allocation_size = self.builder.imm(160);
        let allocation = self.builder.alloc_raw(allocation_size, AllocationSemantics::INTERNAL);
        let end_offset = self.builder.imm(128);
        let end = self.builder.add(allocation, end_offset);
        let zero = self.builder.imm(0);
        self.builder.mstore(end, zero);

        // With the table in the low scratch region, `mload(nibble)` has the
        // selected ASCII digit as its low byte.
        let table_address = self.builder.imm(15);
        let table = self.builder.imm(U256::from_be_slice(b"0123456789abcdef"));
        self.builder.mstore(table_address, table);

        let entry = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.jump(header);

        self.builder.switch_to_block(header);
        let x = self.builder.phi(vec![(entry, value)]);
        let output = self.builder.phi(vec![(entry, end)]);
        self.builder.jump(body);

        self.builder.switch_to_block(body);
        let two = self.builder.imm(2);
        let next_output = self.builder.sub(output, two);
        let fifteen = self.builder.imm(15);
        let low_nibble = self.builder.and(x, fifteen);
        let low_digit = self.builder.mload(low_nibble);
        let one = self.builder.imm(1);
        let low_output = self.builder.add(next_output, one);
        self.builder.mstore8(low_output, low_digit);
        let four = self.builder.imm(4);
        let high_nibble = self.builder.shr(four, x);
        let high_nibble = self.builder.and(high_nibble, fifteen);
        let high_digit = self.builder.mload(high_nibble);
        self.builder.mstore8(next_output, high_digit);
        let eight = self.builder.imm(8);
        let next_x = self.builder.shr(eight, x);
        let finished = self.builder.eq_zero(next_x);
        self.builder.branch(finished, done, header);
        self.builder.add_phi_incoming(x, body, next_x);
        self.builder.add_phi_incoming(output, body, next_output);

        self.builder.switch_to_block(done);
        let cursor = self.builder.phi(vec![(body, next_output)]);
        let first_word = self.builder.mload(cursor);
        let first = self.builder.byte(zero, first_word);
        let ascii_zero = self.builder.imm(48);
        let leading_zero = self.builder.eq(first, ascii_zero);
        let leading_zero = self.builder.cast_word(leading_zero);
        let header_size = self.builder.imm(32);
        let result = self.builder.sub(cursor, header_size);
        let common_header = self.builder.add(result, leading_zero);
        let prefix_word = self.builder.imm(0x3078);
        self.builder.mstore(common_header, prefix_word);
        let result = self.builder.sub(common_header, prefix_length);
        let pair_digits = self.builder.sub(end, cursor);
        let digits = self.builder.sub(pair_digits, leading_zero);
        let length = self.builder.add(digits, prefix_length);
        self.builder.mstore(result, length);
        self.builder.memory_object_from_ptr(result, MemoryObjectKind::Bytes)
    }

    /// Packs one short string with word operations instead of a byte loop.
    pub(super) fn lower_core_string_pack_one_call(
        &mut self,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        let [input] = *operands else { return None };
        let bytes = MemoryObjectKind::Bytes;
        let length = self.builder.memory_object_len(input, bytes);
        let data = self.builder.memory_object_data(input, bytes);
        let word = self.builder.mload(data);
        let thirty_two = self.builder.imm(32);
        let nonempty = self.builder.ne_zero(length);
        let fits = self.builder.lt(length, thirty_two);
        let valid = self.builder.and(nonempty, fits);

        // Move the input down by one byte and clear everything after the
        // logical payload, since bytes padding is not part of the value.
        let eight = self.builder.imm(8);
        let payload = self.builder.shr(eight, word);
        let thirty_one = self.builder.imm(31);
        let padding = self.builder.sub(thirty_one, length);
        let three = self.builder.imm(3);
        let padding_bits = self.builder.shl(three, padding);
        let all = self.builder.imm(U256::MAX);
        let mask = self.builder.shl(padding_bits, all);
        let payload = self.builder.and(payload, mask);
        let top_byte = self.builder.imm(248);
        let length_tag = self.builder.shl(top_byte, length);
        let packed = self.builder.or(length_tag, payload);
        let zero = self.builder.imm(0);
        Some(self.builder.select(valid, packed, zero))
    }

    /// Reconstructs one short string with one allocation and one word store.
    pub(super) fn lower_core_string_unpack_one_call(
        &mut self,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        let [packed] = *operands else { return None };
        let zero = self.builder.imm(0);
        let raw_length = self.builder.byte(zero, packed);
        let thirty_one = self.builder.imm(31);
        let too_long = self.builder.gt(raw_length, thirty_one);
        let length = self.builder.select(too_long, thirty_one, raw_length);
        // Every result fits in one payload word, so its header and payload
        // occupy exactly two words. Avoid rebuilding and checking the generic
        // `align32(length + 32)` allocation expression.
        let size = self.builder.imm(64);
        let out = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(out, length, MemoryObjectKind::Bytes);
        let data = self.builder.memory_object_data(out, MemoryObjectKind::Bytes);
        let eight = self.builder.imm(8);
        let contents = self.builder.shl(eight, packed);
        self.builder.mstore(data, contents);
        Some(out)
    }

    /// Packs two short strings with four word operations and a final mask.
    pub(super) fn lower_core_string_pack_two_call(
        &mut self,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        let [a, b] = *operands else { return None };
        let bytes = MemoryObjectKind::Bytes;
        let a_length = self.builder.memory_object_len(a, bytes);
        let b_length = self.builder.memory_object_len(b, bytes);
        let total = self.builder.add(a_length, b_length);
        let nonempty = self.builder.ne_zero(total);
        let thirty_one = self.builder.imm(31);
        let fits = self.builder.lt(total, thirty_one);
        let valid = self.builder.and(nonempty, fits);
        let eight = self.builder.imm(8);
        let three = self.builder.imm(3);
        let all = self.builder.imm(U256::MAX);

        let a_data = self.builder.memory_object_data(a, bytes);
        let a_word = self.builder.mload(a_data);
        let thirty_two = self.builder.imm(32);
        let a_padding = self.builder.sub(thirty_two, a_length);
        let a_padding_bits = self.builder.shl(three, a_padding);
        let a_mask = self.builder.shl(a_padding_bits, all);
        let a_payload = self.builder.and(a_word, a_mask);
        let a_payload = self.builder.shr(eight, a_payload);

        let b_data = self.builder.memory_object_data(b, bytes);
        let b_word = self.builder.mload(b_data);
        let b_padding = self.builder.sub(thirty_two, b_length);
        let b_padding_bits = self.builder.shl(three, b_padding);
        let b_mask = self.builder.shl(b_padding_bits, all);
        let b_payload = self.builder.and(b_word, b_mask);
        let two = self.builder.imm(2);
        let b_byte_offset = self.builder.add(a_length, two);
        let b_bit_offset = self.builder.shl(three, b_byte_offset);
        let b_payload = self.builder.shr(b_bit_offset, b_payload);

        let top_byte = self.builder.imm(248);
        let a_tag = self.builder.shl(top_byte, a_length);
        let thirty = self.builder.imm(30);
        let b_tag_bytes = self.builder.sub(thirty, a_length);
        let b_tag_bits = self.builder.shl(three, b_tag_bytes);
        let b_tag = self.builder.shl(b_tag_bits, b_length);
        let packed = self.builder.or(a_tag, a_payload);
        let packed = self.builder.or(packed, b_tag);
        let packed = self.builder.or(packed, b_payload);

        // Discard dirty source padding even when one input is empty.
        let padding = self.builder.sub(thirty, total);
        let padding_bits = self.builder.shl(three, padding);
        let result_mask = self.builder.shl(padding_bits, all);
        let packed = self.builder.and(packed, result_mask);
        let zero = self.builder.imm(0);
        Some(self.builder.select(valid, packed, zero))
    }

    /// Reconstructs two short strings with two allocations and word stores.
    pub(super) fn lower_core_string_unpack_two_call(
        &mut self,
        operands: &[ValueId],
    ) -> Option<Vec<ValueId>> {
        let [packed] = *operands else { return None };
        let zero = self.builder.imm(0);
        let thirty = self.builder.imm(30);
        let raw_a_length = self.builder.byte(zero, packed);
        let a_too_long = self.builder.gt(raw_a_length, thirty);
        let a_length = self.builder.select(a_too_long, thirty, raw_a_length);
        // Both bounded strings fit in one payload word. Reserve the two
        // header/payload pairs with one bump and derive the second object from
        // the upper half of that allocation.
        let total_size = self.builder.imm(128);
        let allocation = self.builder.alloc_raw(total_size, AllocationSemantics::INTERNAL);
        let a = self.builder.memory_object_from_ptr(allocation, MemoryObjectKind::Bytes);
        // An unaligned store lays the tag into the low byte of the length word
        // and the first payload immediately after it. Restore the clamped
        // length over that tag so malformed words retain the portable body's
        // bounded result.
        let tag_slot = self.builder.add_u64_offset(allocation, 31);
        self.builder.mstore(tag_slot, packed);
        self.builder.set_memory_object_len(a, a_length, MemoryObjectKind::Bytes);

        let a_data = self.builder.memory_object_data(a, MemoryObjectKind::Bytes);
        let packed_b_address = self.builder.add(a_data, a_length);
        let packed_b = self.builder.mload(packed_b_address);
        let raw_b_length = self.builder.byte(zero, packed_b);
        let remaining = self.builder.sub(thirty, a_length);
        let b_too_long = self.builder.gt(raw_b_length, remaining);
        let b_length = self.builder.select(b_too_long, remaining, raw_b_length);
        let object_size = self.builder.imm(64);
        let b_ptr = self.builder.add(allocation, object_size);
        let b = self.builder.memory_object_from_ptr(b_ptr, MemoryObjectKind::Bytes);
        let b_tag_slot = self.builder.add_u64_offset(b_ptr, 31);
        self.builder.mstore(b_tag_slot, packed_b);
        self.builder.set_memory_object_len(b, b_length, MemoryObjectKind::Bytes);
        Some(vec![a, b])
    }

    /// Keep the non-overlapping search and exact result allocation shared.
    pub(super) fn lower_core_string_indices_of_call(
        &mut self,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        let [subject, needle] = *operands else { return None };
        let helper = self.ensure_core_string_indices_helper()?;
        let split_mode = self.builder.imm(0);
        Some(self.builder.icall(
            helper,
            vec![subject, needle, split_mode],
            MirType::MemoryObject(MemoryObjectKind::DynamicArray),
        ))
    }

    fn ensure_core_string_indices_helper(&mut self) -> Option<FunctionId> {
        self.lazy_helper(Symbol::intern("core_string_search"), |this, function| {
            let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
            let bytes = MirType::MemoryObject(MemoryObjectKind::Bytes);
            let subject = lowerer.builder.add_param(bytes);
            let needle = lowerer.builder.add_param(bytes);
            let split_mode = lowerer.builder.add_param(MirType::I256);
            let result = MirType::MemoryObject(MemoryObjectKind::DynamicArray);
            lowerer.builder.set_return_type(result);
            lowerer.lower_core_string_indices_of(subject, needle, split_mode);
            Some(())
        })
    }

    /// Split through the shared offset scanner and reuse its result allocation.
    pub(super) fn lower_core_string_split_call(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let [subject, delimiter] = *operands else { return None };
        let helper = self.ensure_core_string_indices_helper()?;
        let split_mode = self.builder.imm(1);
        Some(self.builder.icall(
            helper,
            vec![subject, delimiter, split_mode],
            MirType::MemoryObject(MemoryObjectKind::DynamicArray),
        ))
    }

    fn alloc_core_word_array(&mut self, length: ValueId, extra_capacity: ValueId) -> ValueId {
        let one = self.builder.imm(1);
        let words = self.builder.checked_add(length, one);
        let words = self.builder.checked_add(words, extra_capacity);
        let word_size = self.builder.imm(32);
        let size = self.builder.checked_mul(words, word_size);
        let out = self.builder.alloc_object(
            size,
            MemoryObjectLayout::WORD_ARRAY,
            AllocationSemantics::SOLIDITY_UNINITIALIZED,
        );
        self.builder.set_memory_object_len(out, length, MemoryObjectKind::DynamicArray);
        out
    }

    /// Allocate bytes after the caller proves that padding cannot overflow.
    fn alloc_core_proven_bytes(&mut self, length: ValueId) -> ValueId {
        let padding = self.builder.imm(63);
        let mask = self.builder.imm(U256::MAX << 5);
        let size = self.builder.add(length, padding);
        let size = self.builder.and(size, mask);
        let out = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        self.builder.set_memory_object_len(out, length, MemoryObjectKind::Bytes);
        out
    }

    fn lower_core_string_indices_of(
        &mut self,
        subject: ValueId,
        needle: ValueId,
        split_mode: ValueId,
    ) {
        let bytes = MemoryObjectKind::Bytes;
        let length = self.builder.memory_object_len(subject, bytes);
        let needle_length = self.builder.memory_object_len(needle, bytes);
        let empty_result = self.builder.create_block();
        let fits = self.builder.create_block();
        let complete = self.builder.create_block();
        let too_long = self.builder.gt(needle_length, length);
        self.builder.branch(too_long, empty_result, fits);

        self.builder.switch_to_block(empty_result);
        let zero = self.builder.imm(0);
        let empty_out = self.alloc_core_word_array(zero, split_mode);
        self.builder.jump(complete);

        self.builder.switch_to_block(fits);
        let empty_needle = self.builder.create_block();
        let every_index = self.builder.create_block();
        let split_empty = self.builder.create_block();
        let nonempty = self.builder.create_block();
        let needle_empty = self.builder.eq_zero(needle_length);
        self.builder.branch(needle_empty, empty_needle, nonempty);

        self.builder.switch_to_block(empty_needle);
        let make_indices = self.builder.eq_zero(split_mode);
        self.builder.branch(make_indices, every_index, split_empty);

        self.builder.switch_to_block(split_empty);
        self.lower_core_string_split_empty(subject, length);

        self.builder.switch_to_block(every_index);
        let one = self.builder.imm(1);
        let count = self.builder.checked_add(length, one);
        let every_out = self.alloc_core_word_array(count, split_mode);
        self.builder.counted_loop(count, |builder, index| {
            let five = builder.imm(5);
            let offset = builder.shl(five, index);
            builder.memory_object_store_word(every_out, offset, index);
        });
        let every_complete = self.builder.current_block();
        self.builder.jump(complete);

        self.builder.switch_to_block(nonempty);
        let allocation_base = self.builder.fmp();
        let header_size = self.builder.imm(32);
        let destination = self.builder.add(allocation_base, header_size);
        let source = self.builder.memory_object_data(subject, bytes);
        let needle_data = self.builder.memory_object_data(needle, bytes);
        let needle_word = self.builder.mload(needle_data);
        let low_five = self.builder.imm(31);
        let remainder = self.builder.and(needle_length, low_five);
        let word = self.builder.imm(32);
        let missing = self.builder.sub(word, remainder);
        let eight = self.builder.imm(8);
        let masked_bits = self.builder.mul(missing, eight);
        let all = self.builder.imm(U256::MAX);
        let prefix_mask = self.builder.shl(masked_bits, all);
        let long_hash = self.builder.create_block();
        let short_hash = self.builder.create_block();
        let scan_entry = self.builder.create_block();
        let short = self.builder.lt(needle_length, word);
        let long = self.builder.eq_zero(short);
        self.builder.branch(long, long_hash, short_hash);

        self.builder.switch_to_block(long_hash);
        let hash = self.builder.keccak256(needle_data, needle_length);
        self.builder.jump(scan_entry);

        self.builder.switch_to_block(short_hash);
        let zero_hash = self.builder.imm(0);
        self.builder.jump(scan_entry);

        self.builder.switch_to_block(scan_entry);
        let needle_hash = self.builder.phi(vec![(long_hash, hash), (short_hash, zero_hash)]);
        let search_end = self.builder.sub(length, needle_length);
        let search_end = self.builder.add(source, search_end);
        let header = self.builder.create_block();
        let compare_prefix = self.builder.create_block();
        let verify = self.builder.create_block();
        let verify_hash = self.builder.create_block();
        let matched = self.builder.create_block();
        let advance = self.builder.create_block();
        let finish = self.builder.create_block();
        self.builder.jump(header);

        self.builder.switch_to_block(header);
        let cursor = self.builder.phi(vec![(scan_entry, source)]);
        let output = self.builder.phi(vec![(scan_entry, destination)]);
        let past_end = self.builder.gt(cursor, search_end);
        self.builder.branch(past_end, finish, compare_prefix);

        self.builder.switch_to_block(compare_prefix);
        let candidate_word = self.builder.mload(cursor);
        let different = self.builder.xor(candidate_word, needle_word);
        let different = self.builder.and(different, prefix_mask);
        let prefix_equal = self.builder.eq_zero(different);
        self.builder.branch(prefix_equal, verify, advance);

        self.builder.switch_to_block(verify);
        self.builder.branch(long, verify_hash, matched);

        self.builder.switch_to_block(verify_hash);
        let candidate_hash = self.builder.keccak256(cursor, needle_length);
        let equal = self.builder.eq(candidate_hash, needle_hash);
        self.builder.branch(equal, matched, advance);

        self.builder.switch_to_block(matched);
        let at = self.builder.sub(cursor, source);
        self.builder.mstore(output, at);
        let next_output = self.builder.add(output, word);
        let next_cursor = self.builder.add(cursor, needle_length);
        self.builder.jump(header);
        self.builder.add_phi_incoming(cursor, matched, next_cursor);
        self.builder.add_phi_incoming(output, matched, next_output);

        self.builder.switch_to_block(advance);
        let next_cursor = self.builder.add(cursor, one);
        self.builder.jump(header);
        self.builder.add_phi_incoming(cursor, advance, next_cursor);
        self.builder.add_phi_incoming(output, advance, output);

        self.builder.switch_to_block(finish);
        let output_bytes = self.builder.sub(output, destination);
        let five = self.builder.imm(5);
        let count = self.builder.shr(five, output_bytes);
        let words = self.builder.checked_add(count, one);
        let words = self.builder.checked_add(words, split_mode);
        let allocation_size = self.builder.checked_mul(words, word);
        let out = self.builder.alloc_object(
            allocation_size,
            MemoryObjectLayout::WORD_ARRAY,
            AllocationSemantics::INTERNAL,
        );
        let Value::Inst(allocation) = *self.builder.func().value(out) else {
            unreachable!("allocation result must reference its instruction")
        };
        self.builder.func_mut().inst_mut(allocation).metadata.set_preserves_fmp(true);
        self.builder.set_memory_object_len(out, count, MemoryObjectKind::DynamicArray);
        self.builder.jump(complete);

        self.builder.switch_to_block(complete);
        let out = self.builder.phi(vec![
            (empty_result, empty_out),
            (every_complete, every_out),
            (finish, out),
        ]);
        self.lower_core_string_search_result(out, subject, length, needle_length, split_mode);
    }

    fn lower_core_string_split_empty(&mut self, subject: ValueId, length: ValueId) {
        let bytes = MemoryObjectKind::Bytes;
        let (out, _) = self
            .builder
            .alloc_dynamic_word_array(length, AllocationSemantics::SOLIDITY_UNINITIALIZED);
        let source = self.builder.memory_object_data(subject, bytes);
        self.builder.counted_loop(length, |builder, index| {
            let one = builder.imm(1);
            let size = builder.imm(64);
            let piece = builder.alloc_object(
                size,
                MemoryObjectLayout::Bytes,
                AllocationSemantics::INTERNAL,
            );
            builder.set_memory_object_len(piece, one, bytes);
            let source_address = builder.add(source, index);
            let word = builder.mload(source_address);
            let zero = builder.imm(0);
            builder.memory_object_store_word(piece, zero, word);
            let piece = builder.cast(piece, MirType::I256);
            let five = builder.imm(5);
            let offset = builder.shl(five, index);
            builder.memory_object_store_word(out, offset, piece);
        });
        self.builder.ret([out]);
    }

    fn lower_core_string_search_result(
        &mut self,
        offsets: ValueId,
        subject: ValueId,
        length: ValueId,
        delimiter_length: ValueId,
        split_mode: ValueId,
    ) {
        let bytes = MemoryObjectKind::Bytes;
        let array = MemoryObjectKind::DynamicArray;
        let return_indices = self.builder.create_block();
        let split = self.builder.create_block();
        let indices_only = self.builder.eq_zero(split_mode);
        self.builder.branch(indices_only, return_indices, split);

        self.builder.switch_to_block(return_indices);
        self.builder.ret([offsets]);

        self.builder.switch_to_block(split);
        let one = self.builder.imm(1);
        let count = self.builder.memory_object_len(offsets, array);
        // The search allocation already reserved `count + 2` words for split.
        let result_count = self.builder.add(count, one);
        self.builder.set_memory_object_len(offsets, result_count, array);
        let offsets_data = self.builder.memory_object_data(offsets, array);
        let five = self.builder.imm(5);
        let final_offset = self.builder.shl(five, count);
        let final_slot = self.builder.add(offsets_data, final_offset);
        self.builder.mstore(final_slot, length);
        let result_bytes = self.builder.shl(five, result_count);
        let offsets_end = self.builder.add(offsets_data, result_bytes);
        let source = self.builder.memory_object_data(subject, bytes);
        let entry = self.builder.current_block();
        let header = self.builder.create_block();
        let create_piece = self.builder.create_block();
        let empty_piece = self.builder.create_block();
        let copied_piece = self.builder.create_block();
        let store_piece = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.jump(header);

        self.builder.switch_to_block(header);
        let zero = self.builder.imm(0);
        let slot = self.builder.phi(vec![(entry, offsets_data)]);
        let previous = self.builder.phi(vec![(entry, zero)]);
        let finished = self.builder.eq(slot, offsets_end);
        self.builder.branch(finished, done, create_piece);

        self.builder.switch_to_block(create_piece);
        let end = self.builder.mload(slot);
        let piece_length = self.builder.sub(end, previous);
        let piece_empty = self.builder.eq_zero(piece_length);
        self.builder.branch(piece_empty, empty_piece, copied_piece);

        self.builder.switch_to_block(empty_piece);
        let zero_slot = self.builder.imm(EvmMemoryLayout::ZERO_SLOT);
        let empty_value = self.builder.cast(zero_slot, MirType::MemoryObject(bytes));
        self.builder.jump(store_piece);

        self.builder.switch_to_block(copied_piece);
        let piece = self.alloc_core_proven_bytes(piece_length);
        let destination = self.builder.memory_object_data(piece, bytes);
        let source_address = self.builder.add(source, previous);
        self.builder.mcopy_heap(destination, source_address, piece_length);
        self.builder.jump(store_piece);

        self.builder.switch_to_block(store_piece);
        let piece = self.builder.phi(vec![(empty_piece, empty_value), (copied_piece, piece)]);
        let piece = self.builder.cast(piece, MirType::I256);
        self.builder.mstore(slot, piece);
        let next_previous = self.builder.add(end, delimiter_length);
        let word = self.builder.imm(32);
        let next_slot = self.builder.add(slot, word);
        self.builder.jump(header);
        self.builder.add_phi_incoming(slot, store_piece, next_slot);
        self.builder.add_phi_incoming(previous, store_piece, next_previous);

        self.builder.switch_to_block(done);
        self.builder.ret([offsets]);
    }

    fn lower_core_string_replace(
        &mut self,
        subject: ValueId,
        needle: ValueId,
        replacement: ValueId,
    ) {
        let kind = MemoryObjectKind::Bytes;
        let length = self.builder.memory_object_len(subject, kind);
        let needle_length = self.builder.memory_object_len(needle, kind);
        let replacement_length = self.builder.memory_object_len(replacement, kind);
        let return_subject = self.builder.create_block();
        let fits = self.builder.create_block();
        let too_long = self.builder.gt(needle_length, length);
        self.builder.branch(too_long, return_subject, fits);

        self.builder.switch_to_block(return_subject);
        self.builder.ret([subject]);

        self.builder.switch_to_block(fits);
        let empty = self.builder.create_block();
        let nonempty = self.builder.create_block();
        let needle_empty = self.builder.eq_zero(needle_length);
        self.builder.branch(needle_empty, empty, nonempty);

        self.builder.switch_to_block(empty);
        self.lower_core_string_replace_empty(subject, replacement, length, replacement_length);

        self.builder.switch_to_block(nonempty);
        self.lower_core_string_replace_nonempty(
            subject,
            needle,
            replacement,
            length,
            needle_length,
            replacement_length,
        );
    }

    fn lower_core_string_replace_empty(
        &mut self,
        subject: ValueId,
        replacement: ValueId,
        length: ValueId,
        replacement_length: ValueId,
    ) {
        let kind = MemoryObjectKind::Bytes;
        let one = self.builder.imm(1);
        let count = self.builder.checked_add(length, one);
        let inserted = self.builder.checked_mul(count, replacement_length);
        let capacity = self.builder.checked_add(length, inserted);
        let out =
            self.builder.alloc_bytes_object(capacity, AllocationSemantics::SOLIDITY_UNINITIALIZED);
        let source = self.builder.memory_object_data(subject, kind);
        let replacement = self.builder.memory_object_data(replacement, kind);
        let destination = self.builder.memory_object_data(out, kind);
        let entry = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let copy_byte = self.builder.create_block();
        let next_without_byte = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.jump(header);

        self.builder.switch_to_block(header);
        let zero = self.builder.imm(0);
        let index = self.builder.phi(vec![(entry, zero)]);
        let output = self.builder.phi(vec![(entry, zero)]);
        let past_end = self.builder.gt(index, length);
        self.builder.branch(past_end, done, body);

        self.builder.switch_to_block(body);
        let output_address = self.builder.add(destination, output);
        self.builder.mcopy_heap(output_address, replacement, replacement_length);
        let after_replacement = self.builder.add(output, replacement_length);
        let has_byte = self.builder.lt(index, length);
        self.builder.branch(has_byte, copy_byte, next_without_byte);

        self.builder.switch_to_block(copy_byte);
        let source_address = self.builder.add(source, index);
        let word = self.builder.mload(source_address);
        let zero = self.builder.imm(0);
        let byte = self.builder.byte(zero, word);
        let output_address = self.builder.add(destination, after_replacement);
        self.builder.mstore8(output_address, byte);
        let next_index = self.builder.add(index, one);
        let next_output = self.builder.add(after_replacement, one);
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, copy_byte, next_index);
        self.builder.add_phi_incoming(output, copy_byte, next_output);

        self.builder.switch_to_block(next_without_byte);
        let next_index = self.builder.add(index, one);
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, next_without_byte, next_index);
        self.builder.add_phi_incoming(output, next_without_byte, after_replacement);

        self.builder.switch_to_block(done);
        self.builder.ret([out]);
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_core_string_replace_nonempty(
        &mut self,
        subject: ValueId,
        needle: ValueId,
        replacement: ValueId,
        length: ValueId,
        needle_length: ValueId,
        replacement_length: ValueId,
    ) {
        let kind = MemoryObjectKind::Bytes;
        let same_capacity = self.builder.create_block();
        let growing_capacity = self.builder.create_block();
        let allocate = self.builder.create_block();
        let grows = self.builder.gt(replacement_length, needle_length);
        self.builder.branch(grows, growing_capacity, same_capacity);

        self.builder.switch_to_block(same_capacity);
        self.builder.jump(allocate);

        self.builder.switch_to_block(growing_capacity);
        let max_matches = self.builder.div(length, needle_length);
        let growth_per_match = self.builder.sub(replacement_length, needle_length);
        let growth = self.builder.checked_mul(max_matches, growth_per_match);
        let _grown_capacity = self.builder.checked_add(length, growth);
        self.builder.jump(allocate);

        self.builder.switch_to_block(allocate);
        let allocation_base = self.builder.fmp();
        let source = self.builder.memory_object_data(subject, kind);
        let needle_data = self.builder.memory_object_data(needle, kind);
        let replacement_data = self.builder.memory_object_data(replacement, kind);
        let header = self.builder.imm(32);
        let destination = self.builder.add(allocation_base, header);
        let needle_word = self.builder.mload(needle_data);
        let low_five = self.builder.imm(31);
        let remainder = self.builder.and(needle_length, low_five);
        let word = self.builder.imm(32);
        let missing = self.builder.sub(word, remainder);
        let eight = self.builder.imm(8);
        let masked_bits = self.builder.mul(missing, eight);
        let all = self.builder.imm(U256::MAX);
        let prefix_mask = self.builder.shl(masked_bits, all);
        let long_hash = self.builder.create_block();
        let short_hash = self.builder.create_block();
        let scan_entry = self.builder.create_block();
        let short = self.builder.lt(needle_length, word);
        let long = self.builder.eq_zero(short);
        self.builder.branch(long, long_hash, short_hash);

        self.builder.switch_to_block(long_hash);
        let hash = self.builder.keccak256(needle_data, needle_length);
        self.builder.jump(scan_entry);

        self.builder.switch_to_block(short_hash);
        let zero_hash = self.builder.imm(0);
        self.builder.jump(scan_entry);

        self.builder.switch_to_block(scan_entry);
        let needle_hash = self.builder.phi(vec![(long_hash, hash), (short_hash, zero_hash)]);
        let search_end = self.builder.sub(length, needle_length);
        let zero = self.builder.imm(0);
        let header = self.builder.create_block();
        let compare_prefix = self.builder.create_block();
        let verify = self.builder.create_block();
        let verify_hash = self.builder.create_block();
        let matched = self.builder.create_block();
        let advance = self.builder.create_block();
        let finish = self.builder.create_block();
        self.builder.jump(header);

        self.builder.switch_to_block(header);
        let at = self.builder.phi(vec![(scan_entry, zero)]);
        let copied = self.builder.phi(vec![(scan_entry, zero)]);
        let output = self.builder.phi(vec![(scan_entry, zero)]);
        let past_end = self.builder.gt(at, search_end);
        self.builder.branch(past_end, finish, compare_prefix);

        self.builder.switch_to_block(compare_prefix);
        let candidate = self.builder.add(source, at);
        let candidate_word = self.builder.mload(candidate);
        let different = self.builder.xor(candidate_word, needle_word);
        let different = self.builder.and(different, prefix_mask);
        let prefix_equal = self.builder.eq_zero(different);
        self.builder.branch(prefix_equal, verify, advance);

        self.builder.switch_to_block(verify);
        self.builder.branch(long, verify_hash, matched);

        self.builder.switch_to_block(verify_hash);
        let candidate_hash = self.builder.keccak256(candidate, needle_length);
        let equal = self.builder.eq(candidate_hash, needle_hash);
        self.builder.branch(equal, matched, advance);

        self.builder.switch_to_block(matched);
        let run = self.builder.sub(at, copied);
        let output_address = self.builder.add(destination, output);
        let copied_address = self.builder.add(source, copied);
        self.builder.mcopy_heap(output_address, copied_address, run);
        let after_run = self.builder.add(output, run);
        let replacement_address = self.builder.add(destination, after_run);
        self.builder.mcopy_heap(replacement_address, replacement_data, replacement_length);
        let next_output = self.builder.add(after_run, replacement_length);
        let next_at = self.builder.add(at, needle_length);
        self.builder.jump(header);
        self.builder.add_phi_incoming(at, matched, next_at);
        self.builder.add_phi_incoming(copied, matched, next_at);
        self.builder.add_phi_incoming(output, matched, next_output);

        self.builder.switch_to_block(advance);
        let one = self.builder.imm(1);
        let next_at = self.builder.add(at, one);
        self.builder.jump(header);
        self.builder.add_phi_incoming(at, advance, next_at);
        self.builder.add_phi_incoming(copied, advance, copied);
        self.builder.add_phi_incoming(output, advance, output);

        self.builder.switch_to_block(finish);
        let tail = self.builder.sub(length, copied);
        let output_address = self.builder.add(destination, output);
        let copied_address = self.builder.add(source, copied);
        self.builder.mcopy_heap(output_address, copied_address, tail);
        let output_length = self.builder.add(output, tail);
        let allocation_size = self.builder.checked_padded_size(output_length);
        let out = self.builder.alloc_object(
            allocation_size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        let Value::Inst(allocation) = *self.builder.func().value(out) else {
            unreachable!("allocation result must reference its instruction")
        };
        self.builder.func_mut().inst_mut(allocation).metadata.set_preserves_fmp(true);
        self.builder.set_memory_object_len(out, output_length, kind);
        self.builder.ret([out]);
    }
}
