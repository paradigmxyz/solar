//! Expression lowering.

use super::{
    Lowerer,
    checked_arith::{ArithmeticInfo, PanicCode},
};
use crate::mir::{FunctionBuilder, MemoryObjectKind, MirType, ValueId};
use alloy_primitives::U256;
use solar_ast::{LitKind, StrKind};
use solar_interface::{Ident, Span, Symbol, kw, sym};
use solar_sema::{
    builtins::Builtin,
    hir::{self, CallArgs, ElementaryType, ExprKind},
    ty::{Ty, TyKind},
};

pub(super) struct MappingElementSlot {
    pub(super) slot: ValueId,
    pub(super) value_is_mapping: bool,
}

/// The base storage slot of a mapping: a compile-time constant for a state
/// variable, or a runtime value for a storage-reference parameter/local.
enum MappingBaseSlot {
    Const(u64),
    Value(ValueId),
}

impl<'gcx> Lowerer<'gcx> {
    /// Lowers an expression to MIR.
    pub(super) fn lower_expr(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> ValueId {
        match &expr.kind {
            ExprKind::Lit(lit) => {
                // A numeric literal typed `bytesN` uses the left-aligned word
                // representation (data in the high bytes), not the right-aligned
                // integer value, so e.g. `x == 0x11223344` compares correctly.
                if let LitKind::Number(n) = &lit.kind
                    && let Some(width) = self.fixed_bytes_width_of_expr(expr)
                    && width < 32
                {
                    let aligned = *n << (usize::from(32 - width) * 8);
                    return builder.imm_u256(aligned);
                }
                self.lower_literal(builder, lit)
            }

            ExprKind::Ident(res_slice) => {
                if res_slice.is_empty() {
                    builder.imm_u64(0)
                } else if let Some(res) = self.ident_res(expr) {
                    self.lower_ident(builder, &res)
                } else {
                    // The raw resolution set is ambiguous (an overloaded
                    // function or event referenced as a value); the type
                    // checker records disambiguation only for callees.
                    self.err_value(
                        builder,
                        expr.span,
                        "codegen cannot resolve an overloaded identifier used as a value",
                    )
                }
            }

            ExprKind::Binary(lhs, op, rhs) => {
                // Constant operations are not special-cased here: lowering
                // emits the plain instruction and the MIR pass pipeline folds
                // it uniformly, with checked-arithmetic semantics intact.
                let int_info =
                    self.integer_info_for_expr(expr).or_else(|| self.integer_info_for_expr(lhs));
                let is_signed =
                    int_info.map_or_else(|| self.is_expr_signed(lhs), |info| info.signed);
                let unsupported_udvt_operator = self.gcx.unsupported_udvt_operator(expr.id);

                // `&&`/`||` must short-circuit: the right operand may have
                // side effects (external calls, reverts, ...).
                if matches!(op.kind, hir::BinOpKind::And | hir::BinOpKind::Or) {
                    return self.lower_short_circuit(
                        builder,
                        lhs,
                        rhs,
                        op.kind == hir::BinOpKind::And,
                    );
                }

                // Shift operators take a plain integer count on the right, so it
                // must not be treated as a `bytesN` sibling of the left operand.
                let is_shift = matches!(op.kind, hir::BinOpKind::Shl | hir::BinOpKind::Shr);
                let (lhs_val, rhs_val) = if is_shift {
                    (self.lower_expr(builder, lhs), self.lower_expr(builder, rhs))
                } else {
                    (
                        self.lower_fixed_bytes_operand(builder, lhs, rhs),
                        self.lower_fixed_bytes_operand(builder, rhs, lhs),
                    )
                };
                let result = self.lower_binary_op(
                    builder,
                    lhs_val,
                    *op,
                    rhs_val,
                    ArithmeticInfo {
                        integer: int_info,
                        is_signed,
                        span: expr.span,
                        unsupported_udvt_operator,
                    },
                );
                // A `bytesN`-typed result (e.g. `x >> 8`, `x & y`) stays
                // left-aligned and must be re-masked to its width: a right shift
                // moves data below the `N`-byte boundary, which has to be cleared.
                if let Some(width) = self.fixed_bytes_width_of_expr(expr) {
                    return self.clean_fixed_bytes(builder, result, width);
                }
                result
            }

            ExprKind::Unary(op, operand) => {
                use hir::UnOpKind;
                match op.kind {
                    UnOpKind::PreInc | UnOpKind::PostInc | UnOpKind::PreDec | UnOpKind::PostDec => {
                        // Increment/decrement need to read, compute, store, and return
                        let operand_val = self.lower_expr(builder, operand);
                        let one = builder.imm_u64(1);
                        let int_info = self.integer_info_for_expr(operand);
                        if self.gcx.unsupported_udvt_operator(expr.id) {
                            self.emit_unsupported_udvt_operator(operand.span);
                            return operand_val;
                        }
                        let new_val = match op.kind {
                            UnOpKind::PreInc | UnOpKind::PostInc => self
                                .lower_checked_or_wrapping_add(
                                    builder,
                                    operand_val,
                                    one,
                                    int_info,
                                    operand.span,
                                ),
                            UnOpKind::PreDec | UnOpKind::PostDec => self
                                .lower_checked_or_wrapping_sub(
                                    builder,
                                    operand_val,
                                    one,
                                    int_info,
                                    operand.span,
                                ),
                            _ => unreachable!(),
                        };
                        // Store the new value back
                        self.lower_assign(builder, operand, new_val);
                        // Return old value for post, new value for pre
                        match op.kind {
                            UnOpKind::PostInc | UnOpKind::PostDec => operand_val,
                            UnOpKind::PreInc | UnOpKind::PreDec => new_val,
                            _ => unreachable!(),
                        }
                    }
                    _ => {
                        let operand_val = self.lower_expr(builder, operand);
                        let int_info = self
                            .integer_info_for_expr(expr)
                            .or_else(|| self.integer_info_for_expr(operand));
                        if self.gcx.unsupported_udvt_operator(expr.id) {
                            self.emit_unsupported_udvt_operator(expr.span);
                            return operand_val;
                        }
                        self.lower_unary_op(builder, *op, operand_val, int_info, expr.span)
                    }
                }
            }

            ExprKind::Ternary(cond, then_expr, else_expr) => {
                self.lower_ternary(builder, expr, cond, then_expr, else_expr)
            }

            ExprKind::Call(callee, args, call_opts) => {
                self.lower_call(builder, callee, args, (*call_opts).map(|opts| opts.args))
            }

            ExprKind::Index(base, index) => {
                self.lower_index_expr(builder, expr, base, index.as_deref())
            }

            ExprKind::Member(base, member) => {
                if let Some(builtin) = self.resolved_builtin_member(expr) {
                    match builtin {
                        // Handle address member access: addr.balance
                        Builtin::AddressBalance => {
                            let addr = self.lower_expr(builder, base);
                            return builder.balance(addr);
                        }
                        // Handle function and error selector member access.
                        Builtin::FunctionSelector => {
                            if let Some(selector) = self.lower_resolved_function_selector(base) {
                                return builder.imm_u256(U256::from(selector) << 224);
                            }
                            if let ExprKind::Member(receiver, function_name) = &base.kind {
                                let selector =
                                    self.compute_member_selector(receiver, *function_name);
                                return builder.imm_u256(U256::from(selector) << 224);
                            }
                        }
                        Builtin::EventSelector => {
                            if let Some(selector) = self.lower_resolved_event_selector(base) {
                                return builder.imm_u256(selector);
                            }
                        }
                        // Handle type(T).min and type(T).max.
                        Builtin::TypeMin | Builtin::TypeMax => {
                            if let ExprKind::TypeCall(ty) = &base.kind {
                                return self.lower_type_minmax(
                                    builder,
                                    ty,
                                    builtin == Builtin::TypeMax,
                                );
                            }
                        }
                        // Handle type(T).creationCode and type(T).runtimeCode.
                        Builtin::ContractCreationCode | Builtin::ContractRuntimeCode => {
                            if let ExprKind::TypeCall(ty) = &base.kind {
                                return self.lower_type_creation_code(
                                    builder,
                                    ty,
                                    builtin == Builtin::ContractCreationCode,
                                );
                            }
                        }
                        Builtin::ArrayLength => {
                            if let Some(length) = self.lower_array_length_member(builder, base) {
                                return length;
                            }
                        }
                        Builtin::BlockCoinbase
                        | Builtin::BlockTimestamp
                        | Builtin::BlockDifficulty
                        | Builtin::BlockPrevrandao
                        | Builtin::BlockNumber
                        | Builtin::BlockGaslimit
                        | Builtin::BlockChainid
                        | Builtin::BlockBasefee
                        | Builtin::BlockBlobbasefee
                        | Builtin::MsgSender
                        | Builtin::MsgGas
                        | Builtin::MsgValue
                        | Builtin::MsgData
                        | Builtin::MsgSig
                        | Builtin::TxOrigin
                        | Builtin::TxGasPrice
                        | Builtin::AbiEncode
                        | Builtin::AbiEncodePacked
                        | Builtin::AbiEncodeWithSelector
                        | Builtin::AbiEncodeCall
                        | Builtin::AbiEncodeWithSignature
                        | Builtin::AbiDecode => return self.lower_builtin(builder, builtin),
                        _ => {}
                    }
                }

                // Handle enum variant access (e.g., Status.Active or Contract.Status.Active).
                if let Some((_enum_id, variant_index)) = self.resolved_enum_variant(expr) {
                    return builder.imm_u64(variant_index as u64);
                }

                // Handle contract/library constants (e.g. MachineLib.NO_RECOVERY_PC).
                if let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) =
                    self.resolved_member(expr)
                {
                    let var = self.gcx.hir.variable(var_id);
                    if var.is_constant()
                        && let Some(init) = var.initializer
                    {
                        return self.lower_expr(builder, init);
                    }
                }

                // A `bytes`/`string` struct field living in storage, reached
                // through a storage reference (`state.part` with
                // `S storage state`): its value is the packed storage form, so
                // materialize it into a `[length][data...]` memory copy — the
                // same representation a storage bytes state variable lowers to.
                // Reading the field slot as a word (the generic struct-field
                // path below) would hand a length word to consumers expecting
                // a memory pointer.
                if self.expr_is_storage_bytes_lvalue(expr)
                    && let Some(slot) = self.lower_lvalue_slot(builder, expr)
                {
                    return self.materialize_storage_bytes(builder, slot);
                }

                // Keep a name-based fallback for callers without sema results.
                if member.name == sym::length {
                    // Storage array (state variable or storage-reference
                    // local): dynamic length at the base slot, fixed length
                    // is a compile-time constant.
                    if let Some(length) = self.lower_array_length_member(builder, base) {
                        return length;
                    }
                    // Memory dynamic arrays and bytes fall through to the
                    // generic member fallback, which loads the length word at
                    // the base pointer.
                }

                // Check if this is a storage struct member access (e.g., storedPoint.x)
                if let Some((struct_id, field_index)) = self.resolved_struct_field(expr)
                    && let Some(slot) = self.lower_storage_struct_field_slot_by_index(
                        builder,
                        base,
                        struct_id,
                        field_index,
                    )
                {
                    return builder.sload(slot);
                }

                if let Some((base_slot, struct_id, field_index)) =
                    self.get_storage_struct_field_info(base, *member)
                {
                    let field_offset = self.get_struct_field_slot_offset(struct_id, field_index);
                    let slot = base_slot + field_offset;
                    let slot_val = builder.imm_u64(slot);
                    return builder.sload(slot_val);
                }

                // Check if this is a nested storage struct access (e.g., storedNested.point.x)
                if let Some(slot) = self.compute_nested_storage_slot(base, *member) {
                    let slot_val = builder.imm_u64(slot);
                    return builder.sload(slot_val);
                }

                // Storage struct field access where the base is itself a storage
                // location: a storage reference (`Item storage r = items[k]; r.a`)
                // or an indexed element (`items[k].a`, `arr[i].a`).
                if let Some(slot) = self.lower_storage_struct_field_slot(builder, base, *member) {
                    return builder.sload(slot);
                }

                // Regular memory struct member access
                if let Some((struct_id, field_index)) = self.resolved_struct_field(expr)
                    && self.is_memory_struct_base(base, struct_id)
                {
                    let base_val = self.lower_expr(builder, base);
                    let fields = self.gcx.hir.strukt(struct_id).fields.len() as u64;
                    let field_addr = builder.memory_object_field_addr(
                        base_val,
                        crate::mir::MemoryObjectLayout::structure(fields),
                        field_index as u64,
                    );
                    return builder.mload(field_addr);
                }

                if let Some((struct_id, field_index)) =
                    self.get_memory_struct_field_info(base, *member)
                {
                    let base_val = self.lower_expr(builder, base);
                    let fields = self.gcx.hir.strukt(struct_id).fields.len() as u64;
                    let field_addr = builder.memory_object_field_addr(
                        base_val,
                        crate::mir::MemoryObjectLayout::structure(fields),
                        field_index as u64,
                    );
                    return builder.mload(field_addr);
                }

                // Fallback: just load from base address
                let base_val = self.lower_expr(builder, base);
                builder.mload(base_val)
            }

            ExprKind::YulMember(base, member) => self.lower_yul_member(builder, base, *member),

            ExprKind::Assign(lhs, op, rhs) => {
                // Tuple destructuring to existing lvalues, `(a, b) = rhs`.
                if op.is_none()
                    && let ExprKind::Tuple(elements) = &lhs.kind
                {
                    self.lower_tuple_assign(builder, elements, rhs);
                    return builder.imm_u64(0);
                }
                let rhs_val = if op.is_none() && self.lhs_expects_memory_bytes_value(lhs) {
                    self.lower_expr_as_memory_bytes(builder, rhs)
                } else if op.is_none() && self.lhs_expects_memory_dyn_array_value(lhs) {
                    self.lower_expr_as_memory_dyn_array(builder, rhs)
                } else {
                    self.lower_expr(builder, rhs)
                };
                // Handle compound assignment (+=, -=, etc.)
                let final_val = if let Some(bin_op) = op {
                    // Read current value, apply operator, then assign
                    let lhs_val = self.lower_expr(builder, lhs);
                    let int_info = self.integer_info_for_expr(lhs);
                    let is_signed =
                        int_info.map_or_else(|| self.is_expr_signed(lhs), |info| info.signed);
                    let unsupported_udvt_operator = self.gcx.unsupported_udvt_operator(expr.id);
                    self.lower_binary_op(
                        builder,
                        lhs_val,
                        *bin_op,
                        rhs_val,
                        ArithmeticInfo {
                            integer: int_info,
                            is_signed,
                            span: lhs.span,
                            unsupported_udvt_operator,
                        },
                    )
                } else {
                    rhs_val
                };
                self.lower_assign(builder, lhs, final_val);
                final_val
            }

            ExprKind::Tuple(elements) => {
                if let Some(Some(expr)) = elements.first() {
                    return self.lower_expr(builder, expr);
                }
                builder.imm_u64(0)
            }

            ExprKind::Array(elements) => {
                let alloc_size = u64::try_from(elements.len())
                    .ok()
                    .and_then(|len| len.checked_mul(32))
                    .unwrap_or_else(|| {
                        self.gcx
                            .dcx()
                            .err("array literal is too large for codegen")
                            .span(expr.span)
                            .emit();
                        0
                    });
                let ptr = self.allocate_memory_object(
                    builder,
                    alloc_size,
                    crate::mir::MemoryObjectKind::FixedArray,
                );
                for (i, elem) in elements.iter().enumerate() {
                    let elem_val = self.lower_expr(builder, elem);
                    let offset_const = builder.imm_u64(i as u64 * 32);
                    let addr = builder.add(ptr, offset_const);
                    builder.mstore(addr, elem_val);
                }
                ptr
            }

            ExprKind::TypeCall(_ty) => builder.imm_u64(0),

            ExprKind::Payable(inner) => self.lower_expr(builder, inner),

            ExprKind::New(_ty) => builder.imm_u64(0),

            ExprKind::Delete(target) => {
                let zero = builder.imm_u256(U256::ZERO);
                if let Some(ty) = self.get_expr_type(target)
                    && let TyKind::Struct(struct_id) = ty.peel_refs().kind
                    && let Some(slot) = self.lower_lvalue_slot(builder, target)
                {
                    self.clear_storage_struct_at(builder, struct_id, slot);
                    return zero;
                }
                // Deleting a memory fixed-size array zeroes its elements in
                // place; nulling the pointer would alias scratch memory on the
                // next access. Storage targets keep the assignment path.
                if let Some(var_id) = self.ident_variable(target)
                    && !self.storage_ref_locals.contains(var_id)
                    && !self.storage_slots.contains_key(&var_id)
                {
                    let var = self.gcx.hir.variable(var_id);
                    if self.is_fixed_memory_array_type(&var.ty, var.data_location)
                        && let Some(len) = self.fixed_memory_array_len(&var.ty)
                        && let hir::TypeKind::Array(array) = &var.ty.kind
                    {
                        let ptr = self.lower_expr(builder, target);
                        for i in 0..len {
                            let value = self.zero_memory_field_value(builder, &array.element);
                            if i == 0 {
                                builder.mstore(ptr, value);
                            } else {
                                let offset = builder.imm_u64(i * 32);
                                let addr = builder.add(ptr, offset);
                                builder.mstore(addr, value);
                            }
                        }
                        return zero;
                    }
                }
                self.lower_assign(builder, target, zero);
                zero
            }

            ExprKind::Slice(base, start, end) => {
                if let Some((slice, is_bytes)) = self.calldata_bytes_source(builder, base) {
                    let base_ptr = builder.slice_ptr(slice);
                    let base_len = builder.slice_len(slice);
                    let start_val = start
                        .map(|s| self.lower_expr(builder, s))
                        .unwrap_or_else(|| builder.imm_u64(0));
                    let end_val = end.map(|e| self.lower_expr(builder, e)).unwrap_or(base_len);
                    if end_val != base_len {
                        let end_out_of_bounds = builder.gt(end_val, base_len);
                        self.emit_panic_if(
                            builder,
                            end_out_of_bounds,
                            super::checked_arith::PanicCode::ArrayOutOfBounds,
                        );
                    }
                    let backwards = builder.lt(end_val, start_val);
                    self.emit_panic_if(
                        builder,
                        backwards,
                        super::checked_arith::PanicCode::ArrayOutOfBounds,
                    );
                    let len = builder.sub(end_val, start_val);
                    let offset = if is_bytes {
                        start_val
                    } else {
                        let word = builder.imm_u64(32);
                        builder.mul(start_val, word)
                    };
                    let ptr = builder.add(base_ptr, offset);
                    return builder.make_slice(ptr, len, crate::mir::SliceLocation::Calldata);
                }
                // Solidity only permits slicing calldata arrays, so a base that
                // is not a calldata slice is unreachable in valid input.
                // Reject rather than emit raw pointer arithmetic.
                self.err_value(builder, expr.span, "codegen only supports slicing calldata arrays")
            }

            ExprKind::Type(_ty) => builder.imm_u64(0),

            ExprKind::Err(_) => builder.imm_u64(0),
        }
    }

