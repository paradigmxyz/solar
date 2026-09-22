//! Base64 codec lowering for the compiler-owned core module.
//!
//! Encoding maps 24 input bytes to 32 ASCII lanes together, then handles
//! remaining three-byte groups and a one- or two-byte tail. Decoding validates
//! and converts 32 ASCII lanes together, then uses an owned lookup table for
//! remaining groups with an invalid-character sentinel checked before return.
//! Length arithmetic is checked once; loop bounds prove every payload access.
//! A load ending at the last input byte avoids both dirty padding and reads
//! beyond the input allocation. Tail groups read only their one or two bytes.
//! The lookup table has its own allocation: scratch, the zero slot, the input,
//! and unrelated objects are never used as temporary storage. The portable
//! checked Solidity body remains the reference under `-Zno-core-intrinsics`.

use super::*;

impl FunctionLowerer<'_, '_> {
    pub(super) fn lower_core_base64_encode(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let (&input, options) = operands.split_first()?;
        let file_safe = options.first().copied().unwrap_or_else(|| self.builder.imm_bool(false));
        let no_padding = options.get(1).copied().unwrap_or_else(|| self.builder.imm_bool(false));
        let n = self.builder.memory_object_len(input, MemoryObjectKind::Bytes);
        let zero = self.builder.imm(0);
        let one = self.builder.imm(1);
        let two = self.builder.imm(2);
        let three = self.builder.imm(3);
        let four = self.builder.imm(4);
        let rounded = self.builder.add(n, two);
        let overflow = self.builder.lt(rounded, n);
        self.builder.panic_if(overflow, PanicCode::ArithmeticOverflowUnderflow);
        let rounded_groups = self.builder.div(rounded, three);
        let padded = self.builder.mul(rounded_groups, four);
        let recovered = self.builder.div(padded, four);
        let overflow = self.builder.ne(recovered, rounded_groups);
        self.builder.panic_if(overflow, PanicCode::ArithmeticOverflowUnderflow);
        let remainder = self.builder.mod_(n, three);
        let missing = self.builder.sub(three, remainder);
        let has_tail = self.builder.ne_zero(remainder);
        let padding = self.builder.select(has_tail, missing, zero);
        let padding = self.builder.select(no_padding, padding, zero);
        let length = self.builder.sub(padded, padding);
        let out = self.builder.alloc_bytes_object(length, AllocationSemantics::SOLIDITY_ZEROED);
        let work = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.branch(n, work, done);
        self.builder.switch_to_block(work);

        let source = self.builder.cast(input, MirType::MemPtr);
        let destination = self.builder.memory_object_data(out, MemoryObjectKind::Bytes);
        let twenty_four = self.builder.imm(24);
        let chunks = self.builder.div(n, twenty_four);
        self.builder.counted_loop(chunks, |builder, chunk| {
            let offset = builder.mul(chunk, twenty_four);
            let end = builder.add(offset, twenty_four);
            let address = builder.add(source, end);
            let packed = builder.mload(address);
            let encoded = encode_word(builder, packed, file_safe);
            let five = builder.imm(5);
            let offset = builder.shl(five, chunk);
            builder.memory_object_store_word(out, offset, encoded);
        });
        let consumed = self.builder.mul(chunks, twenty_four);
        let more = self.builder.lt(consumed, n);
        let short = self.builder.create_block();
        self.builder.branch(more, short, done);
        self.builder.switch_to_block(short);

        let table_len = self.builder.imm(64);
        let table = self.builder.alloc_bytes_object(table_len, AllocationSemantics::INTERNAL);
        let first = self.builder.imm(U256::from_be_slice(b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef"));
        let standard = self.builder.imm(U256::from_be_slice(b"ghijklmnopqrstuvwxyz0123456789+/"));
        let url = self.builder.imm(U256::from_be_slice(b"ghijklmnopqrstuvwxyz0123456789-_"));
        let second = self.builder.select(file_safe, url, standard);
        self.builder.memory_object_store_word(table, zero, first);
        let offset = self.builder.imm(32);
        self.builder.memory_object_store_word(table, offset, second);
        // mload(table + 1 + sextet) puts the chosen byte in the low eight bits.
        // The first 31 bytes of the load may include the table's length word.
        let table_ptr = self.builder.cast(table, MirType::MemPtr);
        let lookup = self.builder.add(table_ptr, one);
        let groups = self.builder.div(n, three);
        let eight = self.builder.imm(8);
        let wide_groups = self.builder.mul(chunks, eight);
        let short_groups = self.builder.sub(groups, wide_groups);
        self.builder.counted_loop(short_groups, |builder, group| {
            let group = builder.add(group, wide_groups);
            let input_offset = builder.mul(group, three);
            let end = builder.add(input_offset, three);
            let address = builder.add(source, end);
            let word = builder.mload(address);
            let output_offset = builder.mul(group, four);
            let dest = builder.add(destination, output_offset);
            encode_group(builder, word, lookup, dest);
        });

        let tail = self.builder.create_block();
        self.builder.branch(has_tail, tail, done);
        self.builder.switch_to_block(tail);
        let end = self.builder.add(source, n);
        let word = self.builder.mload(end);
        // Keep exactly the remaining input bytes, then right-pad to 24 bits.
        let eight = self.builder.imm(8);
        let bits = self.builder.mul(remainder, eight);
        let all = self.builder.imm(U256::MAX);
        let high = self.builder.shl(bits, all);
        let mask = self.builder.not(high);
        let word = self.builder.and(word, mask);
        let shift = self.builder.mul(missing, eight);
        let word = self.builder.shl(shift, word);
        let output_offset = self.builder.mul(groups, four);
        let dest = self.builder.add(destination, output_offset);
        for (index, shift) in [(0, 18), (1, 12)] {
            encode_character(&mut self.builder, word, lookup, dest, index, shift);
        }
        let third = self.builder.create_block();
        let padding = self.builder.create_block();
        let two_bytes = self.builder.eq(remainder, two);
        self.builder.branch(two_bytes, third, padding);
        self.builder.switch_to_block(third);
        encode_character(&mut self.builder, word, lookup, dest, 2, 6);
        self.builder.jump(padding);
        self.builder.switch_to_block(padding);
        let add_padding = self.builder.create_block();
        self.builder.branch(no_padding, done, add_padding);
        self.builder.switch_to_block(add_padding);
        let equals = self.builder.imm(0x3d);
        let last = self.builder.add(dest, three);
        self.builder.mstore8(last, equals);
        let second_padding = self.builder.create_block();
        self.builder.branch(two_bytes, done, second_padding);
        self.builder.switch_to_block(second_padding);
        let previous = self.builder.add(dest, two);
        self.builder.mstore8(previous, equals);
        self.builder.jump(done);
        self.builder.switch_to_block(done);
        Some(out)
    }
}

fn encode_group(builder: &mut FunctionBuilder<'_>, word: ValueId, table: ValueId, dest: ValueId) {
    for (index, shift) in [18, 12, 6, 0].into_iter().enumerate() {
        encode_character(builder, word, table, dest, index as u64, shift);
    }
}

fn encode_character(
    builder: &mut FunctionBuilder<'_>,
    word: ValueId,
    table: ValueId,
    dest: ValueId,
    index: u64,
    shift: u64,
) {
    let shift = builder.imm(shift);
    let shifted = builder.shr(shift, word);
    let mask = builder.imm(63);
    let sextet = builder.and(shifted, mask);
    let address = builder.add(table, sextet);
    let character = builder.mload(address);
    let address = builder.add_u64_offset(dest, index);
    builder.mstore8(address, character);
}

/// Spread eight 24-bit groups into 32 byte lanes, then map all sextets to
/// ASCII together. Every lane stays below 256, so none of these additions or
/// subtractions carries or borrows into its neighbour.
fn encode_word(builder: &mut FunctionBuilder<'_>, packed: ValueId, file_safe: ValueId) -> ValueId {
    let mask = builder.imm(U256::MAX >> 64);
    let packed = builder.and(packed, mask);
    let shift = builder.imm(96);
    let upper = builder.shr(shift, packed);
    let shift = builder.imm(128);
    let upper = builder.shl(shift, upper);
    let mask = builder.imm(U256::MAX >> 160);
    let lower = builder.and(packed, mask);
    let mut x = builder.or(upper, lower);
    for (shift, high, low) in [
        (
            16,
            "00000000ffffffffffff00000000000000000000ffffffffffff000000000000",
            "00000000000000000000ffffffffffff00000000000000000000ffffffffffff",
        ),
        (
            8,
            "0000ffffff0000000000ffffff0000000000ffffff0000000000ffffff000000",
            "0000000000ffffff0000000000ffffff0000000000ffffff0000000000ffffff",
        ),
    ] {
        let high = builder.imm(U256::from_str_radix(high, 16).unwrap());
        let low = builder.imm(U256::from_str_radix(low, 16).unwrap());
        let upper = builder.and(x, high);
        let lower = builder.and(x, low);
        let shift = builder.imm(shift);
        let upper = builder.shl(shift, upper);
        x = builder.or(upper, lower);
    }
    let mask = builder.imm(
        U256::from_str_radix(
            "0000003f0000003f0000003f0000003f0000003f0000003f0000003f0000003f",
            16,
        )
        .unwrap(),
    );
    let mut v = builder.imm(0);
    for (right, left) in [(18, 24), (12, 16), (6, 8), (0, 0)] {
        let right = builder.imm(right);
        let lane = builder.shr(right, x);
        let lane = builder.and(lane, mask);
        let left = builder.imm(left);
        let lane = builder.shl(left, lane);
        v = builder.or(v, lane);
    }
    let ones = U256::MAX / U256::from(255);
    let spread = builder.imm(ones);
    let seven = builder.imm(7);
    let mut steps = Vec::new();
    for threshold in [26, 52, 62, 63] {
        let delta = builder.imm(ones * U256::from(128 - threshold));
        let marked = builder.add(v, delta);
        let marked = builder.shr(seven, marked);
        steps.push(builder.and(marked, spread));
    }
    let ascii = builder.imm(ones * U256::from(65));
    let mut result = builder.add(v, ascii);
    let six = builder.imm(6);
    let letters = builder.mul(steps[0], six);
    result = builder.add(result, letters);
    let seventy_five = builder.imm(75);
    let digits = builder.mul(steps[1], seventy_five);
    result = builder.sub(result, digits);
    let thirteen = builder.imm(13);
    let fifteen = builder.imm(15);
    let down = builder.select(file_safe, thirteen, fifteen);
    let forty_nine = builder.imm(49);
    let three = builder.imm(3);
    let up = builder.select(file_safe, forty_nine, three);
    let last2 = builder.mul(steps[2], down);
    result = builder.sub(result, last2);
    let last1 = builder.mul(steps[3], up);
    builder.add(result, last1)
}

impl FunctionLowerer<'_, '_> {
    pub(super) fn lower_core_base64_decode(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let (&input, options) = operands.split_first()?;
        let imap = options.first().copied().unwrap_or_else(|| self.builder.imm_bool(false));
        let raw_length = self.builder.memory_object_len(input, MemoryObjectKind::Bytes);
        let source = self.builder.cast(input, MirType::MemPtr);
        let zero = self.builder.imm(0);
        let one = self.builder.imm(1);
        let three = self.builder.imm(3);
        let four = self.builder.imm(4);
        let equals = self.builder.imm(0x3d);
        let byte_mask = self.builder.imm(255);
        // Empty and one-byte inputs use the length word instead of reading
        // before the allocation when examining optional trailing padding.
        let end = self.builder.add(source, raw_length);
        let last = self.builder.mload(end);
        let last = self.builder.and(last, byte_mask);
        let padded = self.builder.eq(last, equals);
        let raw_tail = self.builder.and(raw_length, three);
        let multiple_four = self.builder.eq_zero(raw_tail);
        let padded = self.builder.and(padded, multiple_four);
        let has_input = self.builder.ne_zero(raw_length);
        let padded = self.builder.and(padded, has_input);
        let previous = self.builder.sub(raw_length, one);
        let enough = self.builder.gt(raw_length, one);
        let previous = self.builder.select(enough, previous, zero);
        let previous = self.builder.add(source, previous);
        let previous = self.builder.mload(previous);
        let previous = self.builder.and(previous, byte_mask);
        let second_pad = self.builder.eq(previous, equals);
        let second_pad = self.builder.cast(second_pad, MirType::I256);
        let padding = self.builder.add(second_pad, one);
        let padding = self.builder.select(padded, padding, zero);
        let n = self.builder.sub(raw_length, padding);
        let tail = self.builder.and(n, three);
        let invalid = self.builder.eq(tail, one);
        reject_invalid(&mut self.builder, invalid);
        let groups = self.builder.div(n, four);
        let full_bytes = self.builder.mul(groups, three);
        let has_tail = self.builder.ne_zero(tail);
        let tail_bytes = self.builder.sub(tail, one);
        let tail_bytes = self.builder.select(has_tail, tail_bytes, zero);
        let length = self.builder.add(full_bytes, tail_bytes);
        let out = self.builder.alloc_bytes_object(length, AllocationSemantics::SOLIDITY_ZEROED);
        let work = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.branch(n, work, done);
        self.builder.switch_to_block(work);
        let destination = self.builder.memory_object_data(out, MemoryObjectKind::Bytes);
        let thirty_two = self.builder.imm(32);
        let twenty_four = self.builder.imm(24);
        let chunks = self.builder.div(n, thirty_two);
        self.builder.counted_loop(chunks, |builder, index| {
            let offset = builder.mul(index, thirty_two);
            let end = builder.add(offset, thirty_two);
            let address = builder.add(source, end);
            let word = builder.mload(address);
            let packed = decode_word(builder, word, imap);
            let offset = builder.mul(index, twenty_four);
            let dest = builder.add(destination, offset);
            // Store 24 bytes ending at dest + 24. The preceding eight bytes
            // belong to this output (its length word on the first iteration)
            // and are preserved. No store extends past the output allocation.
            let eight = builder.imm(8);
            let address = builder.sub(dest, eight);
            let previous = builder.mload(address);
            let high = builder.imm(U256::MAX << 192);
            let previous = builder.and(previous, high);
            let packed = builder.or(previous, packed);
            builder.mstore(address, packed);
        });
        let eight = self.builder.imm(8);
        let wide_groups = self.builder.mul(chunks, eight);
        let consumed = self.builder.mul(chunks, thirty_two);
        let more = self.builder.lt(consumed, n);
        let short = self.builder.create_block();
        self.builder.branch(more, short, done);
        self.builder.switch_to_block(short);
        let table_len = self.builder.imm(256);
        let table = self.builder.alloc_bytes_object(table_len, AllocationSemantics::INTERNAL);
        let mut alphabet = [255u8; 256];
        for (index, byte) in
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/".iter().enumerate()
        {
            alphabet[usize::from(*byte)] = index as u8;
        }
        alphabet[usize::from(b'-')] = 62;
        alphabet[usize::from(b'_')] = 63;
        for (index, chunk) in alphabet.chunks_exact(32).enumerate() {
            let value = self.builder.imm(U256::from_be_slice(chunk));
            let value = if index == 1 {
                let mut patched = [0; 32];
                patched.copy_from_slice(chunk);
                patched[usize::from(b',') - 32] = 63;
                let patched = self.builder.imm(U256::from_be_slice(&patched));
                self.builder.select(imap, patched, value)
            } else {
                value
            };
            let offset = self.builder.imm(index * 32);
            self.builder.memory_object_store_word(table, offset, value);
        }
        let lookup = self.builder.cast(table, MirType::MemPtr);
        let lookup = self.builder.add(lookup, one);
        let destination = self.builder.memory_object_data(out, MemoryObjectKind::Bytes);

        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);
        self.builder.switch_to_block(header);
        let index = self.builder.phi(vec![(preheader, wide_groups)]);
        let seen = self.builder.phi(vec![(preheader, zero)]);
        let more = self.builder.lt(index, groups);
        self.builder.branch(more, body, exit);
        self.builder.switch_to_block(body);
        let offset = self.builder.mul(index, four);
        let offset = self.builder.add(offset, four);
        let address = self.builder.add(source, offset);
        let quad = self.builder.mload(address);
        let mut word = zero;
        let mut next_seen = seen;
        for (input_shift, output_shift) in [(24, 18), (16, 12), (8, 6), (0, 0)] {
            let value = decode_sextet(&mut self.builder, quad, input_shift, lookup);
            next_seen = self.builder.or(next_seen, value);
            let shift = self.builder.imm(output_shift);
            let shifted = self.builder.shl(shift, value);
            word = self.builder.or(word, shifted);
        }
        let offset = self.builder.mul(index, three);
        let dest = self.builder.add(destination, offset);
        for (index, shift) in [16, 8, 0].into_iter().enumerate() {
            let shift = self.builder.imm(shift);
            let byte = self.builder.shr(shift, word);
            let address = self.builder.add_u64_offset(dest, index as u64);
            self.builder.mstore8(address, byte);
        }
        let next = self.builder.add(index, one);
        let backedge = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, backedge, next);
        self.builder.add_phi_incoming(seen, backedge, next_seen);
        self.builder.switch_to_block(exit);
        let invalid_bit = self.builder.imm(128);
        let invalid = self.builder.and(seen, invalid_bit);
        reject_invalid(&mut self.builder, invalid);

        let partial = self.builder.create_block();
        self.builder.branch(has_tail, partial, done);
        self.builder.switch_to_block(partial);
        let end = self.builder.add(source, n);
        let quad = self.builder.mload(end);
        let missing = self.builder.sub(four, tail);
        let eight = self.builder.imm(8);
        let shift = self.builder.mul(missing, eight);
        let quad = self.builder.shl(shift, quad);
        let a = decode_sextet(&mut self.builder, quad, 24, lookup);
        let b = decode_sextet(&mut self.builder, quad, 16, lookup);
        let c = decode_sextet(&mut self.builder, quad, 8, lookup);
        let third = self.builder.eq(tail, three);
        let c = self.builder.select(third, c, zero);
        let seen = self.builder.or(a, b);
        let seen = self.builder.or(seen, c);
        let invalid = self.builder.and(seen, invalid_bit);
        reject_invalid(&mut self.builder, invalid);
        let eighteen = self.builder.imm(18);
        let twelve = self.builder.imm(12);
        let six = self.builder.imm(6);
        let a = self.builder.shl(eighteen, a);
        let b = self.builder.shl(twelve, b);
        let c = self.builder.shl(six, c);
        let word = self.builder.or(a, b);
        let word = self.builder.or(word, c);
        let dest = self.builder.add(destination, full_bytes);
        let sixteen = self.builder.imm(16);
        let byte = self.builder.shr(sixteen, word);
        self.builder.mstore8(dest, byte);
        let second = self.builder.create_block();
        self.builder.branch(third, second, done);
        self.builder.switch_to_block(second);
        let dest = self.builder.add(dest, one);
        let byte = self.builder.shr(eight, word);
        self.builder.mstore8(dest, byte);
        self.builder.jump(done);
        self.builder.switch_to_block(done);
        Some(out)
    }
}

