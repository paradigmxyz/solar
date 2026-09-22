//! Base64 codec lowering for the compiler-owned core module.
//!
//! Empty inputs return a fresh empty object without further work, so a
//! terminal return can still encode the result in place. One to three input
//! bytes are encoded as one
//! group in one word: each character is a `byte` lookup in one of two packed
//! 32-character alphabet halves. Longer inputs call the shared wide encoder,
//! which maps 24 input bytes to 32 ASCII lanes per word.
//!
//! Decoding strips trailing padding with one load that ends at the last
//! character. At most sixteen remaining characters are decoded one group of
//! four at a time, and a single group without a loop: a 128-byte table
//! written just past the output allocation
//! turns each character into one load whose top byte is its sextet, shifted
//! left by two, or an invalid mark. Longer inputs call the shared wide decoder,
//! which validates and converts 32 ASCII lanes to 24 bytes per word. A partial
//! group or word uses the same code: the encoder supplies zero bytes and the
//! decoders supply 'A' lanes before discarding the artificial output. Input
//! loads end at valid input bytes, so neither dirty padding nor memory beyond
//! the input allocation is read.
//!
//! Each output owns an extra word of capacity, allowing whole-word stores at
//! the final position without touching another object. Logical length excludes
//! that capacity; stores initialize every returned byte and its ABI padding.
//! Encoded lengths are checked before allocation. A decoded length is at most
//! three quarters of the input length, so its arithmetic cannot overflow.
//! Single-word outputs use one fixed-size allocation. The decode table is the
//! only temporary: it lives in free memory past that allocation and is dead
//! before the function returns. Input, scratch, and the zero slot are never
//! written (error selectors use scratch).
//!
//! One shared encoder body avoids cloning the algorithm at every overload; its
//! wide loop is a separate function so the short path keeps its arguments on
//! the stack. The decoder's word kernel is also a separate function,
//! isolating its live values from loop state and reducing scheduler spills.
//! These decisions are backed by the Base64 benchmark, rather than a
//! universal inlining heuristic. Checked Solidity bodies remain the reference
//! under `-Zno-core-intrinsics`.

use super::*;

/// Decoded inputs of at most this many characters, after padding, use the
/// scalar table decoder. Four groups cost about as much as one word-kernel
/// pass, which amortizes over eight groups.
const SHORT_DECODE_CHARS: u64 = 16;

