//! Base64 codec lowering for the compiler-owned core module.
//!
//! Both directions are one shared function per module: the encoder takes its
//! alphabet and padding flags as arguments, and the decoder has one body per
//! IMAP mode, so a constant mode folds into it. Empty inputs return a fresh
//! empty object at once, so a terminal return can still encode the result in
//! place.
//!
//! Encoding picks its shape by length. Up to six bytes, one or two groups,
//! the 64-character alphabet is written at the output allocation and read one
//! byte early, so each sextet's character is the low byte of its load;
//! `mstore8` gathers the characters in scratch and they load back as one
//! word. Up to 24 bytes, the word kernel runs once without a loop: it spreads
//! eight groups into 32 sextet lanes and maps them all to ASCII in parallel.
//! Longer inputs call the shared wide encoder, which loops the kernel over
//! loads that end at the next 24 input bytes and clears the bytes past the
//! input. In every shape the characters past the input's groups become '='
//! padding, unless it is omitted, and then zero.
//!
//! Decoding strips padding only when the raw length is a multiple of four,
//! with one load ending at the last character; a length of one modulo four is
//! rejected from the raw length alone. Up to sixteen characters decode as one
//! block of four, eight, twelve or sixteen lanes through a 128-byte table,
//! copied from code when that is cheaper, that starts at the output
//! allocation: each character's entry, its sextet shifted left by two or an
//! invalid mark, is the low byte of a load one byte early, and `mstore8`
//! gathers the entries in scratch. One mask then validates the input's lanes,
//! the high bit rejects bytes above 127, and neighbouring lanes are merged
//! pairwise into bytes, joined only as far as the block needs. Longer inputs
//! loop the word kernel, which validates and converts 32 ASCII lanes to 24
//! bytes; the last word's lanes past the input become 'A', sextet zero. The
//! kernel folds each character class into its running value and validity
//! mask as soon as it is known, so few lane masks are live and the loop keeps
//! its state on the stack.
//!
//! Each output owns an extra word of capacity, allowing whole-word stores at
//! the final position without touching another object. Logical length excludes
//! that capacity; stores initialize every returned byte and its ABI padding.
//! Encoded lengths are checked before allocation. A decoded length is at most
//! three quarters of the input length, so its arithmetic cannot overflow.
//! Outputs of one word use one fixed-size allocation, and their tables are
//! written there before the output, which replaces them after the last lookup.
//! Loads may read up to a word past the input or before a table, always inside
//! memory that is already expanded; those bytes are masked or replaced before
//! they are used. The first sixteen scratch bytes hold gathered lanes, the
//! input and the zero slot are never written, and error selectors use scratch.
//! Checked Solidity bodies remain the reference under `-Zno-core-intrinsics`.

use super::*;

/// Decoded inputs of at most this many characters, after padding, use the
/// table lanes. A word-kernel pass costs about as much as sixteen lookups.
const SHORT_DECODE_CHARS: u64 = 16;

/// Input bytes one word-kernel pass encodes: eight groups of three.
const ENCODE_WORD_BYTES: u64 = 24;

/// Bytes in the scalar decode table: one entry per 7-bit character.
const DECODE_TABLE_BYTES: u64 = 128;

/// A word with every byte set to one: multiplying a byte by it repeats the
/// byte in every lane.
const LANE_ONES: U256 = U256::from_limbs([0x0101_0101_0101_0101; 4]);

impl FunctionLowerer<'_, '_> {
    /// Keep the codec as one callable body per module. Expanding this large
    /// algorithm at every overload/call site defeats ordinary small inlining.
    pub(super) fn lower_core_base64_encode_call(
        &mut self,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        let &input = operands.first()?;
        let file_safe = operands.get(1).copied().unwrap_or_else(|| self.builder.imm_bool(false));
        let no_padding = operands.get(2).copied().unwrap_or_else(|| self.builder.imm_bool(false));
        let file_safe = self.builder.cast(file_safe, MirType::I1);
        let no_padding = self.builder.cast(no_padding, MirType::I1);
        let helper = self.lazy_helper(Symbol::intern("core_base64_encode"), |this, function| {
            function.attributes.no_inline = true;
            let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
            let ty = MirType::MemoryObject(MemoryObjectKind::Bytes);
            let input = lowerer.builder.add_param(ty);
            let file_safe = lowerer.builder.add_param(MirType::I1);
            let no_padding = lowerer.builder.add_param(MirType::I1);
            lowerer.builder.set_return_type(ty);
            lowerer.lower_core_base64_encode(input, file_safe, no_padding)
        })?;
        Some(self.builder.icall(
            helper,
            vec![input, file_safe, no_padding],
            MirType::MemoryObject(MemoryObjectKind::Bytes),
        ))
    }

