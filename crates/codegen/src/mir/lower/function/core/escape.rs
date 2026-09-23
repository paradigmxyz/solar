//! Lowering for `Strings.escapeHTML`, `Strings.escapeJSON` and
//! `Strings.encodeURIComponent`.
//!
//! Each copies its input byte by byte, replacing the bytes one bit mask
//! selects with short sequences. The output is streamed at the free-memory
//! pointer and reserved with its exact size at the end, so no pass counts the
//! escapes first and no capacity is reserved for the longest possible result.
//! HTML and JSON test a whole word for bytes to escape first and copy it with
//! one store when it has none; a word that has one, and the tail shorter than
//! a word, go byte by byte. The replacement sequences come from small tables
//! written to scratch on entry: for HTML the entities and their lengths
//! indexed by the escaped byte, which is below 0x40; for JSON the `\u00`
//! template, the hexadecimal digits and the short forms of the control bytes;
//! for URIs the uppercase digits. A replacement is written with whole-word or
//! byte stores that may reach past it, and the next write or the zeroed word
//! after the output covers what they leave. The checked Solidity bodies
//! remain the reference under `-Zno-core-intrinsics`.

use super::*;

/// The escaping a helper performs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Escape {
    Html,
    Json,
    Uri,
}

impl Escape {
    fn helper(self) -> Symbol {
        match self {
            Self::Html => sym::core_string_escape_html,
            Self::Json => sym::core_string_escape_json,
            Self::Uri => sym::core_string_uri_component,
        }
    }

    /// One bit per byte value that is replaced.
    fn special(self) -> U256 {
        match self {
            // `"`, `&`, `'`, `<` and `>`.
            Self::Html => [0x22, 0x26, 0x27, 0x3c, 0x3e]
                .into_iter()
                .fold(U256::ZERO, |mask, byte| mask | (U256::from(1) << byte)),
            // The control bytes, `"` and `\`.
            Self::Json => {
                U256::from(0xffff_ffff_u64) | (U256::from(1) << 0x22) | (U256::from(1) << 0x5c)
            }
            // Everything but the letters, the digits and `-_.!~*'()`.
            Self::Uri => !U256::from(0x47ff_fffe_87ff_fffe_03ff_6782_0000_0000_u128),
        }
    }
}

/// `byte` repeated in every lane of a word.
fn spread(byte: u8) -> U256 {
    U256::from(byte) * (U256::MAX / U256::from(255))
}

impl FunctionLowerer<'_, '_> {
    /// Keep each escaping as one shared body per module.
    pub(super) fn lower_core_string_escape_call(
        &mut self,
        operands: &[ValueId],
        escape: Escape,
    ) -> Option<ValueId> {
        let (subject, quotes) = match (escape, operands) {
            (Escape::Json, &[subject, quotes]) => (subject, Some(quotes)),
            (_, &[subject]) => (subject, None),
            _ => return None,
        };
        // A flag known to be false leaves the quotes out of the body entirely.
        let quotes =
            quotes.filter(|&quotes| self.builder.func().value_u256(quotes) != Some(U256::ZERO));
        let quotable = quotes.is_some();
        let name = if quotable { sym::core_string_escape_json_quotable } else { escape.helper() };
        let bytes = MirType::MemoryObject(MemoryObjectKind::Bytes);
        let helper = self.lazy_helper(name, |this, function| {
            let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
            let subject = lowerer.builder.add_param(bytes);
            let quotes = quotable.then(|| lowerer.builder.add_param(MirType::I256));
            lowerer.builder.set_return_type(bytes);
            let out = lowerer.lower_core_string_escape(subject, quotes, escape);
            lowerer.builder.ret([out]);
            Some(())
        })?;
        // The flag travels as a word; the body normalizes it.
        let mut args = vec![subject];
        args.extend(quotes.map(|quotes| self.builder.cast_word(quotes)));
        Some(self.builder.icall(helper, args, bytes))
    }

