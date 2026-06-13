//! ABI packed encoding lowering helpers.

use super::Lowerer;
use crate::mir::{FunctionBuilder, Value, ValueId};
use alloy_primitives::U256;
use solar_ast::{ElementaryType, Ident, LitKind, StrKind};
use solar_interface::{Symbol, sym};
use solar_sema::{
    builtins::Builtin,
    hir::{self, CallArgs, ExprKind},
    ty::TyKind,
};

enum PackedAbiArg {
    /// Compile-time literal bytes, packed without padding.
    Bytes(Vec<u8>),
    /// A single value occupying the top `size` bytes of its word.
    Value { value: ValueId, size: usize, left_aligned: bool },
    /// A memory `bytes`/`string` pointer (`[length][data...]`) whose data is
    /// copied without padding.
    DynamicBytes(ValueId),
}

impl<'gcx> Lowerer<'gcx> {
    /// Lowers abi.encodePacked with proper tight packing.
    /// Returns bytes memory (pointer to length + data).
    pub(super) fn lower_abi_encode_packed(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        args: &CallArgs<'_>,
    ) -> ValueId {
        // Arguments are fully lowered before the buffer is touched: nested
        // calls in arguments may allocate memory of their own. The writes
        // below allocate nothing, so filling the buffer past the unbumped
        // free memory pointer and reserving it afterwards is safe.
        let packed_args = self.collect_packed_abi_args(builder, args);

        let free_mem_ptr_slot = builder.imm_u64(0x40);
        let ptr = builder.mload(free_mem_ptr_slot);

        // Data starts at ptr+32 (leaving room for the length word).
        let thirty_two = builder.imm_u64(32);
        let data_start = builder.add(ptr, thirty_two);
        let (end, static_len) = self.write_packed_abi_args(builder, data_start, &packed_args);

        // Finalize the allocation: length word + data padded to a word
        // boundary, keeping the free memory pointer aligned.
        let (length, total_size) = match static_len {
            Some(len) => (builder.imm_u64(len), builder.imm_u64(32 + len.div_ceil(32) * 32)),
            None => {
                let length = builder.sub(end, data_start);
                let thirty_one = builder.imm_u64(31);
                let rounded = builder.add(length, thirty_one);
                let mask = builder.not(thirty_one);
                let aligned = builder.and(rounded, mask);
                (length, builder.add(aligned, thirty_two))
            }
        };
        builder.mstore(ptr, length);
        let new_free_ptr = builder.add(ptr, total_size);
        builder.mstore(free_mem_ptr_slot, new_free_ptr);

        // Return pointer to the bytes value
        ptr
    }

    /// Lowers `keccak256(abi.encodePacked(...))` without materializing a temporary bytes object:
    /// the packed data is staged at the unbumped free memory pointer and hashed in place.
    pub(super) fn lower_keccak_abi_encode_packed(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        args: &CallArgs<'_>,
    ) -> ValueId {
        let packed_args = self.collect_packed_abi_args(builder, args);

        let free_mem_ptr_slot = builder.imm_u64(0x40);
        let data_start = builder.mload(free_mem_ptr_slot);
        let (end, static_len) = self.write_packed_abi_args(builder, data_start, &packed_args);

        let size = match static_len {
            Some(len) => builder.imm_u64(len),
            None => builder.sub(end, data_start),
        };
        builder.keccak256(data_start, size)
    }

    fn collect_packed_abi_args(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        args: &CallArgs<'_>,
    ) -> Vec<PackedAbiArg> {
        let mut packed_args = Vec::with_capacity(args.len());

        for arg in args.exprs() {
            if let ExprKind::Lit(lit) = &arg.kind
                && let LitKind::Str(_, bytes, _) = &lit.kind
            {
                let bytes = bytes.as_byte_str().to_vec();
                packed_args.push(PackedAbiArg::Bytes(bytes));
                continue;
            }

            if self.expr_is_calldata_dynamic_bytes(arg) {
                self.gcx
                    .dcx()
                    .err(
                        "codegen does not support packed encoding of calldata `bytes`/`string` yet",
                    )
                    .span(arg.span)
                    .emit();
                continue;
            }

            // `bytes`/`string` values: their data is packed without padding.
            if let Some(ty) = self.get_expr_type(arg)
                && matches!(
                    ty.peel_refs().kind,
                    TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
                )
            {
                let ptr = self.lower_expr(builder, arg);
                packed_args.push(PackedAbiArg::DynamicBytes(ptr));
                continue;
            }

            let size = self.get_packed_size_from_expr(arg);
            let left_aligned = self.expr_is_fixed_bytes(arg);
            let value = self.lower_expr(builder, arg);
            packed_args.push(PackedAbiArg::Value { value, size, left_aligned });
        }

        packed_args
    }

