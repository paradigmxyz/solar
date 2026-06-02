//! ABI encoding of external function return values.
//!
//! Driven by the sema [`Ty`] of each return (obtained via `gcx.type_of_hir_ty`),
//! this lays out the Solidity ABI tuple encoding (head slots + dynamic tail) for
//! a function's return values into a memory buffer and terminates the function
//! with [`crate::mir::Terminator::ReturnData`]. Internal-frame functions do NOT
//! use this — they return raw words/pointers through the internal frame.

use super::Lowerer;
use crate::mir::{FunctionBuilder, ValueId};
use alloy_primitives::U256;
use solar_ast::ElementaryType;
use solar_sema::ty::{Ty, TyKind};

impl<'gcx> Lowerer<'gcx> {
    /// Whether `ty` is encoded dynamically (offset slot in the head + data in the
    /// tail): `bytes`/`string`, dynamic arrays, and any aggregate containing one.
    pub(super) fn abi_is_dynamic(&self, ty: Ty<'gcx>) -> bool {
        ty.peel_refs().is_dynamically_encoded(self.gcx)
    }

    /// Static ABI head size, in bytes, of one top-level item: 32 for any dynamic
    /// type (an offset slot), the recursive sum for a static struct, `N *
    /// head(T)` for a static `T[N]`, and 32 for every value type.
    pub(super) fn abi_head_size(&self, ty: Ty<'gcx>) -> u64 {
        let ty = ty.peel_refs();
        if ty.is_dynamically_encoded(self.gcx) {
            return 32;
        }
        match ty.kind {
            TyKind::Array(elem, n) => n.to::<u64>() * self.abi_head_size(elem),
            TyKind::Struct(id) => {
                self.gcx.struct_field_types(id).iter().map(|&f| self.abi_head_size(f)).sum()
            }
            _ => 32,
        }
    }

    /// `base + off` (or `base` when `off == 0`).
    fn offset_ptr(&self, builder: &mut FunctionBuilder<'_>, base: ValueId, off: u64) -> ValueId {
        if off == 0 {
            base
        } else {
            let o = builder.imm_u64(off);
            builder.add(base, o)
        }
    }

    /// Encodes a tuple of `(value, type)` items at `dest` using ABI head/tail
    /// layout. Static items are written inline in the head; dynamic items write a
    /// relative tail offset in the head and append their body to the shared tail.
    pub(super) fn abi_encode_tuple(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        items: &[(ValueId, Ty<'gcx>)],
        dest: ValueId,
    ) -> ValueId {
        let head_size: u64 = items.iter().map(|&(_, t)| self.abi_head_size(t)).sum();
        let has_dynamic = items.iter().any(|&(_, ty)| self.abi_is_dynamic(ty));
        if !has_dynamic {
            let mut head_off = 0u64;
            for &(val, ty) in items {
                let head_addr = self.offset_ptr(builder, dest, head_off);
                self.abi_encode_static(builder, ty, val, head_addr);
                head_off += self.abi_head_size(ty);
            }
            return builder.imm_u64(head_size);
        }

        let head_size_val = builder.imm_u64(head_size);
        let mut tail = builder.add(dest, head_size_val);
        let mut head_off = 0u64;
        for &(val, ty) in items {
            let head_addr = self.offset_ptr(builder, dest, head_off);
            tail = self.abi_encode_value(builder, ty, val, head_addr, dest, tail);
            head_off += self.abi_head_size(ty);
        }
        builder.sub(tail, dest)
    }

    /// Encodes one ABI tuple element. Dynamic values write their tail offset into
    /// `head_addr` and append the dynamic body at `tail`; static values are
    /// encoded directly into the head.
    fn abi_encode_value(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: Ty<'gcx>,
        value: ValueId,
        head_addr: ValueId,
        tuple_base: ValueId,
        tail: ValueId,
    ) -> ValueId {
        if self.abi_is_dynamic(ty) {
            let rel_off = builder.sub(tail, tuple_base);
            builder.mstore(head_addr, rel_off);
            self.abi_encode_dynamic_body(builder, ty, value, tail)
        } else {
            self.abi_encode_static(builder, ty, value, head_addr);
            tail
        }
    }

    /// Encodes a statically-encoded value into the head region at `head_addr`.
    /// Value types write one word; a static struct/array recurses field/element
    /// wise (each field/element word read from the value's memory pointer).
    fn abi_encode_static(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: Ty<'gcx>,
        value: ValueId,
        head_addr: ValueId,
    ) {
        let ty = ty.peel_refs();
        match ty.kind {
            TyKind::Struct(id) => {
                let fields = self.gcx.struct_field_types(id);
                let mut field_head = head_addr;
                for (i, &fty) in fields.iter().enumerate() {
                    let slot = self.offset_ptr(builder, value, (i as u64) * 32);
                    let fval = builder.mload(slot);
                    self.abi_encode_static(builder, fty, fval, field_head);
                    let fhs = self.abi_head_size(fty);
                    field_head = self.offset_ptr(builder, field_head, fhs);
                }
            }
            TyKind::Array(elem, n) => {
                let mut elem_head = head_addr;
                let ehs = self.abi_head_size(elem);
                for i in 0..n.to::<u64>() {
                    let slot = self.offset_ptr(builder, value, i * 32);
                    let ev = builder.mload(slot);
                    self.abi_encode_static(builder, elem, ev, elem_head);
                    elem_head = self.offset_ptr(builder, elem_head, ehs);
                }
            }
            _ => {
                builder.mstore(head_addr, value);
            }
        }
    }

    /// Encodes a dynamic value body at `dst` and returns the advanced tail
    /// cursor. Phase 2 supports dynamic leaves that do not require runtime
    /// element loops: bytes/string, dynamic arrays of word elements, fixed arrays
    /// containing dynamic no-loop elements, and dynamic structs whose fields are
    /// themselves no-loop encodable.
    fn abi_encode_dynamic_body(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: Ty<'gcx>,
        value: ValueId,
        dst: ValueId,
    ) -> ValueId {
        match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                self.abi_encode_bytes_or_string_body(builder, value, dst)
            }
            TyKind::DynArray(elem) if self.abi_is_word_element(elem) => {
                let len = builder.mload(value);
                builder.mstore(dst, len);

                let word = builder.imm_u64(32);
                let bytes = builder.mul(len, word);
                let data_dst = builder.add(dst, word);
                let data_src = builder.add(value, word);
                builder.mcopy(data_dst, data_src, bytes);
                builder.add(data_dst, bytes)
            }
            TyKind::Array(elem, n) => {
                let mut items = Vec::new();
                for i in 0..n.to::<u64>() {
                    let slot = self.offset_ptr(builder, value, i * 32);
                    items.push((builder.mload(slot), elem));
                }
                let size = self.abi_encode_tuple(builder, &items, dst);
                builder.add(dst, size)
            }
            TyKind::Struct(id) => {
                let fields: Vec<_> = self.gcx.struct_field_types(id).to_vec();
                let mut items = Vec::with_capacity(fields.len());
                for (i, &fty) in fields.iter().enumerate() {
                    let slot = self.offset_ptr(builder, value, (i as u64) * 32);
                    items.push((builder.mload(slot), fty));
                }
                let size = self.abi_encode_tuple(builder, &items, dst);
                builder.add(dst, size)
            }
            TyKind::Tuple(fields) => {
                let fields: Vec<_> = fields.to_vec();
                let mut items = Vec::with_capacity(fields.len());
                for (i, &fty) in fields.iter().enumerate() {
                    let slot = self.offset_ptr(builder, value, (i as u64) * 32);
                    items.push((builder.mload(slot), fty));
                }
                let size = self.abi_encode_tuple(builder, &items, dst);
                builder.add(dst, size)
            }
            _ => unreachable!("unsupported dynamic ABI return type: {:?}", ty.peel_refs()),
        }
    }

