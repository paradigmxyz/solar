//! Expression lowering.

use super::Lowerer;
use crate::mir::{FunctionBuilder, MirType, ValueId};
use alloy_primitives::U256;
use solar_ast::{LitKind, StrKind};
use solar_interface::Ident;
use solar_sema::{
    builtins::Builtin,
    hir::{self, CallArgs, ExprKind},
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
                let lhs_val = self.lower_expr(builder, lhs);
                let rhs_val = self.lower_expr(builder, rhs);
                self.lower_binary_op(builder, lhs_val, *op, rhs_val)
            }

            ExprKind::Unary(op, operand) => {
                let operand_val = self.lower_expr(builder, operand);
                self.lower_unary_op(builder, *op, operand_val)
            }

            ExprKind::Ternary(cond, then_expr, else_expr) => {
                let cond_val = self.lower_expr(builder, cond);
                let then_val = self.lower_expr(builder, then_expr);
                let else_val = self.lower_expr(builder, else_expr);
                builder.select(cond_val, then_val, else_val)
            }

            ExprKind::Call(callee, args, _named_args) => self.lower_call(builder, callee, args),

            ExprKind::Index(base, index) => {
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

            ExprKind::Member(base, _member) => {
                let base_val = self.lower_expr(builder, base);
                builder.mload(base_val)
            }

            ExprKind::Assign(lhs, _op, rhs) => {
                let rhs_val = self.lower_expr(builder, rhs);
                self.lower_assign(builder, lhs, rhs_val);
                rhs_val
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
                    if let Some(&val) = self.locals.get(var_id) {
                        return val;
                    }

                    if let Some(&slot) = self.storage_slots.get(var_id) {
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

    /// Lowers a binary operation.
    fn lower_binary_op(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        lhs: ValueId,
        op: hir::BinOp,
        rhs: ValueId,
    ) -> ValueId {
        use hir::BinOpKind;

        match op.kind {
            BinOpKind::Add => builder.add(lhs, rhs),
            BinOpKind::Sub => builder.sub(lhs, rhs),
            BinOpKind::Mul => builder.mul(lhs, rhs),
            BinOpKind::Div => builder.div(lhs, rhs),
            BinOpKind::Rem => builder.mod_(lhs, rhs),
            BinOpKind::Pow => builder.exp(lhs, rhs),
            BinOpKind::And => {
                let lhs_not = builder.iszero(lhs);
                let lhs_bool = builder.iszero(lhs_not);
                let rhs_not = builder.iszero(rhs);
                let rhs_bool = builder.iszero(rhs_not);
                builder.and(lhs_bool, rhs_bool)
            }
            BinOpKind::Or => {
                let lhs_not = builder.iszero(lhs);
                let lhs_bool = builder.iszero(lhs_not);
                let rhs_not = builder.iszero(rhs);
                let rhs_bool = builder.iszero(rhs_not);
                builder.or(lhs_bool, rhs_bool)
            }
            BinOpKind::BitAnd => builder.and(lhs, rhs),
            BinOpKind::BitOr => builder.or(lhs, rhs),
            BinOpKind::BitXor => builder.xor(lhs, rhs),
            BinOpKind::Shl => builder.shl(rhs, lhs),
            BinOpKind::Shr => builder.shr(rhs, lhs),
            BinOpKind::Sar => builder.sar(rhs, lhs),
            BinOpKind::Lt => builder.lt(lhs, rhs),
            BinOpKind::Gt => builder.gt(lhs, rhs),
            BinOpKind::Le => {
                let gt = builder.gt(lhs, rhs);
                builder.iszero(gt)
            }
            BinOpKind::Ge => {
                let lt = builder.lt(lhs, rhs);
                builder.iszero(lt)
            }
            BinOpKind::Eq => builder.eq(lhs, rhs),
            BinOpKind::Ne => {
                let eq = builder.eq(lhs, rhs);
                builder.iszero(eq)
            }
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
                    if self.locals.contains_key(var_id) {
                        self.locals.insert(*var_id, rhs);
                    } else if let Some(&slot) = self.storage_slots.get(var_id) {
                        let slot_val = builder.imm_u64(slot);
                        builder.sstore(slot_val, rhs);
                    }
                }
            }
            ExprKind::Index(base, index) => {
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
            ExprKind::Member(base, _member) => {
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
    ) -> ValueId {
        if let ExprKind::Ident(res_slice) = &callee.kind
            && let Some(hir::Res::Builtin(builtin)) = res_slice.first()
        {
            return self.lower_builtin_call(builder, *builtin, args);
        }

        if let ExprKind::Member(base, member) = &callee.kind {
            return self.lower_member_call(builder, base, *member, args);
        }

        builder.imm_u64(0)
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
            _ => builder.imm_u64(0),
        }
    }

    /// Lowers a member function call (e.g., addr.call()).
    fn lower_member_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        _base: &hir::Expr<'_>,
        _member: Ident,
        _args: &CallArgs<'_>,
    ) -> ValueId {
        builder.imm_u64(0)
    }
}
