//! `@custom:solar-inline` storage arrays: small arrays kept in their own slot.
//!
//! While an array documented `@custom:solar-inline` holds at most as many elements as fit below
//! one byte of its slot, the slot holds the elements, element `i` at byte `i * size` as in the
//! first word of the standard data area, and the length in its top byte. A `uint64[]` then keeps up
//! to three elements and its length in one word, where the standard layout spends the length slot
//! and a word of the data area at `keccak256(slot)`. An array that grows past the capacity moves
//! to the standard layout: its elements are already laid out as the first data word, so the push
//! that overflows stores the slot's word without the length byte at `keccak256(slot)`, the new
//! element after them, and the length in the slot. A standard length never reaches the top byte,
//! so each operation reads the slot once and takes the inline form when the top byte is nonzero;
//! an empty array reads the same in both, and the next push starts it inline again, which is sound
//! because emptying a standard array cleared its data area.
//!
//! Reading an element, its length and its public getter compute the element's slot and byte
//! offset in one form or the other, and the element is then read or written like any packed
//! element. Pushing and popping in the inline form each store one word: the grown or shrunk slot.
//! A push, a pop and a copy into memory each go through one shared helper per element type, since
//! their overflow and standard branches are as long as the rest of the operation. `delete`
//! clears the slot of an inline array and the standard way otherwise. Type checking rejects
//! every use that would observe the standard layout of the inline form: storage references, the
//! variable's `.slot`, and assignments of whole arrays.

use super::*;
use crate::mir::lower::storage::InlineArray;