    /// Lowers a literal to a MIR value.
    pub(super) fn lower_literal(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        lit: &hir::Lit<'_>,
    ) -> ValueId {
        match &lit.kind {
            LitKind::Bool(b) => builder.imm_bool(*b),
            LitKind::Number(n) => builder.imm_u256(*n),
            LitKind::Rational(_r) => builder.imm_u64(0),
            LitKind::Str(kind, bytes, _extra) => {
                let bytes = bytes.as_byte_str();
                match kind {
                    StrKind::Str | StrKind::Unicode => {
                        let mut padded = [0u8; 32];
                        let len = bytes.len().min(32);
                        padded[..len].copy_from_slice(&bytes[..len]);
                        builder.imm_u256(U256::from_be_bytes(padded))
                    }
                    StrKind::Hex => {
                        let mut padded = [0u8; 32];
                        let len = bytes.len().min(32);
                        padded[..len].copy_from_slice(&bytes[..len]);
                        builder.imm_u256(U256::from_be_bytes(padded))
                    }
                }
            }
            LitKind::Address(addr) => builder.imm_u256(U256::from_be_slice(addr.as_slice())),
            LitKind::Err(_) => builder.imm_u64(0),
        }
    }

    /// Lowers an identifier reference.
    fn lower_ident(&mut self, builder: &mut FunctionBuilder<'_>, res: &hir::Res) -> ValueId {
        match res {
            hir::Res::Item(item_id) => {
                if let hir::ItemId::Variable(var_id) = item_id {
                    let var = self.gcx.hir.variable(*var_id);

                    // First check if it's a function parameter (SSA value)
                    if let Some(&val) = self.locals.get(var_id) {
                        return val;
                    }

                    // Check if it's a local variable stored in memory
                    if let Some(offset) = self.get_local_memory_offset(var_id) {
                        if self.is_slice_slot_local(var_id) {
                            return self.load_slice_slot(
                                builder,
                                offset,
                                crate::mir::SliceLocation::Calldata,
                            );
                        }
                        let offset_val = self.local_memory_addr(builder, offset);
                        return builder.mload(offset_val);
                    }

                    // Check if it's a constant - inline its value
                    if var.is_constant()
                        && let Some(init) = var.initializer
                    {
                        return self.lower_expr(builder, init);
                    }

                    // Check if it's an immutable - load from appended runtime data.
                    if let Some(&offset) = self.immutable_slots.get(var_id) {
                        return self.load_immutable_value(builder, offset);
                    }

                    // Check if it's a storage variable
                    if let Some(&location) = self.storage_locations.get(var_id) {
                        let slot = location.slot;
                        // For storage structs, we need to copy to memory and return the pointer
                        if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind
                        {
                            // Calculate total flattened size (handles nested structs)
                            let total_words = self
                                .calculate_memory_words_for_ty(self.gcx.type_of_hir_ty(&var.ty));
                            let struct_size = total_words * 32;
                            let struct_ptr = self.allocate_memory_object(
                                builder,
                                struct_size,
                                crate::mir::MemoryObjectKind::Struct,
                            );

                            // Recursively copy all fields (handles nested structs)
                            self.copy_storage_to_memory(builder, *struct_id, slot, struct_ptr, 0);
                            return struct_ptr;
                        }

                        // For scalar storage bytes/string, normalize the packed
                        // short-storage slot to the memory layout expected by
                        // the ABI encoder. `.length` and indexing use dedicated
                        // storage-slot paths and do not come through here.
                        let slot_val = builder.imm_u64(slot);
                        if matches!(
                            var.ty.kind,
                            hir::TypeKind::Elementary(
                                hir::ElementaryType::String | hir::ElementaryType::Bytes
                            )
                        ) {
                            return self.materialize_storage_bytes(builder, slot_val);
                        }

                        // For scalar storage variables, just load the value
                        return self.load_storage_location_at_slot(builder, location, slot_val);
                    }
                }
                builder.imm_u64(0)
            }
            hir::Res::Builtin(builtin) => self.lower_builtin(builder, *builtin),
            hir::Res::Namespace(_) => builder.imm_u64(0),
            hir::Res::Err(_) => builder.imm_u64(0),
        }
    }

    /// Lowers a builtin reference.
    fn lower_builtin(&mut self, builder: &mut FunctionBuilder<'_>, builtin: Builtin) -> ValueId {
        match builtin {
            Builtin::MsgSender => builder.caller(),
            Builtin::MsgValue => builder.callvalue(),
            Builtin::MsgData => {
                // `msg.data` is the whole calldata as a lazy calldata slice;
                // `.length`, indexing, slicing, and materialization consume it
                // through the shared calldata-slice paths.
                let zero = builder.imm_u64(0);
                let size = builder.calldatasize();
                builder.make_slice(zero, size, crate::mir::SliceLocation::Calldata)
            }
            Builtin::BlockTimestamp => {
                let inst = builder.func_mut().alloc_inst(crate::mir::Instruction::new(
                    crate::mir::InstKind::Timestamp,
                    Some(MirType::uint256()),
                ));
                let block = builder.current_block();
                builder.func_mut().block_mut(block).instructions.push(inst);
                builder.func_mut().alloc_value(crate::mir::Value::Inst(inst))
            }
            Builtin::BlockNumber => {
                let inst = builder.func_mut().alloc_inst(crate::mir::Instruction::new(
                    crate::mir::InstKind::BlockNumber,
                    Some(MirType::uint256()),
                ));
                let block = builder.current_block();
                builder.func_mut().block_mut(block).instructions.push(inst);
                builder.func_mut().alloc_value(crate::mir::Value::Inst(inst))
            }
            Builtin::TxOrigin => {
                let inst = builder.func_mut().alloc_inst(crate::mir::Instruction::new(
                    crate::mir::InstKind::Origin,
                    Some(MirType::Address),
                ));
                let block = builder.current_block();
                builder.func_mut().block_mut(block).instructions.push(inst);
                builder.func_mut().alloc_value(crate::mir::Value::Inst(inst))
            }
            Builtin::TxGasPrice => {
                let inst = builder.func_mut().alloc_inst(crate::mir::Instruction::new(
                    crate::mir::InstKind::GasPrice,
                    Some(MirType::uint256()),
                ));
                let block = builder.current_block();
                builder.func_mut().block_mut(block).instructions.push(inst);
                builder.func_mut().alloc_value(crate::mir::Value::Inst(inst))
            }
            Builtin::Gasleft => builder.gas(),
            Builtin::This => builder.address(),
            _ => builder.imm_u64(0),
        }
    }

