//! Hexadecimal text lowering for the compiler-owned core modules.
//!
//! `Strings.toHexString(value)` and its unprefixed form spell the fewest whole
//! bytes of a value, two digits per byte from the low end, through the
//! minimal-hex digit loop. Gas builds spell the first two bytes that way and
//! a wider value a word at a time like the fixed widths, placing the header by
//! a count of the value's significant bytes. The fixed-width forms and
//! `Hex.encode` convert
//! sixteen bytes at a time: nibble spreading puts each of 32 nibbles in its
//! own byte, and one carry marks the nibbles that become letters. A value of
//! up to sixteen bytes is one word; up to 32 is two, and a wider width first
//! fills leading '0' words. A bytes input loads sixteen bytes per word,
//! end-aligned so that no load reads past the input; a partial last chunk is
//! shifted up and its artificial digits are cleared by the final padding
//! store.
//!
//! Each output owns an extra word of capacity, so whole-word stores at the
//! end stay inside it, and the padding store initializes the ABI padding.
//! Failures follow the checked bodies: doubling a count panics on arithmetic
//! overflow, a length above `2**64 - 1` panics as an allocation, and a fixed width too
//! narrow for its value reverts with `HexLengthInsufficient()` after the
//! allocation, as the body does. A `0x` prefix is written by one word store
//! that rewrites the length word's low 30 bytes and ends in the first two data
//! bytes, so no byte stores are needed. The checked Solidity bodies remain the
//! reference under `-Zno-core-intrinsics`.

use super::*;

/// The selector of `HexLengthInsufficient()`.
const HEX_LENGTH_INSUFFICIENT: u64 = 0x2194_895a;

/// Thirty-two ASCII '0' digits.
const ASCII_ZEROS: U256 = U256::from_be_bytes([b'0'; 32]);

impl FunctionLowerer<'_, '_> {
    /// Lowers `Strings.toHexString` and `toHexStringNoPrefix` of either arity.
    pub(super) fn lower_core_string_hex_call(
        &mut self,
        operands: &[ValueId],
        prefixed: bool,
    ) -> Option<ValueId> {
        let prefix_length = self.builder.imm(if prefixed { 2 } else { 0 });
        match *operands {
            [value] => Some(self.lower_core_string_minimal_hex(value, prefix_length, true)),
            [value, byte_count] => Some(self.lower_core_fixed_hex(value, byte_count, prefixed)),
            _ => None,
        }
    }

    /// Lowers `Hex.encode` and `Hex.encodePrefixed`.
    pub(super) fn lower_core_hex_encode_call(
        &mut self,
        operands: &[ValueId],
        prefixed: bool,
    ) -> Option<ValueId> {
        let [data] = *operands else { return None };
        Some(self.lower_core_hex_encode(data, prefixed))
    }

