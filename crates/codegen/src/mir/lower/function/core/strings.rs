//! String lowering for compiler-owned core operations.
//!
//! Replacement scans once and copies unmatched runs in bulk. Its pointer
//! cursors and separate loops for short and long needles keep neither the
//! subject base, the output base nor the needle hash in a short scan's state,
//! which is what spilled on short subjects. String search
//! streams non-overlapping match offsets into one exact array. Splitting asks
//! that search for one spare word, appends the subject end, then replaces each
//! offset in place with a bulk-copied string. Short needles use one masked word
//! comparison; long needles use that comparison as a prefix filter before
//! hashing. Growing replacements validate a checked upper bound. Streamed
//! outputs reserve their exact objects only after the scan, so conservative
//! bounds do not inflate later memory costs. Rune counting classifies whole
//! words of well-formed UTF-8 at once and steps the rest through a scratch
//! table of lead lengths. Decimal and minimal-hex spellings fill one fixed
//! region backwards and return a header inside it, so neither counts digits
//! first; a gas build spells a hex value wider than two bytes a word at a
//! time instead, counting its significant bytes with one multiplication.
//! Each spelling path writes its own header, so only the result joins.
//! Repetition sizes its output like the body, moves the subject once and then
//! doubles the filled prefix from the output itself, without the body's
//! bounds checks or zero fill: every move stays inside the output, and only
//! the padding past the payload keeps a zero word. The checked Solidity
//! bodies remain the reference under `-Zno-core-intrinsics`.

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
        Some(self.lower_core_string_minimal_hex(value, prefix_length, false))
    }

    /// Writes two digits per byte from the lowest byte up, stopping after the
    /// highest nonzero byte. The minimal spelling then drops a leading zero
    /// digit; `whole_bytes` keeps it, giving the fewest whole bytes instead.
    pub(super) fn lower_core_string_minimal_hex(
        &mut self,
        value: ValueId,
        prefix_length: ValueId,
        whole_bytes: bool,
    ) -> ValueId {
        // Reserve one fixed region and fill it backwards two digits at a time.
        // The returned bytes header may start inside the region; the allocation
        // still owns every possible header, payload, and trailing padding word.
        let allocation_size = self.builder.imm(160);
        let allocation = self.builder.alloc_raw(allocation_size, AllocationSemantics::INTERNAL);
        let end_offset = self.builder.imm(128);
        let end = self.builder.add(allocation, end_offset);
        let zero = self.builder.imm(0);
        self.builder.mstore(end, zero);

        if !self.cx.gcx.sess.opts.optimization.is_gas() {
            let cursor = self.lower_core_hex_bytes(value, end);
            let result = self.lower_core_hex_header(cursor, end, prefix_length, whole_bytes);
            return self.builder.memory_object_in_allocation(result, MemoryObjectKind::Bytes);
        }

        // The lowest byte is spelled first, which finishes one-byte values at
        // the loop's cost. A second byte is spelled the same way; anything
        // wider is spelled again a word at a time, whose fixed cost beats
        // two digits per byte from three bytes up. Each path finishes its own
        // header so that only the result joins. This trades code for gas;
        // size builds keep only the loop.
        // x1, out1 = spell(value, end)
        // result = x1 == 0 ? header(out1)
        //     : x1 <= 0xff ? header(spell(x1, out1).1)
        //     : header(spell_words(value))
        let table_address = self.builder.imm(15);
        let table = self.builder.imm(U256::from_be_slice(b"0123456789abcdef"));
        self.builder.mstore(table_address, table);
        let one_byte = self.builder.create_block();
        let wider = self.builder.create_block();
        let two_bytes = self.builder.create_block();
        let words = self.builder.create_block();
        let finish = self.builder.create_block();
        let (x1, out1) = self.lower_core_hex_byte(value, end);
        let finished = self.builder.eq_zero(x1);
        self.builder.branch(finished, one_byte, wider);

        self.builder.switch_to_block(one_byte);
        let one_result = self.lower_core_hex_header(out1, end, prefix_length, whole_bytes);
        let one_exit = self.builder.current_block();
        self.builder.jump(finish);

        self.builder.switch_to_block(wider);
        let byte = self.builder.imm(0xff);
        let wide = self.builder.gt(x1, byte);
        self.builder.branch(wide, words, two_bytes);

        self.builder.switch_to_block(two_bytes);
        let (_, out2) = self.lower_core_hex_byte(x1, out1);
        let two_result = self.lower_core_hex_header(out2, end, prefix_length, whole_bytes);
        let two_exit = self.builder.current_block();
        self.builder.jump(finish);

        self.builder.switch_to_block(words);
        let cursor = self.lower_core_minimal_hex_words(value, end);
        let words_result = self.lower_core_hex_header(cursor, end, prefix_length, whole_bytes);
        let words_exit = self.builder.current_block();
        self.builder.jump(finish);

        self.builder.switch_to_block(finish);
        let result = self.builder.phi(vec![
            (one_exit, one_result),
            (two_exit, two_result),
            (words_exit, words_result),
        ]);
        self.builder.memory_object_in_allocation(result, MemoryObjectKind::Bytes)
    }

    /// Spells the fewest whole bytes of `value` two digits at a time through
    /// the scratch digit table, ending at `end`, and returns where the
    /// digits start.
    fn lower_core_hex_bytes(&mut self, value: ValueId, end: ValueId) -> ValueId {
        // With the table in the low scratch region, `mload(nibble)` has the
        // selected ASCII digit as its low byte.
        let table_address = self.builder.imm(15);
        let table = self.builder.imm(U256::from_be_slice(b"0123456789abcdef"));
        self.builder.mstore(table_address, table);

        // do { x, output = spell(x, output) } while (x != 0)
        let entry = self.builder.current_block();
        let body = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.jump(body);

        self.builder.switch_to_block(body);
        let x = self.builder.phi(vec![(entry, value)]);
        let output = self.builder.phi(vec![(entry, end)]);
        let (next_x, next_output) = self.lower_core_hex_byte(x, output);
        let finished = self.builder.eq_zero(next_x);
        self.builder.branch(finished, done, body);
        self.builder.add_phi_incoming(x, body, next_x);
        self.builder.add_phi_incoming(output, body, next_output);

        self.builder.switch_to_block(done);
        next_output
    }

    /// Writes the header for the digits from `cursor` to `end`: the length
    /// word, dropping a leading '0' digit unless `whole_bytes`, and the `0x`
    /// prefix when `prefix_length` is two. Returns the header's address.
    fn lower_core_hex_header(
        &mut self,
        cursor: ValueId,
        end: ValueId,
        prefix_length: ValueId,
        whole_bytes: bool,
    ) -> ValueId {
        // leading_zero = !whole_bytes && byte(0, mload(cursor)) == '0'
        // mstore(cursor - 32 + leading_zero, "0x")
        // result = cursor - 32 + leading_zero - prefix_length
        // mstore(result, end - cursor - leading_zero + prefix_length)
        let zero = self.builder.imm(0);
        let leading_zero = if whole_bytes {
            zero
        } else {
            let first_word = self.builder.mload(cursor);
            let first = self.builder.byte(zero, first_word);
            let ascii_zero = self.builder.imm(48);
            let leading_zero = self.builder.eq(first, ascii_zero);
            self.builder.cast_word(leading_zero)
        };
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
        result
    }

    /// Spells the low byte of `x` as the two digits before `output` through
    /// the scratch digit table, returning the remaining bytes and the new
    /// output start.
    fn lower_core_hex_byte(&mut self, x: ValueId, output: ValueId) -> (ValueId, ValueId) {
        // next_output = output - 2
        // mstore8(next_output + 1, mload(x & 15))
        // mstore8(next_output, mload((x >> 4) & 15))
        // next_x = x >> 8
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
        (next_x, next_output)
    }

    /// Lowers `Strings.toString` of a `uint256` or `int256` into the caller.
    pub(super) fn lower_core_string_to_string_call(
        &mut self,
        operands: &[ValueId],
        parameter_tys: &[Ty<'_>],
    ) -> Option<ValueId> {
        let ([value], [ty]) = (operands, parameter_tys) else { return None };
        Some(self.lower_core_string_to_string(*value, ty.peel_refs().is_signed()))
    }

    /// Writes the decimal digits backwards from the end of one fixed region,
    /// without counting them first, and a `-` below them when the value is
    /// negative. As in the minimal-hex spelling, the returned bytes header
    /// starts inside the region, which owns every possible header, payload
    /// and padding word: 78 digits and a sign fit below its 128-byte end.
    /// Two digits per step would need as many divisions and keep the last
    /// pair live after the loop, which measured slower.
    fn lower_core_string_to_string(&mut self, value: ValueId, signed: bool) -> ValueId {
        // negative = value <s 0; magnitude = negative ? 0 - value : value
        let zero = self.builder.imm(0);
        let (magnitude, negative) = if signed {
            let negative = self.builder.slt(value, zero);
            let negated = self.builder.sub(zero, value);
            (self.builder.select(negative, negated, value), Some(negative))
        } else {
            (value, None)
        };
        // region = alloc(160); end = region + 128; mstore(end, 0)
        let allocation_size = self.builder.imm(160);
        let allocation = self.builder.alloc_raw(allocation_size, AllocationSemantics::INTERNAL);
        let end_offset = self.builder.imm(128);
        let end = self.builder.add(allocation, end_offset);
        self.builder.mstore(end, zero);

        // do { output -= 1; mstore8(output, 48 + x % 10); x /= 10 } while (x != 0)
        let entry = self.builder.current_block();
        let body = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.jump(body);

        self.builder.switch_to_block(body);
        let x = self.builder.phi(vec![(entry, magnitude)]);
        let output = self.builder.phi(vec![(entry, end)]);
        let one = self.builder.imm(1);
        let next_output = self.builder.sub(output, one);
        let ten = self.builder.imm(10);
        let digit = self.builder.mod_(x, ten);
        let ascii_zero = self.builder.imm(48);
        let character = self.builder.add(digit, ascii_zero);
        self.builder.mstore8(next_output, character);
        let next_x = self.builder.div(x, ten);
        let finished = self.builder.eq_zero(next_x);
        self.builder.branch(finished, done, body);
        self.builder.add_phi_incoming(x, body, next_x);
        self.builder.add_phi_incoming(output, body, next_output);

        // mstore8(output - 1, '-'); start = output - negative
        // A non-negative value's length store below overwrites the sign byte.
        self.builder.switch_to_block(done);
        let start = match negative {
            Some(negative) => {
                let sign_address = self.builder.sub(next_output, one);
                let minus = self.builder.imm(b'-');
                self.builder.mstore8(sign_address, minus);
                let negative = self.builder.cast_word(negative);
                self.builder.sub(next_output, negative)
            }
            None => next_output,
        };
        // result = start - 32; mstore(result, end - start)
        let length = self.builder.sub(end, start);
        let header_size = self.builder.imm(32);
        let result = self.builder.sub(start, header_size);
        self.builder.mstore(result, length);
        self.builder.memory_object_in_allocation(result, MemoryObjectKind::Bytes)
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

    /// Packs two short strings with one load each, whose words already hold
    /// each length byte ahead of its payload, and a final mask.
    pub(super) fn lower_core_string_pack_two_call(
        &mut self,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        let [a, b] = *operands else { return None };
        let bytes = MemoryObjectKind::Bytes;
        let a_length = self.builder.memory_object_len(a, bytes);
        let b_length = self.builder.memory_object_len(b, bytes);
        // valid = a.length + b.length - 1 < 30, which is 1..=30 combined
        let total = self.builder.add(a_length, b_length);
        let one = self.builder.imm(1);
        let before_total = self.builder.sub(total, one);
        let thirty = self.builder.imm(30);
        let valid = self.builder.lt(before_total, thirty);

        // The word ending with a's payload starts with the zero high bytes
        // of its length word and then the length byte, so shifting it up
        // leaves the length, the payload and zeros.
        // a_part = mload(a + a.length) << 8 * (31 - a.length)
        let a_base = self.builder.cast_word(a);
        let a_address = self.builder.add(a_base, a_length);
        let a_word = self.builder.mload(a_address);
        let thirty_one = self.builder.imm(31);
        let a_shift_bytes = self.builder.sub(thirty_one, a_length);
        let three = self.builder.imm(3);
        let a_shift = self.builder.shl(three, a_shift_bytes);
        let a_part = self.builder.shl(a_shift, a_word);

        // The word 30 - a.length bytes into b's length word holds zeros for
        // a's bytes, b's length byte right after them, then b's payload.
        // Only a valid pair offsets the load, so a long `a` cannot move it
        // below `b`. The bytes after the payload may be stale; the mask
        // keeps the two lengths and the payloads.
        // b_part = mload(b + 30 - a.length * valid)
        // packed = (a_part | b_part) & ~0 << 8 * (30 - total)
        let offset = self.builder.cast_word(valid);
        let offset = self.builder.mul(a_length, offset);
        let b_base = self.builder.cast_word(b);
        let b_end = self.builder.add(b_base, thirty);
        let b_address = self.builder.sub(b_end, offset);
        let b_part = self.builder.mload(b_address);
        let packed = self.builder.or(a_part, b_part);
        let padding = self.builder.sub(thirty, total);
        let padding_bits = self.builder.shl(three, padding);
        let all = self.builder.imm(U256::MAX);
        let mask = self.builder.shl(padding_bits, all);
        let packed = self.builder.and(packed, mask);
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
        let a = self.builder.memory_object_in_allocation(allocation, MemoryObjectKind::Bytes);
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
        let b = self.builder.memory_object_in_allocation(b_ptr, MemoryObjectKind::Bytes);
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

    /// Count runes directly in the caller: the result is one word and the body
    /// holds no frame, so the call protocol would be most of a short count.
    pub(super) fn lower_core_string_rune_count_call(
        &mut self,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        let [subject] = *operands else { return None };
        Some(self.lower_core_string_rune_count(subject))
    }

    pub(super) fn lower_core_string_repeat_call(
        &mut self,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        let [subject, times] = *operands else { return None };
        Some(self.lower_core_string_repeat(subject, times))
    }

    /// `subject` repeated `times` times, with the body's sizing and failures:
    /// an empty result is a fresh empty string, the length product panics with
    /// `0x11` and the allocation with `0x41`. One move copies the subject and
    /// each later move doubles the filled prefix, capped at the total, so
    /// `times` copies take a logarithmic number of moves.
    fn lower_core_string_repeat(&mut self, subject: ValueId, times: ValueId) -> ValueId {
        let kind = MemoryObjectKind::Bytes;
        let semantics = AllocationSemantics::SOLIDITY_UNINITIALIZED;
        // length = len(subject)
        // branch times == 0 | length == 0, empty, fill
        let length = self.builder.memory_object_len(subject, kind);
        let no_times = self.builder.eq_zero(times);
        let no_length = self.builder.eq_zero(length);
        let empty = self.builder.or(no_times, no_length);
        let empty_block = self.builder.create_block();
        let fill = self.builder.create_block();
        let header = self.builder.create_block();
        let step_block = self.builder.create_block();
        let filled_block = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.branch(empty, empty_block, fill);

        // empty: result = new bytes(0)
        self.builder.switch_to_block(empty_block);
        let zero = self.builder.imm(0);
        let empty_result = self.builder.alloc_bytes_object(zero, semantics);
        let empty_end = self.builder.current_block();
        self.builder.jump(done);

        // fill:
        //   total = length * times; panic 0x11 if total / times != length
        //   out = new bytes(total), uninitialized
        //   mstore(data(out) + padded(total) - 32, 0)
        //   mcopy(data(out), data(subject), length)
        self.builder.switch_to_block(fill);
        let total = self.builder.mul(length, times);
        let quotient = self.builder.div(total, times);
        let overflow = self.builder.ne(quotient, length);
        self.builder.panic_if(overflow, PanicCode::ArithmeticOverflowUnderflow);
        let out = self.builder.alloc_bytes_object(total, semantics);
        let data = self.builder.memory_object_data(out, kind);
        let data = self.builder.cast_word(data);
        let source = self.builder.memory_object_data(subject, kind);
        let source = self.builder.cast_word(source);
        let last_word = self.builder.add_u64_offset(total, 31);
        let mask = self.builder.imm(U256::MAX << 5);
        let last_word = self.builder.and(last_word, mask);
        let last_word = self.builder.add(data, last_word);
        let word = self.builder.imm(32);
        let last_word = self.builder.sub(last_word, word);
        self.builder.mstore(last_word, zero);
        self.builder.mcopy_heap(data, source, length);
        let fill_end = self.builder.current_block();
        self.builder.jump(header);

        // header: filled = phi [fill: length], [step: filled + step]
        //         branch filled < total, step, filled
        self.builder.switch_to_block(header);
        let filled = self.builder.phi(vec![(fill_end, length)]);
        let more = self.builder.lt(filled, total);
        self.builder.branch(more, step_block, filled_block);

        // step: rest = total - filled; step = min(filled, rest)
        //       mcopy(data + filled, data, step)
        self.builder.switch_to_block(step_block);
        let rest = self.builder.sub(total, filled);
        let shorter = self.builder.lt(filled, rest);
        let step = self.builder.select(shorter, filled, rest);
        let destination = self.builder.add(data, filled);
        self.builder.mcopy_heap(destination, data, step);
        let next = self.builder.add(filled, step);
        let step_end = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(filled, step_end, next);

        self.builder.switch_to_block(filled_block);
        self.builder.jump(done);

        // done: result = phi [empty: new bytes(0)], [filled: out]
        self.builder.switch_to_block(done);
        self.builder.phi(vec![(empty_end, empty_result), (filled_block, out)])
    }

    /// Counts the runes of `subject` the way `Strings.runeCount` steps them:
    /// each by the length its lead byte declares.
    ///
    /// A whole word starting at a rune is counted at once when its text is
    /// well formed. Each byte's top bit marks its class: continuation bytes
    /// (`10xxxxxx`) and leads of at least two, three and four bytes. The
    /// continuation bytes must be exactly those the leads declare, and no lead
    /// may declare five or six bytes; then every other byte starts a rune, and
    /// the last rune runs past the word by what its lead declares beyond it.
    /// Summing the marks with one multiplication counts both. A word of bytes
    /// below 0x80 is 32 runes, and so is a shorter tail of them, masked to the
    /// subject, when its first byte is one. Anything else steps rune by rune
    /// through a table of lengths written to scratch, a malformed word until
    /// its end and a tail until the subject's. Loads may read past the
    /// subject, but those bytes are masked or never stepped to.
    fn lower_core_string_rune_count(&mut self, subject: ValueId) -> ValueId {
        let bytes = MemoryObjectKind::Bytes;
        let length = self.builder.memory_object_len(subject, bytes);
        let data = self.builder.memory_object_data(subject, bytes);
        let end = self.builder.add(data, length);
        let zero = self.builder.imm(0);
        let word = self.builder.imm(32);
        let high_bits = self.builder.imm((U256::MAX / U256::from(255)) << 7);
        let entry = self.builder.current_block();
        let words = self.builder.create_block();
        let load = self.builder.create_block();
        let ascii = self.builder.create_block();
        let classify = self.builder.create_block();
        let valid = self.builder.create_block();
        let word_steps = self.builder.create_block();
        let tail = self.builder.create_block();
        let tail_load = self.builder.create_block();
        let tail_probe = self.builder.create_block();
        let tail_ascii = self.builder.create_block();
        let tail_steps = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.jump(words);

        // next = p + 32; a whole word remains when next <= end
        self.builder.switch_to_block(words);
        let cursor = self.builder.phi(vec![(entry, data)]);
        let count = self.builder.phi(vec![(entry, zero)]);
        let next = self.builder.add(cursor, word);
        let short = self.builder.gt(next, end);
        self.builder.branch(short, tail, load);

        // high = mload(p) & 0x8080..80; a word below 0x80 is 32 runes
        self.builder.switch_to_block(load);
        let text = self.builder.mload(cursor);
        let high = self.builder.and(text, high_bits);
        let all_ascii = self.builder.eq_zero(high);
        self.builder.branch(all_ascii, ascii, classify);
        self.builder.switch_to_block(ascii);
        let ascii_count = self.builder.add(count, word);
        self.builder.jump(words);
        self.builder.add_phi_incoming(cursor, ascii, next);
        self.builder.add_phi_incoming(count, ascii, ascii_count);

        self.builder.switch_to_block(classify);
        let marks = self.rune_marks(text, high);
        self.builder.branch(marks.well_formed, valid, word_steps);

        // count += 32 - conts; p = next + spans - conts
        self.builder.switch_to_block(valid);
        let (conts, spans) = self.rune_sums(&marks);
        let runes = self.builder.sub(word, conts);
        let valid_count = self.builder.add(count, runes);
        let past = self.builder.sub(spans, conts);
        let valid_cursor = self.builder.add(next, past);
        self.builder.jump(words);
        self.builder.add_phi_incoming(cursor, valid, valid_cursor);
        self.builder.add_phi_incoming(count, valid, valid_count);

        // A malformed word is stepped rune by rune until its end.
        self.builder.switch_to_block(word_steps);
        let (word_step, stepped_cursor, stepped_count) =
            self.rune_steps(cursor, count, next, words);
        self.builder.add_phi_incoming(cursor, word_step, stepped_cursor);
        self.builder.add_phi_incoming(count, word_step, stepped_count);

        // if p < end: a tail starting with a byte below 0x80 counts at once
        // when all of its bytes are, and is stepped rune by rune otherwise
        self.builder.switch_to_block(tail);
        let more = self.builder.lt(cursor, end);
        self.builder.branch(more, tail_load, done);
        self.builder.switch_to_block(tail_load);
        let text = self.builder.mload(cursor);
        let top_bit = self.builder.imm(255);
        let first_high = self.builder.shr(top_bit, text);
        self.builder.branch(first_high, tail_steps, tail_probe);
        // rest = end - p; mask = ~(MAX >> 8 * rest)
        self.builder.switch_to_block(tail_probe);
        let rest = self.builder.sub(end, cursor);
        let three = self.builder.imm(3);
        let rest_bits = self.builder.shl(three, rest);
        let all = self.builder.imm(U256::MAX);
        let beyond = self.builder.shr(rest_bits, all);
        let within = self.builder.not(beyond);
        let tail_text = self.builder.and(text, within);
        let tail_high = self.builder.and(tail_text, high_bits);
        let tail_all_ascii = self.builder.eq_zero(tail_high);
        self.builder.branch(tail_all_ascii, tail_ascii, tail_steps);
        self.builder.switch_to_block(tail_ascii);
        let ascii_tail_count = self.builder.add(count, rest);
        self.builder.jump(done);

        // The same stepping, until the subject's end.
        self.builder.switch_to_block(tail_steps);
        let (tail_step, _, stepped_tail_count) = self.rune_steps(cursor, count, end, done);

        self.builder.switch_to_block(done);
        self.builder.phi(vec![
            (tail, count),
            (tail_ascii, ascii_tail_count),
            (tail_step, stepped_tail_count),
        ])
    }

    /// Marks each byte of `text` in its top bit by class: continuation bytes,
    /// and leads of at least two, three and four bytes; `high` is the word's
    /// top bits. The word is well formed when the continuation bytes are
    /// exactly those its leads declare and no lead declares five or six bytes.
    fn rune_marks(&mut self, text: ValueId, high: ValueId) -> RuneMarks {
        // lead = high & (w << 1); cont = high ^ lead
        // lead3 = lead & (w << 2); lead4 = lead3 & (w << 3)
        // declared = lead >> 8 | lead3 >> 16 | lead4 >> 24
        // well_formed = (lead4 & (w << 4)) | (declared ^ cont) == 0
        let one = self.builder.imm(1);
        let shifted = self.builder.shl(one, text);
        let lead = self.builder.and(high, shifted);
        let cont = self.builder.xor(high, lead);
        let two = self.builder.imm(2);
        let shifted = self.builder.shl(two, text);
        let lead3 = self.builder.and(lead, shifted);
        let three = self.builder.imm(3);
        let shifted = self.builder.shl(three, text);
        let lead4 = self.builder.and(lead3, shifted);
        let four = self.builder.imm(4);
        let shifted = self.builder.shl(four, text);
        let long_lead = self.builder.and(lead4, shifted);
        let eight = self.builder.imm(8);
        let sixteen = self.builder.imm(16);
        let twenty_four = self.builder.imm(24);
        let declared = self.builder.shr(eight, lead);
        let second = self.builder.shr(sixteen, lead3);
        let declared = self.builder.or(declared, second);
        let third = self.builder.shr(twenty_four, lead4);
        let declared = self.builder.or(declared, third);
        let undeclared = self.builder.xor(declared, cont);
        let malformed = self.builder.or(undeclared, long_lead);
        let well_formed = self.builder.eq_zero(malformed);
        RuneMarks { well_formed, cont, lead, lead3, lead4 }
    }

    /// Counts the continuation bytes and the bytes the leads declare, summing
    /// each word of marks with one multiplication.
    fn rune_sums(&mut self, marks: &RuneMarks) -> (ValueId, ValueId) {
        // conts = ((cont >> 7) * 0x0101..01) >> 248
        // spans = (((lead >> 7) + (lead3 >> 7) + (lead4 >> 7)) * 0x0101..01) >> 248
        let seven = self.builder.imm(7);
        let top = self.builder.imm(248);
        let ones = self.builder.imm(U256::MAX / U256::from(255));
        let cont = self.builder.shr(seven, marks.cont);
        let cont = self.builder.mul(cont, ones);
        let conts = self.builder.shr(top, cont);
        let spans = self.builder.shr(seven, marks.lead);
        let second = self.builder.shr(seven, marks.lead3);
        let spans = self.builder.add(spans, second);
        let third = self.builder.shr(seven, marks.lead4);
        let spans = self.builder.add(spans, third);
        let spans = self.builder.mul(spans, ones);
        let spans = self.builder.shr(top, spans);
        (conts, spans)
    }

    /// Steps runes from `start` until one starts at or past `bound`, then
    /// jumps to `exit`. Returns the loop block and the cursor and count it
    /// leaves with, for `exit`'s phis.
    fn rune_steps(
        &mut self,
        start: ValueId,
        count: ValueId,
        bound: ValueId,
        exit: BlockId,
    ) -> (BlockId, ValueId, ValueId) {
        // mstore(0, 0x0101..01); mstore(32, lengths): byte(0, mload(k)) is the
        // length of a rune whose lead has top six bits k
        let zero = self.builder.imm(0);
        let ones = self.builder.imm(U256::MAX / U256::from(255));
        self.builder.mstore(zero, ones);
        let word = self.builder.imm(32);
        let lengths = self.builder.imm(U256::from_be_slice(&[
            2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3, 4,
            4, 5, 6,
        ]));
        self.builder.mstore(word, lengths);
        // do { q += byte(0, mload(mload(q) >> 250)); c += 1 } while (q < bound)
        let entry = self.builder.current_block();
        let step = self.builder.create_block();
        self.builder.jump(step);
        self.builder.switch_to_block(step);
        let rune = self.builder.phi(vec![(entry, start)]);
        let stepped = self.builder.phi(vec![(entry, count)]);
        let lead_word = self.builder.mload(rune);
        let lead_shift = self.builder.imm(250);
        let class = self.builder.shr(lead_shift, lead_word);
        let class_word = self.builder.mload(class);
        let rune_length = self.builder.byte(zero, class_word);
        let next_rune = self.builder.add(rune, rune_length);
        let one = self.builder.imm(1);
        let next_count = self.builder.add(stepped, one);
        let within = self.builder.lt(next_rune, bound);
        self.builder.branch(within, step, exit);
        self.builder.add_phi_incoming(rune, step, next_rune);
        self.builder.add_phi_incoming(stepped, step, next_count);
        (step, next_rune, next_count)
    }

    /// Keep each search direction as one callable body per module; a scalar
    /// result lets a single call site inline it.
    pub(super) fn lower_core_string_index_of_call(
        &mut self,
        operands: &[ValueId],
        reverse: bool,
    ) -> Option<ValueId> {
        let [subject, needle, from] = *operands else { return None };
        let name = if reverse { sym::core_string_last_index_of } else { sym::core_string_index_of };
        let helper = self.lazy_helper(name, |this, function| {
            let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
            let bytes = MirType::MemoryObject(MemoryObjectKind::Bytes);
            let subject = lowerer.builder.add_param(bytes);
            let needle = lowerer.builder.add_param(bytes);
            let from = lowerer.builder.add_param(MirType::I256);
            lowerer.builder.set_return_type(MirType::I256);
            let result = lowerer.lower_core_string_index_of(subject, needle, from, reverse);
            lowerer.builder.ret([result]);
            Some(())
        })?;
        Some(self.builder.icall(helper, vec![subject, needle, from], MirType::I256))
    }

    /// Returns the first offset at or after `from`, or with `reverse` the last
    /// offset at or before it, where `needle` occurs in `subject`, and `MAX`
    /// when there is none. An empty needle is found at `min(from, length)`.
    /// Each candidate compares the needle's first word under a mask of its
    /// leading bytes, which decides needles of at most a word. A longer needle
    /// then compares its last word, aligned to its end, and any words between;
    /// the first and last words cover a needle of at most two. A load may read
    /// past the subject, but only the needle's bytes are compared.
    fn lower_core_string_index_of(
        &mut self,
        subject: ValueId,
        needle: ValueId,
        from: ValueId,
        reverse: bool,
    ) -> ValueId {
        let bytes = MemoryObjectKind::Bytes;
        let length = self.builder.memory_object_len(subject, bytes);
        let needle_length = self.builder.memory_object_len(needle, bytes);
        let not_found = self.builder.imm(U256::MAX);
        let done = self.builder.create_block();
        let missing = self.builder.create_block();

        // if needle_length == 0: result = min(from, length)
        let empty = self.builder.create_block();
        let nonempty = self.builder.create_block();
        let needle_empty = self.builder.eq_zero(needle_length);
        self.builder.branch(needle_empty, empty, nonempty);
        self.builder.switch_to_block(empty);
        let beyond = self.builder.gt(from, length);
        let at_end = self.builder.select(beyond, length, from);
        self.builder.jump(done);

        // last = length - needle_length, or missing when the needle is longer
        // start = from, or missing past `last`; reversed, min(from, last)
        self.builder.switch_to_block(nonempty);
        let fits = self.builder.create_block();
        let too_long = self.builder.gt(needle_length, length);
        self.builder.branch(too_long, missing, fits);
        self.builder.switch_to_block(fits);
        let last = self.builder.sub(length, needle_length);
        let past_last = self.builder.gt(from, last);
        let start = if reverse {
            self.builder.select(past_last, last, from)
        } else {
            let in_range = self.builder.create_block();
            self.builder.branch(past_last, missing, in_range);
            self.builder.switch_to_block(in_range);
            from
        };

        // A shift of a word or more clears every bit, so a needle of at least a
        // word compares its whole first word.
        // mask = ~(MAX >> 8 * needle_length)
        let source = self.builder.memory_object_data(subject, bytes);
        let needle_data = self.builder.memory_object_data(needle, bytes);
        let needle_word = self.builder.mload(needle_data);
        let word = self.builder.imm(32);
        let three = self.builder.imm(3);
        let needle_bits = self.builder.shl(three, needle_length);
        let all = self.builder.imm(U256::MAX);
        let beyond_needle = self.builder.shr(needle_bits, all);
        let mask = self.builder.not(beyond_needle);
        let long = self.builder.gt(needle_length, word);
        let first = self.builder.add(source, start);
        let end = self.builder.add(source, last);
        let entry = self.builder.current_block();
        let header = self.builder.create_block();
        let verify = self.builder.create_block();
        let verify_tail = self.builder.create_block();
        let verify_middle = self.builder.create_block();
        let middle = self.builder.create_block();
        let middle_step = self.builder.create_block();
        let found = self.builder.create_block();
        let advance = self.builder.create_block();
        self.builder.jump(header);

        // candidate: (mload(cursor) ^ needle_word) & mask == 0
        self.builder.switch_to_block(header);
        let cursor = self.builder.phi(vec![(entry, first)]);
        let candidate = self.builder.mload(cursor);
        let different = self.builder.xor(candidate, needle_word);
        let different = self.builder.and(different, mask);
        let prefix_equal = self.builder.eq_zero(different);
        self.builder.branch(prefix_equal, verify, advance);

        self.builder.switch_to_block(verify);
        self.builder.branch(long, verify_tail, found);

        // mload(cursor + tail) == mload(needle + tail), then the words between
        self.builder.switch_to_block(verify_tail);
        let tail = self.builder.sub(needle_length, word);
        let candidate_tail = self.builder.add(cursor, tail);
        let candidate_tail = self.builder.mload(candidate_tail);
        let needle_tail = self.builder.add(needle_data, tail);
        let needle_tail = self.builder.mload(needle_tail);
        let tail_equal = self.builder.eq(candidate_tail, needle_tail);
        self.builder.branch(tail_equal, verify_middle, advance);

        // for (k = 32; k < tail; k += 32) mload(cursor + k) == mload(needle + k)
        self.builder.switch_to_block(verify_middle);
        let beyond_first = self.builder.gt(tail, word);
        self.builder.branch(beyond_first, middle, found);
        self.builder.switch_to_block(middle);
        let offset = self.builder.phi(vec![(verify_middle, word)]);
        let candidate_word = self.builder.add(cursor, offset);
        let candidate_word = self.builder.mload(candidate_word);
        let needle_middle = self.builder.add(needle_data, offset);
        let needle_middle = self.builder.mload(needle_middle);
        let word_equal = self.builder.eq(candidate_word, needle_middle);
        self.builder.branch(word_equal, middle_step, advance);
        self.builder.switch_to_block(middle_step);
        let next_offset = self.builder.add(offset, word);
        let more_words = self.builder.lt(next_offset, tail);
        self.builder.branch(more_words, middle, found);
        self.builder.add_phi_incoming(offset, middle_step, next_offset);

        self.builder.switch_to_block(found);
        let at = self.builder.sub(cursor, source);
        self.builder.jump(done);

        // forward: cursor += 1 while cursor <= end
        // reverse: cursor -= 1 while cursor > source
        self.builder.switch_to_block(advance);
        let one = self.builder.imm(1);
        if reverse {
            let step = self.builder.create_block();
            let at_first = self.builder.eq(cursor, source);
            self.builder.branch(at_first, missing, step);
            self.builder.switch_to_block(step);
            let next = self.builder.sub(cursor, one);
            self.builder.jump(header);
            self.builder.add_phi_incoming(cursor, step, next);
        } else {
            let next = self.builder.add(cursor, one);
            let past_end = self.builder.gt(next, end);
            self.builder.branch(past_end, missing, header);
            self.builder.add_phi_incoming(cursor, advance, next);
        }

        self.builder.switch_to_block(missing);
        self.builder.jump(done);

        self.builder.switch_to_block(done);
        self.builder.phi(vec![(empty, at_end), (missing, not_found), (found, at)])
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

        // destination = fmp + 32
        // source = data(subject)
        // last = source + (length - needle_length)
        // needle_word = mload data(needle)
        // prefix_mask = ~0 << 8 * (32 - needle_length % 32)
        // jumpi needle_length < 32, short_scan, long_hash
        self.builder.switch_to_block(nonempty);
        let allocation_base = self.builder.fmp();
        let header_size = self.builder.imm(32);
        let destination = self.builder.add(allocation_base, header_size);
        let source = self.builder.memory_object_data(subject, bytes);
        let source = self.builder.cast_word(source);
        let search_end = self.builder.sub(length, needle_length);
        let last = self.builder.add(source, search_end);
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
        let short_scan = self.builder.create_block();
        let long_hash = self.builder.create_block();
        let finish = self.builder.create_block();
        let short = self.builder.lt(needle_length, word);
        self.builder.branch(short, short_scan, long_hash);

        let scan =
            SearchScan { source, destination, last, needle_word, prefix_mask, needle_length };
        self.builder.switch_to_block(short_scan);
        let (short_exit, short_output) = self.lower_core_string_indices_scan(&scan, None, finish);

        // needle_hash = keccak256(data(needle), needle_length)
        self.builder.switch_to_block(long_hash);
        let needle_hash = self.builder.keccak256(needle_data, needle_length);
        let (long_exit, long_output) =
            self.lower_core_string_indices_scan(&scan, Some(needle_hash), finish);

        // output = phi [short: output], [long: output]
        // count = (output - destination) / 32
        // out = alloc word array at fmp for count + 1 + split_mode words; len(out) = count
        self.builder.switch_to_block(finish);
        let output = self.builder.phi(vec![(short_exit, short_output), (long_exit, long_output)]);
        let one = self.builder.imm(1);
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

    /// Emits one match-offset scan from the current block with pointer cursors,
    /// streaming each non-overlapping match's offset as a word. Returns the
    /// header that exits to `finish` and the output cursor it exits with. Long
    /// needles confirm each prefix match by hash.
    fn lower_core_string_indices_scan(
        &mut self,
        scan: &SearchScan,
        needle_hash: Option<ValueId>,
        finish: BlockId,
    ) -> (BlockId, ValueId) {
        let entry = self.builder.current_block();
        let header = self.builder.create_block();
        let compare = self.builder.create_block();
        let matched = self.builder.create_block();
        let advance = self.builder.create_block();
        self.builder.jump(header);

        // cursor = phi [entry: source], [matched: cursor + needle_length], [advance: cursor + 1]
        // output = phi [entry: destination], [matched: output + 32], [advance: output]
        // jumpi cursor > last, finish, compare
        self.builder.switch_to_block(header);
        let cursor = self.builder.phi(vec![(entry, scan.source)]);
        let output = self.builder.phi(vec![(entry, scan.destination)]);
        let past_end = self.builder.gt(cursor, scan.last);
        self.builder.branch(past_end, finish, compare);

        // jumpi (mload(cursor) ^ needle_word) & prefix_mask == 0, matched, advance
        self.builder.switch_to_block(compare);
        let candidate_word = self.builder.mload(cursor);
        let different = self.builder.xor(candidate_word, scan.needle_word);
        let different = self.builder.and(different, scan.prefix_mask);
        let prefix_equal = self.builder.eq_zero(different);
        if let Some(needle_hash) = needle_hash {
            // jumpi keccak256(cursor, needle_length) == needle_hash, matched, advance
            let verify = self.builder.create_block();
            self.builder.branch(prefix_equal, verify, advance);
            self.builder.switch_to_block(verify);
            let candidate_hash = self.builder.keccak256(cursor, scan.needle_length);
            let equal = self.builder.eq(candidate_hash, needle_hash);
            self.builder.branch(equal, matched, advance);
        } else {
            self.builder.branch(prefix_equal, matched, advance);
        }

        // mstore output, cursor - source
        self.builder.switch_to_block(matched);
        let at = self.builder.sub(cursor, scan.source);
        self.builder.mstore(output, at);
        let word = self.builder.imm(32);
        let next_output = self.builder.add(output, word);
        let next_cursor = self.builder.add(cursor, scan.needle_length);
        self.builder.jump(header);
        self.builder.add_phi_incoming(cursor, matched, next_cursor);
        self.builder.add_phi_incoming(output, matched, next_output);

        self.builder.switch_to_block(advance);
        let one = self.builder.imm(1);
        let next_cursor = self.builder.add(cursor, one);
        self.builder.jump(header);
        self.builder.add_phi_incoming(cursor, advance, next_cursor);
        self.builder.add_phi_incoming(output, advance, output);

        (header, output)
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

    /// Spells `value` a word at a time: the digits of its low sixteen bytes
    /// end at `end`, and those of the high sixteen precede them when any is
    /// set. Returns where the digits of the highest set byte start, two per
    /// significant byte before `end`, leaving the builder in the block that
    /// continues with them.
    fn lower_core_minimal_hex_words(&mut self, value: ValueId, end: ValueId) -> ValueId {
        let high_half = self.builder.create_block();
        let join = self.builder.create_block();

        // Each byte from the highest set one down becomes nonzero, and a
        // multiplication sums one mark per such byte into the top byte:
        // smeared = value | value >> 8 | value >> 16 | ... | value >> 248
        // marks = ((smeared & 0x7f..7f) + 0x7f..7f | smeared) & 0x80..80
        // digits = (marks >> 7) * 0x0202..02 >> 248
        // mstore(end - 32, hex_word(value & (2**128 - 1)))
        let mut smeared = value;
        for shift in [8, 16, 32, 64, 128] {
            let shift = self.builder.imm(shift);
            let shifted = self.builder.shr(shift, smeared);
            smeared = self.builder.or(smeared, shifted);
        }
        let ones = U256::MAX / U256::from(255);
        let low_bits = self.builder.imm(ones * U256::from(0x7f));
        let kept = self.builder.and(smeared, low_bits);
        let carried = self.builder.add(kept, low_bits);
        let any = self.builder.or(carried, smeared);
        let top_bits = self.builder.imm(ones * U256::from(0x80));
        let marks = self.builder.and(any, top_bits);
        let seven = self.builder.imm(7);
        let marks = self.builder.shr(seven, marks);
        let twos = self.builder.imm(ones * U256::from(2));
        let digits = self.builder.mul(marks, twos);
        let top = self.builder.imm(248);
        let digits = self.builder.shr(top, digits);
        let cursor = self.builder.sub(end, digits);
        let low_mask = self.builder.imm(U256::MAX >> 128);
        let low = self.builder.and(value, low_mask);
        let low = super::hex::hex_word(&mut self.builder, low);
        let word = self.builder.imm(32);
        let low_start = self.builder.sub(end, word);
        self.builder.mstore(low_start, low);
        let half = self.builder.imm(128);
        let high = self.builder.shr(half, value);
        let has_high = self.builder.ne_zero(high);
        self.builder.branch(has_high, high_half, join);

        // mstore(end - 64, hex_word(value >> 128))
        self.builder.switch_to_block(high_half);
        let high = super::hex::hex_word(&mut self.builder, high);
        let high_start = self.builder.sub(low_start, word);
        self.builder.mstore(high_start, high);
        self.builder.jump(join);

        self.builder.switch_to_block(join);
        cursor
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

        // destination = fmp + 32
        // source = data(subject)
        // last = source + (length - needle_length)
        // needle_word = mload data(needle)
        // prefix_mask = ~0 << 8 * (32 - needle_length % 32)
        // jumpi needle_length < 32, short_scan, long_hash
        self.builder.switch_to_block(allocate);
        let allocation_base = self.builder.fmp();
        let header = self.builder.imm(32);
        let destination = self.builder.add(allocation_base, header);
        let source = self.builder.memory_object_data(subject, kind);
        let source = self.builder.cast_word(source);
        let search_end = self.builder.sub(length, needle_length);
        let last = self.builder.add(source, search_end);
        let needle_data = self.builder.memory_object_data(needle, kind);
        let replacement_data = self.builder.memory_object_data(replacement, kind);
        let needle_word = self.builder.mload(needle_data);
        let low_five = self.builder.imm(31);
        let remainder = self.builder.and(needle_length, low_five);
        let word = self.builder.imm(32);
        let missing = self.builder.sub(word, remainder);
        let eight = self.builder.imm(8);
        let masked_bits = self.builder.mul(missing, eight);
        let all = self.builder.imm(U256::MAX);
        let prefix_mask = self.builder.shl(masked_bits, all);
        let short_scan = self.builder.create_block();
        let long_hash = self.builder.create_block();
        let finish = self.builder.create_block();
        let short = self.builder.lt(needle_length, word);
        self.builder.branch(short, short_scan, long_hash);

        let scan = ReplaceScan {
            source,
            destination,
            last,
            needle_word,
            prefix_mask,
            needle_length,
            replacement_data,
            replacement_length,
        };
        self.builder.switch_to_block(short_scan);
        let (short_exit, short_copied, short_output) =
            self.lower_core_string_replace_scan(&scan, None, finish);

        // needle_hash = keccak256(data(needle), needle_length)
        self.builder.switch_to_block(long_hash);
        let needle_hash = self.builder.keccak256(needle_data, needle_length);
        let (long_exit, long_copied, long_output) =
            self.lower_core_string_replace_scan(&scan, Some(needle_hash), finish);

        // copied = phi [short: copied], [long: copied]
        // output = phi [short: output], [long: output]
        // tail = last + needle_length - copied
        // mcopy output, copied, tail
        // output_length = output + tail - (fmp + 32)
        // out = alloc bytes at fmp, padded(output_length); len(out) = output_length
        self.builder.switch_to_block(finish);
        let copied = self.builder.phi(vec![(short_exit, short_copied), (long_exit, long_copied)]);
        let output = self.builder.phi(vec![(short_exit, short_output), (long_exit, long_output)]);
        let end = self.builder.add(last, needle_length);
        let tail = self.builder.sub(end, copied);
        self.builder.mcopy_heap(output, copied, tail);
        let output_end = self.builder.add(output, tail);
        // The scans allocate nothing, so the free-memory pointer still marks
        // the output header; reading it again keeps the destination out of
        // the loops' live state.
        let allocation_base = self.builder.fmp();
        let header = self.builder.imm(32);
        let destination = self.builder.add(allocation_base, header);
        let output_length = self.builder.sub(output_end, destination);
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

    /// Emits one replacement scan from the current block, with pointer
    /// cursors so that neither the subject nor the output base stays live.
    /// Returns the header that exits to `finish` and the pending-run start and
    /// output cursor it exits with. Long needles confirm each prefix match by
    /// hash.
    fn lower_core_string_replace_scan(
        &mut self,
        scan: &ReplaceScan,
        needle_hash: Option<ValueId>,
        finish: BlockId,
    ) -> (BlockId, ValueId, ValueId) {
        let entry = self.builder.current_block();
        let header = self.builder.create_block();
        let compare = self.builder.create_block();
        let matched = self.builder.create_block();
        let advance = self.builder.create_block();
        self.builder.jump(header);

        // at = phi [entry: source], [matched: at + needle_length], [advance: at + 1]
        // copied = phi [entry: source], [matched: at + needle_length], [advance: copied]
        // output = phi [entry: destination], [matched: next_output], [advance: output]
        // jumpi at > last, finish, compare
        self.builder.switch_to_block(header);
        let at = self.builder.phi(vec![(entry, scan.source)]);
        let copied = self.builder.phi(vec![(entry, scan.source)]);
        let output = self.builder.phi(vec![(entry, scan.destination)]);
        let past_end = self.builder.gt(at, scan.last);
        self.builder.branch(past_end, finish, compare);

        // jumpi (mload(at) ^ needle_word) & prefix_mask == 0, matched, advance
        self.builder.switch_to_block(compare);
        let candidate_word = self.builder.mload(at);
        let different = self.builder.xor(candidate_word, scan.needle_word);
        let different = self.builder.and(different, scan.prefix_mask);
        let prefix_equal = self.builder.eq_zero(different);
        if let Some(needle_hash) = needle_hash {
            // jumpi keccak256(at, needle_length) == needle_hash, matched, advance
            let verify = self.builder.create_block();
            self.builder.branch(prefix_equal, verify, advance);
            self.builder.switch_to_block(verify);
            let candidate_hash = self.builder.keccak256(at, scan.needle_length);
            let equal = self.builder.eq(candidate_hash, needle_hash);
            self.builder.branch(equal, matched, advance);
        } else {
            self.builder.branch(prefix_equal, matched, advance);
        }

        // mcopy output, copied, at - copied
        // mcopy output + (at - copied), data(replacement), replacement_length
        self.builder.switch_to_block(matched);
        let run = self.builder.sub(at, copied);
        self.builder.mcopy_heap(output, copied, run);
        let after_run = self.builder.add(output, run);
        self.builder.mcopy_heap(after_run, scan.replacement_data, scan.replacement_length);
        let next_output = self.builder.add(after_run, scan.replacement_length);
        let next_at = self.builder.add(at, scan.needle_length);
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

        (header, copied, output)
    }
}

/// Loop-invariant words of one replacement scan.
struct ReplaceScan {
    /// First subject byte.
    source: ValueId,
    /// First output byte, one word above the free-memory pointer.
    destination: ValueId,
    /// Last position where the needle still fits.
    last: ValueId,
    /// The needle's first word.
    needle_word: ValueId,
    /// Keeps the needle's first `length % 32` bytes of a word.
    prefix_mask: ValueId,
    needle_length: ValueId,
    replacement_data: ValueId,
    replacement_length: ValueId,
}

/// Loop-invariant words of one match-offset scan.
struct SearchScan {
    /// First subject byte.
    source: ValueId,
    /// First output word, one word above the free-memory pointer.
    destination: ValueId,
    /// Last position where the needle still fits.
    last: ValueId,
    /// The needle's first word.
    needle_word: ValueId,
    /// Keeps the needle's first `length % 32` bytes of a word.
    prefix_mask: ValueId,
    needle_length: ValueId,
}

/// Top-bit marks of one word's byte classes, from which runes are counted.
struct RuneMarks {
    /// Whether the continuation bytes are exactly those the leads declare.
    well_formed: ValueId,
    /// Continuation bytes, `10xxxxxx`.
    cont: ValueId,
    /// Leads of at least two bytes, `11xxxxxx`.
    lead: ValueId,
    /// Leads of at least three bytes, `111xxxxx`.
    lead3: ValueId,
    /// Leads of at least four bytes, `1111xxxx`.
    lead4: ValueId,
}