    /// Streams `subject` with `escape` applied to the free-memory pointer and
    /// reserves it there; JSON output is quoted when `quotes` is set.
    fn lower_core_string_escape(
        &mut self,
        subject: ValueId,
        quotes: Option<ValueId>,
        escape: Escape,
    ) -> ValueId {
        let kind = MemoryObjectKind::Bytes;
        let length = self.builder.memory_object_len(subject, kind);
        let source = self.builder.memory_object_data(subject, kind);
        let end = self.builder.add(source, length);
        let base = self.builder.fmp();
        let header = self.builder.imm(32);
        let destination = self.builder.add(base, header);
        self.store_escape_tables(escape);

        // quoted = quotes != 0; mstore8(destination, '"'); start = destination + quoted
        let quote = self.builder.imm(0x22);
        let quoted = quotes.map(|quotes| {
            let set = self.builder.ne_zero(quotes);
            self.builder.cast_word(set)
        });
        let start = match quoted {
            Some(quoted) => {
                self.builder.mstore8(destination, quote);
                self.builder.add(destination, quoted)
            }
            None => destination,
        };

        let entry = self.builder.current_block();
        let tail = self.builder.create_block();
        let tail_bytes = self.builder.create_block();
        let finish = self.builder.create_block();
        let has_word_test = escape != Escape::Uri;
        // Every byte loop resumes at the word loop, or at the tail without one.
        let resume = if has_word_test { self.builder.create_block() } else { tail };
        let words = resume;
        self.builder.jump(resume);
        self.builder.switch_to_block(resume);
        let cursor = self.builder.phi(vec![(entry, source)]);
        let output = self.builder.phi(vec![(entry, start)]);

        if has_word_test {
            let load = self.builder.create_block();
            let plain = self.builder.create_block();
            let word_bytes = self.builder.create_block();
            // next = p + 32; a whole word remains when next <= end
            let next = self.builder.add(cursor, header);
            let short = self.builder.gt(next, end);
            self.builder.branch(short, tail, load);

            // A word without a byte to escape is copied with one store.
            // mstore(o, w); p = next; o += 32
            self.builder.switch_to_block(load);
            let text = self.builder.mload(cursor);
            let marked = self.escape_marks(escape, text);
            let clean = self.builder.eq_zero(marked);
            self.builder.branch(clean, plain, word_bytes);
            self.builder.switch_to_block(plain);
            self.builder.mstore(output, text);
            let plain_output = self.builder.add(output, header);
            self.builder.jump(words);
            self.builder.add_phi_incoming(cursor, plain, next);
            self.builder.add_phi_incoming(output, plain, plain_output);

            self.builder.switch_to_block(word_bytes);
            let (step, stepped_cursor, stepped_output) =
                self.escape_bytes(escape, cursor, output, next, words);
            self.builder.add_phi_incoming(cursor, step, stepped_cursor);
            self.builder.add_phi_incoming(output, step, stepped_output);
            self.builder.switch_to_block(tail);
        }

        // if p < end: the rest byte by byte
        let more = self.builder.lt(cursor, end);
        self.builder.branch(more, tail_bytes, finish);
        self.builder.switch_to_block(tail_bytes);
        let (step, stepped_cursor, stepped_output) =
            self.escape_bytes(escape, cursor, output, end, resume);
        self.builder.add_phi_incoming(cursor, step, stepped_cursor);
        self.builder.add_phi_incoming(output, step, stepped_output);

        // mstore8(o, '"'); o += quotes; length = o - destination; mstore(o, 0)
        self.builder.switch_to_block(finish);
        let finished = match quoted {
            Some(quoted) => {
                self.builder.mstore8(output, quote);
                self.builder.add(output, quoted)
            }
            None => output,
        };
        let output_length = self.builder.sub(finished, destination);
        let zero = self.builder.imm(0);
        self.builder.mstore(finished, zero);
        // The output was written up to `finished`, so its padded size cannot wrap.
        let size = self.builder.padded_size(output_length);
        let out = self.builder.alloc_object(
            size,
            MemoryObjectLayout::Bytes,
            AllocationSemantics::INTERNAL,
        );
        let Value::Inst(allocation) = *self.builder.func().value(out) else {
            unreachable!("allocation result must reference its instruction")
        };
        self.builder.func_mut().inst_mut(allocation).metadata.set_preserves_fmp(true);
        self.builder.set_memory_object_len(out, output_length, kind);
        out
    }

    /// Writes the replacement tables to scratch.
    fn store_escape_tables(&mut self, escape: Escape) {
        match escape {
            // Stride and offset of each entity, `stride << 5 | offset`, at the
            // address of the byte it replaces, and the entities from 0x00:
            // "&quot;&amp;&#39;&lt;&gt;".
            Escape::Html => {
                for (address, word) in [
                    (0x1f_u64, U256::from(0x90_0094_u64)),
                    (0x08, U256::from(0xc000_0000_a6ab_u64)),
                    (0x00, U256::from_be_slice(b"&quot;&amp;&#39;&lt;&gt;") << 64),
                ] {
                    let address = self.builder.imm(address);
                    let word = self.builder.imm(word);
                    self.builder.mstore(address, word);
                }
            }
            // "\u0000" at 0x19, the digits "0123456789abcdef" at 0x1f and the
            // short forms "btn\0fr" of 0x08 through 0x0d at 0x2f.
            Escape::Json => {
                let address = self.builder.imm(0x15);
                let word = self.builder.imm(U256::from_be_slice(b"\\u00000123456789abcdefbtn\0fr"));
                self.builder.mstore(address, word);
            }
            // "0123456789ABCDEF" at 0x1f.
            Escape::Uri => {
                let address = self.builder.imm(0x0f);
                let word = self.builder.imm(U256::from_be_slice(b"0123456789ABCDEF"));
                self.builder.mstore(address, word);
            }
        }
    }