    /// Allocates the output for `count` bytes of input: two digits each and
    /// the prefix, with one word of slack. Returns the object, its length,
    /// the first digit's address, and the digit count. `prefix` is the
    /// prefix length; `checked_prefix` adds it with overflow checking before
    /// the allocation limit, as a body that allocates the whole string at
    /// once does, instead of after it, as a concatenation does.
    fn alloc_core_hex_output(
        &mut self,
        count: ValueId,
        prefix: u64,
        checked_prefix: bool,
    ) -> (ValueId, ValueId, ValueId, ValueId) {
        // panic 0x11 if count >> 255 != 0
        // digits = count << 1
        // limited = checked_prefix ? digits + prefix (panic 0x11 on wrap) : digits
        // panic 0x41 if limited > 2**64 - 1
        // out = alloc(header + align32(digits + prefix + 32))
        // out.length = digits + prefix
        let top = self.builder.imm(255);
        let high = self.builder.shr(top, count);
        let overflow = self.builder.ne_zero(high);
        self.builder.panic_if(overflow, PanicCode::ArithmeticOverflowUnderflow);
        let one = self.builder.imm(1);
        let digits = self.builder.shl(one, count);
        let prefix = self.builder.imm(prefix);
        let length = if checked_prefix {
            let length = self.builder.add(digits, prefix);
            let overflow = self.builder.lt(length, digits);
            self.builder.panic_if(overflow, PanicCode::ArithmeticOverflowUnderflow);
            length
        } else {
            digits
        };
        let limit = self.builder.imm(u64::MAX);
        let too_long = self.builder.gt(length, limit);
        self.builder.panic_if(too_long, PanicCode::MemoryAllocationOverflow);
        let length = if checked_prefix { length } else { self.builder.add(digits, prefix) };
        let slack = self.builder.imm(95);
        let size = self.builder.add(length, slack);
        let mask = self.builder.imm(U256::MAX << 5);
        let size = self.builder.and(size, mask);
        let out = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::SOLIDITY_UNINITIALIZED,
        );
        self.builder.set_memory_object_len(out, length, MemoryObjectKind::Bytes);
        let data = self.builder.memory_object_data(out, MemoryObjectKind::Bytes);
        let start = self.builder.add(data, prefix);
        (out, length, start, digits)
    }

    /// Writes `0x` into the first two data bytes by restoring the length
    /// word's low 30 bytes one word later.
    fn store_core_hex_prefix(&mut self, out: ValueId, length: ValueId) {
        // mstore(out + 2, length << 16 | "0x")
        let base = self.builder.cast(out, MirType::MemPtr);
        let address = self.builder.add_u64_offset(base, 2);
        let sixteen = self.builder.imm(16);
        let shifted = self.builder.shl(sixteen, length);
        let prefix = self.builder.imm(0x3078);
        let word = self.builder.or(shifted, prefix);
        self.builder.mstore(address, word);
    }

    /// The low `byteCount` bytes of `value`, two digits each.
    fn lower_core_fixed_hex(
        &mut self,
        value: ValueId,
        byte_count: ValueId,
        prefixed: bool,
    ) -> ValueId {
        // The prefixed body concatenates `0x` onto the checked digits.
        let prefix = if prefixed { 2 } else { 0 };
        let (out, length, start, digits) = self.alloc_core_hex_output(byte_count, prefix, false);

        // A width below 32 bytes that loses set bits is too narrow. The width
        // is below 2**63 here, so the shift amount cannot wrap.
        // revert HexLengthInsufficient() if value >> 8 * byteCount != 0
        let three = self.builder.imm(3);
        let bits = self.builder.shl(three, byte_count);
        let lost = self.builder.shr(bits, value);
        let insufficient = self.builder.ne_zero(lost);
        revert_with_selector(&mut self.builder, insufficient, HEX_LENGTH_INSUFFICIENT);

        let one_word = self.builder.create_block();
        let two_words = self.builder.create_block();
        let done = self.builder.create_block();
        let sixteen = self.builder.imm(16);
        let narrow = self.builder.gt(byte_count, sixteen);
        self.builder.branch(narrow, two_words, one_word);

        // mstore(start, hex_word(value) << 8 * (32 - digits))
        self.builder.switch_to_block(one_word);
        let word = hex_word(&mut self.builder, value);
        let word_bits = self.builder.imm(256);
        let digit_bits = self.builder.shl(three, digits);
        let shift = self.builder.sub(word_bits, digit_bits);
        let word = self.builder.shl(shift, word);
        self.builder.mstore(start, word);
        self.builder.jump(done);

        // lead = max(digits - 64, 0) '0' digits, then the high and low halves
        // mstore(start + i, "00...0") for i < lead
        // mstore(start + lead, hex_word(value >> 128) << 8 * max(64 - digits, 0))
        // mstore(start + digits - 32, hex_word(value & (2**128 - 1)))
        // mstore(start + digits, 0)
        self.builder.switch_to_block(two_words);
        let sixty_four = self.builder.imm(64);
        let wide = self.builder.gt(digits, sixty_four);
        let excess = self.builder.sub(digits, sixty_four);
        let zero = self.builder.imm(0);
        let lead = self.builder.select(wide, excess, zero);
        let thirty_one = self.builder.imm(31);
        let lead_words = self.builder.add(lead, thirty_one);
        let five = self.builder.imm(5);
        let lead_words = self.builder.shr(five, lead_words);
        let zeros = self.builder.imm(ASCII_ZEROS);
        self.builder.counted_loop(lead_words, |builder, index| {
            let five = builder.imm(5);
            let offset = builder.shl(five, index);
            let address = builder.add(start, offset);
            builder.mstore(address, zeros);
        });
        let high_start = self.builder.add(start, lead);
        let one_twenty_eight = self.builder.imm(128);
        let high = self.builder.shr(one_twenty_eight, value);
        let high = hex_word(&mut self.builder, high);
        let short_of = self.builder.sub(sixty_four, digits);
        let high_bytes = self.builder.select(wide, zero, short_of);
        let high_shift = self.builder.shl(three, high_bytes);
        let high = self.builder.shl(high_shift, high);
        self.builder.mstore(high_start, high);
        let low_mask = self.builder.imm(U256::MAX >> 128);
        let low = self.builder.and(value, low_mask);
        let low = hex_word(&mut self.builder, low);
        let end = self.builder.add(start, digits);
        let thirty_two = self.builder.imm(32);
        let low_start = self.builder.sub(end, thirty_two);
        self.builder.mstore(low_start, low);
        self.builder.mstore(end, zero);
        self.builder.jump(done);

        self.builder.switch_to_block(done);
        if prefixed {
            self.store_core_hex_prefix(out, length);
        }
        out
    }

    /// The digits of `data`: nothing for an empty input, one or two bytes
    /// through a four-lane spread, up to sixteen bytes in one word without a
    /// loop, and longer inputs sixteen bytes per word.
    fn lower_core_hex_encode(&mut self, data: ValueId, prefixed: bool) -> ValueId {
        let n = self.builder.memory_object_len(data, MemoryObjectKind::Bytes);
        let empty = self.builder.create_block();
        let nonempty = self.builder.create_block();
        let tiny = self.builder.create_block();
        let wider = self.builder.create_block();
        let short = self.builder.create_block();
        let long = self.builder.create_block();
        let done = self.builder.create_block();
        let is_empty = self.builder.eq_zero(n);
        self.builder.branch(is_empty, empty, nonempty);

        // out = alloc(header + prefix word); out.length = prefix; out[0..2] = prefix
        self.builder.switch_to_block(empty);
        let size = self.builder.imm(if prefixed { 64 } else { 32 });
        let empty_out = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        let length = self.builder.imm(if prefixed { 2 } else { 0 });
        self.builder.set_memory_object_len(empty_out, length, MemoryObjectKind::Bytes);
        if prefixed {
            let prefix = self.builder.imm(U256::from(0x3078) << 240);
            let zero = self.builder.imm(0);
            self.builder.memory_object_store_word(empty_out, zero, prefix);
        }
        self.builder.jump(done);

        self.builder.switch_to_block(nonempty);
        let three = self.builder.imm(3);
        let is_tiny = self.builder.lt(n, three);
        self.builder.branch(is_tiny, tiny, wider);

        self.builder.switch_to_block(tiny);
        let tiny_out = self.lower_core_hex_encode_tiny(data, n, prefixed);
        let tiny_exit = self.builder.current_block();
        self.builder.jump(done);

        self.builder.switch_to_block(wider);
        let seventeen = self.builder.imm(17);
        let is_short = self.builder.lt(n, seventeen);
        self.builder.branch(is_short, short, long);

        self.builder.switch_to_block(short);
        let short_out = self.lower_core_hex_encode_short(data, n, prefixed);
        let short_exit = self.builder.current_block();
        self.builder.jump(done);

        self.builder.switch_to_block(long);
        let long_out = self.lower_core_hex_encode_long(data, n, prefixed);
        let long_exit = self.builder.current_block();
        self.builder.jump(done);

        self.builder.switch_to_block(done);
        self.builder.phi(vec![
            (empty, empty_out),
            (tiny_exit, tiny_out),
            (short_exit, short_out),
            (long_exit, long_out),
        ])
    }

    /// Encodes one or two bytes: their four digits come from a two-step
    /// spread over one four-byte lane group.
    fn lower_core_hex_encode_tiny(&mut self, data: ValueId, n: ValueId, prefixed: bool) -> ValueId {
        // out = alloc(64)
        // out.length = 2n + prefix
        let size = self.builder.imm(64);
        let out = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        let one = self.builder.imm(1);
        let digits = self.builder.shl(one, n);
        let prefix = self.builder.imm(if prefixed { 2 } else { 0 });
        let length = self.builder.add(digits, prefix);
        self.builder.set_memory_object_len(out, length, MemoryObjectKind::Bytes);

        // x = (mload(data + n) & (2**(8n) - 1)) << 8 * (2 - n)
        let source = self.builder.cast(data, MirType::MemPtr);
        let end = self.builder.add(source, n);
        let word = self.builder.mload(end);
        let three = self.builder.imm(3);
        let bits = self.builder.shl(three, n);
        let all = self.builder.imm(U256::MAX);
        let high = self.builder.shl(bits, all);
        let keep = self.builder.not(high);
        let word = self.builder.and(word, keep);
        let two = self.builder.imm(2);
        let missing = self.builder.sub(two, n);
        let missing_bits = self.builder.shl(three, missing);
        let x = self.builder.shl(missing_bits, word);

        // x = (x | x << 8) & 0x00ff00ff; x = (x | x << 4) & 0x0f0f0f0f
        // digits = x + 0x30303030 + 39 * (((x + 0x06060606) >> 4) & 0x01010101)
        let mut x = x;
        for (shift, mask) in [(8u64, 0x00ff_00ff_u64), (4, 0x0f0f_0f0f)] {
            let shift = self.builder.imm(shift);
            let shifted = self.builder.shl(shift, x);
            let combined = self.builder.or(x, shifted);
            let mask = self.builder.imm(mask);
            x = self.builder.and(combined, mask);
        }
        let six = self.builder.imm(0x0606_0606_u64);
        let marked = self.builder.add(x, six);
        let four = self.builder.imm(4);
        let marked = self.builder.shr(four, marked);
        let spread = self.builder.imm(0x0101_0101_u64);
        let letters = self.builder.and(marked, spread);
        let ascii = self.builder.imm(0x3030_3030_u64);
        let chars = self.builder.add(x, ascii);
        let thirty_nine = self.builder.imm(39);
        let letters = self.builder.mul(letters, thirty_nine);
        let chars = self.builder.add(chars, letters);

        // word = (prefix ? "0x" << 240 : 0) | chars << (224 - 8 prefix), first 2n digits kept
        let shift = self.builder.imm(if prefixed { 208 } else { 224 });
        let chars = self.builder.shl(shift, chars);
        let length_bits = self.builder.shl(three, length);
        let dropped = self.builder.shr(length_bits, all);
        let kept = self.builder.not(dropped);
        let mut word = self.builder.and(chars, kept);
        if prefixed {
            let prefix = self.builder.imm(U256::from(0x3078) << 240);
            word = self.builder.or(word, prefix);
        }
        let zero = self.builder.imm(0);
        self.builder.memory_object_store_word(out, zero, word);
        out
    }

    /// Encodes at most sixteen bytes. The output fits two words, so one
    /// fixed-size allocation holds it without length checks.
    fn lower_core_hex_encode_short(
        &mut self,
        data: ValueId,
        n: ValueId,
        prefixed: bool,
    ) -> ValueId {
        // out = alloc(96)
        // out.length = 2n + prefix
        let size = self.builder.imm(96);
        let out = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        let one = self.builder.imm(1);
        let digits = self.builder.shl(one, n);
        let prefix = self.builder.imm(if prefixed { 2 } else { 0 });
        let length = self.builder.add(digits, prefix);
        self.builder.set_memory_object_len(out, length, MemoryObjectKind::Bytes);
        let destination = self.builder.memory_object_data(out, MemoryObjectKind::Bytes);
        let start = self.builder.add(destination, prefix);

        // x = (mload(data + n) & (2**(8n) - 1)) << 8 * (16 - n)
        // word = hex_word(x) & ~(MAX >> 16n)
        let source = self.builder.cast(data, MirType::MemPtr);
        let end = self.builder.add(source, n);
        let word = self.builder.mload(end);
        let three = self.builder.imm(3);
        let bits = self.builder.shl(three, n);
        let all = self.builder.imm(U256::MAX);
        let high = self.builder.shl(bits, all);
        let keep = self.builder.not(high);
        let word = self.builder.and(word, keep);
        let sixteen = self.builder.imm(16);
        let missing = self.builder.sub(sixteen, n);
        let missing_bits = self.builder.shl(three, missing);
        let word = self.builder.shl(missing_bits, word);
        let word = hex_word(&mut self.builder, word);
        let four = self.builder.imm(4);
        let digit_bits = self.builder.shl(four, n);
        let dropped = self.builder.shr(digit_bits, all);
        let kept = self.builder.not(dropped);
        let word = self.builder.and(word, kept);

        // A prefixed 16-byte input spells 34 bytes; clear the rest of their
        // second word before the digits overwrite its first two bytes.
        // mstore(data + 32, 0) when prefixed
        // mstore(start, word)
        if prefixed {
            let second = self.builder.add_u64_offset(destination, 32);
            let zero = self.builder.imm(0);
            self.builder.mstore(second, zero);
        }
        self.builder.mstore(start, word);
        if prefixed {
            self.store_core_hex_prefix(out, length);
        }
        out
    }

    /// Encodes more than sixteen bytes, sixteen bytes per word.
    fn lower_core_hex_encode_long(&mut self, data: ValueId, n: ValueId, prefixed: bool) -> ValueId {
        // The prefixed body allocates `n * 2 + 2` bytes at once.
        let prefix = if prefixed { 2 } else { 0 };
        let (out, length, start, digits) = self.alloc_core_hex_output(n, prefix, true);
        let source = self.builder.cast(data, MirType::MemPtr);
        let sixteen = self.builder.imm(16);
        let thirty_two = self.builder.imm(32);
        let low_mask = self.builder.imm(U256::MAX >> 128);

        // for (cursor = data + 16, output = start; cursor != data + 16 + (n & ~15); ...)
        //     mstore(output, hex_word(mload(cursor) & (2**128 - 1)))
        let first = self.builder.add(source, sixteen);
        let fifteen = self.builder.imm(15);
        let whole_mask = self.builder.not(fifteen);
        let whole = self.builder.and(n, whole_mask);
        let stop = self.builder.add(first, whole);
        let entry = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let after = self.builder.create_block();
        self.builder.jump(header);

        self.builder.switch_to_block(header);
        let cursor = self.builder.phi(vec![(entry, first)]);
        let output = self.builder.phi(vec![(entry, start)]);
        let finished = self.builder.eq(cursor, stop);
        self.builder.branch(finished, after, body);

        self.builder.switch_to_block(body);
        let word = self.builder.mload(cursor);
        let word = self.builder.and(word, low_mask);
        let word = hex_word(&mut self.builder, word);
        self.builder.mstore(output, word);
        let next_cursor = self.builder.add(cursor, sixteen);
        let next_output = self.builder.add(output, thirty_two);
        self.builder.jump(header);
        self.builder.add_phi_incoming(cursor, body, next_cursor);
        self.builder.add_phi_incoming(output, body, next_output);

        // rest = n & 15
        // if rest != 0:
        //     x = (mload(data + n) & (2**(8 rest) - 1)) << 8 * (16 - rest)
        //     mstore(output, hex_word(x))
        self.builder.switch_to_block(after);
        let partial = self.builder.create_block();
        let finish = self.builder.create_block();
        let rest = self.builder.and(n, fifteen);
        let has_rest = self.builder.ne_zero(rest);
        self.builder.branch(has_rest, partial, finish);

        self.builder.switch_to_block(partial);
        let end = self.builder.add(source, n);
        let word = self.builder.mload(end);
        let three = self.builder.imm(3);
        let rest_bits = self.builder.shl(three, rest);
        let all = self.builder.imm(U256::MAX);
        let high = self.builder.shl(rest_bits, all);
        let keep = self.builder.not(high);
        let word = self.builder.and(word, keep);
        let missing = self.builder.sub(sixteen, rest);
        let missing_bits = self.builder.shl(three, missing);
        let word = self.builder.shl(missing_bits, word);
        let word = hex_word(&mut self.builder, word);
        self.builder.mstore(output, word);
        self.builder.jump(finish);

        // Clear the partial chunk's artificial digits and the ABI padding.
        // mstore(start + digits, 0)
        self.builder.switch_to_block(finish);
        let end = self.builder.add(start, digits);
        let zero = self.builder.imm(0);
        self.builder.mstore(end, zero);
        if prefixed {
            self.store_core_hex_prefix(out, length);
        }
        out
    }
}