    pub(super) fn expr_is_fixed_bytes(&self, expr: &hir::Expr<'_>) -> bool {
        self.get_expr_type(expr).is_some_and(|ty| {
            matches!(ty.peel_refs().kind, TyKind::Elementary(ElementaryType::FixedBytes(_)))
        })
    }

    pub(super) fn fixed_bytes_width_of_expr(&self, expr: &hir::Expr<'_>) -> Option<u8> {
        match self.get_expr_type(expr)?.peel_refs().kind {
            TyKind::Elementary(ElementaryType::FixedBytes(n)) => Some(n.bytes()),
            _ => None,
        }
    }

    pub(super) fn expr_is_calldata_dynamic_bytes(&self, expr: &hir::Expr<'_>) -> bool {
        let Some(ty) = self.get_expr_type(expr) else { return false };
        match ty.kind {
            TyKind::Ref(inner, solar_ast::DataLocation::Calldata) => matches!(
                inner.kind,
                TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
            ),
            TyKind::Slice(array) => {
                array.is_ref_at(solar_ast::DataLocation::Calldata)
                    && matches!(
                        array.peel_refs().kind,
                        TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
                    )
            }
            _ => false,
        }
    }

    /// Writes packed arguments starting at `data_start`. Returns the end
    /// cursor (one past the last packed byte) and, when every argument has a
    /// compile-time size, the total packed length. Word writes may spill up to
    /// 31 bytes past their packed size; later writes overwrite the spill, and
    /// trailing spill lands in dead padding/scratch.
    fn write_packed_abi_args(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        data_start: ValueId,
        args: &[PackedAbiArg],
    ) -> (ValueId, Option<u64>) {
        // The cursor is `base + offset`: the offset stays compile-time between
        // dynamic (runtime-length) arguments, which rebase the cursor.
        let mut base = data_start;
        let mut offset: u64 = 0;
        let mut is_static = true;

        for arg in args {
            match arg {
                PackedAbiArg::Bytes(bytes) => {
                    for chunk in bytes.chunks(32) {
                        let mut padded = [0u8; 32];
                        padded[..chunk.len()].copy_from_slice(chunk);
                        let val = builder.imm_u256(U256::from_be_bytes(padded));
                        let dest = self.offset_ptr(builder, base, offset);
                        builder.mstore(dest, val);
                        offset += chunk.len() as u64;
                    }
                }
                &PackedAbiArg::Value { value, size, .. } if size >= 32 => {
                    // Full 32 bytes - use MSTORE
                    let dest = self.offset_ptr(builder, base, offset);
                    builder.mstore(dest, value);
                    offset += size as u64;
                }
                &PackedAbiArg::Value { value, size, left_aligned } => {
                    // Less than 32 bytes: the word's top `size` bytes are
                    // written. Fixed-bytes values are already left-aligned;
                    // every other value type is right-aligned and shifts up.
                    // Fixed-bytes constants are disambiguated by value:
                    // number-literal casts carry the raw (right-aligned)
                    // number.
                    let needs_shift = if left_aligned {
                        match builder.func().value(value) {
                            Value::Immediate(imm) => imm
                                .as_u256()
                                .is_some_and(|v| !v.is_zero() && v < U256::from(1) << (8 * size)),
                            _ => false,
                        }
                    } else {
                        true
                    };
                    let value = if needs_shift {
                        let shift_bits = (32 - size) * 8;
                        let shift_amount = builder.imm_u64(shift_bits as u64);
                        builder.shl(shift_amount, value)
                    } else {
                        value
                    };

                    let dest = self.offset_ptr(builder, base, offset);
                    builder.mstore(dest, value);
                    offset += size as u64;
                }
                &PackedAbiArg::DynamicBytes(ptr) => {
                    let dest = self.offset_ptr(builder, base, offset);
                    let len = builder.mload(ptr);
                    let word = builder.imm_u64(32);
                    let src = builder.add(ptr, word);
                    self.mcopy(builder, dest, src, len, None);
                    base = builder.add(dest, len);
                    offset = 0;
                    is_static = false;
                }
            }
        }

        if is_static {
            (data_start, Some(offset))
        } else {
            let end = self.offset_ptr(builder, base, offset);
            (end, None)
        }
    }