    fn abi_encode_bytes_or_string_body(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        value: ValueId,
        dst: ValueId,
    ) -> ValueId {
        let len = builder.mload(value);
        builder.mstore(dst, len);

        let word = builder.imm_u64(32);
        let thirty_one = builder.imm_u64(31);
        let padded_mask = builder.not(thirty_one);
        let len_plus_rounding = builder.add(len, thirty_one);
        let padded = builder.and(len_plus_rounding, padded_mask);
        let data_dst = builder.add(dst, word);

        let zero_block = builder.create_block();
        let copy_block = builder.create_block();
        let is_empty = builder.iszero(padded);
        builder.branch(is_empty, copy_block, zero_block);

        builder.switch_to_block(zero_block);
        let last_word_off = builder.sub(padded, word);
        let last_word = builder.add(data_dst, last_word_off);
        let zero = builder.imm_u64(0);
        builder.mstore(last_word, zero);
        builder.jump(copy_block);

        builder.switch_to_block(copy_block);
        let data_src = builder.add(value, word);
        builder.mcopy(data_dst, data_src, len);
        builder.add(data_dst, padded)
    }

    /// Allocates a return buffer, ABI-encodes `items` into it, and terminates the
    /// function with `ReturnData`.
    pub(super) fn emit_abi_return(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        items: &[(ValueId, Ty<'gcx>)],
    ) {
        if items.is_empty() {
            let zero = builder.imm_u64(0);
            builder.ret_data(zero, zero);
            return;
        }
        let head_size: u64 = items.iter().map(|&(_, t)| self.abi_head_size(t)).sum();
        let buf = self.allocate_memory(builder, head_size);
        let size = self.abi_encode_tuple(builder, items, buf);
        builder.ret_data(buf, size);
    }

    /// Whether every return of the function currently being lowered can be ABI
    /// encoded without runtime element loops.
    pub(super) fn return_tys_no_loop(&self) -> bool {
        self.current_return_tys.iter().all(|&t| !self.abi_needs_loop(t))
    }

    /// Whether ABI encoding this type requires a runtime loop over non-word
    /// dynamic array elements.
    pub(super) fn abi_needs_loop(&self, ty: Ty<'gcx>) -> bool {
        match ty.peel_refs().kind {
            TyKind::DynArray(elem) => !self.abi_is_word_element(elem),
            TyKind::Array(elem, _) => self.abi_needs_loop(elem),
            TyKind::Struct(id) => {
                self.gcx.struct_field_types(id).iter().any(|&f| self.abi_needs_loop(f))
            }
            TyKind::Tuple(fields) => fields.iter().any(|&f| self.abi_needs_loop(f)),
            TyKind::Udvt(inner, _) => self.abi_needs_loop(inner),
            _ => false,
        }
    }

    fn abi_is_word_element(&self, ty: Ty<'gcx>) -> bool {
        match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => false,
            TyKind::Elementary(_) | TyKind::Enum(_) | TyKind::Contract(_) => true,
            TyKind::Udvt(inner, _) => self.abi_is_word_element(inner),
            _ => false,
        }
    }

    fn lower_return_value_for_ty(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &solar_sema::hir::Expr<'_>,
        ty: Ty<'gcx>,
    ) -> ValueId {
        if matches!(
            ty.peel_refs().kind,
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
        ) && let solar_sema::hir::ExprKind::Lit(lit) = &expr.kind
            && let Some(ptr) = self.lower_string_literal_to_memory(builder, lit)
        {
            return ptr;
        }
        self.lower_expr(builder, expr)
    }

    /// Decodes a short storage `bytes`/`string` slot into the memory layout the
    /// ABI encoder expects: `[length][data-word]`.
    pub(super) fn materialize_storage_bytes(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        slot: ValueId,
    ) -> ValueId {
        let word = builder.sload(slot);
        let one = builder.imm_u64(1);
        let long_bit = builder.and(word, one);
        let is_long = builder.eq(long_bit, one);
        let short_block = builder.create_block();
        let long_block = builder.create_block();
        builder.branch(is_long, long_block, short_block);

        builder.switch_to_block(long_block);
        // TODO(phase4): copy long storage bytes/string data from keccak256(slot).
        // Phase 2 intentionally rejects this instead of returning corrupt data.
        builder.invalid();

        builder.switch_to_block(short_block);
        let ptr = self.allocate_memory(builder, 64);
        let low_byte_mask = builder.imm_u64(0xff);
        let len_low = builder.and(word, low_byte_mask);
        let shift = builder.imm_u64(1);
        let len = builder.shr(shift, len_low);
        builder.mstore(ptr, len);
        let data_mask = builder.imm_u256(U256::MAX - U256::from(0xffu64));
        let data = builder.and(word, data_mask);
        let word_size = builder.imm_u64(32);
        let data_ptr = builder.add(ptr, word_size);
        builder.mstore(data_ptr, data);
        ptr
    }

    /// Terminates the current function for the implicit-return epilogue's gathered
    /// `items` (one per declared return). External entries whose returns are all
    /// statically encoded go through the ABI encoder; remaining (dynamic) external
    /// cases use the legacy expand-and-`Return` path; internal-frame functions
    /// return raw words/pointers.
    pub(super) fn finish_external_or_internal_return(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        items: Vec<(ValueId, Ty<'gcx>)>,
        external: bool,
    ) {
        if external && self.return_tys_no_loop() {
            self.emit_abi_return(builder, &items);
            return;
        }
        if external {
            // Legacy fallback for loop-needing dynamic returns until Phase 3.
            let mut ret_vals = Vec::new();
            for (val, ty) in items {
                if let TyKind::Struct(struct_id) = ty.peel_refs().kind {
                    ret_vals.extend(self.load_struct_return_values(builder, struct_id, val));
                } else {
                    ret_vals.push(val);
                }
            }
            builder.ret(ret_vals);
            return;
        }
        let vals: Vec<ValueId> = items.into_iter().map(|(v, _)| v).collect();
        builder.ret(vals);
    }

    /// Gathers `(value, type)` for each declared return of an explicit `return`
    /// expression (tuple / ternary-tuple / single / multi-value call), pairing
    /// values with the function's declared return types.
    pub(super) fn gather_return_items(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        value: Option<&solar_sema::hir::Expr<'_>>,
    ) -> Vec<(ValueId, Ty<'gcx>)> {
        use solar_sema::hir::ExprKind;
        let tys = self.current_return_tys.clone();
        let Some(expr) = value else {
            return Vec::new();
        };
        if let ExprKind::Tuple(elements) = &expr.kind {
            return elements
                .iter()
                .flatten()
                .enumerate()
                .map(|(i, e)| (self.lower_return_value_for_ty(builder, e, tys[i]), tys[i]))
                .collect();
        }
        if let Some(arity) = self.get_ternary_tuple_arity(expr) {
            let _ = self.lower_expr(builder, expr);
            return (0..arity)
                .map(|i| {
                    let off = builder.imm_u64((i * 32) as u64);
                    (builder.mload(off), tys[i])
                })
                .collect();
        }
        let first = self.lower_return_value_for_ty(builder, expr, tys[0]);
        let mut items = vec![(first, tys[0])];
        for (i, &ty) in tys.iter().enumerate().skip(1) {
            let off = builder.imm_u64((i * 32) as u64);
            items.push((builder.mload(off), ty));
        }
        items
    }
}
