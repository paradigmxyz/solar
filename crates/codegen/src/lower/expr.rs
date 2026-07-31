//! Expression lowering.

use super::{
    Lowerer, MIN_BULK_ZERO_MEMORY_WORDS,
    call::StorageArrayMethod,
    checked_arith::{ArithmeticInfo, PanicCode},
};
use crate::{
    memory::EvmMemoryLayout,
    mir::{FunctionBuilder, MemoryObjectKind, TypeSize, ValueId},
};
use alloy_primitives::U256;
use solar_ast::{LitKind, StrKind};
use solar_interface::{Ident, Span, diagnostics::ErrorGuaranteed, sym};
use solar_sema::{
    builtins::Builtin,
    hir::{self, CallArgs, ElementaryType, ExprKind},
    ty::{CallableParamSource, Ty, TyKind},
};

/// Small structs are cheaper to initialize with individual zero stores.
const MIN_BULK_ZERO_STRUCT_FIELDS: usize = 4;

pub(super) struct MappingElementSlot {
    pub(super) slot: ValueId,
    pub(super) value_is_mapping: bool,
}

/// The base storage slot of a mapping: a compile-time constant for a state
/// variable, or a runtime value for a storage-reference parameter/local.
enum MappingBaseSlot {
    Const(U256),
    Value(ValueId),
}

/// How one member of an ABI tuple region is decoded into memory.
pub(super) enum DecodeStrategy<'gcx> {
    /// An elementary value word, validated for cleanliness.
    Word(ElementaryType),
    /// A dynamic `bytes`/`string`.
    DynBytes,
    /// A dynamic array with an elementary element type.
    ElementaryArray(ElementaryType),
    /// A struct, tuple, fixed array, or aggregate array, decoded through the
    /// general recursive materializer.
    General(Ty<'gcx>),
}