/// Bytes in the scalar decode table: one entry per 7-bit character.
const DECODE_TABLE_BYTES: u64 = 128;

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

    /// Encodes empty and single-group inputs here and forwards longer inputs
    /// to the wide encoder.
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
        let short = self.builder.create_block();
        let wide = self.builder.create_block();
        let four = self.builder.imm(4);
        let is_short = self.builder.lt(n, four);
        self.builder.branch(is_short, short, wide);

        self.builder.switch_to_block(short);
        let out = encode_short(&mut self.builder, input, n, file_safe, no_padding);
        self.builder.ret([out]);

        // ret icall core_base64_encode_wide(input, file_safe, no_padding)
        self.builder.switch_to_block(wide);
        let out = self.builder.icall(wide_helper, vec![input, file_safe, no_padding], ty);
        self.builder.ret([out]);
        Some(())
    }

    fn lower_core_base64_encode_wide(
        &mut self,
        input: ValueId,
        file_safe: ValueId,
        no_padding: ValueId,
    ) -> ValueId {
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
        // Every iteration writes a whole output word. Own one extra word
        // so the last write, including zero padding, cannot touch a neighbour.
        let extra = self.builder.imm(32);
        let capacity = self.builder.checked_add(length, extra);
        let out =
            self.builder.alloc_bytes_object(capacity, AllocationSemantics::SOLIDITY_UNINITIALIZED);
        self.builder.set_memory_object_len(out, length, MemoryObjectKind::Bytes);
        let source = self.builder.cast(input, MirType::MemPtr);
        let destination = self.builder.memory_object_data(out, MemoryObjectKind::Bytes);
        let twenty_four = self.builder.imm(24);
        let twenty_three = self.builder.imm(23);
        let rounded = self.builder.add(n, twenty_three);
        let chunks = self.builder.div(rounded, twenty_four);
        let thirty_two = self.builder.imm(32);
        let eight = self.builder.imm(8);
        self.builder.counted_loop(chunks, |builder, chunk| {
            let offset = builder.mul(chunk, twenty_four);
            let remaining = builder.sub(n, offset);
            let short = builder.lt(remaining, twenty_four);
            let count = builder.select(short, remaining, twenty_four);
            let end = builder.add(offset, count);
            let address = builder.add(source, end);
            let word = builder.mload(address);
            let bits = builder.mul(count, eight);
            let all = builder.imm(U256::MAX);
            let high = builder.shl(bits, all);
            let mask = builder.not(high);
            let word = builder.and(word, mask);
            let missing = builder.sub(twenty_four, count);
            let shift = builder.mul(missing, eight);
            let word = builder.shl(shift, word);
            let encoded = encode_word(builder, word, file_safe);
            let offset = builder.mul(chunk, thirty_two);
            let dest = builder.add(destination, offset);
            let remaining = builder.sub(length, offset);
            let short = builder.lt(remaining, thirty_two);
            let count = builder.select(short, remaining, thirty_two);
            let missing = builder.sub(thirty_two, count);
            let bits = builder.mul(missing, eight);
            let mask = builder.shl(bits, all);
            let encoded = builder.and(encoded, mask);
            builder.mstore(dest, encoded);
        });

        let add_padding = self.builder.create_block();
        let done = self.builder.create_block();
        let padded = self.builder.eq_zero(no_padding);
        let needs_padding = self.builder.and(has_tail, padded);
        self.builder.branch(needs_padding, add_padding, done);
        self.builder.switch_to_block(add_padding);
        let last = self.builder.add(destination, length);
        let last = self.builder.sub(last, one);
        let equals = self.builder.imm(0x3d);
        self.builder.mstore8(last, equals);
        let second_padding = self.builder.create_block();
        let one_byte = self.builder.eq(remainder, one);
        self.builder.branch(one_byte, second_padding, done);
        self.builder.switch_to_block(second_padding);
        let previous = self.builder.sub(last, one);
        self.builder.mstore8(previous, equals);
        self.builder.jump(done);
        self.builder.switch_to_block(done);
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

/// Encodes one to three bytes as a single group in one word. Two packed
/// 32-byte alphabet halves make each character two `byte` lookups, of which
/// the index outside its half yields zero; this is cheaper than running the
/// eight-group lane kernel for one group.
fn encode_short(
    builder: &mut FunctionBuilder<'_>,
    input: ValueId,
    n: ValueId,
    file_safe: ValueId,
    no_padding: ValueId,
) -> ValueId {
    // word = mload(input + n) & ~(MAX << 8n)
    // group = word << (24 - 8n)
    let source = builder.cast(input, MirType::MemPtr);
    let end = builder.add(source, n);
    let word = builder.mload(end);
    let three = builder.imm(3);
    let bits = builder.shl(three, n);
    let all = builder.imm(U256::MAX);
    let high = builder.shl(bits, all);
    let low = builder.not(high);
    let word = builder.and(word, low);
    let twenty_four = builder.imm(24);
    let shift = builder.sub(twenty_four, bits);
    let group = builder.shl(shift, word);

    // second = "ghijklmnopqrstuvwxyz0123456789+/" ^ file_safe * ("+/" ^ "-_")
    let first = builder.imm(U256::from_be_slice(b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef"));
    let second = builder.imm(U256::from_be_slice(b"ghijklmnopqrstuvwxyz0123456789+/"));
    let url_safe = builder.imm(0x0670);
    let file_safe = builder.cast(file_safe, MirType::I256);
    let url_safe = builder.mul(file_safe, url_safe);
    let second = builder.xor(second, url_safe);

    // sextet = group >> right & 63
    // chars |= (byte(sextet, first) | byte(sextet ^ 32, second)) << (248 - 8 * position)
    let thirty_two = builder.imm(32);
    let sixty_three = builder.imm(63);
    let mut chars = None;
    for (position, right) in [18u64, 12, 6, 0].into_iter().enumerate() {
        let right = builder.imm(right);
        let sextet = builder.shr(right, group);
        let sextet = if position == 0 { sextet } else { builder.and(sextet, sixty_three) };
        let lower = builder.byte(sextet, first);
        let upper_index = builder.xor(sextet, thirty_two);
        let upper = builder.byte(upper_index, second);
        let character = builder.or(lower, upper);
        let left = builder.imm(248 - 8 * position as u64);
        let character = builder.shl(left, character);
        chars = Some(match chars {
            Some(chars) => builder.or(chars, character),
            None => character,
        });
    }
    let chars = chars.expect("four characters");

    // rest = MAX >> 8(n + 1)
    // word = chars & ~rest | (no_padding ? 0 : "====") & rest
    let one = builder.imm(1);
    let kept = builder.add(n, one);
    let kept_bits = builder.shl(three, kept);
    let rest = builder.shr(kept_bits, all);
    let keep = builder.not(rest);
    let chars = builder.and(chars, keep);
    let zero = builder.imm(0);
    let equals = builder.imm(U256::from(0x3d3d_3d3d_u64) << 224);
    let padding = builder.select(no_padding, zero, equals);
    let padding = builder.and(padding, rest);
    let word = builder.or(chars, padding);

    // out = alloc(64)
    // out.length = no_padding ? n + 1 : 4
    // out[0..32] = word
    let four = builder.imm(4);
    let length = builder.select(no_padding, kept, four);
    let size = builder.imm(64);
    let out = builder.alloc_object(size, MemoryObjectLayout::Bytes, AllocationSemantics::INTERNAL);
    builder.set_memory_object_len(out, length, MemoryObjectKind::Bytes);
    builder.memory_object_store_word(out, zero, word);
    out
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
        let imap = self.builder.cast(imap, MirType::I1);
        let ty = MirType::MemoryObject(MemoryObjectKind::Bytes);
        let wide_helper =
            self.lazy_helper(Symbol::intern("core_base64_decode_wide"), |this, function| {
                function.attributes.no_inline = true;
                let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
                let input = lowerer.builder.add_param(ty);
                let n = lowerer.builder.add_param(MirType::I256);
                let imap = lowerer.builder.add_param(MirType::I1);
                lowerer.builder.set_return_type(ty);
                let out = lowerer.lower_core_base64_decode_wide(input, n, imap)?;
                lowerer.builder.ret([out]);
                Some(())
            })?;
        let raw_length = self.builder.memory_object_len(input, MemoryObjectKind::Bytes);
        let empty = self.builder.create_block();
        let nonempty = self.builder.create_block();
        let done = self.builder.create_block();
        let is_empty = self.builder.eq_zero(raw_length);
        self.builder.branch(is_empty, empty, nonempty);

        self.builder.switch_to_block(empty);
        let empty_out = alloc_empty(&mut self.builder);
        self.builder.jump(done);

        // A length that is not a multiple of four has no padding; folding its
        // remainder into the low byte makes both tests fail. The second '='
        // counts only after a last one.
        // x = (mload(input + raw_length) ^ "==") | raw_length % 4
        // n = raw_length - (x & 0xffff == 0) - (x & 0xff == 0)
        self.builder.switch_to_block(nonempty);
        let source = self.builder.cast(input, MirType::MemPtr);
        let end = self.builder.add(source, raw_length);
        let last = self.builder.mload(end);
        let equals = self.builder.imm(0x3d3d);
        let last = self.builder.xor(last, equals);
        let three = self.builder.imm(3);
        let raw_tail = self.builder.and(raw_length, three);
        let last = self.builder.or(last, raw_tail);
        let pair_mask = self.builder.imm(0xffff);
        let pair = self.builder.and(last, pair_mask);
        let pair_padding = self.builder.eq_zero(pair);
        let byte_mask = self.builder.imm(0xff);
        let last_char = self.builder.and(last, byte_mask);
        let last_padding = self.builder.eq_zero(last_char);
        let pair_padding = self.builder.cast(pair_padding, MirType::I256);
        let last_padding = self.builder.cast(last_padding, MirType::I256);
        let n = self.builder.sub(raw_length, pair_padding);
        let n = self.builder.sub(n, last_padding);
        let single = self.builder.create_block();
        let several = self.builder.create_block();
        let short = self.builder.create_block();
        let wide = self.builder.create_block();
        let five = self.builder.imm(5);
        let is_single = self.builder.lt(n, five);
        self.builder.branch(is_single, single, several);

        // One group checks a lone character with its own characters.
        self.builder.switch_to_block(single);
        let single_out = decode_single(&mut self.builder, input, n, imap);
        let single_exit = self.builder.current_block();
        self.builder.jump(done);

        // A length of one modulo four has no encoding.
        self.builder.switch_to_block(several);
        let tail = self.builder.and(n, three);
        let one = self.builder.imm(1);
        let invalid = self.builder.eq(tail, one);
        reject_invalid(&mut self.builder, invalid);
        let limit = self.builder.imm(SHORT_DECODE_CHARS + 1);
        let is_short = self.builder.lt(n, limit);
        self.builder.branch(is_short, short, wide);

        self.builder.switch_to_block(short);
        let short_out = decode_short(&mut self.builder, input, n, tail, imap);
        let short_exit = self.builder.current_block();
        self.builder.jump(done);

        // result = icall core_base64_decode_wide(input, n, imap)
        self.builder.switch_to_block(wide);
        let wide_out = self.builder.icall(wide_helper, vec![input, n, imap], ty);
        self.builder.jump(done);

        self.builder.switch_to_block(done);
        Some(self.builder.phi(vec![
            (empty, empty_out),
            (single_exit, single_out),
            (short_exit, short_out),
            (wide, wide_out),
        ]))
    }

    /// Decodes more than sixteen characters, `n` of them after padding, 32
    /// lanes per word kernel call. Whole words are loaded straight from the
    /// input; the partial last word is end-aligned and padded with 'A' lanes.
    fn lower_core_base64_decode_wide(
        &mut self,
        input: ValueId,
        n: ValueId,
        imap: ValueId,
    ) -> Option<ValueId> {
        let word_helper =
            self.lazy_helper(Symbol::intern("core_base64_decode_word"), |_, function| {
                function.attributes.no_inline = true;
                let mut builder = FunctionBuilder::new_semantic(function);
                let word = builder.add_param(MirType::I256);
                let imap = builder.add_param(MirType::I1);
                builder.set_return_type(MirType::I256);
                let packed = decode_word(&mut builder, word, imap);
                builder.ret([packed]);
                Some(())
            })?;
        // length = n - n / 4 - (n % 4 != 0)
        // size = header + align32(length + 32)
        let three = self.builder.imm(3);
        let tail = self.builder.and(n, three);
        let length = decoded_length(&mut self.builder, n, tail);
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
        let destination = self.builder.memory_object_data(out, MemoryObjectKind::Bytes);
        let source = self.builder.cast(input, MirType::MemPtr);
        let thirty_two = self.builder.imm(32);
        let twenty_four = self.builder.imm(24);
        let sixty_four = self.builder.imm(64);

        // for (cursor = input + 32, output = data; cursor != input + 32 + (n & ~31); ...)
        //     output[0..32] = decode_word(mload(cursor)) << 64
        let data = self.builder.add(source, thirty_two);
        let low = self.builder.imm(31);
        let whole_mask = self.builder.not(low);
        let whole = self.builder.and(n, whole_mask);
        let whole_end = self.builder.add(data, whole);
        let entry = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let after = self.builder.create_block();
        self.builder.jump(header);

        self.builder.switch_to_block(header);
        let cursor = self.builder.phi(vec![(entry, data)]);
        let output = self.builder.phi(vec![(entry, destination)]);
        let finished = self.builder.eq(cursor, whole_end);
        self.builder.branch(finished, after, body);

        self.builder.switch_to_block(body);
        let word = self.builder.mload(cursor);
        let packed = self.builder.icall(word_helper, vec![word, imap], MirType::I256);
        let packed = self.builder.shl(sixty_four, packed);
        self.builder.mstore(output, packed);
        let next_cursor = self.builder.add(cursor, thirty_two);
        let next_output = self.builder.add(output, twenty_four);
        self.builder.jump(header);
        self.builder.add_phi_incoming(cursor, body, next_cursor);
        self.builder.add_phi_incoming(output, body, next_output);

        // rest = n & 31
        // if rest != 0:
        //     word = mload(input + n) << 8(32 - rest) | "AAAA..." >> 8 rest
        //     output[0..32] = decode_word(word) << 64
        self.builder.switch_to_block(after);
        let partial = self.builder.create_block();
        let finish = self.builder.create_block();
        let rest = self.builder.and(n, low);
        let has_rest = self.builder.ne_zero(rest);
        self.builder.branch(has_rest, partial, finish);

        self.builder.switch_to_block(partial);
        let end = self.builder.add(source, n);
        let word = self.builder.mload(end);
        let missing = self.builder.sub(thirty_two, rest);
        let missing_bits = self.builder.shl(three, missing);
        let word = self.builder.shl(missing_bits, word);
        // Fill missing input lanes with 'A' (sextet zero). Shifting the
        // load to the top first discards bytes preceding this group.
        let rest_bits = self.builder.shl(three, rest);
        let filler = self.builder.imm((U256::MAX / U256::from(255)) * U256::from(65));
        let filler = self.builder.shr(rest_bits, filler);
        let word = self.builder.or(word, filler);
        let packed = self.builder.icall(word_helper, vec![word, imap], MirType::I256);
        let packed = self.builder.shl(sixty_four, packed);
        self.builder.mstore(output, packed);
        self.builder.jump(finish);

        // Clear the artificial output of the last word and the ABI padding.
        // data[length..length + 32] = 0
        self.builder.switch_to_block(finish);
        let end = self.builder.add(destination, length);
        let zero = self.builder.imm(0);
        self.builder.mstore(end, zero);
        Some(out)
    }
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

/// Writes the 128-byte decode table at `table`, with the IMAP comma in word
/// one when `imap` is set.
fn store_decode_table(builder: &mut FunctionBuilder<'_>, table: ValueId, imap: ValueId) {
    // table[32i..32i + 32] = decode_table_word(i)
    for index in 0..DECODE_TABLE_BYTES / 32 {
        let mut word = builder.imm(decode_table_word(index, false));
        if index == decode_table_word_of(b',') {
            let imap_word = builder.imm(decode_table_word(index, true));
            word = builder.select(imap, imap_word, word);
        }
        let offset = builder.imm(32 * index);
        let address = builder.add(table, offset);
        builder.mstore(address, word);
    }
}

/// Returns the last group's four characters in the low bytes of a word:
/// `missing` trailing lanes of a partial group become 'A', sextet zero.
fn load_last_group(
    builder: &mut FunctionBuilder<'_>,
    source: ValueId,
    n: ValueId,
    missing: ValueId,
) -> ValueId {
    // shift = 8 * missing
    // word = mload(input + n) << shift | "AAAA" >> (32 - shift)
    let three = builder.imm(3);
    let shift = builder.shl(three, missing);
    let end = builder.add(source, n);
    let word = builder.mload(end);
    let word = builder.shl(shift, word);
    let thirty_two = builder.imm(32);
    let fill_shift = builder.sub(thirty_two, shift);
    let filler = builder.imm(0x4141_4141_u64);
    let filler = builder.shr(fill_shift, filler);
    builder.or(word, filler)
}

/// Decodes two to four characters, `n` of them after padding: one group,
/// `n - 1` bytes, one masked store. The 128-byte table follows the fixed
/// allocation in free memory.
fn decode_single(
    builder: &mut FunctionBuilder<'_>,
    input: ValueId,
    n: ValueId,
    imap: ValueId,
) -> ValueId {
    // out = alloc(64)
    // table = out + 64
    let size = builder.imm(64);
    let out = builder.alloc_object(size, MemoryObjectLayout::Bytes, AllocationSemantics::INTERNAL);
    let base = builder.cast(out, MirType::MemPtr);
    let table = builder.add(base, size);
    store_decode_table(builder, table, imap);

    // group = decode_group(last_group(4 - n)), invalid when n == 1
    let source = builder.cast(input, MirType::MemPtr);
    let four = builder.imm(4);
    let missing = builder.sub(four, n);
    let word = load_last_group(builder, source, n, missing);
    let one = builder.imm(1);
    let lone = builder.eq(n, one);
    let lone = builder.cast(lone, MirType::I256);
    let group = decode_group(builder, word, table, Some(lone));

    // length = n - 1
    // out.length = length
    // out[0..32] = (group << 232) & ~(MAX >> 8 * length)
    let length = builder.sub(n, one);
    builder.set_memory_object_len(out, length, MemoryObjectKind::Bytes);
    let top = builder.imm(232);
    let data = builder.shl(top, group);
    let three = builder.imm(3);
    let length_bits = builder.shl(three, length);
    let all = builder.imm(U256::MAX);
    let dropped = builder.shr(length_bits, all);
    let keep = builder.not(dropped);
    let data = builder.and(data, keep);
    let zero = builder.imm(0);
    builder.memory_object_store_word(out, zero, data);
    out
}

/// Decodes five to sixteen characters, `n` of them after padding, one group
/// of four at a time and last group first. That group is the only partial
/// one, so its 'A' lanes are filled once before a loop that needs no other
/// test. Each group is stored right-aligned: the zero bytes above it land on
/// the groups not yet written, then on the length word, which is written
/// last. The output fits two words, so a fixed allocation holds every store;
/// the 128-byte table follows it in free memory.
fn decode_short(
    builder: &mut FunctionBuilder<'_>,
    input: ValueId,
    n: ValueId,
    tail: ValueId,
    imap: ValueId,
) -> ValueId {
    // out = alloc(96)
    // table = out + 96
    let length = decoded_length(builder, n, tail);
    let size = builder.imm(96);
    let out = builder.alloc_object(size, MemoryObjectLayout::Bytes, AllocationSemantics::INTERNAL);
    let base = builder.cast(out, MirType::MemPtr);
    let table = builder.add(base, size);
    store_decode_table(builder, table, imap);

    // word = last_group((0 - n) % 4)
    // whole = (n + 3) & ~3
    // output = out + 3 * whole / 4
    // next = input + whole - 4
    let source = builder.cast(input, MirType::MemPtr);
    let three = builder.imm(3);
    let zero = builder.imm(0);
    let negated = builder.sub(zero, n);
    let missing = builder.and(negated, three);
    let last = load_last_group(builder, source, n, missing);
    let whole = builder.add(n, three);
    let low = builder.not(three);
    let whole = builder.and(whole, low);
    let two = builder.imm(2);
    let groups = builder.shr(two, whole);
    let output_bytes = builder.sub(whole, groups);
    let output = builder.add(base, output_bytes);
    let four = builder.imm(4);
    let next = builder.add(source, whole);
    let next = builder.sub(next, four);
    let entry = builder.current_block();
    let body = builder.create_block();
    let advance = builder.create_block();
    let exit = builder.create_block();
    builder.jump(body);

    // mstore(output, decode_group(word))
    // if next > input: word = mload(next); output -= 3; next -= 4; repeat
    builder.switch_to_block(body);
    let word = builder.phi(vec![(entry, last)]);
    let output = builder.phi(vec![(entry, output)]);
    let next = builder.phi(vec![(entry, next)]);
    let group = decode_group(builder, word, table, None);
    builder.mstore(output, group);
    let more = builder.gt(next, source);
    builder.branch(more, advance, exit);

    builder.switch_to_block(advance);
    let next_word = builder.mload(next);
    let next_output = builder.sub(output, three);
    let following = builder.sub(next, four);
    builder.jump(body);
    builder.add_phi_incoming(word, advance, next_word);
    builder.add_phi_incoming(output, advance, next_output);
    builder.add_phi_incoming(next, advance, following);

    // Clear the last group's artificial bytes and the ABI padding, then
    // restore the length word under the first group's store.
    // data[length..length + 32] = 0
    // out.length = length
    builder.switch_to_block(exit);
    let destination = builder.memory_object_data(out, MemoryObjectKind::Bytes);
    let end = builder.add(destination, length);
    builder.mstore(end, zero);
    builder.set_memory_object_len(out, length, MemoryObjectKind::Bytes);
    out
}

/// Decodes the four characters in the low bytes of `word` through `table`.
/// Returns the three bytes right-aligned, with zeros above them. A nonzero
/// `also_invalid` rejects the input with the characters.
fn decode_group(
    builder: &mut FunctionBuilder<'_>,
    word: ValueId,
    table: ValueId,
    also_invalid: Option<ValueId>,
) -> ValueId {
    // entry[i] = byte(0, mload(table + byte(28 + i, word)))
    let zero = builder.imm(0);
    let mut entries = Vec::with_capacity(4);
    for position in 28..32u64 {
        let position = builder.imm(position);
        let character = builder.byte(position, word);
        let address = builder.add(table, character);
        let loaded = builder.mload(address);
        entries.push(builder.byte(zero, loaded));
    }
    let [a, b, c, d] = entries[..] else { unreachable!() };

    // invalid = (a | b | c | d) & 3 | word & 0x80808080
    // A character above 127 loads past the table, so its high bit decides it.
    let any = builder.or(a, b);
    let rest = builder.or(c, d);
    let any = builder.or(any, rest);
    let three = builder.imm(3);
    let marked = builder.and(any, three);
    let high = builder.imm(0x8080_8080_u64);
    let high = builder.and(word, high);
    let mut invalid = builder.or(marked, high);
    if let Some(also_invalid) = also_invalid {
        invalid = builder.or(invalid, also_invalid);
    }
    let invalid = builder.ne_zero(invalid);
    reject_invalid(builder, invalid);

    // Each entry is its sextet shifted left by two.
    // group = a << 16 | b << 10 | c << 4 | d >> 2
    let sixteen = builder.imm(16);
    let a = builder.shl(sixteen, a);
    let ten = builder.imm(10);
    let b = builder.shl(ten, b);
    let four = builder.imm(4);
    let c = builder.shl(four, c);
    let two = builder.imm(2);
    let d = builder.shr(two, d);
    let group = builder.or(a, b);
    let rest = builder.or(c, d);
    builder.or(group, rest)
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
/// Invalid lanes are rejected before packing or writing the result.
fn decode_word(builder: &mut FunctionBuilder<'_>, word: ValueId, imap: ValueId) -> ValueId {
    let ones = U256::MAX / U256::from(255);
    let mask = builder.imm(ones * U256::from(127));
    let c = builder.and(word, mask);
    let case_bit = builder.imm(ones * U256::from(32));
    let folded = builder.or(c, case_bit);
    let alpha = decode_range(builder, folded, 97, 123);
    let five = builder.imm(5);
    let lowercase = builder.shr(five, c);
    let lowercase = builder.and(lowercase, alpha);
    let digits = decode_range(builder, c, 48, 58);
    let punctuation = decode_range(builder, c, 43, 48);
    // Within '+'..'/', the odd bytes are '+', '-' and '/'. The only
    // even byte with bit 1 clear is the optional IMAP comma.
    let odd = builder.and(punctuation, c);
    let one = builder.imm(1);
    let two = builder.imm(2);
    let bit1 = builder.shr(one, c);
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
    let symbols = builder.or(odd, comma);
    let symbols = builder.or(symbols, underscore);
    let valid = builder.or(alpha, digits);
    let valid = builder.or(valid, symbols);
    let all = builder.imm(ones);
    let invalid = builder.ne(valid, all);
    let high = builder.imm(ones * U256::from(128));
    let high = builder.and(word, high);
    let high = builder.ne_zero(high);
    let invalid = builder.or(invalid, high);
    reject_invalid(builder, invalid);
    let four = builder.imm(ones * U256::from(4));
    let mut v = builder.add(c, four);
    for (lane, delta) in [(alpha, 69), (lowercase, 6)] {
        let delta = builder.imm(delta);
        let adjustment = builder.mul(lane, delta);
        v = builder.sub(v, adjustment);
    }
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
