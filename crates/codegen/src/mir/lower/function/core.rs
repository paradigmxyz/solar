//! Lowering for the compiler-owned `solar:core/` modules.
//!
//! A function declared in one of those modules is identified by the module it
//! comes from and its name, never by name alone, so an ordinary function
//! spelled `readBytes4` is still an ordinary function. Each entry point here
//! lowers to word operations directly instead of calling the body that ships
//! with the module; `-Zno-core-intrinsics` calls the body instead, which is
//! how the two are compared.
//!
//! Bounded byte operations check that the full range fits before writing
//! and raise the same `Panic(0x32)` the body raises. Fixed-width reads and
//! writes touch the word containing the range, which can reach up to
//! thirty-one bytes past it; a write puts those bytes back unchanged, so the
//! difference is visible only as memory expansion and `msize`, never as a
//! value. `copyInto` is a move by contract, which is what `mcopy` is, so its
//! two ranges need no disjointness proof; `truncate` is a store to the length
//! word, which the alias and value-numbering analyses already model.
//!
//! The `WordArrays` set operations merge with one branch per step and move the
//! remaining tail with `mcopy`. Their bodies' index and truncation checks
//! cannot fail, because every committed element consumes an input element and
//! the output holds as many as the inputs can supply, so the lowering leaves
//! them out; the output allocation keeps the body's size and panics.
//! `WordArrays.copy` allocates like the body's `new` and moves the length word
//! and the elements with one `mcopy`, instead of zeroing the words it then
//! overwrites one at a time.
//!
//! A storage reference argument is passed as its slot, as to any internal
//! call. `Slots` hashes a root's slot the way a dynamic array's data slot is
//! hashed and checks the index after the hash, so a loop over one region can
//! hoist the hash. Its byte operations check the range and count once, hash
//! once, and move whole words in a loop before the last partial word, which a
//! store masks and a load merges into the bytes around it. `Return.abiEncoded`
//! encodes the string where it lies, since the call ends before memory is read
//! again.

use super::*;
use solar_sema::core::CoreIntrinsic;

mod base64;
mod escape;
mod hex;
mod strings;

