//! Expression lowering.

use super::{
    Lowerer,
    checked_arith::{ArithmeticInfo, PanicCode},
};
use crate::{
    mir::{FunctionBuilder, MirType, ValueId},
    utils::{ConstantFolder, FoldResult},
};
use alloy_primitives::{U256, keccak256};
use solar_ast::{LitKind, StrKind};
use solar_data_structures::map::FxHashSet;
use solar_interface::{Ident, Span, Symbol, kw, sym};
use solar_sema::{
    builtins::Builtin,
    hir::{self, CallArgs, ElementaryType, ExprKind},
    ty::TyKind,
};

pub(super) struct MappingElementSlot {
    pub(super) slot: ValueId,
    pub(super) value_is_mapping: bool,
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
                if let Some(res) = res_slice.first() {
                    self.lower_ident(builder, res)
                } else {
                    builder.imm_u64(0)
                }
            }

            ExprKind::Binary(lhs, op, rhs) => {
                // Try constant folding first
                let folder = ConstantFolder::new(&self.gcx.hir);
                let int_info =
                    self.integer_info_for_expr(expr).or_else(|| self.integer_info_for_expr(lhs));
                let is_signed =
                    int_info.map_or_else(|| self.is_expr_signed(lhs), |info| info.signed);
                let unsupported_udvt_operator = int_info.is_none()
                    && !matches!(op.kind, hir::BinOpKind::Eq | hir::BinOpKind::Ne)
                    && (self.expr_has_udvt_type(expr)
                        || self.expr_has_udvt_type(lhs)
                        || self.expr_has_udvt_type(rhs));
                if !Self::signed_binary_fold_is_unsafe(op.kind, is_signed) {
                    if let Some(folded) = folder.fold_to_integer(expr) {
                        return builder.imm_u256(folded);
                    }
                    if let FoldResult::Bool(b) = folder.try_fold(expr) {
                        return builder.imm_bool(b);
                    }
                }

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
                        if int_info.is_none() && self.expr_has_udvt_type(operand) {
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
                        // Try constant folding for non-mutating unary ops
                        let folder = ConstantFolder::new(&self.gcx.hir);
                        if let Some(folded) = folder.fold_to_integer(expr) {
                            return builder.imm_u256(folded);
                        }
                        if let FoldResult::Bool(b) = folder.try_fold(expr) {
                            return builder.imm_bool(b);
                        }

                        let operand_val = self.lower_expr(builder, operand);
                        let int_info = self
                            .integer_info_for_expr(expr)
                            .or_else(|| self.integer_info_for_expr(operand));
                        if int_info.is_none()
                            && !matches!(op.kind, UnOpKind::Not)
                            && self.expr_has_udvt_type(operand)
                        {
                            self.emit_unsupported_udvt_operator(expr.span);
                            return operand_val;
                        }
                        self.lower_unary_op(builder, *op, operand_val, int_info, expr.span)
                    }
                }
            }

            ExprKind::Ternary(cond, then_expr, else_expr) => {
                self.lower_ternary(builder, cond, then_expr, else_expr)
            }

            ExprKind::Call(callee, args, call_opts) => {
                self.lower_call(builder, callee, args, (*call_opts).map(|opts| opts.args))
            }

            ExprKind::Index(base, index) => {
                self.lower_index_expr(builder, expr, base, index.as_deref())
            }

            ExprKind::Member(base, member) => {
                // Check if this is a builtin module member access (e.g., msg.sender,
                // block.timestamp)
                if let ExprKind::Ident(res_slice) = &base.kind
                    && let Some(hir::Res::Builtin(base_builtin)) = res_slice.first()
                    && let Some(member_builtin) =
                        self.resolve_builtin_member(*base_builtin, member.name)
                {
                    return self.lower_builtin(builder, member_builtin);
                }

                // Handle enum variant access (e.g., Status.Active)
                if let ExprKind::Ident(res_slice) = &base.kind
                    && let Some(hir::Res::Item(hir::ItemId::Enum(enum_id))) = res_slice.first()
                {
                    let enum_def = self.gcx.hir.enumm(*enum_id);
                    for (i, variant) in enum_def.variants.iter().enumerate() {
                        if variant.name == member.name {
                            return builder.imm_u64(i as u64);
                        }
                    }
                }

                // Handle contract/library constants (e.g. MachineLib.NO_RECOVERY_PC).
                if let ExprKind::Ident(res_slice) = &base.kind
                    && let Some(hir::Res::Item(hir::ItemId::Contract(contract_id))) =
                        res_slice.first()
                {
                    let contract = self.gcx.hir.contract(*contract_id);
                    for var_id in contract.variables() {
                        let var = self.gcx.hir.variable(var_id);
                        if var.is_constant()
                            && var.name.is_some_and(|name| name.name == member.name)
                            && let Some(init) = var.initializer
                        {
                            return self.lower_expr(builder, init);
                        }
                    }
                }

                // Handle nested enum variant access (e.g., Contract.Status.Active)
                // base is Member(Ident(Contract), enum_name)
                if let ExprKind::Member(contract_expr, enum_name) = &base.kind
                    && let ExprKind::Ident(res_slice) = &contract_expr.kind
                    && let Some(hir::Res::Item(hir::ItemId::Contract(contract_id))) =
                        res_slice.first()
                {
                    let contract = self.gcx.hir.contract(*contract_id);
                    for &item_id in contract.items {
                        if let hir::ItemId::Enum(enum_id) = item_id {
                            let enum_def = self.gcx.hir.enumm(enum_id);
                            if enum_def.name.name == enum_name.name {
                                for (i, variant) in enum_def.variants.iter().enumerate() {
                                    if variant.name == member.name {
                                        return builder.imm_u64(i as u64);
                                    }
                                }
                            }
                        }
                    }
                }

                // Handle type(T).min, type(T).max, type(T).creationCode, type(T).runtimeCode
                if let ExprKind::TypeCall(ty) = &base.kind {
                    let member_name = member.name.as_str();
                    match member_name {
                        "max" | "min" => {
                            return self.lower_type_minmax(builder, ty, member_name == "max");
                        }
                        "creationCode" => {
                            return self.lower_type_creation_code(builder, ty, true);
                        }
                        "runtimeCode" => {
                            return self.lower_type_creation_code(builder, ty, false);
                        }
                        _ => {}
                    }
                }

                // Handle dynamic array .length
                if member.name.as_str() == "length" {
                    // Storage array (state variable or storage-reference
                    // local): dynamic length at the base slot, fixed length
                    // is a compile-time constant.
                    if let Some((slot_val, fixed_len, _)) =
                        self.storage_array_slot_of_base(builder, base)
                    {
                        return match fixed_len {
                            Some(len) => builder.imm_u64(len),
                            None => builder.sload(slot_val),
                        };
                    }
                    // Calldata dynamic array/bytes: length word at `4 + head`.
                    if let Some((head, _)) = self.calldata_dyn_head(base) {
                        let four = builder.imm_u64(4);
                        let len_pos = builder.add(four, head);
                        return builder.calldataload(len_pos);
                    }
                    // Fixed-size arrays have a compile-time length.
                    if let Some(len) = self.fixed_array_len_of_expr(base) {
                        return builder.imm_u64(len);
                    }
                    // Memory dynamic arrays and bytes fall through to the
                    // generic member fallback, which loads the length word at
                    // the base pointer.
                }

                // Handle function selector member access: `this.foo.selector`.
                if member.name == sym::selector
                    && let ExprKind::Member(receiver, function_name) = &base.kind
                {
                    let selector = self.compute_member_selector(receiver, *function_name);
                    return builder.imm_u256(U256::from(selector) << 224);
                }

                // Handle address member access: addr.balance
                if member.name.as_str() == "balance" {
                    let addr = self.lower_expr(builder, base);
                    return builder.balance(addr);
                }

                // Check if this is a storage struct member access (e.g., storedPoint.x)
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

                // Check if this is a nested memory struct access (e.g., o.inner.a)
                if let Some((var_id, total_offset, _inner_struct_id)) =
                    self.compute_nested_memory_struct_info(base, *member)
                {
                    // Get the base pointer for the outermost struct
                    let base_ptr = if let Some(offset) = self.get_local_memory_offset(&var_id) {
                        // Variable is stored in local memory slot
                        let offset_val = self.local_memory_addr(builder, offset);
                        builder.mload(offset_val)
                    } else if let Some(&val) = self.locals.get(&var_id) {
                        // Variable is an SSA value (pointer to struct)
                        val
                    } else {
                        builder.imm_u64(0)
                    };

                    if total_offset == 0 {
                        return builder.mload(base_ptr);
                    }
                    let offset_val = builder.imm_u64(total_offset);
                    let field_addr = builder.add(base_ptr, offset_val);
                    return builder.mload(field_addr);
                }

                // Regular memory struct member access
                if let Some((struct_id, field_index)) =
                    self.get_memory_struct_field_info(base, *member)
                {
                    let base_val = self.lower_expr(builder, base);
                    let field_offset = self.get_struct_field_memory_offset(struct_id, field_index);
                    if field_offset == 0 {
                        return builder.mload(base_val);
                    }
                    let offset_val = builder.imm_u64(field_offset);
                    let field_addr = builder.add(base_val, offset_val);
                    return builder.mload(field_addr);
                }

                // Fallback: just load from base address
                let base_val = self.lower_expr(builder, base);
                builder.mload(base_val)
            }

            ExprKind::YulMember(base, member) => self.lower_yul_member(builder, base, *member),

            ExprKind::Assign(lhs, op, rhs) => {
                let rhs_val = if op.is_none() && self.lhs_expects_memory_bytes_value(lhs) {
                    self.lower_expr_as_memory_bytes(builder, rhs)
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
                    let unsupported_udvt_operator =
                        int_info.is_none() && self.expr_has_udvt_type(lhs);
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
                let ptr = self.allocate_memory(builder, alloc_size);
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
                // Deleting a memory fixed-size array zeroes its elements in
                // place; nulling the pointer would alias scratch memory on the
                // next access. Storage targets keep the assignment path.
                if let ExprKind::Ident(res_slice) = &target.kind
                    && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
                    && !self.storage_ref_locals.contains(var_id)
                    && !self.storage_slots.contains_key(var_id)
                {
                    let var = self.gcx.hir.variable(*var_id);
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
                let base_val = self.lower_expr(builder, base);
                let start_val = start
                    .map(|s| self.lower_expr(builder, s))
                    .unwrap_or_else(|| builder.imm_u64(0));
                let _end_val = end.map(|e| self.lower_expr(builder, e));
                let offset_32 = builder.imm_u64(32);
                let byte_offset = builder.mul(start_val, offset_32);
                builder.add(base_val, byte_offset)
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
                    if let Some(&slot) = self.storage_slots.get(var_id) {
                        // For storage structs, we need to copy to memory and return the pointer
                        if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind
                        {
                            // Calculate total flattened size (handles nested structs)
                            let total_words = self.calculate_memory_words_for_type(&var.ty);
                            let struct_size = total_words * 32;
                            let struct_ptr = self.allocate_memory(builder, struct_size);

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
                        return builder.sload(slot_val);
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
            Builtin::MsgData => builder.imm_u64(0),
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
        let ExprKind::Ident(res_slice) = &base.kind else {
            self.gcx
                .dcx()
                .err(format!("unsupported Yul member `.{}`", member.name))
                .span(member.span)
                .emit();
            return builder.imm_u64(0);
        };

        let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first() else {
            self.gcx
                .dcx()
                .err(format!("unsupported Yul member `.{}`", member.name))
                .span(member.span)
                .emit();
            return builder.imm_u64(0);
        };

        // For a calldata array/bytes parameter, its lowered value is the ABI
        // head: the offset (relative to the start of the args, i.e. after the
        // 4-byte selector) to the length word. So the length word is at calldata
        // position `4 + head`, and the first element at `4 + head + 32`.
        let calldata_head = (self.gcx.hir.variable(*var_id).data_location
            == Some(solar_ast::DataLocation::Calldata))
        .then(|| self.locals.get(var_id).copied())
        .flatten();

        match member.name {
            sym::slot => {
                if let Some(&slot) = self.storage_slots.get(var_id) {
                    return builder.imm_u64(slot);
                }
                if let Some(&slot) = self.locals.get(var_id) {
                    return slot;
                }
                if let Some(offset) = self.get_local_memory_offset(var_id) {
                    let offset = self.local_memory_addr(builder, offset);
                    return builder.mload(offset);
                }
            }
            sym::offset => {
                if let Some(head) = calldata_head {
                    let base = builder.imm_u64(4 + 32);
                    return builder.add(base, head);
                }
                return builder.imm_u64(0);
            }
            sym::length => {
                if let Some(head) = calldata_head {
                    let four = builder.imm_u64(4);
                    let pos = builder.add(four, head);
                    return builder.calldataload(pos);
                }
            }
            _ => {}
        }

        self.gcx
            .dcx()
            .err(format!("unsupported Yul member `.{}`", member.name))
            .span(member.span)
            .emit();
        builder.imm_u64(0)
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

    fn emit_revert_error_string_from_expr(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> bool {
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

        let len = builder.mload(ptr);
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
        let contract_id = match &ty.kind {
            hir::TypeKind::Custom(hir::ItemId::Contract(id)) => *id,
            _ => panic!("codegen expected contract type for `type(C).creationCode`"),
        };

        // Look up pre-compiled bytecode
        // For creationCode we use the deployment bytecode (initcode)
        if !is_creation_code {
            panic!("codegen does not support `type(C).runtimeCode` yet");
        }

        let (bytecode, _segment_idx) = match self.contract_bytecodes.get(&contract_id) {
            Some(bc) => bc.clone(),
            None => panic!("codegen missing creation bytecode for `type(C).creationCode`"),
        };

        let bytecode_len = bytecode.len();

        // Allocate memory for bytes: 32 bytes length + bytecode
        // Layout: [length (32 bytes)][data...]
        //
        // ptr = mload(0x40)           // get free memory pointer
        // mstore(ptr, bytecode_len)   // store length
        // // copy bytecode to ptr+32
        // mstore(0x40, ptr + 32 + aligned_len)  // update free memory pointer

        let free_mem_ptr_slot = builder.imm_u64(0x40);
        let ptr = builder.mload(free_mem_ptr_slot);

        // Store length at ptr
        let len_val = builder.imm_u64(bytecode_len as u64);
        builder.mstore(ptr, len_val);

        // Copy bytecode to ptr+32 using MSTORE loop
        let thirty_two = builder.imm_u64(32);
        let data_start = builder.add(ptr, thirty_two);

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

        // Update free memory pointer: ptr + 32 + ceil(bytecode_len / 32) * 32
        let aligned_data_len = bytecode_len.div_ceil(32) * 32;
        let total_size = 32 + aligned_data_len;
        let total_size_val = builder.imm_u64(total_size as u64);
        let new_free_ptr = builder.add(ptr, total_size_val);
        builder.mstore(free_mem_ptr_slot, new_free_ptr);

        // Return ptr (the bytes memory value)
        ptr
    }

    /// Lowers a ternary conditional expression with proper branching.
    /// This handles both scalar and tuple returns correctly by using control flow
    /// instead of select, and storing multi-value results in scratch memory.
    fn lower_ternary(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        cond: &hir::Expr<'_>,
        then_expr: &hir::Expr<'_>,
        else_expr: &hir::Expr<'_>,
    ) -> ValueId {
        // Determine if this is a tuple-typed ternary by checking if either branch is a tuple
        let is_tuple = matches!(then_expr.kind, ExprKind::Tuple(_))
            || matches!(else_expr.kind, ExprKind::Tuple(_));

        if is_tuple {
            // For tuple ternaries, use branching to write values to scratch memory
            let cond_val = self.lower_expr(builder, cond);

            let then_block = builder.create_block();
            let else_block = builder.create_block();
            let merge_block = builder.create_block();

            builder.branch(cond_val, then_block, else_block);

            // Then block: evaluate then_expr and write tuple elements to memory
            builder.switch_to_block(then_block);
            self.lower_tuple_to_scratch(builder, then_expr);
            builder.jump(merge_block);

            // Else block: evaluate else_expr and write tuple elements to memory
            builder.switch_to_block(else_block);
            self.lower_tuple_to_scratch(builder, else_expr);
            builder.jump(merge_block);

            // Merge block: load first value from scratch memory
            builder.switch_to_block(merge_block);
            let zero = builder.imm_u64(0);
            builder.mload(zero)
        } else {
            // For non-tuple ternaries, still use branching for correct semantics
            // (only one branch should be evaluated for side effects)
            let cond_val = self.lower_expr(builder, cond);

            let then_block = builder.create_block();
            let else_block = builder.create_block();
            let merge_block = builder.create_block();

            builder.branch(cond_val, then_block, else_block);

            // Then block
            builder.switch_to_block(then_block);
            let then_val = self.lower_expr(builder, then_expr);
            let zero_then = builder.imm_u64(0);
            builder.mstore(zero_then, then_val);
            builder.jump(merge_block);

            // Else block
            builder.switch_to_block(else_block);
            let else_val = self.lower_expr(builder, else_expr);
            let zero_else = builder.imm_u64(0);
            builder.mstore(zero_else, else_val);
            builder.jump(merge_block);

            // Merge block: load result from scratch memory
            builder.switch_to_block(merge_block);
            let zero = builder.imm_u64(0);
            builder.mload(zero)
        }
    }

    /// Lowers a tuple expression by writing all elements to scratch memory.
    /// Element i is stored at offset i*32.
    fn lower_tuple_to_scratch(&mut self, builder: &mut FunctionBuilder<'_>, expr: &hir::Expr<'_>) {
        if let ExprKind::Tuple(elements) = &expr.kind {
            for (i, elem_opt) in elements.iter().enumerate() {
                if let Some(elem) = elem_opt {
                    let val = self.lower_expr(builder, elem);
                    let offset = builder.imm_u64(i as u64 * 32);
                    builder.mstore(offset, val);
                }
            }
        } else {
            // Not a tuple - just store single value at offset 0
            let val = self.lower_expr(builder, expr);
            let zero = builder.imm_u64(0);
            builder.mstore(zero, val);
        }
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

    pub(super) fn expr_has_bytes_or_string_type(&self, expr: &hir::Expr<'_>) -> bool {
        self.get_expr_type(expr).is_some_and(|ty| {
            matches!(
                ty.peel_refs().kind,
                TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
            )
        })
    }

    /// Lowers an assignment.
    fn lower_assign(
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
                        let offset_val = self.local_memory_addr(builder, offset);
                        builder.mstore(offset_val, rhs);
                    } else if self.locals.contains_key(var_id) {
                        // Function parameter - update SSA mapping (shouldn't happen normally)
                        self.locals.insert(*var_id, rhs);
                    } else if let Some(&offset) = self.immutable_slots.get(var_id) {
                        self.store_immutable_value(builder, offset, rhs);
                    } else if let Some(&base_slot) = self.storage_slots.get(var_id) {
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
                            let slot_val = builder.imm_u64(base_slot);
                            builder.sstore(slot_val, rhs);
                        }
                    }
                }
            }
            ExprKind::Index(base, index) => {
                self.lower_index_assign(builder, lhs, base, index.as_deref(), rhs);
            }
            ExprKind::Member(base, member) => {
                // Check if this is a storage struct member assignment (e.g., storedPoint.x = value)
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

                // Check if this is a nested memory struct assignment (e.g., o.inner.a = value)
                if let Some((var_id, total_offset, _inner_struct_id)) =
                    self.compute_nested_memory_struct_info(base, *member)
                {
                    // Get the base pointer for the outermost struct
                    let base_ptr = if let Some(offset) = self.get_local_memory_offset(&var_id) {
                        // Variable is stored in local memory slot
                        let offset_val = self.local_memory_addr(builder, offset);
                        builder.mload(offset_val)
                    } else if let Some(&val) = self.locals.get(&var_id) {
                        // Variable is an SSA value (pointer to struct)
                        val
                    } else {
                        builder.imm_u64(0)
                    };

                    if total_offset == 0 {
                        builder.mstore(base_ptr, rhs);
                        return;
                    }
                    let offset_val = builder.imm_u64(total_offset);
                    let field_addr = builder.add(base_ptr, offset_val);
                    builder.mstore(field_addr, rhs);
                    return;
                }

                // Regular memory struct member assignment
                if let Some((struct_id, field_index)) =
                    self.get_memory_struct_field_info(base, *member)
                {
                    let base_val = self.lower_expr(builder, base);
                    let field_offset = self.get_struct_field_memory_offset(struct_id, field_index);
                    if field_offset == 0 {
                        builder.mstore(base_val, rhs);
                        return;
                    }
                    let offset_val = builder.imm_u64(field_offset);
                    let field_addr = builder.add(base_val, offset_val);
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

    /// Lowers a function call.
    fn lower_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        callee: &hir::Expr<'_>,
        args: &CallArgs<'_>,
        call_opts: Option<&[hir::NamedArg<'_>]>,
    ) -> ValueId {
        if let ExprKind::Ident(res_slice) = &callee.kind
            && let Some(builtin) = self.select_builtin_overload(res_slice, args)
        {
            return self.lower_builtin_call(builder, builtin, args);
        }

        if let Some(error_id) = self.custom_error_id_from_callee(callee) {
            self.emit_custom_error_revert(builder, error_id, args);
            return builder.imm_u64(0);
        }

        // `T.wrap(x)` / `T.unwrap(v)` for a user-defined value type are identity
        // operations at the EVM level: a UDVT value is represented exactly as its
        // underlying type, so no wrapper is added or removed.
        if let ExprKind::Member(base, member) = &callee.kind
            && matches!(member.name.as_str(), "wrap" | "unwrap")
            && let ExprKind::Ident(res_slice) = &base.kind
            && res_slice.iter().any(|r| matches!(r, hir::Res::Item(hir::ItemId::Udvt(_))))
            && let Some(arg) = args.exprs().next()
        {
            return self.lower_expr(builder, arg);
        }

        if let ExprKind::Member(base, member) = &callee.kind {
            return self.lower_member_call_with_opts(builder, base, *member, args, call_opts);
        }

        // Handle `new Contract(args)` - contract creation
        if let ExprKind::New(ty) = &callee.kind {
            if self.is_memory_array_new_type(ty) {
                return self.lower_new_array(builder, ty, args);
            }
            return self.lower_new_contract(builder, ty, args, call_opts);
        }

        // Handle internal function calls: func(args) where func is a function in the same contract
        if let ExprKind::Ident(res_slice) = &callee.kind {
            let arg_count = args.exprs().count();

            // Find the best matching overload based on argument count
            // First, collect all function resolutions
            let func_resolutions: Vec<_> = res_slice
                .iter()
                .filter_map(|r| {
                    if let hir::Res::Item(hir::ItemId::Function(fid)) = r {
                        Some(*fid)
                    } else {
                        None
                    }
                })
                .collect();

            // Select the overload that matches the argument count
            let selected_func = func_resolutions
                .iter()
                .find(|&&fid| {
                    let f = self.gcx.hir.function(fid);
                    f.parameters.len() == arg_count
                })
                .or_else(|| func_resolutions.first());

            if let Some(&func_id) = selected_func {
                return self.lower_internal_call(builder, func_id, args);
            }
        }

        // Handle type conversion calls: Type(expr)
        // e.g., ICallee(addr), uint256(x), address(y), Status(n)
        // The callee is an Ident resolving to a contract/interface/enum type
        if let ExprKind::Ident(res_slice) = &callee.kind {
            // Check if this resolves to a contract or interface type
            if let Some(hir::Res::Item(hir::ItemId::Contract(_))) = res_slice.first() {
                // Type conversion: just return the first argument unchanged
                // (The actual conversion is a no-op at the EVM level for addresses/contracts)
                if let Some(first_arg) = args.exprs().next() {
                    return self.lower_expr(builder, first_arg);
                }
            }
            // Check if this resolves to an enum type (e.g., Status(n))
            if let Some(hir::Res::Item(hir::ItemId::Enum(_))) = res_slice.first() {
                // Enum conversion: just return the first argument unchanged
                // (Enums are represented as uint8 at the EVM level)
                if let Some(first_arg) = args.exprs().next() {
                    return self.lower_expr(builder, first_arg);
                }
            }
            // Check if this resolves to a struct type (struct constructor call)
            // e.g., Point(10, 20) creates a memory struct
            if let Some(hir::Res::Item(hir::ItemId::Struct(struct_id))) = res_slice.first() {
                return self.lower_struct_constructor(builder, *struct_id, args);
            }
        }

        // Handle Type(expr) where callee is an explicit Type expression
        // e.g., uint256(x), address(y), bytes32(z)
        if let ExprKind::Type(ty) = &callee.kind
            && let Some(first_arg) = args.exprs().next()
        {
            let value = self.lower_expr(builder, first_arg);
            return self.lower_type_conversion(builder, ty, first_arg, value);
        }

        builder.imm_u64(0)
    }

    fn select_builtin_overload(
        &self,
        res_slice: &[hir::Res],
        args: &CallArgs<'_>,
    ) -> Option<Builtin> {
        let arg_count = args.exprs().count();
        res_slice
            .iter()
            .filter_map(|res| match res {
                hir::Res::Builtin(builtin) => Some(*builtin),
                _ => None,
            })
            .find(|builtin| match builtin {
                Builtin::Revert => arg_count == 0,
                Builtin::RevertMsg => arg_count == 1,
                _ => true,
            })
    }

    fn custom_error_id_from_callee(&self, callee: &hir::Expr<'_>) -> Option<hir::ErrorId> {
        if let Some(ty) = self.get_expr_type(callee)
            && let TyKind::Error(_, error_id) = ty.kind
        {
            return Some(error_id);
        }

        let ExprKind::Ident(res_slice) = &callee.kind else { return None };
        res_slice.iter().find_map(|res| {
            if let hir::Res::Item(hir::ItemId::Error(error_id)) = res {
                Some(*error_id)
            } else {
                None
            }
        })
    }

    fn emit_revert_payload_from_expr(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> bool {
        if self.emit_custom_error_revert_from_expr(builder, expr) {
            return true;
        }
        self.emit_revert_error_string_from_expr(builder, expr)
    }

    fn emit_custom_error_revert_from_expr(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> bool {
        let ExprKind::Call(callee, args, _) = &expr.kind else { return false };
        let Some(error_id) = self.custom_error_id_from_callee(callee) else {
            return false;
        };
        self.emit_custom_error_revert(builder, error_id, args);
        true
    }

    fn emit_custom_error_revert(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        error_id: hir::ErrorId,
        args: &CallArgs<'_>,
    ) {
        let param_tys = self.gcx.item_parameter_types(hir::ItemId::Error(error_id));
        let arg_exprs = self.ordered_custom_error_args(error_id, args);
        let mut items = Vec::with_capacity(param_tys.len());
        for (&ty, arg) in param_tys.iter().zip(arg_exprs) {
            let value = self.lower_return_value_for_ty(builder, arg, ty);
            items.push((value, ty));
        }

        let selector = self.custom_error_selector(error_id);
        self.emit_abi_error_revert(builder, selector, &items);
    }

    fn ordered_custom_error_args<'a>(
        &self,
        error_id: hir::ErrorId,
        args: &'a CallArgs<'a>,
    ) -> Vec<&'a hir::Expr<'a>> {
        match args.kind {
            hir::CallArgsKind::Unnamed(exprs) => exprs.iter().collect(),
            hir::CallArgsKind::Named(named_args) => {
                let error = self.gcx.hir.error(error_id);
                let mut ordered = Vec::with_capacity(error.parameters.len());
                for &param_id in error.parameters {
                    let Some(param_name) =
                        self.gcx.hir.variable(param_id).name.map(|name| name.name)
                    else {
                        continue;
                    };
                    if let Some(arg) = named_args.iter().find(|arg| arg.name.name == param_name) {
                        ordered.push(&arg.value);
                    }
                }
                ordered
            }
        }
    }

    fn custom_error_selector(&self, error_id: hir::ErrorId) -> [u8; 4] {
        let signature = self.gcx.item_signature(hir::ItemId::Error(error_id));
        let hash = keccak256(signature.as_bytes());
        [hash[0], hash[1], hash[2], hash[3]]
    }

    fn lower_type_conversion(
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
        let low_bits = u64::from(32 - bytes) * 8;
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

    /// Lowers a `new T[](len)` memory array expression.
    fn lower_new_array(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: &hir::Type<'_>,
        args: &CallArgs<'_>,
    ) -> ValueId {
        if !self.is_memory_array_new_type(ty) {
            return builder.imm_u64(0);
        }

        let len = args
            .exprs()
            .next()
            .map(|arg| self.lower_expr(builder, arg))
            .unwrap_or_else(|| builder.imm_u64(0));

        let free_ptr_addr = builder.imm_u64(0x40);
        let ptr = builder.mload(free_ptr_addr);
        builder.mstore(ptr, len);

        let word_size = builder.imm_u64(32);
        let data_size = if matches!(
            &ty.kind,
            hir::TypeKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
        ) {
            // `bytes`/`string`: the length counts bytes; the data area is the
            // length padded up to a word.
            let thirty_one = builder.imm_u64(31);
            let rounded = builder.add(len, thirty_one);
            let rounded_overflow = builder.lt(rounded, len);
            self.emit_panic_if(builder, rounded_overflow, PanicCode::MemoryAllocationOverflow);
            let mask = builder.not(thirty_one);
            builder.and(rounded, mask)
        } else {
            // Arrays: one word per element.
            let data_size = builder.mul(len, word_size);
            let checked_len = builder.div(data_size, word_size);
            let overflow = builder.eq(checked_len, len);
            self.emit_panic_if_zero(builder, overflow, PanicCode::MemoryAllocationOverflow);
            data_size
        };
        let total_size = builder.add(data_size, word_size);
        let total_overflow = builder.lt(total_size, data_size);
        self.emit_panic_if(builder, total_overflow, PanicCode::MemoryAllocationOverflow);
        let new_free_ptr = builder.add(ptr, total_size);
        let bump_overflow = builder.lt(new_free_ptr, ptr);
        self.emit_panic_if(builder, bump_overflow, PanicCode::MemoryAllocationOverflow);
        // Solidity caps memory at 2^64 bytes: an allocation past that limit
        // panics (0x41) rather than running the VM out of gas on a huge size.
        let mem_limit = builder.imm_u64(0xffff_ffff_ffff_ffff);
        let over_limit = builder.gt(new_free_ptr, mem_limit);
        self.emit_panic_if(builder, over_limit, PanicCode::MemoryAllocationOverflow);
        let free_ptr_addr = builder.imm_u64(0x40);
        builder.mstore(free_ptr_addr, new_free_ptr);

        // Zero-initialize the data area: memory past the free pointer can be
        // dirty (keccak staging fast paths write there without bumping it).
        // `calldatacopy` from the end of calldata writes zeroes.
        let data_ptr = builder.add(ptr, word_size);
        let cds = builder.calldatasize();
        builder.calldatacopy(data_ptr, cds, data_size);

        ptr
    }

    fn is_memory_array_new_type(&self, ty: &hir::Type<'_>) -> bool {
        match &ty.kind {
            hir::TypeKind::Array(array) => array.size.is_none(),
            hir::TypeKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => true,
            _ => false,
        }
    }

    /// Lowers a `new Contract(args)` expression.
    /// Supports call options like `new Contract{salt: s, value: v}(args)`.
    fn lower_new_contract(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: &hir::Type<'_>,
        args: &CallArgs<'_>,
        call_opts: Option<&[hir::NamedArg<'_>]>,
    ) -> ValueId {
        // Extract ContractId from the type
        let contract_id = match &ty.kind {
            hir::TypeKind::Custom(hir::ItemId::Contract(id)) => *id,
            _ => {
                self.gcx
                    .dcx()
                    .err("codegen expected a contract type for `new` expression")
                    .span(ty.span)
                    .emit();
                return builder.imm_u64(0);
            }
        };

        // Look up pre-compiled bytecode
        let (bytecode, _segment_idx) = match self.contract_bytecodes.get(&contract_id) {
            Some(bc) => bc.clone(),
            None => {
                self.gcx
                    .dcx()
                    .err(format!(
                        "codegen is missing creation bytecode for `new {}`",
                        self.gcx.hir.contract(contract_id).name
                    ))
                    .span(ty.span)
                    .note("the deployed contract did not compile or was not lowered first")
                    .emit();
                return builder.imm_u64(0);
            }
        };

        let bytecode_len = bytecode.len();

        // Extract call options (salt, value)
        let mut salt_opt: Option<ValueId> = None;
        let mut value_opt: Option<ValueId> = None;

        if let Some(opts) = call_opts {
            for opt in opts {
                let name = opt.name.name.as_str();
                match name {
                    "salt" => {
                        salt_opt = Some(self.lower_expr(builder, &opt.value));
                    }
                    "value" => {
                        value_opt = Some(self.lower_expr(builder, &opt.value));
                    }
                    _ => {
                        // gas option is not supported for contract creation
                    }
                }
            }
        }

        // Allocate memory for bytecode + constructor args from free memory pointer
        let free_mem_ptr_slot = builder.imm_u64(0x40);
        let mem_offset = builder.mload(free_mem_ptr_slot);

        // Copy bytecode to memory using MSTORE
        // For each 32-byte chunk of bytecode, emit an MSTORE at (mem_offset + offset)
        for (i, chunk) in bytecode.chunks(32).enumerate() {
            let mut padded = [0u8; 32];
            padded[..chunk.len()].copy_from_slice(chunk);
            let value = U256::from_be_bytes(padded);
            let val_id = builder.imm_u256(value);
            let chunk_offset = builder.imm_u64((i as u64) * 32);
            let dest = builder.add(mem_offset, chunk_offset);
            builder.mstore(dest, val_id);
        }

        // Append constructor arguments after bytecode
        let mut args_offset = bytecode_len as u64;
        for arg in args.exprs() {
            let arg_val = self.lower_expr(builder, arg);
            let arg_offset_imm = builder.imm_u64(args_offset);
            let arg_dest = builder.add(mem_offset, arg_offset_imm);
            builder.mstore(arg_dest, arg_val);
            args_offset += 32; // Each arg is 32 bytes ABI encoded
        }

        // Total size = bytecode + args
        let total_size = builder.imm_u64(args_offset);

        // Update free memory pointer: new_free = mem_offset + ((total_size + 31) & ~31)
        let thirty_one = builder.imm_u64(31);
        let aligned_size = builder.add(total_size, thirty_one);
        let mask = builder.imm_u256(U256::from(!31u64));
        let aligned_size = builder.and(aligned_size, mask);
        let new_free = builder.add(mem_offset, aligned_size);
        builder.mstore(free_mem_ptr_slot, new_free);

        // Value to send with CREATE/CREATE2 (0 for non-payable, or from value option)
        let value = value_opt.unwrap_or_else(|| builder.imm_u64(0));

        // Emit CREATE2 if salt is provided, otherwise CREATE
        if let Some(salt) = salt_opt {
            builder.create2(value, mem_offset, total_size, salt)
        } else {
            builder.create(value, mem_offset, total_size)
        }
    }

    /// Lowers a builtin function call.
    fn lower_builtin_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        builtin: Builtin,
        args: &CallArgs<'_>,
    ) -> ValueId {
        match builtin {
            Builtin::Keccak256 => {
                let mut exprs = args.exprs();
                if let Some(first) = exprs.next() {
                    if let ExprKind::Lit(lit) = &first.kind
                        && let LitKind::Str(_, bytes, _) = &lit.kind
                    {
                        let hash = keccak256(bytes.as_byte_str());
                        return builder.imm_u256(U256::from_be_bytes(hash.0));
                    }

                    if let Some(packed_args) = self.abi_encode_packed_call_args(first) {
                        return self.lower_keccak_abi_encode_packed(builder, packed_args);
                    }
                    if let Some(encode_args) = self.abi_encode_call_args(first) {
                        let arg_exprs: Vec<_> = encode_args.exprs().collect();
                        if let Some(hash) = self.lower_keccak_abi_encode(builder, &arg_exprs) {
                            return hash;
                        }
                    }

                    // Dynamic `bytes`/`string` (incl. `bytes(s)` of a calldata
                    // value): hash the raw data after materializing it to memory.
                    if let Some(hash) = self.keccak_dynamic_bytes(builder, first) {
                        return hash;
                    }
                    let arg_val = self.lower_expr(builder, first);
                    let ptr = builder.imm_u64(0);
                    builder.mstore(ptr, arg_val);
                    let size = builder.imm_u64(32);
                    return builder.keccak256(ptr, size);
                }
                builder.imm_u64(0)
            }
            Builtin::Require | Builtin::Assert => {
                let mut exprs = args.exprs();
                if let Some(first) = exprs.next() {
                    let cond = self.lower_expr(builder, first);
                    let is_false = builder.iszero(cond);

                    let revert_block = builder.create_block();
                    let continue_block = builder.create_block();

                    builder.branch(is_false, revert_block, continue_block);

                    builder.switch_to_block(revert_block);
                    if matches!(builtin, Builtin::Assert) {
                        self.emit_panic_revert(builder, PanicCode::Assert);
                    } else if let Some(message) = exprs.next() {
                        if !self.emit_revert_payload_from_expr(builder, message) {
                            let zero = builder.imm_u64(0);
                            builder.revert(zero, zero);
                        }
                    } else {
                        let zero = builder.imm_u64(0);
                        builder.revert(zero, zero);
                    }

                    builder.switch_to_block(continue_block);
                }
                builder.imm_u64(0)
            }
            Builtin::Revert => {
                let zero = builder.imm_u64(0);
                builder.revert(zero, zero);
                zero
            }
            Builtin::RevertMsg => {
                let mut exprs = args.exprs();
                let emitted = exprs.next().is_some_and(|message| {
                    self.emit_revert_error_string_from_expr(builder, message)
                });
                let zero = builder.imm_u64(0);
                if !emitted {
                    builder.revert(zero, zero);
                }
                zero
            }
            Builtin::AddressBalance => {
                let mut exprs = args.exprs();
                if let Some(first) = exprs.next() {
                    let addr = self.lower_expr(builder, first);
                    return builder.balance(addr);
                }
                builder.imm_u64(0)
            }
            Builtin::AddMod | Builtin::MulMod => {
                let mut exprs = args.exprs();
                let Some(a) = exprs.next() else { return builder.imm_u64(0) };
                let Some(b) = exprs.next() else { return builder.imm_u64(0) };
                let Some(n) = exprs.next() else { return builder.imm_u64(0) };
                let a = self.lower_expr(builder, a);
                let b = self.lower_expr(builder, b);
                let n = self.lower_expr(builder, n);
                if matches!(builtin, Builtin::AddMod) {
                    builder.addmod(a, b, n)
                } else {
                    builder.mulmod(a, b, n)
                }
            }
            Builtin::AbiEncode => {
                // abi.encode: a fresh `bytes memory` allocation holding the
                // padded ABI tuple encoding of the arguments.
                let arg_exprs: Vec<_> = args.exprs().collect();
                if let Some(ptr) = self.lower_abi_encode_to_bytes(builder, &arg_exprs) {
                    return ptr;
                }
                self.gcx
                    .dcx()
                    .err("codegen does not support these `abi.encode` arguments yet")
                    .span(args.span)
                    .emit();
                builder.imm_u64(0)
            }
            Builtin::AbiEncodePacked => {
                // abi.encodePacked: pack values tightly based on their types
                // Returns bytes memory (length + data)
                self.lower_abi_encode_packed(builder, args)
            }
            Builtin::AbiDecode => self.lower_abi_decode(builder, args),
            Builtin::YulAdd
            | Builtin::YulSub
            | Builtin::YulMul
            | Builtin::YulDiv
            | Builtin::YulMod
            | Builtin::YulExp
            | Builtin::YulNot
            | Builtin::YulAnd
            | Builtin::YulOr
            | Builtin::YulXor
            | Builtin::YulShl
            | Builtin::YulShr
            | Builtin::YulSar
            | Builtin::YulStop
            | Builtin::YulSdiv
            | Builtin::YulSmod
            | Builtin::YulLt
            | Builtin::YulGt
            | Builtin::YulSlt
            | Builtin::YulSgt
            | Builtin::YulEq
            | Builtin::YulIszero
            | Builtin::YulByte
            | Builtin::YulClz
            | Builtin::YulAddmod
            | Builtin::YulMulmod
            | Builtin::YulSignextend
            | Builtin::YulKeccak256
            | Builtin::YulAddress
            | Builtin::YulBalance
            | Builtin::YulSelfbalance
            | Builtin::YulCaller
            | Builtin::YulCallvalue
            | Builtin::YulCalldataload
            | Builtin::YulCalldatasize
            | Builtin::YulCalldatacopy
            | Builtin::YulCodesize
            | Builtin::YulCodecopy
            | Builtin::YulExtcodesize
            | Builtin::YulExtcodecopy
            | Builtin::YulReturndatasize
            | Builtin::YulReturndatacopy
            | Builtin::YulExtcodehash
            | Builtin::YulMload
            | Builtin::YulMstore
            | Builtin::YulMstore8
            | Builtin::YulSload
            | Builtin::YulSstore
            | Builtin::YulTload
            | Builtin::YulTstore
            | Builtin::YulMsize
            | Builtin::YulGas
            | Builtin::YulLog0
            | Builtin::YulLog1
            | Builtin::YulLog2
            | Builtin::YulLog3
            | Builtin::YulLog4
            | Builtin::YulCreate
            | Builtin::YulCreate2
            | Builtin::YulCall
            | Builtin::YulCallcode
            | Builtin::YulDelegatecall
            | Builtin::YulStaticcall
            | Builtin::YulExtcall
            | Builtin::YulExtdelegatecall
            | Builtin::YulExtstaticcall
            | Builtin::YulReturn
            | Builtin::YulRevert
            | Builtin::YulSelfdestruct
            | Builtin::YulInvalid
            | Builtin::YulChainid
            | Builtin::YulBasefee
            | Builtin::YulBlobbasefee
            | Builtin::YulBlobhash
            | Builtin::YulCoinbase
            | Builtin::YulDifficulty
            | Builtin::YulPrevrandao
            | Builtin::YulGaslimit
            | Builtin::YulNumber
            | Builtin::YulTimestamp
            | Builtin::YulGasprice
            | Builtin::YulOrigin
            | Builtin::YulBlockhash
            | Builtin::YulPop
            | Builtin::YulMcopy => self.lower_yul_builtin_call(builder, builtin, args),
            _ => builder.imm_u64(0),
        }
    }

    fn lower_yul_builtin_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        builtin: Builtin,
        args: &CallArgs<'_>,
    ) -> ValueId {
        let arg_vals: Vec<ValueId> =
            args.exprs().map(|arg| self.lower_expr(builder, arg)).collect();
        if let Some(expected) = Self::yul_builtin_arity(builtin)
            && arg_vals.len() != expected
        {
            self.gcx
                .dcx()
                .err(format!(
                    "wrong number of arguments for Yul builtin `{}`: expected {}, found {}",
                    builtin.name(),
                    expected,
                    arg_vals.len()
                ))
                .span(args.span)
                .emit();
            return builder.imm_u64(0);
        }

        match builtin {
            Builtin::YulAdd => builder.add(arg_vals[0], arg_vals[1]),
            Builtin::YulSub => builder.sub(arg_vals[0], arg_vals[1]),
            Builtin::YulMul => builder.mul(arg_vals[0], arg_vals[1]),
            Builtin::YulDiv => builder.div(arg_vals[0], arg_vals[1]),
            Builtin::YulSdiv => builder.sdiv(arg_vals[0], arg_vals[1]),
            Builtin::YulMod => builder.mod_(arg_vals[0], arg_vals[1]),
            Builtin::YulSmod => builder.smod(arg_vals[0], arg_vals[1]),
            Builtin::YulAddmod => builder.addmod(arg_vals[0], arg_vals[1], arg_vals[2]),
            Builtin::YulMulmod => builder.mulmod(arg_vals[0], arg_vals[1], arg_vals[2]),
            Builtin::YulExp => builder.exp(arg_vals[0], arg_vals[1]),
            Builtin::YulSignextend => builder.signextend(arg_vals[0], arg_vals[1]),
            Builtin::YulAnd => builder.and(arg_vals[0], arg_vals[1]),
            Builtin::YulOr => builder.or(arg_vals[0], arg_vals[1]),
            Builtin::YulXor => builder.xor(arg_vals[0], arg_vals[1]),
            Builtin::YulNot => builder.not(arg_vals[0]),
            Builtin::YulByte => builder.byte(arg_vals[0], arg_vals[1]),
            Builtin::YulShl => builder.shl(arg_vals[0], arg_vals[1]),
            Builtin::YulShr => builder.shr(arg_vals[0], arg_vals[1]),
            Builtin::YulSar => builder.sar(arg_vals[0], arg_vals[1]),
            Builtin::YulLt => builder.lt(arg_vals[0], arg_vals[1]),
            Builtin::YulGt => builder.gt(arg_vals[0], arg_vals[1]),
            Builtin::YulSlt => builder.slt(arg_vals[0], arg_vals[1]),
            Builtin::YulSgt => builder.sgt(arg_vals[0], arg_vals[1]),
            Builtin::YulEq => builder.eq(arg_vals[0], arg_vals[1]),
            Builtin::YulIszero => builder.iszero(arg_vals[0]),
            Builtin::YulMload => builder.mload(arg_vals[0]),
            Builtin::YulMstore => {
                builder.mstore(arg_vals[0], arg_vals[1]);
                builder.imm_u64(0)
            }
            Builtin::YulMstore8 => {
                builder.mstore8(arg_vals[0], arg_vals[1]);
                builder.imm_u64(0)
            }
            Builtin::YulMsize => builder.msize(),
            Builtin::YulMcopy => {
                self.mcopy(builder, arg_vals[0], arg_vals[1], arg_vals[2], Some(args.span));
                builder.imm_u64(0)
            }
            Builtin::YulSload => builder.sload(arg_vals[0]),
            Builtin::YulSstore => {
                builder.sstore(arg_vals[0], arg_vals[1]);
                builder.imm_u64(0)
            }
            Builtin::YulTload => builder.tload(arg_vals[0]),
            Builtin::YulTstore => {
                builder.tstore(arg_vals[0], arg_vals[1]);
                builder.imm_u64(0)
            }
            Builtin::YulCalldataload => builder.calldataload(arg_vals[0]),
            Builtin::YulCalldatasize => builder.calldatasize(),
            Builtin::YulCalldatacopy => {
                builder.calldatacopy(arg_vals[0], arg_vals[1], arg_vals[2]);
                builder.imm_u64(0)
            }
            Builtin::YulCodesize => builder.codesize(),
            Builtin::YulCodecopy => {
                builder.codecopy(arg_vals[0], arg_vals[1], arg_vals[2]);
                builder.imm_u64(0)
            }
            Builtin::YulExtcodesize => builder.extcodesize(arg_vals[0]),
            Builtin::YulExtcodecopy => {
                builder.extcodecopy(arg_vals[0], arg_vals[1], arg_vals[2], arg_vals[3]);
                builder.imm_u64(0)
            }
            Builtin::YulExtcodehash => builder.extcodehash(arg_vals[0]),
            Builtin::YulReturndatasize => builder.returndatasize(),
            Builtin::YulReturndatacopy => {
                builder.returndatacopy(arg_vals[0], arg_vals[1], arg_vals[2]);
                builder.imm_u64(0)
            }
            Builtin::YulAddress => builder.address(),
            Builtin::YulBalance => builder.balance(arg_vals[0]),
            Builtin::YulSelfbalance => builder.selfbalance(),
            Builtin::YulCaller => builder.caller(),
            Builtin::YulCallvalue => builder.callvalue(),
            Builtin::YulOrigin => builder.origin(),
            Builtin::YulGasprice => builder.gasprice(),
            Builtin::YulBlockhash => builder.blockhash(arg_vals[0]),
            Builtin::YulCoinbase => builder.coinbase(),
            Builtin::YulTimestamp => builder.timestamp(),
            Builtin::YulNumber => builder.number(),
            Builtin::YulDifficulty | Builtin::YulPrevrandao => builder.prevrandao(),
            Builtin::YulGaslimit => builder.gaslimit(),
            Builtin::YulChainid => builder.chainid(),
            Builtin::YulGas => builder.gas(),
            Builtin::YulBasefee => builder.basefee(),
            Builtin::YulBlobbasefee => builder.blobbasefee(),
            Builtin::YulBlobhash => builder.blobhash(arg_vals[0]),
            Builtin::YulKeccak256 => builder.keccak256(arg_vals[0], arg_vals[1]),
            Builtin::YulCall => builder.call(
                arg_vals[0],
                arg_vals[1],
                arg_vals[2],
                arg_vals[3],
                arg_vals[4],
                arg_vals[5],
                arg_vals[6],
            ),
            Builtin::YulStaticcall => builder.staticcall(
                arg_vals[0],
                arg_vals[1],
                arg_vals[2],
                arg_vals[3],
                arg_vals[4],
                arg_vals[5],
            ),
            Builtin::YulDelegatecall => builder.delegatecall(
                arg_vals[0],
                arg_vals[1],
                arg_vals[2],
                arg_vals[3],
                arg_vals[4],
                arg_vals[5],
            ),
            Builtin::YulCreate => builder.create(arg_vals[0], arg_vals[1], arg_vals[2]),
            Builtin::YulCreate2 => {
                builder.create2(arg_vals[0], arg_vals[1], arg_vals[2], arg_vals[3])
            }
            Builtin::YulLog0 => {
                builder.log0(arg_vals[0], arg_vals[1]);
                builder.imm_u64(0)
            }
            Builtin::YulLog1 => {
                builder.log1(arg_vals[0], arg_vals[1], arg_vals[2]);
                builder.imm_u64(0)
            }
            Builtin::YulLog2 => {
                builder.log2(arg_vals[0], arg_vals[1], arg_vals[2], arg_vals[3]);
                builder.imm_u64(0)
            }
            Builtin::YulLog3 => {
                builder.log3(arg_vals[0], arg_vals[1], arg_vals[2], arg_vals[3], arg_vals[4]);
                builder.imm_u64(0)
            }
            Builtin::YulLog4 => {
                builder.log4(
                    arg_vals[0],
                    arg_vals[1],
                    arg_vals[2],
                    arg_vals[3],
                    arg_vals[4],
                    arg_vals[5],
                );
                builder.imm_u64(0)
            }
            Builtin::YulRevert => {
                builder.revert(arg_vals[0], arg_vals[1]);
                builder.imm_u64(0)
            }
            Builtin::YulStop => {
                builder.stop();
                builder.imm_u64(0)
            }
            Builtin::YulInvalid => {
                builder.invalid();
                builder.imm_u64(0)
            }
            Builtin::YulSelfdestruct => {
                builder.selfdestruct(arg_vals[0]);
                builder.imm_u64(0)
            }
            Builtin::YulPop => builder.imm_u64(0),
            Builtin::YulClz
            | Builtin::YulCallcode
            | Builtin::YulExtcall
            | Builtin::YulExtdelegatecall
            | Builtin::YulExtstaticcall
            | Builtin::YulReturn => self.unsupported_yul_builtin(builder, builtin, args.span),
            _ => unreachable!("non-Yul builtin passed to Yul lowering"),
        }
    }

    fn yul_builtin_arity(builtin: Builtin) -> Option<usize> {
        Some(match builtin {
            Builtin::YulStop
            | Builtin::YulAddress
            | Builtin::YulSelfbalance
            | Builtin::YulCaller
            | Builtin::YulCallvalue
            | Builtin::YulCalldatasize
            | Builtin::YulCodesize
            | Builtin::YulReturndatasize
            | Builtin::YulMsize
            | Builtin::YulGas
            | Builtin::YulInvalid
            | Builtin::YulChainid
            | Builtin::YulBasefee
            | Builtin::YulBlobbasefee
            | Builtin::YulCoinbase
            | Builtin::YulDifficulty
            | Builtin::YulPrevrandao
            | Builtin::YulGaslimit
            | Builtin::YulNumber
            | Builtin::YulTimestamp
            | Builtin::YulGasprice
            | Builtin::YulOrigin => 0,
            Builtin::YulNot
            | Builtin::YulIszero
            | Builtin::YulClz
            | Builtin::YulBalance
            | Builtin::YulCalldataload
            | Builtin::YulExtcodesize
            | Builtin::YulExtcodehash
            | Builtin::YulMload
            | Builtin::YulSload
            | Builtin::YulTload
            | Builtin::YulBlobhash
            | Builtin::YulBlockhash
            | Builtin::YulPop
            | Builtin::YulSelfdestruct => 1,
            Builtin::YulAdd
            | Builtin::YulSub
            | Builtin::YulMul
            | Builtin::YulDiv
            | Builtin::YulMod
            | Builtin::YulExp
            | Builtin::YulAnd
            | Builtin::YulOr
            | Builtin::YulXor
            | Builtin::YulShl
            | Builtin::YulShr
            | Builtin::YulSar
            | Builtin::YulSdiv
            | Builtin::YulSmod
            | Builtin::YulLt
            | Builtin::YulGt
            | Builtin::YulSlt
            | Builtin::YulSgt
            | Builtin::YulEq
            | Builtin::YulByte
            | Builtin::YulSignextend
            | Builtin::YulKeccak256
            | Builtin::YulMstore
            | Builtin::YulMstore8
            | Builtin::YulSstore
            | Builtin::YulTstore
            | Builtin::YulLog0
            | Builtin::YulReturn
            | Builtin::YulRevert => 2,
            Builtin::YulAddmod
            | Builtin::YulMulmod
            | Builtin::YulCalldatacopy
            | Builtin::YulCodecopy
            | Builtin::YulReturndatacopy
            | Builtin::YulMcopy
            | Builtin::YulLog1
            | Builtin::YulCreate
            | Builtin::YulExtdelegatecall
            | Builtin::YulExtstaticcall => 3,
            Builtin::YulExtcodecopy
            | Builtin::YulLog2
            | Builtin::YulCreate2
            | Builtin::YulExtcall => 4,
            Builtin::YulLog3 => 5,
            Builtin::YulDelegatecall | Builtin::YulStaticcall | Builtin::YulLog4 => 6,
            Builtin::YulCall | Builtin::YulCallcode => 7,
            _ => return None,
        })
    }

    fn unsupported_yul_builtin(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        builtin: Builtin,
        span: Span,
    ) -> ValueId {
        self.gcx
            .dcx()
            .err(format!("unsupported Yul builtin `{}`", builtin.name()))
            .span(span)
            .emit();
        builder.imm_u64(0)
    }

    /// Lowers a member function call (e.g., counter.increment()).
    fn lower_member_call_with_opts(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        base: &hir::Expr<'_>,
        member: Ident,
        args: &CallArgs<'_>,
        call_opts: Option<&[hir::NamedArg<'_>]>,
    ) -> ValueId {
        // Handle builtin member calls: abi.encode(), abi.encodePacked(), etc.
        if let ExprKind::Ident(res_slice) = &base.kind
            && let Some(hir::Res::Builtin(base_builtin)) = res_slice.first()
            && let Some(member_builtin) = self.resolve_builtin_member(*base_builtin, member.name)
        {
            return self.lower_builtin_call(builder, member_builtin, args);
        }

        // Handle library function calls: Library.func(args)
        // The base is an Ident resolving to a ContractId for a library
        if let ExprKind::Ident(res_slice) = &base.kind
            && let Some(hir::Res::Item(hir::ItemId::Contract(contract_id))) = res_slice.first()
        {
            let contract = self.gcx.hir.contract(*contract_id);

            // Check if member is a struct type defined in this contract: Contract.StructType(args)
            for &item_id in contract.items {
                if let hir::ItemId::Struct(struct_id) = item_id {
                    let strukt = self.gcx.hir.strukt(struct_id);
                    if strukt.name.name == member.name {
                        return self.lower_struct_constructor(builder, struct_id, args);
                    }
                }
            }

            if contract.kind.is_library() {
                // Find the library function by name, matching argument count for overloads
                let arg_count = args.exprs().count();
                if let Some(func_id) =
                    self.find_library_function(*contract_id, member.name, arg_count)
                {
                    return self.lower_library_call(builder, func_id, args, None);
                }
            }
        }

        // Handle address payable transfer/send builtins
        // Only treat as address builtins if base is NOT a contract type
        let member_name = member.name.as_str();
        if (member_name == "transfer" || member_name == "send") && !self.is_contract_type_expr(base)
        {
            // payable(addr).transfer(amount) or payable(addr).send(amount)
            // CALL(2300, addr, amount, 0, 0, 0, 0)
            let addr = self.lower_expr(builder, base);
            let mut exprs = args.exprs();
            let amount = if let Some(first) = exprs.next() {
                self.lower_expr(builder, first)
            } else {
                builder.imm_u64(0)
            };

            // transfer/send uses 2300 gas stipend
            let gas_stipend = builder.imm_u64(2300);
            // Create fresh zero values for each CALL argument to avoid stack issues
            let zero_args_offset = builder.imm_u64(0);
            let zero_args_size = builder.imm_u64(0);
            let zero_ret_offset = builder.imm_u64(0);
            let zero_ret_size = builder.imm_u64(0);

            // CALL(gas, addr, value, argsOffset, argsSize, retOffset, retSize)
            let success = builder.call(
                gas_stipend,
                addr,
                amount,
                zero_args_offset,
                zero_args_size,
                zero_ret_offset,
                zero_ret_size,
            );

            if member_name == "transfer" {
                // transfer reverts on failure
                let is_failure = builder.iszero(success);
                let revert_block = builder.create_block();
                let continue_block = builder.create_block();
                builder.branch(is_failure, revert_block, continue_block);
                builder.switch_to_block(revert_block);
                let revert_offset = builder.imm_u64(0);
                let revert_size = builder.imm_u64(0);
                builder.revert(revert_offset, revert_size);
                builder.switch_to_block(continue_block);
                return builder.imm_u64(0);
            }
            // send returns success bool
            return success;
        }

        // Handle low-level call/staticcall/delegatecall
        // addr.call{value: X}(data) returns (bool success, bytes memory returndata)
        // addr.staticcall(data) returns (bool success, bytes memory returndata)
        // addr.delegatecall(data) returns (bool success, bytes memory returndata)
        if member_name == "call" || member_name == "staticcall" || member_name == "delegatecall" {
            let addr = self.lower_expr(builder, base);

            // Get the calldata bytes argument.
            let mut exprs = args.exprs();
            let (calldata_offset, calldata_size) = if let Some(data_arg) = exprs.next() {
                // Supported inputs are literals and ABI encode calls. Other
                // bytes expressions panic in `lower_bytes_arg_to_memory`.
                self.lower_bytes_arg_to_memory(builder, data_arg)
            } else {
                // No argument means empty calldata
                (builder.imm_u64(0), builder.imm_u64(0))
            };

            // Gas: use all available gas
            let gas = builder.gas();

            // Value: extract from call options {value: X} or default to 0
            let value = if member_name == "call" {
                self.extract_call_value(builder, call_opts)
            } else {
                // staticcall and delegatecall don't transfer value
                builder.imm_u64(0)
            };

            // This lowering models only the success flag. Solidity's second
            // `bytes` result is rejected by `lower_multi_var_decl` until the
            // compiler materializes returndata bytes.
            let ret_offset = builder.imm_u64(0);
            let ret_size = builder.imm_u64(0);

            // Emit the appropriate CALL/STATICCALL/DELEGATECALL instruction
            let success = match member_name {
                "call" => builder.call(
                    gas,
                    addr,
                    value,
                    calldata_offset,
                    calldata_size,
                    ret_offset,
                    ret_size,
                ),
                "staticcall" => builder.staticcall(
                    gas,
                    addr,
                    calldata_offset,
                    calldata_size,
                    ret_offset,
                    ret_size,
                ),
                "delegatecall" => builder.delegatecall(
                    gas,
                    addr,
                    calldata_offset,
                    calldata_size,
                    ret_offset,
                    ret_size,
                ),
                _ => unreachable!(),
            };

            // Low-level calls return `(bool, bytes)`, but this expression path
            // exposes only the first value. `lower_multi_var_decl` copies the
            // returndata bytes out of the return buffer when they are bound.
            return success;
        }

        // Handle storage `bytes`/`string` methods before the generic member
        // call path. Their storage layout is Solidity's packed short/long
        // bytes form, not the generic dynamic-array layout.
        if self.is_storage_bytes_expr(base)
            && matches!(member_name, "push" | "pop")
            && let Some(slot) = self.lower_lvalue_slot(builder, base)
        {
            return self.lower_storage_bytes_method_call(builder, slot, member_name, args);
        }

        // Handle dynamic array methods (push, pop)
        if let Some((var_id, slot)) = self.get_dyn_array_base_slot(base) {
            return self.lower_array_method_call(builder, var_id, slot, member_name, args);
        }

        // Handle `using X for Y` library calls: x.method(args) -> Library.method(x, args)
        if let Some(func_id) = self.resolve_using_directive_call(base, member.name) {
            let bound_arg = self.lower_expr(builder, base);
            return self.lower_library_call(builder, func_id, args, Some(bound_arg));
        }

        // Look up the function being called to get its selector and return count
        let selector = self.compute_member_selector(base, member);
        let num_returns = self.get_member_function_return_count(base, member);

        // Collect argument info: for structs we need the field count, for scalars just 1 slot
        let arg_infos: Vec<_> = args
            .exprs()
            .map(|arg| {
                let struct_info = self.get_expr_struct_info(arg);
                (arg, struct_info)
            })
            .collect();

        // Calculate calldata size: 4 bytes selector + sum of all argument slots
        let total_arg_slots: usize =
            arg_infos.iter().map(|(_, info)| info.map(|(_, n)| n).unwrap_or(1)).sum();
        let calldata_size_bytes = 4 + total_arg_slots * 32;

        // IMPORTANT: Evaluate all arguments FIRST before writing to memory.
        // For structs, lower_expr returns the memory pointer.
        let arg_vals: Vec<ValueId> =
            arg_infos.iter().map(|(arg, _)| self.lower_expr(builder, arg)).collect();

        // Evaluate the address and spill it to scratch memory at 0x00.
        // This ensures it survives all the MSTORE operations for calldata setup.
        // We reload it right before the CALL.
        let addr_expr = self.lower_expr(builder, base);
        let scratch_addr = builder.imm_u64(0x00);
        builder.mstore(scratch_addr, addr_expr);

        // Allocate calldata from the free memory pointer (like solc does).
        // This avoids clobbering the free memory pointer at 0x40 when encoding
        // calldata with 2+ arguments (which would span 0x04-0x43+).
        let free_ptr_addr = builder.imm_u64(0x40);
        let calldata_start = builder.mload(free_ptr_addr);

        // Store calldata_start to scratch memory at 0x20.
        // We need to reload it right before the CALL because:
        // 1. The scheduler may lose track of this value after many MSTOREs
        // 2. For struct returns, we update the free memory pointer, so reading 0x40 again would be
        //    wrong
        let scratch_calldata = builder.imm_u64(0x20);
        builder.mstore(scratch_calldata, calldata_start);

        // Write the selector at calldata_start (left-aligned in 32-byte word)
        let selector_word = U256::from(selector) << 224;
        let selector_val = builder.imm_u256(selector_word);
        builder.mstore(calldata_start, selector_val);

        // Write arguments after selector
        // For struct arguments, we need to load each field from memory and write them
        let mut arg_offset = 4u64;
        for (i, arg_val) in arg_vals.iter().enumerate() {
            let struct_info = &arg_infos[i].1;

            if let Some((_, field_count)) = struct_info {
                // Struct argument: load each field from memory and write to calldata
                for field_idx in 0..*field_count {
                    let field_mem_offset = (field_idx as u64) * 32;
                    let field_val = if field_mem_offset == 0 {
                        builder.mload(*arg_val)
                    } else {
                        let field_offset_val = builder.imm_u64(field_mem_offset);
                        let field_addr = builder.add(*arg_val, field_offset_val);
                        builder.mload(field_addr)
                    };

                    let offset_val = builder.imm_u64(arg_offset);
                    let write_addr = builder.add(calldata_start, offset_val);
                    builder.mstore(write_addr, field_val);
                    arg_offset += 32;
                }
            } else {
                // Scalar argument: write directly
                let offset_val = builder.imm_u64(arg_offset);
                let write_addr = builder.add(calldata_start, offset_val);
                builder.mstore(write_addr, *arg_val);
                arg_offset += 32;
            }
        }

        // Check if the return type is a struct - need special handling for return data
        let struct_return_info = self.get_member_function_struct_return(base, member);

        // Determine where to store return data and whether it's a struct
        let (ret_offset, ret_size, struct_ptr_opt) =
            if let Some((_struct_id, field_count)) = struct_return_info {
                // For struct returns: allocate space after calldata for the return value
                let struct_size = (field_count as u64) * 32;
                let calldata_end_offset = builder.imm_u64(calldata_size_bytes as u64);
                let struct_ptr = builder.add(calldata_start, calldata_end_offset);

                // Update free memory pointer past the struct
                let struct_size_val = builder.imm_u64(struct_size);
                let new_free_ptr = builder.add(struct_ptr, struct_size_val);
                builder.mstore(free_ptr_addr, new_free_ptr);

                let ret_size = builder.imm_u64(struct_size);
                (struct_ptr, ret_size, Some(struct_ptr))
            } else {
                // For non-struct returns: use scratch space at offset 0
                // (safe because we're done with calldata after the CALL)
                let ret_offset = builder.imm_u64(0);
                let ret_size = builder.imm_u64((num_returns * 32) as u64);
                (ret_offset, ret_size, None)
            };

        // Total calldata size = 4 (selector) + 32 * num_args
        let calldata_size = builder.imm_u64(calldata_size_bytes as u64);

        // Value: extract from call options {value: X} or default to 0
        let value = self.extract_call_value(builder, call_opts);

        // Reload the address from scratch memory (0x00) where we stored it earlier.
        // This avoids stack depth issues after all the MSTORE operations.
        let scratch_addr_reload = builder.imm_u64(0x00);
        let addr = builder.mload(scratch_addr_reload);

        // Gas: use all available gas (must be right before CALL to be on top of stack)
        let gas = builder.gas();

        // Reload calldata_start from scratch memory at 0x20.
        // Cannot re-read from 0x40 because struct return handling may have updated it.
        let scratch_calldata_reload = builder.imm_u64(0x20);
        let calldata_start_reload = builder.mload(scratch_calldata_reload);

        // Emit the CALL instruction
        let _success = builder.call(
            gas,
            addr,
            value,
            calldata_start_reload,
            calldata_size,
            ret_offset,
            ret_size,
        );

        // For struct returns, the data is already in the right place (at struct_ptr).
        // Just return the pointer.
        if let Some(struct_ptr) = struct_ptr_opt {
            return struct_ptr;
        }

        // Load first return value from memory
        // Note: for multi-return calls, lower_multi_var_decl will read additional values
        // from memory at offsets 32, 64, etc.
        builder.mload(ret_offset)
    }

    /// Extracts the `value` from call options `{value: X}`, or returns 0 if not present.
    pub(super) fn extract_call_value(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        call_opts: Option<&[hir::NamedArg<'_>]>,
    ) -> ValueId {
        if let Some(opts) = call_opts {
            for opt in opts {
                if opt.name.name.as_str() == "value" {
                    return self.lower_expr(builder, &opt.value);
                }
            }
        }
        builder.imm_u64(0)
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
    fn get_dyn_array_base_slot(&self, expr: &hir::Expr<'_>) -> Option<(hir::VariableId, u64)> {
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
        let elem_slots = self.calculate_storage_slots_for_type(&arr.element);
        if let Some(&slot) = self.storage_slots.get(var_id) {
            return Some((builder.imm_u64(slot), fixed_len, elem_slots));
        }
        if self.storage_ref_locals.contains(var_id) {
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

    /// Checks if an expression is a dynamically-sized calldata parameter (dynamic array or
    /// bytes/string) and returns its ABI head value (the offset, relative to the start of the
    /// args after the 4-byte selector, of its length word) and whether it is bytes/string.
    ///
    /// Fixed-size calldata array parameters are not ABI heads: they are decoded to memory in
    /// the function prologue and take the regular memory path.
    pub(super) fn calldata_dyn_head(&self, expr: &hir::Expr<'_>) -> Option<(ValueId, bool)> {
        let ExprKind::Ident(res_slice) = &expr.kind else { return None };
        let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first() else {
            return None;
        };
        let var = self.gcx.hir.variable(*var_id);
        if var.data_location != Some(solar_ast::DataLocation::Calldata) {
            return None;
        }
        let head = self.locals.get(var_id).copied()?;
        match &var.ty.kind {
            hir::TypeKind::Array(arr) if arr.size.is_none() => Some((head, false)),
            hir::TypeKind::Elementary(hir::ElementaryType::Bytes | hir::ElementaryType::String) => {
                Some((head, true))
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
    fn lower_struct_constructor(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        struct_id: hir::StructId,
        args: &CallArgs<'_>,
    ) -> ValueId {
        let strukt = self.gcx.hir.strukt(struct_id);
        let num_fields = strukt.fields.len();

        // Allocate memory for the struct (each field is 32 bytes)
        let struct_size = (num_fields as u64) * 32;
        let struct_ptr = self.allocate_memory(builder, struct_size);

        // Store each argument into the corresponding field
        for (i, arg) in args.exprs().enumerate() {
            if i >= num_fields {
                break;
            }
            let field_val = self.lower_expr(builder, arg);
            let field_offset = (i as u64) * 32;
            if field_offset == 0 {
                builder.mstore(struct_ptr, field_val);
            } else {
                let offset_val = builder.imm_u64(field_offset);
                let field_addr = builder.add(struct_ptr, offset_val);
                builder.mstore(field_addr, field_val);
            }
        }

        // Return the pointer to the struct
        struct_ptr
    }

    /// Allocates memory for a given size and returns the pointer.
    /// Uses the Solidity free memory pointer pattern.
    pub(super) fn allocate_memory(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        size: u64,
    ) -> ValueId {
        // Load free memory pointer from 0x40
        let free_ptr_addr = builder.imm_u64(0x40);
        let ptr = builder.mload(free_ptr_addr);

        // Update free memory pointer: free_ptr + size
        let size_val = builder.imm_u64(size);
        let new_free_ptr = builder.add(ptr, size_val);
        let free_ptr_addr2 = builder.imm_u64(0x40);
        builder.mstore(free_ptr_addr2, new_free_ptr);

        ptr
    }

    /// Lowers `abi.decode(data, (T...))` for elementary values from memory
    /// `bytes`: the first decoded value is returned and additional values are
    /// written to the same scratch slots used by multi-return calls. Dynamic
    /// `bytes`/`string` values are copied into fresh memory bytes.
    ///
    /// Like solc, a word that is not a clean value of `T` reverts with empty
    /// returndata instead of being silently truncated.
    fn lower_abi_decode(
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
        // A calldata `bytes`/`string` parameter lowers to its ABI head, so copy
        // it into memory first. Other calldata sources (e.g. `msg.data`, slices)
        // don't have a head we can materialize, so reject them rather than
        // silently decode garbage.
        let ptr = if let Some((head, _)) = self.calldata_dyn_head(data) {
            self.materialize_calldata_bytes(builder, head)
        } else if self.expr_is_calldata_dynamic_bytes(data) {
            self.gcx
                .dcx()
                .err("codegen does not support `abi.decode` from this calldata source yet")
                .span(data.span)
                .emit();
            return builder.imm_u64(0);
        } else {
            self.lower_expr(builder, data)
        };
        let word = builder.imm_u64(32);
        let len = builder.mload(ptr);
        let head_size = (elems.len() * 32) as u64;
        let required = builder.imm_u64(head_size);
        let is_short = builder.lt(len, required);
        self.emit_abi_decode_revert_if(builder, is_short);

        let data_start = builder.add(ptr, word);
        let mut first = None;
        for (i, elem) in elems.iter().enumerate() {
            let addr = self.offset_ptr(builder, data_start, (i * 32) as u64);
            let value = builder.mload(addr);
            let decoded = if matches!(elem, ElementaryType::Bytes | ElementaryType::String) {
                self.lower_abi_decode_dynamic_bytes(builder, data_start, len, head_size, value)
            } else {
                self.lower_abi_decode_word(builder, elem, value)
            };
            if i == 0 {
                first = Some(decoded);
            } else {
                let out = builder.imm_u64((i * 32) as u64);
                builder.mstore(out, decoded);
            }
        }
        first.unwrap_or_else(|| builder.imm_u64(0))
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
        let ptr = self.allocate_memory_dynamic(builder, total_size);
        builder.mstore(ptr, tail_len);

        let data_ptr = builder.add(ptr, word);
        let zero = builder.imm_u64(0);
        let last_word_offset = builder.sub(data_size, word);
        let last_word = builder.add(data_ptr, last_word_offset);
        builder.mstore(last_word, zero);

        let src = builder.add(tail_len_addr, word);
        self.mcopy(builder, data_ptr, src, tail_len, None);
        ptr
    }

    fn emit_abi_decode_revert_if(&mut self, builder: &mut FunctionBuilder<'_>, cond: ValueId) {
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
            && self.storage_ref_locals.contains(var_id)
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
    fn struct_id_of_expr(&self, expr: &hir::Expr<'_>) -> Option<hir::StructId> {
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
        let base_slot = self.lower_lvalue_slot(builder, base)?;
        let field_offset = self.get_struct_field_slot_offset(struct_id, field_index);
        Some(if field_offset == 0 {
            base_slot
        } else {
            let off = builder.imm_u64(field_offset);
            builder.add(base_slot, off)
        })
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
                    if self.storage_ref_locals.contains(var_id) {
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

        if let Some(ty) = self.get_expr_type(base)
            && let solar_sema::ty::TyKind::Struct(struct_id) = ty.kind
        {
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

    /// Helper to get memory struct info with type for recursive traversal.
    /// Returns (var_id, byte_offset, struct_id_of_field_type) if the member is a struct field.
    fn compute_nested_memory_struct_info_with_type(
        &mut self,
        expr: &hir::Expr<'_>,
    ) -> Option<(hir::VariableId, u64, Option<hir::StructId>)> {
        if let ExprKind::Member(base, member) = &expr.kind {
            // Base case: base is a memory struct variable
            if let ExprKind::Ident(res_slice) = &base.kind
                && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
            {
                let var = self.gcx.hir.variable(*var_id);
                if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind {
                    // Ensure this is a memory struct, not storage
                    if !self.struct_storage_base_slots.contains_key(var_id) {
                        let strukt = self.gcx.hir.strukt(*struct_id);
                        for (i, &field_id) in strukt.fields.iter().enumerate() {
                            let field = self.gcx.hir.variable(field_id);
                            if let Some(field_name) = field.name
                                && field_name.name == member.name
                            {
                                let offset = self.get_struct_field_memory_offset(*struct_id, i);
                                if let hir::TypeKind::Custom(hir::ItemId::Struct(inner_struct_id)) =
                                    &field.ty.kind
                                {
                                    return Some((*var_id, offset, Some(*inner_struct_id)));
                                }
                                return Some((*var_id, offset, None));
                            }
                        }
                    }
                }
            }

            // Recursive case: base is itself a nested member access
            if let Some((var_id, parent_offset, Some(parent_struct_id))) =
                self.compute_nested_memory_struct_info_with_type(base)
            {
                let parent_strukt = self.gcx.hir.strukt(parent_struct_id);
                for (i, &field_id) in parent_strukt.fields.iter().enumerate() {
                    let field = self.gcx.hir.variable(field_id);
                    if let Some(field_name) = field.name
                        && field_name.name == member.name
                    {
                        let field_offset = self.get_struct_field_memory_offset(parent_struct_id, i);
                        let total_offset = parent_offset + field_offset;

                        if let hir::TypeKind::Custom(hir::ItemId::Struct(inner_struct_id)) =
                            &field.ty.kind
                        {
                            return Some((var_id, total_offset, Some(*inner_struct_id)));
                        }
                        return Some((var_id, total_offset, None));
                    }
                }
            }
        }
        None
    }

    /// Computes the memory byte offset for a nested memory struct member access.
    /// For expressions like `o.l2.l1.a` where `o` is a memory struct with arbitrarily deep nesting.
    /// Returns (base_variable_id, total_byte_offset, inner_struct_id) if successful.
    fn compute_nested_memory_struct_info(
        &mut self,
        base: &hir::Expr<'_>,
        member: Ident,
    ) -> Option<(hir::VariableId, u64, hir::StructId)> {
        // Get the info for the base expression (which should be a struct-typed field)
        if let Some((var_id, parent_offset, Some(parent_struct_id))) =
            self.compute_nested_memory_struct_info_with_type(base)
        {
            // Find the final member within the parent struct
            let parent_strukt = self.gcx.hir.strukt(parent_struct_id);
            for (i, &field_id) in parent_strukt.fields.iter().enumerate() {
                let field = self.gcx.hir.variable(field_id);
                if let Some(field_name) = field.name
                    && field_name.name == member.name
                {
                    let field_offset = self.get_struct_field_memory_offset(parent_struct_id, i);
                    return Some((var_id, parent_offset + field_offset, parent_struct_id));
                }
            }
        }

        // Fallback: try the original 2-level approach for backward compatibility
        if let ExprKind::Member(inner_base, inner_member) = &base.kind
            && let ExprKind::Ident(res_slice) = &inner_base.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            let var = self.gcx.hir.variable(*var_id);
            if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind
                && !self.struct_storage_base_slots.contains_key(var_id)
            {
                let strukt = self.gcx.hir.strukt(*struct_id);
                for (i, &field_id) in strukt.fields.iter().enumerate() {
                    let field = self.gcx.hir.variable(field_id);
                    if let Some(field_name) = field.name
                        && field_name.name == inner_member.name
                        && let hir::TypeKind::Custom(hir::ItemId::Struct(inner_struct_id)) =
                            &field.ty.kind
                    {
                        let base_offset = self.get_struct_field_memory_offset(*struct_id, i);

                        let inner_strukt = self.gcx.hir.strukt(*inner_struct_id);
                        for (j, &inner_field_id) in inner_strukt.fields.iter().enumerate() {
                            let inner_field = self.gcx.hir.variable(inner_field_id);
                            if let Some(inner_field_name) = inner_field.name
                                && inner_field_name.name == member.name
                            {
                                let inner_offset =
                                    self.get_struct_field_memory_offset(*inner_struct_id, j);
                                return Some((
                                    *var_id,
                                    base_offset + inner_offset,
                                    *inner_struct_id,
                                ));
                            }
                        }
                    }
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
    fn lower_array_method_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        _var_id: hir::VariableId,
        slot: u64,
        method: &str,
        args: &CallArgs<'_>,
    ) -> ValueId {
        let slot_val = builder.imm_u64(slot);

        match method {
            "push" => {
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
            "pop" => {
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
            _ => {
                // Unknown method, fall back to dummy
                builder.imm_u64(0)
            }
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
        if let Some((var_id, slot)) = self.get_mapping_base_slot(base) {
            let index_val = self.lower_index_or_zero(builder, index);
            let slot_val = builder.imm_u64(slot);
            let var = self.gcx.hir.variable(var_id);
            let (key_is_dynamic, value_is_mapping) =
                if let hir::TypeKind::Mapping(map) = &var.ty.kind {
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
            return Some(MappingElementSlot { slot, value_is_mapping });
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
        // Store key at memory offset 0
        let mem_0 = builder.imm_u64(0);
        builder.mstore(mem_0, key);

        // Store slot at memory offset 32
        let mem_32 = builder.imm_u64(32);
        builder.mstore(mem_32, slot);

        // Compute keccak256 of 64 bytes starting at offset 0
        let size_64 = builder.imm_u64(64);
        builder.keccak256(mem_0, size_64)
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
            // Storage `bytes`/`string` state variable: its lowering already
            // materialized a `[length][data...]` memory copy in `key`.
            if self.expr_yields_memory_bytes(expr) || self.is_storage_bytes_expr(expr) {
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
            && self.storage_ref_locals.contains(var_id)
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
        let free_mem_ptr_slot = builder.imm_u64(0x40);
        let scratch = builder.mload(free_mem_ptr_slot);
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
        let len = builder.mload(ptr);
        let word_size = builder.imm_u64(32);
        let data_start = builder.add(ptr, word_size);
        let free_mem_ptr_slot = builder.imm_u64(0x40);
        let scratch = builder.mload(free_mem_ptr_slot);
        self.mcopy(builder, scratch, data_start, len, None);
        let slot_addr = builder.add(scratch, len);
        builder.mstore(slot_addr, slot);
        let hash_len = builder.add(len, word_size);
        builder.keccak256(scratch, hash_len)
    }

    fn compute_dynamic_calldata_mapping_slot(
        &self,
        builder: &mut FunctionBuilder<'_>,
        head_offset: ValueId,
        slot: ValueId,
    ) -> ValueId {
        let selector_size = builder.imm_u64(4);
        let data_head = builder.add(head_offset, selector_size);
        let len = builder.calldataload(data_head);
        let word_size = builder.imm_u64(32);
        let data_start = builder.add(data_head, word_size);
        // Stage at the unbumped free-memory scratch: staging at offset 0 would
        // clobber the free memory pointer (and live heap) for keys > 32 bytes.
        let free_mem_ptr_slot = builder.imm_u64(0x40);
        let scratch = builder.mload(free_mem_ptr_slot);
        builder.calldatacopy(scratch, data_start, len);
        let slot_addr = builder.add(scratch, len);
        builder.mstore(slot_addr, slot);
        let hash_len = builder.add(len, word_size);
        builder.keccak256(scratch, hash_len)
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

    /// Computes the function selector for a member call.
    pub(super) fn compute_member_selector(&self, base: &hir::Expr<'_>, member: Ident) -> u32 {
        // Try to get the type of the base expression and find the function
        // For contract types, we look up the function in the contract's interface

        // Helper to look up selector from a contract, including inherited functions.
        // Searches through the linearized inheritance chain.
        let lookup_in_contract = |contract_id: hir::ContractId| -> Option<u32> {
            let contract = self.gcx.hir.contract(contract_id);
            // Search through the inheritance chain (linearized_bases includes self at index 0)
            for &base_id in contract.linearized_bases.iter() {
                let base_contract = self.gcx.hir.contract(base_id);
                for func_id in base_contract.all_functions() {
                    let func = self.gcx.hir.function(func_id);
                    if func.name.is_some_and(|n| n.name == member.name) {
                        let selector = self.gcx.function_selector(func_id);
                        return Some(u32::from_be_bytes(selector.0));
                    }
                }
            }
            None
        };

        // Case 1: base is an identifier (variable with contract type)
        if let ExprKind::Ident(res_slice) = &base.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            let var = self.gcx.hir.variable(*var_id);
            let ty = self.gcx.type_of_hir_ty(&var.ty);
            if let solar_sema::ty::TyKind::Contract(contract_id) = ty.kind
                && let Some(sel) = lookup_in_contract(contract_id)
            {
                return sel;
            }
        }

        // Case 2: base is a type conversion call like ICallee(addr)
        // The call's callee is an Ident resolving to a Contract/Interface
        if let ExprKind::Call(callee, _args, _named) = &base.kind
            && let ExprKind::Ident(res_slice) = &callee.kind
            && let Some(hir::Res::Item(hir::ItemId::Contract(contract_id))) = res_slice.first()
            && let Some(sel) = lookup_in_contract(*contract_id)
        {
            return sel;
        }

        // Case 2b: base is the contract/interface name itself, e.g.
        // `IERC20Minimal.transfer.selector`.
        if let ExprKind::Ident(res_slice) = &base.kind
            && let Some(hir::Res::Item(hir::ItemId::Contract(contract_id))) = res_slice.first()
            && let Some(sel) = lookup_in_contract(*contract_id)
        {
            return sel;
        }

        // Case 3: base is `this` (Builtin::This)
        if let ExprKind::Ident(res_slice) = &base.kind
            && let Some(hir::Res::Builtin(Builtin::This)) = res_slice.first()
            && let Some(contract_id) = self.current_contract_id
            && let Some(sel) = lookup_in_contract(contract_id)
        {
            return sel;
        }

        // Fallback: compute selector from member name
        // This is a simplified version - proper implementation would use full signature
        let sig = format!("{}()", member.name);
        let hash = alloy_primitives::keccak256(sig.as_bytes());
        u32::from_be_bytes(hash[..4].try_into().unwrap())
    }

    /// Gets the number of return values for a member function call.
    pub(super) fn get_member_function_return_count(
        &self,
        base: &hir::Expr<'_>,
        member: Ident,
    ) -> usize {
        // Helper to count the number of 32-byte slots a return type occupies.
        // Structs are expanded to their number of fields.
        let count_return_slots = |returns: &[hir::VariableId]| -> usize {
            let mut total = 0;
            for &var_id in returns {
                let var = self.gcx.hir.variable(var_id);
                if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind {
                    // Struct: count its fields
                    let strukt = self.gcx.hir.strukt(*struct_id);
                    total += strukt.fields.len();
                } else {
                    // Non-struct: 1 slot
                    total += 1;
                }
            }
            total.max(1)
        };

        // Helper to look up return count from a contract, including inherited functions.
        // Searches through the linearized inheritance chain.
        let lookup_in_contract = |contract_id: hir::ContractId| -> Option<usize> {
            let contract = self.gcx.hir.contract(contract_id);
            // Search through the inheritance chain (linearized_bases includes self at index 0)
            for &base_id in contract.linearized_bases.iter() {
                let base_contract = self.gcx.hir.contract(base_id);
                for func_id in base_contract.all_functions() {
                    let func = self.gcx.hir.function(func_id);
                    if func.name.is_some_and(|n| n.name == member.name) {
                        return Some(count_return_slots(func.returns));
                    }
                }
            }
            None
        };

        // Case 1: base is an identifier (variable with contract type)
        if let ExprKind::Ident(res_slice) = &base.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            let var = self.gcx.hir.variable(*var_id);
            let ty = self.gcx.type_of_hir_ty(&var.ty);
            if let solar_sema::ty::TyKind::Contract(contract_id) = ty.kind
                && let Some(count) = lookup_in_contract(contract_id)
            {
                return count;
            }
        }

        // Case 2: base is a type conversion call like ICallee(addr)
        if let ExprKind::Call(callee, _args, _named) = &base.kind
            && let ExprKind::Ident(res_slice) = &callee.kind
            && let Some(hir::Res::Item(hir::ItemId::Contract(contract_id))) = res_slice.first()
            && let Some(count) = lookup_in_contract(*contract_id)
        {
            return count;
        }

        // Case 3: base is `this` (Builtin::This)
        if let ExprKind::Ident(res_slice) = &base.kind
            && let Some(hir::Res::Builtin(Builtin::This)) = res_slice.first()
        {
            // Look up the function in the current contract
            // We need to find it through the module's functions
            // Search all known contracts because `this` carries the current
            // contract value rather than a specific function declaration.
            for contract_id in self.gcx.hir.contract_ids() {
                if let Some(count) = lookup_in_contract(contract_id) {
                    return count;
                }
            }
        }

        // Unknown member calls are treated as single-value calls.
        1
    }

    /// Gets struct return info for a member function call.
    /// Returns Some((struct_id, field_count)) if the function returns a single struct.
    pub(super) fn get_member_function_struct_return(
        &self,
        base: &hir::Expr<'_>,
        member: Ident,
    ) -> Option<(hir::StructId, usize)> {
        // Helper to check if the function returns a single struct and get its info.
        let check_struct_return = |returns: &[hir::VariableId]| -> Option<(hir::StructId, usize)> {
            if returns.len() == 1 {
                let var = self.gcx.hir.variable(returns[0]);
                if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind {
                    let strukt = self.gcx.hir.strukt(*struct_id);
                    return Some((*struct_id, strukt.fields.len()));
                }
            }
            None
        };

        // Helper to look up struct return from a contract, including inherited functions.
        let lookup_in_contract = |contract_id: hir::ContractId| -> Option<(hir::StructId, usize)> {
            let contract = self.gcx.hir.contract(contract_id);
            for &base_id in contract.linearized_bases.iter() {
                let base_contract = self.gcx.hir.contract(base_id);
                for func_id in base_contract.all_functions() {
                    let func = self.gcx.hir.function(func_id);
                    if func.name.is_some_and(|n| n.name == member.name) {
                        return check_struct_return(func.returns);
                    }
                }
            }
            None
        };

        // Case 1: base is an identifier (variable with contract type)
        if let ExprKind::Ident(res_slice) = &base.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            let var = self.gcx.hir.variable(*var_id);
            let ty = self.gcx.type_of_hir_ty(&var.ty);
            if let solar_sema::ty::TyKind::Contract(contract_id) = ty.kind
                && let Some(info) = lookup_in_contract(contract_id)
            {
                return Some(info);
            }
        }

        // Case 2: base is a type conversion call like ICallee(addr)
        if let ExprKind::Call(callee, _args, _named) = &base.kind
            && let ExprKind::Ident(res_slice) = &callee.kind
            && let Some(hir::Res::Item(hir::ItemId::Contract(contract_id))) = res_slice.first()
            && let Some(info) = lookup_in_contract(*contract_id)
        {
            return Some(info);
        }

        // Case 3: base is `this` (Builtin::This)
        if let ExprKind::Ident(res_slice) = &base.kind
            && let Some(hir::Res::Builtin(Builtin::This)) = res_slice.first()
        {
            for contract_id in self.gcx.hir.contract_ids() {
                if let Some(info) = lookup_in_contract(contract_id) {
                    return Some(info);
                }
            }
        }

        None
    }

    /// Resolves a member of a builtin module to its corresponding builtin.
    /// For example, (Builtin::Msg, "sender") -> Some(Builtin::MsgSender)
    fn resolve_builtin_member(&self, base: Builtin, member: Symbol) -> Option<Builtin> {
        match base {
            Builtin::Msg => {
                if member == sym::sender {
                    Some(Builtin::MsgSender)
                } else if member == sym::value {
                    Some(Builtin::MsgValue)
                } else if member == sym::data {
                    Some(Builtin::MsgData)
                } else if member == sym::sig {
                    Some(Builtin::MsgSig)
                } else if member == kw::Gas {
                    Some(Builtin::MsgGas)
                } else {
                    None
                }
            }
            Builtin::Block => {
                if member == kw::Coinbase {
                    Some(Builtin::BlockCoinbase)
                } else if member == kw::Timestamp {
                    Some(Builtin::BlockTimestamp)
                } else if member == kw::Difficulty {
                    Some(Builtin::BlockDifficulty)
                } else if member == kw::Prevrandao {
                    Some(Builtin::BlockPrevrandao)
                } else if member == kw::Number {
                    Some(Builtin::BlockNumber)
                } else if member == kw::Gaslimit {
                    Some(Builtin::BlockGaslimit)
                } else if member == kw::Chainid {
                    Some(Builtin::BlockChainid)
                } else if member == kw::Basefee {
                    Some(Builtin::BlockBasefee)
                } else if member == kw::Blobbasefee {
                    Some(Builtin::BlockBlobbasefee)
                } else {
                    None
                }
            }
            Builtin::Tx => {
                if member == kw::Origin {
                    Some(Builtin::TxOrigin)
                } else if member == kw::Gasprice {
                    Some(Builtin::TxGasPrice)
                } else {
                    None
                }
            }
            Builtin::Abi => {
                if member == sym::encode {
                    Some(Builtin::AbiEncode)
                } else if member == sym::encodePacked {
                    Some(Builtin::AbiEncodePacked)
                } else if member == sym::encodeWithSelector {
                    Some(Builtin::AbiEncodeWithSelector)
                } else if member == sym::encodeCall {
                    Some(Builtin::AbiEncodeCall)
                } else if member == sym::encodeWithSignature {
                    Some(Builtin::AbiEncodeWithSignature)
                } else if member == sym::decode {
                    Some(Builtin::AbiDecode)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Finds a library function by name, preferring the overload matching arg_count.
    fn find_library_function(
        &self,
        library_id: hir::ContractId,
        name: Symbol,
        arg_count: usize,
    ) -> Option<hir::FunctionId> {
        let library = self.gcx.hir.contract(library_id);
        let mut candidates: Vec<hir::FunctionId> = Vec::new();

        for func_id in library.all_functions() {
            let func = self.gcx.hir.function(func_id);
            if func.name.is_some_and(|n| n.name == name) {
                candidates.push(func_id);
            }
        }

        // Select the overload that matches the argument count
        candidates
            .iter()
            .find(|&&fid| {
                let f = self.gcx.hir.function(fid);
                f.parameters.len() == arg_count
            })
            .copied()
            .or_else(|| candidates.first().copied())
    }

    /// Lowers an internal function call by inlining it.
    /// This handles calls like `add(a, b)` where `add` is a function in the same contract.
    fn lower_internal_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func_id: hir::FunctionId,
        args: &CallArgs<'_>,
    ) -> ValueId {
        let func = self.gcx.hir.function(func_id);

        // Collect argument values FIRST (before entering inline tracking)
        // This allows nested calls to the same function (e.g., add(add(x, 1), 2))
        // because we evaluate arguments before marking ourselves as "in progress"
        let arg_vals: Vec<ValueId> =
            args.exprs().map(|arg| self.lower_expr(builder, arg)).collect();

        if func.returns.is_empty() {
            if self.function_is_recursive(func_id) {
                return self.lower_internal_call_fallback(builder, func_id, arg_vals);
            }
            return self.lower_inline_void_call(builder, func_id, arg_vals);
        }

        // The SSA inline path (`lower_library_body_simple`) only models a
        // straight-line body that ends in a `return`. Anything else — a loop, an
        // `if`, a multi-statement control flow — is lowered as a real
        // `internal_call` instead, where the memory-backed internal frame handles
        // reassigned locals, loops, and recursion correctly. Recursive functions
        // with a simple ternary body (which `is_simple_return_function` accepts)
        // are caught separately so inlining does not hit a recursive cycle.
        // Simple, non-recursive functions still inline. Internal/private callees
        // use the internal-frame convention directly; a public callee is compiled
        // for the external ABI, so it needs an internal-frame copy
        // (`ensure_internal_mir_function`) for `internal_call` to target.
        let needs_call =
            !Self::is_simple_return_function(func) || self.function_is_recursive(func_id);
        if needs_call {
            return self.lower_internal_call_fallback(builder, func_id, arg_vals);
        }

        // Check for recursive inlining cycle AFTER evaluating arguments.
        if !self.try_enter_inline(func_id) {
            return self.lower_internal_call_fallback(builder, func_id, arg_vals);
        }

        // Save current locals
        let saved_locals = std::mem::take(&mut self.locals);

        // Bind parameters to argument values directly (SSA style)
        for (i, &param_id) in func.parameters.iter().enumerate() {
            if let Some(&arg_val) = arg_vals.get(i) {
                self.locals.insert(param_id, arg_val);
            }
        }

        // For simple functions with a single return statement, extract and evaluate directly
        let result = if let Some(body) = &func.body {
            self.lower_library_body_simple(builder, body, func)
        } else {
            builder.imm_u64(0)
        };

        // Restore locals
        self.locals = saved_locals;

        // Exit inline tracking
        self.exit_inline();

        result
    }

    fn lower_internal_call_fallback(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func_id: hir::FunctionId,
        arg_vals: Vec<ValueId>,
    ) -> ValueId {
        let func = self.gcx.hir.function(func_id);
        let result_ty = func
            .returns
            .first()
            .map(|&ret_id| self.lower_type_from_var(self.gcx.hir.variable(ret_id)));
        let is_internal =
            matches!(func.visibility, hir::Visibility::Internal | hir::Visibility::Private);
        let mir_id = if is_internal {
            self.ensure_function_lowered(func_id)
        } else {
            self.ensure_internal_mir_function(func_id)
        };
        builder.internal_call(mir_id, arg_vals, result_ty, func.returns.len())
    }

    /// Lowers a base constructor call using already-resolved constructor arguments.
    pub(super) fn lower_base_constructor_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ctor_id: hir::FunctionId,
        modifier: Option<&hir::Modifier<'_>>,
    ) -> ValueId {
        let ctor = self.gcx.hir.function(ctor_id);
        let arg_exprs: Vec<_> = modifier.map(|m| m.args.exprs().collect()).unwrap_or_default();
        let arg_vals: Vec<ValueId> = ctor
            .parameters
            .iter()
            .enumerate()
            .map(|(i, &param_id)| {
                let param = self.gcx.hir.variable(param_id);
                if let Some(arg) = arg_exprs.get(i) {
                    self.lower_constructor_arg(builder, arg, &param.ty)
                } else {
                    builder.imm_u64(0)
                }
            })
            .collect();

        self.lower_inline_void_call(builder, ctor_id, arg_vals)
    }

    /// Lowers a void internal function by inlining its full statement body.
    fn lower_inline_void_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func_id: hir::FunctionId,
        arg_vals: Vec<ValueId>,
    ) -> ValueId {
        let func = self.gcx.hir.function(func_id);
        let parameters: Vec<_> = func.parameters.to_vec();
        let body = func.body;

        if !self.try_enter_inline(func_id) {
            panic!("codegen hit unsupported void-call inline recursion");
        }

        let saved_locals = std::mem::take(&mut self.locals);
        let saved_local_memory_slots = std::mem::take(&mut self.local_memory_slots);
        let saved_next_local_memory_offset = self.next_local_memory_offset;
        let saved_assigned_vars = std::mem::take(&mut self.assigned_vars);

        if let Some(body) = body {
            self.collect_assigned_vars_block(&body);
        }

        for (i, param_id) in parameters.into_iter().enumerate() {
            if let Some(&arg_val) = arg_vals.get(i) {
                self.locals.insert(param_id, arg_val);
            }
        }

        if let Some(body) = body {
            let saved_in_unchecked_block = self.in_unchecked_block;
            self.in_unchecked_block = false;
            self.lower_block(builder, &body);
            self.in_unchecked_block = saved_in_unchecked_block;
        }

        self.locals = saved_locals;
        self.local_memory_slots = saved_local_memory_slots;
        self.next_local_memory_offset = saved_next_local_memory_offset;
        self.assigned_vars = saved_assigned_vars;
        self.exit_inline();

        builder.imm_u64(0)
    }

    /// Lowers constructor arguments into the representation expected by the
    /// callee body. Memory `bytes`/`string` parameters receive Solidity's
    /// `[length][data...]` memory pointer, including literal base-constructor
    /// arguments such as `ERC20("Name", "SYM")`.
    fn lower_constructor_arg(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        arg: &hir::Expr<'_>,
        param_ty: &hir::Type<'_>,
    ) -> ValueId {
        if matches!(
            param_ty.kind,
            hir::TypeKind::Elementary(hir::ElementaryType::String | hir::ElementaryType::Bytes)
        ) {
            return self.lower_expr_as_memory_bytes(builder, arg);
        }

        self.lower_expr(builder, arg)
    }

    /// Lowers an internal library function call by inlining it.
    /// For internal library functions, we inline the function body.
    fn lower_library_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        func_id: hir::FunctionId,
        args: &CallArgs<'_>,
        bound_arg: Option<ValueId>,
    ) -> ValueId {
        let func = self.gcx.hir.function(func_id);

        // For internal library functions, inline the function body
        if func.visibility == hir::Visibility::Internal
            || func.visibility == hir::Visibility::Private
        {
            // Collect argument values FIRST (before entering inline tracking)
            // This allows nested calls to the same function (e.g., add(add(x, 1), 2))
            // because we evaluate arguments before marking ourselves as "in progress"
            let mut arg_vals: Vec<ValueId> = Vec::new();

            // If there's a bound argument (from `using X for T`), it's the first argument
            if let Some(bound_val) = bound_arg {
                arg_vals.push(bound_val);
            }

            // Lower all explicit arguments
            for arg in args.exprs() {
                arg_vals.push(self.lower_expr(builder, arg));
            }

            if func.returns.is_empty() {
                if self.function_is_recursive(func_id) {
                    return self.lower_internal_call_fallback(builder, func_id, arg_vals);
                }
                return self.lower_inline_void_call(builder, func_id, arg_vals);
            }

            if !Self::is_simple_return_function(func) || self.function_is_recursive(func_id) {
                return self.lower_internal_call_fallback(builder, func_id, arg_vals);
            }

            // Check for recursive inlining cycle AFTER evaluating arguments.
            if !self.try_enter_inline(func_id) {
                return self.lower_internal_call_fallback(builder, func_id, arg_vals);
            }

            // Simple inlining: bind parameters directly as SSA values
            // This works for pure functions that don't mutate parameters
            // Save current locals
            let saved_locals = std::mem::take(&mut self.locals);
            let saved_local_memory_slots = std::mem::take(&mut self.local_memory_slots);
            let saved_next_local_memory_offset = self.next_local_memory_offset;
            let saved_assigned_vars = std::mem::take(&mut self.assigned_vars);

            if let Some(body) = &func.body {
                self.collect_assigned_vars_block(body);
            }

            // Bind parameters to argument values directly (SSA style)
            for (i, &param_id) in func.parameters.iter().enumerate() {
                if let Some(&arg_val) = arg_vals.get(i) {
                    self.locals.insert(param_id, arg_val);
                }
            }

            // For simple functions with a single return statement, extract and evaluate directly
            let result = if let Some(body) = &func.body {
                self.lower_library_body_simple(builder, body, func)
            } else {
                builder.imm_u64(0)
            };

            // Restore locals
            self.locals = saved_locals;
            self.local_memory_slots = saved_local_memory_slots;
            self.next_local_memory_offset = saved_next_local_memory_offset;
            self.assigned_vars = saved_assigned_vars;

            // Exit inline tracking
            self.exit_inline();

            result
        } else {
            panic!("codegen does not support external library calls yet")
        }
    }

    fn is_simple_return_function(func: &hir::Function<'_>) -> bool {
        if func.returns.len() != 1 {
            return false;
        }
        let Some(body) = func.body else {
            return false;
        };
        body.stmts.iter().any(|stmt| matches!(stmt.kind, hir::StmtKind::Return(Some(_))))
            && body.stmts.iter().all(|stmt| {
                matches!(
                    stmt.kind,
                    hir::StmtKind::DeclSingle(_)
                        | hir::StmtKind::Expr(_)
                        | hir::StmtKind::Return(Some(_))
                )
            })
    }

    /// Whether `func_id` directly or indirectly calls itself (cached). A recursive function
    /// must be lowered as a real `internal_call` instead of being inlined.
    fn function_is_recursive(&mut self, func_id: hir::FunctionId) -> bool {
        if let Some(&cached) = self.recursive_functions.get(&func_id) {
            return cached;
        }
        let mut visiting = FxHashSet::default();
        let result = self.function_reaches(func_id, func_id, &mut visiting);
        self.recursive_functions.insert(func_id, result);
        result
    }

    fn function_reaches(
        &self,
        current: hir::FunctionId,
        target: hir::FunctionId,
        visiting: &mut FxHashSet<hir::FunctionId>,
    ) -> bool {
        if !visiting.insert(current) {
            return false;
        }

        for callee in self.function_callees(current) {
            if callee == target || self.function_reaches(callee, target, visiting) {
                return true;
            }
        }

        false
    }

    fn function_callees(&self, func_id: hir::FunctionId) -> Vec<hir::FunctionId> {
        let mut callees = Vec::new();
        let func = self.gcx.hir.function(func_id);
        if let Some(body) = func.body {
            for stmt in body.stmts {
                self.stmt_collect_callees(stmt, &mut callees);
            }
        }
        callees
    }

    /// Collects calls contained recursively in a statement.
    fn stmt_collect_callees(&self, stmt: &hir::Stmt<'_>, callees: &mut Vec<hir::FunctionId>) {
        use hir::StmtKind;
        match &stmt.kind {
            StmtKind::Expr(e)
            | StmtKind::Return(Some(e))
            | StmtKind::Revert(e)
            | StmtKind::Emit(e) => self.expr_collect_callees(e, callees),
            StmtKind::Block(b) | StmtKind::UncheckedBlock(b) | StmtKind::AssemblyBlock(b) => {
                for stmt in b.stmts {
                    self.stmt_collect_callees(stmt, callees);
                }
            }
            StmtKind::If(c, t, e) => {
                self.expr_collect_callees(c, callees);
                self.stmt_collect_callees(t, callees);
                if let Some(e) = e {
                    self.stmt_collect_callees(e, callees);
                }
            }
            StmtKind::Loop(b, _) => {
                for stmt in b.stmts {
                    self.stmt_collect_callees(stmt, callees);
                }
            }
            StmtKind::Switch(sw) => {
                self.expr_collect_callees(sw.selector, callees);
                for case in sw.cases {
                    for stmt in case.body.stmts {
                        self.stmt_collect_callees(stmt, callees);
                    }
                }
            }
            StmtKind::Try(t) => {
                self.expr_collect_callees(&t.expr, callees);
                for clause in t.clauses {
                    for stmt in clause.block.stmts {
                        self.stmt_collect_callees(stmt, callees);
                    }
                }
            }
            StmtKind::DeclSingle(var_id) => {
                if let Some(init) = self.gcx.hir.variable(*var_id).initializer {
                    self.expr_collect_callees(init, callees);
                }
            }
            StmtKind::DeclMulti(_, init) => self.expr_collect_callees(init, callees),
            StmtKind::Return(None)
            | StmtKind::Continue
            | StmtKind::Break
            | StmtKind::Placeholder
            | StmtKind::Err(_) => {}
        }
    }

    /// Collects calls contained recursively in an expression.
    fn expr_collect_callees(&self, expr: &hir::Expr<'_>, callees: &mut Vec<hir::FunctionId>) {
        match &expr.kind {
            ExprKind::Call(callee, args, _) => {
                if let ExprKind::Ident(res) = &callee.kind
                    && let Some(hir::Res::Item(hir::ItemId::Function(f))) = res.first()
                {
                    callees.push(*f);
                }
                self.expr_collect_callees(callee, callees);
                for arg in args.exprs() {
                    self.expr_collect_callees(arg, callees);
                }
            }
            ExprKind::Binary(l, _, r) | ExprKind::Assign(l, _, r) => {
                self.expr_collect_callees(l, callees);
                self.expr_collect_callees(r, callees);
            }
            ExprKind::Unary(_, e)
            | ExprKind::Member(e, _)
            | ExprKind::YulMember(e, _)
            | ExprKind::Payable(e)
            | ExprKind::Delete(e) => self.expr_collect_callees(e, callees),
            ExprKind::Ternary(c, t, f) => {
                self.expr_collect_callees(c, callees);
                self.expr_collect_callees(t, callees);
                self.expr_collect_callees(f, callees);
            }
            ExprKind::Index(b, i) => {
                self.expr_collect_callees(b, callees);
                if let Some(i) = i {
                    self.expr_collect_callees(i, callees);
                }
            }
            ExprKind::Slice(b, s, e) => {
                self.expr_collect_callees(b, callees);
                if let Some(s) = s {
                    self.expr_collect_callees(s, callees);
                }
                if let Some(e) = e {
                    self.expr_collect_callees(e, callees);
                }
            }
            ExprKind::Array(es) => {
                for e in *es {
                    self.expr_collect_callees(e, callees);
                }
            }
            ExprKind::Tuple(es) => {
                for e in es.iter().flatten() {
                    self.expr_collect_callees(e, callees);
                }
            }
            ExprKind::New(_)
            | ExprKind::TypeCall(_)
            | ExprKind::Lit(_)
            | ExprKind::Ident(_)
            | ExprKind::Type(_)
            | ExprKind::Err(_) => {}
        }
    }

    /// Lowers a simple library function body.
    /// For functions with a single return statement, directly evaluate the return expression.
    fn lower_library_body_simple(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        body: &hir::Block<'_>,
        func: &hir::Function<'_>,
    ) -> ValueId {
        let saved_in_unchecked_block = self.in_unchecked_block;
        self.in_unchecked_block = false;

        for &return_id in func.returns {
            let zero = builder.imm_u64(0);
            self.locals.insert(return_id, zero);
        }

        let result = if let Some(value) = self.lower_library_block_return(builder, body) {
            value
        } else {
            // Implicit named returns: the body assigned the named return variables
            // (e.g. `success = ...; result = ...;` with no explicit `return`). Write
            // returns 1..N to scratch memory at offset `i * 32` so the caller's
            // `lower_multi_var_decl` (which reads `mload(i * 32)`) recovers them; the
            // first return flows back as the MIR value below.
            if func.returns.len() > 1 {
                for (i, &return_id) in func.returns.iter().enumerate().skip(1) {
                    if let Some(&value) = self.locals.get(&return_id) {
                        let offset = builder.imm_u64((i * 32) as u64);
                        builder.mstore(offset, value);
                    }
                }
            }

            if let Some(&return_id) = func.returns.first()
                && let Some(&value) = self.locals.get(&return_id)
            {
                value
            } else {
                builder.imm_u64(0)
            }
        };

        self.in_unchecked_block = saved_in_unchecked_block;
        result
    }

    fn lower_library_block_return(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        block: &hir::Block<'_>,
    ) -> Option<ValueId> {
        for stmt in block.stmts {
            if let Some(value) = self.lower_library_stmt_return(builder, stmt) {
                return Some(value);
            }
        }
        None
    }

    /// Extract return value from a statement after lowering prior side effects in that statement.
    fn lower_library_stmt_return(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        stmt: &hir::Stmt<'_>,
    ) -> Option<ValueId> {
        match &stmt.kind {
            hir::StmtKind::Return(Some(expr)) => Some(self.lower_expr(builder, expr)),
            hir::StmtKind::Return(None) => Some(builder.imm_u64(0)),
            hir::StmtKind::DeclSingle(var_id) => {
                let var = self.gcx.hir.variable(*var_id);
                let init_val = if let Some(init) = var.initializer {
                    self.lower_expr(builder, init)
                } else {
                    builder.imm_u64(0)
                };
                self.locals.insert(*var_id, init_val);
                None
            }
            hir::StmtKind::Expr(expr) => {
                self.lower_expr(builder, expr);
                None
            }
            hir::StmtKind::Block(block) => self.lower_library_block_return(builder, block),
            hir::StmtKind::UncheckedBlock(block) => self.lower_library_block_return(builder, block),
            hir::StmtKind::If(cond, then_stmt, else_stmt) => {
                let cond_val = self.lower_expr(builder, cond);
                let then_return = self.lower_library_stmt_return(builder, then_stmt);
                let else_return =
                    else_stmt.map(|else_stmt| self.lower_library_stmt_return(builder, else_stmt));

                match (then_return, else_return.flatten()) {
                    (Some(then_val), Some(else_val)) => {
                        Some(builder.select(cond_val, then_val, else_val))
                    }
                    // A one-sided return is an early-return control-flow shape. This helper
                    // returns expression values only, so let later statements provide the
                    // fallthrough value instead of treating the branch as unconditional.
                    _ => None,
                }
            }
            hir::StmtKind::DeclMulti(vars, rhs) => {
                self.lower_multi_var_decl(builder, vars, rhs);
                None
            }
            hir::StmtKind::Loop(..)
            | hir::StmtKind::AssemblyBlock(_)
            | hir::StmtKind::Switch(_)
            | hir::StmtKind::Emit(_)
            | hir::StmtKind::Revert(_)
            | hir::StmtKind::Break
            | hir::StmtKind::Continue
            | hir::StmtKind::Try(_)
            | hir::StmtKind::Placeholder
            | hir::StmtKind::Err(_) => {
                self.lower_stmt(builder, stmt);
                None
            }
        }
    }

    /// Checks if an expression has a contract type (as opposed to address type).
    /// Used to distinguish between address.transfer(amount) and token.transfer(to, amount).
    pub(super) fn is_contract_type_expr(&self, expr: &hir::Expr<'_>) -> bool {
        // Case 1: Variable with contract type (e.g., `token` where `MinimalERC20 token`)
        if let ExprKind::Ident(res_slice) = &expr.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            let var = self.gcx.hir.variable(*var_id);
            let ty = self.gcx.type_of_hir_ty(&var.ty);
            if matches!(ty.kind, solar_sema::ty::TyKind::Contract(_)) {
                return true;
            }
        }

        // Case 2: Type conversion call like IToken(addr)
        if let ExprKind::Call(callee, _, _) = &expr.kind
            && let ExprKind::Ident(res_slice) = &callee.kind
            && let Some(hir::Res::Item(hir::ItemId::Contract(_))) = res_slice.first()
        {
            return true;
        }

        false
    }

    /// Resolves a member call that may be a `using X for Y` library extension call.
    ///
    /// If `base.member` matches a using directive, returns the library function ID.
    /// The caller should pass the evaluated `base` as the first (bound) argument.
    fn resolve_using_directive_call(
        &self,
        base: &hir::Expr<'_>,
        member_name: Symbol,
    ) -> Option<hir::FunctionId> {
        let contract_id = self.current_contract_id?;
        let contract = self.gcx.hir.contract(contract_id);

        // Get the type of the base expression
        let base_ty = self.get_expr_type(base)?;

        // Search through source- and contract-level using directives.
        for using in self.gcx.hir.source(contract.source).usings.iter().chain(contract.usings) {
            // Check if this using directive applies to the base type
            let type_matches = if let Some(ref target_ty) = using.ty {
                // Check if the types match
                self.types_match_for_using(&base_ty, target_ty)
            } else {
                // `using X for *` matches all types
                true
            };

            if !type_matches {
                continue;
            }

            for entry in using.entries {
                if entry.operator.is_some() {
                    continue;
                }

                match &entry.kind {
                    hir::UsingEntryKind::Library(library) => {
                        for func_id in self.gcx.hir.contract(*library).functions() {
                            let func = self.gcx.hir.function(func_id);
                            if func.name.map(|n| n.name) != Some(member_name) {
                                continue;
                            }
                            if self.using_function_matches(&base_ty, func_id) {
                                return Some(func_id);
                            }
                        }
                    }
                    hir::UsingEntryKind::Functions(functions) => {
                        for &func_id in *functions {
                            let func = self.gcx.hir.function(func_id);
                            let name = entry.name.or_else(|| func.name.map(|n| n.name));
                            if name != Some(member_name) {
                                continue;
                            }
                            if self.using_function_matches(&base_ty, func_id) {
                                return Some(func_id);
                            }
                        }
                    }
                    hir::UsingEntryKind::Err(_) => {}
                }
            }
        }

        None
    }

    fn using_function_matches(
        &self,
        base_ty: &solar_sema::ty::Ty<'_>,
        func_id: hir::FunctionId,
    ) -> bool {
        let func = self.gcx.hir.function(func_id);
        if !matches!(func.visibility, hir::Visibility::Internal | hir::Visibility::Private) {
            return false;
        }

        let Some(&first_param_id) = func.parameters.first() else {
            return false;
        };
        let first_param = self.gcx.hir.variable(first_param_id);
        self.types_match_for_using(base_ty, &first_param.ty)
    }

    /// Gets the type of an expression. Prefers the type computed by sema's type
    /// checker (recorded during analysis whenever codegen runs); falls back to
    /// re-deriving it from the HIR for any expression the checker didn't record.
    pub(super) fn get_expr_type(&self, expr: &hir::Expr<'_>) -> Option<solar_sema::ty::Ty<'gcx>> {
        if let Some(ty) = self.gcx.type_of_expr(expr.id) {
            return Some(ty);
        }
        self.get_expr_type_fallback(expr)
    }

    /// Re-derives the type of an expression by walking the HIR. Used as a
    /// fallback when sema's type checker did not record a type for it.
    fn get_expr_type_fallback(&self, expr: &hir::Expr<'_>) -> Option<solar_sema::ty::Ty<'gcx>> {
        // Case 1: Variable - get its declared type
        if let ExprKind::Ident(res_slice) = &expr.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            let var = self.gcx.hir.variable(*var_id);
            return Some(self.gcx.type_of_hir_ty(&var.ty));
        }

        // Case 2: Struct field access - get the declared field type.
        if let ExprKind::Member(base, member) = &expr.kind
            && let Some(struct_id) = self.struct_id_of_expr(base)
        {
            let strukt = self.gcx.hir.strukt(struct_id);
            for &field_id in strukt.fields {
                let field = self.gcx.hir.variable(field_id);
                if field.name.is_some_and(|name| name.name == member.name) {
                    return Some(self.gcx.type_of_hir_ty(&field.ty));
                }
            }
        }

        // Case 3: Indexing - use the array element or mapping value type.
        if let ExprKind::Index(base, _) = &expr.kind
            && let Some(element_ty) = self.indexed_value_type(base)
        {
            return Some(element_ty);
        }

        // Case 3: Call result - use the callee's first return type.
        if let ExprKind::Call(callee, args, _) = &expr.kind {
            // Type conversion: `address(this)`, `uint160(x)`, ...
            if let ExprKind::Type(ty) = &callee.kind {
                return Some(self.gcx.type_of_hir_ty(ty));
            }

            if let ExprKind::Ident(res_slice) = &callee.kind
                && let Some(hir::Res::Item(hir::ItemId::Function(func_id))) = res_slice.first()
            {
                return self.first_return_type(*func_id);
            }

            if let ExprKind::Member(base, member) = &callee.kind {
                if let Some(func_id) = self.resolve_using_directive_call(base, member.name) {
                    return self.first_return_type(func_id);
                }

                if let ExprKind::Ident(res_slice) = &base.kind
                    && let Some(hir::Res::Item(hir::ItemId::Contract(contract_id))) =
                        res_slice.first()
                {
                    let arg_count = args.exprs().count();
                    if let Some(func_id) =
                        self.find_library_function(*contract_id, member.name, arg_count)
                    {
                        return self.first_return_type(func_id);
                    }
                }
            }
        }

        // Case 2: Literal - could be integer, string, etc.
        if let ExprKind::Lit(lit) = &expr.kind {
            use solar_ast::{LitKind, StrKind};
            return match &lit.kind {
                LitKind::Number(_) => Some(self.gcx.types.uint(256)),
                LitKind::Bool(_) => Some(self.gcx.types.bool),
                LitKind::Address(_) => Some(self.gcx.types.address),
                LitKind::Str(StrKind::Hex, ..) => Some(self.gcx.types.bytes),
                LitKind::Str(..) => Some(self.gcx.types.string),
                LitKind::Rational(_) | LitKind::Err(_) => None,
            };
        }

        // Case 3: Binary/unary operations - typically return the operand type
        if let ExprKind::Binary(lhs, _, rhs) = &expr.kind {
            return self.get_expr_type(lhs).or_else(|| self.get_expr_type(rhs));
        }
        if let ExprKind::Unary(_, operand) = &expr.kind {
            return self.get_expr_type(operand);
        }
        if let ExprKind::Tuple(elements) = &expr.kind {
            return elements.iter().flatten().find_map(|expr| self.get_expr_type(expr));
        }

        None
    }

    fn indexed_value_type(&self, expr: &hir::Expr<'_>) -> Option<solar_sema::ty::Ty<'gcx>> {
        let ty = self.get_expr_type(expr)?;
        let loc = ty.loc();
        match ty.peel_refs().kind {
            TyKind::Mapping(_, value) => {
                Some(value.with_loc_if_ref(self.gcx, solar_ast::DataLocation::Storage))
            }
            TyKind::Array(element, _) | TyKind::DynArray(element) => {
                Some(element.with_loc_if_ref_opt(self.gcx, loc))
            }
            TyKind::Slice(array) => array.with_loc_if_ref_opt(self.gcx, loc).base_type(self.gcx),
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                Some(self.gcx.types.fixed_bytes(1))
            }
            _ => None,
        }
    }

    fn first_return_type(&self, func_id: hir::FunctionId) -> Option<solar_sema::ty::Ty<'gcx>> {
        let func = self.gcx.hir.function(func_id);
        let ret_id = *func.returns.first()?;
        let ret = self.gcx.hir.variable(ret_id);
        Some(self.gcx.type_of_hir_ty(&ret.ty))
    }

    pub(super) fn is_dynamic_memory_array_expr(&self, expr: &hir::Expr<'_>) -> bool {
        match &expr.kind {
            ExprKind::Ident(res_slice) => {
                let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first() else {
                    return false;
                };
                if self.storage_slots.contains_key(var_id) {
                    return false;
                }
                let var = self.gcx.hir.variable(*var_id);
                matches!(&var.ty.kind, hir::TypeKind::Array(array) if array.size.is_none())
            }
            ExprKind::Member(base, member) => {
                let Some(struct_id) = self.expr_struct_id(base) else {
                    return false;
                };
                let strukt = self.gcx.hir.strukt(struct_id);
                for &field_id in strukt.fields {
                    let field = self.gcx.hir.variable(field_id);
                    if field.name.is_some_and(|name| name.name == member.name) {
                        return matches!(
                            &field.ty.kind,
                            hir::TypeKind::Array(array) if array.size.is_none()
                        );
                    }
                }
                false
            }
            ExprKind::Call(callee, _, _) => {
                matches!(&callee.kind, ExprKind::New(ty) if matches!(&ty.kind, hir::TypeKind::Array(array) if array.size.is_none()))
            }
            _ => false,
        }
    }

    pub(super) fn new_dynamic_memory_array_const_len(&self, expr: &hir::Expr<'_>) -> Option<u64> {
        let ExprKind::Call(callee, args, _) = &expr.kind else {
            return None;
        };
        if !matches!(
            &callee.kind,
            ExprKind::New(ty) if matches!(&ty.kind, hir::TypeKind::Array(array) if array.size.is_none())
        ) {
            return None;
        }

        let len = args.exprs().next()?;
        let ExprKind::Lit(lit) = &len.kind else {
            return None;
        };
        let LitKind::Number(value) = &lit.kind else {
            return None;
        };
        u64::try_from(*value).ok()
    }

    pub(super) fn is_dynamic_bytes_expr(&self, expr: &hir::Expr<'_>) -> bool {
        match &expr.kind {
            ExprKind::Ident(res_slice) => {
                let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first() else {
                    return false;
                };
                let var = self.gcx.hir.variable(*var_id);
                matches!(
                    var.ty.kind,
                    hir::TypeKind::Elementary(
                        hir::ElementaryType::String | hir::ElementaryType::Bytes
                    )
                )
            }
            ExprKind::Call(callee, _, _) => {
                let ExprKind::Member(base, member) = &callee.kind else {
                    return false;
                };
                let ExprKind::Ident(res_slice) = &base.kind else {
                    return false;
                };
                matches!(res_slice.first(), Some(hir::Res::Builtin(Builtin::Abi)))
                    && matches!(
                        member.name.as_str(),
                        "encode" | "encodePacked" | "encodeWithSelector" | "encodeWithSignature"
                    )
            }
            _ => false,
        }
    }

    pub(super) fn expr_struct_id(&self, expr: &hir::Expr<'_>) -> Option<hir::StructId> {
        match &expr.kind {
            ExprKind::Ident(res_slice) => {
                let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first() else {
                    return None;
                };
                if self.struct_storage_base_slots.contains_key(var_id) {
                    return None;
                }
                let var = self.gcx.hir.variable(*var_id);
                if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind {
                    Some(*struct_id)
                } else {
                    None
                }
            }
            ExprKind::Member(base, member) => {
                let base_struct_id = self.expr_struct_id(base)?;
                let strukt = self.gcx.hir.strukt(base_struct_id);
                for &field_id in strukt.fields {
                    let field = self.gcx.hir.variable(field_id);
                    if field.name.is_some_and(|name| name.name == member.name)
                        && let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) =
                            &field.ty.kind
                    {
                        return Some(*struct_id);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Checks if an expression type matches a using directive target type.
    fn types_match_for_using(
        &self,
        expr_ty: &solar_sema::ty::Ty<'_>,
        target_ty: &hir::Type<'_>,
    ) -> bool {
        use hir::TypeKind;
        use solar_sema::ty::TyKind;

        // A reference-type receiver (struct/array/bytes) has type `Ref(_, loc)`;
        // strip the data location so it matches the location-less target type.
        let expr_ty = expr_ty.peel_refs();

        match (&expr_ty.kind, &target_ty.kind) {
            // Elementary types (uint256, bool, etc.)
            (TyKind::Elementary(e1), TypeKind::Elementary(e2)) => e1 == e2,
            // Contract types
            (TyKind::Contract(c1), TypeKind::Custom(hir::ItemId::Contract(c2))) => c1 == c2,
            // Struct types
            (TyKind::Struct(s1), TypeKind::Custom(hir::ItemId::Struct(s2))) => s1 == s2,
            // Enum types
            (TyKind::Enum(e1), TypeKind::Custom(hir::ItemId::Enum(e2))) => e1 == e2,
            _ => false,
        }
    }

    /// Gets struct info for an expression if it has a struct type.
    /// Returns (struct_id, field_count) if the expression is a struct.
    fn get_expr_struct_info(&self, expr: &hir::Expr<'_>) -> Option<(hir::StructId, usize)> {
        // Case 1: Variable with struct type (e.g., `p` where `Point memory p`)
        if let ExprKind::Ident(res_slice) = &expr.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            let var = self.gcx.hir.variable(*var_id);
            if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind {
                let strukt = self.gcx.hir.strukt(*struct_id);
                return Some((*struct_id, strukt.fields.len()));
            }
        }

        // Case 2: Struct constructor call like Point(x, y)
        if let ExprKind::Call(callee, _, _) = &expr.kind
            && let ExprKind::Ident(res_slice) = &callee.kind
            && let Some(hir::Res::Item(hir::ItemId::Struct(struct_id))) = res_slice.first()
        {
            let strukt = self.gcx.hir.strukt(*struct_id);
            return Some((*struct_id, strukt.fields.len()));
        }

        None
    }
}