    /// Encodes inputs of at most one kernel word here and forwards longer
    /// inputs to the wide encoder.
    fn lower_core_base64_encode(
        &mut self,
        input: ValueId,
        file_safe: ValueId,
        no_padding: ValueId,
    ) -> Option<()> {
        let ty = MirType::MemoryObject(MemoryObjectKind::Bytes);
        let wide_helper =
            self.lazy_helper(Symbol::intern("core_base64_encode_wide"), |this, function| {
                function.attributes.no_inline = true;
                let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
                let input = lowerer.builder.add_param(ty);
                let file_safe = lowerer.builder.add_param(MirType::I1);
                let no_padding = lowerer.builder.add_param(MirType::I1);
                lowerer.builder.set_return_type(ty);
                let out = lowerer.lower_core_base64_encode_wide(input, file_safe, no_padding);
                lowerer.builder.ret([out]);
                Some(())
            })?;
        let n = self.builder.memory_object_len(input, MemoryObjectKind::Bytes);
        let empty = self.builder.create_block();
        let nonempty = self.builder.create_block();
        let is_empty = self.builder.eq_zero(n);
        self.builder.branch(is_empty, empty, nonempty);

        self.builder.switch_to_block(empty);
        let empty_out = alloc_empty(&mut self.builder);
        self.builder.ret([empty_out]);

        self.builder.switch_to_block(nonempty);
        for groups in 1..=2 {
            // if n <= 3 * groups: ret encode_lanes(input, n, groups)
            let block = self.builder.create_block();
            let next = self.builder.create_block();
            let limit = self.builder.imm(3 * groups + 1);
            let fits = self.builder.lt(n, limit);
            self.builder.branch(fits, block, next);
            self.builder.switch_to_block(block);
            let out = encode_lanes(&mut self.builder, input, n, file_safe, no_padding, groups);
            self.builder.ret([out]);
            self.builder.switch_to_block(next);
        }
        let single_word = self.builder.create_block();
        let wide = self.builder.create_block();
        let limit = self.builder.imm(ENCODE_WORD_BYTES + 1);
        let fits_word = self.builder.lt(n, limit);
        self.builder.branch(fits_word, single_word, wide);

        self.builder.switch_to_block(single_word);
        let out = encode_single_word(&mut self.builder, input, n, file_safe, no_padding);
        self.builder.ret([out]);

        // ret icall core_base64_encode_wide(input, file_safe, no_padding)
        self.builder.switch_to_block(wide);
        let out = self.builder.icall(wide_helper, vec![input, file_safe, no_padding], ty);
        self.builder.ret([out]);
        Some(())
    }

    /// Encodes more than 24 bytes, one word-kernel pass per 24 input bytes.
    fn lower_core_base64_encode_wide(
        &mut self,
        input: ValueId,
        file_safe: ValueId,
        no_padding: ValueId,
    ) -> ValueId {
        // length = 4 * ceil(n / 3), minus the padding when it is omitted
        let n = self.builder.memory_object_len(input, MemoryObjectKind::Bytes);
        let two = self.builder.imm(2);
        let three = self.builder.imm(3);
        let four = self.builder.imm(4);
        let rounded = self.builder.add(n, two);
        let overflow = self.builder.lt(rounded, n);
        self.builder.panic_if(overflow, PanicCode::ArithmeticOverflowUnderflow);
        let groups = self.builder.div(rounded, three);
        let padded = self.builder.mul(groups, four);
        let recovered = self.builder.div(padded, four);
        let overflow = self.builder.ne(recovered, groups);
        self.builder.panic_if(overflow, PanicCode::ArithmeticOverflowUnderflow);
        let unpadded = self.builder.add(n, groups);
        let dropped = self.builder.sub(padded, unpadded);
        let no_padding_word = self.builder.cast(no_padding, MirType::I256);
        let dropped = self.builder.mul(no_padding_word, dropped);
        let length = self.builder.sub(padded, dropped);
        // Every iteration writes a whole output word. Own one extra word
        // so the last write, including zero padding, cannot touch a neighbour.
        let extra = self.builder.imm(32);
        let capacity = self.builder.checked_add(length, extra);
        let out =
            self.builder.alloc_bytes_object(capacity, AllocationSemantics::SOLIDITY_UNINITIALIZED);
        self.builder.set_memory_object_len(out, length, MemoryObjectKind::Bytes);
        let destination = self.builder.memory_object_data(out, MemoryObjectKind::Bytes);

        // Each load ends at the next 24 input bytes, which the kernel encodes;
        // bytes past the input are cleared.
        // cursor = input + 24; end = input + 32 + n
        // do:
        //     word = mload(cursor) & ~(MAX >> (64 + 8 * (end - cursor - 8)))
        //     output[0..32] = encode_word(word)
        //     cursor += 24; output += 32
        // while cursor + 8 < end
        let source = self.builder.cast(input, MirType::MemPtr);
        let twenty_four = self.builder.imm(24);
        let first = self.builder.add(source, twenty_four);
        let header = self.builder.imm(32);
        let data = self.builder.add(source, header);
        let end = self.builder.add(data, n);
        let entry = self.builder.current_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(body);

        self.builder.switch_to_block(body);
        let cursor = self.builder.phi(vec![(entry, first)]);
        let output = self.builder.phi(vec![(entry, destination)]);
        let word = self.builder.mload(cursor);
        let eight = self.builder.imm(8);
        let start = self.builder.add(cursor, eight);
        let remaining = self.builder.sub(end, start);
        let remaining_bits = self.builder.shl(three, remaining);
        let sixty_four = self.builder.imm(64);
        let past = self.builder.add(remaining_bits, sixty_four);
        let all = self.builder.imm(U256::MAX);
        let clear = self.builder.shr(past, all);
        let keep = self.builder.not(clear);
        let word = self.builder.and(word, keep);
        let encoded = encode_word(&mut self.builder, word, file_safe);
        self.builder.mstore(output, encoded);
        let next_cursor = self.builder.add(cursor, twenty_four);
        let thirty_two = self.builder.imm(32);
        let next_output = self.builder.add(output, thirty_two);
        let next_start = self.builder.add(next_cursor, eight);
        let more = self.builder.lt(next_start, end);
        let latch = self.builder.current_block();
        self.builder.branch(more, body, exit);
        self.builder.add_phi_incoming(cursor, latch, next_cursor);
        self.builder.add_phi_incoming(output, latch, next_output);

        // The last word holds the characters past the input: '=' up to the
        // padded length unless padding is omitted, then zero.
        // rest = MAX >> 8 * (destination + unpadded - output)
        // padding = no_padding ? 0 : "====..." & rest & ~(MAX >> 8 * (destination + padded -
        // output)) output[0..32] = mload(output) & ~rest | padding
        self.builder.switch_to_block(exit);
        let last = self.builder.mload(output);
        let unpadded_end = self.builder.add(destination, unpadded);
        let kept = self.builder.sub(unpadded_end, output);
        let kept_bits = self.builder.shl(three, kept);
        let rest = self.builder.shr(kept_bits, all);
        let keep = self.builder.not(rest);
        let last = self.builder.and(last, keep);
        let padded_end = self.builder.add(destination, padded);
        let chars = self.builder.sub(padded_end, output);
        let chars_bits = self.builder.shl(three, chars);
        let beyond = self.builder.shr(chars_bits, all);
        let padding_lanes = self.builder.xor(rest, beyond);
        let equals = self.builder.imm(LANE_ONES * U256::from(b'='));
        let padding = self.builder.and(equals, padding_lanes);
        let padding = flag_clear(&mut self.builder, no_padding, padding);
        let last = self.builder.or(last, padding);
        self.builder.mstore(output, last);
        out
    }
}

