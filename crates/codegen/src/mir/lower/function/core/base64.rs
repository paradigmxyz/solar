//! Base64 codec lowering for the compiler-owned core module.
//!
//! Encoding maps 24 input bytes to 32 ASCII lanes; decoding validates and
//! converts 32 ASCII lanes to 24 bytes. A partial group uses the same kernel:
//! the encoder supplies zero bytes and the decoder supplies 'A' lanes before
//! discarding the artificial output. Loads end at valid input bytes, so neither
//! dirty padding nor memory beyond the input allocation is read.
//!
//! Each output owns an extra word of capacity, allowing whole-word stores at
//! the final position without touching another object. Logical length excludes
//! that capacity; stores initialize every returned byte and its ABI padding.
//! Length arithmetic is checked before allocation. Input, scratch, and the zero
//! slot are not borrowed as temporary storage (error selectors use scratch).
//!
//! One shared encoder body avoids cloning the algorithm at every overload.
//! The decoder's word kernel is also a separate function, isolating its live
//! values from loop state and reducing scheduler spills. Both decisions are
//! backed by the Base64 benchmark, rather than a universal inlining heuristic.
//! Checked Solidity bodies remain the reference under `-Zno-core-intrinsics`.

use super::*;

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
            let out = lowerer.lower_core_base64_encode(&[input, file_safe, no_padding])?;
            lowerer.builder.ret([out]);
            Some(())
        })?;
        Some(self.builder.icall(
            helper,
            vec![input, file_safe, no_padding],
            MirType::MemoryObject(MemoryObjectKind::Bytes),
        ))
    }

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
        // Every iteration writes a whole output word. Own one extra word
        // so the last write, including zero padding, cannot touch a neighbour.
        let extra = self.builder.imm(32);
        let capacity = self.builder.checked_add(length, extra);
        let out =
            self.builder.alloc_bytes_object(capacity, AllocationSemantics::SOLIDITY_UNINITIALIZED);
        self.builder.set_memory_object_len(out, length, MemoryObjectKind::Bytes);
        let work = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.branch(n, work, done);
        self.builder.switch_to_block(work);
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
        Some(out)
    }
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
        let imap = self.builder.cast(imap, MirType::I1);
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
        // A word store writes each group and its zero padding together.
        // Reserve a full extra word so even the last short store is owned.
        let extra = self.builder.imm(32);
        let capacity = self.builder.checked_add(length, extra);
        let out =
            self.builder.alloc_bytes_object(capacity, AllocationSemantics::SOLIDITY_UNINITIALIZED);
        self.builder.set_memory_object_len(out, length, MemoryObjectKind::Bytes);
        let work = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.branch(n, work, done);
        self.builder.switch_to_block(work);
        let destination = self.builder.memory_object_data(out, MemoryObjectKind::Bytes);
        let thirty_two = self.builder.imm(32);
        let twenty_four = self.builder.imm(24);
        let thirty_one = self.builder.imm(31);
        let rounded = self.builder.add(n, thirty_one);
        let chunks = self.builder.div(rounded, thirty_two);
        self.builder.counted_loop(chunks, |builder, index| {
            let offset = builder.mul(index, thirty_two);
            let remaining = builder.sub(n, offset);
            let short = builder.lt(remaining, thirty_two);
            let count = builder.select(short, remaining, thirty_two);
            let end = builder.add(offset, count);
            let address = builder.add(source, end);
            let word = builder.mload(address);
            let eight = builder.imm(8);
            let missing = builder.sub(thirty_two, count);
            let shift = builder.mul(missing, eight);
            let word = builder.shl(shift, word);
            // Fill missing input lanes with 'A' (sextet zero). Shifting the
            // load to the top first discards bytes preceding this group.
            let bits = builder.mul(count, eight);
            let filler = builder.imm((U256::MAX / U256::from(255)) * U256::from(65));
            let filler = builder.shr(bits, filler);
            let word = builder.or(word, filler);
            let packed = builder.icall(word_helper, vec![word, imap], MirType::I256);
            let offset = builder.mul(index, twenty_four);
            let dest = builder.add(destination, offset);
            let remaining = builder.sub(length, offset);
            let short = builder.lt(remaining, twenty_four);
            let count = builder.select(short, remaining, twenty_four);
            let sixty_four = builder.imm(64);
            let packed = builder.shl(sixty_four, packed);
            let missing = builder.sub(thirty_two, count);
            let bits = builder.mul(missing, eight);
            let all = builder.imm(U256::MAX);
            let mask = builder.shl(bits, all);
            let packed = builder.and(packed, mask);
            builder.mstore(dest, packed);
        });
        self.builder.jump(done);
        self.builder.switch_to_block(done);
        Some(out)
    }
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