    pub(super) fn abi_encode_packed_call_args<'a>(
        &self,
        expr: &'a hir::Expr<'a>,
    ) -> Option<&'a CallArgs<'a>> {
        self.abi_member_call_args(expr, sym::encodePacked)
    }

    pub(super) fn abi_encode_call_args<'a>(
        &self,
        expr: &'a hir::Expr<'a>,
    ) -> Option<&'a CallArgs<'a>> {
        self.abi_member_call_args(expr, sym::encode)
    }

    /// Returns the arguments of an `abi.<member>(...)` call expression.
    fn abi_member_call_args<'a>(
        &self,
        expr: &'a hir::Expr<'a>,
        name: Symbol,
    ) -> Option<&'a CallArgs<'a>> {
        let ExprKind::Call(callee, args, _) = &expr.kind else {
            return None;
        };
        let ExprKind::Member(base, member) = &callee.kind else {
            return None;
        };
        let ExprKind::Ident(res_slice) = &base.kind else {
            return None;
        };
        matches!(res_slice.first(), Some(hir::Res::Builtin(Builtin::Abi)))
            .then_some(args)
            .filter(|_| member.name == name)
    }

    /// Gets the packed size in bytes for an expression (used by abi.encodePacked).
    fn get_packed_size_from_expr(&self, expr: &hir::Expr<'_>) -> usize {
        match &expr.kind {
            ExprKind::Lit(lit) => match &lit.kind {
                LitKind::Str(StrKind::Hex, bytes, _) => bytes.as_byte_str().len(),
                LitKind::Str(_, bytes, _) => bytes.as_byte_str().len(),
                LitKind::Address(_) => 20,
                LitKind::Bool(_) => 1,
                LitKind::Number(_) | LitKind::Rational(_) => 32,
                LitKind::Err(_) => 32,
            },
            ExprKind::Ident(res_slice) => {
                if let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first() {
                    let var = self.gcx.hir.variable(*var_id);
                    return self.get_packed_size_from_hir_type(&var.ty);
                }
                32
            }
            ExprKind::Call(callee, _, _) => {
                if let ExprKind::Type(ty) = &callee.kind {
                    return self.get_packed_size_from_hir_type(ty);
                }
                32
            }
            ExprKind::Member(base, member) => {
                self.get_member_packed_size(base, *member).unwrap_or(32)
            }
            _ => 32,
        }
    }

    /// Gets the packed size from an HIR type.
    fn get_packed_size_from_hir_type(&self, ty: &hir::Type<'_>) -> usize {
        match &ty.kind {
            hir::TypeKind::Elementary(elem) => match elem {
                ElementaryType::FixedBytes(size) => size.bytes() as usize,
                ElementaryType::Address(_) => 20,
                ElementaryType::Bool => 1,
                ElementaryType::Int(size) | ElementaryType::UInt(size) => size.bytes() as usize,
                // Dynamic - handled specially.
                ElementaryType::String | ElementaryType::Bytes => 32,
                ElementaryType::Fixed(size, _) | ElementaryType::UFixed(size, _) => {
                    size.bytes() as usize
                }
            },
            hir::TypeKind::Custom(item_id) => match item_id {
                hir::ItemId::Enum(_) => 1,
                hir::ItemId::Contract(_) => 20,
                _ => 32,
            },
            // Everything else: 32 bytes default
            _ => 32,
        }
    }

    fn get_member_packed_size(&self, base: &hir::Expr<'_>, member: Ident) -> Option<usize> {
        let struct_id = self.expr_struct_id(base)?;
        let strukt = self.gcx.hir.strukt(struct_id);
        for &field_id in strukt.fields {
            let field = self.gcx.hir.variable(field_id);
            if field.name.is_some_and(|name| name.name == member.name) {
                return Some(self.get_packed_size_from_hir_type(&field.ty));
            }
        }
        None
    }
}
