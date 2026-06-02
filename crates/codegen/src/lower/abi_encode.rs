//! ABI encoding of external function return values.
//!
//! Driven by the sema [`Ty`] of each return (obtained via `gcx.type_of_hir_ty`),
//! this lays out the Solidity ABI tuple encoding (head slots + dynamic tail) for
//! a function's return values into a memory buffer and terminates the function
//! with [`crate::mir::Terminator::ReturnData`]. Internal-frame functions do NOT
//! use this — they return raw words/pointers through the internal frame.

use super::Lowerer;
use crate::mir::{FunctionBuilder, ValueId};
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

    /// Encodes a tuple of `(value, type)` items as an ABI head at `dest`. Phase 1
    /// handles statically-encoded items only; the returned size is the (constant)
    /// head size. Dynamic tails are added in a later phase.
    pub(super) fn abi_encode_tuple(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        items: &[(ValueId, Ty<'gcx>)],
        dest: ValueId,
    ) -> ValueId {
        let head_size: u64 = items.iter().map(|&(_, t)| self.abi_head_size(t)).sum();
        let mut head_off = 0u64;
        for &(val, ty) in items {
            let head_addr = self.offset_ptr(builder, dest, head_off);
            self.abi_encode_static(builder, ty, val, head_addr);
            head_off += self.abi_head_size(ty);
        }
        builder.imm_u64(head_size)
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

    /// Allocates a return buffer, ABI-encodes `items` into it, and terminates the
    /// function with `ReturnData`. Phase 1: all items must be statically encoded.
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

    /// Whether every return of the function currently being lowered is statically
    /// encoded (so Phase 1's static path applies).
    pub(super) fn return_tys_all_static(&self) -> bool {
        self.current_return_tys.iter().all(|&t| !self.abi_is_dynamic(t))
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
        if external && self.return_tys_all_static() {
            self.emit_abi_return(builder, &items);
            return;
        }
        if external {
            // Legacy fallback for dynamic returns (until later phases): expand a
            // struct to its fields; a single dynamic word-array is ABI-encoded at
            // the backend via `returns_dynamic_array`.
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
                .map(|(i, e)| (self.lower_expr(builder, e), tys[i]))
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
        let first = self.lower_expr(builder, expr);
        let mut items = vec![(first, tys[0])];
        for (i, &ty) in tys.iter().enumerate().skip(1) {
            let off = builder.imm_u64((i * 32) as u64);
            items.push((builder.mload(off), ty));
        }
        items
    }
}