/// The 32 lowercase digits of the sixteen bytes in `x`, which must be below
/// `2**128`, most significant first.
pub(super) fn hex_word(builder: &mut FunctionBuilder<'_>, x: ValueId) -> ValueId {
    // Spread the 32 nibbles one to a byte, halving the packing each step.
    // x = (x | x << s) & mask for s = 64, 32, 16, 8, 4
    let mut x = x;
    for (shift, mask) in [
        (64, "0000000000000000ffffffffffffffff0000000000000000ffffffffffffffff"),
        (32, "00000000ffffffff00000000ffffffff00000000ffffffff00000000ffffffff"),
        (16, "0000ffff0000ffff0000ffff0000ffff0000ffff0000ffff0000ffff0000ffff"),
        (8, "00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff"),
        (4, "0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f"),
    ] {
        let shift = builder.imm(shift);
        let shifted = builder.shl(shift, x);
        let combined = builder.or(x, shifted);
        let mask = builder.imm(U256::from_str_radix(mask, 16).unwrap());
        x = builder.and(combined, mask);
    }
    // Adding six carries every nibble above nine into bit four of its byte,
    // which marks the ones that become letters rather than digits.
    // letters = ((x + 0x06..06) >> 4) & 0x01..01
    // result = x + 0x30..30 + letters * 39
    let ones = U256::MAX / U256::from(255);
    let six = builder.imm(ones * U256::from(6));
    let marked = builder.add(x, six);
    let four = builder.imm(4);
    let marked = builder.shr(four, marked);
    let spread = builder.imm(ones);
    let letters = builder.and(marked, spread);
    let ascii = builder.imm(ones * U256::from(b'0'));
    let digits = builder.add(x, ascii);
    let thirty_nine = builder.imm(39);
    let letters = builder.mul(letters, thirty_nine);
    builder.add(digits, letters)
}

/// Reverts with the four-byte custom error `selector` when `condition` holds.
fn revert_with_selector(builder: &mut FunctionBuilder<'_>, condition: ValueId, selector: u64) {
    let fail = builder.create_block();
    let success = builder.create_block();
    builder.branch(condition, fail, success);
    builder.switch_to_block(fail);
    let selector = builder.imm(selector);
    let zero = builder.imm(0);
    builder.mstore(zero, selector);
    let offset = builder.imm(28);
    let length = builder.imm(4);
    builder.revert(offset, length);
    builder.switch_to_block(success);
}
