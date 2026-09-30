//! `@custom:solar-handle` fields: struct fields kept as indexes into a dictionary.
//!
//! A handle field of a struct documented `@custom:solar-handle <field> <dictionary>` keeps, in
//! place of its value, its handle: one plus the index of the value in the dictionary, a storage
//! array of the field's type, or zero for the value zero. A dictionary has fewer than 2^64
//! elements, so the storage layout gives the handle the 9 bytes of a `uint72`, which pack with the
//! struct's other fields where the value would take a word of its own.
//!
//! Reading the field reads the handle and, when it is nonzero, the dictionary element at
//! `keccak256(slot) + handle - 1`; a zero handle reads zero without touching the dictionary.
//! Type checking allows two writes: an element of the dictionary, `field = dictionary[index]`,
//! which checks `index` against the length as reading `dictionary[index]` does and stores
//! `index + 1`, and zero, which stores the zero handle as `delete` does. The element itself is
//! read only for the assignment's value, which is usually unused. The dictionary never shrinks and
//! its elements never change, so a handle keeps resolving to the value it was set to.

use super::*;

impl<'gcx> FunctionLowerer<'gcx, '_> {
    /// The dictionary of the handle field that the assignment target `lhs` names in storage.
    pub(super) fn handle_assignment_dictionary(&self, lhs: &hir::Expr<'_>) -> Option<VariableId> {
        let lhs = lhs.peel_parens();
        let ExprKind::Member(base, _) = lhs.kind else { return None };
        let dictionary = self.cx.storage.handle_dictionary(self.cx.gcx.resolved_variable(lhs)?)?;
        self.cx.gcx.type_of_expr(base.id)?.is_ref_at(DataLocation::Storage).then_some(dictionary)
    }

    /// Lowers `lhs = rhs` for the handle field `lhs` into `dictionary`, where `rhs` is an element
    /// of the dictionary or, as type checking ensures otherwise, zero.
    pub(super) fn lower_handle_assignment(
        &mut self,
        lhs: &hir::Expr<'_>,
        rhs: &hir::Expr<'_>,
        dictionary: VariableId,
    ) -> Option<ValueId> {
        let slot = self.handle_dictionary_slot(dictionary, lhs.span)?;
        if let ExprKind::Index(base, Some(index)) = rhs.peel_parens().kind
            && self.cx.gcx.resolved_variable(base.peel_parens()) == Some(dictionary)
        {
            // length = sload(dictionary_slot)
            // bounds_check(index, length)
            // store(lhs, index + 1)
            // value = sload(keccak256(dictionary_slot) + index)
            let index = self.lower_expr(index)?;
            let dictionary_slot = self.builder.imm(slot);
            let length = self.builder.sload(dictionary_slot);
            self.builder.bounds_check(index, length);
            let Some(access) = self.storage_access(lhs) else {
                return self.cx.report_unsupported(lhs.span, "storage access");
            };
            let one = self.builder.imm(1);
            let handle = self.builder.add(index, one);
            self.cx.storage.store_at(&mut self.builder, access.location, access.slot, handle);
            let data = self.builder.imm(data_slot(slot));
            let element = self.builder.add(data, index);
            return Some(self.builder.sload(element));
        }
        // store(lhs, 0)
        let Some(access) = self.storage_access(lhs) else {
            return self.cx.report_unsupported(lhs.span, "storage access");
        };
        let zero = self.builder.imm(U256::ZERO);
        self.cx.storage.store_at(&mut self.builder, access.location, access.slot, zero);
        Some(zero)
    }

    /// The value of a handle field into `dictionary` whose handle is `handle`: zero for the zero
    /// handle, and the dictionary element `handle - 1` otherwise.
    pub(super) fn resolve_handle(
        &mut self,
        handle: ValueId,
        dictionary: VariableId,
        span: Span,
    ) -> Option<ValueId> {
        let slot = self.handle_dictionary_slot(dictionary, span)?;
        // if handle != 0 { value = sload(keccak256(dictionary_slot) - 1 + handle) }
        // else { value = 0 }
        let zero = self.builder.imm(U256::ZERO);
        let set = self.builder.ne_zero(handle);
        let entry = self.builder.current_block();
        let load_block = self.builder.create_block();
        let join = self.builder.create_block();
        self.builder.branch(set, load_block, join);

        self.builder.switch_to_block(load_block);
        let base = self.builder.imm(data_slot(slot).wrapping_sub(U256::from(1)));
        let element_slot = self.builder.add(base, handle);
        let element = self.builder.sload(element_slot);
        let load_end = self.builder.current_block();
        self.builder.jump(join);

        self.builder.switch_to_block(join);
        Some(self.builder.phi(vec![(entry, zero), (load_end, element)]))
    }

    /// The slot of the handle dictionary `dictionary` in the contract's storage.
    fn handle_dictionary_slot(&self, dictionary: VariableId, span: Span) -> Option<U256> {
        let Some(location) = self.cx.storage.get(dictionary) else {
            return self.cx.report_unsupported(span, "handle field outside its contract");
        };
        Some(location.slot)
    }
}

/// The slot of element zero of the storage array at `slot`: `keccak256(slot)`.
fn data_slot(slot: U256) -> U256 {
    U256::from_be_bytes(keccak256(slot.to_be_bytes::<32>()).0)
}