    fn lower_yul_member(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        base: &hir::Expr<'_>,
        member: Ident,
    ) -> ValueId {
        let Some(var_id) = self.ident_variable(base) else {
            return self.err_value(
                builder,
                member.span,
                format!("unsupported Yul member `.{}`", member.name),
            );
        };
        let calldata_slice = Self::calldata_dynamic_var_kind(self.gcx.hir.variable(var_id))
            .and_then(|_| self.locals.get(&var_id).copied());

        match member.name {
            sym::slot => {
                if let Some(&slot) = self.storage_slots.get(&var_id) {
                    return builder.imm_u64(slot);
                }
                if let Some(&slot) = self.locals.get(&var_id) {
                    return slot;
                }
                if let Some(offset) = self.get_local_memory_offset(&var_id) {
                    let offset = self.local_memory_addr(builder, offset);
                    return builder.mload(offset);
                }
            }
            sym::offset => {
                if let Some(location) = self.storage_locations.get(&var_id) {
                    return builder.imm_u64(u64::from(location.offset));
                }
                if let Some(slice) = calldata_slice {
                    return builder.slice_ptr(slice);
                }
                return builder.imm_u64(0);
            }
            sym::length => {
                if let Some(slice) = calldata_slice {
                    return builder.slice_len(slice);
                }
            }
            _ => {}
        }

        self.err_value(builder, member.span, format!("unsupported Yul member `.{}`", member.name))
    }

    /// Lowers `lhs && rhs` / `lhs || rhs` with short-circuit evaluation: the
    /// right operand is only evaluated when the left operand does not already
    /// decide the result.
    fn lower_short_circuit(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        lhs: &hir::Expr<'_>,
        rhs: &hir::Expr<'_>,
        is_and: bool,
    ) -> ValueId {
        let lhs_val = self.lower_expr(builder, lhs);
        let pred_block = builder.current_block();
        let rhs_block = builder.create_block();
        let merge_block = builder.create_block();
        if is_and {
            builder.branch(lhs_val, rhs_block, merge_block);
        } else {
            builder.branch(lhs_val, merge_block, rhs_block);
        }

        builder.switch_to_block(rhs_block);
        let rhs_val = self.lower_expr(builder, rhs);
        let rhs_end = builder.current_block();
        let rhs_terminated = builder.func().block(rhs_end).is_terminated();
        if !rhs_terminated {
            builder.jump(merge_block);
        }

        builder.switch_to_block(merge_block);
        // `a && b` is false when `a` is false; `a || b` is true when `a` is
        // true (bool values are canonical 0/1).
        let decided = builder.imm_bool(!is_and);
        let mut incoming = vec![(pred_block, decided)];
        if !rhs_terminated {
            incoming.push((rhs_end, rhs_val));
        }
        builder.phi(incoming)
    }

    /// Returns the bytes of a compile-time-constant string expression: a
    /// string literal, or an identifier/member reference to a `constant`
    /// string variable whose initializer (transitively) is a literal — e.g.
    /// aave's `Errors.X` library constants.
    fn constant_string_bytes(&self, expr: &hir::Expr<'_>) -> Option<Vec<u8>> {
        let mut expr = expr;
        for _ in 0..4 {
            match &expr.kind {
                ExprKind::Lit(lit) => {
                    let LitKind::Str(_, bytes, _) = &lit.kind else { return None };
                    return Some(bytes.as_byte_str().to_vec());
                }
                ExprKind::Ident([hir::Res::Item(hir::ItemId::Variable(var_id))]) => {
                    let var = self.gcx.hir.variable(*var_id);
                    if !var.is_constant() {
                        return None;
                    }
                    expr = var.initializer?;
                }
                ExprKind::Member(..) => {
                    let hir::Res::Item(hir::ItemId::Variable(var_id)) =
                        self.resolved_member(expr)?
                    else {
                        return None;
                    };
                    let var = self.gcx.hir.variable(var_id);
                    if !var.is_constant() {
                        return None;
                    }
                    expr = var.initializer?;
                }
                _ => return None,
            }
        }
        None
    }

    pub(super) fn emit_revert_error_string_from_expr(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> bool {
        // A constant message (a literal, or a `constant` string like aave's
        // `Errors.X`). Short messages revert through the module's shared
        // helper: one call with the length and the left-aligned data word,
        // instead of materializing and ABI-encoding the string at every site
        // — the revert data is identical to the generic path below. Longer
        // (or empty) constants materialize their resolved bytes directly:
        // `lower_expr` on a constant reference would yield a truncated
        // immediate word, not a memory string.
        if let Some(bytes) = self.constant_string_bytes(expr) {
            if (1..=32).contains(&bytes.len()) {
                let helper = self.ensure_revert_error_helper();
                let mut padded = [0u8; 32];
                padded[..bytes.len()].copy_from_slice(&bytes);
                let len = builder.imm_u64(bytes.len() as u64);
                let data = builder.imm_u256(U256::from_be_bytes(padded));
                builder.internal_call_void(helper, vec![len, data], 0);
                // The helper reverts; this terminator is unreachable.
                builder.invalid();
                return true;
            }
            let ptr = self.lower_string_bytes_to_memory(builder, &bytes);
            self.emit_revert_error_string_from_memory(builder, ptr);
            return true;
        }

        let ptr = if let ExprKind::Lit(lit) = &expr.kind {
            let Some(ptr) = self.lower_string_literal_to_memory(builder, lit) else {
                return false;
            };
            ptr
        } else {
            let Some(ty) = self.get_expr_type(expr) else { return false };
            if !matches!(ty.peel_refs().kind, TyKind::Elementary(ElementaryType::String)) {
                return false;
            }
            self.lower_expr(builder, expr)
        };

        self.emit_revert_error_string_from_memory(builder, ptr);
        true
    }

    fn emit_revert_error_string_from_memory(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ptr: ValueId,
    ) {
        let selector = U256::from(0x08c3_79a0u64) << 224;
        let zero = builder.imm_u64(0);
        let selector = builder.imm_u256(selector);
        builder.mstore(zero, selector);

        let selector_size = builder.imm_u64(4);
        let head_offset = builder.imm_u64(32);
        builder.mstore(selector_size, head_offset);

        let len = builder.memory_object_len(ptr, MemoryObjectKind::Bytes);
        let len_offset = builder.imm_u64(36);
        builder.mstore(len_offset, len);

        let thirty_one = builder.imm_u64(31);
        let padded = builder.add(len, thirty_one);
        let mask = builder.imm_u256(U256::MAX - U256::from(31));
        let padded = builder.and(padded, mask);

        let data_offset = builder.imm_u64(68);
        let no_data = builder.iszero(padded);
        let has_data = builder.iszero(no_data);
        let zero_final_word = builder.create_block();
        let copy_data = builder.create_block();
        builder.branch(has_data, zero_final_word, copy_data);

        builder.switch_to_block(zero_final_word);
        let word = builder.imm_u64(32);
        let final_word_offset = builder.sub(padded, word);
        let final_word = builder.add(data_offset, final_word_offset);
        builder.mstore(final_word, zero);
        builder.jump(copy_data);

        builder.switch_to_block(copy_data);
        let src = builder.add(ptr, head_offset);
        self.mcopy(builder, data_offset, src, len, None);
        let size = builder.add(data_offset, padded);
        builder.revert(zero, size);
    }

    fn lower_array_length_member(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        base: &hir::Expr<'_>,
    ) -> Option<ValueId> {
        // Storage array (state variable or storage-reference local): dynamic
        // length at the base slot, fixed length is a compile-time constant.
        if let Some((slot_val, fixed_len, _)) = self.storage_array_slot_of_base(builder, base) {
            return Some(match fixed_len {
                Some(len) => builder.imm_u64(len),
                None => builder.sload(slot_val),
            });
        }

        // Calldata dynamic array/bytes (and `msg.data`) carry their length in
        // the slice.
        if let Some((slice, _)) = self.calldata_bytes_source(builder, base) {
            return Some(builder.slice_len(slice));
        }

        // Fixed-size arrays have a compile-time length.
        if let Some(len) = self.fixed_array_len_of_expr(base) {
            return Some(builder.imm_u64(len));
        }

        // Memory dynamic arrays and bytes fall through to the generic member
        // fallback, which loads the length word at the base pointer.
        None
    }

    fn lower_resolved_function_selector(&self, expr: &hir::Expr<'_>) -> Option<u32> {
        let hir::Res::Item(item_id) = self.resolved_member(expr)? else {
            return None;
        };
        match item_id {
            hir::ItemId::Function(id) => Some(u32::from_be_bytes(self.gcx.function_selector(id).0)),
            hir::ItemId::Error(id) => Some(u32::from_be_bytes(self.gcx.function_selector(id).0)),
            _ => None,
        }
    }

    fn lower_resolved_event_selector(&self, expr: &hir::Expr<'_>) -> Option<U256> {
        let hir::Res::Item(hir::ItemId::Event(event_id)) = self.resolved_member(expr)? else {
            return None;
        };
        Some(U256::from_be_bytes(self.gcx.event_selector(event_id).0))
    }

    /// Lowers type(T).min or type(T).max to a constant value.
    fn lower_type_minmax(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: &hir::Type<'_>,
        is_max: bool,
    ) -> ValueId {
        match &ty.kind {
            hir::TypeKind::Elementary(elem) => match elem {
                ElementaryType::UInt(size) => {
                    let bits = size.bits() as u32;
                    if is_max {
                        // max = 2^bits - 1
                        if bits == 256 {
                            builder.imm_u256(U256::MAX)
                        } else {
                            let max_val = (U256::from(1) << bits) - U256::from(1);
                            builder.imm_u256(max_val)
                        }
                    } else {
                        // min = 0 for unsigned
                        builder.imm_u256(U256::ZERO)
                    }
                }
                ElementaryType::Int(size) => {
                    let bits = size.bits() as u32;
                    if is_max {
                        // max = 2^(bits-1) - 1
                        let max_val = (U256::from(1) << (bits - 1)) - U256::from(1);
                        builder.imm_u256(max_val)
                    } else {
                        // min = -2^(bits-1), stored as two's complement
                        // For signed int, min is represented as 2^256 - 2^(bits-1) in unsigned
                        // But for intN where N < 256, the value 0x80..0 with N bits sign-extended
                        // to 256 bits is: NOT((2^(bits-1) - 1))
                        if bits == 256 {
                            // int256 min = -2^255 = 0x8000...0000 (2^255)
                            builder.imm_u256(U256::from(1) << 255)
                        } else {
                            // For smaller types, min as two's complement 256-bit:
                            // -2^(bits-1) = 2^256 - 2^(bits-1)
                            let min_val = U256::MAX - (U256::from(1) << (bits - 1)) + U256::from(1);
                            builder.imm_u256(min_val)
                        }
                    }
                }
                _ => builder.imm_u64(0),
            },
            _ => builder.imm_u64(0),
        }
    }

    /// Lowers `type(Contract).creationCode` or `type(Contract).runtimeCode`.
    /// Returns a `bytes memory` pointer with layout: [length (32 bytes)][bytecode data...]
    fn lower_type_creation_code(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: &hir::Type<'_>,
        is_creation_code: bool,
    ) -> ValueId {
        // Extract ContractId from the type
        let hir::TypeKind::Custom(hir::ItemId::Contract(contract_id)) = ty.kind else {
            return self.err_value(
                builder,
                ty.span,
                "codegen expected a contract type for `creationCode`/`runtimeCode`",
            );
        };

        // Look up pre-compiled bytecode
        // For creationCode we use the deployment bytecode (initcode)
        if !is_creation_code {
            return self.err_value(
                builder,
                ty.span,
                "codegen does not support `type(C).runtimeCode` yet",
            );
        }

        let bytecode = match self.contract_bytecodes.get(&contract_id) {
            Some(bc) => bc.clone(),
            None => {
                return self.err_value(
                    builder,
                    ty.span,
                    "codegen is missing creation bytecode for `type(C).creationCode`",
                );
            }
        };

        let bytecode_len = bytecode.len();

        // Allocate memory for bytes: 32 bytes length + bytecode
        // Layout: [length (32 bytes)][data...]
        //
        let aligned_data_len = bytecode_len.div_ceil(32) * 32;
        let total_size = 32 + aligned_data_len;
        let ptr = self.allocate_memory_object(
            builder,
            total_size as u64,
            crate::mir::MemoryObjectKind::Bytes,
        );

        // Store length at ptr
        let len_val = builder.imm_u64(bytecode_len as u64);
        builder.set_memory_object_len(ptr, len_val, MemoryObjectKind::Bytes);

        // Copy bytecode to ptr+32 using MSTORE loop
        let data_start = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);

        let mut offset = 0u64;
        for chunk in bytecode.chunks(32) {
            let mut padded = [0u8; 32];
            padded[..chunk.len()].copy_from_slice(chunk);
            let value = U256::from_be_bytes(padded);
            let val_id = builder.imm_u256(value);
            let offset_id = builder.imm_u64(offset);
            let dest = builder.add(data_start, offset_id);
            builder.mstore(dest, val_id);
            offset += 32;
        }

        // Return ptr (the bytes memory value)
        ptr
    }

