//! Expression lowering.

use super::Lowerer;
use crate::{
    mir::{FunctionBuilder, MirType, ValueId},
    transform::{ConstantFolder, FoldResult},
};
use alloy_primitives::U256;
use solar_ast::{LitKind, StrKind};
use solar_interface::{Ident, Symbol, kw, sym};
use solar_sema::{
    builtins::Builtin,
    hir::{self, CallArgs, ElementaryType, ExprKind},
};

impl<'gcx> Lowerer<'gcx> {
    /// Lowers an expression to MIR.
    pub(super) fn lower_expr(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> ValueId {
        match &expr.kind {
            ExprKind::Lit(lit) => self.lower_literal(builder, lit),

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
                if let Some(folded) = folder.fold_to_integer(expr) {
                    return builder.imm_u256(folded);
                }
                if let FoldResult::Bool(b) = folder.try_fold(expr) {
                    return builder.imm_bool(b);
                }

                let lhs_val = self.lower_expr(builder, lhs);
                let rhs_val = self.lower_expr(builder, rhs);
                let is_signed = self.is_expr_signed(lhs);
                self.lower_binary_op(builder, lhs_val, *op, rhs_val, is_signed)
            }

            ExprKind::Unary(op, operand) => {
                use hir::UnOpKind;
                match op.kind {
                    UnOpKind::PreInc | UnOpKind::PostInc | UnOpKind::PreDec | UnOpKind::PostDec => {
                        // Increment/decrement need to read, compute, store, and return
                        let operand_val = self.lower_expr(builder, operand);
                        let one = builder.imm_u64(1);
                        let new_val = match op.kind {
                            UnOpKind::PreInc | UnOpKind::PostInc => builder.add(operand_val, one),
                            UnOpKind::PreDec | UnOpKind::PostDec => builder.sub(operand_val, one),
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
                        self.lower_unary_op(builder, *op, operand_val)
                    }
                }
            }

            ExprKind::Ternary(cond, then_expr, else_expr) => {
                let cond_val = self.lower_expr(builder, cond);
                let then_val = self.lower_expr(builder, then_expr);
                let else_val = self.lower_expr(builder, else_expr);
                builder.select(cond_val, then_val, else_val)
            }

            ExprKind::Call(callee, args, call_opts) => {
                self.lower_call(builder, callee, args, *call_opts)
            }

            ExprKind::Index(base, index) => {
                // Check if base is a mapping (state variable with mapping type)
                if let Some((var_id, slot)) = self.get_mapping_base_slot(base) {
                    // This is a mapping access: mapping[key]
                    // Storage slot = keccak256(abi.encode(key, base_slot))
                    let index_val = match index {
                        Some(idx) => self.lower_expr(builder, idx),
                        None => builder.imm_u64(0),
                    };
                    let slot_val = builder.imm_u64(slot);
                    let computed_slot = self.compute_mapping_slot(builder, index_val, slot_val);

                    // Check if this is a nested mapping
                    let var = self.gcx.hir.variable(var_id);
                    if let hir::TypeKind::Mapping(map) = &var.ty.kind
                        && matches!(map.value.kind, hir::TypeKind::Mapping(_))
                    {
                        // Nested mapping - return the computed slot for further indexing
                        return computed_slot;
                    }

                    return builder.sload(computed_slot);
                }

                // Check if base is a nested mapping access (e.g., m[a][b] where m[a] returns a
                // slot)
                if self.is_nested_mapping_index(base) {
                    // This is a nested mapping access
                    let inner_slot = self.lower_nested_mapping_slot(builder, base);
                    let index_val = match index {
                        Some(idx) => self.lower_expr(builder, idx),
                        None => builder.imm_u64(0),
                    };
                    let computed_slot = self.compute_mapping_slot(builder, index_val, inner_slot);

                    // Check if the value is another nested mapping
                    if self.nested_mapping_value_is_mapping(base) {
                        return computed_slot;
                    }

                    return builder.sload(computed_slot);
                }

                // Check if base is a dynamic array in storage
                if let Some((_var_id, slot)) = self.get_dyn_array_base_slot(base) {
                    // Dynamic array access: array[idx]
                    // Data is stored at keccak256(slot) + idx
                    let slot_val = builder.imm_u64(slot);

                    // Compute data slot: keccak256(slot)
                    let mem_0 = builder.imm_u64(0);
                    builder.mstore(mem_0, slot_val);
                    let size_32 = builder.imm_u64(32);
                    let data_slot = builder.keccak256(mem_0, size_32);

                    // Compute element slot: data_slot + index
                    let index_val = match index {
                        Some(idx) => self.lower_expr(builder, idx),
                        None => builder.imm_u64(0),
                    };
                    let element_slot = builder.add(data_slot, index_val);

                    return builder.sload(element_slot);
                }

                // Regular array/memory access
                let base_val = self.lower_expr(builder, base);
                let index_val = match index {
                    Some(idx) => self.lower_expr(builder, idx),
                    None => builder.imm_u64(0),
                };
                let offset_32 = builder.imm_u64(32);
                let byte_offset = builder.mul(index_val, offset_32);
                let addr = builder.add(base_val, byte_offset);
                builder.mload(addr)
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

                // Handle type(T).min and type(T).max
                if let ExprKind::TypeCall(ty) = &base.kind {
                    let member_name = member.name.as_str();
                    if member_name == "max" || member_name == "min" {
                        return self.lower_type_minmax(builder, ty, member_name == "max");
                    }
                }

                // Handle dynamic array .length
                if member.name.as_str() == "length"
                    && let Some((_var_id, slot)) = self.get_dyn_array_base_slot(base)
                {
                    // Length is stored directly at the base slot
                    let slot_val = builder.imm_u64(slot);
                    return builder.sload(slot_val);
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

                // Regular memory struct member access
                if let Some((_struct_id, field_index)) =
                    self.get_memory_struct_field_info(base, *member)
                {
                    let base_val = self.lower_expr(builder, base);
                    let field_offset = field_index as u64 * 32;
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

            ExprKind::Assign(lhs, op, rhs) => {
                let rhs_val = self.lower_expr(builder, rhs);
                // Handle compound assignment (+=, -=, etc.)
                let final_val = if let Some(bin_op) = op {
                    // Read current value, apply operator, then assign
                    let lhs_val = self.lower_expr(builder, lhs);
                    let is_signed = self.is_expr_signed(lhs);
                    self.lower_binary_op(builder, lhs_val, *bin_op, rhs_val, is_signed)
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
                let ptr = builder.imm_u64(0x80);
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
    fn lower_literal(&mut self, builder: &mut FunctionBuilder<'_>, lit: &hir::Lit<'_>) -> ValueId {
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
                        padded[32 - len..].copy_from_slice(&bytes[..len]);
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
                        let offset_val = builder.imm_u64(offset);
                        return builder.mload(offset_val);
                    }

                    // Check if it's a constant - inline its value
                    if var.is_constant()
                        && let Some(init) = var.initializer
                    {
                        return self.lower_expr(builder, init);
                    }

                    // Check if it's a storage variable
                    if let Some(&slot) = self.storage_slots.get(var_id) {
                        // For storage structs, we need to copy to memory and return the pointer
                        if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind
                        {
                            let strukt = self.gcx.hir.strukt(*struct_id);
                            let num_fields = strukt.fields.len();
                            let struct_size = (num_fields as u64) * 32;
                            let struct_ptr = self.allocate_memory(builder, struct_size);

                            // Load each field from storage and store to memory
                            for i in 0..num_fields {
                                let field_slot = slot + (i as u64);
                                let field_slot_val = builder.imm_u64(field_slot);
                                let field_val = builder.sload(field_slot_val);

                                let field_mem_offset = (i as u64) * 32;
                                if field_mem_offset == 0 {
                                    builder.mstore(struct_ptr, field_val);
                                } else {
                                    let offset_val = builder.imm_u64(field_mem_offset);
                                    let field_addr = builder.add(struct_ptr, offset_val);
                                    builder.mstore(field_addr, field_val);
                                }
                            }
                            return struct_ptr;
                        }

                        // For scalar storage variables, just load the value
                        let slot_val = builder.imm_u64(slot);
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

    /// Checks if a HIR type is a signed integer type.
    fn is_hir_type_signed(&self, ty: &hir::Type<'_>) -> bool {
        matches!(ty.kind, hir::TypeKind::Elementary(ElementaryType::Int(_)))
    }

    /// Checks if an expression has a signed integer type.
    /// This is a best-effort check based on the expression structure.
    fn is_expr_signed(&self, expr: &hir::Expr<'_>) -> bool {
        match &expr.kind {
            ExprKind::Ident(res_slice) => {
                if let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first() {
                    let var = self.gcx.hir.variable(*var_id);
                    self.is_hir_type_signed(&var.ty)
                } else {
                    false
                }
            }
            ExprKind::Unary(_, inner) => self.is_expr_signed(inner),
            ExprKind::Binary(lhs, _, _) => self.is_expr_signed(lhs),
            ExprKind::Tuple(elements) => {
                if let Some(Some(inner)) = elements.first() {
                    self.is_expr_signed(inner)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Lowers a binary operation.
    fn lower_binary_op(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        lhs: ValueId,
        op: hir::BinOp,
        rhs: ValueId,
        is_signed: bool,
    ) -> ValueId {
        use hir::BinOpKind;

        match op.kind {
            BinOpKind::Add => builder.add(lhs, rhs),
            BinOpKind::Sub => builder.sub(lhs, rhs),
            BinOpKind::Mul => builder.mul(lhs, rhs),
            BinOpKind::Div => {
                if is_signed {
                    builder.sdiv(lhs, rhs)
                } else {
                    builder.div(lhs, rhs)
                }
            }
            BinOpKind::Rem => {
                if is_signed {
                    builder.smod(lhs, rhs)
                } else {
                    builder.mod_(lhs, rhs)
                }
            }
            BinOpKind::Pow => builder.exp(lhs, rhs),
            // Logical AND: for bool inputs (guaranteed by type checker), just use bitwise AND.
            // Bool values are already 0 or 1, so a && b == a & b.
            BinOpKind::And => builder.and(lhs, rhs),
            // Logical OR: for bool inputs (guaranteed by type checker), just use bitwise OR.
            // Bool values are already 0 or 1, so a || b == a | b.
            BinOpKind::Or => builder.or(lhs, rhs),
            BinOpKind::BitAnd => builder.and(lhs, rhs),
            BinOpKind::BitOr => builder.or(lhs, rhs),
            BinOpKind::BitXor => builder.xor(lhs, rhs),
            BinOpKind::Shl => builder.shl(rhs, lhs),
            BinOpKind::Shr => {
                // For signed types, >> is arithmetic shift (SAR)
                if is_signed { builder.sar(rhs, lhs) } else { builder.shr(rhs, lhs) }
            }
            BinOpKind::Sar => builder.sar(rhs, lhs),
            BinOpKind::Lt => {
                if is_signed {
                    builder.slt(lhs, rhs)
                } else {
                    builder.lt(lhs, rhs)
                }
            }
            BinOpKind::Gt => {
                if is_signed {
                    builder.sgt(lhs, rhs)
                } else {
                    builder.gt(lhs, rhs)
                }
            }
            BinOpKind::Le => {
                if is_signed {
                    let gt = builder.sgt(lhs, rhs);
                    builder.iszero(gt)
                } else {
                    let gt = builder.gt(lhs, rhs);
                    builder.iszero(gt)
                }
            }
            BinOpKind::Ge => {
                if is_signed {
                    let lt = builder.slt(lhs, rhs);
                    builder.iszero(lt)
                } else {
                    let lt = builder.lt(lhs, rhs);
                    builder.iszero(lt)
                }
            }
            BinOpKind::Eq => builder.eq(lhs, rhs),
            BinOpKind::Ne => {
                let eq = builder.eq(lhs, rhs);
                builder.iszero(eq)
            }
        }
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

    /// Lowers a unary operation.
    fn lower_unary_op(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        op: hir::UnOp,
        operand: ValueId,
    ) -> ValueId {
        use hir::UnOpKind;

        match op.kind {
            UnOpKind::Not => builder.iszero(operand),
            UnOpKind::BitNot => builder.not(operand),
            UnOpKind::Neg => {
                let zero = builder.imm_u256(U256::ZERO);
                builder.sub(zero, operand)
            }
            UnOpKind::PreInc | UnOpKind::PostInc => {
                let one = builder.imm_u64(1);
                builder.add(operand, one)
            }
            UnOpKind::PreDec | UnOpKind::PostDec => {
                let one = builder.imm_u64(1);
                builder.sub(operand, one)
            }
        }
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
                        let offset_val = builder.imm_u64(offset);
                        builder.mstore(offset_val, rhs);
                    } else if self.locals.contains_key(var_id) {
                        // Function parameter - update SSA mapping (shouldn't happen normally)
                        self.locals.insert(*var_id, rhs);
                    } else if let Some(&base_slot) = self.storage_slots.get(var_id) {
                        // Check if this is a struct assignment (memory struct -> storage struct)
                        if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind
                        {
                            // Copy each field from memory to storage
                            let strukt = self.gcx.hir.strukt(*struct_id);
                            for (i, &_field_id) in strukt.fields.iter().enumerate() {
                                let field_slot_offset =
                                    self.get_struct_field_slot_offset(*struct_id, i);
                                let slot = base_slot + field_slot_offset;
                                let slot_val = builder.imm_u64(slot);

                                // Load field from memory (rhs is memory pointer)
                                let field_mem_offset = (i as u64) * 32;
                                let field_val = if field_mem_offset == 0 {
                                    builder.mload(rhs)
                                } else {
                                    let offset_val = builder.imm_u64(field_mem_offset);
                                    let field_addr = builder.add(rhs, offset_val);
                                    builder.mload(field_addr)
                                };

                                builder.sstore(slot_val, field_val);
                            }
                        } else {
                            // Simple scalar storage assignment
                            let slot_val = builder.imm_u64(base_slot);
                            builder.sstore(slot_val, rhs);
                        }
                    }
                }
            }
            ExprKind::Index(base, index) => {
                // Check if base is a mapping (state variable with mapping type)
                if let Some((_var_id, slot)) = self.get_mapping_base_slot(base) {
                    // This is a mapping assignment: mapping[key] = value
                    // Storage slot = keccak256(abi.encode(key, base_slot))
                    let index_val = match index {
                        Some(idx) => self.lower_expr(builder, idx),
                        None => builder.imm_u64(0),
                    };
                    let slot_val = builder.imm_u64(slot);
                    let computed_slot = self.compute_mapping_slot(builder, index_val, slot_val);
                    builder.sstore(computed_slot, rhs);
                    return;
                }

                // Check if base is a nested mapping access (e.g., m[a][b] = value)
                if self.is_nested_mapping_index(base) {
                    let inner_slot = self.lower_nested_mapping_slot(builder, base);
                    let index_val = match index {
                        Some(idx) => self.lower_expr(builder, idx),
                        None => builder.imm_u64(0),
                    };
                    let computed_slot = self.compute_mapping_slot(builder, index_val, inner_slot);
                    builder.sstore(computed_slot, rhs);
                    return;
                }

                // Regular array/memory assignment
                let base_val = self.lower_expr(builder, base);
                let index_val = match index {
                    Some(idx) => self.lower_expr(builder, idx),
                    None => builder.imm_u64(0),
                };
                let offset_32 = builder.imm_u64(32);
                let byte_offset = builder.mul(index_val, offset_32);
                let addr = builder.add(base_val, byte_offset);
                builder.mstore(addr, rhs);
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

                // Regular memory struct member assignment
                if let Some((_struct_id, field_index)) =
                    self.get_memory_struct_field_info(base, *member)
                {
                    let base_val = self.lower_expr(builder, base);
                    let field_offset = field_index as u64 * 32;
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
                let base_val = self.lower_expr(builder, base);
                builder.mstore(base_val, rhs);
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
            && let Some(hir::Res::Builtin(builtin)) = res_slice.first()
        {
            return self.lower_builtin_call(builder, *builtin, args);
        }

        if let ExprKind::Member(base, member) = &callee.kind {
            return self.lower_member_call_with_opts(builder, base, *member, args, call_opts);
        }

        // Handle `new Contract(args)` - contract creation
        if let ExprKind::New(ty) = &callee.kind {
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
        if let ExprKind::Type(_ty) = &callee.kind {
            // Type conversion: return the first argument
            if let Some(first_arg) = args.exprs().next() {
                return self.lower_expr(builder, first_arg);
            }
        }

        builder.imm_u64(0)
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
                // Not a contract type, return placeholder
                return builder.imm_u64(0);
            }
        };

        // Look up pre-compiled bytecode
        let (bytecode, _segment_idx) = match self.contract_bytecodes.get(&contract_id) {
            Some(bc) => bc.clone(),
            None => {
                // Bytecode not available - return placeholder
                // This happens if contracts aren't compiled in the right order
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

        // Allocate memory for bytecode + constructor args
        // We'll put the bytecode starting at a free memory offset
        // For simplicity, use memory offset 0 (assuming it's available)
        let mem_offset = builder.imm_u64(0);

        // Copy bytecode to memory using CODECOPY isn't right here since
        // the bytecode is from another contract. We need to use MSTORE
        // to write the bytecode bytes to memory.
        //
        // For each 32-byte chunk of bytecode, emit an MSTORE
        let mut offset = 0u64;
        for chunk in bytecode.chunks(32) {
            let mut padded = [0u8; 32];
            padded[..chunk.len()].copy_from_slice(chunk);
            let value = U256::from_be_bytes(padded);
            let val_id = builder.imm_u256(value);
            let offset_id = builder.imm_u64(offset);
            builder.mstore(offset_id, val_id);
            offset += 32;
        }

        // Append constructor arguments after bytecode
        let mut args_offset = bytecode_len as u64;
        for arg in args.exprs() {
            let arg_val = self.lower_expr(builder, arg);
            let arg_offset = builder.imm_u64(args_offset);
            builder.mstore(arg_offset, arg_val);
            args_offset += 32; // Each arg is 32 bytes ABI encoded
        }

        // Total size = bytecode + args
        let total_size = builder.imm_u64(args_offset);

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
                    let arg_val = self.lower_expr(builder, first);
                    let ptr = builder.imm_u64(0);
                    builder.mstore(ptr, arg_val);
                    let size = builder.imm_u64(32);
                    return builder.keccak256(ptr, size);
                }
                builder.imm_u64(0)
            }
            Builtin::Require | Builtin::RequireMsg | Builtin::Assert => {
                let mut exprs = args.exprs();
                if let Some(first) = exprs.next() {
                    let cond = self.lower_expr(builder, first);
                    let is_false = builder.iszero(cond);

                    let revert_block = builder.create_block();
                    let continue_block = builder.create_block();

                    builder.branch(is_false, revert_block, continue_block);

                    builder.switch_to_block(revert_block);
                    let zero = builder.imm_u64(0);
                    builder.revert(zero, zero);

                    builder.switch_to_block(continue_block);
                }
                builder.imm_u64(0)
            }
            Builtin::Revert | Builtin::RevertMsg => {
                let zero = builder.imm_u64(0);
                builder.revert(zero, zero);
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
            Builtin::AbiEncode | Builtin::AbiEncodePacked => {
                // abi.encode/encodePacked: for single 32-byte values, just return the value
                // For multiple values, encode them sequentially in memory
                let arg_vals: Vec<ValueId> =
                    args.exprs().map(|arg| self.lower_expr(builder, arg)).collect();

                if arg_vals.is_empty() {
                    return builder.imm_u64(0);
                }

                if arg_vals.len() == 1 {
                    // Single value - just return it (keccak256 will store it to memory)
                    return arg_vals[0];
                }

                // Multiple values - store them at scratch memory and return pointer
                // Write to memory starting at offset 0
                for (i, &val) in arg_vals.iter().enumerate() {
                    let offset = builder.imm_u64((i * 32) as u64);
                    builder.mstore(offset, val);
                }
                // Return pointer to the encoded data (offset 0)
                builder.imm_u64(0)
            }
            _ => builder.imm_u64(0),
        }
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

            // Get the calldata bytes argument
            let mut exprs = args.exprs();
            let (calldata_offset, calldata_size) = if let Some(data_arg) = exprs.next() {
                // The data argument is bytes - could be a literal, memory reference, etc.
                // For now, handle string/bytes literals and empty bytes
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

            // Return data location - we store at a fixed offset after calldata
            // Use offset 0 for return data since we don't need calldata after call
            let ret_offset = builder.imm_u64(0);
            // We'll store the return data size at runtime via RETURNDATASIZE
            // For now, assume no return data copying (size 0)
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

            // Low-level calls return (bool success, bytes memory returndata)
            // For now, we just return the success bool. The returndata can be accessed
            // via returndatasize()/returndatacopy() if needed.
            // TODO: Support returning the bytes memory returndata as second value
            return success;
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
            eprintln!(
                "DEBUG get_mapping_base_slot: var_id={:?}, name={:?}, is_mapping={}",
                var_id,
                var.name,
                matches!(var.ty.kind, hir::TypeKind::Mapping(_))
            );
            // Check if this variable has mapping type
            if matches!(var.ty.kind, hir::TypeKind::Mapping(_)) {
                // Look up the storage slot
                if let Some(&slot) = self.storage_slots.get(var_id) {
                    eprintln!("DEBUG   -> found slot {slot}");
                    return Some((*var_id, slot));
                } else {
                    eprintln!(
                        "DEBUG   -> NOT FOUND in storage_slots! keys={:?}",
                        self.storage_slots.keys().collect::<Vec<_>>()
                    );
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
        None
    }

    /// Computes the storage slot for a nested struct member access.
    /// For expressions like `storedNested.point.x` where `storedNested` is a storage struct
    /// with a nested struct field `point`.
    fn compute_nested_storage_slot(&mut self, base: &hir::Expr<'_>, member: Ident) -> Option<u64> {
        // The base must be a Member expression on a storage struct
        if let ExprKind::Member(inner_base, inner_member) = &base.kind {
            // Try to get the base storage slot from the innermost variable
            if let Some((base_slot, struct_id, field_index)) =
                self.get_storage_struct_field_info(inner_base, *inner_member)
            {
                // Get the type of the accessed field
                let strukt = self.gcx.hir.strukt(struct_id);
                if field_index < strukt.fields.len() {
                    let field_var = self.gcx.hir.variable(strukt.fields[field_index]);
                    // Check if this field is itself a struct
                    if let hir::TypeKind::Custom(hir::ItemId::Struct(inner_struct_id)) =
                        &field_var.ty.kind
                    {
                        // Calculate the slot of the nested struct field
                        let inner_field_offset =
                            self.get_struct_field_slot_offset(struct_id, field_index);
                        let nested_base_slot = base_slot + inner_field_offset;

                        // Now find the member within the inner struct
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

    /// Computes the storage slot for a nested mapping access.
    /// For `m[a][b]`, this computes: `keccak256(b, keccak256(a, base_slot))`
    fn lower_nested_mapping_slot(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> ValueId {
        if let ExprKind::Index(inner_base, inner_index) = &expr.kind {
            // Check if inner_base is the root mapping variable
            if let Some((_var_id, slot)) = self.get_mapping_base_slot(inner_base) {
                // Compute the slot for the inner access
                let inner_index_val = match inner_index {
                    Some(idx) => self.lower_expr(builder, idx),
                    None => builder.imm_u64(0),
                };
                let slot_val = builder.imm_u64(slot);
                return self.compute_mapping_slot(builder, inner_index_val, slot_val);
            }

            // Recursively compute deeper nesting slot
            let deeper_slot = self.lower_nested_mapping_slot(builder, inner_base);
            let inner_index_val = match inner_index {
                Some(idx) => self.lower_expr(builder, idx),
                None => builder.imm_u64(0),
            };
            return self.compute_mapping_slot(builder, inner_index_val, deeper_slot);
        }
        // Should not reach here if is_nested_mapping_index returned true
        builder.imm_u64(0)
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

        // Case 3: base is `this` (Builtin::This)
        if let ExprKind::Ident(res_slice) = &base.kind
            && let Some(hir::Res::Builtin(Builtin::This)) = res_slice.first()
        {
            // Look up the function in the current contract
            for contract_id in self.gcx.hir.contract_ids() {
                if let Some(sel) = lookup_in_contract(contract_id) {
                    return sel;
                }
            }
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
            // For now, iterate all contracts and find the function
            for contract_id in self.gcx.hir.contract_ids() {
                if let Some(count) = lookup_in_contract(contract_id) {
                    return count;
                }
            }
        }

        // Default: assume single return value
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

        // Check for recursive inlining cycle AFTER evaluating arguments
        if !self.try_enter_inline(func_id) {
            // Cycle detected or max depth exceeded - return placeholder
            return builder.imm_u64(0);
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

            // Check for recursive inlining cycle AFTER evaluating arguments
            if !self.try_enter_inline(func_id) {
                // Cycle detected or max depth exceeded - return placeholder
                return builder.imm_u64(0);
            }

            // Simple inlining: bind parameters directly as SSA values
            // This works for pure functions that don't mutate parameters
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
        } else {
            // External library functions use DELEGATECALL
            // For now, return placeholder (external library calls are less common)
            builder.imm_u64(0)
        }
    }

    /// Lowers a simple library function body.
    /// For functions with a single return statement, directly evaluate the return expression.
    fn lower_library_body_simple(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        body: &hir::Block<'_>,
        _func: &hir::Function<'_>,
    ) -> ValueId {
        // Look for a return statement and evaluate its expression
        for stmt in body.stmts {
            match &stmt.kind {
                hir::StmtKind::Return(Some(expr)) => {
                    return self.lower_expr(builder, expr);
                }
                hir::StmtKind::Return(None) => {
                    return builder.imm_u64(0);
                }
                hir::StmtKind::DeclSingle(var_id) => {
                    // Handle local variable declarations
                    let var = self.gcx.hir.variable(*var_id);
                    let init_val = if let Some(init) = var.initializer {
                        self.lower_expr(builder, init)
                    } else {
                        builder.imm_u64(0)
                    };
                    self.locals.insert(*var_id, init_val);
                }
                hir::StmtKind::Expr(expr) => {
                    self.lower_expr(builder, expr);
                }
                hir::StmtKind::If(cond, then_stmt, else_stmt) => {
                    // Check if this is an early return pattern: `if (cond) return val;`
                    // followed by more code (no else branch)
                    let cond_val = self.lower_expr(builder, cond);
                    let then_return = self.lower_library_stmt_return(builder, then_stmt);

                    if else_stmt.is_none() && then_return != builder.imm_u64(0) {
                        // Early return pattern: `if (cond) return val;`
                        // We need to continue processing remaining statements for the else case
                        // The remaining statements in the current body are the "else" branch
                        // We'll process them and create a Select between early return and result
                        // Store cond and then_return, continue processing remaining stmts
                        // This is handled by building proper control flow with branches
                        // For now, use proper branching via lower_if
                        let then_block = builder.create_block();
                        let merge_block = builder.create_block();

                        builder.branch(cond_val, then_block, merge_block);

                        // Then block: return the early return value
                        builder.switch_to_block(then_block);
                        builder.ret([then_return]);

                        // Merge block: continue with rest of function
                        builder.switch_to_block(merge_block);
                        // Continue processing remaining statements (fall through to next iteration)
                    } else if else_stmt.is_some() {
                        // If-else: both branches have potential returns
                        let else_val = self.lower_library_stmt_return(builder, else_stmt.unwrap());
                        if then_return != builder.imm_u64(0) || else_val != builder.imm_u64(0) {
                            return builder.select(cond_val, then_return, else_val);
                        }
                    }
                    // If no return in either branch, continue processing
                }
                _ => {}
            }
        }
        builder.imm_u64(0)
    }

    /// Extract return value from a statement (for simple conditional handling).
    fn lower_library_stmt_return(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        stmt: &hir::Stmt<'_>,
    ) -> ValueId {
        match &stmt.kind {
            hir::StmtKind::Return(Some(expr)) => self.lower_expr(builder, expr),
            hir::StmtKind::Return(None) => builder.imm_u64(0),
            hir::StmtKind::Block(block) => {
                // Recursively process nested blocks
                for inner_stmt in block.stmts {
                    if let hir::StmtKind::Return(Some(expr)) = &inner_stmt.kind {
                        return self.lower_expr(builder, expr);
                    }
                }
                builder.imm_u64(0)
            }
            _ => builder.imm_u64(0),
        }
    }

    /// Lowers a bytes argument to memory and returns (offset, size).
    /// Used for low-level calls: addr.call(data), addr.staticcall(data), addr.delegatecall(data).
    fn lower_bytes_arg_to_memory(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> (ValueId, ValueId) {
        // Handle literal strings/bytes: "" or hex"..."
        if let ExprKind::Lit(lit) = &expr.kind
            && let LitKind::Str(kind, bytes, _) = &lit.kind
        {
            let bytes = bytes.as_byte_str();
            let len = bytes.len();

            if len == 0 {
                // Empty bytes - no calldata
                return (builder.imm_u64(0), builder.imm_u64(0));
            }

            // Write bytes to memory at offset 0
            // For bytes up to 32, we can use a single MSTORE
            // For longer bytes, we need multiple MSTOREs
            let mut offset = 0u64;
            for chunk in bytes.chunks(32) {
                let mut padded = [0u8; 32];
                match kind {
                    StrKind::Str | StrKind::Unicode => {
                        // Left-aligned for strings
                        padded[..chunk.len()].copy_from_slice(chunk);
                    }
                    StrKind::Hex => {
                        // Left-aligned for hex bytes
                        padded[..chunk.len()].copy_from_slice(chunk);
                    }
                }
                let val = builder.imm_u256(U256::from_be_bytes(padded));
                let offset_val = builder.imm_u64(offset);
                builder.mstore(offset_val, val);
                offset += 32;
            }

            return (builder.imm_u64(0), builder.imm_u64(len as u64));
        }

        // Handle abi.encodeWithSelector and similar
        if let ExprKind::Call(callee, args, _) = &expr.kind
            && let ExprKind::Member(base, member) = &callee.kind
            && let ExprKind::Ident(res_slice) = &base.kind
            && let Some(hir::Res::Builtin(Builtin::Abi)) = res_slice.first()
        {
            let member_name = member.name.as_str();
            if member_name == "encodeWithSelector"
                || member_name == "encodeWithSignature"
                || member_name == "encode"
                || member_name == "encodePacked"
            {
                // Lower the abi.encode* call which writes to memory and returns length
                let result = self.lower_builtin_call(builder, Builtin::AbiEncode, args);
                // The result is the size of encoded data, data is at offset 0
                return (builder.imm_u64(0), result);
            }
        }

        // For other expressions (e.g., variables containing bytes), we need to:
        // 1. Get the memory location of the bytes
        // 2. Return (offset, size)
        // For now, fall back to evaluating the expression and treating it as empty
        // TODO: Support bytes memory variables
        let _val = self.lower_expr(builder, expr);
        (builder.imm_u64(0), builder.imm_u64(0))
    }

    /// Checks if an expression has a contract type (as opposed to address type).
    /// Used to distinguish between address.transfer(amount) and token.transfer(to, amount).
    fn is_contract_type_expr(&self, expr: &hir::Expr<'_>) -> bool {
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

        // Search through using directives in the contract
        for using in contract.using_directives {
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

            // Look for a matching function in the library
            let library = self.gcx.hir.contract(using.library);
            for func_id in library.functions() {
                let func = self.gcx.hir.function(func_id);

                // Check if the function name matches
                if func.name.map(|n| n.name) != Some(member_name) {
                    continue;
                }

                // The function must be internal or private (library functions called via using)
                if !matches!(func.visibility, hir::Visibility::Internal | hir::Visibility::Private)
                {
                    continue;
                }

                // Found a matching function
                return Some(func_id);
            }
        }

        None
    }

    /// Gets the type of an expression for using directive matching.
    fn get_expr_type(&self, expr: &hir::Expr<'_>) -> Option<solar_sema::ty::Ty<'_>> {
        // Case 1: Variable - get its declared type
        if let ExprKind::Ident(res_slice) = &expr.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            let var = self.gcx.hir.variable(*var_id);
            return Some(self.gcx.type_of_hir_ty(&var.ty));
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
        if let ExprKind::Binary(lhs, _, _) = &expr.kind {
            return self.get_expr_type(lhs);
        }
        if let ExprKind::Unary(_, operand) = &expr.kind {
            return self.get_expr_type(operand);
        }

        None
    }

    /// Checks if an expression type matches a using directive target type.
    fn types_match_for_using(
        &self,
        expr_ty: &solar_sema::ty::Ty<'_>,
        target_ty: &hir::Type<'_>,
    ) -> bool {
        use hir::TypeKind;
        use solar_sema::ty::TyKind;

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
