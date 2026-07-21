//! ABI packed encoding lowering helpers.

use super::Lowerer;
use crate::{
    memory::EvmMemoryLayout,
    mir::{FunctionBuilder, MemoryObjectKind, Value, ValueId},
};
use alloy_primitives::U256;
use solar_ast::{ElementaryType, LitKind};
use solar_interface::{Symbol, sym};
use solar_sema::{
    builtins::Builtin,
    hir::{self, CallArgs, ExprKind},
    ty::{Ty, TyKind},
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

        let ptr = builder.fmp_object(crate::mir::MemoryObjectLayout::Bytes);

        // Data starts at ptr+32 (leaving room for the length word).
        let thirty_two = builder.imm_u64(32);
        let data_start = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);
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
        builder.set_memory_object_len(ptr, length, MemoryObjectKind::Bytes);
        let new_free_ptr = builder.add(ptr, total_size);
        builder.set_fmp(new_free_ptr);

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

        let data_start = if Self::static_packed_max_write(&packed_args)
            .is_some_and(|end| end <= EvmMemoryLayout::FMP_SLOT as usize)
        {
            builder.imm_u64(0)
        } else {
            builder.fmp()
        };
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

            // Calldata `bytes`/`string`: copy into a `[len][data]` memory buffer
            // and pack its data like any other dynamic bytes value.
            if self.expr_is_calldata_dynamic_bytes(arg) {
                if let Some((slice, _)) = self.calldata_dyn_slice(builder, arg) {
                    let ptr = self.materialize_calldata_bytes(builder, slice);
                    packed_args.push(PackedAbiArg::DynamicBytes(ptr));
                } else if let ExprKind::Slice(base, low, high) = &arg.kind
                    && let Some((slice, _)) = self.calldata_dyn_slice(builder, base)
                {
                    // A slice `base[low:high]` of calldata bytes.
                    let start = (*low).map(|e| self.lower_expr(builder, e));
                    let end = (*high).map(|e| self.lower_expr(builder, e));
                    let ptr = self.materialize_calldata_slice(builder, slice, start, end);
                    packed_args.push(PackedAbiArg::DynamicBytes(ptr));
                } else {
                    self.gcx
                        .dcx()
                        .err(
                            "codegen does not support packed encoding of calldata `bytes`/`string` yet",
                        )
                        .span(arg.span)
                        .emit();
                }
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

    /// Lowers an `abi.encodeWithSelector` selector argument as a left-aligned
    /// selector word. Sema can type a bare number literal as an integer, whose
    /// lowering is right-aligned; `bytes4`-typed values are already aligned by
    /// their producers.
    pub(super) fn lower_selector_word(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> ValueId {
        if let ExprKind::Lit(lit) = &expr.kind
            && let LitKind::Number(n) = &lit.kind
            && self.fixed_bytes_width_of_expr(expr).is_none()
        {
            return builder.imm_u256(*n << 224);
        }
        self.lower_expr(builder, expr)
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

        let mut i = 0;
        while i < args.len() {
            if let Some((consumed, len)) =
                self.try_write_static_packed_word(builder, base, offset, &args[i..])
            {
                offset += len;
                i += consumed;
                continue;
            }

            match &args[i] {
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
                    let len = builder.memory_object_len(ptr, MemoryObjectKind::Bytes);
                    let src = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);
                    self.mcopy(builder, dest, src, len, None);
                    base = builder.add(dest, len);
                    offset = 0;
                    is_static = false;
                }
            }
            i += 1;
        }

        if is_static {
            (data_start, Some(offset))
        } else {
            let end = self.offset_ptr(builder, base, offset);
            (end, None)
        }
    }

    /// Coalesces a run of static packed arguments that fit in one ABI-packed
    /// word into one `mstore`. This avoids overlapping word stores for common
    /// patterns like `abi.encodePacked("prefix", uint64(a), uint64(b), x)`.
    fn try_write_static_packed_word(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        base: ValueId,
        offset: u64,
        args: &[PackedAbiArg],
    ) -> Option<(usize, u64)> {
        let mut const_word = U256::ZERO;
        let mut terms = Vec::new();
        let mut len = 0usize;
        let mut consumed = 0usize;

        for arg in args {
            match arg {
                PackedAbiArg::Bytes(bytes) => {
                    if bytes.is_empty() {
                        consumed += 1;
                        continue;
                    }
                    if len + bytes.len() > 32 {
                        break;
                    }
                    let shift = (32 - len - bytes.len()) * 8;
                    const_word |= U256::from_be_slice(bytes) << shift;
                    len += bytes.len();
                    consumed += 1;
                }
                &PackedAbiArg::Value { value, size, left_aligned: false } if size < 32 => {
                    if size == 0 {
                        consumed += 1;
                        continue;
                    }
                    if len + size > 32 {
                        break;
                    }
                    let shift = (32 - len - size) * 8;
                    terms.push((value, shift));
                    len += size;
                    consumed += 1;
                }
                _ => break,
            }
        }

        if consumed < 2 || len == 0 || terms.is_empty() {
            return None;
        }

        let mut value = builder.imm_u256(const_word);
        for (term, shift) in terms {
            let shifted = if shift == 0 {
                term
            } else {
                let shift = builder.imm_u64(shift as u64);
                builder.shl(shift, term)
            };
            value = builder.or(value, shifted);
        }

        let dest = self.offset_ptr(builder, base, offset);
        builder.mstore(dest, value);
        Some((consumed, len as u64))
    }

    fn static_packed_max_write(args: &[PackedAbiArg]) -> Option<usize> {
        let mut offset = 0usize;
        let mut max_write = 0usize;
        let mut i = 0usize;

        while i < args.len() {
            if let Some((consumed, len)) = Self::static_packed_word_run(&args[i..]) {
                max_write = max_write.max(offset + 32);
                offset += len;
                i += consumed;
                continue;
            }

            match &args[i] {
                PackedAbiArg::Bytes(bytes) => {
                    for chunk in bytes.chunks(32) {
                        max_write = max_write.max(offset + 32);
                        offset += chunk.len();
                    }
                }
                PackedAbiArg::Value { size, .. } => {
                    max_write = max_write.max(offset + 32);
                    offset += *size;
                }
                PackedAbiArg::DynamicBytes(_) => return None,
            }
            i += 1;
        }

        Some(max_write)
    }

    fn static_packed_word_run(args: &[PackedAbiArg]) -> Option<(usize, usize)> {
        let mut len = 0usize;
        let mut consumed = 0usize;
        let mut has_value = false;

        for arg in args {
            match arg {
                PackedAbiArg::Bytes(bytes) => {
                    if bytes.is_empty() {
                        consumed += 1;
                        continue;
                    }
                    if len + bytes.len() > 32 {
                        break;
                    }
                    len += bytes.len();
                    consumed += 1;
                }
                PackedAbiArg::Value { size, left_aligned: false, .. } if *size < 32 => {
                    if *size == 0 {
                        consumed += 1;
                        continue;
                    }
                    if len + *size > 32 {
                        break;
                    }
                    len += *size;
                    consumed += 1;
                    has_value = true;
                }
                _ => break,
            }
        }

        (consumed >= 2 && len != 0 && has_value).then_some((consumed, len))
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
        matches!(self.ident_builtin(base), Some(Builtin::Abi))
            .then_some(args)
            .filter(|_| member.name == name)
    }

    /// Gets the packed size in bytes for an expression (used by abi.encodePacked).
    fn get_packed_size_from_expr(&self, expr: &hir::Expr<'_>) -> usize {
        if let ExprKind::Lit(lit) = &expr.kind {
            return match &lit.kind {
                LitKind::Str(_, bytes, _) => bytes.as_byte_str().len(),
                LitKind::Address(_) => 20,
                LitKind::Bool(_) => 1,
                LitKind::Number(_) | LitKind::Rational(_) | LitKind::Err(_) => 32,
            };
        }

        self.get_expr_type(expr).map(|ty| self.get_packed_size_from_ty(ty)).unwrap_or(32)
    }

    /// Gets the packed size from a sema type.
    fn get_packed_size_from_ty(&self, ty: Ty<'gcx>) -> usize {
        match ty.peel_refs().kind {
            TyKind::Elementary(elem) => match elem {
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
            TyKind::StringLiteral(_, size) => size.bytes() as usize,
            TyKind::IntLiteral(..) => 32,
            TyKind::Contract(_) | TyKind::Super(_) => 20,
            TyKind::Enum(_) => 1,
            TyKind::Udvt(inner, _) => self.get_packed_size_from_ty(inner),
            TyKind::Ref(inner, _) => self.get_packed_size_from_ty(inner),
            _ => 32,
        }
    }
}