impl<'gcx> FunctionLowerer<'gcx, '_> {
    /// The layout of the `@custom:solar-inline` storage array at `slot`, a state variable's
    /// constant slot. Inside the shared helpers the slot is a parameter, which takes the
    /// standard paths.
    pub(super) fn inline_array_at(&self, slot: ValueId) -> Option<InlineArray> {
        let slot = self.builder.func().value_u256(slot)?;
        self.cx.storage.inline_array_at(slot)
    }

    /// The word of the inline array's slot, whether the array has the inline form, and its
    /// length.
    fn inline_array_header(&mut self, slot: ValueId) -> (ValueId, ValueId, ValueId) {
        // header = sload(slot)
        // top = header >> 248
        // inline = top != 0
        // length = inline ? top : header
        let header = self.builder.sload(slot);
        let length_shift = self.builder.imm(248);
        let top = self.builder.shr(length_shift, header);
        let inline = self.builder.ne_zero(top);
        let length = self.builder.select(inline, top, header);
        (header, inline, length)
    }

    /// The length of the inline array at `slot`.
    pub(super) fn inline_array_length(&mut self, slot: ValueId) -> ValueId {
        self.inline_array_header(slot).2
    }

    /// The access to element `index` of the inline array at `slot`, checked against the length
    /// as `a[index]` checks it: with `Panic(0x32)`, or with an empty revert in a getter.
    pub(super) fn inline_array_element_access(
        &mut self,
        slot: ValueId,
        index: ValueId,
        array: InlineArray,
    ) -> StorageAccess {
        let (_, inline, length) = self.inline_array_header(slot);
        if self.is_getter {
            // if index >= length { revert(0, 0) }
            let valid = self.builder.lt(index, length);
            self.builder.revert_if_zero(valid, RevertReason::Empty);
        } else {
            // bounds_check(index, length)
            self.builder.bounds_check(index, length);
        }
        self.inline_array_element_at(slot, inline, index, array)
    }

    /// The access to element `index` of the inline array at `slot`: in the slot itself while it
    /// has the inline form, and in the standard data area otherwise.
    fn inline_array_element_at(
        &mut self,
        slot: ValueId,
        inline: ValueId,
        index: ValueId,
        array: InlineArray,
    ) -> StorageAccess {
        // if inline { element_slot = slot; offset = index * size }
        // else { element_slot = keccak256(slot) + index / per_slot; offset = index % per_slot *
        // size }
        let inline_block = self.builder.create_block();
        let standard_block = self.builder.create_block();
        let join = self.builder.create_block();
        self.builder.branch(inline, inline_block, standard_block);

        self.builder.switch_to_block(inline_block);
        let size = self.builder.imm(array.size());
        let inline_offset = self.builder.mul(index, size);
        let inline_end = self.builder.current_block();
        self.builder.jump(join);

        self.builder.switch_to_block(standard_block);
        let data = self.builder.storage_array_data_slot(slot);
        let per_slot = self.builder.imm(array.per_slot());
        let word = self.builder.div(index, per_slot);
        let standard_slot = self.builder.add(data, word);
        let index_in_word = self.builder.mod_(index, per_slot);
        let size = self.builder.imm(array.size());
        let standard_offset = self.builder.mul(index_in_word, size);
        let standard_end = self.builder.current_block();
        self.builder.jump(join);

        self.builder.switch_to_block(join);
        let element_slot =
            self.builder.phi(vec![(inline_end, slot), (standard_end, standard_slot)]);
        let offset =
            self.builder.phi(vec![(inline_end, inline_offset), (standard_end, standard_offset)]);
        StorageAccess { slot: element_slot, location: array.element, offset: Some(offset) }
    }

    /// Appends `value` to the inline array at `slot`, through the shared helper of its element
    /// type. The value is already of the element type.
    pub(super) fn lower_inline_array_push(
        &mut self,
        slot: ValueId,
        value: ValueId,
        array: InlineArray,
        element: Ty<'gcx>,
        span: Span,
    ) -> Option<()> {
        let name = helper_name(sym::inline_array_push, inline_helper_suffix(array, element));
        let helper = self.lazy_helper(name, |this, function| {
            let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
            lowerer.lower_inline_array_push_helper(array, element, span)
        })?;
        // icall_void(inline_array_push, slot, value)
        self.builder.icall_void(helper, vec![slot, value]);
        Some(())
    }

    /// The access to the element a `push()` without a value appends to the inline array at
    /// `slot`.
    pub(super) fn inline_array_push_access(
        &mut self,
        slot: ValueId,
        array: InlineArray,
        element: Ty<'gcx>,
        span: Span,
    ) -> Option<StorageAccess> {
        // inline_array_push(slot, 0)
        // (_, inline, length) = header(slot)
        // access = element_at(slot, inline, length - 1)
        let zero = self.builder.imm(0);
        self.lower_inline_array_push(slot, zero, array, element, span)?;
        let (_, inline, length) = self.inline_array_header(slot);
        let one = self.builder.imm(1);
        let last = self.builder.sub(length, one);
        Some(self.inline_array_element_at(slot, inline, last, array))
    }

    fn lower_inline_array_push_helper(
        &mut self,
        array: InlineArray,
        element: Ty<'gcx>,
        span: Span,
    ) -> Option<()> {
        let slot = self.builder.add_param(MirType::I256);
        let value = self.builder.add_param(MirType::I256);
        let (header, inline, length) = self.inline_array_header(slot);
        let append = self.builder.create_block();
        let full = self.builder.create_block();
        let spill = self.builder.create_block();
        let standard = self.builder.create_block();
        // if (inline || header == 0) && length < capacity { append } else { full }
        let empty = self.builder.eq_zero(header);
        let inline_form = self.builder.or(inline, empty);
        let capacity = self.builder.imm(u64::from(array.capacity));
        let room = self.builder.lt(length, capacity);
        let fits = self.builder.and(inline_form, room);
        self.builder.branch(fits, append, full);

        // append:
        //   sstore(slot, (header + (1 << 248)) | encode(value) << length * size * 8)
        self.builder.switch_to_block(append);
        let one_length = self.builder.imm(U256::from(1) << 248);
        let grown = self.builder.add(header, one_length);
        let encoded = array.element.encode(&mut self.builder, value);
        let bits = self.builder.imm(array.size() * 8);
        let shift = self.builder.mul(length, bits);
        let placed = self.builder.shl(shift, encoded);
        let appended = self.builder.or(grown, placed);
        self.builder.sstore(slot, appended);
        self.builder.ret([]);

        // full: if inline { spill } else { standard }
        self.builder.switch_to_block(full);
        self.builder.branch(inline, spill, standard);

        // spill:
        //   data = keccak256(slot)
        //   elements = header & (2**248 - 1)
        //   if capacity < per_slot {
        //       sstore(data, elements | encode(value) << capacity * size * 8)
        //   } else {
        //       sstore(data, elements); sstore(data + 1, encode(value))
        //   }
        //   sstore(slot, capacity + 1)
        self.builder.switch_to_block(spill);
        let data = self.builder.storage_array_data_slot(slot);
        let elements_mask = self.builder.imm((U256::from(1) << 248) - U256::from(1));
        let elements = self.builder.and(header, elements_mask);
        let encoded = array.element.encode(&mut self.builder, value);
        let capacity_bytes = u64::from(array.capacity) * array.size();
        if u64::from(array.capacity) < array.per_slot() {
            let shift = self.builder.imm(capacity_bytes * 8);
            let placed = self.builder.shl(shift, encoded);
            let word = self.builder.or(elements, placed);
            self.builder.sstore(data, word);
        } else {
            self.builder.sstore(data, elements);
            let next = self.builder.add_u64_offset(data, 1);
            self.builder.sstore(next, encoded);
        }
        let spilled_length = self.builder.imm(u64::from(array.capacity) + 1);
        self.builder.sstore(slot, spilled_length);
        self.builder.ret([]);

        // standard:
        //   element = storage_array_element(slot, length); store(element, value)
        //   sstore(slot, length + 1)
        self.builder.switch_to_block(standard);
        let base = StorageAccess {
            slot,
            location: crate::mir::lower::storage::StorageLocation::word(U256::ZERO),
            offset: None,
        };
        let (access, new_length) = self.storage_array_push_access(base, element, span)?;
        self.store_storage_value(element, access, value, span)?;
        self.builder.sstore(slot, new_length);
        self.builder.ret([]);
        Some(())
    }

    /// Removes the last element of the inline array at `slot`, through the shared helper of its
    /// element type.
    pub(super) fn lower_inline_array_pop(
        &mut self,
        slot: ValueId,
        array: InlineArray,
        element: Ty<'gcx>,
        span: Span,
    ) -> Option<()> {
        let name = helper_name(sym::inline_array_pop, inline_helper_suffix(array, element));
        let helper = self.lazy_helper(name, |this, function| {
            let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
            lowerer.lower_inline_array_pop_helper(array, element, span)
        })?;
        // icall_void(inline_array_pop, slot)
        self.builder.icall_void(helper, vec![slot]);
        Some(())
    }

    fn lower_inline_array_pop_helper(
        &mut self,
        array: InlineArray,
        element: Ty<'gcx>,
        span: Span,
    ) -> Option<()> {
        // if length == 0 { panic(EmptyArrayPop) }
        let slot = self.builder.add_param(MirType::I256);
        let (header, inline, length) = self.inline_array_header(slot);
        let empty = self.builder.eq_zero(length);
        self.builder.panic_if(empty, PanicCode::EmptyArrayPop);
        let one = self.builder.imm(1);
        let last = self.builder.sub(length, one);
        let shrink = self.builder.create_block();
        let standard = self.builder.create_block();
        self.builder.branch(inline, shrink, standard);

        // shrink: sstore(slot, (header & ~(mask << last * size * 8)) - (1 << 248))
        self.builder.switch_to_block(shrink);
        let bits = self.builder.imm(array.size() * 8);
        let shift = self.builder.mul(last, bits);
        let mask = self.builder.imm(array.element.mask());
        let element_mask = self.builder.shl(shift, mask);
        let keep = self.builder.not(element_mask);
        let cleared = self.builder.and(header, keep);
        let one_length = self.builder.imm(U256::from(1) << 248);
        let shrunk = self.builder.sub(cleared, one_length);
        self.builder.sstore(slot, shrunk);
        self.builder.ret([]);

        // standard: sstore(slot, last); clear(element[last])
        self.builder.switch_to_block(standard);
        self.builder.sstore(slot, last);
        let access = self.storage_array_element_access(slot, last, element, true, span)?;
        self.clear_storage_access(element, access, span)?;
        self.builder.ret([]);
        Some(())
    }

    /// Deletes the inline array at `slot` of type `ty`: clears the slot of an inline array, and
    /// the standard array's length and data area otherwise.
    pub(super) fn clear_inline_array(
        &mut self,
        ty: Ty<'gcx>,
        access: StorageAccess,
        span: Span,
    ) -> Option<()> {
        // if inline { sstore(slot, 0) } else { clear_standard(slot) }
        let (_, inline, _) = self.inline_array_header(access.slot);
        let clear_inline = self.builder.create_block();
        let clear_standard = self.builder.create_block();
        let done = self.builder.create_block();
        self.builder.branch(inline, clear_inline, clear_standard);

        self.builder.switch_to_block(clear_inline);
        let zero = self.builder.imm(0);
        self.builder.sstore(access.slot, zero);
        self.builder.jump(done);

        self.builder.switch_to_block(clear_standard);
        let TyKind::DynArray(element) = ty.peel_refs().kind else { return None };
        self.clear_standard_storage_array(element, access, span)?;
        self.builder.jump(done);

        self.builder.switch_to_block(done);
        Some(())
    }

    /// Copies the inline array at `slot` into memory, through the shared helper of its element
    /// type.
    pub(super) fn load_inline_array(
        &mut self,
        slot: ValueId,
        array: InlineArray,
        element: Ty<'gcx>,
    ) -> Option<ValueId> {
        let name =
            helper_name(sym::load_storage_inline_array, inline_helper_suffix(array, element));
        let helper = self.lazy_helper(name, |this, function| {
            let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
            lowerer.lower_load_inline_array_helper(array, element)
        })?;
        // object = load_storage_inline_array(slot)
        let ty = MirType::MemoryObject(MemoryObjectKind::DynamicArray);
        Some(self.builder.icall(helper, vec![slot], ty))
    }

    fn lower_load_inline_array_helper(
        &mut self,
        array: InlineArray,
        element: Ty<'gcx>,
    ) -> Option<()> {
        let slot = self.builder.add_param(MirType::I256);
        let ty = MirType::MemoryObject(MemoryObjectKind::DynamicArray);
        self.builder.set_return_type(ty);
        let (header, inline, length) = self.inline_array_header(slot);
        let unpack = self.builder.create_block();
        let standard = self.builder.create_block();
        self.builder.branch(inline, unpack, standard);

        // unpack:
        //   array = alloc_dynamic_array(length)
        //   for i in 0..length { array[i] = decode(header >> i * size * 8) }
        self.builder.switch_to_block(unpack);
        let (object, layout) = self
            .builder
            .alloc_dynamic_word_array(length, AllocationSemantics::SOLIDITY_UNINITIALIZED);
        self.counted_loop(length, |this, index| {
            let bits = this.builder.imm(array.size() * 8);
            let shift = this.builder.mul(index, bits);
            let value = array.element.load_word(&mut this.builder, header, Some(shift));
            this.validate_enum(element, value);
            let value = this.encode_memory_scalar(element, value);
            // array[index] = value
            this.builder.memory_object_store_element(object, layout, index, value);
        });
        self.builder.ret([object]);

        // standard: array = load_storage_array(slot)
        self.builder.switch_to_block(standard);
        let loader = self.ensure_storage_array_helper(element)?;
        let standard_object = self.builder.icall(loader, vec![slot], ty);
        self.builder.ret([standard_object]);
        Some(())
    }
}

/// The suffix that names an inline array helper for the element type `element`.
fn inline_helper_suffix(array: InlineArray, element: Ty<'_>) -> String {
    let enum_variants = match element.peel_refs().kind {
        TyKind::Enum(_) => "_enum",
        _ => "",
    };
    format!("{}_{}{enum_variants}", array.size(), array.element.encoding as u8)
}