    /// Lowers a ternary conditional expression with proper branching.
    /// This handles both scalar and tuple returns correctly by using control flow
    /// instead of select, and staging multi-value results in the return buffer.
    fn lower_ternary(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
        cond: &hir::Expr<'_>,
        then_expr: &hir::Expr<'_>,
        else_expr: &hir::Expr<'_>,
    ) -> ValueId {
        // Determine if this is a tuple-typed ternary by checking if either branch is a tuple.
        let tuple_arity = match (&then_expr.kind, &else_expr.kind) {
            (ExprKind::Tuple(elements), _) | (_, ExprKind::Tuple(elements))
                if elements.len() > 1 =>
            {
                Some(elements.len())
            }
            _ => None,
        };

        if let Some(tuple_arity) = tuple_arity {
            // For tuple ternaries, use branching to stage values in the
            // ephemeral multi-return buffer.
            let cond_val = self.lower_expr(builder, cond);

            let then_block = builder.create_block();
            let else_block = builder.create_block();
            let merge_block = builder.create_block();

            builder.branch(cond_val, then_block, else_block);

            // Then block: evaluate then_expr and write tuple elements to memory
            builder.switch_to_block(then_block);
            self.lower_tuple_to_multi_return_buffer(builder, then_expr, tuple_arity);
            builder.jump(merge_block);

            // Else block: evaluate else_expr and write tuple elements to memory
            builder.switch_to_block(else_block);
            self.lower_tuple_to_multi_return_buffer(builder, else_expr, tuple_arity);
            builder.jump(merge_block);

            // Merge block: load the first value from the selected buffer.
            builder.switch_to_block(merge_block);
            let base = self.multi_return_buffer_base(builder);
            self.load_multi_return_value(builder, base, 0)
        } else {
            // For non-tuple ternaries, still use branching for correct semantics
            // (only one branch should be evaluated for side effects)
            let result_ty = self.get_expr_type(expr);
            // A calldata bytes/string/array ternary produces a logical slice:
            // its pointer and length round-trip through both scratch words and
            // re-form a slice at the merge, keeping the value lazy.
            let slice_location = result_ty.and_then(|ty| match ty.kind {
                TyKind::Ref(inner, solar_ast::DataLocation::Calldata) => match inner.kind {
                    TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
                    | TyKind::DynArray(_)
                    | TyKind::Slice(_) => Some(crate::mir::SliceLocation::Calldata),
                    _ => None,
                },
                _ => None,
            });
            let cond_val = self.lower_expr(builder, cond);

            let then_block = builder.create_block();
            let else_block = builder.create_block();
            let merge_block = builder.create_block();

            builder.branch(cond_val, then_block, else_block);

            for (block, arm) in [(then_block, then_expr), (else_block, else_expr)] {
                builder.switch_to_block(block);
                if slice_location.is_some() {
                    let value = self.lower_expr(builder, arm);
                    let ptr = builder.slice_ptr(value);
                    let len = builder.slice_len(value);
                    let ptr_slot = builder.imm_u64(0);
                    builder.mstore(ptr_slot, ptr);
                    // The second scratch word doubles as the ephemeral
                    // multi-return buffer pointer, which is only live between
                    // a multi-return call and its immediately-emitted reads,
                    // never across an arm of a user expression.
                    let len_slot = builder.imm_u64(32);
                    builder.mstore(len_slot, len);
                } else {
                    let value = self.lower_ternary_arm_value(builder, arm, result_ty);
                    let slot = builder.imm_u64(0);
                    builder.mstore(slot, value);
                }
                builder.jump(merge_block);
            }

            // Merge block: load the selected result from scratch memory.
            builder.switch_to_block(merge_block);
            if let Some(location) = slice_location {
                let ptr_slot = builder.imm_u64(0);
                let ptr = builder.mload(ptr_slot);
                let len_slot = builder.imm_u64(32);
                let len = builder.mload(len_slot);
                builder.make_slice(ptr, len, location)
            } else {
                let slot = builder.imm_u64(0);
                builder.mload(slot)
            }
        }
    }