    /// A nonzero word when some byte of `text` must be escaped. A lane is
    /// zero after xor with a byte it equals, or with a pair of bytes it may
    /// equal once the bit they differ in is cleared; a zero lane borrows when
    /// one is taken from it, leaving a top bit it did not have. Borrows reach
    /// only lanes above a zero one, so the word is zero exactly when no lane
    /// matched.
    fn escape_marks(&mut self, escape: Escape, text: ValueId) -> ValueId {
        let ones = self.builder.imm(spread(1));
        let high_bits = self.builder.imm(spread(0x80));
        let mut marks = Vec::new();
        let zero_lanes = |this: &mut Self, lanes: ValueId| {
            // (x - 0x0101..01) & ~x
            let borrowed = this.builder.sub(lanes, ones);
            let inverted = this.builder.not(lanes);
            this.builder.and(borrowed, inverted)
        };
        match escape {
            // w ^ '"'; (w ^ '&') & ~1 for `&` and `'`; (w ^ '<') & ~2 for `<` and `>`
            Escape::Html => {
                for (byte, pair) in [(0x22, 0xff), (0x26, 0xfe), (0x3c, 0xfd)] {
                    let target = self.builder.imm(spread(byte));
                    let lanes = self.builder.xor(text, target);
                    let lanes = if pair == 0xff {
                        lanes
                    } else {
                        let pair = self.builder.imm(spread(pair));
                        self.builder.and(lanes, pair)
                    };
                    marks.push(zero_lanes(self, lanes));
                }
            }
            // (w - 0x2020..20) & ~w marks control bytes; w ^ '"' and w ^ '\'
            Escape::Json => {
                let space = self.builder.imm(spread(0x20));
                let below = self.builder.sub(text, space);
                let inverted = self.builder.not(text);
                marks.push(self.builder.and(below, inverted));
                for byte in [0x22, 0x5c] {
                    let target = self.builder.imm(spread(byte));
                    let lanes = self.builder.xor(text, target);
                    marks.push(zero_lanes(self, lanes));
                }
            }
            Escape::Uri => unreachable!("URI components have no word test"),
        }
        let marked = marks
            .into_iter()
            .reduce(|left, right| self.builder.or(left, right))
            .expect("every word test marks something");
        self.builder.and(marked, high_bits)
    }

    /// Escapes the bytes from `start` until `bound`, then jumps to `exit`.
    /// Returns the loop's latch and the cursor and output it leaves with.
    fn escape_bytes(
        &mut self,
        escape: Escape,
        start: ValueId,
        output: ValueId,
        bound: ValueId,
        exit: BlockId,
    ) -> (BlockId, ValueId, ValueId) {
        let entry = self.builder.current_block();
        let step = self.builder.create_block();
        let copy = self.builder.create_block();
        let replace = self.builder.create_block();
        let join = self.builder.create_block();
        self.builder.jump(step);

        // c = byte(0, mload(q)); replaced when (special >> c) & 1
        self.builder.switch_to_block(step);
        let cursor = self.builder.phi(vec![(entry, start)]);
        let written = self.builder.phi(vec![(entry, output)]);
        let zero = self.builder.imm(0);
        let text = self.builder.mload(cursor);
        let byte = self.builder.byte(zero, text);
        let special = self.builder.imm(escape.special());
        let selected = self.builder.shr(byte, special);
        let one = self.builder.imm(1);
        let selected = self.builder.and(selected, one);
        let replaced = self.builder.ne(selected, zero);
        self.builder.branch(replaced, replace, copy);

        // mstore8(o, c); o += 1
        self.builder.switch_to_block(copy);
        self.builder.mstore8(written, byte);
        let copied = self.builder.add(written, one);
        self.builder.jump(join);

        self.builder.switch_to_block(replace);
        let escaped = self.write_escape(escape, byte, written, join);

        // q += 1; continue while q < bound
        self.builder.switch_to_block(join);
        let mut incoming = vec![(copy, copied)];
        incoming.extend(escaped);
        let next_written = self.builder.phi(incoming);
        let next_cursor = self.builder.add(cursor, one);
        let within = self.builder.lt(next_cursor, bound);
        self.builder.branch(within, step, exit);
        self.builder.add_phi_incoming(cursor, join, next_cursor);
        self.builder.add_phi_incoming(written, join, next_written);
        (join, next_cursor, next_written)
    }

