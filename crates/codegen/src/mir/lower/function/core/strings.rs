//! String lowering for compiler-owned core operations.
//!
//! Replacement scans once and copies unmatched runs in bulk. Short needles
//! use one masked word comparison; long needles use that comparison as a
//! prefix filter before hashing. Growing replacements validate a checked upper
//! bound. Output is streamed at the free-memory pointer and the exact object is
//! reserved after the scan, so a conservative bound does not inflate later
//! memory costs. The checked Solidity body remains the reference under
//! `-Zno-core-intrinsics`.

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

    /// Keep the non-overlapping search and exact result allocation shared.
    pub(super) fn lower_core_string_indices_of_call(
        &mut self,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        let [subject, needle] = *operands else { return None };
        let helper =
            self.lazy_helper(Symbol::intern("core_string_indices_of"), |this, function| {
                function.attributes.no_inline = true;
                let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
                let bytes = MirType::MemoryObject(MemoryObjectKind::Bytes);
                let subject = lowerer.builder.add_param(bytes);
                let needle = lowerer.builder.add_param(bytes);
                let result = MirType::MemoryObject(MemoryObjectKind::DynamicArray);
                lowerer.builder.set_return_type(result);
                lowerer.lower_core_string_indices_of(subject, needle);
                Some(())
            })?;
        Some(self.builder.icall(
            helper,
            vec![subject, needle],
            MirType::MemoryObject(MemoryObjectKind::DynamicArray),
        ))
    }

    fn lower_core_string_indices_of(&mut self, subject: ValueId, needle: ValueId) {
        let bytes = MemoryObjectKind::Bytes;
        let length = self.builder.memory_object_len(subject, bytes);
        let needle_length = self.builder.memory_object_len(needle, bytes);
        let empty_result = self.builder.create_block();
        let fits = self.builder.create_block();
        let too_long = self.builder.gt(needle_length, length);
        self.builder.branch(too_long, empty_result, fits);

        self.builder.switch_to_block(empty_result);
        let zero = self.builder.imm(0);
        let (out, _) = self
            .builder
            .alloc_dynamic_word_array(zero, AllocationSemantics::SOLIDITY_UNINITIALIZED);
        self.builder.ret([out]);

        self.builder.switch_to_block(fits);
        let every_index = self.builder.create_block();
        let nonempty = self.builder.create_block();
        let needle_empty = self.builder.eq_zero(needle_length);
        self.builder.branch(needle_empty, every_index, nonempty);

        self.builder.switch_to_block(every_index);
        let one = self.builder.imm(1);
        let count = self.builder.checked_add(length, one);
        let (out, _) = self
            .builder
            .alloc_dynamic_word_array(count, AllocationSemantics::SOLIDITY_UNINITIALIZED);
        self.builder.counted_loop(count, |builder, index| {
            let five = builder.imm(5);
            let offset = builder.shl(five, index);
            builder.memory_object_store_word(out, offset, index);
        });
        self.builder.ret([out]);

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
        let count = self.builder.phi(vec![(scan_entry, zero)]);
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
        let five = self.builder.imm(5);
        let offset = self.builder.shl(five, count);
        let address = self.builder.add(destination, offset);
        self.builder.mstore(address, at);
        let next_count = self.builder.add(count, one);
        let next_at = self.builder.add(at, needle_length);
        self.builder.jump(header);
        self.builder.add_phi_incoming(at, matched, next_at);
        self.builder.add_phi_incoming(count, matched, next_count);

        self.builder.switch_to_block(advance);
        let next_at = self.builder.add(at, one);
        self.builder.jump(header);
        self.builder.add_phi_incoming(at, advance, next_at);
        self.builder.add_phi_incoming(count, advance, count);

        self.builder.switch_to_block(finish);
        let words = self.builder.checked_add(count, one);
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
        self.builder.ret([out]);
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