/// Returns a fresh empty object. The terminal return encoder may then write
/// its ABI head over the object, which it may not do to the zero slot.
fn alloc_empty(builder: &mut FunctionBuilder<'_>) -> ValueId {
    // out = alloc(32)
    // out.length = 0
    let size = builder.imm(32);
    let out = builder.alloc_object(size, MemoryObjectLayout::Bytes, AllocationSemantics::INTERNAL);
    let zero = builder.imm(0);
    builder.set_memory_object_len(out, zero, MemoryObjectKind::Bytes);
    out
}

/// Encodes at most `3 * groups` bytes, and more than `3 * (groups - 1)`, as
/// `groups` groups, one or two. The 64-character alphabet is written at the
/// output allocation and read one byte early, so each sextet's character is
/// the low byte of its load; `mstore8` writes it to scratch and the
/// characters then load as one word. The output fits one word, so a fixed
/// allocation holds it, and it is written only after the last lookup.
fn encode_lanes(
    builder: &mut FunctionBuilder<'_>,
    input: ValueId,
    n: ValueId,
    file_safe: ValueId,
    no_padding: ValueId,
    groups: u64,
) -> ValueId {
    // out = alloc(64)
    // out[0..64] = "ABC..." ++ ("ghi...+/" ^ file_safe * ("+/" ^ "-_"))
    let size = builder.imm(64);
    let out = builder.alloc_object(size, MemoryObjectLayout::Bytes, AllocationSemantics::INTERNAL);
    let table = builder.cast(out, MirType::MemPtr);
    let first = builder.imm(U256::from_be_slice(b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef"));
    builder.mstore(table, first);
    let second = builder.imm(U256::from_be_slice(b"ghijklmnopqrstuvwxyz0123456789+/"));
    let url_safe = builder.imm(0x0670);
    let file_safe = builder.cast(file_safe, MirType::I256);
    let url_safe = builder.mul(file_safe, url_safe);
    let second = builder.xor(second, url_safe);
    let thirty_two = builder.imm(32);
    let upper = builder.add(table, thirty_two);
    builder.mstore(upper, second);

    // The input fills the low 3 * groups bytes from the top.
    // field = (mload(input + n) & ~(MAX << 8n)) << (24 * groups - 8n)
    let source = builder.cast(input, MirType::MemPtr);
    let end = builder.add(source, n);
    let word = builder.mload(end);
    let three = builder.imm(3);
    let bits = builder.shl(three, n);
    let all = builder.imm(U256::MAX);
    let high = builder.shl(bits, all);
    let low = builder.not(high);
    let word = builder.and(word, low);
    let width = builder.imm(24 * groups);
    let shift = builder.sub(width, bits);
    let field = builder.shl(shift, word);

    // scratch[i] = mload(table - 31 + (field >> (24 * groups - 6 - 6i) & 63))
    let early = builder.imm(31);
    let window = builder.sub(table, early);
    let sixty_three = builder.imm(63);
    for lane in 0..4 * groups {
        let right = builder.imm(24 * groups - 6 - 6 * lane);
        let sextet = builder.shr(right, field);
        let sextet = if lane == 0 { sextet } else { builder.and(sextet, sixty_three) };
        let address = builder.add(window, sextet);
        let character = builder.mload(address);
        let position = builder.imm(lane);
        builder.mstore8(position, character);
    }

    // unpadded = n + groups
    // rest = MAX >> 8 * unpadded
    // padding = no_padding ? 0 : "====..." & rest & ~(MAX >> 32 * groups)
    // word = mload(0) & ~rest | padding
    let zero = builder.imm(0);
    let chars = builder.mload(zero);
    let group_count = builder.imm(groups);
    let unpadded = builder.add(n, group_count);
    let unpadded_bits = builder.shl(three, unpadded);
    let rest = builder.shr(unpadded_bits, all);
    let keep = builder.not(rest);
    let chars = builder.and(chars, keep);
    let equals =
        builder.imm((LANE_ONES * U256::from(b'=')) & !(U256::MAX >> (32 * groups as usize)));
    let padding = builder.and(equals, rest);
    let padding = flag_clear(builder, no_padding, padding);
    let word = builder.or(chars, padding);

    // out.length = 4 * groups - no_padding * (3 * groups - n)
    // out[0..32] = word
    let full = builder.imm(4 * groups);
    let capacity = builder.imm(3 * groups);
    let missing = builder.sub(capacity, n);
    let no_padding_word = builder.cast(no_padding, MirType::I256);
    let missing = builder.mul(no_padding_word, missing);
    let length = builder.sub(full, missing);
    builder.set_memory_object_len(out, length, MemoryObjectKind::Bytes);
    builder.memory_object_store_word(out, zero, word);
    out
}

/// Encodes seven to 24 bytes with one word-kernel pass and no loop. The input
/// fills the kernel's 24-byte field from the top, so the lanes past it encode
/// zero bytes; those characters become '=' padding or are dropped. The output
/// fits one word, so a fixed allocation holds it.
fn encode_single_word(
    builder: &mut FunctionBuilder<'_>,
    input: ValueId,
    n: ValueId,
    file_safe: ValueId,
    no_padding: ValueId,
) -> ValueId {
    // groups = (n + 2) / 3
    // chars = 4 * groups
    // unpadded = n + groups
    // length = no_padding ? unpadded : chars
    let two = builder.imm(2);
    let rounded = builder.add(n, two);
    let three = builder.imm(3);
    let groups = builder.div(rounded, three);
    let chars = builder.shl(two, groups);
    let unpadded = builder.add(n, groups);
    // length = chars - no_padding * (chars - unpadded)
    let dropped = builder.sub(chars, unpadded);
    let no_padding_word = builder.cast(no_padding, MirType::I256);
    let dropped = builder.mul(no_padding_word, dropped);
    let length = builder.sub(chars, dropped);

    // word = (mload(input + n) & ~(MAX << 8n)) << (192 - 8n)
    let source = builder.cast(input, MirType::MemPtr);
    let end = builder.add(source, n);
    let word = builder.mload(end);
    let bits = builder.shl(three, n);
    let all = builder.imm(U256::MAX);
    let high = builder.shl(bits, all);
    let low = builder.not(high);
    let word = builder.and(word, low);
    let field = builder.imm(8 * ENCODE_WORD_BYTES);
    let shift = builder.sub(field, bits);
    let word = builder.shl(shift, word);
    let encoded = encode_word(builder, word, file_safe);

    // rest = MAX >> 8 * unpadded
    // padding = no_padding ? 0 : "====..." & rest & ~(MAX >> 8 * chars)
    // out[0..32] = encoded & ~rest | padding
    let unpadded_bits = builder.shl(three, unpadded);
    let rest = builder.shr(unpadded_bits, all);
    let keep = builder.not(rest);
    let kept = builder.and(encoded, keep);
    let chars_bits = builder.shl(three, chars);
    let beyond = builder.shr(chars_bits, all);
    let padding_lanes = builder.xor(rest, beyond);
    let equals = builder.imm(LANE_ONES * U256::from(b'='));
    let padding = builder.and(equals, padding_lanes);
    let padding = flag_clear(builder, no_padding, padding);
    let word = builder.or(kept, padding);

    // out = alloc(64)
    // out.length = length
    // out[0..32] = word
    let size = builder.imm(64);
    let out = builder.alloc_object(size, MemoryObjectLayout::Bytes, AllocationSemantics::INTERNAL);
    builder.set_memory_object_len(out, length, MemoryObjectKind::Bytes);
    let zero = builder.imm(0);
    builder.memory_object_store_word(out, zero, word);
    out
}

/// Returns `flag ? a : b` for constants `a` and `b` as `b + flag * (a - b)`,
/// with the difference folded.
fn flag_select(builder: &mut FunctionBuilder<'_>, flag: ValueId, a: U256, b: U256) -> ValueId {
    // b + zext(flag) * (a - b)
    let flag = builder.cast(flag, MirType::I256);
    let difference = builder.imm(a.wrapping_sub(b));
    let step = builder.mul(flag, difference);
    let base = builder.imm(b);
    builder.add(base, step)
}

/// Returns `flag ? 0 : value` as `value & (zext(flag) - 1)`.
fn flag_clear(builder: &mut FunctionBuilder<'_>, flag: ValueId, value: ValueId) -> ValueId {
    // value & (zext(flag) - 1)
    let flag = builder.cast(flag, MirType::I256);
    let one = builder.imm(1);
    let keep = builder.sub(flag, one);
    builder.and(value, keep)
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
    // Each threshold step is folded into the result as soon as it is known,
    // so one lane mask is live at a time.
    // result = v + 'A' + 6 [v >= 26] - 75 [v >= 52] - down [v >= 62] + up [v >= 63]
    let spread = builder.imm(LANE_ONES);
    let seven = builder.imm(7);
    let ascii = builder.imm(LANE_ONES * U256::from(b'A'));
    let mut result = builder.add(v, ascii);
    let down = flag_select(builder, file_safe, U256::from(13), U256::from(15));
    let up = flag_select(builder, file_safe, U256::from(49), U256::from(3));
    let six = builder.imm(6);
    let seventy_five = builder.imm(75);
    for (threshold, factor, add) in
        [(26, six, true), (52, seventy_five, false), (62, down, false), (63, up, true)]
    {
        let delta = builder.imm(LANE_ONES * U256::from(128 - threshold));
        let marked = builder.add(v, delta);
        let marked = builder.shr(seven, marked);
        let step = builder.and(marked, spread);
        let adjustment = builder.mul(step, factor);
        result =
            if add { builder.add(result, adjustment) } else { builder.sub(result, adjustment) };
    }
    result
}

impl FunctionLowerer<'_, '_> {
    /// Keep the decoder as one callable body per module and IMAP mode. A
    /// constant mode is folded into the body; a run-time mode is a parameter.
    pub(super) fn lower_core_base64_decode(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let (&input, options) = operands.split_first()?;
        let imap = options.first().copied().unwrap_or_else(|| self.builder.imm_bool(false));
        let imap = self.builder.cast(imap, MirType::I1);
        let constant = self.builder.func().value_u64(imap);
        let name = match constant {
            Some(0) => sym::core_base64_decode,
            Some(_) => sym::core_base64_decode_imap,
            None => sym::core_base64_decode_any,
        };
        let ty = MirType::MemoryObject(MemoryObjectKind::Bytes);
        let helper = self.lazy_helper(name, |this, function| {
            function.attributes.no_inline = true;
            let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
            let input = lowerer.builder.add_param(ty);
            let imap = match constant {
                Some(value) => lowerer.builder.imm_bool(value != 0),
                None => lowerer.builder.add_param(MirType::I1),
            };
            lowerer.builder.set_return_type(ty);
            let out = lowerer.lower_core_base64_decode_body(input, imap)?;
            lowerer.builder.ret([out]);
            Some(())
        })?;
        // result = icall core_base64_decode(input[, imap])
        let args = if constant.is_some() { vec![input] } else { vec![input, imap] };
        Some(self.builder.icall(helper, args, ty))
    }

    /// Decodes `input`, returning the fresh output object.
    fn lower_core_base64_decode_body(&mut self, input: ValueId, imap: ValueId) -> Option<ValueId> {
        let raw_length = self.builder.memory_object_len(input, MemoryObjectKind::Bytes);
        let empty = self.builder.create_block();
        let nonempty = self.builder.create_block();
        let done = self.builder.create_block();
        let is_empty = self.builder.eq_zero(raw_length);
        self.builder.branch(is_empty, empty, nonempty);

        self.builder.switch_to_block(empty);
        let empty_out = alloc_empty(&mut self.builder);
        self.builder.jump(done);

        // A length that is not a multiple of four has no padding, so only an
        // aligned length loads its last characters. The second '=' counts only
        // after a last one. A length of one modulo four has no encoding: it
        // stays one modulo four after stripping at most two characters from an
        // aligned length, so the raw length decides it.
        // lone = raw_length % 4 == 1
        // padding = 0
        // if raw_length % 4 == 0:
        //     x = mload(input + raw_length) ^ "=="
        //     padding = (x & 0xff == 0) << (x & 0xffff == 0)
        // n = raw_length - padding
        self.builder.switch_to_block(nonempty);
        let three = self.builder.imm(3);
        let raw_tail = self.builder.and(raw_length, three);
        let one = self.builder.imm(1);
        let lone = self.builder.eq(raw_tail, one);
        let aligned = self.builder.eq_zero(raw_tail);
        let unpadded = self.builder.current_block();
        let padded = self.builder.create_block();
        let counted = self.builder.create_block();
        self.builder.branch(aligned, padded, counted);

        self.builder.switch_to_block(padded);
        let source = self.builder.cast(input, MirType::MemPtr);
        let end = self.builder.add(source, raw_length);
        let last = self.builder.mload(end);
        let equals = self.builder.imm(0x3d3d);
        let last = self.builder.xor(last, equals);
        let pair_mask = self.builder.imm(0xffff);
        let pair = self.builder.and(last, pair_mask);
        let pair_padding = self.builder.eq_zero(pair);
        let byte_mask = self.builder.imm(0xff);
        let last_char = self.builder.and(last, byte_mask);
        let last_padding = self.builder.eq_zero(last_char);
        let pair_padding = self.builder.cast(pair_padding, MirType::I256);
        let last_padding = self.builder.cast(last_padding, MirType::I256);
        let padding = self.builder.shl(pair_padding, last_padding);
        self.builder.jump(counted);

        self.builder.switch_to_block(counted);
        let none = self.builder.imm(0);
        let padding = self.builder.phi(vec![(unpadded, none), (padded, padding)]);
        let n = self.builder.sub(raw_length, padding);
        let tail = self.builder.and(n, three);
        let mut exits = vec![(empty, empty_out)];
        for lanes in [4, 8, 12, SHORT_DECODE_CHARS] {
            // if n <= lanes: result = decode_lanes(input, n, lanes)
            let block = self.builder.create_block();
            let next = self.builder.create_block();
            let limit = self.builder.imm(lanes + 1);
            let fits = self.builder.lt(n, limit);
            self.builder.branch(fits, block, next);
            self.builder.switch_to_block(block);
            let out = decode_lanes(
                self.cx.gcx,
                self.cx.module,
                &mut self.builder,
                input,
                n,
                tail,
                lone,
                imap,
                lanes,
            );
            exits.push((self.builder.current_block(), out));
            self.builder.jump(done);
            self.builder.switch_to_block(next);
        }
        reject_invalid(&mut self.builder, lone);
        let wide_out = decode_words(&mut self.builder, input, n, tail, imap);
        exits.push((self.builder.current_block(), wide_out));
        self.builder.jump(done);

        self.builder.switch_to_block(done);
        Some(self.builder.phi(exits))
    }
}

/// Decodes `n` characters after padding, 32 lanes per word-kernel pass. The
/// last word's lanes at and past the input's end become 'A', sextet zero, so
/// every word runs the same kernel and a whole word keeps all of its lanes.
/// Loads may read up to 31 bytes past the input; those bytes are replaced.
fn decode_words(
    builder: &mut FunctionBuilder<'_>,
    input: ValueId,
    n: ValueId,
    tail: ValueId,
    imap: ValueId,
) -> ValueId {
    // length = n - n / 4 - (n % 4 != 0)
    // size = header + align32(length + 32)
    let length = decoded_length(builder, n, tail);
    let slack = builder.imm(95);
    let size = builder.add(length, slack);
    let mask = builder.imm(U256::MAX << 5);
    let size = builder.and(size, mask);
    let out = builder.alloc_object(
        size,
        MemoryObjectLayout::Bytes,
        AllocationSemantics::SOLIDITY_UNINITIALIZED,
    );
    builder.set_memory_object_len(out, length, MemoryObjectKind::Bytes);
    let destination = builder.memory_object_data(out, MemoryObjectKind::Bytes);
    let source = builder.cast(input, MirType::MemPtr);
    let thirty_two = builder.imm(32);
    let data = builder.add(source, thirty_two);
    let end = builder.add(data, n);
    let entry = builder.current_block();
    let body = builder.create_block();
    let exit = builder.create_block();
    builder.jump(body);

    // do:
    //     past = MAX >> 8 * (end - cursor)
    //     word = mload(cursor) ^ ((mload(cursor) ^ "AAAA...") & past)
    //     output[0..32] = decode_word(word) << 64
    //     cursor += 32; output += 24
    // while cursor < end
    builder.switch_to_block(body);
    let cursor = builder.phi(vec![(entry, data)]);
    let output = builder.phi(vec![(entry, destination)]);
    let word = builder.mload(cursor);
    let remaining = builder.sub(end, cursor);
    let three = builder.imm(3);
    let remaining_bits = builder.shl(three, remaining);
    let all = builder.imm(U256::MAX);
    let past = builder.shr(remaining_bits, all);
    let filler = builder.imm(LANE_ONES * U256::from(b'A'));
    let differ = builder.xor(word, filler);
    let differ = builder.and(differ, past);
    let word = builder.xor(word, differ);
    let packed = decode_word(builder, word, imap);
    let sixty_four = builder.imm(64);
    let packed = builder.shl(sixty_four, packed);
    builder.mstore(output, packed);
    let next_cursor = builder.add(cursor, thirty_two);
    let twenty_four = builder.imm(24);
    let next_output = builder.add(output, twenty_four);
    let more = builder.lt(next_cursor, end);
    let latch = builder.current_block();
    builder.branch(more, body, exit);
    builder.add_phi_incoming(cursor, latch, next_cursor);
    builder.add_phi_incoming(output, latch, next_output);

    // Clear the artificial output of the last word and the ABI padding.
    // data[length..length + 32] = 0
    builder.switch_to_block(exit);
    let end = builder.add(destination, length);
    let zero = builder.imm(0);
    builder.mstore(end, zero);
    out
}

/// Returns `n - n / 4 - (tail != 0)`, the bytes that `n` characters decode
/// to when `tail = n % 4` is not one. It cannot overflow or underflow.
fn decoded_length(builder: &mut FunctionBuilder<'_>, n: ValueId, tail: ValueId) -> ValueId {
    // length = n - (n >> 2) - zext(tail != 0)
    let two = builder.imm(2);
    let quarter = builder.shr(two, n);
    let length = builder.sub(n, quarter);
    let has_tail = builder.ne_zero(tail);
    let has_tail = builder.cast(has_tail, MirType::I256);
    builder.sub(length, has_tail)
}

/// Writes the 128-byte decode table at `table`, copied from code data when
/// that is cheaper than word stores. A run-time `imap` selects the word
/// holding the comma after the copy.
fn store_decode_table(
    gcx: Gcx<'_>,
    module: &mut Module,
    builder: &mut FunctionBuilder<'_>,
    table: ValueId,
    imap: ValueId,
) {
    // table[0..128] = decode_table(imap)
    let constant = builder.func().value_u64(imap);
    let mut bytes = Vec::with_capacity(DECODE_TABLE_BYTES as usize);
    for index in 0..DECODE_TABLE_BYTES / 32 {
        let word = decode_table_word(index, constant == Some(1));
        bytes.extend_from_slice(&word.to_be_bytes::<32>());
    }
    crate::mir::lower::data::copy_data_to_memory(
        gcx,
        module,
        builder,
        table,
        &bytes,
        bytes.len(),
        None,
    );
    if constant.is_none() {
        // table[32i..32i + 32] = imap ? decode_table_word(i, true) : decode_table_word(i, false)
        let index = decode_table_word_of(b',');
        let plain = builder.imm(decode_table_word(index, false));
        let imap_word = builder.imm(decode_table_word(index, true));
        let word = builder.select(imap, imap_word, plain);
        let address = builder.add_u64_offset(table, 32 * index);
        builder.mstore(address, word);
    }
}

/// Decodes at most sixteen characters, `n` of them after padding, as one
/// block of `lanes` characters, a multiple of four. The table is read one
/// byte early, so each character's entry is the low byte of its load and
/// `mstore8` writes it to scratch. The block's entries then load as one word:
/// one mask validates the input's lanes and the word kernel's lane packing
/// turns them into bytes. The output fits one word, so a fixed allocation
/// holds it. The 128-byte table starts at that allocation and runs into free
/// memory; the output is written only after the last lookup.
#[allow(clippy::too_many_arguments)]
fn decode_lanes(
    gcx: Gcx<'_>,
    module: &mut Module,
    builder: &mut FunctionBuilder<'_>,
    input: ValueId,
    n: ValueId,
    tail: ValueId,
    lone: ValueId,
    imap: ValueId,
    lanes: u64,
) -> ValueId {
    // One group decodes to n - 1 bytes.
    // length = lanes == 4 ? n - 1 : n - n / 4 - (n % 4 != 0)
    // out = alloc(64)
    // table = out + 64
    let length = if lanes == 4 {
        let one = builder.imm(1);
        builder.sub(n, one)
    } else {
        decoded_length(builder, n, tail)
    };
    let size = builder.imm(64);
    let out = builder.alloc_object(size, MemoryObjectLayout::Bytes, AllocationSemantics::INTERNAL);
    let table = builder.cast(out, MirType::MemPtr);
    store_decode_table(gcx, module, builder, table, imap);

    // Lanes past the end read NUL, whose entry stays inside the table; the
    // checks below skip those lanes.
    // within = ~(MAX >> 8n)
    // word = mload(input + 32) & within
    let source = builder.cast(input, MirType::MemPtr);
    let thirty_two = builder.imm(32);
    let data = builder.add(source, thirty_two);
    let word = builder.mload(data);
    let three = builder.imm(3);
    let bits = builder.shl(three, n);
    let all = builder.imm(U256::MAX);
    let past = builder.shr(bits, all);
    let within = builder.not(past);
    let word = builder.and(word, within);

    // scratch[i] = mload(table - 31 + byte(i, word)) for each lane i
    let early = builder.imm(31);
    let window = builder.sub(table, early);
    for lane in 0..lanes {
        let position = builder.imm(lane);
        let character = builder.byte(position, word);
        let address = builder.add(window, character);
        let entry = builder.mload(address);
        builder.mstore8(position, entry);
    }

    // A character above 127 loads past the table, so its high bit decides it.
    // entries = mload(0)
    // invalid = (entries & 0x0303.. & within) | (word & 0x8080..) over the
    //     block's lanes || lone
    let zero = builder.imm(0);
    let entries = builder.mload(zero);
    let block_bits = 8 * (32 - lanes as usize);
    let marks = builder.imm((LANE_ONES * U256::from(3)) << block_bits);
    let marked = builder.and(entries, marks);
    let marked = builder.and(marked, within);
    let high = builder.imm((LANE_ONES * U256::from(0x80)) << block_bits);
    let high = builder.and(word, high);
    let invalid = builder.or(marked, high);
    let lone = builder.cast(lone, MirType::I256);
    let invalid = builder.or(invalid, lone);
    let invalid = builder.ne_zero(invalid);
    reject_invalid(builder, invalid);

    // Join only as many levels as the block's groups span, then move them to
    // the top; the length mask below discards every other lane.
    // packed = join(groups, levels) << (8 << levels)
    let groups = pack_groups(builder, entries, 2);
    let levels = match lanes {
        4 => 0,
        8 => 1,
        _ => 2,
    };
    let joined = join_levels(builder, groups, levels);
    let shift = builder.imm(8 << levels);
    let packed = builder.shl(shift, joined);

    // One group keeps n - 1 bytes; packing left its low byte zero.
    // out.length = length
    // out[0..32] = packed & (lanes == 4 ? ~(past << 8) : ~(MAX >> 8 * length))
    let dropped = if lanes == 4 {
        let eight = builder.imm(8);
        builder.shl(eight, past)
    } else {
        let length_bits = builder.shl(three, length);
        builder.shr(length_bits, all)
    };
    let keep = builder.not(dropped);
    let packed = builder.and(packed, keep);
    builder.set_memory_object_len(out, length, MemoryObjectKind::Bytes);
    builder.memory_object_store_word(out, zero, packed);
    out
}

/// The sextet of `character`, or `None` outside the decoder's alphabets.
const fn decode_sextet(character: u8, imap: bool) -> Option<u8> {
    match character {
        b'A'..=b'Z' => Some(character - b'A'),
        b'a'..=b'z' => Some(character - b'a' + 26),
        b'0'..=b'9' => Some(character - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        b',' if imap => Some(63),
        _ => None,
    }
}

/// The table word holding the entry of `character`.
const fn decode_table_word_of(character: u8) -> u64 {
    character as u64 / 32
}

/// Table word `index`: each byte is a character's sextet shifted left by two,
/// or `0x03` when the character is invalid.
fn decode_table_word(index: u64, imap: bool) -> U256 {
    let mut bytes = [0x03; 32];
    for (offset, entry) in bytes.iter_mut().enumerate() {
        let character = (index * 32) as u8 + offset as u8;
        if let Some(sextet) = decode_sextet(character, imap) {
            *entry = sextet << 2;
        }
    }
    U256::from_be_bytes(bytes)
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
/// Each character class is folded into the running value and validity mask
/// as soon as it is known, so few lane masks are live at once. Invalid lanes
/// are rejected before packing or writing the result.
fn decode_word(builder: &mut FunctionBuilder<'_>, word: ValueId, imap: ValueId) -> ValueId {
    // c = word & 0x7f..; high = word ^ c
    // v = c + 0x04..
    let mask = builder.imm(LANE_ONES * U256::from(127));
    let c = builder.and(word, mask);
    let high = builder.xor(word, c);
    let four = builder.imm(LANE_ONES * U256::from(4));
    let mut v = builder.add(c, four);

    // Letters: 'A'..'Z' and 'a'..'z' after folding case.
    // alpha = range(c | 0x20.., 'a', '{')
    // v -= alpha * 69 + ((c >> 5) & alpha) * 6
    let case_bit = builder.imm(LANE_ONES * U256::from(32));
    let folded = builder.or(c, case_bit);
    let alpha = decode_range(builder, folded, 97, 123);
    let sixty_nine = builder.imm(69);
    let adjustment = builder.mul(alpha, sixty_nine);
    v = builder.sub(v, adjustment);
    let five = builder.imm(5);
    let lowercase = builder.shr(five, c);
    let lowercase = builder.and(lowercase, alpha);
    let six = builder.imm(6);
    let adjustment = builder.mul(lowercase, six);
    v = builder.sub(v, adjustment);

    // Digits map by the common offset alone.
    // valid = alpha | range(c, '0', ':')
    let digits = decode_range(builder, c, 48, 58);
    let mut valid = builder.or(alpha, digits);

    // Within '+'..'/', the odd bytes are '+', '-' and '/'. The only
    // even byte with bit 1 clear is the optional IMAP comma.
    // odd = range(c, '+', '0') & c
    // slash = odd & c >> 1 & c >> 2
    // comma = imap ? range(c, '+', '0') & ~c & ~(c >> 1) : 0
    let punctuation = decode_range(builder, c, 43, 48);
    let odd = builder.and(punctuation, c);
    let one = builder.imm(1);
    let bit1 = builder.shr(one, c);
    let two = builder.imm(2);
    let bit2 = builder.shr(two, c);
    let slash = builder.and(odd, bit1);
    let slash = builder.and(slash, bit2);
    let not_c = builder.not(c);
    let not_bit1 = builder.not(bit1);
    let comma = builder.and(punctuation, not_c);
    let comma = builder.and(comma, not_bit1);
    let zero = builder.imm(0);
    let comma = builder.select(imap, comma, zero);
    let underscore = decode_range(builder, c, 95, 96);

    // Symbols decode to 62, plus one for '/', ',' and '_'.
    // symbols = odd | comma | underscore
    // v += symbols * 58 - (c & symbols * 255) + (slash | comma | underscore)
    let symbols = builder.or(odd, comma);
    let symbols = builder.or(symbols, underscore);
    valid = builder.or(valid, symbols);
    let fifty_eight = builder.imm(58);
    let correction = builder.mul(symbols, fifty_eight);
    v = builder.add(v, correction);
    let byte_mask = builder.imm(255);
    let symbol_mask = builder.mul(symbols, byte_mask);
    let symbol_bytes = builder.and(c, symbol_mask);
    v = builder.sub(v, symbol_bytes);
    let extra = builder.or(slash, comma);
    let extra = builder.or(extra, underscore);
    v = builder.add(v, extra);

    // invalid = valid != 0x01.. || high != 0
    let all = builder.imm(LANE_ONES);
    let invalid = builder.ne(valid, all);
    let high = builder.ne_zero(high);
    let invalid = builder.or(invalid, high);
    reject_invalid(builder, invalid);
    let groups = pack_groups(builder, v, 0);
    join_groups(builder, groups)
}

/// Packs the sextets in the byte lanes of `lanes`, each shifted left by
/// `bias` bits, four at a time: every 32-bit group holds its three bytes in
/// its low 24 bits.
fn pack_groups(builder: &mut FunctionBuilder<'_>, lanes: ValueId, bias: u64) -> ValueId {
    // Merge neighbouring bytes, then neighbouring halves:
    // pairs = (lanes >> (2 + bias)) & 0x0fc0.. | (lanes >> bias) & 0x003f..
    // groups = (pairs >> 4) & 0x00fff000.. | pairs & 0x00000fff..
    let high = builder.imm(repeat_pattern(0x0fc0, 16));
    let low = builder.imm(repeat_pattern(0x003f, 16));
    let shift = builder.imm(2 + bias);
    let upper = builder.shr(shift, lanes);
    let upper = builder.and(upper, high);
    let lower = if bias == 0 {
        lanes
    } else {
        let shift = builder.imm(bias);
        builder.shr(shift, lanes)
    };
    let lower = builder.and(lower, low);
    let pairs = builder.or(upper, lower);
    let high = builder.imm(repeat_pattern(0x00ff_f000, 32));
    let low = builder.imm(repeat_pattern(0x0000_0fff, 32));
    let four = builder.imm(4);
    let upper = builder.shr(four, pairs);
    let upper = builder.and(upper, high);
    let lower = builder.and(pairs, low);
    builder.or(upper, lower)
}

/// Joins the eight 24-bit groups of `groups` into 24 bytes, right-aligned.
fn join_groups(builder: &mut FunctionBuilder<'_>, groups: ValueId) -> ValueId {
    // packed = join(groups, 2)
    // packed = packed >> 128 << 96 | packed & (MAX >> 160)
    let packed = join_levels(builder, groups, 2);
    let shift = builder.imm(128);
    let upper = builder.shr(shift, packed);
    let shift = builder.imm(96);
    let upper = builder.shl(shift, upper);
    let mask = builder.imm(U256::MAX >> 160);
    let lower = builder.and(packed, mask);
    builder.or(upper, lower)
}

/// Joins neighbouring 24-bit groups `levels` times, inverting the encoder's
/// lane spreading: once gives 48 bits right-aligned in each 64-bit lane,
/// twice 96 bits in each 128-bit lane.
fn join_levels(builder: &mut FunctionBuilder<'_>, groups: ValueId, levels: usize) -> ValueId {
    // packed = (packed >> 8) & HIGH48 | packed & LOW48, then with 16 for 96 bits
    let mut packed = groups;
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
    ]
    .into_iter()
    .take(levels)
    {
        let high = builder.imm(U256::from_str_radix(high, 16).unwrap());
        let low = builder.imm(U256::from_str_radix(low, 16).unwrap());
        let shift = builder.imm(shift);
        let upper = builder.shr(shift, packed);
        let upper = builder.and(upper, high);
        let lower = builder.and(packed, low);
        packed = builder.or(upper, lower);
    }
    packed
}

/// `pattern` repeated in every `width`-bit lane of a word.
fn repeat_pattern(pattern: u64, width: usize) -> U256 {
    (0..256 / width).fold(U256::ZERO, |word, _| (word << width) | U256::from(pattern))
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