    /// Writes the replacement of `byte` at `output` and jumps to `join`.
    /// Returns each predecessor of `join` it adds with the output after it.
    fn write_escape(
        &mut self,
        escape: Escape,
        byte: ValueId,
        output: ValueId,
        join: BlockId,
    ) -> Vec<(BlockId, ValueId)> {
        let zero = self.builder.imm(0);
        match escape {
            // t = byte(0, mload(c)); mstore(o, mload(t & 31)); o += t >> 5
            Escape::Html => {
                let packed = self.builder.mload(byte);
                let packed = self.builder.byte(zero, packed);
                let thirty_one = self.builder.imm(31);
                let offset = self.builder.and(packed, thirty_one);
                let entity = self.builder.mload(offset);
                self.builder.mstore(output, entity);
                let five = self.builder.imm(5);
                let stride = self.builder.shr(five, packed);
                let after = self.builder.add(output, stride);
                let block = self.builder.current_block();
                self.builder.jump(join);
                vec![(block, after)]
            }
            Escape::Json => {
                let quoted = self.builder.create_block();
                let control = self.builder.create_block();
                let short = self.builder.create_block();
                let long = self.builder.create_block();
                let backslash = self.builder.imm(0x5c);
                let two = self.builder.imm(2);
                let one = self.builder.imm(1);
                let space = self.builder.imm(0x20);
                let control_byte = self.builder.lt(byte, space);
                self.builder.branch(control_byte, control, quoted);

                // mstore8(o, '\'); mstore8(o + 1, c); o += 2
                self.builder.switch_to_block(quoted);
                self.builder.mstore8(output, backslash);
                let second = self.builder.add(output, one);
                self.builder.mstore8(second, byte);
                let after_quoted = self.builder.add(output, two);
                self.builder.jump(join);

                // `\b`, `\t`, `\n`, `\f` and `\r` have short forms.
                self.builder.switch_to_block(control);
                let short_forms = self.builder.imm(0x3700);
                let has_short = self.builder.shr(byte, short_forms);
                let has_short = self.builder.and(has_short, one);
                let has_short = self.builder.ne(has_short, zero);
                self.builder.branch(has_short, short, long);

                // mstore8(o, '\'); mstore8(o + 1, mload(c + 8)); o += 2
                self.builder.switch_to_block(short);
                self.builder.mstore8(output, backslash);
                let eight = self.builder.imm(8);
                let letter = self.builder.add(byte, eight);
                let letter = self.builder.mload(letter);
                let second = self.builder.add(output, one);
                self.builder.mstore8(second, letter);
                let after_short = self.builder.add(output, two);
                self.builder.jump(join);

                // mstore8(0x1d, mload(c >> 4)); mstore8(0x1e, mload(c & 15))
                // mstore(o, mload(0x19)); o += 6
                self.builder.switch_to_block(long);
                let four = self.builder.imm(4);
                let high = self.builder.shr(four, byte);
                let high = self.builder.mload(high);
                let high_at = self.builder.imm(0x1d);
                self.builder.mstore8(high_at, high);
                let fifteen = self.builder.imm(15);
                let low = self.builder.and(byte, fifteen);
                let low = self.builder.mload(low);
                let low_at = self.builder.imm(0x1e);
                self.builder.mstore8(low_at, low);
                let template = self.builder.imm(0x19);
                let template = self.builder.mload(template);
                self.builder.mstore(output, template);
                let six = self.builder.imm(6);
                let after_long = self.builder.add(output, six);
                self.builder.jump(join);

                vec![(quoted, after_quoted), (short, after_short), (long, after_long)]
            }
            // mstore8(o, '%'); mstore8(o + 1, mload(c >> 4)); mstore8(o + 2, mload(c & 15)); o += 3
            Escape::Uri => {
                let percent = self.builder.imm(0x25);
                self.builder.mstore8(output, percent);
                let four = self.builder.imm(4);
                let high = self.builder.shr(four, byte);
                let high = self.builder.mload(high);
                let one = self.builder.imm(1);
                let second = self.builder.add(output, one);
                self.builder.mstore8(second, high);
                let fifteen = self.builder.imm(15);
                let low = self.builder.and(byte, fifteen);
                let low = self.builder.mload(low);
                let two = self.builder.imm(2);
                let third = self.builder.add(output, two);
                self.builder.mstore8(third, low);
                let three = self.builder.imm(3);
                let after = self.builder.add(output, three);
                let block = self.builder.current_block();
                self.builder.jump(join);
                vec![(block, after)]
            }
        }
    }
}