/// The merges of two sorted word arrays that `WordArrays` provides.
#[derive(Clone, Copy)]
enum SetOperation {
    Union,
    Intersection,
    Difference,
}

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    /// Returns the intrinsic `function_id` names, when it is one and intrinsic
    /// lowering is enabled.
    pub(super) fn core_intrinsic(&self, function_id: hir::FunctionId) -> Option<CoreIntrinsic> {
        if self.cx.gcx.sess.opts.unstable.no_core_intrinsics {
            return None;
        }
        let intrinsic = solar_sema::core::intrinsic_of(self.cx.gcx, function_id)?;
        // Without the instruction the shipped body is the implementation.
        // `trailingZeros` has a lookup of its own that needs no `clz`.
        let needs_clz =
            matches!(intrinsic, CoreIntrinsic::LeadingZeros | CoreIntrinsic::HighestSetBit);
        if needs_clz && !self.cx.gcx.sess.opts.evm_version.has_clz() {
            return None;
        }
        Some(intrinsic)
    }

    /// Lowers a call to a compiler-owned module function.
    pub(super) fn lower_core_intrinsic_call(
        &mut self,
        expr: &hir::Expr<'_>,
        intrinsic: CoreIntrinsic,
        function_id: hir::FunctionId,
        receiver: Option<&hir::Expr<'_>>,
        args: hir::CallArgs<'_>,
    ) -> Option<ValueId> {
        let function = self.cx.gcx.hir.function(function_id);
        // The receiver of `using Bytes for bytes` is the first parameter, so
        // both spellings reach the same operand list and the same operation.
        let mut operands = Vec::with_capacity(function.parameters.len());
        let mut parameter_tys = Vec::with_capacity(function.parameters.len());
        let exprs = receiver.into_iter().chain(args.exprs());
        for (index, argument) in exprs.enumerate() {
            let parameter = *function.parameters.get(index)?;
            let parameter_ty = self.cx.gcx.type_of_item(parameter.into());
            // A storage reference is passed as its slot, as to any internal call.
            let value = if Self::is_storage_parameter(parameter_ty) {
                let Some(access) = self.storage_access(argument) else {
                    return self.cx.report_unsupported(argument.span, "storage access");
                };
                access.slot
            } else {
                let value = self.lower_typed_expr(argument, parameter_ty)?;
                self.materialize_call_argument(parameter_ty, value, argument.span)?
            };
            operands.push(value);
            parameter_tys.push(parameter_ty);
        }
        if operands.len() != function.parameters.len() {
            return self.cx.report_unsupported(expr.span, "compiler module argument list");
        }

        match intrinsic {
            CoreIntrinsic::Base64Encode => self.lower_core_base64_encode_call(&operands),
            CoreIntrinsic::Base64Decode => self.lower_core_base64_decode(&operands),
            CoreIntrinsic::ReadBytes(width) => self.lower_core_read(&operands, width),
            CoreIntrinsic::ReadUint256Be => self.lower_core_read(&operands, 32),
            CoreIntrinsic::WriteBytes(width) => self.lower_core_write(&operands, width),
            CoreIntrinsic::WriteUint256Be => self.lower_core_write(&operands, 32),
            CoreIntrinsic::CopyInto => self.lower_core_copy(&operands),
            CoreIntrinsic::Fill => self.lower_core_fill(&operands),
            CoreIntrinsic::EqualsAt => self.lower_core_equals_at(&operands),
            CoreIntrinsic::Truncate => self.lower_core_truncate(expr, &operands, &parameter_tys),
            CoreIntrinsic::ArrayGroupSum => self.lower_core_array_group_sum_call(&operands),
            CoreIntrinsic::ArrayHasDuplicate => self.lower_core_array_has_duplicate_call(&operands),
            CoreIntrinsic::ArraySort => self.lower_core_array_sort_call(&operands, &parameter_tys),
            CoreIntrinsic::ArrayUniquifySorted => {
                self.lower_core_array_uniquify_sorted_call(&operands)
            }
            CoreIntrinsic::ArrayUnion => {
                self.lower_core_array_set_call(&operands, &parameter_tys, SetOperation::Union)
            }
            CoreIntrinsic::ArrayIntersection => self.lower_core_array_set_call(
                &operands,
                &parameter_tys,
                SetOperation::Intersection,
            ),
            CoreIntrinsic::ArrayDifference => {
                self.lower_core_array_set_call(&operands, &parameter_tys, SetOperation::Difference)
            }
            CoreIntrinsic::ArrayCopy => self.lower_core_array_copy_call(&operands, &parameter_tys),
            CoreIntrinsic::StringReplace => self.lower_core_string_replace_call(&operands),
            CoreIntrinsic::StringIndicesOf => self.lower_core_string_indices_of_call(&operands),
            CoreIntrinsic::StringSplit => self.lower_core_string_split_call(&operands),
            CoreIntrinsic::StringIndexOf => self.lower_core_string_index_of_call(&operands, false),
            CoreIntrinsic::StringLastIndexOf => {
                self.lower_core_string_index_of_call(&operands, true)
            }
            CoreIntrinsic::StringRuneCount => self.lower_core_string_rune_count_call(&operands),
            CoreIntrinsic::StringRepeat => self.lower_core_string_repeat_call(&operands),
            CoreIntrinsic::StringToString => {
                self.lower_core_string_to_string_call(&operands, &parameter_tys)
            }
            CoreIntrinsic::StringEscapeHTML => {
                self.lower_core_string_escape_call(&operands, escape::Escape::Html)
            }
            CoreIntrinsic::StringEscapeJSON => {
                self.lower_core_string_escape_call(&operands, escape::Escape::Json)
            }
            CoreIntrinsic::StringEncodeURIComponent => {
                self.lower_core_string_escape_call(&operands, escape::Escape::Uri)
            }
            CoreIntrinsic::StringMinimalHex => {
                self.lower_core_string_minimal_hex_call(&operands, false)
            }
            CoreIntrinsic::StringMinimalHexPrefixed => {
                self.lower_core_string_minimal_hex_call(&operands, true)
            }
            CoreIntrinsic::StringHex => self.lower_core_string_hex_call(&operands, false),
            CoreIntrinsic::StringHexPrefixed => self.lower_core_string_hex_call(&operands, true),
            CoreIntrinsic::HexEncode => self.lower_core_hex_encode_call(&operands, false),
            CoreIntrinsic::HexEncodePrefixed => self.lower_core_hex_encode_call(&operands, true),
            CoreIntrinsic::StringPackOne => self.lower_core_string_pack_one_call(&operands),
            CoreIntrinsic::StringUnpackOne => self.lower_core_string_unpack_one_call(&operands),
            CoreIntrinsic::StringPackTwo => self.lower_core_string_pack_two_call(&operands),
            CoreIntrinsic::StringUnpackTwo => {
                let values = self.lower_core_string_unpack_two_call(&operands)?;
                let return_tys = function
                    .returns
                    .iter()
                    .map(|&id| self.cx.gcx.type_of_item(id.into()))
                    .collect::<Vec<_>>();
                Some(self.pack_return_values(values, &return_tys))
            }
            CoreIntrinsic::RevertRaw => self.lower_core_revert_raw(&operands),
            CoreIntrinsic::ReturnAbiEncoded => self.lower_core_return_abi_encoded(&operands),
            CoreIntrinsic::SlotsLoad => self.lower_core_slots_load(&operands),
            CoreIntrinsic::SlotsStore => self.lower_core_slots_store(&operands),
            CoreIntrinsic::SlotsStoreBytes => self.lower_core_slots_store_bytes(&operands, false),
            CoreIntrinsic::SlotsStoreCalldataBytes => {
                self.lower_core_slots_store_bytes(&operands, true)
            }
            CoreIntrinsic::SlotsLoadBytes => self.lower_core_slots_load_bytes(&operands),
            CoreIntrinsic::Keccak256Range => self.lower_core_keccak256_range(&operands),
            CoreIntrinsic::Deploy | CoreIntrinsic::Deploy2 => {
                self.lower_core_deploy(intrinsic, &operands)
            }
            CoreIntrinsic::TryDeploy | CoreIntrinsic::TryDeploy2 => {
                self.lower_core_try_deploy(intrinsic, function_id, &operands)
            }
            CoreIntrinsic::TryDeployInto => self.lower_core_try_deploy_into(function_id, &operands),
            CoreIntrinsic::TryReadBytes(width) => {
                self.lower_core_try_read(function_id, &operands, width)
            }
            CoreIntrinsic::TryReadUint256Be => self.lower_core_try_read(function_id, &operands, 32),
            CoreIntrinsic::CalldataReadBytes(width) => {
                self.lower_core_calldata_read(&operands, width)
            }
            CoreIntrinsic::CalldataReadUint256Be => self.lower_core_calldata_read(&operands, 32),
            CoreIntrinsic::CalldataCopyInto => self.lower_core_calldata_copy(&operands),
            CoreIntrinsic::CalldataTryReadBytes(width) => {
                self.lower_core_calldata_try_read(function_id, &operands, width)
            }
            CoreIntrinsic::CalldataTryReadUint256Be => {
                self.lower_core_calldata_try_read(function_id, &operands, 32)
            }
            CoreIntrinsic::CodeCopyInto => self.lower_core_code_copy(&operands),
            CoreIntrinsic::LeadingZeros => {
                let [value] = *operands.as_slice() else { return None };
                Some(self.builder.clz(value))
            }
            CoreIntrinsic::HighestSetBit => {
                let [value] = *operands.as_slice() else { return None };
                Some(self.core_highest_set_bit(value))
            }
            CoreIntrinsic::TrailingZeros => {
                let [value] = *operands.as_slice() else { return None };
                // lowest = value & (0 - value)
                let zero = self.builder.imm(U256::ZERO);
                let negated = self.builder.sub(zero, value);
                let lowest = self.builder.and(value, negated);
                if self.cx.gcx.sess.opts.evm_version.has_clz() {
                    Some(self.core_highest_set_bit(lowest))
                } else {
                    Some(self.core_lowest_bit_index(lowest))
                }
            }
            CoreIntrinsic::CallInto
            | CoreIntrinsic::StaticCallInto
            | CoreIntrinsic::DelegateCallInto => {
                self.lower_core_call_into(intrinsic, function_id, &operands)
            }
            CoreIntrinsic::Mul512 => self.lower_core_mul512(function_id, &operands),
            CoreIntrinsic::WrappingAdd
            | CoreIntrinsic::WrappingSub
            | CoreIntrinsic::WrappingMul => {
                let [x, y] = *operands.as_slice() else { return None };
                // result = add|sub|mul(x, y)
                Some(match intrinsic {
                    CoreIntrinsic::WrappingAdd => self.builder.add(x, y),
                    CoreIntrinsic::WrappingSub => self.builder.sub(x, y),
                    _ => self.builder.mul(x, y),
                })
            }
        }
    }

    /// The index of the single set bit of `bit`, or 256 when it is zero,
    /// without `clz`. A De Bruijn-like product's top six bits select the
    /// packed nibble holding the index's top three bits (eight for zero); a
    /// De Bruijn division inside that bit's 32-bit window gives the low five.
    /// EVM division by zero yields zero, which the zero input relies on.
    fn core_lowest_bit_index(&mut self, bit: ValueId) -> ValueId {
        // high = ((NIBBLES << ((bit * SPREAD >> 250) << 2)) >> 252) << 5
        // low = byte((0xd76453e0 / (bit >> high)) & 31, LOW)
        // index = high | low
        let spread = self.builder.imm(
            U256::from_str_radix(
                "b6db6db6ddddddddd34d34d349249249210842108c6318c639ce739cffffffff",
                16,
            )
            .unwrap(),
        );
        let product = self.builder.mul(bit, spread);
        let top_six = self.builder.imm(250);
        let slot = self.builder.shr(top_six, product);
        let two = self.builder.imm(2);
        let slot_bits = self.builder.shl(two, slot);
        let nibbles = self.builder.imm(
            U256::from_str_radix(
                "8040405543005266443200005020610674053026020000107506200176117077",
                16,
            )
            .unwrap(),
        );
        let selected = self.builder.shl(slot_bits, nibbles);
        let top_nibble = self.builder.imm(252);
        let high = self.builder.shr(top_nibble, selected);
        let five = self.builder.imm(5);
        let high = self.builder.shl(five, high);
        let window = self.builder.shr(high, bit);
        let de_bruijn = self.builder.imm(0xd764_53e0_u64);
        let quotient = self.builder.div(de_bruijn, window);
        let thirty_one = self.builder.imm(31);
        let slot = self.builder.and(quotient, thirty_one);
        let low_table = self.builder.imm(
            U256::from_str_radix(
                "001f0d1e100c1d070f090b19131c1706010e11080a1a141802121b1503160405",
                16,
            )
            .unwrap(),
        );
        let low = self.builder.byte(slot, low_table);
        self.builder.or(high, low)
    }

    /// The index of the highest set bit of `value`, 256 for zero. A count of
    /// leading zeros is at most 255 for a non-zero word, where exclusive-or
    /// with 255 subtracts it; zero counts 256, which that turns into 511, and
    /// the second term brings back to 256.
    fn core_highest_set_bit(&mut self, value: ValueId) -> ValueId {
        // index = (255 ^ clz(value)) ^ (255 * zext(value == 0))
        let count = self.builder.clz(value);
        let top = self.builder.imm(U256::from(255));
        let index = self.builder.xor(top, count);
        let is_zero = self.builder.eq_zero(value);
        let is_zero = self.builder.cast_word(is_zero);
        let fix = self.builder.mul(top, is_zero);
        self.builder.xor(index, fix)
    }

    /// Lowers every supported one-word array overload to one shared helper.
    /// ABI decoding has already validated and canonicalized each element, so
    /// equality can compare the words without retaining the nominal type.
    fn lower_core_array_has_duplicate_call(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let [input] = *operands else { return None };
        let helper =
            self.lazy_helper(Symbol::intern("core_array_has_duplicate"), |this, function| {
                function.attributes.no_inline = true;
                let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
                let ty = MirType::MemoryObject(MemoryObjectKind::DynamicArray);
                let input = lowerer.builder.add_param(ty);
                lowerer.builder.set_return_type(MirType::I1);
                lowerer.lower_core_array_has_duplicate(input);
                Some(())
            })?;
        Some(self.builder.icall(helper, vec![input], MirType::I1))
    }

    /// Implements an open-addressed set over the array's canonical words.
    /// Slots hold non-zero input addresses so zero remains the empty marker.
    /// The temporary table is compiler-owned and does not escape. Up to six
    /// words compare every pair instead: without a duplicate that is at most
    /// fifteen comparisons, which cost less than sizing, clearing and filling
    /// a table, and nothing is allocated.
    fn lower_core_array_has_duplicate(&mut self, input: ValueId) {
        let two = self.builder.imm(2);
        let length = self.builder.memory_object_len(input, MemoryObjectKind::DynamicArray);
        let small = self.builder.lt(length, two);
        let no_duplicate = self.builder.create_block();
        let sized = self.builder.create_block();
        self.builder.branch(small, no_duplicate, sized);

        self.builder.switch_to_block(no_duplicate);
        let false_ = self.builder.imm_bool(false);
        self.builder.ret([false_]);

        // branch (lt length, 7), pairwise, allocate
        self.builder.switch_to_block(sized);
        let seven = self.builder.imm(7);
        let few = self.builder.lt(length, seven);
        let pairwise = self.builder.create_block();
        let allocate = self.builder.create_block();
        self.builder.branch(few, pairwise, allocate);

        self.builder.switch_to_block(pairwise);
        self.lower_core_array_pairwise_duplicate(input, length);

        self.builder.switch_to_block(allocate);
        // Round 48 * length up to a power-of-two byte extent, then clear the
        // low five bits. `mask` selects one of its 32-byte table slots.
        let forty_eight = self.builder.imm(48);
        let mut mask = self.builder.checked_mul(length, forty_eight);
        for shift in [1, 2, 4, 8, 16, 32, 64, 128] {
            let shift = self.builder.imm(shift);
            let high = self.builder.shr(shift, mask);
            mask = self.builder.or(mask, high);
        }
        let word_mask = self.builder.imm(U256::MAX << 5);
        mask = self.builder.and(mask, word_mask);
        let word_size = self.builder.imm(32);
        let table_size = self.builder.checked_add(mask, word_size);
        let table = self.builder.alloc_raw(table_size, AllocationSemantics::SOLIDITY_ZEROED);
        let table = self.builder.cast(table, MirType::I256);
        let data = self.builder.memory_object_data(input, MemoryObjectKind::DynamicArray);
        let data = self.builder.cast(data, MirType::I256);
        let five = self.builder.imm(5);
        let byte_length = self.builder.shl(five, length);
        let end = self.builder.add(data, byte_length);
        let outer_header = self.builder.create_block();
        let outer_body = self.builder.create_block();
        let outer_done = self.builder.create_block();
        self.builder.jump(outer_header);

        self.builder.switch_to_block(outer_header);
        let cursor = self.builder.phi(vec![(allocate, end)]);
        let more = self.builder.gt(cursor, data);
        self.builder.branch(more, outer_body, outer_done);

        self.builder.switch_to_block(outer_done);
        let false_ = self.builder.imm_bool(false);
        self.builder.ret([false_]);

        self.builder.switch_to_block(outer_body);
        let element_address = self.builder.sub(cursor, word_size);
        let value = self.builder.mload(element_address);
        let hash_multiplier = self
            .builder
            .imm(U256::from_str_radix("100000000000000000000000000000051", 16).unwrap());
        let hash_modulus = self.builder.imm(!U256::from(0xbcu64));
        let hash = self.builder.mulmod(value, hash_multiplier, hash_modulus);
        let initial_slot = self.builder.and(hash, mask);
        let probe_header = self.builder.create_block();
        let insert = self.builder.create_block();
        let compare = self.builder.create_block();
        let collision = self.builder.create_block();
        let found = self.builder.create_block();
        self.builder.jump(probe_header);

        self.builder.switch_to_block(probe_header);
        let slot = self.builder.phi(vec![(outer_body, initial_slot)]);
        let table_address = self.builder.add(table, slot);
        let previous_address = self.builder.mload(table_address);
        let empty = self.builder.eq_zero(previous_address);
        self.builder.branch(empty, insert, compare);

        self.builder.switch_to_block(insert);
        self.builder.mstore(table_address, element_address);
        self.builder.jump(outer_header);
        self.builder.add_phi_incoming(cursor, insert, element_address);

        self.builder.switch_to_block(compare);
        let previous_value = self.builder.mload(previous_address);
        let equal = self.builder.eq(previous_value, value);
        self.builder.branch(equal, found, collision);

        self.builder.switch_to_block(collision);
        let next_slot = self.builder.add(slot, word_size);
        let next_slot = self.builder.and(next_slot, mask);
        self.builder.jump(probe_header);
        self.builder.add_phi_incoming(slot, collision, next_slot);

        self.builder.switch_to_block(found);
        let true_ = self.builder.imm_bool(true);
        self.builder.ret([true_]);
    }

    /// Compares each word of a short array with every word after it. The caller
    /// established at least two words, so every word but the last has a
    /// successor and both loops test at the bottom.
    fn lower_core_array_pairwise_duplicate(&mut self, input: ValueId, length: ValueId) {
        // data = array data
        // end = data + (length << 5)
        // last = end - 32
        let data = self.builder.memory_object_data(input, MemoryObjectKind::DynamicArray);
        let data = self.builder.cast(data, MirType::I256);
        let five = self.builder.imm(5);
        let byte_length = self.builder.shl(five, length);
        let end = self.builder.add(data, byte_length);
        let word_size = self.builder.imm(32);
        let last = self.builder.sub(end, word_size);
        let entry = self.builder.current_block();
        let outer = self.builder.create_block();
        let inner = self.builder.create_block();
        let inner_next = self.builder.create_block();
        let outer_next = self.builder.create_block();
        let done = self.builder.create_block();
        let found = self.builder.create_block();
        self.builder.jump(outer);

        // outer:
        //   cursor = phi [entry: data], [outer_next: after]
        //   value = mload cursor
        //   after = cursor + 32
        self.builder.switch_to_block(outer);
        let cursor = self.builder.phi(vec![(entry, data)]);
        let value = self.builder.mload(cursor);
        let after = self.builder.add(cursor, word_size);
        self.builder.jump(inner);

        // inner:
        //   other = phi [outer: after], [inner_next: next_other]
        //   branch (eq (mload other), value), found, inner_next
        self.builder.switch_to_block(inner);
        let other = self.builder.phi(vec![(outer, after)]);
        let other_value = self.builder.mload(other);
        let equal = self.builder.eq(other_value, value);
        self.builder.branch(equal, found, inner_next);

        // inner_next:
        //   next_other = other + 32
        //   branch (lt next_other, end), inner, outer_next
        self.builder.switch_to_block(inner_next);
        let next_other = self.builder.add(other, word_size);
        let more_others = self.builder.lt(next_other, end);
        self.builder.branch(more_others, inner, outer_next);
        self.builder.add_phi_incoming(other, inner_next, next_other);

        // outer_next: branch (lt after, last), outer, done
        self.builder.switch_to_block(outer_next);
        let more_cursors = self.builder.lt(after, last);
        self.builder.branch(more_cursors, outer, done);
        self.builder.add_phi_incoming(cursor, outer_next, after);

        self.builder.switch_to_block(done);
        let false_ = self.builder.imm_bool(false);
        self.builder.ret([false_]);

        self.builder.switch_to_block(found);
        let true_ = self.builder.imm_bool(true);
        self.builder.ret([true_]);
    }

    /// Lowers every supported one-word array overload to one compaction loop.
    fn lower_core_array_uniquify_sorted_call(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let [input] = *operands else { return None };
        let helper =
            self.lazy_helper(Symbol::intern("core_array_uniquify_sorted"), |this, function| {
                function.attributes.no_inline = true;
                function.attributes.preserves_array_elements = true;
                let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
                let input = lowerer
                    .builder
                    .add_param(MirType::MemoryObject(MemoryObjectKind::DynamicArray));
                lowerer.lower_core_array_uniquify_sorted(input);
                Some(())
            })?;
        self.builder.icall_void(helper, vec![input]);
        Some(self.builder.imm(U256::ZERO))
    }

    /// Compacts adjacent equal words and shortens the array in place.
    ///
    /// The store is unconditional: before the first duplicate it stores a word
    /// back to its own address, and after a duplicate the next distinct word
    /// overwrites that uncommitted slot. This removes the inner branch without
    /// changing which elements survive.
    fn lower_core_array_uniquify_sorted(&mut self, input: ValueId) {
        let kind = MemoryObjectKind::DynamicArray;
        let length = self.builder.memory_object_len(input, kind);
        let two = self.builder.imm(2);
        let small = self.builder.lt(length, two);
        let done = self.builder.create_block();
        let setup = self.builder.create_block();
        self.builder.branch(small, done, setup);

        self.builder.switch_to_block(done);
        self.builder.ret([]);

        self.builder.switch_to_block(setup);
        let data = self.builder.memory_object_data(input, kind);
        let data = self.builder.cast(data, MirType::I256);
        let word = self.builder.imm(32);
        let first = self.builder.add(data, word);
        let previous = self.builder.mload(data);
        let five = self.builder.imm(5);
        let byte_length = self.builder.shl(five, length);
        let end = self.builder.add(data, byte_length);
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let finish = self.builder.create_block();
        self.builder.jump(header);

        self.builder.switch_to_block(header);
        let read = self.builder.phi(vec![(setup, first)]);
        let write = self.builder.phi(vec![(setup, first)]);
        let previous = self.builder.phi(vec![(setup, previous)]);
        let more = self.builder.lt(read, end);
        self.builder.branch(more, body, finish);

        self.builder.switch_to_block(body);
        let value = self.builder.mload(read);
        let distinct = self.builder.ne(value, previous);
        let distinct = self.builder.cast_word(distinct);
        self.builder.mstore(write, value);
        let advance = self.builder.shl(five, distinct);
        let next_write = self.builder.add(write, advance);
        let next_read = self.builder.add(read, word);
        self.builder.jump(header);
        self.builder.add_phi_incoming(read, body, next_read);
        self.builder.add_phi_incoming(write, body, next_write);
        self.builder.add_phi_incoming(previous, body, value);

        self.builder.switch_to_block(finish);
        let compacted_bytes = self.builder.sub(write, data);
        let compacted_length = self.builder.shr(five, compacted_bytes);
        self.builder.set_memory_object_len(input, compacted_length, kind);
        self.builder.ret([]);
    }

    /// Lowers every overload of one set operation to a shared helper, one for
    /// signed and one for unsigned words: addresses and `bytes32` values
    /// compare as unsigned words, like the body's `>`. Addresses still get a
    /// helper of their own: element cleanup bounds a helper's result by the
    /// widest array any call site passes it, so sharing one with full-word
    /// arrays would lose the proof that the returned addresses are clean.
    fn lower_core_array_set_call(
        &mut self,
        operands: &[ValueId],
        parameter_tys: &[Ty<'gcx>],
        operation: SetOperation,
    ) -> Option<ValueId> {
        let ([a, b], [array_ty, _]) = (operands, parameter_tys) else { return None };
        let TyKind::DynArray(element) = array_ty.peel_refs().kind else { return None };
        let signed = element.is_signed();
        let address = matches!(element.kind, TyKind::Elementary(ElementaryType::Address(_)));
        let name = match (operation, signed, address) {
            (SetOperation::Union, true, _) => sym::core_array_set_union_signed,
            (SetOperation::Union, false, true) => sym::core_array_set_union_address,
            (SetOperation::Union, false, false) => sym::core_array_set_union,
            (SetOperation::Intersection, true, _) => sym::core_array_set_intersection_signed,
            (SetOperation::Intersection, false, true) => sym::core_array_set_intersection_address,
            (SetOperation::Intersection, false, false) => sym::core_array_set_intersection,
            (SetOperation::Difference, true, _) => sym::core_array_set_difference_signed,
            (SetOperation::Difference, false, true) => sym::core_array_set_difference_address,
            (SetOperation::Difference, false, false) => sym::core_array_set_difference,
        };
        let array = MirType::MemoryObject(MemoryObjectKind::DynamicArray);
        let helper = self.lazy_helper(name, |this, function| {
            function.attributes.no_inline = true;
            function.attributes.preserves_array_elements = true;
            function.attributes.returns_param_elements = true;
            let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
            let a = lowerer.builder.add_param(array);
            let b = lowerer.builder.add_param(array);
            lowerer.builder.set_return_type(array);
            lowerer.lower_core_array_set(operation, signed, a, b);
            Some(())
        })?;
        Some(self.builder.icall(helper, vec![*a, *b], array))
    }

    /// Lowers every `copy` overload to one helper; addresses get their own, so
    /// callers of the word helper cannot widen the proved width of their results.
    fn lower_core_array_copy_call(
        &mut self,
        operands: &[ValueId],
        parameter_tys: &[Ty<'gcx>],
    ) -> Option<ValueId> {
        let ([a], [array_ty]) = (operands, parameter_tys) else { return None };
        let TyKind::DynArray(element) = array_ty.peel_refs().kind else { return None };
        let address = matches!(element.kind, TyKind::Elementary(ElementaryType::Address(_)));
        let name = if address { sym::core_array_copy_address } else { sym::core_array_copy };
        let array = MirType::MemoryObject(MemoryObjectKind::DynamicArray);
        let helper = self.lazy_helper(name, |this, function| {
            function.attributes.no_inline = true;
            function.attributes.preserves_array_elements = true;
            function.attributes.returns_param_elements = true;
            let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
            let a = lowerer.builder.add_param(array);
            lowerer.builder.set_return_type(array);
            let copy = lowerer.lower_core_array_copy(a);
            lowerer.builder.ret([copy]);
            Some(())
        })?;
        Some(self.builder.icall(helper, vec![*a], array))
    }

    /// Copies a word array as the body's `new` and element loop do: the same
    /// allocation and panics, then the length word and elements in one move.
    fn lower_core_array_copy(&mut self, a: ValueId) -> ValueId {
        // length = len(a)
        // panic 0x41 if length >> 64 != 0, as `new` checks its length
        // size = (length + 1) << 5
        // c = alloc size, uninitialized
        // mcopy(c, a, size)
        let length = self.builder.memory_object_len(a, MemoryObjectKind::DynamicArray);
        let shifting = self.cx.gcx.sess.opts.evm_version.has_bitwise_shifting();
        let too_long = self.builder.exceeds_bits(length, 64, shifting);
        self.builder.panic_if(too_long, PanicCode::MemoryAllocationOverflow);
        let one = self.builder.imm(1);
        let words = self.builder.add(length, one);
        let five = self.builder.imm(5);
        let size = self.builder.shl(five, words);
        let copy = self.builder.alloc_object(
            size,
            MemoryObjectLayout::WORD_ARRAY,
            AllocationSemantics::SOLIDITY_UNINITIALIZED,
        );
        let destination = self.builder.cast_word(copy);
        let source = self.builder.cast_word(a);
        self.builder.mcopy_heap(destination, source, size);
        copy
    }

    /// Merges two word arrays the way the module's bodies do, without their
    /// checks, which cannot fail: the output allocation holds every element
    /// the merge can store, and the cursors never pass their arrays' ends.
    ///
    /// The result is allocated as the body allocates it, with the same panics,
    /// but uninitialized. One merge step compares the heads `u` and `v` and
    /// takes one of three branches:
    ///
    ///   u == v: union and intersection store u; both cursors advance
    ///   u < v:  union and difference store u; a advances
    ///   u > v:  union stores v; b advances
    ///
    /// Each branch updates only the cursors it moves, which costs less per step
    /// than computing every cursor's increment from the comparisons. Every
    /// store consumes at least one input element, so the allocation, which
    /// holds as many elements as the inputs the operation can store from, is
    /// never exceeded. Union then moves the rest of both inputs, and difference
    /// the rest of `a`, with `mcopy`; the other input is exhausted. The final
    /// length counts the stored words. An output without capacity, empty for
    /// every operation, returns its header before the cursors are set up. Each
    /// step tests the cursors it leaves and continues straight into the next:
    /// a nonzero capacity already shows that intersection has work, so its
    /// first step needs no test at all.
    fn lower_core_array_set(
        &mut self,
        operation: SetOperation,
        signed: bool,
        a: ValueId,
        b: ValueId,
    ) {
        let kind = MemoryObjectKind::DynamicArray;
        let a_length = self.builder.memory_object_len(a, kind);
        let b_length = self.builder.memory_object_len(b, kind);
        // capacity = union: a.length + b.length (panic 0x11 on overflow)
        //            intersection: min(a.length, b.length)
        //            difference: a.length
        let capacity = match operation {
            SetOperation::Union => {
                let sum = self.builder.add(a_length, b_length);
                let overflow = self.builder.lt(sum, a_length);
                self.builder.panic_if(overflow, PanicCode::ArithmeticOverflowUnderflow);
                sum
            }
            SetOperation::Intersection => {
                let a_shorter = self.builder.lt(a_length, b_length);
                self.builder.select(a_shorter, a_length, b_length)
            }
            SetOperation::Difference => a_length,
        };
        // The merge stores the output's length once, after it knows it, so
        // the allocation leaves the header unwritten.
        // c = alloc (capacity + 1) * 32, uninitialized
        let one = self.builder.imm(1);
        let words = self.builder.checked_add(capacity, one);
        let word = self.builder.imm(32);
        let size = self.builder.checked_mul(words, word);
        let output = self.builder.alloc_object(
            size,
            MemoryObjectLayout::WORD_ARRAY,
            AllocationSemantics::SOLIDITY_UNINITIALIZED,
        );

        // An output with no capacity is empty, so it needs no merge.
        // branch capacity == 0, empty, merge
        // empty: mstore(c, 0); ret c
        let empty = self.builder.create_block();
        let merge = self.builder.create_block();
        let no_capacity = self.builder.eq_zero(capacity);
        self.builder.branch(no_capacity, empty, merge);
        self.builder.switch_to_block(empty);
        let zero = self.builder.imm(0);
        self.builder.set_memory_object_len(output, zero, kind);
        self.builder.ret([output]);
        self.builder.switch_to_block(merge);

        // a_cursor = data(a); a_end = a_cursor + 32 * a.length
        // b_cursor = data(b); b_end = b_cursor + 32 * b.length
        // out_start = data(c)
        let five = self.builder.imm(5);
        let a_start = self.builder.memory_object_data(a, kind);
        let a_start = self.builder.cast_word(a_start);
        let a_bytes = self.builder.shl(five, a_length);
        let a_end = self.builder.add(a_start, a_bytes);
        let b_start = self.builder.memory_object_data(b, kind);
        let b_start = self.builder.cast_word(b_start);
        let b_bytes = self.builder.shl(five, b_length);
        let b_end = self.builder.add(b_start, b_bytes);
        let out_start = self.builder.memory_object_data(output, kind);
        let out_start = self.builder.cast_word(out_start);
        let entry = self.builder.current_block();
        let body = self.builder.create_block();
        let done = self.builder.create_block();

        // The merge runs while both inputs have elements, and each step tests
        // the cursors it leaves. A nonzero capacity already gives intersection
        // two nonempty inputs and difference a nonempty `a`, so only the rest
        // is tested on entry.
        // intersection: jump body
        // difference:   branch b.length != 0, body, done
        // union:        branch a.length != 0 & b.length != 0, body, done
        match operation {
            SetOperation::Intersection => self.builder.jump(body),
            SetOperation::Difference => {
                let b_more = self.builder.ne_zero(b_length);
                self.builder.branch(b_more, body, done);
            }
            SetOperation::Union => {
                let a_more = self.builder.ne_zero(a_length);
                let b_more = self.builder.ne_zero(b_length);
                let both = self.builder.and(a_more, b_more);
                self.builder.branch(both, body, done);
            }
        }

        // body: out, a, b = phi [entry: starts], [each step: its cursors]
        //       u = mload a_cursor; v = mload b_cursor; branch u == v, equal, unequal
        self.builder.switch_to_block(body);
        let out_cursor = self.builder.phi(vec![(entry, out_start)]);
        let a_cursor = self.builder.phi(vec![(entry, a_start)]);
        let b_cursor = self.builder.phi(vec![(entry, b_start)]);
        let u = self.builder.mload(a_cursor);
        let v = self.builder.mload(b_cursor);
        let equal = self.builder.eq(u, v);
        let equal_block = self.builder.create_block();
        let unequal = self.builder.create_block();
        let a_arm = self.builder.create_block();
        let b_arm = self.builder.create_block();
        self.builder.branch(equal, equal_block, unequal);

        // equal: union, intersection: mstore out, u; out += 32
        //        a += 32; b += 32
        self.builder.switch_to_block(equal_block);
        let equal_out = if matches!(operation, SetOperation::Difference) {
            out_cursor
        } else {
            self.builder.mstore(out_cursor, u);
            self.builder.add_u64_offset(out_cursor, 32)
        };
        let equal_a = self.builder.add_u64_offset(a_cursor, 32);
        let equal_b = self.builder.add_u64_offset(b_cursor, 32);
        let equal_end = self.set_merge_step(equal_a, a_end, equal_b, b_end, body, done);

        // unequal: branch u < v (signed: slt), a_arm, b_arm
        self.builder.switch_to_block(unequal);
        let less = if signed { self.builder.slt(u, v) } else { self.builder.lt(u, v) };
        self.builder.branch(less, a_arm, b_arm);

        // a_arm: union, difference: mstore out, u; out += 32
        //        a += 32
        self.builder.switch_to_block(a_arm);
        let a_out = if matches!(operation, SetOperation::Intersection) {
            out_cursor
        } else {
            self.builder.mstore(out_cursor, u);
            self.builder.add_u64_offset(out_cursor, 32)
        };
        let a_next = self.builder.add_u64_offset(a_cursor, 32);
        let a_end_block = self.set_merge_step(a_next, a_end, b_cursor, b_end, body, done);

        // b_arm: union: mstore out, v; out += 32
        //        b += 32
        self.builder.switch_to_block(b_arm);
        let b_out = if matches!(operation, SetOperation::Union) {
            self.builder.mstore(out_cursor, v);
            self.builder.add_u64_offset(out_cursor, 32)
        } else {
            out_cursor
        };
        let b_next = self.builder.add_u64_offset(b_cursor, 32);
        let b_end_block = self.set_merge_step(a_cursor, a_end, b_next, b_end, body, done);

        let steps = [
            (equal_end, equal_out, equal_a, equal_b),
            (a_end_block, a_out, a_next, b_cursor),
            (b_end_block, b_out, a_cursor, b_next),
        ];
        for &(block, out, a_next, b_next) in &steps {
            self.builder.add_phi_incoming(out_cursor, block, out);
            self.builder.add_phi_incoming(a_cursor, block, a_next);
            self.builder.add_phi_incoming(b_cursor, block, b_next);
        }

        // done: out, a, b = phi [entry: starts (not intersection)], [each step: its cursors]
        self.builder.switch_to_block(done);
        let mut exits = steps.to_vec();
        if !matches!(operation, SetOperation::Intersection) {
            exits.push((entry, out_start, a_start, b_start));
        }
        let out_cursor =
            self.builder.phi(exits.iter().map(|&(block, out, _, _)| (block, out)).collect());
        let a_cursor = self.builder.phi(exits.iter().map(|&(block, _, a, _)| (block, a)).collect());
        let b_cursor = self.builder.phi(exits.iter().map(|&(block, _, _, b)| (block, b)).collect());

        // union: mcopy the rest of a, then of b; difference: the rest of a
        // len(c) = (out_cursor - c) / 32 - 1
        // Measuring from `c` keeps the start cursor out of the loop's state.
        let mut out_end = out_cursor;
        let tails = match operation {
            SetOperation::Union => vec![(a_cursor, a_end), (b_cursor, b_end)],
            SetOperation::Intersection => Vec::new(),
            SetOperation::Difference => vec![(a_cursor, a_end)],
        };
        for (cursor, end) in tails {
            let rest = self.builder.sub(end, cursor);
            self.builder.mcopy_heap(out_end, cursor, rest);
            out_end = self.builder.add(out_end, rest);
        }
        let base = self.builder.cast_word(output);
        let out_bytes = self.builder.sub(out_end, base);
        let words = self.builder.shr(five, out_bytes);
        let one = self.builder.imm(1);
        let length = self.builder.sub(words, one);
        self.builder.set_memory_object_len(output, length, kind);
        self.builder.ret([output]);
    }

    /// Ends a set-merge step: continues at `body` while both cursors are
    /// inside their arrays and leaves for `done` otherwise. Returns the block
    /// the step ends in, for the phis both targets take.
    fn set_merge_step(
        &mut self,
        a_cursor: ValueId,
        a_end: ValueId,
        b_cursor: ValueId,
        b_end: ValueId,
        body: BlockId,
        done: BlockId,
    ) -> BlockId {
        // branch a_cursor < a_end & b_cursor < b_end, body, done
        let a_more = self.builder.lt(a_cursor, a_end);
        let b_more = self.builder.lt(b_cursor, b_end);
        let more = self.builder.and(a_more, b_more);
        let block = self.builder.current_block();
        self.builder.branch(more, body, done);
        block
    }

    /// Lowers every `groupSum` overload to one helper: keys compare as words.
    fn lower_core_array_group_sum_call(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let [keys, values] = *operands else { return None };
        let inner = self.lazy_helper(sym::core_array_group_sort_inner, |this, function| {
            function.attributes.no_inline = true;
            function.attributes.preserves_array_elements = true;
            let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
            let low = lowerer.builder.add_param(MirType::I256);
            let high = lowerer.builder.add_param(MirType::I256);
            let pair = lowerer.builder.add_param(MirType::I256);
            lowerer.lower_core_array_sort(false, low, high, Some(pair));
            Some(())
        })?;
        let sort = self.lazy_helper(sym::core_array_group_sort, |this, function| {
            function.attributes.no_inline = true;
            function.attributes.preserves_array_elements = true;
            let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
            let low = lowerer.builder.add_param(MirType::I256);
            let high = lowerer.builder.add_param(MirType::I256);
            let pair = lowerer.builder.add_param(MirType::I256);
            lowerer.lower_core_array_sort_entry(inner, false, low, high, Some(pair));
            lowerer.builder.ret([]);
            Some(())
        })?;
        let helper = self.lazy_helper(sym::core_array_group_sum, |this, function| {
            function.attributes.no_inline = true;
            // Keys are only permuted; the sums land in `uint256[]` values.
            function.attributes.preserves_array_elements = true;
            let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
            let ty = MirType::MemoryObject(MemoryObjectKind::DynamicArray);
            let keys = lowerer.builder.add_param(ty);
            let values = lowerer.builder.add_param(ty);
            lowerer.lower_core_array_group_sum(sort, keys, values);
            Some(())
        })?;
        self.builder.icall_void(helper, vec![keys, values]);
        Some(self.builder.imm(U256::ZERO))
    }

    /// Sorts the pairs by key word, then keeps the first key of every run and
    /// gives it the checked sum of the run's values, shrinking both arrays.
    fn lower_core_array_group_sum(&mut self, sort: FunctionId, keys: ValueId, values: ValueId) {
        let kind = MemoryObjectKind::DynamicArray;
        // panic(0x32) if len(keys) != len(values)
        let length = self.builder.memory_object_len(keys, kind);
        let value_length = self.builder.memory_object_len(values, kind);
        let differs = self.builder.ne(length, value_length);
        self.builder.panic_if(differs, PanicCode::ArrayOutOfBounds);
        let two = self.builder.imm(2);
        let small = self.builder.lt(length, two);
        let done = self.builder.create_block();
        let start = self.builder.create_block();
        self.builder.branch(small, done, start);

        // low = data(keys); pair = data(values) - low; high = low + 32 * length
        self.builder.switch_to_block(start);
        let low = self.builder.memory_object_data(keys, kind);
        let low = self.builder.cast(low, MirType::I256);
        let value_low = self.builder.memory_object_data(values, kind);
        let value_low = self.builder.cast(value_low, MirType::I256);
        let pair = self.builder.sub(value_low, low);
        let five = self.builder.imm(5);
        let bytes = self.builder.shl(five, length);
        let high = self.builder.add(low, bytes);
        self.builder.icall_void(sort, vec![low, high, pair]);

        // first run: write = low, sum = mload(low + pair)
        let word = self.builder.imm(32);
        let first = self.builder.add(low, word);
        let first_sum = self.builder.mload(value_low);
        let scan = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let accumulate = self.builder.create_block();
        let new_run = self.builder.create_block();
        let latch = self.builder.create_block();
        let finish = self.builder.create_block();
        self.builder.jump(header);

        self.builder.switch_to_block(header);
        let read = self.builder.phi(vec![(scan, first)]);
        let write = self.builder.phi(vec![(scan, low)]);
        let sum = self.builder.phi(vec![(scan, first_sum)]);
        let more = self.builder.lt(read, high);
        self.builder.branch(more, body, finish);

        // key = mload(read); value = mload(read + pair)
        self.builder.switch_to_block(body);
        let key = self.builder.mload(read);
        let kept = self.builder.mload(write);
        let value_address = self.builder.add(read, pair);
        let value = self.builder.mload(value_address);
        let same = self.builder.eq(key, kept);
        self.builder.branch(same, accumulate, new_run);

        // sum' = sum + value; panic(0x11) if sum' < value
        self.builder.switch_to_block(accumulate);
        let total = self.builder.add(sum, value);
        let overflow = self.builder.lt(total, value);
        self.builder.panic_if(overflow, PanicCode::ArithmeticOverflowUnderflow);
        let accumulated = self.builder.current_block();
        self.builder.jump(latch);

        // mstore(write + pair, sum); write' = write + 32; mstore(write', key)
        self.builder.switch_to_block(new_run);
        let sum_address = self.builder.add(write, pair);
        self.builder.mstore(sum_address, sum);
        let next_write = self.builder.add(write, word);
        self.builder.mstore(next_write, key);
        self.builder.jump(latch);

        self.builder.switch_to_block(latch);
        let latch_write = self.builder.phi(vec![(accumulated, write), (new_run, next_write)]);
        let latch_sum = self.builder.phi(vec![(accumulated, total), (new_run, value)]);
        let next_read = self.builder.add(read, word);
        self.builder.jump(header);
        self.builder.add_phi_incoming(read, latch, next_read);
        self.builder.add_phi_incoming(write, latch, latch_write);
        self.builder.add_phi_incoming(sum, latch, latch_sum);

        // mstore(write + pair, sum); count = (write - low) / 32 + 1
        // set_len(keys, count); set_len(values, count)
        self.builder.switch_to_block(finish);
        let sum_address = self.builder.add(write, pair);
        self.builder.mstore(sum_address, sum);
        let kept_bytes = self.builder.sub(write, low);
        let kept_words = self.builder.shr(five, kept_bytes);
        let one = self.builder.imm(1);
        let count = self.builder.add(kept_words, one);
        self.builder.set_memory_object_len(keys, count, kind);
        self.builder.set_memory_object_len(values, count, kind);
        self.builder.jump(done);

        self.builder.switch_to_block(done);
        self.builder.ret([]);
    }

    /// Lowers every supported sort overload to one signed or unsigned helper.
    fn lower_core_array_sort_call(
        &mut self,
        operands: &[ValueId],
        parameter_tys: &[Ty<'gcx>],
    ) -> Option<ValueId> {
        let ([input], [array_ty]) = (operands, parameter_tys) else { return None };
        let TyKind::DynArray(element) = array_ty.peel_refs().kind else { return None };
        let signed = element.is_signed();
        let inner_name = Symbol::intern(if signed {
            "core_array_sort_inner_signed"
        } else {
            "core_array_sort_inner"
        });
        let inner = self.lazy_helper(inner_name, |this, function| {
            function.attributes.no_inline = true;
            function.attributes.preserves_array_elements = true;
            let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
            let low = lowerer.builder.add_param(MirType::I256);
            let high = lowerer.builder.add_param(MirType::I256);
            lowerer.lower_core_array_sort(signed, low, high, None);
            Some(())
        })?;
        let entry_name =
            Symbol::intern(if signed { "core_array_sort_signed" } else { "core_array_sort" });
        let helper = self.lazy_helper(entry_name, |this, function| {
            function.attributes.no_inline = true;
            function.attributes.preserves_array_elements = true;
            let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
            let low = lowerer.builder.add_param(MirType::I256);
            let high = lowerer.builder.add_param(MirType::I256);
            lowerer.lower_core_array_sort_entry(inner, signed, low, high, None);
            lowerer.builder.ret([]);
            Some(())
        })?;

        let kind = MemoryObjectKind::DynamicArray;
        let length = self.builder.memory_object_len(*input, kind);
        let two = self.builder.imm(2);
        let small = self.builder.lt(length, two);
        let sort = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.branch(small, done, sort);

        self.builder.switch_to_block(sort);
        let low = self.builder.memory_object_data(*input, kind);
        let low = self.builder.cast(low, MirType::I256);
        let five = self.builder.imm(5);
        let bytes = self.builder.shl(five, length);
        let high = self.builder.add(low, bytes);
        self.builder.icall_void(helper, vec![low, high]);
        self.builder.jump(done);

        self.builder.switch_to_block(done);
        Some(self.builder.imm(U256::ZERO))
    }

    /// Returns for sorted input, reverses descending input, and sends only a
    /// genuinely mixed range through quicksort. With `pair`, the byte distance
    /// from each key to its value, every move of a key moves its value too.
    /// Leaves the builder in the block reached once the range is sorted.
    fn lower_core_array_sort_entry(
        &mut self,
        inner: FunctionId,
        signed: bool,
        low: ValueId,
        high: ValueId,
        pair: Option<ValueId>,
    ) {
        let word = self.builder.imm(32);
        let last = self.builder.sub(high, word);
        let entry = self.builder.current_block();
        let ascending_header = self.builder.create_block();
        let ascending_body = self.builder.create_block();
        let ascending_next = self.builder.create_block();
        let descending_start = self.builder.create_block();
        let descending_header = self.builder.create_block();
        let descending_body = self.builder.create_block();
        let descending_next = self.builder.create_block();
        let mixed = self.builder.create_block();
        let reverse_header = self.builder.create_block();
        let reverse_body = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.jump(ascending_header);

        self.builder.switch_to_block(ascending_header);
        let ascending = self.builder.phi(vec![(entry, last)]);
        let has_pair = self.builder.gt(ascending, low);
        self.builder.branch(has_pair, ascending_body, done);

        self.builder.switch_to_block(ascending_body);
        let previous = self.builder.sub(ascending, word);
        let previous_value = self.builder.mload(previous);
        let value = self.builder.mload(ascending);
        let out_of_order = self.core_sort_lt(value, previous_value, signed);
        self.builder.branch(out_of_order, descending_start, ascending_next);

        self.builder.switch_to_block(ascending_next);
        self.builder.jump(ascending_header);
        self.builder.add_phi_incoming(ascending, ascending_next, previous);

        self.builder.switch_to_block(descending_start);
        self.builder.jump(descending_header);

        self.builder.switch_to_block(descending_header);
        let descending = self.builder.phi(vec![(descending_start, last)]);
        let has_pair = self.builder.gt(descending, low);
        self.builder.branch(has_pair, descending_body, reverse_header);

        self.builder.switch_to_block(descending_body);
        let previous = self.builder.sub(descending, word);
        let previous_value = self.builder.mload(previous);
        let value = self.builder.mload(descending);
        let rises = self.core_sort_lt(previous_value, value, signed);
        self.builder.branch(rises, mixed, descending_next);

        self.builder.switch_to_block(descending_next);
        self.builder.jump(descending_header);
        self.builder.add_phi_incoming(descending, descending_next, previous);

        self.builder.switch_to_block(mixed);
        let header = self.builder.sub(low, word);
        let length = self.builder.mload(header);
        let sentinel = self.builder.imm(if signed { U256::ONE << 255 } else { U256::ZERO });
        self.builder.mstore(header, sentinel);
        self.builder.icall_void(inner, [low, high].into_iter().chain(pair).collect());
        self.builder.mstore(header, length);
        self.builder.jump(done);

        self.builder.switch_to_block(reverse_header);
        let left = self.builder.phi(vec![(descending_header, low)]);
        let right = self.builder.phi(vec![(descending_header, last)]);
        let more = self.builder.lt(left, right);
        self.builder.branch(more, reverse_body, done);

        self.builder.switch_to_block(reverse_body);
        let left_value = self.builder.mload(left);
        let right_value = self.builder.mload(right);
        self.core_sort_exchange(left, right, left_value, right_value, pair);
        let next_left = self.builder.add(left, word);
        let next_right = self.builder.sub(right, word);
        self.builder.jump(reverse_header);
        self.builder.add_phi_incoming(left, reverse_body, next_left);
        self.builder.add_phi_incoming(right, reverse_body, next_right);

        self.builder.switch_to_block(done);
    }

    /// Exchanges the keys at `left` and `right`, and with `pair` their values.
    /// The values move first, so keys and values passed as one array (a zero
    /// `pair`) end up exchanged once: the key stores repeat the value stores.
    fn core_sort_exchange(
        &mut self,
        left: ValueId,
        right: ValueId,
        left_key: ValueId,
        right_key: ValueId,
        pair: Option<ValueId>,
    ) {
        if let Some(pair) = pair {
            // left_value = mload(left + pair); right_value = mload(right + pair)
            // mstore(left + pair, right_value); mstore(right + pair, left_value)
            let left = self.builder.add(left, pair);
            let right = self.builder.add(right, pair);
            let left_value = self.builder.mload(left);
            let right_value = self.builder.mload(right);
            self.builder.mstore(left, right_value);
            self.builder.mstore(right, left_value);
        }
        // mstore(left, right_key); mstore(right, left_key)
        self.builder.mstore(left, right_key);
        self.builder.mstore(right, left_key);
    }

    /// Emits median-of-three Hoare quicksort with insertion-sort leaves.
    /// After each partition the loop goes on with the smaller part and stacks
    /// the larger one above the free-memory pointer, so at most `log2(n)`
    /// ranges wait there and the helper never calls itself. The stack is
    /// addressed by its entry count, never below the free-memory pointer.
    /// With `pair`, each value moves with its key.
    fn lower_core_array_sort(
        &mut self,
        signed: bool,
        initial_low: ValueId,
        initial_high: ValueId,
        pair: Option<ValueId>,
    ) {
        // base = fmp
        let base = self.builder.fmp();
        let zero = self.builder.imm(0);
        let entry = self.builder.current_block();
        let partition_header = self.builder.create_block();
        let partition = self.builder.create_block();
        let insertion = self.builder.create_block();
        self.builder.jump(partition_header);

        self.builder.switch_to_block(partition_header);
        let low = self.builder.phi(vec![(entry, initial_low)]);
        let high = self.builder.phi(vec![(entry, initial_high)]);
        let pending = self.builder.phi(vec![(entry, zero)]);
        let extent = self.builder.sub(high, low);
        let threshold = self.builder.imm(13 * 32);
        let large = self.builder.gt(extent, threshold);
        self.builder.branch(large, partition, insertion);

        self.builder.switch_to_block(partition);
        let word = self.builder.imm(32);
        let six = self.builder.imm(6);
        let middle_words = self.builder.shr(six, extent);
        let five = self.builder.imm(5);
        let middle_offset = self.builder.shl(five, middle_words);
        let middle = self.builder.add(low, middle_offset);
        let last = self.builder.sub(high, word);
        // The paired values of the three sampled keys follow the same selects.
        let mut paired = pair.map(|pair| {
            let [first, middle, last] = [low, middle, last].map(|key| {
                let address = self.builder.add(key, pair);
                (address, self.builder.mload(address))
            });
            [first, middle, last]
        });
        let mut first_value = self.builder.mload(low);
        let mut middle_value = self.builder.mload(middle);
        let swap = self.core_sort_lt(middle_value, first_value, signed);
        let new_first = self.builder.select(swap, middle_value, first_value);
        let new_middle = self.builder.select(swap, first_value, middle_value);
        first_value = new_first;
        middle_value = new_middle;
        self.core_sort_select_pair(&mut paired, swap, 0, 1);
        let mut last_value = self.builder.mload(last);
        let swap = self.core_sort_lt(last_value, middle_value, signed);
        let new_middle = self.builder.select(swap, last_value, middle_value);
        let new_last = self.builder.select(swap, middle_value, last_value);
        middle_value = new_middle;
        last_value = new_last;
        self.core_sort_select_pair(&mut paired, swap, 1, 2);
        let swap = self.core_sort_lt(middle_value, first_value, signed);
        let new_first = self.builder.select(swap, middle_value, first_value);
        let new_middle = self.builder.select(swap, first_value, middle_value);
        first_value = new_first;
        middle_value = new_middle;
        self.core_sort_select_pair(&mut paired, swap, 0, 1);
        self.builder.mstore(low, first_value);
        self.builder.mstore(middle, middle_value);
        self.builder.mstore(last, last_value);
        for (address, value) in paired.into_iter().flatten() {
            self.builder.mstore(address, value);
        }
        let initial_left = self.builder.add(low, word);
        let initial_right = self.builder.sub(last, word);
        let scan_left = self.builder.create_block();
        let advance_left = self.builder.create_block();
        let scan_right = self.builder.create_block();
        let advance_right = self.builder.create_block();
        let compare = self.builder.create_block();
        let exchange = self.builder.create_block();
        let partition_done = self.builder.create_block();
        self.builder.jump(scan_left);

        self.builder.switch_to_block(scan_left);
        let left = self.builder.phi(vec![(partition, initial_left)]);
        let right_start = self.builder.phi(vec![(partition, initial_right)]);
        let left_value = self.builder.mload(left);
        let before_pivot = self.core_sort_lt(left_value, middle_value, signed);
        self.builder.branch(before_pivot, advance_left, scan_right);

        self.builder.switch_to_block(advance_left);
        let next_left = self.builder.add(left, word);
        self.builder.jump(scan_left);
        self.builder.add_phi_incoming(left, advance_left, next_left);
        self.builder.add_phi_incoming(right_start, advance_left, right_start);

        self.builder.switch_to_block(scan_right);
        let right = self.builder.phi(vec![(scan_left, right_start)]);
        let right_value = self.builder.mload(right);
        let after_pivot = self.core_sort_lt(middle_value, right_value, signed);
        self.builder.branch(after_pivot, advance_right, compare);

        self.builder.switch_to_block(advance_right);
        let next_right = self.builder.sub(right, word);
        self.builder.jump(scan_right);
        self.builder.add_phi_incoming(right, advance_right, next_right);

        self.builder.switch_to_block(compare);
        let ordered = self.builder.lt(left, right);
        self.builder.branch(ordered, exchange, partition_done);

        self.builder.switch_to_block(exchange);
        self.core_sort_exchange(left, right, left_value, right_value, pair);
        let next_left = self.builder.add(left, word);
        let next_right = self.builder.sub(right, word);
        self.builder.jump(scan_left);
        self.builder.add_phi_incoming(left, exchange, next_left);
        self.builder.add_phi_incoming(right_start, exchange, next_right);

        self.builder.switch_to_block(partition_done);
        let split = self.builder.add(right, word);
        let left_size = self.builder.sub(split, low);
        let right_size = self.builder.sub(high, split);
        let left_smaller = self.builder.lt(left_size, right_size);
        let sort_left = self.builder.create_block();
        let sort_right = self.builder.create_block();
        self.builder.branch(left_smaller, sort_left, sort_right);

        // stack [split, high); go on with [low, split)
        self.builder.switch_to_block(sort_left);
        let stacked = self.core_sort_push_range(base, pending, split, high);
        self.builder.jump(partition_header);
        self.builder.add_phi_incoming(low, sort_left, low);
        self.builder.add_phi_incoming(high, sort_left, split);
        self.builder.add_phi_incoming(pending, sort_left, stacked);

        // stack [low, split); go on with [split, high)
        self.builder.switch_to_block(sort_right);
        let stacked = self.core_sort_push_range(base, pending, low, split);
        self.builder.jump(partition_header);
        self.builder.add_phi_incoming(low, sort_right, split);
        self.builder.add_phi_incoming(high, sort_right, high);
        self.builder.add_phi_incoming(pending, sort_right, stacked);

        self.builder.switch_to_block(insertion);
        self.lower_core_array_insertion_sort(signed, low, high, pair);
        let pop = self.builder.create_block();
        let done = self.builder.create_block();
        let waiting = self.builder.ne_zero(pending);
        self.builder.branch(waiting, pop, done);

        // pending' = pending - 1; slot = base + (pending' << 6)
        // low' = mload(slot); high' = mload(slot + 32)
        self.builder.switch_to_block(pop);
        let one = self.builder.imm(1);
        let remaining = self.builder.sub(pending, one);
        let six = self.builder.imm(6);
        let offset = self.builder.shl(six, remaining);
        let slot = self.builder.add(base, offset);
        let popped_low = self.builder.mload(slot);
        let upper = self.builder.add(slot, word);
        let popped_high = self.builder.mload(upper);
        self.builder.jump(partition_header);
        self.builder.add_phi_incoming(low, pop, popped_low);
        self.builder.add_phi_incoming(high, pop, popped_high);
        self.builder.add_phi_incoming(pending, pop, remaining);

        self.builder.switch_to_block(done);
        self.builder.ret([]);
    }

    /// Stacks the range `[low, high)` as entry `pending` above `base` and
    /// returns the new entry count.
    fn core_sort_push_range(
        &mut self,
        base: ValueId,
        pending: ValueId,
        low: ValueId,
        high: ValueId,
    ) -> ValueId {
        // slot = base + (pending << 6); mstore(slot, low); mstore(slot + 32, high)
        let six = self.builder.imm(6);
        let offset = self.builder.shl(six, pending);
        let slot = self.builder.add(base, offset);
        self.builder.mstore(slot, low);
        let word = self.builder.imm(32);
        let upper = self.builder.add(slot, word);
        self.builder.mstore(upper, high);
        let one = self.builder.imm(1);
        self.builder.add(pending, one)
    }

    /// Exchanges the sampled values at `a` and `b` when `swap` holds, as the
    /// median-of-three selects exchange their keys.
    fn core_sort_select_pair(
        &mut self,
        paired: &mut Option<[(ValueId, ValueId); 3]>,
        swap: ValueId,
        a: usize,
        b: usize,
    ) {
        let Some(values) = paired else { return };
        // value_a' = swap ? value_b : value_a; value_b' = swap ? value_a : value_b
        let (first, second) = (values[a].1, values[b].1);
        values[a].1 = self.builder.select(swap, second, first);
        values[b].1 = self.builder.select(swap, first, second);
    }

    /// Emits insertion sort for the current quicksort leaf and leaves the
    /// builder in the block reached once the leaf is sorted.
    fn lower_core_array_insertion_sort(
        &mut self,
        signed: bool,
        low: ValueId,
        high: ValueId,
        pair: Option<ValueId>,
    ) {
        let word = self.builder.imm(32);
        let initial = self.builder.add(low, word);
        let preheader = self.builder.current_block();
        let outer_header = self.builder.create_block();
        let outer_body = self.builder.create_block();
        let shift_header = self.builder.create_block();
        let shift = self.builder.create_block();
        let place = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.jump(outer_header);

        self.builder.switch_to_block(outer_header);
        let cursor = self.builder.phi(vec![(preheader, initial)]);
        let more = self.builder.lt(cursor, high);
        self.builder.branch(more, outer_body, done);

        self.builder.switch_to_block(outer_body);
        let key = self.builder.mload(cursor);
        let key_value = pair.map(|pair| {
            let address = self.builder.add(cursor, pair);
            self.builder.mload(address)
        });
        self.builder.jump(shift_header);

        self.builder.switch_to_block(shift_header);
        let slot = self.builder.phi(vec![(outer_body, cursor)]);
        let previous = self.builder.sub(slot, word);
        let previous_value = self.builder.mload(previous);
        let out_of_order = self.core_sort_lt(key, previous_value, signed);
        self.builder.branch(out_of_order, shift, place);

        self.builder.switch_to_block(shift);
        self.builder.mstore(slot, previous_value);
        if let Some(pair) = pair {
            // mstore(slot + pair, mload(previous + pair))
            let from = self.builder.add(previous, pair);
            let value = self.builder.mload(from);
            let to = self.builder.add(slot, pair);
            self.builder.mstore(to, value);
        }
        self.builder.jump(shift_header);
        self.builder.add_phi_incoming(slot, shift, previous);

        self.builder.switch_to_block(place);
        self.builder.mstore(slot, key);
        if let (Some(pair), Some(key_value)) = (pair, key_value) {
            // mstore(slot + pair, key_value)
            let to = self.builder.add(slot, pair);
            self.builder.mstore(to, key_value);
        }
        let next = self.builder.add(cursor, word);
        self.builder.jump(outer_header);
        self.builder.add_phi_incoming(cursor, place, next);

        self.builder.switch_to_block(done);
    }

    fn core_sort_lt(&mut self, lhs: ValueId, rhs: ValueId, signed: bool) -> ValueId {
        if signed { self.builder.slt(lhs, rhs) } else { self.builder.lt(lhs, rhs) }
    }

    /// Packs an intrinsic's results the way a call to `function_id` returns
    /// them, so the caller destructures either the same way.
    fn core_results(&mut self, function_id: hir::FunctionId, values: Vec<ValueId>) -> ValueId {
        let function = self.cx.gcx.hir.function(function_id);
        let types = function
            .returns
            .iter()
            .map(|&variable| self.cx.gcx.type_of_item(variable.into()))
            .collect::<Vec<_>>();
        self.pack_return_values(values, &types)
    }

    /// `Calls.callInto`, `staticCallInto` and `delegateCallInto`: the output
    /// lands in the caller's buffer, never past it.
    fn lower_core_call_into(
        &mut self,
        intrinsic: CoreIntrinsic,
        function_id: hir::FunctionId,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        let (target, value, gas, payload, output) = match (intrinsic, operands) {
            (CoreIntrinsic::CallInto, &[target, value, gas, payload, output]) => {
                (target, Some(value), gas, payload, output)
            }
            (
                CoreIntrinsic::StaticCallInto | CoreIntrinsic::DelegateCallInto,
                &[target, gas, payload, output],
            ) => (target, None, gas, payload, output),
            _ => return None,
        };
        let input_size = self.builder.memory_object_len(payload, MemoryObjectKind::Bytes);
        let input = self.builder.memory_object_data(payload, MemoryObjectKind::Bytes);
        let capacity = self.builder.memory_object_len(output, MemoryObjectKind::Bytes);
        let destination = self.builder.memory_object_data(output, MemoryObjectKind::Bytes);
        // success = call|staticcall|delegatecall(gas, target[, value], input, input_size,
        //                                        destination, capacity)
        let success = match (intrinsic, value) {
            (CoreIntrinsic::CallInto, Some(value)) => {
                self.builder.call(gas, target, value, input, input_size, destination, capacity)
            }
            (CoreIntrinsic::StaticCallInto, _) => {
                self.builder.staticcall(gas, target, input, input_size, destination, capacity)
            }
            _ => self.builder.delegatecall(gas, target, input, input_size, destination, capacity),
        };
        // total = returndatasize()
        // copied = total < capacity ? total : capacity
        let total = self.builder.returndatasize();
        let shorter = self.builder.lt(total, capacity);
        let copied = self.builder.select(shorter, total, capacity);
        Some(self.core_results(function_id, vec![success, copied, total]))
    }

    /// `Math.mul512(x, y)`: the product modulo `2**256 - 1` is `high + low`
    /// there, so the high word is its difference from the low one, less a
    /// borrow.
    fn lower_core_mul512(
        &mut self,
        function_id: hir::FunctionId,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        let [x, y] = *operands else { return None };
        // low = mul(x, y)
        // folded = mulmod(x, y, not(0))
        // high = sub(sub(folded, low), lt(folded, low))
        let low = self.builder.mul(x, y);
        let modulus = self.builder.imm(U256::MAX);
        let folded = self.builder.mulmod(x, y, modulus);
        let difference = self.builder.sub(folded, low);
        let borrow = self.builder.lt(folded, low);
        let high = self.builder.sub(difference, borrow);
        Some(self.core_results(function_id, vec![high, low]))
    }

    /// `Revert.raw(data)`: the call never returns.
    fn lower_core_revert_raw(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let [data] = *operands else { return None };
        // revert(data(object), len(object))
        let length = self.builder.memory_object_len(data, MemoryObjectKind::Bytes);
        let pointer = self.builder.memory_object_data(data, MemoryObjectKind::Bytes);
        self.builder.revert(pointer, length);
        Some(self.builder.imm(U256::ZERO))
    }

    /// `Return.abiEncoded(s)` returns `s` encoded where it lies, as the tail of
    /// a one-word head: the offset goes in the word below the string and a
    /// zero word pads its end. The call ends at the return, so nothing reads
    /// the words written over afterwards.
    fn lower_core_return_abi_encoded(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let [s] = *operands else { return None };
        let kind = MemoryObjectKind::Bytes;
        // head = s - 32
        // mstore(head, 32)
        // mstore(data(s) + len(s), 0)
        // return(head, ((len(s) + 31) & ~31) + 64)
        let object = self.builder.cast_word(s);
        let word = self.builder.imm(32);
        let head = self.builder.sub(object, word);
        self.builder.mstore(head, word);
        let length = self.builder.memory_object_len(s, kind);
        let data = self.builder.memory_object_data(s, kind);
        let data = self.builder.cast_word(data);
        let end = self.builder.add(data, length);
        let zero = self.builder.imm(0);
        self.builder.mstore(end, zero);
        let rounded = self.builder.add_u64_offset(length, 31);
        let mask = self.builder.imm(U256::MAX << 5);
        let padded = self.builder.and(rounded, mask);
        let size = self.builder.add_u64_offset(padded, 64);
        self.builder.ret_data(head, size);
        Some(self.builder.imm(U256::ZERO))
    }

    /// The slot of word `index` in the region `root` roots: past the hash of
    /// the root's slot, where a dynamic array there keeps its elements. An
    /// index of `2**64` or more panics as the body's range check does. The
    /// hash comes first, so a loop over one region can hoist it.
    fn core_slots_word(&mut self, root: ValueId, index: ValueId) -> ValueId {
        // data = keccak256(root)
        // panic 0x32 if index >> 64 != 0
        // slot = data + index
        let data = self.builder.storage_array_data_slot(root);
        let shifting = self.cx.gcx.sess.opts.evm_version.has_bitwise_shifting();
        let out_of_range = self.builder.exceeds_bits(index, 64, shifting);
        self.builder.panic_if(out_of_range, PanicCode::ArrayOutOfBounds);
        self.builder.add(data, index)
    }

    /// `Slots.load(root, index)`.
    fn lower_core_slots_load(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let [root, index] = *operands else { return None };
        let slot = self.core_slots_word(root, index);
        // value = sload(slot)
        Some(self.builder.sload(slot))
    }

    /// `Slots.store(root, index, value)`.
    fn lower_core_slots_store(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let [root, index, value] = *operands else { return None };
        let slot = self.core_slots_word(root, index);
        // sstore(slot, value)
        self.builder.sstore(slot, value);
        Some(self.builder.imm(U256::ZERO))
    }

    /// The hash of the root's slot, where word 0 of its region lies, after the
    /// byte operations' count check: below `2**69`, so every word they touch
    /// has an index below `2**64`.
    fn core_slots_region(&mut self, root: ValueId, count: ValueId) -> ValueId {
        // data = keccak256(root)
        // panic 0x32 if count >> 69 != 0
        let data = self.builder.storage_array_data_slot(root);
        let shifting = self.cx.gcx.sess.opts.evm_version.has_bitwise_shifting();
        let too_many = self.builder.exceeds_bits(count, 69, shifting);
        self.builder.panic_if(too_many, PanicCode::ArrayOutOfBounds);
        data
    }

    /// `Slots.storeBytes` and `Slots.storeCalldataBytes`: the range's whole
    /// words one store each, from one hash, then the rest of the range in one
    /// more word whose bytes past it are zero.
    fn lower_core_slots_store_bytes(
        &mut self,
        operands: &[ValueId],
        calldata: bool,
    ) -> Option<ValueId> {
        let [root, buffer, offset, count] = *operands else { return None };
        let data = self.core_slots_region(root, count);
        let source = if calldata {
            self.core_checked_calldata_range(buffer, offset, Width::Dynamic(count))
        } else {
            self.core_checked_range(buffer, offset, Width::Dynamic(count))
        };
        // words = count >> 5
        // for k < words: sstore(data + k, load(source + (k << 5)))
        let five = self.builder.imm(5);
        let words = self.builder.shr(five, count);
        self.builder.counted_loop(words, |builder, k| {
            let five = builder.imm(5);
            let step = builder.shl(five, k);
            let address = builder.add(source, step);
            let word =
                if calldata { builder.calldataload(address) } else { builder.mload(address) };
            let slot = builder.add(data, k);
            builder.sstore(slot, word);
        });
        // rest = count & 31
        // if rest != 0: sstore(data + words, load(source + (words << 5)) & ~(MAX >> (rest << 3)))
        let thirty_one = self.builder.imm(31);
        let rest = self.builder.and(count, thirty_one);
        let partial = self.builder.ne_zero(rest);
        let tail = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.branch(partial, tail, done);
        self.builder.switch_to_block(tail);
        let five = self.builder.imm(5);
        let step = self.builder.shl(five, words);
        let address = self.builder.add(source, step);
        let word =
            if calldata { self.builder.calldataload(address) } else { self.builder.mload(address) };
        let three = self.builder.imm(3);
        let bits = self.builder.shl(three, rest);
        let all = self.builder.imm(U256::MAX);
        let past = self.builder.shr(bits, all);
        let kept = self.builder.not(past);
        let word = self.builder.and(word, kept);
        let slot = self.builder.add(data, words);
        self.builder.sstore(slot, word);
        self.builder.jump(done);
        self.builder.switch_to_block(done);
        Some(self.builder.imm(U256::ZERO))
    }

    /// `Slots.loadBytes`: the region's whole words one load and one word store
    /// into the buffer each, from one hash, then the rest of the range merged
    /// into the word that holds it, so the buffer's bytes past it stay.
    fn lower_core_slots_load_bytes(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let [root, buffer, offset, count] = *operands else { return None };
        let data = self.core_slots_region(root, count);
        let target = self.core_checked_range(buffer, offset, Width::Dynamic(count));
        // words = count >> 5
        // for k < words: buffer[offset + (k << 5)..] = sload(data + k)
        let five = self.builder.imm(5);
        let words = self.builder.shr(five, count);
        self.builder.counted_loop(words, |builder, k| {
            let slot = builder.add(data, k);
            let word = builder.sload(slot);
            let five = builder.imm(5);
            let step = builder.shl(five, k);
            let at = builder.add(offset, step);
            builder.memory_object_store_word(buffer, at, word);
        });
        // rest = count & 31
        // if rest != 0:
        //   keep = MAX >> (rest << 3)
        //   buffer[offset + (words << 5)..] =
        //     (sload(data + words) & ~keep) | (mload(target + (words << 5)) & keep)
        let thirty_one = self.builder.imm(31);
        let rest = self.builder.and(count, thirty_one);
        let partial = self.builder.ne_zero(rest);
        let tail = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.branch(partial, tail, done);
        self.builder.switch_to_block(tail);
        let slot = self.builder.add(data, words);
        let word = self.builder.sload(slot);
        let three = self.builder.imm(3);
        let bits = self.builder.shl(three, rest);
        let all = self.builder.imm(U256::MAX);
        let keep = self.builder.shr(bits, all);
        let taken_mask = self.builder.not(keep);
        let taken = self.builder.and(word, taken_mask);
        let five = self.builder.imm(5);
        let step = self.builder.shl(five, words);
        let address = self.builder.add(target, step);
        let old = self.builder.mload(address);
        let kept = self.builder.and(old, keep);
        let merged = self.builder.or(taken, kept);
        let at = self.builder.add(offset, step);
        self.builder.memory_object_store_word(buffer, at, merged);
        self.builder.jump(done);
        self.builder.switch_to_block(done);
        Some(self.builder.imm(U256::ZERO))
    }

    /// `Hash.keccak256Range(b, offset, count)`: the range is hashed where it
    /// lies instead of being copied out first.
    fn lower_core_keccak256_range(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let [object, offset, count] = *operands else { return None };
        let start = self.core_checked_range(object, offset, Width::Dynamic(count));
        // hash = keccak256(start, count)
        Some(self.builder.keccak256(start, count))
    }

    /// The `create` or `create2` both deployment families share.
    fn core_create(&mut self, initcode: ValueId, salt: Option<ValueId>, value: ValueId) -> ValueId {
        let length = self.builder.memory_object_len(initcode, MemoryObjectKind::Bytes);
        let pointer = self.builder.memory_object_data(initcode, MemoryObjectKind::Bytes);
        // deployed = create|create2(value, data, len[, salt])
        match salt {
            Some(salt) => self.builder.create2(value, pointer, length, salt),
            None => self.builder.create(value, pointer, length),
        }
    }

    /// `Create.tryDeploy(initcode, value)` and `tryDeploy2(initcode, salt, value)`:
    /// a creation that returns no address is `false`, not a revert.
    fn lower_core_try_deploy(
        &mut self,
        intrinsic: CoreIntrinsic,
        function_id: hir::FunctionId,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        let (initcode, salt, value) = match (intrinsic, operands) {
            (CoreIntrinsic::TryDeploy, &[initcode, value]) => (initcode, None, value),
            (CoreIntrinsic::TryDeploy2, &[initcode, salt, value]) => (initcode, Some(salt), value),
            _ => return None,
        };
        let deployed = self.core_create(initcode, salt, value);
        // success = deployed != 0
        let success = self.builder.ne_zero(deployed);
        Some(self.core_results(function_id, vec![success, deployed]))
    }

    /// `Create.tryDeployInto(initcode, value, diagnostics)`. A creation that
    /// succeeds leaves no return data, so the copy needs no branch: it moves
    /// nothing then.
    fn lower_core_try_deploy_into(
        &mut self,
        function_id: hir::FunctionId,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        let [initcode, value, diagnostics] = *operands else { return None };
        let capacity = self.builder.memory_object_len(diagnostics, MemoryObjectKind::Bytes);
        let destination = self.builder.memory_object_data(diagnostics, MemoryObjectKind::Bytes);
        let deployed = self.core_create(initcode, None, value);
        // total = returndatasize()
        // copied = total < capacity ? total : capacity
        // returndatacopy(data(diagnostics), 0, copied)
        let total = self.builder.returndatasize();
        let shorter = self.builder.lt(total, capacity);
        let copied = self.builder.select(shorter, total, capacity);
        let zero = self.builder.imm(U256::ZERO);
        self.builder.returndatacopy_heap(destination, zero, copied);
        // success = deployed != 0
        let success = self.builder.ne_zero(deployed);
        Some(self.core_results(function_id, vec![success, deployed, copied, total]))
    }

    /// `Create.deploy(initcode, value)` and `Create.deploy2(initcode, salt, value)`.
    fn lower_core_deploy(
        &mut self,
        intrinsic: CoreIntrinsic,
        operands: &[ValueId],
    ) -> Option<ValueId> {
        let (initcode, salt, value) = match (intrinsic, operands) {
            (CoreIntrinsic::Deploy, &[initcode, value]) => (initcode, None, value),
            (CoreIntrinsic::Deploy2, &[initcode, salt, value]) => (initcode, Some(salt), value),
            _ => return None,
        };
        let deployed = self.core_create(initcode, salt, value);
        // if deployed == 0 { mstore(0, DeploymentFailed.selector); revert(0, 4) }
        let failed = self.builder.eq_zero(deployed);
        let failure = self.builder.create_block();
        let success = self.builder.create_block();
        self.builder.branch(failed, failure, success);
        self.builder.switch_to_block(failure);
        let zero = self.builder.imm(U256::ZERO);
        let selector = self.builder.imm(DEPLOYMENT_FAILED_SELECTOR << 224);
        self.builder.mstore(zero, selector);
        let four = self.builder.imm(4);
        self.builder.revert(zero, four);
        self.builder.switch_to_block(success);
        Some(deployed)
    }

    /// `Code.copyInto(dst, dstOffset, target, start, count)`: checked against
    /// both the buffer and the code's size, so nothing is zero-padded.
    fn lower_core_code_copy(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let [dst, dst_offset, target, start, count] = *operands else { return None };
        let destination = self.core_checked_range(dst, dst_offset, Width::Dynamic(count));
        // size = extcodesize(target)
        // end = start + count
        // panic(0x32) if end < start || end > size
        let size = self.builder.extcodesize(target);
        let end = self.builder.add(start, count);
        let wrapped = self.builder.lt(end, start);
        let over = self.builder.gt(end, size);
        let bad = self.builder.or(wrapped, over);
        self.builder.panic_if(bad, PanicCode::ArrayOutOfBounds);
        // extcodecopy(target, destination, start, count)
        self.builder.extcodecopy_heap(target, destination, start, count);
        Some(self.builder.imm(U256::ZERO))
    }

    /// `tryReadBytesN(b, offset)` and `tryReadUint256BE(b, offset)`: the range
    /// test becomes the flag instead of a panic. A read that fails is aimed at
    /// the start of the buffer, so it never reaches far past it, and its
    /// result is discarded.
    fn lower_core_try_read(
        &mut self,
        function_id: hir::FunctionId,
        operands: &[ValueId],
        width: u8,
    ) -> Option<ValueId> {
        let [object, offset] = *operands else { return None };
        let length = self.builder.memory_object_len(object, MemoryObjectKind::Bytes);
        let misses = self.core_range_misses(length, offset, Width::Const(u64::from(width)));
        // ok = !misses
        // word = mload(data(object) + (ok ? offset : 0))
        // value = (ok ? word : 0) & leading(width)
        let ok = self.builder.eq_zero(misses);
        let zero = self.builder.imm(U256::ZERO);
        let aimed = self.builder.select(ok, offset, zero);
        let data = self.builder.memory_object_data(object, MemoryObjectKind::Bytes);
        let address = self.builder.add(data, aimed);
        let word = self.builder.mload(address);
        let gated = self.builder.select(ok, word, zero);
        // The mask comes last so that a cleanup of the typed result folds
        // into it.
        let value = match leading_mask(width) {
            Some(mask) => {
                let mask = self.builder.imm(mask);
                self.builder.and(gated, mask)
            }
            None => gated,
        };
        Some(self.core_results(function_id, vec![ok, value]))
    }

    /// `CalldataBytes.readBytesN(b, offset)` and `readUint256BE(b, offset)`.
    fn lower_core_calldata_read(&mut self, operands: &[ValueId], width: u8) -> Option<ValueId> {
        let [slice, offset] = *operands else { return None };
        let address =
            self.core_checked_calldata_range(slice, offset, Width::Const(u64::from(width)));
        // word = calldataload(ptr(slice) + offset)
        // result = width < 32 ? word & leading(width) : word
        let word = self.builder.calldataload(address);
        Some(match leading_mask(width) {
            Some(mask) => {
                let mask = self.builder.imm(mask);
                self.builder.and(word, mask)
            }
            None => word,
        })
    }

    /// `CalldataBytes.tryReadBytesN(b, offset)` and `tryReadUint256BE(b, offset)`.
    /// A load from calldata cannot fault, so a read that fails is aimed at the
    /// start of the slice only to keep its address small, and is discarded.
    fn lower_core_calldata_try_read(
        &mut self,
        function_id: hir::FunctionId,
        operands: &[ValueId],
        width: u8,
    ) -> Option<ValueId> {
        let [slice, offset] = *operands else { return None };
        let length = self.builder.slice_len(slice);
        let misses = self.core_range_misses(length, offset, Width::Const(u64::from(width)));
        // ok = !misses
        // word = calldataload(ptr(slice) + (ok ? offset : 0))
        // value = (ok ? word : 0) & leading(width)
        let ok = self.builder.eq_zero(misses);
        let zero = self.builder.imm(U256::ZERO);
        let aimed = self.builder.select(ok, offset, zero);
        let base = self.builder.slice_ptr(slice);
        let address = self.builder.add(base, aimed);
        let word = self.builder.calldataload(address);
        let gated = self.builder.select(ok, word, zero);
        let value = match leading_mask(width) {
            Some(mask) => {
                let mask = self.builder.imm(mask);
                self.builder.and(gated, mask)
            }
            None => gated,
        };
        Some(self.core_results(function_id, vec![ok, value]))
    }

    /// `CalldataBytes.copyInto(dst, dstOffset, src, srcOffset, count)`.
    fn lower_core_calldata_copy(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let [dst, dst_offset, src, src_offset, count] = *operands else { return None };
        let destination = self.core_checked_range(dst, dst_offset, Width::Dynamic(count));
        let source = self.core_checked_calldata_range(src, src_offset, Width::Dynamic(count));
        // calldatacopy(destination, source, count)
        self.builder.calldatacopy_heap(destination, source, count);
        Some(self.builder.imm(U256::ZERO))
    }

    /// `readBytesN(b, offset)` and `readUint256BE(b, offset)`.
    fn lower_core_read(&mut self, operands: &[ValueId], width: u8) -> Option<ValueId> {
        let [object, offset] = *operands else { return None };
        let data = self.core_checked_range(object, offset, Width::Const(u64::from(width)));
        // word = mload(data + offset)
        // result = width < 32 ? word & leading(width) : word
        let word = self.builder.mload(data);
        Some(match leading_mask(width) {
            Some(mask) => {
                let mask = self.builder.imm(mask);
                self.builder.and(word, mask)
            }
            None => word,
        })
    }

    /// `writeBytesN(b, offset, value)` and `writeUint256BE(b, offset, value)`.
    ///
    /// The store is the semantic word store into the object's payload rather than a store
    /// through the computed address: the alias analysis then knows it stays inside the
    /// object, so lengths and words of other objects read before it, and the object's own
    /// length, stay forwarded across it. Both lower to the same `mstore`.
    fn lower_core_write(&mut self, operands: &[ValueId], width: u8) -> Option<ValueId> {
        let [object, offset, value] = *operands else { return None };
        let data = self.core_checked_range(object, offset, Width::Const(u64::from(width)));
        match leading_mask(width) {
            // stored = (mload(data) & ~leading) | (value & leading)
            // memory_object_store_word(object, offset, stored)
            Some(mask) => {
                let tail = self.builder.imm(!mask);
                let mask = self.builder.imm(mask);
                let old = self.builder.mload(data);
                let kept = self.builder.and(old, tail);
                let taken = self.builder.and(value, mask);
                let stored = self.builder.or(kept, taken);
                self.builder.memory_object_store_word(object, offset, stored);
            }
            // A whole word replaces everything at the offset.
            None => self.builder.memory_object_store_word(object, offset, value),
        }
        Some(self.builder.imm(U256::ZERO))
    }

    /// `copyInto(dst, dstOffset, src, srcOffset, count)`.
    fn lower_core_copy(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let [dst, dst_offset, src, src_offset, count] = *operands else { return None };
        let destination = self.core_checked_range(dst, dst_offset, Width::Dynamic(count));
        let source = self.core_checked_range(src, src_offset, Width::Dynamic(count));
        // mcopy(destination, source, count)
        self.builder.mcopy_heap(destination, source, count);
        Some(self.builder.imm(U256::ZERO))
    }

    /// `fill(dst, offset, count, value)`.
    fn lower_core_fill(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let [dst, offset, count, value] = *operands else { return None };
        let base = self.core_checked_range(dst, offset, Width::Dynamic(count));
        // pattern = byte(0, value) * 0x0101..01
        let top = self.builder.imm(248);
        let byte = self.builder.shr(top, value);
        let ones = self.builder.imm(U256::MAX / U256::from(255));
        let pattern = self.builder.mul(byte, ones);
        // for word in 0..count / 32 { mstore(base + word * 32, pattern) }
        let five = self.builder.imm(5);
        let words = self.builder.shr(five, count);
        self.builder.counted_loop(words, |builder, word| {
            let five = builder.imm(5);
            let stride = builder.shl(five, word);
            let address = builder.add(base, stride);
            builder.mstore(address, pattern);
        });
        // rest = count & 31
        // if rest != 0 {
        //   keep = max >> (8 * rest)
        //   tail = base + (words << 5)
        //   mstore(tail, (mload(tail) & keep) | (pattern & ~keep))
        // }
        let low = self.builder.imm(31);
        let rest = self.builder.and(count, low);
        let partial = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.branch(rest, partial, done);
        self.builder.switch_to_block(partial);
        let three = self.builder.imm(3);
        let bits = self.builder.shl(three, rest);
        let all = self.builder.imm(U256::MAX);
        let keep = self.builder.shr(bits, all);
        let five = self.builder.imm(5);
        let stride = self.builder.shl(five, words);
        let tail = self.builder.add(base, stride);
        let old = self.builder.mload(tail);
        let kept = self.builder.and(old, keep);
        let drop = self.builder.not(keep);
        let taken = self.builder.and(pattern, drop);
        let stored = self.builder.or(kept, taken);
        self.builder.mstore(tail, stored);
        self.builder.jump(done);
        self.builder.switch_to_block(done);
        Some(self.builder.imm(U256::ZERO))
    }

    /// `equalsAt(a, offset, b)`: whether the `b.length` bytes of `a` at
    /// `offset` are the bytes of `b`, after the range check the portable body
    /// makes first.
    ///
    /// Whole words compare from the front; the bytes a whole word did not cover
    /// compare as the low bytes of the two words that end where the ranges end.
    /// Every read lies inside memory the objects already occupy: the range is
    /// inside `a`, and a word ending at most one word into an object's data
    /// starts no lower than the object's own length word. The differences
    /// accumulate without an early exit, so a mismatch costs no branch per word.
    /// The whole-word loop steps one cursor over `a`, reads `b` at a fixed
    /// displacement and tests at the bottom behind one guard, so a range shorter
    /// than a word skips the loop entirely.
    fn lower_core_equals_at(&mut self, operands: &[ValueId]) -> Option<ValueId> {
        let [a, offset, b] = *operands else { return None };
        let kind = MemoryObjectKind::Bytes;
        // count = len(b)
        // left = data(a) + offset, after panic(0x32) unless offset + count <= len(a)
        // right = data(b)
        let count = self.builder.memory_object_len(b, kind);
        let left = self.core_checked_range(a, offset, Width::Dynamic(count));
        let left = self.builder.cast_word(left);
        let right = self.builder.memory_object_data(b, kind);
        let right = self.builder.cast_word(right);

        // rest = count & 31; words_end = left + (count - rest)
        // delta = right - left
        // branch left < words_end, body, tail
        let low = self.builder.imm(31);
        let rest = self.builder.and(count, low);
        let whole = self.builder.sub(count, rest);
        let words_end = self.builder.add(left, whole);
        let delta = self.builder.sub(right, left);
        let zero = self.builder.imm(0);
        let entry = self.builder.current_block();
        let body = self.builder.create_block();
        let tail = self.builder.create_block();
        let any_word = self.builder.lt(left, words_end);
        self.builder.branch(any_word, body, tail);

        // body:
        //   cursor = phi [entry: left], [body: cursor + 32]
        //   diff = phi [entry: 0], [body: diff | mload(cursor) ^ mload(cursor + delta)]
        //   branch cursor + 32 < words_end, body, tail
        self.builder.switch_to_block(body);
        let cursor = self.builder.phi(vec![(entry, left)]);
        let diff = self.builder.phi(vec![(entry, zero)]);
        let left_word = self.builder.mload(cursor);
        let right_cursor = self.builder.add(cursor, delta);
        let right_word = self.builder.mload(right_cursor);
        let different = self.builder.xor(left_word, right_word);
        let next_diff = self.builder.or(diff, different);
        let next = self.builder.add_u64_offset(cursor, 32);
        let more = self.builder.lt(next, words_end);
        self.builder.branch(more, body, tail);
        self.builder.add_phi_incoming(cursor, body, next);
        self.builder.add_phi_incoming(diff, body, next_diff);

        // tail:
        //   diff = phi [entry: 0], [body: next_diff]
        //   back = count - 32 (below the data start when count < 32)
        //   rest_mask = (1 << (8 * rest)) - 1
        //   diff |= (mload(left + back) ^ mload(right + back)) & rest_mask
        //   result = diff == 0
        self.builder.switch_to_block(tail);
        let diff = self.builder.phi(vec![(entry, zero), (body, next_diff)]);
        let thirty_two = self.builder.imm(32);
        let back = self.builder.sub(count, thirty_two);
        let left_end = self.builder.add(left, back);
        let left_end = self.builder.mload(left_end);
        let right_end = self.builder.add(right, back);
        let right_end = self.builder.mload(right_end);
        let different = self.builder.xor(left_end, right_end);
        let three = self.builder.imm(3);
        let bits = self.builder.shl(three, rest);
        let one = self.builder.imm(1);
        let bit = self.builder.shl(bits, one);
        let rest_mask = self.builder.sub(bit, one);
        let different = self.builder.and(different, rest_mask);
        let total = self.builder.or(diff, different);
        Some(self.builder.eq_zero(total))
    }

    /// `truncate(a, n)`: shortens a dynamic memory array in place.
    fn lower_core_truncate(
        &mut self,
        expr: &hir::Expr<'_>,
        operands: &[ValueId],
        parameter_tys: &[Ty<'gcx>],
    ) -> Option<ValueId> {
        let ([object, new_len], [array_ty, _]) = (operands, parameter_tys) else { return None };
        let Some(layout) = self.types.memory_layout(*array_ty) else {
            return self.cx.report_unsupported(expr.span, "truncated array type");
        };
        let kind = layout.kind();
        // panic(0x32) if new_len > len(a)
        // set_len(a, new_len)
        let length = self.builder.memory_object_len(*object, kind);
        let grows = self.builder.gt(*new_len, length);
        self.builder.panic_if(grows, PanicCode::ArrayOutOfBounds);
        self.builder.set_memory_object_len(*object, *new_len, kind);
        Some(self.builder.imm(U256::ZERO))
    }

    /// Checks that `[offset, offset + width)` lies inside the `bytes` object
    /// and returns the address of its first byte.
    ///
    /// The sum is tested for wrapping as well as for fit, so an offset near
    /// the top of the word cannot wrap into a range that looks valid.
    fn core_checked_range(&mut self, object: ValueId, offset: ValueId, width: Width) -> ValueId {
        let length = self.builder.memory_object_len(object, MemoryObjectKind::Bytes);
        // panic(0x32) if misses(length, offset, width)
        let misses = self.core_range_misses(length, offset, width);
        self.builder.panic_if(misses, PanicCode::ArrayOutOfBounds);
        // address = data(object) + offset
        let data = self.builder.memory_object_data(object, MemoryObjectKind::Bytes);
        self.builder.add(data, offset)
    }

    /// The calldata counterpart of [`Self::core_checked_range`]: the range is
    /// checked against the slice's length and the address is a calldata
    /// offset.
    fn core_checked_calldata_range(
        &mut self,
        slice: ValueId,
        offset: ValueId,
        width: Width,
    ) -> ValueId {
        let length = self.builder.slice_len(slice);
        // panic(0x32) if misses(length, offset, width)
        let misses = self.core_range_misses(length, offset, width);
        self.builder.panic_if(misses, PanicCode::ArrayOutOfBounds);
        // address = ptr(slice) + offset
        let base = self.builder.slice_ptr(slice);
        self.builder.add(base, offset)
    }

    /// Whether `[offset, offset + width)` fails to lie inside `length` bytes.
    fn core_range_misses(&mut self, length: ValueId, offset: ValueId, width: Width) -> ValueId {
        // end = offset + width
        // misses = end < offset || end > length
        let end = match width {
            Width::Dynamic(count) => self.builder.add(offset, count),
            Width::Const(width) => {
                let width = self.builder.imm(width);
                self.builder.add(offset, width)
            }
        };
        let wrapped = self.builder.lt(end, offset);
        let over = self.builder.gt(end, length);
        self.builder.or(wrapped, over)
    }
}

/// The selector of `Create`'s `DeploymentFailed()` error.
const DEPLOYMENT_FAILED_SELECTOR: U256 = U256::from_limbs([0x3011_6425, 0, 0, 0]);

/// How wide a checked range is.
#[derive(Clone, Copy)]
enum Width {
    /// A width fixed by the operation.
    Const(u64),
    /// A width the caller passed.
    Dynamic(ValueId),
}

/// The mask keeping the leading `width` bytes of a word, or `None` at a whole
/// word, where nothing is masked away. `width` is at most 32.
fn leading_mask(width: u8) -> Option<U256> {
    (width < 32).then(|| !(U256::MAX >> (8 * usize::from(width))))
}