impl<'gcx> Lowerer<'gcx> {
    /// Lowers an expression, preserving whether it produces one MIR value.
    pub(super) fn lower_expr(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> Option<ValueId> {
        if self.check_expr_errors
            && let Err(guar) = self.expr_references_error(expr)
        {
            return self.expr_error_result(builder, expr, guar);
        }
        let check_expr_errors = std::mem::replace(&mut self.check_expr_errors, false);
        let result = if let ExprKind::Assign(lhs, None, rhs) = &expr.kind
            && let ExprKind::Tuple(elements) = &lhs.kind
        {
            self.lower_tuple_assign(builder, elements, rhs);
            None
        } else if self.get_expr_type(expr).is_some_and(|ty| ty.is_unit()) {
            self.lower_unit_expr(builder, expr);
            None
        } else {
            Some(self.lower_value_expr_unchecked(builder, expr))
        };
        self.check_expr_errors = check_expr_errors;
        result
    }

    /// Lowers an expression which is required to produce a MIR value.
    pub(super) fn lower_value_expr(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> ValueId {
        if self.check_expr_errors
            && let Err(guar) = self.expr_references_error(expr)
        {
            return builder.error_value(guar);
        }
        let check_expr_errors = std::mem::replace(&mut self.check_expr_errors, false);
        let value = self.lower_value_expr_unchecked(builder, expr);
        self.check_expr_errors = check_expr_errors;
        value
    }

    fn lower_value_expr_unchecked(
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

            ExprKind::Ident(_) => {
                if let Some(res) = self.gcx.resolved_expr(expr) {
                    self.lower_ident(builder, &res, expr.span)
                } else {
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
                // A user-defined operator on a UDVT resolves to a function; call
                // it with the operand values (UDVTs are transparent at runtime).
                if let Some(op_fn) = self.gcx.user_operator(expr.id) {
                    let lhs_val = self.lower_value_expr(builder, lhs);
                    let rhs_val = self.lower_value_expr(builder, rhs);
                    return self
                        .lower_internal_call_values(builder, op_fn, vec![lhs_val, rhs_val])
                        .unwrap_or_else(|| {
                            self.err_value(
                                builder,
                                expr.span,
                                "codegen expected user-defined operator to return a value",
                            )
                        });
                }

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
                    (self.lower_value_expr(builder, lhs), self.lower_value_expr(builder, rhs))
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
                    return self.clean_fixed_bytes(builder, result, TypeSize::new_fb_bytes(width));
                }
                result
            }

            ExprKind::Unary(op, operand) => {
                use hir::UnOpKind;
                match op.kind {
                    UnOpKind::PreInc | UnOpKind::PostInc | UnOpKind::PreDec | UnOpKind::PostDec => {
                        // Increment/decrement need to read, compute, store, and return
                        let operand_val = self.lower_value_expr(builder, operand);
                        let one = builder.imm_u64(1);
                        let int_info = self.integer_info_for_expr(operand);
                        if self.gcx.unsupported_udvt_operator(expr.id) {
                            let guar = self.emit_unsupported_udvt_operator(operand.span);
                            return builder.error_value(guar);
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
                        // A user-defined unary operator resolves to a
                        // single-argument function.
                        if let Some(op_fn) = self.gcx.user_operator(expr.id) {
                            let operand_val = self.lower_value_expr(builder, operand);
                            return self
                                .lower_internal_call_values(builder, op_fn, vec![operand_val])
                                .unwrap_or_else(|| {
                                    self.err_value(
                                        builder,
                                        expr.span,
                                        "codegen expected user-defined operator to return a value",
                                    )
                                });
                        }
                        let operand_val = self.lower_value_expr(builder, operand);
                        let int_info = self
                            .integer_info_for_expr(expr)
                            .or_else(|| self.integer_info_for_expr(operand));
                        if self.gcx.unsupported_udvt_operator(expr.id) {
                            let guar = self.emit_unsupported_udvt_operator(expr.span);
                            return builder.error_value(guar);
                        }
                        self.lower_unary_op(builder, *op, operand_val, int_info, expr.span)
                    }
                }
            }

            ExprKind::Ternary(cond, then_expr, else_expr) => {
                self.lower_ternary(builder, expr, cond, then_expr, else_expr)
            }

            ExprKind::Call(callee, args, call_opts) => self
                .lower_call(builder, callee, args, (*call_opts).map(|opts| opts.args))
                .unwrap_or_else(|| {
                    self.gcx
                        .dcx()
                        .bug("unit call expression lowered in value context")
                        .span(expr.span)
                        .emit()
                }),

            ExprKind::Index(base, index) => {
                self.lower_index_expr(builder, expr, base, index.as_deref())
            }

            ExprKind::Member(base, member) => {
                if let Some(builtin) = self.gcx.resolved_builtin(expr) {
                    match builtin {
                        // Handle address member access: addr.balance
                        Builtin::AddressBalance => {
                            let addr = self.lower_value_expr(builder, base);
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
                            if let Some(selector) = self.ident_function_selector(base) {
                                return builder.imm_u256(U256::from(selector) << 224);
                            }
                        }
                        Builtin::EventSelector => {
                            if let Some(selector) = self.lower_resolved_event_selector(base) {
                                return builder.imm_u256(selector);
                            }
                        }
                        Builtin::InterfaceId => {
                            if let ExprKind::TypeCall(ty) = &base.kind {
                                return self.lower_interface_id(builder, ty);
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
                        | Builtin::AbiDecode => {
                            return self.lower_builtin(builder, builtin, expr.span);
                        }
                        _ => {}
                    }
                }

                // Handle enum variant access (e.g., Status.Active or Contract.Status.Active).
                if let Some((_enum_id, variant_index)) = self.resolved_enum_variant(expr) {
                    return builder.imm_u64(variant_index as u64);
                }

                if let Some(TyKind::Fn(function)) = self.get_expr_type(expr).map(|ty| ty.kind)
                    && function.is_internal()
                    && let Some(hir::Res::Item(hir::ItemId::Function(function_id))) =
                        self.gcx.resolved_expr(expr)
                {
                    let function_id =
                        self.resolved_exact_function_callee(base, expr).unwrap_or(function_id);
                    self.internal_function_pointer_targets.insert(function_id);
                    return builder.imm_u64(Self::internal_function_pointer_id(function_id));
                }

                if let Some(TyKind::Fn(function)) = self.get_expr_type(expr).map(|ty| ty.kind)
                    && function.is_external()
                    && let Some(function_id) =
                        function.function_id.or_else(|| self.gcx.resolved_function(expr))
                {
                    let address = self.lower_value_expr(builder, base);
                    let address_shift = builder.imm_u64(32);
                    let address = builder.shl(address_shift, address);
                    let selector =
                        u32::from_be_bytes(self.gcx.function_selector(function_id).0) as u64;
                    let selector = builder.imm_u64(selector);
                    return builder.or(address, selector);
                }

                // Handle contract/library constants (e.g. MachineLib.NO_RECOVERY_PC).
                if let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) =
                    self.gcx.resolved_expr(expr)
                {
                    let var = self.gcx.hir.variable(var_id);
                    if var.is_constant()
                        && let Some(init) = var.initializer
                    {
                        return self.lower_value_expr(builder, init);
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
                    let slot = base_slot + U256::from(field_offset);
                    let slot_val = builder.imm_u256(slot);
                    return builder.sload(slot_val);
                }

                // Check if this is a nested storage struct access (e.g., storedNested.point.x)
                if let Some(slot) = self.compute_nested_storage_slot(base, *member) {
                    let slot_val = builder.imm_u256(slot);
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
                    let base_val = self.lower_value_expr(builder, base);
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
                    let base_val = self.lower_value_expr(builder, base);
                    let fields = self.gcx.hir.strukt(struct_id).fields.len() as u64;
                    let field_addr = builder.memory_object_field_addr(
                        base_val,
                        crate::mir::MemoryObjectLayout::structure(fields),
                        field_index as u64,
                    );
                    return builder.mload(field_addr);
                }

                // Fallback: just load from base address
                let base_val = self.lower_value_expr(builder, base);
                builder.mload(base_val)
            }

            ExprKind::YulMember(base, member) => self.lower_yul_member(builder, base, *member),

            ExprKind::Assign(lhs, op, rhs) => {
                // Tuple destructuring to existing lvalues, `(a, b) = rhs`.
                if op.is_none()
                    && let ExprKind::Tuple(elements) = &lhs.kind
                {
                    self.lower_tuple_assign(builder, elements, rhs);
                    return self.err_value(
                        builder,
                        expr.span,
                        "tuple assignment does not produce a single value",
                    );
                }
                let rhs_val = if op.is_none()
                    && self
                        .gcx
                        .resolved_variable(lhs)
                        .is_some_and(|var_id| self.storage_ref_locals.contains(var_id))
                {
                    self.lower_lvalue_slot(builder, rhs).unwrap_or_else(|| {
                        self.err_value(
                            builder,
                            rhs.span,
                            "unsupported storage reference assignment",
                        )
                    })
                } else if op.is_none() && self.lhs_expects_memory_bytes_value(lhs) {
                    self.lower_expr_as_memory_bytes(builder, rhs)
                } else if op.is_none() && self.lhs_expects_memory_dyn_array_value(lhs) {
                    self.lower_expr_as_memory_dyn_array(builder, rhs)
                } else {
                    self.lower_value_expr(builder, rhs)
                };
                // Handle compound assignment (+=, -=, etc.)
                let final_val = if let Some(bin_op) = op {
                    // Read current value, apply operator, then assign
                    let lhs_val = self.lower_value_expr(builder, lhs);
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
                if let [Some(expr)] = elements {
                    self.lower_value_expr(builder, expr)
                } else {
                    self.err_value(
                        builder,
                        expr.span,
                        "tuple expression does not produce a single value",
                    )
                }
            }

            ExprKind::Array(elements) => {
                let Some(alloc_size) =
                    u64::try_from(elements.len()).ok().and_then(|len| len.checked_mul(32))
                else {
                    return self.err_value(
                        builder,
                        expr.span,
                        "array literal is too large for codegen",
                    );
                };
                let ptr = self.allocate_memory_object(
                    builder,
                    alloc_size,
                    crate::mir::MemoryObjectKind::FixedArray,
                );
                for (i, elem) in elements.iter().enumerate() {
                    let elem_val = self.lower_value_expr(builder, elem);
                    let offset_const = builder.imm_u64(i as u64 * 32);
                    let addr = builder.add(ptr, offset_const);
                    builder.mstore(addr, elem_val);
                }
                ptr
            }

            ExprKind::TypeCall(_) => {
                self.err_value(builder, expr.span, "`type(...)` does not produce a value")
            }

            ExprKind::Payable(inner) => self.lower_value_expr(builder, inner),

            ExprKind::New(_) => {
                self.err_value(builder, expr.span, "`new` must be used as a call expression")
            }

            ExprKind::Delete(_) => self
                .gcx
                .dcx()
                .bug("unit `delete` expression lowered in value context")
                .span(expr.span)
                .emit(),

            ExprKind::Slice(base, start, end) => {
                // Slicing a `calldata` struct member has to stay in calldata:
                // the result keeps a calldata-located type, so it may be sliced
                // again or have its `.offset` read in assembly, neither of which
                // the rebuilt copy can answer. Other reads of a member go
                // through the copy.
                let is_bytes = self.expr_is_calldata_dynamic_bytes(base);
                let member_slice = self.calldata_member_slice(builder, base);
                let value = match member_slice {
                    Some(slice) => slice,
                    None => self.lower_value_expr(builder, base),
                };
                let source = if Self::value_is_calldata_slice(builder, value) {
                    Some((value, crate::mir::SliceLocation::Calldata))
                } else if is_bytes || self.is_dynamic_array_expr(base) {
                    let kind = if is_bytes {
                        crate::mir::MemoryObjectKind::Bytes
                    } else {
                        crate::mir::MemoryObjectKind::DynamicArray
                    };
                    let len = builder.memory_object_len(value, kind);
                    let data = builder.memory_object_data(value, kind);
                    Some((
                        builder.make_slice(data, len, crate::mir::SliceLocation::Memory),
                        crate::mir::SliceLocation::Memory,
                    ))
                } else {
                    None
                };
                if let Some((slice, location)) = source {
                    // A slice of a calldata array whose elements are dynamic
                    // keeps element offset words relative to the original
                    // array base, which the slice value does not carry, so a
                    // rebuild would read from the wrong positions. Reject
                    // rather than miscompile.
                    if location == crate::mir::SliceLocation::Calldata
                        && !is_bytes
                        && let Some(ty) = self.get_expr_type(base)
                        && let TyKind::DynArray(elem) = ty.peel_refs().kind
                        && !self.abi_is_word_element(elem)
                    {
                        return self.err_value(
                            builder,
                            expr.span,
                            "codegen does not support slicing a calldata array of dynamic \
                             elements yet",
                        );
                    }
                    let base_ptr = builder.slice_ptr(slice);
                    let base_len = builder.slice_len(slice);
                    let start_val = start
                        .map(|s| self.lower_value_expr(builder, s))
                        .unwrap_or_else(|| builder.imm_u64(0));
                    let end_val =
                        end.map(|e| self.lower_value_expr(builder, e)).unwrap_or(base_len);
                    if end_val != base_len {
                        let end_out_of_bounds = builder.gt(end_val, base_len);
                        Self::emit_revert_if(builder, end_out_of_bounds);
                    }
                    let backwards = builder.lt(end_val, start_val);
                    Self::emit_revert_if(builder, backwards);
                    let len = builder.sub(end_val, start_val);
                    let offset = if is_bytes {
                        start_val
                    } else {
                        let word = builder.imm_u64(32);
                        builder.mul(start_val, word)
                    };
                    let ptr = builder.add(base_ptr, offset);
                    return builder.make_slice(ptr, len, location);
                }
                self.err_value(builder, expr.span, "codegen only supports slicing calldata arrays")
            }

            ExprKind::Type(_) => {
                self.err_value(builder, expr.span, "a type name does not produce a value")
            }

            ExprKind::Err(guar) => builder.error_value(*guar),
        }
    }

    fn expr_error_result(
        &self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
        guar: ErrorGuaranteed,
    ) -> Option<ValueId> {
        (!self.get_expr_type(expr).is_some_and(|ty| ty.is_unit()))
            .then(|| builder.error_value(guar))
    }

    fn lower_unit_expr(&mut self, builder: &mut FunctionBuilder<'_>, expr: &hir::Expr<'_>) {
        match &expr.kind {
            ExprKind::Call(callee, args, call_opts) => {
                let result =
                    self.lower_call(builder, callee, args, (*call_opts).map(|opts| opts.args));
                if result.is_some() {
                    self.gcx
                        .dcx()
                        .bug("unit call expression produced a MIR value")
                        .span(expr.span)
                        .emit();
                }
            }
            ExprKind::Delete(target) => self.lower_delete(builder, target),
            ExprKind::Ternary(cond, then_expr, else_expr) => {
                self.lower_unit_ternary(builder, cond, then_expr, else_expr);
            }
            ExprKind::Tuple(elements) => {
                for element in elements.iter().flatten() {
                    let _ = self.lower_expr(builder, element);
                }
            }
            ExprKind::Err(_) => {}
            _ => {
                self.gcx
                    .dcx()
                    .bug(format!("unexpected unit expression in codegen: {:?}", expr.kind))
                    .span(expr.span)
                    .emit();
            }
        }
    }

    fn lower_delete(&mut self, builder: &mut FunctionBuilder<'_>, target: &hir::Expr<'_>) {
        if let Some(ty) = self.get_expr_type(target)
            && let TyKind::Struct(struct_id) = ty.peel_refs().kind
            && let Some(slot) = self.lower_lvalue_slot(builder, target)
        {
            self.clear_storage_struct_at(builder, struct_id, slot);
            return;
        }

        // Deleting a memory fixed-size array zeroes its elements in place;
        // nulling the pointer would alias scratch memory on the next access.
        // Storage targets keep the assignment path.
        if let Some(var_id) = self.gcx.resolved_variable(target)
            && self.gcx.hir.variable(var_id).is_local_variable()
            && !self.storage_ref_locals.contains(var_id)
            && !self.storage_slots.contains_key(&var_id)
        {
            let var = self.gcx.hir.variable(var_id);
            let ty = self.gcx.type_of_item(var_id.into()).peel_refs();
            if matches!(var.data_location, None | Some(solar_ast::DataLocation::Memory))
                && let TyKind::Array(element_ty, len) = ty.kind
                && let Ok(len) = u64::try_from(len)
            {
                let ptr = self.lower_value_expr(builder, target);
                if len >= MIN_BULK_ZERO_MEMORY_WORDS && element_ty.peel_refs().is_value_type() {
                    let size = builder.imm_u64(len * EvmMemoryLayout::WORD_SIZE);
                    builder.memory_zero(ptr, size);
                    return;
                }
                for i in 0..len {
                    let value = self.zero_memory_field_value_ty(builder, element_ty, var.ty.span);
                    if i == 0 {
                        builder.mstore(ptr, value);
                    } else {
                        let offset = builder.imm_u64(i * 32);
                        let addr = builder.add(ptr, offset);
                        builder.mstore(addr, value);
                    }
                }
                return;
            }
        }

        let zero = builder.imm_u256(U256::ZERO);
        self.lower_assign(builder, target, zero);
    }

    fn lower_unit_ternary(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        cond: &hir::Expr<'_>,
        then_expr: &hir::Expr<'_>,
        else_expr: &hir::Expr<'_>,
    ) {
        let cond = self.lower_value_expr(builder, cond);
        let then_block = builder.create_block();
        let else_block = builder.create_block();
        let merge_block = builder.create_block();
        builder.branch(cond, then_block, else_block);

        for (block, arm) in [(then_block, then_expr), (else_block, else_expr)] {
            builder.switch_to_block(block);
            let _ = self.lower_expr(builder, arm);
            if !builder.func().block(builder.current_block()).is_terminated() {
                builder.jump(merge_block);
            }
        }

        builder.switch_to_block(merge_block);
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
            LitKind::Rational(_) => self.err_value(
                builder,
                lit.span,
                "fractional rational literal cannot be lowered to an EVM value",
            ),
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
            LitKind::Err(guar) => builder.error_value(*guar),
        }
    }

    /// Lowers an identifier reference.
    fn lower_ident(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        res: &hir::Res,
        span: Span,
    ) -> ValueId {
        match res {
            hir::Res::Item(item_id) => {
                if let hir::ItemId::Function(function_id) = item_id {
                    let function_id = self.virtual_function_target(*function_id);
                    self.internal_function_pointer_targets.insert(function_id);
                    return builder.imm_u64(Self::internal_function_pointer_id(function_id));
                }
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
                        return self.lower_value_expr(builder, init);
                    }

                    // Check if it's an immutable - load from appended runtime data.
                    if let Some(&id) = self.immutable_ids.get(var_id) {
                        return self.load_immutable_value(builder, id);
                    }

                    // Check if it's a storage variable
                    if let Some(&location) = self.storage_locations.get(var_id) {
                        let slot = location.slot;
                        // For storage structs, we need to copy to memory and return the pointer
                        if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind
                        {
                            // Calculate total flattened size (handles nested structs)
                            let total_words = self.calculate_memory_words_for_ty(
                                self.gcx.type_of_item((*var_id).into()),
                            );
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
                        let slot_val = builder.imm_u256(slot);
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

                    if let Some(value) = self.lower_default_variable_value(builder, *var_id) {
                        return value;
                    }
                }
                self.err_value(builder, span, "codegen cannot lower this item as a value")
            }
            hir::Res::Builtin(builtin) => self.lower_builtin(builder, *builtin, span),
            hir::Res::Namespace(_) => {
                self.err_value(builder, span, "a namespace does not produce a value")
            }
            hir::Res::Err(guar) => builder.error_value(*guar),
        }
    }

    /// Materializes a wide default return struct with one bulk zeroing
    /// operation while giving reference fields real empty objects.
    pub(super) fn lower_bulk_zero_return_struct(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        var_id: hir::VariableId,
    ) -> Option<ValueId> {
        let var = self.gcx.hir.variable(var_id);
        let ty = self.gcx.type_of_hir_ty(&var.ty);
        let TyKind::Struct(struct_id) = ty.peel_refs().kind else { return None };
        if var.initializer.is_some()
            || var.data_location != Some(solar_ast::DataLocation::Memory)
            || self.lowering_internal_function
            || self.current_return_tys.len() != 1
        {
            return None;
        }

        let field_tys = self.gcx.struct_field_types(struct_id).to_vec();
        if field_tys.len() < MIN_BULK_ZERO_STRUCT_FIELDS {
            return None;
        }
        let ptr = self.allocate_zeroed_memory_object(
            builder,
            self.calculate_memory_words_for_ty(ty) * crate::memory::EvmMemoryLayout::WORD_SIZE,
            crate::mir::MemoryObjectKind::Struct,
        );
        let layout = crate::mir::MemoryObjectLayout::structure(field_tys.len() as u64);
        for (i, field_ty) in field_tys.into_iter().enumerate() {
            if field_ty.peel_refs().is_value_type() {
                continue;
            }
            let value = self.zero_memory_field_value_ty(builder, field_ty, var.ty.span);
            let field_addr = builder.memory_object_field_addr(ptr, layout, i as u64);
            builder.mstore(field_addr, value);
        }
        Some(ptr)
    }

    /// Materializes a language-defined default for an uninitialized local or
    /// named return. Reference values get a real empty object rather than a
    /// zero pointer.
    pub(super) fn lower_default_variable_value(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        var_id: hir::VariableId,
    ) -> Option<ValueId> {
        let var = self.gcx.hir.variable(var_id);
        if var.initializer.is_some() || !var.is_local_or_return() {
            return None;
        }
        if Self::calldata_dynamic_var_kind(var).is_some() {
            let zero = builder.imm_u64(0);
            return Some(builder.make_slice(zero, zero, crate::mir::SliceLocation::Calldata));
        }

        let ty = self.gcx.type_of_item(var_id.into());
        if let TyKind::Err(guar) = ty.peel_refs().kind {
            return Some(builder.error_value(guar));
        }
        if var.data_location == Some(solar_ast::DataLocation::Memory) && !ty.is_value_type() {
            return Some(self.zero_memory_field_value_ty(builder, ty, var.ty.span));
        }
        ty.is_value_type().then(|| builder.imm_u64(0))
    }

    /// Lowers a builtin reference.
    fn lower_builtin(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        builtin: Builtin,
        span: Span,
    ) -> ValueId {
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
            Builtin::MsgSig => {
                let zero = builder.imm_u64(0);
                let word = builder.calldataload(zero);
                let shift = builder.imm_u64(224);
                let selector = builder.shr(shift, word);
                builder.shl(shift, selector)
            }
            Builtin::BlockCoinbase => builder.coinbase(),
            Builtin::BlockTimestamp => builder.timestamp(),
            Builtin::BlockDifficulty | Builtin::BlockPrevrandao => builder.prevrandao(),
            Builtin::BlockNumber => builder.number(),
            Builtin::BlockGaslimit => builder.gaslimit(),
            Builtin::BlockChainid => builder.chainid(),
            Builtin::BlockBasefee => builder.basefee(),
            Builtin::BlockBlobbasefee => builder.blobbasefee(),
            Builtin::TxOrigin => builder.origin(),
            Builtin::TxGasPrice => builder.gasprice(),
            Builtin::Gasleft | Builtin::MsgGas => builder.gas(),
            Builtin::This => builder.address(),
            _ => self.err_value(
                builder,
                span,
                format!("builtin `{}` does not produce a value", builtin.name()),
            ),
        }
    }

    fn lower_yul_member(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        base: &hir::Expr<'_>,
        member: Ident,
    ) -> ValueId {
        let Some(var_id) = self.gcx.resolved_variable(base) else {
            return self.err_value(
                builder,
                member.span,
                format!("unsupported Yul member `.{}`", member.name),
            );
        };
        match member.name {
            sym::slot => {
                if let Some(&slot) = self.storage_slots.get(&var_id) {
                    return builder.imm_u256(slot);
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
                if Self::calldata_dynamic_var_kind(self.gcx.hir.variable(var_id)).is_some() {
                    if self.is_slice_slot_local(&var_id)
                        && let Some(offset) = self.get_local_memory_offset(&var_id)
                    {
                        let addr = self.local_memory_addr(builder, offset);
                        return builder.mload(addr);
                    }
                    if let Some(&slice) = self.locals.get(&var_id) {
                        return builder.slice_ptr(slice);
                    }
                    return builder.imm_u64(0);
                }
            }
            sym::length
                if Self::calldata_dynamic_var_kind(self.gcx.hir.variable(var_id)).is_some() =>
            {
                if self.is_slice_slot_local(&var_id)
                    && let Some(offset) = self.get_local_memory_offset(&var_id)
                {
                    let addr = self.local_memory_addr(builder, offset + EvmMemoryLayout::WORD_SIZE);
                    return builder.mload(addr);
                }
                if let Some(&slice) = self.locals.get(&var_id) {
                    return builder.slice_len(slice);
                }
                return builder.imm_u64(0);
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
        let lhs_val = self.lower_value_expr(builder, lhs);
        let pred_block = builder.current_block();
        let rhs_block = builder.create_block();
        let merge_block = builder.create_block();
        if is_and {
            builder.branch(lhs_val, rhs_block, merge_block);
        } else {
            builder.branch(lhs_val, merge_block, rhs_block);
        }

        builder.switch_to_block(rhs_block);
        let rhs_val = self.lower_value_expr(builder, rhs);
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
                ExprKind::Ident(_) => {
                    let var = self.gcx.hir.variable(self.gcx.resolved_variable(expr)?);
                    if !var.is_constant() {
                        return None;
                    }
                    expr = var.initializer?;
                }
                ExprKind::Member(..) => {
                    let hir::Res::Item(hir::ItemId::Variable(var_id)) =
                        self.gcx.resolved_expr(expr)?
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
            self.lower_value_expr(builder, expr)
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
        builder.mcopy(data_offset, src, len);
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

    pub(super) fn lower_resolved_function_selector(&self, expr: &hir::Expr<'_>) -> Option<u32> {
        let hir::Res::Item(item_id) = self.gcx.resolved_expr(expr)? else {
            return None;
        };
        match item_id {
            hir::ItemId::Function(id) => Some(u32::from_be_bytes(self.gcx.function_selector(id).0)),
            hir::ItemId::Error(id) => Some(u32::from_be_bytes(self.gcx.function_selector(id).0)),
            _ => None,
        }
    }

    /// The selector of a bare function name used as `f.selector`.
    ///
    /// Name resolution hands back every candidate for the name without
    /// accounting for overloading, and the type checker only disambiguates
    /// callees, so a name reached through an override chain or visible along
    /// several inheritance paths arrives here as a multi-candidate set. Those
    /// candidates all describe the same signature and so share one selector;
    /// take it. A set that genuinely disagrees is ambiguous in Solidity too,
    /// and keeps the caller's diagnostic.
    fn ident_function_selector(&self, expr: &hir::Expr<'_>) -> Option<u32> {
        let ExprKind::Ident(res_slice) = &expr.kind else { return None };
        let mut selector = None;
        for res in res_slice.iter() {
            let hir::Res::Item(item_id) = res else { return None };
            let candidate = match *item_id {
                hir::ItemId::Function(id) => u32::from_be_bytes(self.gcx.function_selector(id).0),
                hir::ItemId::Error(id) => u32::from_be_bytes(self.gcx.function_selector(id).0),
                _ => return None,
            };
            match selector {
                None => selector = Some(candidate),
                Some(selector) if selector == candidate => {}
                Some(_) => return None,
            }
        }
        selector
    }

    fn lower_resolved_event_selector(&self, expr: &hir::Expr<'_>) -> Option<U256> {
        let hir::Res::Item(hir::ItemId::Event(event_id)) = self.gcx.resolved_expr(expr)? else {
            return None;
        };
        Some(U256::from_be_bytes(self.gcx.event_selector(event_id).0))
    }

    /// Lowers `type(Interface).interfaceId` to its left-aligned `bytes4` value.
    fn lower_interface_id(&self, builder: &mut FunctionBuilder<'_>, ty: &hir::Type<'_>) -> ValueId {
        let hir::TypeKind::Custom(hir::ItemId::Contract(contract_id)) = ty.kind else {
            return self.err_value(
                builder,
                ty.span,
                "codegen expected an interface type for `interfaceId`",
            );
        };
        if !self.gcx.hir.contract(contract_id).kind.is_interface() {
            return self.err_value(
                builder,
                ty.span,
                "codegen expected an interface type for `interfaceId`",
            );
        }

        let selector = u32::from_be_bytes(self.gcx.interface_id(contract_id).0);
        builder.imm_u256(U256::from(selector) << 224)
    }

    /// Lowers type(T).min or type(T).max to a constant value.
    fn lower_type_minmax(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: &hir::Type<'_>,
        is_max: bool,
    ) -> ValueId {
        match &ty.kind {
            hir::TypeKind::Custom(hir::ItemId::Enum(enum_id)) => {
                let value =
                    if is_max { self.gcx.hir.enumm(*enum_id).variants.len() - 1 } else { 0 };
                builder.imm_u64(value as u64)
            }
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
                _ => self.err_value(
                    builder,
                    ty.span,
                    "`type(T).min` and `type(T).max` require an integer type",
                ),
            },
            _ => self.err_value(
                builder,
                ty.span,
                "`type(T).min` and `type(T).max` require an integer type",
            ),
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
            let cond_val = self.lower_value_expr(builder, cond);

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
            let cond_val = self.lower_value_expr(builder, cond);

            let then_block = builder.create_block();
            let else_block = builder.create_block();
            let merge_block = builder.create_block();

            builder.branch(cond_val, then_block, else_block);

            for (block, arm) in [(then_block, then_expr), (else_block, else_expr)] {
                builder.switch_to_block(block);
                if slice_location.is_some() {
                    let value = self.lower_value_expr(builder, arm);
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
        self.lower_value_expr(builder, arm)
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
                .filter_map(|elem| elem.map(|elem| self.lower_value_expr(builder, elem)))
                .collect::<Vec<_>>()
        } else {
            let first = self.lower_value_expr(builder, expr);
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
        self.lower_value_expr(builder, operand)
    }

    /// Lowers an assignment.
    pub(super) fn lower_assign(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        lhs: &hir::Expr<'_>,
        rhs: ValueId,
    ) {
        match &lhs.kind {
            ExprKind::Ident(_) => {
                if let Some(var_id) = self.gcx.resolved_variable(lhs) {
                    let var = self.gcx.hir.variable(var_id);

                    // Check if it's a local variable stored in memory
                    if let Some(offset) = self.get_local_memory_offset(&var_id) {
                        if self.is_slice_slot_local(&var_id) {
                            self.store_slice_slot(builder, offset, rhs);
                            return;
                        }
                        let offset_val = self.local_memory_addr(builder, offset);
                        builder.mstore(offset_val, rhs);
                    } else if let Some(local) = self.locals.get_mut(&var_id) {
                        // Function parameter - update SSA mapping (shouldn't happen normally)
                        *local = rhs;
                    } else if let Some(&id) = self.immutable_ids.get(&var_id) {
                        builder.store_immutable(id, rhs);
                    } else if let Some(&location) = self.storage_locations.get(&var_id) {
                        let base_slot = location.slot;
                        let ty = self.gcx.type_of_hir_ty(&var.ty).peel_refs();
                        match ty.kind {
                            TyKind::Struct(struct_id) => {
                                // Recursively copy all fields (handles nested structs).
                                self.copy_memory_to_storage(builder, struct_id, base_slot, rhs, 0);
                            }
                            TyKind::Elementary(
                                hir::ElementaryType::String | hir::ElementaryType::Bytes,
                            ) => {
                                // `string`/`bytes` state variable: `rhs` is a memory
                                // `[length][data...]` pointer; encode it into the
                                // short/long storage form instead of storing the
                                // pointer word.
                                let slot = builder.imm_u256(base_slot);
                                self.copy_memory_bytes_to_storage(builder, slot, rhs);
                            }
                            TyKind::DynArray(elem) => {
                                let slot = builder.imm_u256(base_slot);
                                self.copy_memory_dyn_array_to_storage(builder, slot, rhs, elem);
                            }
                            TyKind::Array(elem, len) => {
                                let slot = builder.imm_u256(base_slot);
                                self.copy_memory_fixed_array_to_storage(
                                    builder,
                                    slot,
                                    rhs,
                                    elem,
                                    len.to(),
                                );
                            }
                            _ => {
                                // Simple scalar storage assignment.
                                self.store_storage_location(builder, location, rhs);
                            }
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
                    let slot = base_slot + U256::from(field_offset);
                    let slot_val = builder.imm_u256(slot);
                    builder.sstore(slot_val, rhs);
                    return;
                }

                // Check if this is a nested storage struct assignment (e.g., storedNested.point.x =
                // value)
                if let Some(slot) = self.compute_nested_storage_slot(base, *member) {
                    let slot_val = builder.imm_u256(slot);
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
                    let base_val = self.lower_value_expr(builder, base);
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
                    let base_val = self.lower_value_expr(builder, base);
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
                let base_val = self.lower_value_expr(builder, base);
                builder.mstore(base_val, rhs);
            }
            ExprKind::Call(..) => {
                if let Some(slot) = self.lower_lvalue_slot(builder, lhs)
                    && let Some(ty) = self.get_expr_type(lhs)
                {
                    self.store_storage_value_at(builder, ty, slot, rhs);
                }
            }
            ExprKind::YulMember(base, member) => {
                // `r.slot := x` sets the storage pointer's slot value. The pointer
                // is marked as a storage ref so later `r.field` access resolves to
                // `sload`/`sstore(slot + off)`.
                if member.name == sym::slot
                    && let Some(var_id) = self.gcx.resolved_variable(base)
                {
                    if let Some(offset) = self.get_local_memory_offset(&var_id) {
                        let addr = self.local_memory_addr(builder, offset);
                        builder.mstore(addr, rhs);
                    } else {
                        self.locals.insert(var_id, rhs);
                    }
                    self.storage_ref_locals.insert(var_id);
                    return;
                }
                // `d.offset := x` / `d.length := x` on a `bytes`/`string` calldata
                // slice rewrites one component of its `(offset, length)` pair.
                // This is the `bytes calldata` empty/sub-slice idiom
                // (`data.length := 0`) used to build calldata slices in assembly.
                if matches!(member.name, sym::offset | sym::length)
                    && let Some(var_id) = self.gcx.resolved_variable(base)
                    && Self::calldata_dynamic_var_kind(self.gcx.hir.variable(var_id)).is_some()
                {
                    // A reassignable slice lives in a two-word slot so component
                    // writes merge across control flow. A straight-line slice
                    // remains in `locals` and is reconstructed below.
                    if self.is_slice_slot_local(&var_id)
                        && let Some(offset) = self.get_local_memory_offset(&var_id)
                    {
                        let offset = if member.name == sym::length {
                            offset + EvmMemoryLayout::WORD_SIZE
                        } else {
                            offset
                        };
                        let addr = self.local_memory_addr(builder, offset);
                        builder.mstore(addr, rhs);
                        return;
                    }
                    // An uninitialized calldata slice has the empty `(0, 0)`
                    // default, so the untouched component is zero when there is
                    // no current slice to project.
                    let current = self
                        .locals
                        .get(&var_id)
                        .copied()
                        .filter(|&slice| Self::value_is_calldata_slice(builder, slice));
                    let ptr = if member.name == sym::offset {
                        rhs
                    } else if let Some(current) = current {
                        builder.slice_ptr(current)
                    } else {
                        builder.imm_u64(0)
                    };
                    let len = if member.name == sym::length {
                        rhs
                    } else if let Some(current) = current {
                        builder.slice_len(current)
                    } else {
                        builder.imm_u64(0)
                    };
                    let slice = builder.make_slice(ptr, len, crate::mir::SliceLocation::Calldata);
                    self.locals.insert(var_id, slice);
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
            hir::TypeKind::Custom(hir::ItemId::Enum(enum_id)) => {
                let variant_count = self.gcx.hir.enumm(*enum_id).variants.len();
                self.emit_enum_range_check(builder, value, variant_count);
                value
            }
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
            ElementaryType::Address(_) => {
                self.mask_to_bits(builder, value, TypeSize::new_int_bits(160))
            }
            ElementaryType::UInt(size) => self.mask_to_bits(builder, value, *size),
            ElementaryType::Int(size) => self.sign_extend_to_bits(builder, value, *size),
            ElementaryType::FixedBytes(size) => {
                // `bytesN(someBytesSlice)` takes the slice's leading word,
                // which is already left-aligned like every fixed-bytes value.
                // Without this the slice value itself would be shifted as if it
                // were a number, and the slice would survive to the backend.
                if let Some(word) = self.slice_leading_word(builder, value) {
                    return self.clean_fixed_bytes(builder, word, *size);
                }
                if self.expr_is_fixed_bytes(source) {
                    self.clean_fixed_bytes(builder, value, *size)
                } else {
                    self.shift_numeric_to_fixed_bytes(builder, value, *size)
                }
            }
            ElementaryType::String
            | ElementaryType::Bytes
            | ElementaryType::Fixed(_, _)
            | ElementaryType::UFixed(_, _) => value,
        }
    }

    /// The first data word of a lowered slice value, read from wherever the
    /// slice lives. `None` when the value is not a slice.
    fn slice_leading_word(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        value: ValueId,
    ) -> Option<ValueId> {
        let calldata = Self::value_is_calldata_slice(builder, value);
        if !calldata && !Self::value_is_memory_slice(builder, value) {
            return None;
        }
        let ptr = builder.slice_ptr(value);
        Some(if calldata { builder.calldataload(ptr) } else { builder.mload(ptr) })
    }

    fn shift_numeric_to_fixed_bytes(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        value: ValueId,
        size: TypeSize,
    ) -> ValueId {
        let bytes = size.bytes();
        let shift_bits = u64::from(32 - bytes) * 8;
        let shifted = if shift_bits == 0 {
            value
        } else {
            let shift = builder.imm_u64(shift_bits);
            builder.shl(shift, value)
        };
        self.clean_fixed_bytes(builder, shifted, size)
    }

    pub(super) fn clean_fixed_bytes(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        value: ValueId,
        size: TypeSize,
    ) -> ValueId {
        let bytes = size.bytes();
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
        size: TypeSize,
    ) -> ValueId {
        let bits = size.bits();
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
        size: TypeSize,
    ) -> ValueId {
        let bits = size.bits();
        if bits == 0 || bits >= 256 {
            return value;
        }

        let shift = builder.imm_u64(u64::from(256 - bits));
        let shifted = builder.shl(shift, value);
        builder.sar(shift, shifted)
    }

    /// Returns the compile-time slot of a mapping state variable.
    fn get_mapping_base_slot(&self, expr: &hir::Expr<'_>) -> Option<U256> {
        let var_id = self.gcx.resolved_variable(expr)?;
        self.storage_slots.get(&var_id).copied()
    }

    /// Resolves an array living in storage and returns its element type and
    /// constant length (`None` for a dynamic array).
    fn storage_array_type_of_expr(&self, expr: &hir::Expr<'_>) -> Option<(Ty<'gcx>, Option<u64>)> {
        let ty = self.get_expr_type(expr)?;
        let TyKind::Ref(inner, solar_ast::DataLocation::Storage) = ty.kind else {
            return None;
        };
        match inner.kind {
            TyKind::Array(element, len) => Some((element, Some(u64::try_from(len).ok()?))),
            TyKind::DynArray(element) => Some((element, None)),
            _ => None,
        }
    }

    /// Resolves an array living in storage: a state variable, storage-reference
    /// local, mapping value, struct field, or nested array element. Returns its
    /// runtime base slot, constant length (`None` for dynamic arrays), and the
    /// number of storage slots occupied by one element.
    pub(super) fn storage_array_slot_of_base(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> Option<(ValueId, Option<u64>, u64)> {
        let (element, fixed_len) = self.storage_array_type_of_expr(expr)?;
        let elem_slots = self.calculate_storage_slots_for_ty(element, expr.span);
        let slot = self.lower_lvalue_slot(builder, expr)?;
        Some((slot, fixed_len, elem_slots))
    }

    /// Resolves a dynamic array living in storage.
    pub(super) fn storage_dynamic_array_info(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> Option<(ValueId, Ty<'gcx>, u64)> {
        let (element, fixed_len) = self.storage_array_type_of_expr(expr)?;
        if fixed_len.is_some() {
            return None;
        }
        let element_slots = self.calculate_storage_slots_for_ty(element, expr.span);
        let slot = self.lower_lvalue_slot(builder, expr)?;
        Some((slot, element, element_slots))
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
        matches!(self.gcx.resolved_builtin(expr), Some(Builtin::MsgData))
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
            let slice = self.lower_value_expr(builder, base);
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
        let value = self.lower_value_expr(builder, base);
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
        let var_id = self.gcx.resolved_variable(expr)?;
        let var = self.gcx.hir.variable(var_id);
        if var.data_location != Some(solar_ast::DataLocation::Calldata) {
            return None;
        }
        let is_bytes = Self::calldata_dynamic_var_kind(var)?;
        if self.is_slice_slot_local(&var_id) {
            let offset = self.get_local_memory_offset(&var_id)?;
            let slice = self.load_slice_slot(builder, offset, crate::mir::SliceLocation::Calldata);
            return Some((slice, is_bytes));
        }
        let slice = self.locals.get(&var_id).copied()?;
        // A calldata-located declaration does not guarantee a calldata slice
        // value. Inlining binds the callee's parameters to the caller's
        // argument values, and a calldata struct's dynamic member is rebuilt in
        // memory by the prologue, so a `T[] calldata` parameter can be bound to
        // a memory object. Projecting a slice off that would read a pointer as
        // if it were a `(ptr, len)` pair; let callers take the memory path.
        Self::value_is_calldata_slice(builder, slice).then_some((slice, is_bytes))
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
        // Use the variable's declared type directly; `get_expr_type` may not resolve every local.
        if let Some(var_id) = self.gcx.resolved_variable(expr) {
            let var = self.gcx.hir.variable(var_id);
            if let hir::TypeKind::Array(arr) = &var.ty.kind {
                arr.size.as_ref()?;
                if let solar_sema::ty::TyKind::Array(_, len) =
                    self.gcx.type_of_item(var_id.into()).peel_refs().kind
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
        let arg_exprs =
            match self.ordered_args_for(args, Some(CallableParamSource::Struct(struct_id))) {
                Ok(exprs) => exprs,
                Err(guar) => return builder.error_value(guar),
            };

        // Store each argument into the corresponding field
        for (i, (arg, &field_ty)) in arg_exprs.into_iter().zip(&field_tys).enumerate() {
            // Memory struct fields hold memory values. Calldata reference
            // values therefore materialize recursively before storing their
            // pointer in the field slot.
            let field_val = self.lower_return_value_for_ty(builder, arg, field_ty);
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
        self.allocate_memory_object_with_semantics(
            builder,
            size,
            kind,
            crate::mir::AllocationSemantics::INTERNAL,
        )
    }

    /// Allocates a zero-initialized shaped memory object with a constant byte size.
    pub(super) fn allocate_zeroed_memory_object(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        size: u64,
        kind: crate::mir::MemoryObjectKind,
    ) -> ValueId {
        let ptr = self.allocate_memory_object(builder, size, kind);
        let size = builder.imm_u64(size);
        builder.memory_zero(ptr, size);
        ptr
    }

    fn allocate_memory_object_with_semantics(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        size: u64,
        kind: crate::mir::MemoryObjectKind,
        semantics: crate::mir::AllocationSemantics,
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
        builder.alloc_object(size, layout, semantics)
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
        data: &hir::Expr<'_>,
        types: &hir::Expr<'_>,
        span: Span,
    ) -> Result<ValueId, ErrorGuaranteed> {
        let tys = self.abi_decode_tuple_tys(types, span)?;

        // The decode logic below expects a memory `[length][data...]` pointer.
        // Calldata values and subslices carry `(ptr, len)` explicitly and are
        // copied only at this memory-consuming boundary.
        let ptr = if self.expr_is_calldata_dynamic_bytes(data) {
            let value = self.lower_value_expr(builder, data);
            // A decoded calldata-struct member is already a memory bytes
            // pointer despite its calldata-located type.
            if Self::value_is_calldata_slice(builder, value) {
                self.materialize_calldata_bytes(builder, value)
            } else {
                value
            }
        } else {
            self.lower_value_expr(builder, data)
        };
        let len = builder.memory_object_len(ptr, MemoryObjectKind::Bytes);
        let data_start = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);

        let decoded_values = self.decode_abi_region(builder, data_start, len, &tys);
        self.stage_multi_return_tail(builder, &decoded_values);
        decoded_values.first().copied().ok_or_else(|| {
            self.gcx.dcx().err("`abi.decode` must decode at least one value").span(span).emit()
        })
    }

    /// Decodes an ABI tuple `(T...)` from the memory region `[data_start,
    /// data_start + len)` into one memory value per member. The region base is
    /// the tuple's head base: static members occupy their head bytes inline;
    /// dynamic members store an offset (relative to `data_start`) to their
    /// tail. Reverts, like solc, when the region is too short for the heads.
    pub(super) fn decode_abi_region(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        data_start: ValueId,
        len: ValueId,
        tys: &[Ty<'gcx>],
    ) -> Vec<ValueId> {
        let head_size = match self.abi_head_size_sum(tys.iter().copied()) {
            Ok(size) => size,
            Err(guar) => {
                let err = builder.error_value(guar);
                return vec![err; tys.len()];
            }
        };
        let required = builder.imm_u64(head_size);
        let is_short = builder.lt(len, required);
        self.emit_abi_decode_revert_if(builder, is_short);

        let head_size_val = builder.imm_u64(head_size);
        let mut out = Vec::with_capacity(tys.len());
        let mut head_offset = 0u64;
        for &ty in tys {
            let head_pos = self.offset_ptr(builder, data_start, head_offset);
            let Some(strategy) = self.abi_decode_strategy(ty) else {
                let guar = self.recovery_error(None, "codegen cannot decode this ABI member type");
                let err = builder.error_value(guar);
                return vec![err; tys.len()];
            };
            let decoded = match strategy {
                DecodeStrategy::Word(elem) => {
                    let word = builder.mload(head_pos);
                    self.lower_abi_decode_word(builder, &elem, word)
                }
                DecodeStrategy::DynBytes => {
                    let head = builder.mload(head_pos);
                    self.lower_abi_decode_dynamic_bytes(
                        builder,
                        data_start,
                        len,
                        head_size_val,
                        head,
                    )
                }
                DecodeStrategy::ElementaryArray(elem) => {
                    let head = builder.mload(head_pos);
                    self.lower_abi_decode_dyn_array(
                        builder, data_start, len, head_size, &elem, head,
                    )
                }
                DecodeStrategy::General(ty) => {
                    // Resolve the member's body position, then decode it with
                    // the same recursive materializer that decodes calldata
                    // struct-array parameters.
                    let pos = if self.abi_is_dynamic(ty) {
                        let offset = builder.mload(head_pos);
                        builder.add(data_start, offset)
                    } else {
                        head_pos
                    };
                    self.materialize_calldata_value_at(
                        builder,
                        super::bytes::AbiSource::Memory,
                        ty,
                        pos,
                    )
                }
            };
            out.push(decoded);
            let ty_size = match self.abi_head_size(ty) {
                Ok(size) => size,
                Err(guar) => {
                    let err = builder.error_value(guar);
                    return vec![err; tys.len()];
                }
            };
            head_offset += ty_size;
        }
        out
    }

    /// The decode strategy for an ABI member type, or `None` when codegen
    /// cannot decode it.
    pub(super) fn abi_decode_strategy(&self, ty: Ty<'gcx>) -> Option<DecodeStrategy<'gcx>> {
        let peeled = ty.peel_refs();
        match peeled.kind {
            TyKind::Udvt(inner, _) => self.abi_decode_strategy(inner),
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                Some(DecodeStrategy::DynBytes)
            }
            TyKind::Elementary(elem) => Some(DecodeStrategy::Word(elem)),
            TyKind::DynArray(elem) => match elem.peel_refs().kind {
                TyKind::Elementary(elem) => Some(DecodeStrategy::ElementaryArray(elem)),
                _ => self
                    .abi_type(peeled, false)
                    .is_some()
                    .then_some(DecodeStrategy::General(peeled)),
            },
            _ => self.abi_type(peeled, false).is_some().then_some(DecodeStrategy::General(peeled)),
        }
    }

    /// The sema types of an `abi.decode(data, (T...))` target tuple, reporting
    /// any member codegen cannot decode.
    fn abi_decode_tuple_tys(
        &self,
        types: &hir::Expr<'_>,
        span: Span,
    ) -> Result<Vec<Ty<'gcx>>, ErrorGuaranteed> {
        let unsupported = |span: Span| {
            self.gcx
                .dcx()
                .err("codegen does not support this `abi.decode` target type yet")
                .span(span)
                .emit()
        };
        let ExprKind::Tuple(elems) = &types.kind else {
            return Err(unsupported(span));
        };

        let mut out = Vec::with_capacity(elems.len());
        for elem in elems.iter().copied() {
            let Some(elem_expr) = elem else {
                return Err(unsupported(span));
            };
            // Type checking records each tuple component's resolved type as
            // `Type(inner)` on the expression, regardless of whether it is
            // written as a builtin type keyword, a user type name, or an
            // array of one.
            let Some(TyKind::Type(sema_ty)) = self.get_expr_type(elem_expr).map(|ty| ty.kind)
            else {
                return Err(unsupported(elem_expr.span));
            };
            let sema_ty = sema_ty.with_loc_if_ref(self.gcx, solar_ast::DataLocation::Memory);
            if self.abi_decode_strategy(sema_ty).is_none() {
                return Err(unsupported(elem_expr.span));
            }
            out.push(sema_ty);
        }
        Ok(out)
    }

    /// Decodes one dynamic-array member of an `abi.decode` tuple: validates
    /// the head offset and the array bounds against the encoded region, then
    /// copies the payload into a fresh memory array. Word elements copy in
    /// bulk with a per-element cleanliness check where the type requires one;
    /// `bytes`/`string` elements decode each element like a dynamic-bytes
    /// tuple member against the array's own data region.
    pub(super) fn lower_abi_decode_dyn_array(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        tuple_base: ValueId,
        tuple_len: ValueId,
        head_size: u64,
        elem: &ElementaryType,
        head: ValueId,
    ) -> ValueId {
        // The head word is an offset to `[length][payload...]`; it must land
        // after the head area and its length word must be in bounds.
        let head_size_val = builder.imm_u64(head_size);
        let head_before_tail = builder.lt(head, head_size_val);
        self.emit_abi_decode_revert_if(builder, head_before_tail);
        let word = builder.imm_u64(32);
        let tail_head_end = builder.add(head, word);
        let head_overflow = builder.lt(tail_head_end, head);
        self.emit_abi_decode_revert_if(builder, head_overflow);
        let head_oob = builder.gt(tail_head_end, tuple_len);
        self.emit_abi_decode_revert_if(builder, head_oob);

        let len_addr = builder.add(tuple_base, head);
        let arr_len = builder.mload(len_addr);
        // Guard `len * 32` before it can wrap.
        let shift = builder.imm_u64(250);
        let shifted = builder.shr(shift, arr_len);
        let in_range = builder.iszero(shifted);
        let too_big = builder.iszero(in_range);
        self.emit_abi_decode_revert_if(builder, too_big);
        let payload_bytes = builder.mul(arr_len, word);
        let payload_end = builder.add(tail_head_end, payload_bytes);
        let payload_oob = builder.gt(payload_end, tuple_len);
        self.emit_abi_decode_revert_if(builder, payload_oob);

        let total_size = builder.add(word, payload_bytes);
        let payload_src = builder.add(len_addr, word);

        if matches!(elem, ElementaryType::Bytes | ElementaryType::String) {
            // The payload is a head area of per-element offsets relative to
            // the array's own data region; decode each element against that
            // region into a fresh array of pointers.
            let region_base = payload_src;
            let region_len = builder.sub(tuple_len, tail_head_end);
            let ptr = self.allocate_memory_object_dynamic(
                builder,
                total_size,
                MemoryObjectKind::DynamicArray,
            );
            builder.set_memory_object_len(ptr, arr_len, MemoryObjectKind::DynamicArray);
            let dst_data = builder.memory_object_data(ptr, MemoryObjectKind::DynamicArray);
            self.emit_decode_elements_loop(builder, arr_len, |this, builder, index| {
                let offset = builder.mul(index, word);
                let head_addr = builder.add(region_base, offset);
                let elem_head = builder.mload(head_addr);
                let elem_ptr = this.lower_abi_decode_dynamic_bytes(
                    builder,
                    region_base,
                    region_len,
                    payload_bytes,
                    elem_head,
                );
                let dst_addr = builder.add(dst_data, offset);
                builder.mstore(dst_addr, elem_ptr);
            });
            return ptr;
        }

        // Word elements: bulk copy, then a cleanliness sweep where the
        // element type does not span the full word.
        let ptr = self.allocate_memory_object_dynamic(
            builder,
            total_size,
            MemoryObjectKind::DynamicArray,
        );
        builder.set_memory_object_len(ptr, arr_len, MemoryObjectKind::DynamicArray);
        let dst_data = builder.memory_object_data(ptr, MemoryObjectKind::DynamicArray);
        builder.mcopy(dst_data, payload_src, payload_bytes);

        let needs_validation = !matches!(
            elem,
            ElementaryType::UInt(size) if size.bits() == 256
        ) && !matches!(elem, ElementaryType::Int(size) if size.bits() == 256)
            && !matches!(elem, ElementaryType::FixedBytes(size) if size.bytes() == 32);
        if needs_validation {
            let elem = *elem;
            self.emit_decode_elements_loop(builder, arr_len, |this, builder, index| {
                let offset = builder.mul(index, word);
                let addr = builder.add(dst_data, offset);
                let value = builder.mload(addr);
                let _ = this.lower_abi_decode_word(builder, &elem, value);
            });
        }
        ptr
    }

    /// Emits a `for index in 0..len` loop around `body`; the builder ends up
    /// in the exit block.
    pub(super) fn emit_decode_elements_loop(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        len: ValueId,
        body: impl FnOnce(&mut Self, &mut FunctionBuilder<'_>, ValueId) + Copy,
    ) {
        let preheader = builder.current_block();
        let header = builder.create_block();
        let body_block = builder.create_block();
        let exit = builder.create_block();
        let zero = builder.imm_u64(0);
        builder.jump(header);

        builder.switch_to_block(header);
        let index_phi = builder.phi(vec![(preheader, zero)]);
        let has_more = builder.lt(index_phi, len);
        builder.branch(has_more, body_block, exit);

        builder.switch_to_block(body_block);
        body(self, builder, index_phi);
        let one = builder.imm_u64(1);
        let index_next = builder.add(index_phi, one);
        let latch = builder.current_block();
        builder.jump(header);
        builder.add_phi_incoming(index_phi, latch, index_next);

        builder.switch_to_block(exit);
    }

    pub(super) fn lower_abi_decode_dynamic_bytes(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        tuple_base: ValueId,
        tuple_len: ValueId,
        head_size: ValueId,
        head: ValueId,
    ) -> ValueId {
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
        builder.mcopy(data_ptr, src, tail_len);
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

    pub(super) fn lower_abi_decode_word(
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
            ElementaryType::Address(_) => {
                self.mask_to_bits(builder, value, TypeSize::new_int_bits(160))
            }
            ElementaryType::UInt(size) => self.mask_to_bits(builder, value, *size),
            ElementaryType::Int(size) => self.sign_extend_to_bits(builder, value, *size),
            ElementaryType::FixedBytes(size) => self.clean_fixed_bytes(builder, value, *size),
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
    ) -> Option<(U256, hir::StructId, usize)> {
        if let Some(var_id) = self.gcx.resolved_variable(base) {
            let var = self.gcx.hir.variable(var_id);
            // Check if the variable has a struct type and is stored in storage
            if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind
                && let Some(&base_slot) = self.struct_storage_base_slots.get(&var_id)
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
        if let Some(var_id) = self.gcx.resolved_variable(base)
            && self.storage_ref_locals.contains(var_id)
            && let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) =
                &self.gcx.hir.variable(var_id).ty.kind
        {
            let strukt = self.gcx.hir.strukt(*struct_id);
            for (i, &field_id) in strukt.fields.iter().enumerate() {
                let field = self.gcx.hir.variable(field_id);
                if let Some(field_name) = field.name
                    && field_name.name == member.name
                {
                    return Some((var_id, *struct_id, i));
                }
            }
        }
        None
    }

    /// Resolves the struct type of an expression, for storage struct field
    /// access. Uses the variable's declared type when available and the inferred
    /// expression type otherwise (e.g. a mapping/array element).
    pub(super) fn struct_id_of_expr(&self, expr: &hir::Expr<'_>) -> Option<hir::StructId> {
        if let Some(vid) = self.gcx.resolved_variable(expr)
            && let hir::TypeKind::Custom(hir::ItemId::Struct(sid)) =
                &self.gcx.hir.variable(vid).ty.kind
        {
            return Some(*sid);
        }
        // Indexed element (`items[k]`, `arr[i]`): the mapping value / array
        // element type, resolved from the indexed variable's declared type.
        if let ExprKind::Index(arr, _) = &expr.kind
            && let Some(vid) = self.gcx.resolved_variable(arr)
        {
            let elem_kind = match &self.gcx.hir.variable(vid).ty.kind {
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
            && let Some(fid) = self.gcx.resolved_function(callee)
            && let Some(&rid) = self.gcx.hir.function(fid).returns.first()
            && let hir::TypeKind::Custom(hir::ItemId::Struct(sid)) =
                &self.gcx.hir.variable(rid).ty.kind
        {
            return Some(*sid);
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
            ExprKind::Ident(_) => {
                if let Some(var_id) = self.gcx.resolved_variable(expr) {
                    // Another storage reference: its value is already the slot.
                    if self.storage_ref_locals.contains(var_id) {
                        return self.load_storage_ref_slot(builder, var_id);
                    }
                    // A state variable: its base slot is known at compile time.
                    if let Some(&slot) = self.storage_slots.get(&var_id) {
                        return Some(builder.imm_u256(slot));
                    }
                    if let Some(&slot) = self.struct_storage_base_slots.get(&var_id) {
                        return Some(builder.imm_u256(slot));
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
                    return Some(builder.imm_u256(base_slot + U256::from(field_offset)));
                }
                // Storage-reference local struct field.
                if let Some((var_id, struct_id, field_index)) =
                    self.get_storage_ref_struct_field_info(base, *member)
                {
                    let field_offset = self.get_struct_field_slot_offset(struct_id, field_index);
                    let base_slot = self.load_storage_ref_slot(builder, var_id)?;
                    return Some(if field_offset == 0 {
                        base_slot
                    } else {
                        let off = builder.imm_u64(field_offset);
                        builder.add(base_slot, off)
                    });
                }
                // Nested state-variable storage struct field.
                if let Some(slot) = self.compute_nested_storage_slot(base, *member) {
                    return Some(builder.imm_u256(slot));
                }
                None
            }
            ExprKind::Call(callee, args, _)
                if self.gcx.resolved_builtin(callee) == Some(Builtin::ArrayPush0) =>
            {
                match self.builtin_args(Builtin::ArrayPush0, args) {
                    Ok([]) => {}
                    Err(guar) => return Some(builder.error_value(guar)),
                }
                let ExprKind::Member(base, _) = &callee.kind else { return None };
                let (slot, element_ty, element_slots) =
                    self.storage_dynamic_array_info(builder, base)?;
                Some(self.lower_storage_array_push_slot(builder, slot, element_ty, element_slots))
            }
            // A call to a function returning a storage reference (e.g. the
            // ERC-7201 `_layout()` getter) yields the slot value directly.
            ExprKind::Call(callee, ..) if self.call_returns_storage_ref(callee) => {
                Some(self.lower_value_expr(builder, expr))
            }
            // A parenthesized lvalue, `(m[k])`, reaches HIR as a one-element
            // tuple. Parentheses do not change what is being addressed, so
            // resolve the slot of the inner expression.
            ExprKind::Tuple([Some(inner)]) => self.lower_lvalue_slot(builder, inner),
            _ => None,
        }
    }

    fn load_storage_ref_slot(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        var_id: hir::VariableId,
    ) -> Option<ValueId> {
        if let Some(&slot) = self.locals.get(&var_id) {
            return Some(slot);
        }
        let offset = self.get_local_memory_offset(&var_id)?;
        let addr = self.local_memory_addr(builder, offset);
        Some(builder.mload(addr))
    }

    /// Whether `callee` resolves to a function whose first return is a storage
    /// reference, so a call to it yields a storage slot value.
    fn call_returns_storage_ref(&self, callee: &hir::Expr<'_>) -> bool {
        let Some(fid) = self.gcx.resolved_function(callee) else {
            return false;
        };
        self.gcx.hir.function(fid).returns.first().is_some_and(|&rid| {
            self.gcx.hir.variable(rid).data_location == Some(solar_ast::DataLocation::Storage)
        })
    }

    /// Checks if a member access is on a memory struct.
    /// Returns (struct_id, field_index) if the base expression is a memory struct.
    pub(super) fn get_memory_struct_field_info(
        &self,
        base: &hir::Expr<'_>,
        member: Ident,
    ) -> Option<(hir::StructId, usize)> {
        if let Some(var_id) = self.gcx.resolved_variable(base) {
            let var = self.gcx.hir.variable(var_id);
            if var.is_local_variable()
                && let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind
            {
                // For memory structs, we need to verify this is NOT a storage struct
                if !self.struct_storage_base_slots.contains_key(&var_id) {
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
    ) -> Option<(U256, Option<hir::StructId>)> {
        if let ExprKind::Member(base, member) = &expr.kind {
            // First try: base is a direct storage struct variable
            if let Some((base_slot, struct_id, field_index)) =
                self.get_storage_struct_field_info(base, *member)
            {
                let field_offset = self.get_struct_field_slot_offset(struct_id, field_index);
                let slot = base_slot + U256::from(field_offset);

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
                        let slot = parent_slot + U256::from(field_offset);

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
    fn compute_nested_storage_slot(&mut self, base: &hir::Expr<'_>, member: Ident) -> Option<U256> {
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
                        return Some(parent_slot + U256::from(field_offset));
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
                        let nested_base_slot = base_slot + U256::from(inner_field_offset);

                        let inner_strukt = self.gcx.hir.strukt(*inner_struct_id);
                        for (i, &inner_field_id) in inner_strukt.fields.iter().enumerate() {
                            let inner_field = self.gcx.hir.variable(inner_field_id);
                            if let Some(field_name) = inner_field.name
                                && field_name.name == member.name
                            {
                                let inner_offset =
                                    self.get_struct_field_slot_offset(*inner_struct_id, i);
                                return Some(nested_base_slot + U256::from(inner_offset));
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Appends a zero-initialized element and returns its storage slot.
    fn lower_storage_array_push_slot(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        slot: ValueId,
        element_ty: Ty<'gcx>,
        element_slots: u64,
    ) -> ValueId {
        let length = builder.sload(slot);
        let one = builder.imm_u64(1);
        let new_length = builder.add(length, one);
        let overflow = builder.lt(new_length, length);
        self.emit_panic_if(builder, overflow, PanicCode::MemoryAllocationOverflow);

        let scratch = builder.imm_u64(0);
        builder.mstore(scratch, slot);
        let word = builder.imm_u64(32);
        let data_slot = builder.keccak256(scratch, word);
        let offset = Self::scale_index_by_slots(builder, length, element_slots);
        let element_slot = builder.add(data_slot, offset);

        self.clear_storage_value_at(builder, element_ty, element_slot);
        builder.sstore(slot, new_length);
        element_slot
    }

    /// Lowers dynamic storage-array method calls.
    pub(super) fn lower_array_method_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        array: (ValueId, Ty<'gcx>, u64),
        builtin: Builtin,
        args: &CallArgs<'_>,
    ) -> Option<ValueId> {
        let method = match self.storage_array_method(builtin, args) {
            Ok(method) => method,
            Err(guar) => {
                return (builtin == Builtin::ArrayPush0).then(|| builder.error_value(guar));
            }
        };
        let (slot, element_ty, element_slots) = array;
        match method {
            StorageArrayMethod::PushDefault => {
                let element_slot =
                    self.lower_storage_array_push_slot(builder, slot, element_ty, element_slots);
                if element_ty.is_reference_type() {
                    Some(element_slot)
                } else {
                    // The storage reference returned by `push()` reads as the
                    // newly zero-initialized scalar when used as an rvalue.
                    Some(builder.imm_u64(0))
                }
            }
            StorageArrayMethod::Push(arg) => {
                let value = match element_ty.peel_refs().kind {
                    TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                        self.lower_expr_as_memory_bytes(builder, arg)
                    }
                    TyKind::DynArray(_) => self.lower_expr_as_memory_dyn_array(builder, arg),
                    _ => self.lower_value_expr(builder, arg),
                };

                let length = builder.sload(slot);
                let one = builder.imm_u64(1);
                let new_length = builder.add(length, one);
                let overflow = builder.lt(new_length, length);
                self.emit_panic_if(builder, overflow, PanicCode::MemoryAllocationOverflow);

                let scratch = builder.imm_u64(0);
                builder.mstore(scratch, slot);
                let word = builder.imm_u64(32);
                let data_slot = builder.keccak256(scratch, word);
                let offset = Self::scale_index_by_slots(builder, length, element_slots);
                let element_slot = builder.add(data_slot, offset);
                self.store_storage_value_at(builder, element_ty, element_slot, value);
                builder.sstore(slot, new_length);
                None
            }
            StorageArrayMethod::Pop => {
                let length = builder.sload(slot);
                self.emit_panic_if_zero(builder, length, PanicCode::PopEmptyArray);
                let one = builder.imm_u64(1);
                let new_length = builder.sub(length, one);

                let scratch = builder.imm_u64(0);
                builder.mstore(scratch, slot);
                let word = builder.imm_u64(32);
                let data_slot = builder.keccak256(scratch, word);
                let offset = Self::scale_index_by_slots(builder, new_length, element_slots);
                let element_slot = builder.add(data_slot, offset);
                self.clear_storage_value_at(builder, element_ty, element_slot);
                builder.sstore(slot, new_length);

                None
            }
        }
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
        let (key_is_dynamic, value_is_mapping) = self.mapping_type_info(base)?;

        // Mapping state variable: base slot is a compile-time constant.
        let base_slot = if let Some(slot) = self.get_mapping_base_slot(base) {
            MappingBaseSlot::Const(slot)
        } else if let Some(slot) = self.mapping_ref_base_slot_value(base) {
            // A mapping storage-reference parameter/local already holds its
            // runtime base slot.
            MappingBaseSlot::Value(slot)
        } else {
            // Mapping-valued struct fields, calls, and preceding mapping
            // indexes expose their runtime slot through the generic lvalue
            // path.
            MappingBaseSlot::Value(self.lower_lvalue_slot(builder, base)?)
        };
        Some(self.finish_mapping_element_slot(
            builder,
            base.span,
            base_slot,
            index,
            key_is_dynamic,
            value_is_mapping,
        ))
    }

    /// Given the (already resolved) base slot of a mapping and an index, computes
    /// the element's storage slot.
    fn finish_mapping_element_slot(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        base_span: Span,
        base_slot: MappingBaseSlot,
        index: Option<&hir::Expr<'_>>,
        key_is_dynamic: bool,
        value_is_mapping: bool,
    ) -> MappingElementSlot {
        let index_val = self.lower_index_value(builder, base_span, index);
        // Materialize the base slot after the index so a constant state-variable
        // slot keeps its original emission order (the index is lowered first).
        let slot_val = match base_slot {
            MappingBaseSlot::Const(slot) => builder.imm_u256(slot),
            MappingBaseSlot::Value(val) => val,
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

    /// Returns the runtime slot held by a mapping storage-reference parameter
    /// or local.
    fn mapping_ref_base_slot_value(&self, base: &hir::Expr<'_>) -> Option<ValueId> {
        let var_id = self.gcx.resolved_variable(base)?;
        self.locals.get(&var_id).copied()
    }

    /// Returns the key and value shape of the mapping represented by `expr`.
    fn mapping_type_info(&self, expr: &hir::Expr<'_>) -> Option<(bool, bool)> {
        let ty = self.get_expr_type(expr)?.peel_refs();
        let TyKind::Mapping(key, value) = ty.kind else { return None };
        Some((
            Self::is_dynamic_mapping_key_ty(key),
            matches!(value.peel_refs().kind, TyKind::Mapping(..)),
        ))
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
            // A calldata slice reaches here from every slicing form, not only a
            // directly named parameter, so decide from the lowered value. The
            // expression check below still covers the forms whose value is not
            // a slice.
            if Self::value_is_calldata_slice(builder, key) {
                return self.compute_dynamic_calldata_mapping_slot(builder, key, slot);
            }
            // A memory slice — a sub-slice of a calldata struct's member, which
            // the prologue rebuilt in memory — hashes where it already is.
            if Self::value_is_memory_slice(builder, key) {
                let ptr = self.materialize_memory_slice_bytes(builder, key);
                return self.compute_dynamic_memory_mapping_slot(builder, ptr, slot);
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
        if let Some(var_id) = self.gcx.resolved_variable(expr)
            && self.storage_ref_locals.contains(var_id)
        {
            let var = self.gcx.hir.variable(var_id);
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
        builder.mcopy(scratch, data_start, len);
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

    fn is_dynamic_mapping_key_ty(ty: Ty<'_>) -> bool {
        matches!(
            ty.peel_refs().kind,
            TyKind::Elementary(ElementaryType::String | ElementaryType::Bytes)
        )
    }

    fn is_dynamic_calldata_arg(&self, expr: Option<&hir::Expr<'_>>) -> bool {
        let Some(expr) = expr else {
            return false;
        };
        let Some(var_id) = self.gcx.resolved_variable(expr) else {
            return false;
        };
        if !self.locals.contains_key(&var_id) || self.get_local_memory_offset(&var_id).is_some() {
            return false;
        }
        let var = self.gcx.hir.variable(var_id);
        if var.data_location != Some(solar_ast::DataLocation::Calldata) {
            return false;
        }
        Self::is_dynamic_mapping_key(&var.ty.kind)
    }
}