    /// Lowers one arm of a word-merged ternary. A memory-located dynamic
    /// result adopts calldata arms by materializing them: their logical slice
    /// value has no single-word form to round-trip through scratch.
    fn lower_ternary_arm_value(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        arm: &hir::Expr<'_>,
        result_ty: Option<Ty<'gcx>>,
    ) -> ValueId {
        if let Some(ty) = result_ty
            && !matches!(
                ty.kind,
                TyKind::Ref(
                    _,
                    solar_ast::DataLocation::Calldata | solar_ast::DataLocation::Storage
                )
            )
        {
            match ty.peel_refs().kind {
                TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                    return self.lower_expr_as_memory_bytes(builder, arm);
                }
                TyKind::DynArray(_) => {
                    return self.lower_expr_as_memory_dyn_array(builder, arm);
                }
                _ => {}
            }
        }
        self.lower_expr(builder, arm)
    }

    /// Lowers a tuple expression by evaluating every element before staging
    /// the values in the ephemeral multi-return buffer.
    fn lower_tuple_to_multi_return_buffer(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
        arity: usize,
    ) {
        let values = if let ExprKind::Tuple(elements) = &expr.kind {
            elements
                .iter()
                .filter_map(|elem| elem.map(|elem| self.lower_expr(builder, elem)))
                .collect::<Vec<_>>()
        } else {
            let first = self.lower_expr(builder, expr);
            let base = self.multi_return_buffer_base(builder);
            let mut values = Vec::with_capacity(arity);
            values.push(first);
            for i in 1..arity {
                values.push(self.load_multi_return_value(builder, base, i));
            }
            values
        };
        self.stage_multi_return_values(builder, &values);
    }

    /// Lowers a binary-operator operand, left-aligning a bare numeric literal
    /// when its sibling is `bytesN`. A literal like `0x11223344` in
    /// `x == 0x11223344` is typed from its sibling, so it must use the same
    /// left-aligned word representation as the `bytesN` value it is compared to.
    fn lower_fixed_bytes_operand(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        operand: &hir::Expr<'_>,
        sibling: &hir::Expr<'_>,
    ) -> ValueId {
        if let ExprKind::Lit(lit) = &operand.kind
            && let LitKind::Number(n) = &lit.kind
            && self.fixed_bytes_width_of_expr(operand).is_none()
            && let Some(width) = self.fixed_bytes_width_of_expr(sibling)
            && width < 32
        {
            return builder.imm_u256(*n << (usize::from(32 - width) * 8));
        }
        self.lower_expr(builder, operand)
    }

    /// Lowers an assignment.
    pub(super) fn lower_assign(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        lhs: &hir::Expr<'_>,
        rhs: ValueId,
    ) {
        match &lhs.kind {
            ExprKind::Ident(res_slice) => {
                if let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first() {
                    let var = self.gcx.hir.variable(*var_id);

                    // Check if it's a local variable stored in memory
                    if let Some(offset) = self.get_local_memory_offset(var_id) {
                        if self.is_slice_slot_local(var_id) {
                            self.store_slice_slot(builder, offset, rhs);
                            return;
                        }
                        let offset_val = self.local_memory_addr(builder, offset);
                        builder.mstore(offset_val, rhs);
                    } else if self.locals.contains_key(var_id) {
                        // Function parameter - update SSA mapping (shouldn't happen normally)
                        self.locals.insert(*var_id, rhs);
                    } else if let Some(&offset) = self.immutable_slots.get(var_id) {
                        self.store_immutable_value(builder, offset, rhs);
                    } else if let Some(&location) = self.storage_locations.get(var_id) {
                        let base_slot = location.slot;
                        // Check if this is a struct assignment (memory struct -> storage struct)
                        if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind
                        {
                            // Recursively copy all fields (handles nested structs)
                            self.copy_memory_to_storage(builder, *struct_id, base_slot, rhs, 0);
                        } else if matches!(
                            var.ty.kind,
                            hir::TypeKind::Elementary(
                                hir::ElementaryType::String | hir::ElementaryType::Bytes
                            )
                        ) {
                            // `string`/`bytes` state variable: `rhs` is a memory
                            // `[length][data...]` pointer; encode it into the
                            // short/long storage form instead of storing the
                            // pointer word.
                            let slot_val = builder.imm_u64(base_slot);
                            self.copy_memory_bytes_to_storage(builder, slot_val, rhs);
                        } else {
                            // Simple scalar storage assignment
                            self.store_storage_location(builder, location, rhs);
                        }
                    }
                }
            }
            ExprKind::Index(base, index) => {
                self.lower_index_assign(builder, lhs, base, index.as_deref(), rhs);
            }
            ExprKind::Member(base, member) => {
                // Check if this is a storage struct member assignment (e.g., storedPoint.x = value)
                if let Some((struct_id, field_index)) = self.resolved_struct_field(lhs)
                    && let Some(slot) = self.lower_storage_struct_field_slot_by_index(
                        builder,
                        base,
                        struct_id,
                        field_index,
                    )
                {
                    builder.sstore(slot, rhs);
                    return;
                }

                if let Some((base_slot, struct_id, field_index)) =
                    self.get_storage_struct_field_info(base, *member)
                {
                    let field_offset = self.get_struct_field_slot_offset(struct_id, field_index);
                    let slot = base_slot + field_offset;
                    let slot_val = builder.imm_u64(slot);
                    builder.sstore(slot_val, rhs);
                    return;
                }

                // Check if this is a nested storage struct assignment (e.g., storedNested.point.x =
                // value)
                if let Some(slot) = self.compute_nested_storage_slot(base, *member) {
                    let slot_val = builder.imm_u64(slot);
                    builder.sstore(slot_val, rhs);
                    return;
                }

                // Storage struct field assignment where the base is itself a
                // storage location: a storage reference (`Item storage r =
                // items[k]; r.a = v`) or an indexed element (`items[k].a = v`).
                if let Some(slot) = self.lower_storage_struct_field_slot(builder, base, *member) {
                    builder.sstore(slot, rhs);
                    return;
                }

                // Regular memory struct member assignment
                if let Some((struct_id, field_index)) = self.resolved_struct_field(lhs)
                    && self.is_memory_struct_base(base, struct_id)
                {
                    let base_val = self.lower_expr(builder, base);
                    let fields = self.gcx.hir.strukt(struct_id).fields.len() as u64;
                    let field_addr = builder.memory_object_field_addr(
                        base_val,
                        crate::mir::MemoryObjectLayout::structure(fields),
                        field_index as u64,
                    );
                    builder.mstore(field_addr, rhs);
                    return;
                }

                if let Some((struct_id, field_index)) =
                    self.get_memory_struct_field_info(base, *member)
                {
                    let base_val = self.lower_expr(builder, base);
                    let fields = self.gcx.hir.strukt(struct_id).fields.len() as u64;
                    let field_addr = builder.memory_object_field_addr(
                        base_val,
                        crate::mir::MemoryObjectLayout::structure(fields),
                        field_index as u64,
                    );
                    builder.mstore(field_addr, rhs);
                    return;
                }

                // Fallback: store at base address
                // This should only be reached for memory structs, not storage
                let base_val = self.lower_expr(builder, base);
                builder.mstore(base_val, rhs);
            }
            ExprKind::YulMember(base, member) => {
                // `r.slot := x` sets the storage pointer's slot value. The pointer
                // is modeled as an SSA slot in `locals`, marked as a storage ref so
                // later `r.field` access resolves to `sload`/`sstore(slot + off)`.
                if member.name == sym::slot
                    && let ExprKind::Ident(res_slice) = &base.kind
                    && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
                {
                    self.locals.insert(*var_id, rhs);
                    self.storage_ref_locals.insert(*var_id);
                    return;
                }
                self.gcx
                    .dcx()
                    .err(format!("unsupported Yul assignment target `.{}`", member.name))
                    .span(member.span)
                    .emit();
            }
            _ => {}
        }
    }

    pub(super) fn lower_type_conversion(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: &hir::Type<'_>,
        source: &hir::Expr<'_>,
        value: ValueId,
    ) -> ValueId {
        match &ty.kind {
            hir::TypeKind::Elementary(elem) => {
                self.lower_elementary_type_conversion(builder, elem, source, value)
            }
            hir::TypeKind::Custom(hir::ItemId::Enum(_)) => self.mask_to_bits(builder, value, 8),
            _ => value,
        }
    }

    fn lower_elementary_type_conversion(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        elem: &ElementaryType,
        source: &hir::Expr<'_>,
        value: ValueId,
    ) -> ValueId {
        // A fixed-bytes source is left-aligned (data in the high bytes). When the
        // target is a numeric type, those bytes are reinterpreted as a
        // right-aligned integer (`uint32(bytes4)`), so shift the data down first.
        let value = match self.fixed_bytes_width_of_expr(source) {
            Some(width)
                if matches!(
                    elem,
                    ElementaryType::UInt(_) | ElementaryType::Int(_) | ElementaryType::Address(_)
                ) =>
            {
                let shift_bits = u64::from(32 - width) * 8;
                if shift_bits == 0 {
                    value
                } else {
                    let shift = builder.imm_u64(shift_bits);
                    builder.shr(shift, value)
                }
            }
            _ => value,
        };
        match elem {
            ElementaryType::Bool => {
                let is_zero = builder.iszero(value);
                builder.iszero(is_zero)
            }
            ElementaryType::Address(_) => self.mask_to_bits(builder, value, 160),
            ElementaryType::UInt(size) => {
                let bits = size.bits() as u32;
                self.mask_to_bits(builder, value, bits)
            }
            ElementaryType::Int(size) => {
                let bits = size.bits() as u32;
                self.sign_extend_to_bits(builder, value, bits)
            }
            ElementaryType::FixedBytes(size) => {
                let bytes = size.bytes();
                if self.expr_is_fixed_bytes(source) {
                    self.clean_fixed_bytes(builder, value, bytes)
                } else {
                    self.shift_numeric_to_fixed_bytes(builder, value, bytes)
                }
            }
            ElementaryType::String
            | ElementaryType::Bytes
            | ElementaryType::Fixed(_, _)
            | ElementaryType::UFixed(_, _) => value,
        }
    }

    fn shift_numeric_to_fixed_bytes(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        value: ValueId,
        bytes: u8,
    ) -> ValueId {
        let shift_bits = u64::from(32 - bytes) * 8;
        let shifted = if shift_bits == 0 {
            value
        } else {
            let shift = builder.imm_u64(shift_bits);
            builder.shl(shift, value)
        };
        self.clean_fixed_bytes(builder, shifted, bytes)
    }

    pub(super) fn clean_fixed_bytes(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        value: ValueId,
        bytes: u8,
    ) -> ValueId {
        if bytes >= 32 {
            return value;
        }
        let low_bits = usize::from(32 - bytes) * 8;
        let mask = U256::MAX << low_bits;
        let mask = builder.imm_u256(mask);
        builder.and(value, mask)
    }

    pub(super) fn mask_to_bits(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        value: ValueId,
        bits: u32,
    ) -> ValueId {
        if bits == 0 || bits >= 256 {
            return value;
        }

        let mask = (U256::from(1) << bits) - U256::from(1);
        let mask = builder.imm_u256(mask);
        builder.and(value, mask)
    }

    pub(super) fn sign_extend_to_bits(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        value: ValueId,
        bits: u32,
    ) -> ValueId {
        if bits == 0 || bits >= 256 {
            return value;
        }

        let shift = builder.imm_u64(u64::from(256 - bits));
        let shifted = builder.shl(shift, value);
        builder.sar(shift, shifted)
    }

    /// Checks if an expression is a mapping state variable and returns its var_id and storage slot.
    fn get_mapping_base_slot(&self, expr: &hir::Expr<'_>) -> Option<(hir::VariableId, u64)> {
        if let ExprKind::Ident(res_slice) = &expr.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            let var = self.gcx.hir.variable(*var_id);
            // Check if this variable has mapping type
            if matches!(var.ty.kind, hir::TypeKind::Mapping(_)) {
                // Look up the storage slot
                if let Some(&slot) = self.storage_slots.get(var_id) {
                    return Some((*var_id, slot));
                }
            }
        }
        None
    }

    /// Checks if an expression is a dynamic array state variable and returns its var_id and
    /// storage slot.
    pub(super) fn get_dyn_array_base_slot(
        &self,
        expr: &hir::Expr<'_>,
    ) -> Option<(hir::VariableId, u64)> {
        if let ExprKind::Ident(res_slice) = &expr.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            let var = self.gcx.hir.variable(*var_id);
            // Check if this variable has dynamic array type (Array with no size)
            if let hir::TypeKind::Array(arr) = &var.ty.kind
                && arr.size.is_none()
                && let Some(&slot) = self.storage_slots.get(var_id)
            {
                return Some((*var_id, slot));
            }
        }
        None
    }

    /// Resolves an indexing base that is an array living in storage: an array state variable
    /// or an array storage-reference local. Returns the base slot as a runtime value and the
    /// constant length for fixed-size arrays (`None` for dynamic arrays, whose length is
    /// stored at the base slot). Fixed-size elements occupy one slot each starting at the
    /// base slot (this codebase does not pack storage).
    pub(super) fn storage_array_slot_of_base(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> Option<(ValueId, Option<u64>, u64)> {
        let ExprKind::Ident(res_slice) = &expr.kind else { return None };
        let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first() else {
            return None;
        };
        let var = self.gcx.hir.variable(*var_id);
        let hir::TypeKind::Array(arr) = &var.ty.kind else { return None };
        let fixed_len = if arr.size.is_some() {
            let solar_sema::ty::TyKind::Array(_, len) =
                self.gcx.type_of_hir_ty(&var.ty).peel_refs().kind
            else {
                return None;
            };
            // Larger lengths already produced a layout diagnostic.
            Some(u64::try_from(len).ok()?)
        } else {
            None
        };
        let elem_slots = self.calculate_storage_slots_for_ty(
            self.gcx.type_of_hir_ty(&arr.element),
            arr.element.span,
        );
        if let Some(&slot) = self.storage_slots.get(var_id) {
            return Some((builder.imm_u64(slot), fixed_len, elem_slots));
        }
        if self.storage_ref_locals.contains(*var_id) {
            let slot_val = self.locals.get(var_id).copied()?;
            return Some((slot_val, fixed_len, elem_slots));
        }
        None
    }

    /// Emits the bounds check for a storage array access and returns the element slot.
    /// Dynamic arrays: length at `slot`, elements at `keccak256(slot) + index * elem_slots`.
    /// Fixed-size arrays: constant length, elements at `slot + index * elem_slots`.
    pub(super) fn lower_storage_array_element_slot(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        slot_val: ValueId,
        fixed_len: Option<u64>,
        index_val: ValueId,
        elem_slots: u64,
    ) -> ValueId {
        match fixed_len {
            Some(len) => {
                let len_val = builder.imm_u64(len);
                self.emit_index_bounds_check(builder, index_val, len_val);
                let offset = Self::scale_index_by_slots(builder, index_val, elem_slots);
                builder.add(slot_val, offset)
            }
            None => {
                let len = builder.sload(slot_val);
                self.emit_index_bounds_check(builder, index_val, len);
                let mem_0 = builder.imm_u64(0);
                builder.mstore(mem_0, slot_val);
                let size_32 = builder.imm_u64(32);
                let data_slot = builder.keccak256(mem_0, size_32);
                let offset = Self::scale_index_by_slots(builder, index_val, elem_slots);
                builder.add(data_slot, offset)
            }
        }
    }

    /// Scales an array index by its element's slot count; single-slot elements
    /// are addressed by the index directly.
    fn scale_index_by_slots(
        builder: &mut FunctionBuilder<'_>,
        index_val: ValueId,
        elem_slots: u64,
    ) -> ValueId {
        if elem_slots <= 1 {
            return index_val;
        }
        let elem_slots = builder.imm_u64(elem_slots);
        builder.mul(index_val, elem_slots)
    }

    /// Whether an expression is `msg.data`.
    pub(super) fn expr_is_msg_data(&self, expr: &hir::Expr<'_>) -> bool {
        matches!(self.resolved_builtin_member(expr), Some(Builtin::MsgData))
    }

    /// Resolves a calldata bytes/array base to its logical slice: an
    /// `argN`-bound calldata dynamic parameter, or `msg.data` (bytes).
    /// Returns the slice and whether it is bytes/string.
    pub(super) fn calldata_bytes_source(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        base: &hir::Expr<'_>,
    ) -> Option<(ValueId, bool)> {
        if let Some(found) = self.calldata_dyn_slice(builder, base) {
            return Some(found);
        }
        if self.expr_is_msg_data(base) {
            let slice = self.lower_expr(builder, base);
            return Some((slice, true));
        }
        // Any other calldata dynamic bytes/array expression (for example a
        // chained slice `x[1:][2:]`) whose lowering is itself a calldata
        // slice value.
        let ty = self.get_expr_type(base)?;
        if !matches!(ty.kind, TyKind::Ref(_, solar_ast::DataLocation::Calldata) | TyKind::Slice(_))
        {
            return None;
        }
        // `expr_is_calldata_dynamic_bytes` looks through a slice type to its
        // element, so it distinguishes a byte-strided bytes slice from a
        // word-strided array slice.
        let is_bytes = self.expr_is_calldata_dynamic_bytes(base);
        let value = self.lower_expr(builder, base);
        Self::value_is_calldata_slice(builder, value).then_some((value, is_bytes))
    }

    /// Checks if an expression is a dynamically-sized calldata parameter (dynamic array or
    /// bytes/string) and returns its MIR slice and whether it is bytes/string.
    ///
    /// Fixed-size calldata array parameters are not ABI heads: they are decoded to memory in
    /// the function prologue and take the regular memory path.
    pub(super) fn calldata_dyn_slice(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> Option<(ValueId, bool)> {
        let ExprKind::Ident(res_slice) = &expr.kind else { return None };
        let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first() else {
            return None;
        };
        let var = self.gcx.hir.variable(*var_id);
        if var.data_location != Some(solar_ast::DataLocation::Calldata) {
            return None;
        }
        let is_bytes = Self::calldata_dynamic_var_kind(var)?;
        if self.is_slice_slot_local(var_id) {
            let offset = self.get_local_memory_offset(var_id)?;
            let slice = self.load_slice_slot(builder, offset, crate::mir::SliceLocation::Calldata);
            return Some((slice, is_bytes));
        }
        let slice = self.locals.get(var_id).copied()?;
        Some((slice, is_bytes))
    }

    pub(super) fn calldata_dynamic_var_kind(var: &hir::Variable<'_>) -> Option<bool> {
        if var.data_location != Some(solar_ast::DataLocation::Calldata) {
            return None;
        }
        match &var.ty.kind {
            hir::TypeKind::Array(arr) if arr.size.is_none() => Some(false),
            hir::TypeKind::Elementary(hir::ElementaryType::Bytes | hir::ElementaryType::String) => {
                Some(true)
            }
            _ => None,
        }
    }

    /// Returns the constant length of a fixed-size array expression, if its type is known.
    pub(super) fn fixed_array_len_of_expr(&self, expr: &hir::Expr<'_>) -> Option<u64> {
        // Identifier: use the variable's declared type directly; `get_expr_type` may not
        // resolve every local.
        if let ExprKind::Ident(res_slice) = &expr.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            let var = self.gcx.hir.variable(*var_id);
            if let hir::TypeKind::Array(arr) = &var.ty.kind {
                arr.size.as_ref()?;
                if let solar_sema::ty::TyKind::Array(_, len) =
                    self.gcx.type_of_hir_ty(&var.ty).peel_refs().kind
                {
                    return u64::try_from(len).ok();
                }
            }
            return None;
        }
        if let Some(ty) = self.get_expr_type(expr)
            && let solar_sema::ty::TyKind::Array(_, len) = ty.peel_refs().kind
        {
            return u64::try_from(len).ok();
        }
        None
    }

    /// Lowers a struct constructor call (e.g., Point(10, 20)).
    /// Allocates memory for the struct and stores each field value.
    pub(super) fn lower_struct_constructor(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        struct_id: hir::StructId,
        args: &CallArgs<'_>,
    ) -> ValueId {
        let strukt = self.gcx.hir.strukt(struct_id);
        let num_fields = strukt.fields.len();

        // Memory struct fields are one word each. Reference-typed fields,
        // including nested structs, store pointers to separate allocations.
        let struct_size = (num_fields as u64) * 32;
        let struct_ptr =
            self.allocate_memory_object(builder, struct_size, crate::mir::MemoryObjectKind::Struct);
        let field_tys = self.gcx.struct_field_types(struct_id).to_vec();

        // Store each argument into the corresponding field
        for (i, arg) in args.exprs().enumerate() {
            if i >= num_fields {
                break;
            }
            // Memory struct fields hold memory values. Calldata reference
            // values therefore materialize recursively before storing their
            // pointer in the field slot.
            let field_val = self.lower_return_value_for_ty(builder, arg, field_tys[i]);
            let field_addr = builder.memory_object_field_addr(
                struct_ptr,
                crate::mir::MemoryObjectLayout::structure(num_fields as u64),
                i as u64,
            );
            builder.mstore(field_addr, field_val);
        }

        // Return the pointer to the struct
        struct_ptr
    }

    /// Allocates memory for a given size and returns the pointer.
    pub(super) fn allocate_memory(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        size: u64,
    ) -> ValueId {
        let size_val = builder.imm_u64(size);
        builder.alloc(size_val, crate::mir::AllocationSemantics::INTERNAL)
    }

    /// Allocates a shaped Solidity memory object with a constant byte size.
    pub(super) fn allocate_memory_object(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        size: u64,
        kind: crate::mir::MemoryObjectKind,
    ) -> ValueId {
        let layout = match kind {
            crate::mir::MemoryObjectKind::Bytes => crate::mir::MemoryObjectLayout::Bytes,
            crate::mir::MemoryObjectKind::DynamicArray => {
                crate::mir::MemoryObjectLayout::DynamicArray { element_words: 1 }
            }
            crate::mir::MemoryObjectKind::FixedArray => {
                crate::mir::MemoryObjectLayout::FixedArray { len: size / 32, element_words: 1 }
            }
            crate::mir::MemoryObjectKind::Struct => {
                crate::mir::MemoryObjectLayout::Struct { fields: size / 32 }
            }
        };
        let size = builder.imm_u64(size);
        builder.alloc_object(size, layout, crate::mir::AllocationSemantics::INTERNAL)
    }

    /// Lowers `abi.decode(data, (T...))` for elementary values from memory
    /// `bytes`: the first decoded value is returned and additional values are
    /// staged in the same ephemeral buffer used by multi-return calls. Dynamic
    /// `bytes`/`string` values are copied into fresh memory bytes.
    ///
    /// Like solc, a word that is not a clean value of `T` reverts with empty
    /// returndata instead of being silently truncated.
    pub(super) fn lower_abi_decode(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        args: &CallArgs<'_>,
    ) -> ValueId {
        let mut exprs = args.exprs();
        let (Some(data), Some(types)) = (exprs.next(), exprs.next()) else {
            return builder.imm_u64(0);
        };

        let Some(elems) = self.abi_decode_elementary_types(types, args.span) else {
            return builder.imm_u64(0);
        };

        // The decode logic below expects a memory `[length][data...]` pointer.
        // Calldata values and subslices carry `(ptr, len)` explicitly and are
        // copied only at this memory-consuming boundary.
        let ptr = if self.expr_is_calldata_dynamic_bytes(data) {
            let slice = self.lower_expr(builder, data);
            self.materialize_calldata_bytes(builder, slice)
        } else {
            self.lower_expr(builder, data)
        };
        let len = builder.memory_object_len(ptr, MemoryObjectKind::Bytes);
        let head_size = (elems.len() * 32) as u64;
        let required = builder.imm_u64(head_size);
        let is_short = builder.lt(len, required);
        self.emit_abi_decode_revert_if(builder, is_short);

        let data_start = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);
        let mut decoded_values = Vec::with_capacity(elems.len());
        for (i, elem) in elems.iter().enumerate() {
            let addr = self.offset_ptr(builder, data_start, (i * 32) as u64);
            let value = builder.mload(addr);
            let decoded = if matches!(elem, ElementaryType::Bytes | ElementaryType::String) {
                self.lower_abi_decode_dynamic_bytes(builder, data_start, len, head_size, value)
            } else {
                self.lower_abi_decode_word(builder, elem, value)
            };
            decoded_values.push(decoded);
        }
        self.stage_multi_return_tail(builder, &decoded_values);
        decoded_values.first().copied().unwrap_or_else(|| builder.imm_u64(0))
    }

    fn abi_decode_elementary_types(
        &self,
        types: &hir::Expr<'_>,
        span: Span,
    ) -> Option<Vec<ElementaryType>> {
        let ExprKind::Tuple(elems) = &types.kind else {
            self.gcx
                .dcx()
                .err("codegen only supports `abi.decode` into static values")
                .span(span)
                .emit();
            return None;
        };

        let mut out = Vec::with_capacity(elems.len());
        for elem in elems.iter().copied() {
            let Some(elem_expr) = elem else {
                self.gcx
                    .dcx()
                    .err("codegen only supports `abi.decode` into static values")
                    .span(span)
                    .emit();
                return None;
            };
            let ExprKind::Type(ty) = &elem_expr.kind else {
                self.gcx
                    .dcx()
                    .err("codegen only supports `abi.decode` into static values")
                    .span(elem_expr.span)
                    .emit();
                return None;
            };
            let hir::TypeKind::Elementary(elem) = &ty.kind else {
                self.gcx
                    .dcx()
                    .err("codegen only supports `abi.decode` into static values")
                    .span(ty.span)
                    .emit();
                return None;
            };
            out.push(*elem);
        }
        Some(out)
    }

    fn lower_abi_decode_dynamic_bytes(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        tuple_base: ValueId,
        tuple_len: ValueId,
        head_size: u64,
        head: ValueId,
    ) -> ValueId {
        let head_size = builder.imm_u64(head_size);
        let head_before_tail = builder.lt(head, head_size);
        self.emit_abi_decode_revert_if(builder, head_before_tail);

        let word = builder.imm_u64(32);
        let tail_head_end = builder.add(head, word);
        let head_overflow = builder.lt(tail_head_end, head);
        self.emit_abi_decode_revert_if(builder, head_overflow);
        let head_oob = builder.gt(tail_head_end, tuple_len);
        self.emit_abi_decode_revert_if(builder, head_oob);

        let tail_len_addr = builder.add(tuple_base, head);
        let tail_len = builder.mload(tail_len_addr);
        let thirty_one = builder.imm_u64(31);
        let rounded = builder.add(tail_len, thirty_one);
        let rounded_overflow = builder.lt(rounded, tail_len);
        self.emit_abi_decode_revert_if(builder, rounded_overflow);
        let mask = builder.not(thirty_one);
        let padded = builder.and(rounded, mask);
        let tail_end = builder.add(tail_head_end, padded);
        let tail_overflow = builder.lt(tail_end, tail_head_end);
        self.emit_abi_decode_revert_if(builder, tail_overflow);
        let tail_oob = builder.gt(tail_end, tuple_len);
        self.emit_abi_decode_revert_if(builder, tail_oob);

        let is_empty = builder.iszero(padded);
        let data_size = builder.select(is_empty, word, padded);
        let total_size = builder.add(word, data_size);
        let total_overflow = builder.lt(total_size, data_size);
        self.emit_panic_if(builder, total_overflow, PanicCode::MemoryAllocationOverflow);
        let ptr = self.allocate_memory_object_dynamic(
            builder,
            total_size,
            crate::mir::MemoryObjectKind::Bytes,
        );
        builder.set_memory_object_len(ptr, tail_len, MemoryObjectKind::Bytes);

        let data_ptr = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);
        let zero = builder.imm_u64(0);
        let last_word_offset = builder.sub(data_size, word);
        let last_word = builder.add(data_ptr, last_word_offset);
        builder.mstore(last_word, zero);

        let src = builder.add(tail_len_addr, word);
        self.mcopy(builder, data_ptr, src, tail_len, None);
        ptr
    }

    pub(super) fn emit_abi_decode_revert_if(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        cond: ValueId,
    ) {
        let revert_block = builder.create_block();
        let continue_block = builder.create_block();
        builder.branch(cond, revert_block, continue_block);
        builder.switch_to_block(revert_block);
        let zero_off = builder.imm_u64(0);
        let zero_len = builder.imm_u64(0);
        builder.revert(zero_off, zero_len);
        builder.switch_to_block(continue_block);
    }

    fn lower_abi_decode_word(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        elem: &ElementaryType,
        value: ValueId,
    ) -> ValueId {
        let cleaned = match elem {
            ElementaryType::Bool => {
                let is_zero = builder.iszero(value);
                builder.iszero(is_zero)
            }
            ElementaryType::Address(_) => self.mask_to_bits(builder, value, 160),
            ElementaryType::UInt(size) => self.mask_to_bits(builder, value, size.bits() as u32),
            ElementaryType::Int(size) => {
                self.sign_extend_to_bits(builder, value, size.bits() as u32)
            }
            ElementaryType::FixedBytes(size) => {
                self.clean_fixed_bytes(builder, value, size.bytes())
            }
            ElementaryType::String
            | ElementaryType::Bytes
            | ElementaryType::Fixed(_, _)
            | ElementaryType::UFixed(_, _) => value,
        };
        if cleaned != value {
            let is_clean = builder.eq(value, cleaned);
            let is_dirty = builder.iszero(is_clean);
            let revert_block = builder.create_block();
            let continue_block = builder.create_block();
            builder.branch(is_dirty, revert_block, continue_block);
            builder.switch_to_block(revert_block);
            let zero_off = builder.imm_u64(0);
            let zero_len = builder.imm_u64(0);
            builder.revert(zero_off, zero_len);
            builder.switch_to_block(continue_block);
        }
        cleaned
    }

    /// Checks if a member access is on a storage struct variable.
    /// Returns (base_slot, struct_id, field_index) if the base expression is a storage struct.
    fn get_storage_struct_field_info(
        &self,
        base: &hir::Expr<'_>,
        member: Ident,
    ) -> Option<(u64, hir::StructId, usize)> {
        // The base must be an identifier resolving to a variable with struct type
        if let ExprKind::Ident(res_slice) = &base.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            let var = self.gcx.hir.variable(*var_id);
            // Check if the variable has a struct type and is stored in storage
            if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind
                && let Some(&base_slot) = self.struct_storage_base_slots.get(var_id)
            {
                // Find the field index by name
                let strukt = self.gcx.hir.strukt(*struct_id);
                for (i, &field_id) in strukt.fields.iter().enumerate() {
                    let field = self.gcx.hir.variable(field_id);
                    if let Some(field_name) = field.name
                        && field_name.name == member.name
                    {
                        return Some((base_slot, *struct_id, i));
                    }
                }
            }
        }
        None
    }

    /// Checks if a member access is on a storage-reference local of struct type.
    /// Returns (var_id, struct_id, field_index) for `base.member`.
    fn get_storage_ref_struct_field_info(
        &self,
        base: &hir::Expr<'_>,
        member: Ident,
    ) -> Option<(hir::VariableId, hir::StructId, usize)> {
        if let ExprKind::Ident(res_slice) = &base.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
            && self.storage_ref_locals.contains(*var_id)
            && let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) =
                &self.gcx.hir.variable(*var_id).ty.kind
        {
            let strukt = self.gcx.hir.strukt(*struct_id);
            for (i, &field_id) in strukt.fields.iter().enumerate() {
                let field = self.gcx.hir.variable(field_id);
                if let Some(field_name) = field.name
                    && field_name.name == member.name
                {
                    return Some((*var_id, *struct_id, i));
                }
            }
        }
        None
    }

    /// Resolves the struct type of an expression, for storage struct field
    /// access. Uses the variable's declared type directly for an identifier and
    /// the inferred expression type otherwise (e.g. a mapping/array element).
    pub(super) fn struct_id_of_expr(&self, expr: &hir::Expr<'_>) -> Option<hir::StructId> {
        // Identifier: use the variable's declared type.
        if let ExprKind::Ident(res) = &expr.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(vid))) = res.first()
            && let hir::TypeKind::Custom(hir::ItemId::Struct(sid)) =
                &self.gcx.hir.variable(*vid).ty.kind
        {
            return Some(*sid);
        }
        // Indexed element (`items[k]`, `arr[i]`): the mapping value / array
        // element type, resolved from the indexed variable's declared type.
        if let ExprKind::Index(arr, _) = &expr.kind
            && let ExprKind::Ident(res) = &arr.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(vid))) = res.first()
        {
            let elem_kind = match &self.gcx.hir.variable(*vid).ty.kind {
                hir::TypeKind::Mapping(m) => &m.value.kind,
                hir::TypeKind::Array(a) => &a.element.kind,
                _ => return None,
            };
            if let hir::TypeKind::Custom(hir::ItemId::Struct(sid)) = elem_kind {
                return Some(*sid);
            }
            return None;
        }
        // Call returning a (storage) struct, e.g. an ERC-7201 `_layout()` getter:
        // use the callee's declared return type.
        if let ExprKind::Call(callee, ..) = &expr.kind
            && let ExprKind::Ident(res) = &callee.kind
        {
            for r in res.iter() {
                if let hir::Res::Item(hir::ItemId::Function(fid)) = r
                    && let Some(&rid) = self.gcx.hir.function(*fid).returns.first()
                    && let hir::TypeKind::Custom(hir::ItemId::Struct(sid)) =
                        &self.gcx.hir.variable(rid).ty.kind
                {
                    return Some(*sid);
                }
            }
        }
        // Fall back to the inferred expression type.
        if let Some(ty) = self.get_expr_type(expr)
            && let TyKind::Struct(sid) = ty.peel_refs().kind
        {
            return Some(sid);
        }
        None
    }

    /// Finds the index of a struct field by name.
    fn struct_field_index(&self, struct_id: hir::StructId, member: Ident) -> Option<usize> {
        let strukt = self.gcx.hir.strukt(struct_id);
        strukt
            .fields
            .iter()
            .position(|&fid| self.gcx.hir.variable(fid).name.is_some_and(|n| n.name == member.name))
    }

    fn is_memory_struct_base(&self, base: &hir::Expr<'_>, struct_id: hir::StructId) -> bool {
        let Some(ty) = self.get_expr_type(base) else { return false };
        match ty.kind {
            TyKind::Ref(inner, solar_ast::DataLocation::Memory) => {
                matches!(inner.kind, TyKind::Struct(id) if id == struct_id)
            }
            TyKind::Struct(id) => id == struct_id,
            _ => false,
        }
    }

    fn lower_storage_struct_field_slot_by_index(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        base: &hir::Expr<'_>,
        struct_id: hir::StructId,
        field_index: usize,
    ) -> Option<ValueId> {
        if self.struct_id_of_expr(base)? != struct_id {
            return None;
        }
        let base_slot = self.lower_lvalue_slot(builder, base)?;
        let field_offset = self.get_struct_field_slot_offset(struct_id, field_index);
        Some(if field_offset == 0 {
            base_slot
        } else {
            let off = builder.imm_u64(field_offset);
            builder.add(base_slot, off)
        })
    }

    /// If `base` is a storage location of struct type and `member` is one of its
    /// fields, returns the field's storage slot (`base_slot + field_offset`) as a
    /// runtime value. Handles storage references (`r.a`) and storage struct
    /// fields reached through indexing (`items[k].a`, `arr[i].a`). Returns `None`
    /// for memory/calldata bases (whose `lower_lvalue_slot` yields `None`).
    fn lower_storage_struct_field_slot(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        base: &hir::Expr<'_>,
        member: Ident,
    ) -> Option<ValueId> {
        let struct_id = self.struct_id_of_expr(base)?;
        let field_index = self.struct_field_index(struct_id, member)?;
        self.lower_storage_struct_field_slot_by_index(builder, base, struct_id, field_index)
    }

    /// Computes the storage slot of an lvalue expression as a runtime value.
    /// Used to bind storage references (`T storage r = <lvalue>`): the pointer's
    /// value is the slot itself. Returns `None` for expressions whose slot we
    /// cannot compute, so the caller can report an error rather than miscompile.
    pub(super) fn lower_lvalue_slot(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> Option<ValueId> {
        match &expr.kind {
            ExprKind::Ident(res_slice) => {
                if let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first() {
                    // Another storage reference: its value is already the slot.
                    if self.storage_ref_locals.contains(*var_id) {
                        return self.locals.get(var_id).copied();
                    }
                    // A state variable: its base slot is known at compile time.
                    if let Some(&slot) = self.storage_slots.get(var_id) {
                        return Some(builder.imm_u64(slot));
                    }
                    if let Some(&slot) = self.struct_storage_base_slots.get(var_id) {
                        return Some(builder.imm_u64(slot));
                    }
                }
                None
            }
            ExprKind::Index(base, index) => {
                self.lower_index_lvalue_slot(builder, base, index.as_deref())
            }
            ExprKind::Member(base, member) => {
                if let Some((struct_id, field_index)) = self.resolved_struct_field(expr)
                    && let Some(slot) = self.lower_storage_struct_field_slot_by_index(
                        builder,
                        base,
                        struct_id,
                        field_index,
                    )
                {
                    return Some(slot);
                }

                // State-variable storage struct field.
                if let Some((base_slot, struct_id, field_index)) =
                    self.get_storage_struct_field_info(base, *member)
                {
                    let field_offset = self.get_struct_field_slot_offset(struct_id, field_index);
                    return Some(builder.imm_u64(base_slot + field_offset));
                }
                // Storage-reference local struct field.
                if let Some((var_id, struct_id, field_index)) =
                    self.get_storage_ref_struct_field_info(base, *member)
                {
                    let field_offset = self.get_struct_field_slot_offset(struct_id, field_index);
                    let base_slot = self.locals.get(&var_id).copied()?;
                    return Some(if field_offset == 0 {
                        base_slot
                    } else {
                        let off = builder.imm_u64(field_offset);
                        builder.add(base_slot, off)
                    });
                }
                // Nested state-variable storage struct field.
                if let Some(slot) = self.compute_nested_storage_slot(base, *member) {
                    return Some(builder.imm_u64(slot));
                }
                None
            }
            // A call to a function returning a storage reference (e.g. the
            // ERC-7201 `_layout()` getter) yields the slot value directly.
            ExprKind::Call(callee, ..) if self.call_returns_storage_ref(callee) => {
                Some(self.lower_expr(builder, expr))
            }
            _ => None,
        }
    }

    /// Whether `callee` resolves to a function whose first return is a storage
    /// reference, so a call to it yields a storage slot value.
    fn call_returns_storage_ref(&self, callee: &hir::Expr<'_>) -> bool {
        let ExprKind::Ident(res) = &callee.kind else {
            return false;
        };
        res.iter().any(|r| {
            if let hir::Res::Item(hir::ItemId::Function(fid)) = r {
                let f = self.gcx.hir.function(*fid);
                f.returns.first().is_some_and(|&rid| {
                    self.gcx.hir.variable(rid).data_location
                        == Some(solar_ast::DataLocation::Storage)
                })
            } else {
                false
            }
        })
    }

    /// Checks if a member access is on a memory struct.
    /// Returns (struct_id, field_index) if the base expression is a memory struct.
    fn get_memory_struct_field_info(
        &self,
        base: &hir::Expr<'_>,
        member: Ident,
    ) -> Option<(hir::StructId, usize)> {
        // The base is a local variable (memory struct) - check if it's in local_memory_slots
        if let ExprKind::Ident(res_slice) = &base.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            let var = self.gcx.hir.variable(*var_id);
            if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind {
                // For memory structs, we need to verify this is NOT a storage struct
                if !self.struct_storage_base_slots.contains_key(var_id) {
                    let strukt = self.gcx.hir.strukt(*struct_id);
                    for (i, &field_id) in strukt.fields.iter().enumerate() {
                        let field = self.gcx.hir.variable(field_id);
                        if let Some(field_name) = field.name
                            && field_name.name == member.name
                        {
                            return Some((*struct_id, i));
                        }
                    }
                }
            }
        }

        // A struct value or a struct reference reached by field access (for
        // example `outer.inner`). A calldata struct parameter is decoded to a
        // memory pointer in the prologue, so its nested struct fields are
        // memory pointers too and read through memory field addressing.
        // Storage bases are handled by earlier member paths.
        let struct_id = self.get_expr_type(base).and_then(|ty| {
            let inner = match ty.kind {
                solar_sema::ty::TyKind::Struct(_) => ty,
                solar_sema::ty::TyKind::Ref(
                    inner,
                    solar_ast::DataLocation::Memory | solar_ast::DataLocation::Calldata,
                ) => inner,
                _ => return None,
            };
            match inner.kind {
                solar_sema::ty::TyKind::Struct(id) => Some(id),
                _ => None,
            }
        });
        if let Some(struct_id) = struct_id {
            let strukt = self.gcx.hir.strukt(struct_id);
            for (i, &field_id) in strukt.fields.iter().enumerate() {
                let field = self.gcx.hir.variable(field_id);
                if let Some(field_name) = field.name
                    && field_name.name == member.name
                {
                    return Some((struct_id, i));
                }
            }
        }
        None
    }

    /// Computes the storage slot for a nested struct member access.
    /// For expressions like `stored.l2.l1.a` where `stored` is a storage struct
    /// with arbitrarily deep nested struct fields.
    /// Returns (slot, struct_id_of_field_type) if the member is a struct, or just slot if scalar.
    fn compute_nested_storage_slot_with_type(
        &mut self,
        expr: &hir::Expr<'_>,
    ) -> Option<(u64, Option<hir::StructId>)> {
        if let ExprKind::Member(base, member) = &expr.kind {
            // First try: base is a direct storage struct variable
            if let Some((base_slot, struct_id, field_index)) =
                self.get_storage_struct_field_info(base, *member)
            {
                let field_offset = self.get_struct_field_slot_offset(struct_id, field_index);
                let slot = base_slot + field_offset;

                // Check if the field itself is a struct
                let strukt = self.gcx.hir.strukt(struct_id);
                let field_var = self.gcx.hir.variable(strukt.fields[field_index]);
                if let hir::TypeKind::Custom(hir::ItemId::Struct(inner_struct_id)) =
                    &field_var.ty.kind
                {
                    return Some((slot, Some(*inner_struct_id)));
                }
                return Some((slot, None));
            }

            // Recursive case: base is itself a nested member access
            if let Some((parent_slot, Some(parent_struct_id))) =
                self.compute_nested_storage_slot_with_type(base)
            {
                // Find the member within the parent struct
                let parent_strukt = self.gcx.hir.strukt(parent_struct_id);
                for (i, &field_id) in parent_strukt.fields.iter().enumerate() {
                    let field = self.gcx.hir.variable(field_id);
                    if let Some(field_name) = field.name
                        && field_name.name == member.name
                    {
                        let field_offset = self.get_struct_field_slot_offset(parent_struct_id, i);
                        let slot = parent_slot + field_offset;

                        // Check if this field is also a struct
                        if let hir::TypeKind::Custom(hir::ItemId::Struct(inner_struct_id)) =
                            &field.ty.kind
                        {
                            return Some((slot, Some(*inner_struct_id)));
                        }
                        return Some((slot, None));
                    }
                }
            }
        }
        None
    }

    /// Computes the storage slot for a nested struct member access (scalar fields only).
    fn compute_nested_storage_slot(&mut self, base: &hir::Expr<'_>, member: Ident) -> Option<u64> {
        // Check if base is a Member expression (needed for 2+ level nesting)
        if let ExprKind::Member(inner_base, inner_member) = &base.kind {
            // Get the slot and type info for the base member expression
            if let Some((parent_slot, Some(parent_struct_id))) =
                self.compute_nested_storage_slot_with_type(base)
            {
                // Find the final member within the parent struct
                let parent_strukt = self.gcx.hir.strukt(parent_struct_id);
                for (i, &field_id) in parent_strukt.fields.iter().enumerate() {
                    let field = self.gcx.hir.variable(field_id);
                    if let Some(field_name) = field.name
                        && field_name.name == member.name
                    {
                        let field_offset = self.get_struct_field_slot_offset(parent_struct_id, i);
                        return Some(parent_slot + field_offset);
                    }
                }
            }

            // Fallback: try the original 2-level approach
            if let Some((base_slot, struct_id, field_index)) =
                self.get_storage_struct_field_info(inner_base, *inner_member)
            {
                let strukt = self.gcx.hir.strukt(struct_id);
                if field_index < strukt.fields.len() {
                    let field_var = self.gcx.hir.variable(strukt.fields[field_index]);
                    if let hir::TypeKind::Custom(hir::ItemId::Struct(inner_struct_id)) =
                        &field_var.ty.kind
                    {
                        let inner_field_offset =
                            self.get_struct_field_slot_offset(struct_id, field_index);
                        let nested_base_slot = base_slot + inner_field_offset;

                        let inner_strukt = self.gcx.hir.strukt(*inner_struct_id);
                        for (i, &inner_field_id) in inner_strukt.fields.iter().enumerate() {
                            let inner_field = self.gcx.hir.variable(inner_field_id);
                            if let Some(field_name) = inner_field.name
                                && field_name.name == member.name
                            {
                                let inner_offset =
                                    self.get_struct_field_slot_offset(*inner_struct_id, i);
                                return Some(nested_base_slot + inner_offset);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Lowers dynamic array method calls (push, pop).
    /// For dynamic arrays:
    /// - Length is stored at the base slot
    /// - Elements are stored at keccak256(slot) + index
    pub(super) fn lower_array_method_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        _var_id: hir::VariableId,
        slot: u64,
        method: Symbol,
        args: &CallArgs<'_>,
    ) -> ValueId {
        let slot_val = builder.imm_u64(slot);

        match method {
            sym::push => {
                // 1. Load current length from slot
                let length = builder.sload(slot_val);

                // 2. Compute data slot: keccak256(slot)
                let mem_0 = builder.imm_u64(0);
                builder.mstore(mem_0, slot_val);
                let size_32 = builder.imm_u64(32);
                let data_slot = builder.keccak256(mem_0, size_32);

                // 3. Compute element slot: data_slot + length
                let element_slot = builder.add(data_slot, length);

                // 4. Store the new value at element slot
                let mut exprs = args.exprs();
                let value = if let Some(first) = exprs.next() {
                    self.lower_expr(builder, first)
                } else {
                    builder.imm_u64(0)
                };
                builder.sstore(element_slot, value);

                // 5. Increment length and store back
                let one = builder.imm_u64(1);
                let new_length = builder.add(length, one);
                let slot_val2 = builder.imm_u64(slot);
                builder.sstore(slot_val2, new_length);

                // push returns void, return dummy
                builder.imm_u64(0)
            }
            kw::Pop => {
                // pop() decrements length and clears the last element
                // Storage layout:
                // - Length at slot
                // - Elements at keccak256(slot) + index

                // 1. Load current length and decrement
                let length = builder.sload(slot_val);
                let one = builder.imm_u64(1);
                let new_length = builder.sub(length, one);

                // 2. Store decremented length back
                let slot_val2 = builder.imm_u64(slot);
                builder.sstore(slot_val2, new_length);

                // 3. Compute data slot: keccak256(slot)
                let slot_val3 = builder.imm_u64(slot);
                let mem_0 = builder.imm_u64(0);
                builder.mstore(mem_0, slot_val3);
                let size_32 = builder.imm_u64(32);
                let data_slot = builder.keccak256(mem_0, size_32);

                // 4. Compute element slot using stored length (already decremented)
                let slot_val4 = builder.imm_u64(slot);
                let length2 = builder.sload(slot_val4);
                let element_slot = builder.add(data_slot, length2);

                // 5. Clear the popped element
                let zero = builder.imm_u64(0);
                builder.sstore(element_slot, zero);

                builder.imm_u64(0)
            }
            s => unreachable!("{s}"),
        }
    }

    /// Checks if an expression is an Index into a nested mapping (e.g., `m[a][b]`).
    /// Returns true if the expression is a nested mapping access.
    fn is_nested_mapping_index(&self, expr: &hir::Expr<'_>) -> bool {
        if let ExprKind::Index(inner_base, _) = &expr.kind {
            // Check if inner_base is a direct mapping variable access
            if self.get_mapping_base_slot(inner_base).is_some() {
                return true;
            }
            // Recursively check for deeper nesting
            return self.is_nested_mapping_index(inner_base);
        }
        false
    }

    /// Computes the storage slot for `base[index]` when the base is a mapping
    /// or nested mapping expression. Also reports whether the indexed value is
    /// itself another mapping, in which case callers should forward the slot
    /// instead of loading from it.
    pub(super) fn lower_mapping_element_slot(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        base: &hir::Expr<'_>,
        index: Option<&hir::Expr<'_>>,
    ) -> Option<MappingElementSlot> {
        // Mapping state variable: base slot is a compile-time constant.
        if let Some((var_id, slot)) = self.get_mapping_base_slot(base) {
            let base_slot = MappingBaseSlot::Const(slot);
            return Some(self.finish_mapping_element_slot(builder, var_id, base_slot, index));
        }

        // Mapping held as a storage-reference parameter or local (e.g. a `library`
        // function taking `mapping(...) storage`): its base slot is a runtime
        // value in `locals`, not a compile-time constant.
        if let Some((var_id, slot_val)) = self.mapping_ref_base_slot_value(base) {
            let base_slot = MappingBaseSlot::Value(slot_val);
            return Some(self.finish_mapping_element_slot(builder, var_id, base_slot, index));
        }

        if self.is_nested_mapping_index(base) {
            let inner_slot = self.lower_nested_mapping_slot(builder, base);
            let index_val = self.lower_index_or_zero(builder, index);
            let key_is_dynamic = self.mapping_level_key_is_dynamic(base);
            let slot = self.compute_mapping_slot_for_index(
                builder,
                index,
                index_val,
                inner_slot,
                key_is_dynamic,
            );
            return Some(MappingElementSlot {
                slot,
                value_is_mapping: self.nested_mapping_value_is_mapping(base),
            });
        }

        None
    }

    /// Given the (already resolved) base slot of a mapping and an index, computes
    /// the element's storage slot, reading the key/value kinds off the mapping's
    /// declared type. Shared by the state-variable and storage-reference paths.
    fn finish_mapping_element_slot(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        var_id: hir::VariableId,
        base_slot: MappingBaseSlot,
        index: Option<&hir::Expr<'_>>,
    ) -> MappingElementSlot {
        let index_val = self.lower_index_or_zero(builder, index);
        // Materialize the base slot after the index so a constant state-variable
        // slot keeps its original emission order (the index is lowered first).
        let slot_val = match base_slot {
            MappingBaseSlot::Const(slot) => builder.imm_u64(slot),
            MappingBaseSlot::Value(val) => val,
        };
        let var = self.gcx.hir.variable(var_id);
        let (key_is_dynamic, value_is_mapping) = if let hir::TypeKind::Mapping(map) = &var.ty.kind {
            (
                Self::is_dynamic_mapping_key(&map.key.kind),
                matches!(map.value.kind, hir::TypeKind::Mapping(_)),
            )
        } else {
            (false, false)
        };
        let slot = self.compute_mapping_slot_for_index(
            builder,
            index,
            index_val,
            slot_val,
            key_is_dynamic,
        );
        MappingElementSlot { slot, value_is_mapping }
    }

    /// If `base` denotes a mapping held as a storage-reference parameter or local
    /// (not a state variable), returns its variable id and the runtime value that
    /// is its base slot. Such a mapping is passed by slot number — its value in
    /// `locals` is the slot itself.
    fn mapping_ref_base_slot_value(
        &self,
        base: &hir::Expr<'_>,
    ) -> Option<(hir::VariableId, ValueId)> {
        let ExprKind::Ident(res_slice) = &base.kind else { return None };
        let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first() else {
            return None;
        };
        if !matches!(self.gcx.hir.variable(*var_id).ty.kind, hir::TypeKind::Mapping(_)) {
            return None;
        }
        let slot_val = self.locals.get(var_id).copied()?;
        Some((*var_id, slot_val))
    }

    /// Computes the storage slot for a nested mapping access.
    /// For `m[a][b]`, this computes: `keccak256(b, keccak256(a, base_slot))`
    fn lower_nested_mapping_slot(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> ValueId {
        if let ExprKind::Index(inner_base, inner_index) = &expr.kind {
            let key_is_dynamic = self.mapping_level_key_is_dynamic(inner_base);

            // Check if inner_base is the root mapping variable
            if let Some((_var_id, slot)) = self.get_mapping_base_slot(inner_base) {
                // Compute the slot for the inner access
                let inner_index_val = match inner_index {
                    Some(idx) => self.lower_expr(builder, idx),
                    None => builder.imm_u64(0),
                };
                let slot_val = builder.imm_u64(slot);
                return self.compute_mapping_slot_for_index(
                    builder,
                    inner_index.as_deref(),
                    inner_index_val,
                    slot_val,
                    key_is_dynamic,
                );
            }

            // Recursively compute deeper nesting slot
            let deeper_slot = self.lower_nested_mapping_slot(builder, inner_base);
            let inner_index_val = match inner_index {
                Some(idx) => self.lower_expr(builder, idx),
                None => builder.imm_u64(0),
            };
            return self.compute_mapping_slot_for_index(
                builder,
                inner_index.as_deref(),
                inner_index_val,
                deeper_slot,
                key_is_dynamic,
            );
        }
        // Should not reach here if is_nested_mapping_index returned true
        builder.imm_u64(0)
    }

    /// Whether the mapping denoted by `base` (the expression being indexed,
    /// `count_index_depth(base)` levels below the root mapping variable) has a
    /// dynamic (`string`/`bytes`) key type.
    fn mapping_level_key_is_dynamic(&self, base: &hir::Expr<'_>) -> bool {
        let Some(var_id) = self.find_mapping_root(base) else {
            return false;
        };
        let depth = self.count_index_depth(base);
        let var = self.gcx.hir.variable(var_id);
        let mut current_ty = &var.ty.kind;
        for _ in 0..depth {
            if let hir::TypeKind::Mapping(map) = current_ty {
                current_ty = &map.value.kind;
            } else {
                return false;
            }
        }
        if let hir::TypeKind::Mapping(map) = current_ty {
            Self::is_dynamic_mapping_key(&map.key.kind)
        } else {
            false
        }
    }

    /// Checks if the value type at this nesting level is itself a mapping.
    /// For `m[a][b]` where `m: mapping(A => mapping(B => C))`, this returns false
    /// because the value at `m[a][b]` is C, not a mapping.
    fn nested_mapping_value_is_mapping(&self, expr: &hir::Expr<'_>) -> bool {
        // Count how many Index levels we have
        let depth = self.count_index_depth(expr);

        // Find the root mapping variable
        let Some(var_id) = self.find_mapping_root(expr) else {
            return false;
        };
        let var = self.gcx.hir.variable(var_id);

        // Navigate `depth + 1` levels into the mapping type to find the value type
        // after indexing one more time from the current expression
        let mut current_ty = &var.ty.kind;
        for _ in 0..=depth {
            if let hir::TypeKind::Mapping(map) = current_ty {
                current_ty = &map.value.kind;
            } else {
                return false;
            }
        }

        matches!(current_ty, hir::TypeKind::Mapping(_))
    }

    /// Counts how many Index levels deep an expression is.
    fn count_index_depth(&self, expr: &hir::Expr<'_>) -> usize {
        let mut depth = 0;
        let mut current = expr;
        while let ExprKind::Index(inner_base, _) = &current.kind {
            depth += 1;
            current = inner_base;
        }
        depth
    }

    /// Finds the root mapping variable of a nested Index expression.
    fn find_mapping_root(&self, expr: &hir::Expr<'_>) -> Option<hir::VariableId> {
        let mut current = expr;
        while let ExprKind::Index(inner_base, _) = &current.kind {
            current = inner_base;
        }
        if let ExprKind::Ident(res_slice) = &current.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            return Some(*var_id);
        }
        None
    }

    /// Computes the storage slot for a mapping access: keccak256(abi.encode(key, slot))
    /// Memory layout: key at offset 0, slot at offset 32, hash from [0, 64)
    fn compute_mapping_slot(
        &self,
        builder: &mut FunctionBuilder<'_>,
        key: ValueId,
        slot: ValueId,
    ) -> ValueId {
        builder.mapping_slot(key, slot)
    }

    /// Dispatches a mapping-key hash on the key kind. Dynamic (`string`/`bytes`)
    /// keys are hashed per spec as `keccak256(key bytes ++ uint256(slot))`;
    /// everything else is the fixed `keccak256(key word ++ slot word)`.
    fn compute_mapping_slot_for_index(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        index_expr: Option<&hir::Expr<'_>>,
        key: ValueId,
        slot: ValueId,
        key_is_dynamic: bool,
    ) -> ValueId {
        if key_is_dynamic && let Some(expr) = index_expr {
            // String/bytes literal: hash exactly the literal's bytes. The
            // lowered `key` is a left-aligned word and must not be hashed.
            if let Some(bytes) = Self::str_lit_key_bytes(expr) {
                return self.compute_literal_mapping_slot(builder, bytes, slot);
            }
            if self.is_dynamic_calldata_arg(Some(expr)) {
                return self.compute_dynamic_calldata_mapping_slot(builder, key, slot);
            }
            // Storage `bytes`/`string` (state variable or a field reached
            // through a storage reference): its lowering already materialized
            // a `[length][data...]` memory copy in `key`.
            if self.expr_yields_memory_bytes(expr) || self.expr_is_storage_bytes_lvalue(expr) {
                return self.compute_dynamic_memory_mapping_slot(builder, key, slot);
            }
            // Storage-reference local (`string storage r`): `key` is the
            // storage slot; materialize to memory first, then hash the bytes.
            if self.is_storage_ref_bytes_local(expr) {
                let ptr = self.materialize_storage_bytes(builder, key);
                return self.compute_dynamic_memory_mapping_slot(builder, ptr, slot);
            }
        }
        self.compute_mapping_slot(builder, key, slot)
    }

    /// Returns the raw bytes of a string/bytes literal expression.
    fn str_lit_key_bytes<'a>(expr: &'a hir::Expr<'_>) -> Option<&'a [u8]> {
        if let ExprKind::Lit(lit) = &expr.kind
            && let LitKind::Str(_, bytes, _) = &lit.kind
        {
            return Some(bytes.as_byte_str());
        }
        None
    }

    /// Whether `expr` is a storage-reference local of `string`/`bytes` type,
    /// which lowers to its storage slot rather than a memory pointer.
    fn is_storage_ref_bytes_local(&self, expr: &hir::Expr<'_>) -> bool {
        if let ExprKind::Ident(res_slice) = &expr.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
            && self.storage_ref_locals.contains(*var_id)
        {
            let var = self.gcx.hir.variable(*var_id);
            return Self::is_dynamic_mapping_key(&var.ty.kind);
        }
        false
    }

    /// Hashes a literal mapping key per spec: stage the literal's bytes at the
    /// unbumped free-memory scratch, append the 32-byte slot, and hash exactly
    /// `len + 32` bytes. The trailing slot store overwrites any zero padding
    /// written by the last partial data word.
    fn compute_literal_mapping_slot(
        &self,
        builder: &mut FunctionBuilder<'_>,
        bytes: &[u8],
        slot: ValueId,
    ) -> ValueId {
        let scratch = builder.fmp();
        for (i, chunk) in bytes.chunks(32).enumerate() {
            let mut padded = [0u8; 32];
            padded[..chunk.len()].copy_from_slice(chunk);
            let val = builder.imm_u256(U256::from_be_bytes(padded));
            let off = builder.imm_u64((i * 32) as u64);
            let dest = builder.add(scratch, off);
            builder.mstore(dest, val);
        }
        let len = builder.imm_u64(bytes.len() as u64);
        let slot_addr = builder.add(scratch, len);
        builder.mstore(slot_addr, slot);
        let word_size = builder.imm_u64(32);
        let hash_len = builder.add(len, word_size);
        builder.keccak256(scratch, hash_len)
    }

    fn compute_dynamic_memory_mapping_slot(
        &self,
        builder: &mut FunctionBuilder<'_>,
        ptr: ValueId,
        slot: ValueId,
    ) -> ValueId {
        if self.gcx.sess.opts.evm_version.has_mcopy() {
            return builder.mapping_slot_memory(ptr, slot);
        }

        let len = builder.memory_object_len(ptr, MemoryObjectKind::Bytes);
        let word_size = builder.imm_u64(32);
        let data_start = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);
        let scratch = builder.fmp();
        self.mcopy(builder, scratch, data_start, len, None);
        let slot_addr = builder.add(scratch, len);
        builder.mstore(slot_addr, slot);
        let hash_len = builder.add(len, word_size);
        builder.keccak256(scratch, hash_len)
    }

    fn compute_dynamic_calldata_mapping_slot(
        &self,
        builder: &mut FunctionBuilder<'_>,
        slice: ValueId,
        slot: ValueId,
    ) -> ValueId {
        builder.mapping_slot_calldata(slice, slot)
    }

    fn is_dynamic_mapping_key(kind: &hir::TypeKind<'_>) -> bool {
        matches!(
            kind,
            hir::TypeKind::Elementary(hir::ElementaryType::String | hir::ElementaryType::Bytes)
        )
    }

    fn is_dynamic_calldata_arg(&self, expr: Option<&hir::Expr<'_>>) -> bool {
        let Some(expr) = expr else {
            return false;
        };
        let ExprKind::Ident(res_slice) = &expr.kind else {
            return false;
        };
        let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first() else {
            return false;
        };
        if !self.locals.contains_key(var_id) || self.get_local_memory_offset(var_id).is_some() {
            return false;
        }
        let var = self.gcx.hir.variable(*var_id);
        if var.data_location != Some(solar_ast::DataLocation::Calldata) {
            return false;
        }
        Self::is_dynamic_mapping_key(&var.ty.kind)
    }
}