fn decode_sextet(
    builder: &mut FunctionBuilder<'_>,
    word: ValueId,
    shift: u64,
    table: ValueId,
) -> ValueId {
    let shift = builder.imm(shift);
    let shifted = builder.shr(shift, word);
    let mask = builder.imm(255);
    let byte = builder.and(shifted, mask);
    let address = builder.add(table, byte);
    let value = builder.mload(address);
    builder.and(value, mask)
}

/// Invalid input is rejected with the portable module's exact custom error.
fn reject_invalid(builder: &mut FunctionBuilder<'_>, invalid: ValueId) {
    let fail = builder.create_block();
    let success = builder.create_block();
    builder.branch(invalid, fail, success);
    builder.switch_to_block(fail);
    let selector = builder.imm(0xa164_f8fe_u64);
    let zero = builder.imm(0);
    builder.mstore(zero, selector);
    let offset = builder.imm(28);
    let length = builder.imm(4);
    builder.revert(offset, length);
    builder.switch_to_block(success);
}

/// Validate and decode 32 ASCII bytes in parallel. Lane arithmetic first
/// masks the high bit, so threshold additions cannot carry between bytes.
/// Invalid lanes are rejected before packing or writing the result.
fn decode_word(builder: &mut FunctionBuilder<'_>, word: ValueId, imap: ValueId) -> ValueId {
    let ones = U256::MAX / U256::from(255);
    let mask = builder.imm(ones * U256::from(127));
    let c = builder.and(word, mask);
    let upper = decode_range(builder, c, 65, 91);
    let lower = decode_range(builder, c, 97, 123);
    let digits = decode_range(builder, c, 48, 58);
    let plus = decode_range(builder, c, 43, 44);
    let minus = decode_range(builder, c, 45, 46);
    let slash = decode_range(builder, c, 47, 48);
    let underscore = decode_range(builder, c, 95, 96);
    let comma = decode_range(builder, c, 44, 45);
    let zero = builder.imm(0);
    let comma = builder.select(imap, comma, zero);
    let mut valid = builder.or(upper, lower);
    for lane in [digits, plus, minus, slash, underscore, comma] {
        valid = builder.or(valid, lane);
    }
    let all = builder.imm(ones);
    let invalid = builder.ne(valid, all);
    let high = builder.imm(ones * U256::from(128));
    let high = builder.and(word, high);
    let high = builder.ne_zero(high);
    let invalid = builder.or(invalid, high);
    reject_invalid(builder, invalid);
    let four = builder.imm(ones * U256::from(4));
    let mut v = builder.add(c, four);
    for (lane, delta, subtract) in [
        (upper, 69, true),
        (lower, 75, true),
        (plus, 15, false),
        (minus, 13, false),
        (slash, 12, false),
        (underscore, 36, true),
        (comma, 15, false),
    ] {
        let delta = builder.imm(delta);
        let adjustment = builder.mul(lane, delta);
        v = if subtract { builder.sub(v, adjustment) } else { builder.add(v, adjustment) };
    }
    let mask = builder.imm(
        U256::from_str_radix(
            "0000003f0000003f0000003f0000003f0000003f0000003f0000003f0000003f",
            16,
        )
        .unwrap(),
    );
    let mut packed = zero;
    for (right, left) in [(24, 18), (16, 12), (8, 6), (0, 0)] {
        let right = builder.imm(right);
        let lane = builder.shr(right, v);
        let lane = builder.and(lane, mask);
        let left = builder.imm(left);
        let lane = builder.shl(left, lane);
        packed = builder.or(packed, lane);
    }
    // Invert the encoder's lane spreading, joining 24-, 48-, then 96-bit groups.
    for (shift, high, low) in [
        (
            8,
            "0000ffffff0000000000ffffff0000000000ffffff0000000000ffffff000000",
            "0000000000ffffff0000000000ffffff0000000000ffffff0000000000ffffff",
        ),
        (
            16,
            "00000000ffffffffffff00000000000000000000ffffffffffff000000000000",
            "00000000000000000000ffffffffffff00000000000000000000ffffffffffff",
        ),
    ] {
        let high = builder.imm(U256::from_str_radix(high, 16).unwrap());
        let low = builder.imm(U256::from_str_radix(low, 16).unwrap());
        let shift = builder.imm(shift);
        let upper = builder.shr(shift, packed);
        let upper = builder.and(upper, high);
        let lower = builder.and(packed, low);
        packed = builder.or(upper, lower);
    }
    let shift = builder.imm(128);
    let upper = builder.shr(shift, packed);
    let shift = builder.imm(96);
    let upper = builder.shl(shift, upper);
    let mask = builder.imm(U256::MAX >> 160);
    let lower = builder.and(packed, mask);
    builder.or(upper, lower)
}

fn decode_range(builder: &mut FunctionBuilder<'_>, value: ValueId, low: u64, high: u64) -> ValueId {
    let ones = U256::MAX / U256::from(255);
    let mask = builder.imm(ones);
    let seven = builder.imm(7);
    let mut ge = |threshold| {
        let delta = builder.imm(ones * U256::from(128 - threshold));
        let marked = builder.add(value, delta);
        let marked = builder.shr(seven, marked);
        builder.and(marked, mask)
    };
    let low = ge(low);
    let high = ge(high);
    builder.sub(low, high)
}
