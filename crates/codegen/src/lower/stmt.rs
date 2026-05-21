//! Statement lowering.

use super::{LoopContext, Lowerer};
use crate::mir::{BlockId, FunctionBuilder, MirType, Value, ValueId};
use alloy_primitives::U256;
use rustc_hash::FxHashMap;
use solar_interface::{Symbol, kw};
use solar_sema::hir::{self, StmtKind, yul as hir_yul};

impl<'gcx> Lowerer<'gcx> {
    /// Lowers a block of statements.
    pub(super) fn lower_block(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        block: &hir::Block<'_>,
    ) {
        for stmt in block.stmts {
            self.lower_stmt(builder, stmt);
            if builder.func().block(builder.current_block()).terminator.is_some() {
                break;
            }
        }
    }

    /// Lowers a statement to MIR.
    pub(super) fn lower_stmt(&mut self, builder: &mut FunctionBuilder<'_>, stmt: &hir::Stmt<'_>) {
        match &stmt.kind {
            StmtKind::DeclSingle(var_id) => {
                self.lower_single_var_decl(builder, *var_id);
            }

            StmtKind::DeclMulti(var_ids, init) => {
                self.lower_multi_var_decl(builder, var_ids, init);
            }

            StmtKind::Expr(expr) => {
                self.lower_expr(builder, expr);
            }

            StmtKind::Block(block) => {
                self.lower_block(builder, block);
            }

            StmtKind::If(cond, then_stmt, else_stmt) => {
                self.lower_if(builder, cond, then_stmt, *else_stmt);
            }

            StmtKind::Loop(block, source) => {
                self.lower_loop(builder, block, *source);
            }

            StmtKind::Return(value) => {
                self.lower_return(builder, *value);
            }

            StmtKind::Revert(expr) => {
                let _ = self.lower_expr(builder, expr);
                let zero = builder.imm_u64(0);
                builder.revert(zero, zero);
            }

            StmtKind::Emit(expr) => {
                self.lower_emit(builder, expr);
            }

            StmtKind::Try(try_stmt) => {
                self.lower_try(builder, try_stmt);
            }

            StmtKind::Continue => {
                if let Some(loop_ctx) = self.current_loop() {
                    builder.jump(loop_ctx.continue_target);
                }
            }

            StmtKind::Break => {
                if let Some(loop_ctx) = self.current_loop() {
                    builder.jump(loop_ctx.break_target);
                }
            }

            StmtKind::Placeholder => {}

            StmtKind::UncheckedBlock(block) => {
                self.lower_block(builder, block);
            }

            StmtKind::Assembly(assembly) => {
                self.lower_yul_block(builder, &assembly.block);
            }

            StmtKind::Err(_) => {}
        }
    }

    fn lower_yul_block(&mut self, builder: &mut FunctionBuilder<'_>, block: &hir_yul::Block<'_>) {
        self.enter_yul_scope();
        for stmt in block.stmts {
            self.lower_yul_stmt(builder, stmt);
            if builder.func().block(builder.current_block()).is_terminated() {
                break;
            }
        }
        self.exit_yul_scope();
    }

    fn lower_yul_stmt(&mut self, builder: &mut FunctionBuilder<'_>, stmt: &hir_yul::Stmt<'_>) {
        match &stmt.kind {
            hir_yul::StmtKind::Block(block) => self.lower_yul_block(builder, block),
            hir_yul::StmtKind::AssignSingle(path, expr) => {
                let value = self.lower_yul_expr(builder, expr);
                self.assign_yul_path(builder, path, value);
            }
            hir_yul::StmtKind::Expr(expr) => {
                let _ = self.lower_yul_expr(builder, expr);
            }
            hir_yul::StmtKind::If(cond, body) => self.lower_yul_if(builder, cond, body),
            hir_yul::StmtKind::VarDecl(names, init) => {
                if names.len() != 1 {
                    self.unsupported_yul_stmt(builder, stmt, "multiple variable declaration");
                    return;
                }
                let value = init.map(|expr| self.lower_yul_expr(builder, expr));
                for name in *names {
                    let value = value.unwrap_or_else(|| builder.imm_u64(0));
                    self.declare_yul_variable(name.name, value);
                }
            }
            hir_yul::StmtKind::AssignMulti(_, _) => {
                self.unsupported_yul_stmt(builder, stmt, "multiple assignment");
            }
            hir_yul::StmtKind::For(_) => self.unsupported_yul_stmt(builder, stmt, "for statement"),
            hir_yul::StmtKind::Switch(_) => {
                self.unsupported_yul_stmt(builder, stmt, "switch statement");
            }
            hir_yul::StmtKind::Leave => self.unsupported_yul_stmt(builder, stmt, "leave statement"),
            hir_yul::StmtKind::Break => self.unsupported_yul_stmt(builder, stmt, "break statement"),
            hir_yul::StmtKind::Continue => {
                self.unsupported_yul_stmt(builder, stmt, "continue statement");
            }
            hir_yul::StmtKind::FunctionDef(_) => {
                self.unsupported_yul_stmt(builder, stmt, "function definition");
            }
        }
    }

    fn lower_yul_if(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        cond: &hir_yul::Expr<'_>,
        body: &hir_yul::Block<'_>,
    ) {
        let before_scopes = self.yul_scopes.clone();
        let cond_block = builder.current_block();
        let then_block = builder.create_block();
        let cont_block = builder.create_block();
        let cond = self.lower_yul_expr(builder, cond);
        builder.branch(cond, then_block, cont_block);

        builder.switch_to_block(then_block);
        self.lower_yul_block(builder, body);
        let then_exit = builder.current_block();
        let then_reaches_cont = !builder.func().block(then_exit).is_terminated();
        let then_scopes = self.yul_scopes.clone();
        if then_reaches_cont {
            builder.jump(cont_block);
        }

        builder.switch_to_block(cont_block);
        if then_reaches_cont {
            self.merge_yul_if_scopes(builder, cond_block, then_exit, before_scopes, then_scopes);
        } else {
            self.yul_scopes = before_scopes;
        }
    }

    fn lower_yul_expr(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir_yul::Expr<'_>,
    ) -> ValueId {
        match &expr.kind {
            hir_yul::ExprKind::Path(path) => self.lower_yul_path(builder, path),
            hir_yul::ExprKind::Lit(lit) => self.lower_literal(builder, lit),
            hir_yul::ExprKind::Call(call) => self.lower_yul_call(builder, call),
        }
    }

    fn lower_yul_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        call: &hir_yul::ExprCall<'_>,
    ) -> ValueId {
        let args: Vec<_> =
            call.arguments.iter().map(|arg| self.lower_yul_expr(builder, arg)).collect();
        if let Some(expected) = Self::supported_yul_builtin_arity(call.name.name)
            && args.len() != expected
        {
            self.gcx
                .dcx()
                .err(format!(
                    "wrong number of arguments for Yul builtin `{}`: expected {}, found {}",
                    call.name.name,
                    expected,
                    args.len()
                ))
                .span(call.name.span)
                .emit();
            return builder.imm_u64(0);
        }

        match call.name.name {
            kw::Add => {
                let a = Self::yul_arg(&args, 0);
                let b = Self::yul_arg(&args, 1);
                builder.add(a, b)
            }
            kw::Sub => {
                let a = Self::yul_arg(&args, 0);
                let b = Self::yul_arg(&args, 1);
                builder.sub(a, b)
            }
            kw::Mul => {
                let a = Self::yul_arg(&args, 0);
                let b = Self::yul_arg(&args, 1);
                builder.mul(a, b)
            }
            kw::Div => {
                let a = Self::yul_arg(&args, 0);
                let b = Self::yul_arg(&args, 1);
                builder.div(a, b)
            }
            kw::Sdiv => {
                let a = Self::yul_arg(&args, 0);
                let b = Self::yul_arg(&args, 1);
                builder.sdiv(a, b)
            }
            kw::Mod => {
                let a = Self::yul_arg(&args, 0);
                let b = Self::yul_arg(&args, 1);
                builder.mod_(a, b)
            }
            kw::Smod => {
                let a = Self::yul_arg(&args, 0);
                let b = Self::yul_arg(&args, 1);
                builder.smod(a, b)
            }
            kw::Exp => {
                let a = Self::yul_arg(&args, 0);
                let b = Self::yul_arg(&args, 1);
                builder.exp(a, b)
            }
            kw::And => {
                let a = Self::yul_arg(&args, 0);
                let b = Self::yul_arg(&args, 1);
                builder.and(a, b)
            }
            kw::Or => {
                let a = Self::yul_arg(&args, 0);
                let b = Self::yul_arg(&args, 1);
                builder.or(a, b)
            }
            kw::Xor => {
                let a = Self::yul_arg(&args, 0);
                let b = Self::yul_arg(&args, 1);
                builder.xor(a, b)
            }
            kw::Not => {
                let a = Self::yul_arg(&args, 0);
                builder.not(a)
            }
            kw::Shl => {
                let shift = Self::yul_arg(&args, 0);
                let value = Self::yul_arg(&args, 1);
                builder.shl(shift, value)
            }
            kw::Shr => {
                let shift = Self::yul_arg(&args, 0);
                let value = Self::yul_arg(&args, 1);
                builder.shr(shift, value)
            }
            kw::Sar => {
                let shift = Self::yul_arg(&args, 0);
                let value = Self::yul_arg(&args, 1);
                builder.sar(shift, value)
            }
            kw::Lt => {
                let a = Self::yul_arg(&args, 0);
                let b = Self::yul_arg(&args, 1);
                builder.lt(a, b)
            }
            kw::Slt => {
                let a = Self::yul_arg(&args, 0);
                let b = Self::yul_arg(&args, 1);
                builder.slt(a, b)
            }
            kw::Gt => {
                let a = Self::yul_arg(&args, 0);
                let b = Self::yul_arg(&args, 1);
                builder.gt(a, b)
            }
            kw::Sgt => {
                let a = Self::yul_arg(&args, 0);
                let b = Self::yul_arg(&args, 1);
                builder.sgt(a, b)
            }
            kw::Eq => {
                let a = Self::yul_arg(&args, 0);
                let b = Self::yul_arg(&args, 1);
                builder.eq(a, b)
            }
            kw::Iszero => {
                let a = Self::yul_arg(&args, 0);
                builder.iszero(a)
            }
            kw::Byte => {
                let index = Self::yul_arg(&args, 0);
                let value = Self::yul_arg(&args, 1);
                Self::lower_yul_byte(builder, index, value)
            }
            kw::Mload => {
                let offset = Self::yul_arg(&args, 0);
                builder.mload(offset)
            }
            kw::Mstore => {
                let offset = Self::yul_arg(&args, 0);
                let value = Self::yul_arg(&args, 1);
                builder.mstore(offset, value);
                builder.imm_u64(0)
            }
            kw::Mstore8 => {
                let offset = Self::yul_arg(&args, 0);
                let value = Self::yul_arg(&args, 1);
                builder.mstore8(offset, value);
                builder.imm_u64(0)
            }
            kw::Msize => builder.msize(),
            kw::Mcopy => {
                let dest = Self::yul_arg(&args, 0);
                let src = Self::yul_arg(&args, 1);
                let len = Self::yul_arg(&args, 2);
                builder.mcopy(dest, src, len);
                builder.imm_u64(0)
            }
            kw::Sload => {
                let slot = Self::yul_arg(&args, 0);
                builder.sload(slot)
            }
            kw::Sstore => {
                let slot = Self::yul_arg(&args, 0);
                let value = Self::yul_arg(&args, 1);
                builder.sstore(slot, value);
                builder.imm_u64(0)
            }
            kw::Tload => {
                let slot = Self::yul_arg(&args, 0);
                builder.tload(slot)
            }
            kw::Tstore => {
                let slot = Self::yul_arg(&args, 0);
                let value = Self::yul_arg(&args, 1);
                builder.tstore(slot, value);
                builder.imm_u64(0)
            }
            kw::Calldataload => {
                let offset = Self::yul_arg(&args, 0);
                builder.calldataload(offset)
            }
            kw::Calldatasize => builder.calldatasize(),
            kw::Calldatacopy => {
                let dest = Self::yul_arg(&args, 0);
                let offset = Self::yul_arg(&args, 1);
                let size = Self::yul_arg(&args, 2);
                builder.calldatacopy(dest, offset, size);
                builder.imm_u64(0)
            }
            kw::Extcodesize => {
                let addr = Self::yul_arg(&args, 0);
                builder.extcodesize(addr)
            }
            kw::Extcodecopy => {
                let addr = Self::yul_arg(&args, 0);
                let dest = Self::yul_arg(&args, 1);
                let offset = Self::yul_arg(&args, 2);
                let size = Self::yul_arg(&args, 3);
                builder.extcodecopy(addr, dest, offset, size);
                builder.imm_u64(0)
            }
            kw::Extcodehash => {
                let addr = Self::yul_arg(&args, 0);
                builder.extcodehash(addr)
            }
            kw::Returndatasize => builder.returndatasize(),
            kw::Returndatacopy => {
                let dest = Self::yul_arg(&args, 0);
                let offset = Self::yul_arg(&args, 1);
                let size = Self::yul_arg(&args, 2);
                builder.returndatacopy(dest, offset, size);
                builder.imm_u64(0)
            }
            kw::Address => builder.address(),
            kw::Balance => {
                let addr = Self::yul_arg(&args, 0);
                builder.balance(addr)
            }
            kw::Selfbalance => builder.selfbalance(),
            kw::Caller => builder.caller(),
            kw::Callvalue => builder.callvalue(),
            kw::Origin => builder.origin(),
            kw::Gasprice => builder.gasprice(),
            kw::Blockhash => {
                let block_num = Self::yul_arg(&args, 0);
                builder.blockhash(block_num)
            }
            kw::Coinbase => builder.coinbase(),
            kw::Timestamp => builder.timestamp(),
            kw::Number => builder.number(),
            kw::Difficulty | kw::Prevrandao => builder.prevrandao(),
            kw::Gaslimit => builder.gaslimit(),
            kw::Chainid => builder.chainid(),
            kw::Gas => builder.gas(),
            kw::Basefee => builder.basefee(),
            kw::Blobbasefee => builder.blobbasefee(),
            kw::Blobhash => {
                let index = Self::yul_arg(&args, 0);
                builder.blobhash(index)
            }
            kw::Keccak256 => {
                let offset = Self::yul_arg(&args, 0);
                let size = Self::yul_arg(&args, 1);
                builder.keccak256(offset, size)
            }
            kw::Call => {
                let gas = Self::yul_arg(&args, 0);
                let addr = Self::yul_arg(&args, 1);
                let value = Self::yul_arg(&args, 2);
                let args_offset = Self::yul_arg(&args, 3);
                let args_size = Self::yul_arg(&args, 4);
                let ret_offset = Self::yul_arg(&args, 5);
                let ret_size = Self::yul_arg(&args, 6);
                builder.call(gas, addr, value, args_offset, args_size, ret_offset, ret_size)
            }
            kw::Staticcall => {
                let gas = Self::yul_arg(&args, 0);
                let addr = Self::yul_arg(&args, 1);
                let args_offset = Self::yul_arg(&args, 2);
                let args_size = Self::yul_arg(&args, 3);
                let ret_offset = Self::yul_arg(&args, 4);
                let ret_size = Self::yul_arg(&args, 5);
                builder.staticcall(gas, addr, args_offset, args_size, ret_offset, ret_size)
            }
            kw::Delegatecall => {
                let gas = Self::yul_arg(&args, 0);
                let addr = Self::yul_arg(&args, 1);
                let args_offset = Self::yul_arg(&args, 2);
                let args_size = Self::yul_arg(&args, 3);
                let ret_offset = Self::yul_arg(&args, 4);
                let ret_size = Self::yul_arg(&args, 5);
                builder.delegatecall(gas, addr, args_offset, args_size, ret_offset, ret_size)
            }
            kw::Create => {
                let value = Self::yul_arg(&args, 0);
                let offset = Self::yul_arg(&args, 1);
                let size = Self::yul_arg(&args, 2);
                builder.create(value, offset, size)
            }
            kw::Create2 => {
                let value = Self::yul_arg(&args, 0);
                let offset = Self::yul_arg(&args, 1);
                let size = Self::yul_arg(&args, 2);
                let salt = Self::yul_arg(&args, 3);
                builder.create2(value, offset, size, salt)
            }
            kw::Log0 => {
                let offset = Self::yul_arg(&args, 0);
                let size = Self::yul_arg(&args, 1);
                builder.log0(offset, size);
                builder.imm_u64(0)
            }
            kw::Log1 => {
                let offset = Self::yul_arg(&args, 0);
                let size = Self::yul_arg(&args, 1);
                let topic1 = Self::yul_arg(&args, 2);
                builder.log1(offset, size, topic1);
                builder.imm_u64(0)
            }
            kw::Log2 => {
                let offset = Self::yul_arg(&args, 0);
                let size = Self::yul_arg(&args, 1);
                let topic1 = Self::yul_arg(&args, 2);
                let topic2 = Self::yul_arg(&args, 3);
                builder.log2(offset, size, topic1, topic2);
                builder.imm_u64(0)
            }
            kw::Log3 => {
                let offset = Self::yul_arg(&args, 0);
                let size = Self::yul_arg(&args, 1);
                let topic1 = Self::yul_arg(&args, 2);
                let topic2 = Self::yul_arg(&args, 3);
                let topic3 = Self::yul_arg(&args, 4);
                builder.log3(offset, size, topic1, topic2, topic3);
                builder.imm_u64(0)
            }
            kw::Log4 => {
                let offset = Self::yul_arg(&args, 0);
                let size = Self::yul_arg(&args, 1);
                let topic1 = Self::yul_arg(&args, 2);
                let topic2 = Self::yul_arg(&args, 3);
                let topic3 = Self::yul_arg(&args, 4);
                let topic4 = Self::yul_arg(&args, 5);
                builder.log4(offset, size, topic1, topic2, topic3, topic4);
                builder.imm_u64(0)
            }
            kw::Revert => {
                let offset = Self::yul_arg(&args, 0);
                let size = Self::yul_arg(&args, 1);
                builder.revert(offset, size);
                builder.imm_u64(0)
            }
            kw::Stop => {
                builder.stop();
                builder.imm_u64(0)
            }
            kw::Invalid => {
                builder.invalid();
                builder.imm_u64(0)
            }
            kw::Pop => builder.imm_u64(0),
            _ => {
                let message = match call.res {
                    hir_yul::CallRes::Builtin => {
                        format!("unsupported Yul builtin `{}`", call.name.name)
                    }
                    hir_yul::CallRes::Function => {
                        format!("unsupported Yul function `{}`", call.name.name)
                    }
                    hir_yul::CallRes::Unknown => {
                        format!("undefined Yul function `{}`", call.name.name)
                    }
                };
                self.gcx.dcx().err(message).span(call.name.span).emit();
                builder.imm_u64(0)
            }
        }
    }

    fn lower_yul_byte(
        builder: &mut FunctionBuilder<'_>,
        index: ValueId,
        value: ValueId,
    ) -> ValueId {
        let in_range = {
            let limit = builder.imm_u64(32);
            builder.lt(index, limit)
        };
        let shift = {
            let last = builder.imm_u64(31);
            let byte_width = builder.imm_u64(8);
            let from_right = builder.sub(last, index);
            builder.mul(from_right, byte_width)
        };
        let shifted = builder.shr(shift, value);
        let mask = builder.imm_u64(0xff);
        let byte = builder.and(shifted, mask);
        let zero = builder.imm_u64(0);
        builder.select(in_range, byte, zero)
    }

    fn yul_arg(args: &[ValueId], index: usize) -> ValueId {
        args[index]
    }

    fn supported_yul_builtin_arity(name: Symbol) -> Option<usize> {
        Some(match name {
            kw::Add
            | kw::Sub
            | kw::Mul
            | kw::Div
            | kw::Sdiv
            | kw::Mod
            | kw::Smod
            | kw::Exp
            | kw::And
            | kw::Or
            | kw::Xor
            | kw::Shl
            | kw::Shr
            | kw::Sar
            | kw::Lt
            | kw::Slt
            | kw::Gt
            | kw::Sgt
            | kw::Eq
            | kw::Byte => 2,
            kw::Not
            | kw::Iszero
            | kw::Mload
            | kw::Sload
            | kw::Tload
            | kw::Calldataload
            | kw::Extcodesize
            | kw::Extcodehash
            | kw::Balance
            | kw::Blockhash
            | kw::Blobhash
            | kw::Pop => 1,
            kw::Mstore
            | kw::Mstore8
            | kw::Sstore
            | kw::Tstore
            | kw::Keccak256
            | kw::Log0
            | kw::Revert => 2,
            kw::Mcopy | kw::Calldatacopy | kw::Returndatacopy | kw::Create | kw::Log1 => 3,
            kw::Extcodecopy | kw::Create2 | kw::Log2 => 4,
            kw::Log3 => 5,
            kw::Staticcall | kw::Delegatecall | kw::Log4 => 6,
            kw::Call => 7,
            kw::Msize
            | kw::Calldatasize
            | kw::Returndatasize
            | kw::Address
            | kw::Selfbalance
            | kw::Caller
            | kw::Callvalue
            | kw::Origin
            | kw::Gasprice
            | kw::Coinbase
            | kw::Timestamp
            | kw::Number
            | kw::Difficulty
            | kw::Prevrandao
            | kw::Gaslimit
            | kw::Chainid
            | kw::Gas
            | kw::Basefee
            | kw::Blobbasefee
            | kw::Stop
            | kw::Invalid => 0,
            _ => return None,
        })
    }

    fn unsupported_yul_stmt(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        stmt: &hir_yul::Stmt<'_>,
        kind: &str,
    ) {
        self.gcx.dcx().err(format!("unsupported Yul {kind}")).span(stmt.span).emit();
        builder.invalid();
    }

    fn merge_yul_if_scopes(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        false_pred: BlockId,
        then_pred: BlockId,
        before_scopes: Vec<FxHashMap<Symbol, ValueId>>,
        then_scopes: Vec<FxHashMap<Symbol, ValueId>>,
    ) {
        self.yul_scopes = before_scopes;
        if self.yul_scopes.len() != then_scopes.len() {
            return;
        }

        for (scope_idx, then_scope) in then_scopes.iter().enumerate() {
            for (&name, &then_value) in then_scope {
                let Some(&before_value) = self.yul_scopes[scope_idx].get(&name) else {
                    continue;
                };
                if before_value == then_value {
                    continue;
                }

                let ty = Self::value_ty(builder, before_value);
                let phi =
                    builder.phi(ty, vec![(false_pred, before_value), (then_pred, then_value)]);
                self.yul_scopes[scope_idx].insert(name, phi);
            }
        }
    }

    fn value_ty(builder: &FunctionBuilder<'_>, value: ValueId) -> MirType {
        match builder.func().value(value) {
            Value::Inst(inst) => {
                builder.func().instruction(*inst).result_ty.unwrap_or(MirType::uint256())
            }
            value => value.ty(),
        }
    }

    fn enter_yul_scope(&mut self) {
        self.yul_scopes.push(Default::default());
    }

    fn exit_yul_scope(&mut self) {
        self.yul_scopes.pop();
    }

    fn declare_yul_variable(&mut self, name: Symbol, value: ValueId) {
        let scope = self.yul_scopes.last_mut().expect("missing Yul scope");
        scope.insert(name, value);
    }

    fn lookup_yul_variable(&self, name: Symbol) -> Option<ValueId> {
        self.yul_scopes.iter().rev().find_map(|scope| scope.get(&name).copied())
    }

    fn assign_yul_variable(&mut self, name: Symbol, value: ValueId) -> bool {
        for scope in self.yul_scopes.iter_mut().rev() {
            if let Some(local) = scope.get_mut(&name) {
                *local = value;
                return true;
            }
        }
        false
    }

    fn yul_path_symbol(path: &hir_yul::Path<'_>) -> Option<Symbol> {
        match path.segments {
            [ident] => Some(ident.name),
            _ => None,
        }
    }

    fn undefined_yul_variable(&mut self, path: &hir_yul::Path<'_>, name: Symbol) {
        self.gcx.dcx().err(format!("undefined Yul variable `{name}`")).span(path.span).emit();
    }

    fn unsupported_yul_path(&mut self, path: &hir_yul::Path<'_>, kind: &str) {
        self.gcx.dcx().err(format!("unsupported Yul {kind}")).span(path.span).emit();
    }

    fn lower_yul_path(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        path: &hir_yul::Path<'_>,
    ) -> ValueId {
        match path.res {
            hir_yul::PathRes::SolidityVariable(var_id) => {
                self.lower_solidity_variable_by_id(builder, var_id)
            }
            hir_yul::PathRes::StorageSlot(var_id) => {
                let slot = self.storage_slots.get(&var_id).copied().unwrap_or(0);
                builder.imm_u64(slot)
            }
            hir_yul::PathRes::StorageOffset(_) => builder.imm_u64(0),
            hir_yul::PathRes::YulVariable => {
                let Some(name) = Self::yul_path_symbol(path) else {
                    self.unsupported_yul_path(path, "path expression");
                    return builder.imm_u64(0);
                };
                match self.lookup_yul_variable(name) {
                    Some(value) => value,
                    None => {
                        self.undefined_yul_variable(path, name);
                        builder.imm_u64(0)
                    }
                }
            }
            hir_yul::PathRes::Err => builder.imm_u64(0),
        }
    }

    fn lower_solidity_variable_by_id(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        var_id: hir::VariableId,
    ) -> ValueId {
        if let Some(&val) = self.locals.get(&var_id) {
            return val;
        }

        if let Some(offset) = self.get_local_memory_offset(&var_id) {
            let offset_val = self.local_memory_addr(builder, offset);
            return builder.mload(offset_val);
        }

        if let Some(&slot) = self.storage_slots.get(&var_id) {
            let slot_val = builder.imm_u64(slot);
            return builder.sload(slot_val);
        }

        builder.imm_u64(0)
    }

    fn assign_yul_path(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        path: &hir_yul::Path<'_>,
        value: ValueId,
    ) {
        match path.res {
            hir_yul::PathRes::YulVariable => {
                let Some(name) = Self::yul_path_symbol(path) else {
                    self.unsupported_yul_path(path, "assignment target");
                    return;
                };
                if !self.assign_yul_variable(name, value) {
                    self.undefined_yul_variable(path, name);
                }
            }
            hir_yul::PathRes::SolidityVariable(var_id) => {
                if let Some(offset) = self.get_local_memory_offset(&var_id) {
                    let offset_val = self.local_memory_addr(builder, offset);
                    builder.mstore(offset_val, value);
                } else if let Some(local) = self.locals.get_mut(&var_id) {
                    *local = value;
                } else if let Some(&slot) = self.storage_slots.get(&var_id) {
                    let slot_val = builder.imm_u64(slot);
                    builder.sstore(slot_val, value);
                }
            }
            hir_yul::PathRes::StorageSlot(_) => {
                self.unsupported_yul_path(path, "storage slot assignment target");
            }
            hir_yul::PathRes::StorageOffset(_) => {
                self.unsupported_yul_path(path, "storage offset assignment target");
            }
            hir_yul::PathRes::Err => {}
        }
    }

    /// Lowers a single variable declaration.
    /// Variables that are never assigned after declaration and don't involve external calls
    /// are kept as SSA values. Variables that are assigned later or initialized from external
    /// calls (which use shared memory) are stored in memory.
    fn lower_single_var_decl(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        var_id: hir::VariableId,
    ) {
        let var = self.gcx.hir.variable(var_id);
        let _ty = self.lower_type_from_var(var);

        // Check if initializer involves external calls (results stored in shared memory)
        let has_external_call = var.initializer.is_some_and(|init| self.has_external_call(init));

        // Check if this is a struct type - struct returns from external calls are already
        // allocated in proper memory, so they don't need extra local memory storage
        let is_struct_type = matches!(var.ty.kind, hir::TypeKind::Custom(hir::ItemId::Struct(_)));

        let initial_value = if let Some(init) = var.initializer {
            self.lower_expr(builder, init)
        } else if let hir::TypeKind::Custom(hir::ItemId::Struct(_)) = &var.ty.kind {
            // Struct without initializer: allocate memory and zero-initialize
            let struct_size = self.memory_struct_size(&var.ty);
            let struct_ptr = self.allocate_memory(builder, struct_size);
            self.zero_initialize_memory_value(builder, &var.ty, struct_ptr);
            struct_ptr
        } else {
            builder.imm_u256(U256::ZERO)
        };

        // Variables need memory storage if:
        // 1. They are assigned after declaration, OR
        // 2. They are initialized from external calls (which write to shared memory at offset 0)
        //    EXCEPT for struct types, which already have properly allocated memory
        let needs_local_memory =
            self.is_var_assigned(&var_id) || (has_external_call && !is_struct_type);

        if needs_local_memory {
            let offset = self.alloc_local_memory(var_id);
            let offset_val = self.local_memory_addr(builder, offset);
            builder.mstore(offset_val, initial_value);
        } else {
            // Variable is never reassigned and not from external call - keep as SSA value
            self.locals.insert(var_id, initial_value);
        }
    }

    fn memory_struct_size(&self, ty: &hir::Type<'_>) -> u64 {
        if matches!(ty.kind, hir::TypeKind::Custom(hir::ItemId::Struct(_))) {
            self.calculate_memory_words_for_type(ty) * 32
        } else {
            32
        }
    }

    fn zero_initialize_memory_value(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: &hir::Type<'_>,
        ptr: ValueId,
    ) {
        let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &ty.kind else {
            let zero = builder.imm_u256(U256::ZERO);
            builder.mstore(ptr, zero);
            return;
        };

        let strukt = self.gcx.hir.strukt(*struct_id);
        for (i, &field_id) in strukt.fields.iter().enumerate() {
            let field = self.gcx.hir.variable(field_id);
            let value = self.zero_memory_field_value(builder, &field.ty);
            let field_offset = (i as u64) * 32;
            if field_offset == 0 {
                builder.mstore(ptr, value);
            } else {
                let offset_val = builder.imm_u64(field_offset);
                let field_addr = builder.add(ptr, offset_val);
                builder.mstore(field_addr, value);
            }
        }
    }

    fn zero_memory_field_value(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: &hir::Type<'_>,
    ) -> ValueId {
        match &ty.kind {
            hir::TypeKind::Array(array) if array.size.is_none() => {
                let ptr = self.allocate_memory(builder, 32);
                let zero = builder.imm_u256(U256::ZERO);
                builder.mstore(ptr, zero);
                ptr
            }
            hir::TypeKind::Custom(hir::ItemId::Struct(_)) => {
                let ptr = self.allocate_memory(builder, self.memory_struct_size(ty));
                self.zero_initialize_memory_value(builder, ty, ptr);
                ptr
            }
            _ => builder.imm_u256(U256::ZERO),
        }
    }

    /// Lowers a multi-variable declaration.
    /// For external calls with multiple returns, the return data is written to memory
    /// at offsets 0, 32, 64, etc. after the CALL instruction.
    pub(super) fn lower_multi_var_decl(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        var_ids: &[Option<hir::VariableId>],
        init: &hir::Expr<'_>,
    ) {
        // lower_expr for an external call returns the first value (from memory offset 0)
        // and leaves additional return values at memory offsets 32, 64, etc.
        let first_val = self.lower_expr(builder, init);

        for (i, var_id_opt) in var_ids.iter().enumerate() {
            if let Some(var_id) = var_id_opt {
                let val = if i == 0 {
                    first_val
                } else {
                    // Read additional return values from memory at offset i * 32
                    let mem_offset = builder.imm_u64((i * 32) as u64);
                    builder.mload(mem_offset)
                };
                // Allocate memory slot and store value
                let offset = self.alloc_local_memory(*var_id);
                let offset_val = self.local_memory_addr(builder, offset);
                builder.mstore(offset_val, val);
            }
        }
    }

    /// Lowers an if statement.
    fn lower_if(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        cond: &hir::Expr<'_>,
        then_stmt: &hir::Stmt<'_>,
        else_stmt: Option<&hir::Stmt<'_>>,
    ) {
        let cond_val = self.lower_expr(builder, cond);

        let then_block = builder.create_block();
        let merge_block = builder.create_block();
        let else_block = if else_stmt.is_some() { builder.create_block() } else { merge_block };

        builder.branch(cond_val, then_block, else_block);

        builder.switch_to_block(then_block);
        self.lower_stmt(builder, then_stmt);
        if !builder.func().block(builder.current_block()).is_terminated() {
            builder.jump(merge_block);
        }

        if let Some(else_stmt) = else_stmt {
            builder.switch_to_block(else_block);
            self.lower_stmt(builder, else_stmt);
            if !builder.func().block(builder.current_block()).is_terminated() {
                builder.jump(merge_block);
            }
        }

        builder.switch_to_block(merge_block);
    }

    /// Lowers a loop statement (desugared from for/while/do-while).
    fn lower_loop(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        block: &hir::Block<'_>,
        source: hir::LoopSource,
    ) {
        let loop_block = builder.create_block();
        let exit_block = builder.create_block();

        // For `for` loops, we need a separate update block for `continue` to jump to.
        // The desugared structure is: if (cond) { body; update; } else { break; }
        // We need to handle the update separately so continue jumps to it.
        let (continue_target, is_for_with_update) = if source == hir::LoopSource::For {
            if self.is_for_loop_with_update(block) {
                let update_block = builder.create_block();
                (update_block, true)
            } else {
                (loop_block, false)
            }
        } else {
            (loop_block, false)
        };

        // Push loop context for break/continue
        self.push_loop(LoopContext { break_target: exit_block, continue_target });

        builder.jump(loop_block);

        builder.switch_to_block(loop_block);

        // For for loops with update, lower body without the update, then emit update block
        if is_for_with_update {
            self.lower_for_loop_body(builder, block, continue_target, loop_block);
        } else {
            self.lower_block(builder, block);
            if !builder.func().block(builder.current_block()).is_terminated() {
                builder.jump(loop_block);
            }
        }

        // Pop loop context
        self.pop_loop();

        builder.switch_to_block(exit_block);
    }

    /// Checks if a for loop has an update expression in the expected desugared structure.
    fn is_for_loop_with_update(&self, block: &hir::Block<'_>) -> bool {
        let stmts = block.stmts;
        if stmts.len() != 1 {
            return false;
        }

        let StmtKind::If(_, then_stmt, _) = &stmts[0].kind else {
            return false;
        };

        let StmtKind::Block(b) = &then_stmt.kind else {
            return false;
        };

        // Need at least 2 statements: body and update
        if b.stmts.len() < 2 {
            return false;
        }

        // Last statement should be an expression (the update)
        matches!(b.stmts.last().map(|s| &s.kind), Some(StmtKind::Expr(_)))
    }

    /// Lowers a for loop body with special handling for update expression.
    /// Creates: loop_block -> if(cond) { body -> update_block -> loop_block } else { exit }
    fn lower_for_loop_body(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        block: &hir::Block<'_>,
        update_block: crate::mir::BlockId,
        loop_block: crate::mir::BlockId,
    ) {
        let stmts = block.stmts;

        // Extract the if statement
        let StmtKind::If(cond, then_stmt, else_stmt) = &stmts[0].kind else {
            self.lower_block(builder, block);
            return;
        };

        let StmtKind::Block(then_body) = &then_stmt.kind else {
            self.lower_block(builder, block);
            return;
        };

        // Create blocks for the if
        let then_block = builder.create_block();
        let else_block = builder.create_block();

        let cond_val = self.lower_expr(builder, cond);
        builder.branch(cond_val, then_block, else_block);

        // Then branch: lower all statements except the last (update)
        builder.switch_to_block(then_block);
        let body_stmts = &then_body.stmts[..then_body.stmts.len() - 1];
        for stmt in body_stmts {
            self.lower_stmt(builder, stmt);
        }
        if !builder.func().block(builder.current_block()).is_terminated() {
            builder.jump(update_block);
        }

        // Update block: lower the update expression, then jump to loop
        builder.switch_to_block(update_block);
        if let Some(last_stmt) = then_body.stmts.last() {
            self.lower_stmt(builder, last_stmt);
        }
        if !builder.func().block(builder.current_block()).is_terminated() {
            builder.jump(loop_block);
        }

        // Else branch: should be break
        builder.switch_to_block(else_block);
        if let Some(else_s) = else_stmt {
            self.lower_stmt(builder, else_s);
        }
        // Note: else branch with break will be terminated, no need for explicit jump
    }

    /// Lowers a return statement.
    fn lower_return(&mut self, builder: &mut FunctionBuilder<'_>, value: Option<&hir::Expr<'_>>) {
        if let Some(expr) = value {
            // Check if this is a tuple return (multiple values)
            if let hir::ExprKind::Tuple(elements) = &expr.kind {
                // For multi-value returns, collect all values and pass to ret().
                // The EVM codegen handles storing them to memory at offsets 0, 32, 64, etc.
                let ret_vals: Vec<_> = elements
                    .iter()
                    .filter_map(|elem_opt| {
                        elem_opt.as_ref().map(|elem| self.lower_expr(builder, elem))
                    })
                    .collect();
                builder.ret(ret_vals);
            } else if let Some(arity) = self.get_ternary_tuple_arity(expr) {
                // Ternary expression returning a tuple - values are in scratch memory
                // lower_expr already evaluated the ternary and wrote to scratch memory
                let _ = self.lower_expr(builder, expr);
                let mut ret_vals = Vec::new();
                for i in 0..arity {
                    let offset = builder.imm_u64(i as u64 * 32);
                    let val = builder.mload(offset);
                    ret_vals.push(val);
                }
                builder.ret(ret_vals);
            } else {
                // Check if returning a memory struct - expand to individual fields
                let struct_type = self.get_return_struct_type(expr);
                if let Some(struct_id) = struct_type
                    && builder.func().is_public()
                {
                    let struct_ptr = self.lower_expr(builder, expr);
                    let ret_vals = self.load_struct_return_values(builder, struct_id, struct_ptr);
                    builder.ret(ret_vals);
                } else {
                    let ret_val = self.lower_expr(builder, expr);
                    builder.ret([ret_val]);
                }
            }
        } else {
            builder.ret([]);
        }
    }

    /// Gets the tuple arity if this is a ternary expression with tuple branches.
    fn get_ternary_tuple_arity(&self, expr: &hir::Expr<'_>) -> Option<usize> {
        if let hir::ExprKind::Ternary(_, then_expr, else_expr) = &expr.kind {
            // Check if either branch is a tuple
            if let hir::ExprKind::Tuple(elements) = &then_expr.kind {
                return Some(elements.len());
            }
            if let hir::ExprKind::Tuple(elements) = &else_expr.kind {
                return Some(elements.len());
            }
        }
        None
    }

    /// Gets the struct ID if the expression returns a memory struct.
    fn get_return_struct_type(&self, expr: &hir::Expr<'_>) -> Option<hir::StructId> {
        match &expr.kind {
            // Variable with struct type
            hir::ExprKind::Ident(res_slice) => {
                for res in res_slice.iter() {
                    if let hir::Res::Item(hir::ItemId::Variable(var_id)) = res {
                        let var = self.gcx.hir.variable(*var_id);
                        // Check if this variable has a struct type
                        if let hir::TypeKind::Custom(hir::ItemId::Struct(struct_id)) = &var.ty.kind
                        {
                            return Some(*struct_id);
                        }
                    }
                }
                None
            }
            // Struct constructor
            hir::ExprKind::Call(callee, _, _) => {
                if let hir::ExprKind::Ident(res_slice) = &callee.kind {
                    for res in res_slice.iter() {
                        if let hir::Res::Item(hir::ItemId::Struct(struct_id)) = res {
                            return Some(*struct_id);
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Lowers an emit statement.
    fn lower_emit(&mut self, builder: &mut FunctionBuilder<'_>, expr: &hir::Expr<'_>) {
        // expr is always a Call expression: EventName(args)
        let hir::ExprKind::Call(callee, args, _named) = &expr.kind else {
            return;
        };

        // Get the event from the callee
        let hir::ExprKind::Ident(res_slice) = &callee.kind else {
            return;
        };

        let Some(hir::Res::Item(hir::ItemId::Event(event_id))) = res_slice.first() else {
            return;
        };

        let event = self.gcx.hir.event(*event_id);

        // Compute event signature hash (topic0 for non-anonymous events)
        let sig = self.compute_event_signature(event);
        let sig_hash = alloy_primitives::keccak256(sig.as_bytes());
        let topic0 = builder.imm_u256(alloy_primitives::U256::from_be_bytes(sig_hash.0));

        // Collect indexed parameters (additional topics) and non-indexed (data)
        let mut topics = vec![topic0];
        let mut data_values = Vec::new();

        for (i, param_id) in event.parameters.iter().enumerate() {
            let param = self.gcx.hir.variable(*param_id);
            let arg_expr = args.exprs().nth(i);

            if let Some(arg) = arg_expr {
                let arg_val = self.lower_expr(builder, arg);

                if param.indexed {
                    topics.push(arg_val);
                } else {
                    data_values.push(arg_val);
                }
            }
        }

        // ABI-encode non-indexed data to memory
        let data_size = data_values.len() * 32;
        let mem_offset = builder.imm_u64(0);
        for (i, val) in data_values.iter().enumerate() {
            let offset = builder.imm_u64(i as u64 * 32);
            builder.mstore(offset, *val);
        }
        let size = builder.imm_u64(data_size as u64);

        // Emit the appropriate LOG instruction based on number of topics
        match topics.len() {
            0 => builder.log0(mem_offset, size),
            1 => builder.log1(mem_offset, size, topics[0]),
            2 => builder.log2(mem_offset, size, topics[0], topics[1]),
            3 => builder.log3(mem_offset, size, topics[0], topics[1], topics[2]),
            4 => builder.log4(mem_offset, size, topics[0], topics[1], topics[2], topics[3]),
            _ => {} // More than 4 topics not supported by EVM
        }
    }

    /// Computes the event signature string: "EventName(type1,type2,...)"
    fn compute_event_signature(&self, event: &hir::Event<'_>) -> String {
        let params: Vec<String> = event
            .parameters
            .iter()
            .map(|param_id| {
                let param = self.gcx.hir.variable(*param_id);
                self.type_to_abi_string(&param.ty)
            })
            .collect();
        format!("{}({})", event.name.name, params.join(","))
    }

    /// Converts a HIR type to its ABI string representation
    fn type_to_abi_string(&self, ty: &hir::Type<'_>) -> String {
        match &ty.kind {
            hir::TypeKind::Elementary(elem) => elem.to_abi_str().to_string(),
            hir::TypeKind::Custom(item_id) => {
                // For contracts, use "address"
                if let hir::ItemId::Contract(_) = item_id {
                    "address".to_string()
                } else {
                    "uint256".to_string() // Fallback
                }
            }
            hir::TypeKind::Array(arr) => {
                let inner = self.type_to_abi_string(&arr.element);
                format!("{inner}[]")
            }
            _ => "uint256".to_string(), // Fallback for other types
        }
    }

    /// Lowers a try/catch statement.
    ///
    /// try expr returns (...) { success_block } catch (...) { catch_block }
    ///
    /// EVM semantics:
    /// 1. Execute the call (expr must be an external call)
    /// 2. CALL returns 1 for success, 0 for failure
    /// 3. If success (1), jump to success block
    /// 4. If failure (0), jump to catch block
    fn lower_try(&mut self, builder: &mut FunctionBuilder<'_>, try_stmt: &hir::StmtTry<'_>) {
        // Create blocks for success, catch, and merge
        let success_block = builder.create_block();
        let catch_block = builder.create_block();
        let merge_block = builder.create_block();

        // Lower the call expression and get the success flag.
        // We need to handle the call specially to get the success flag, not the return value.
        let success = self.lower_try_call(builder, &try_stmt.expr);

        // Branch: if success (non-zero), go to success_block, else catch_block
        builder.branch(success, success_block, catch_block);

        // Generate success block (returns clause - always first in clauses)
        builder.switch_to_block(success_block);
        if let Some(returns_clause) = try_stmt.clauses.first() {
            // TODO: Handle return values binding to args
            self.lower_block(builder, &returns_clause.block);
        }
        builder.jump(merge_block);

        // Generate catch block(s)
        builder.switch_to_block(catch_block);
        // The catch clauses are after the first (returns) clause
        for clause in try_stmt.clauses.iter().skip(1) {
            // For simplicity, we execute all catch blocks in sequence
            // A proper impl would check the error selector (Error, Panic, or custom)
            self.lower_block(builder, &clause.block);
        }
        // If no catch clauses (only returns clause), this is just an empty block
        if try_stmt.clauses.len() <= 1 {
            // No catch clause - re-revert
            let zero = builder.imm_u64(0);
            builder.revert(zero, zero);
        } else {
            builder.jump(merge_block);
        }

        // Continue after try/catch
        builder.switch_to_block(merge_block);
    }

    /// Lowers a call expression for try/catch, returning the success flag.
    /// This is different from lower_expr which returns the return value.
    fn lower_try_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> crate::mir::ValueId {
        use hir::ExprKind;

        // The try expression should be a call
        if let ExprKind::Call(callee, args, call_opts) = &expr.kind {
            // Check if this is a member access (external call)
            if let ExprKind::Member(base, member) = &callee.kind {
                return self.lower_try_member_call(builder, base, *member, args, *call_opts);
            }
        }

        // Fallback: lower as normal and use the result
        // This is incorrect but allows compilation to continue
        let result = self.lower_expr(builder, expr);
        let is_zero = builder.iszero(result);
        builder.iszero(is_zero)
    }

    /// Lowers a member call for try/catch, returning the CALL success flag.
    fn lower_try_member_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        base: &hir::Expr<'_>,
        member: solar_interface::Ident,
        args: &hir::CallArgs<'_>,
        call_opts: Option<&[hir::NamedArg<'_>]>,
    ) -> crate::mir::ValueId {
        // Get the selector
        let selector = self.compute_member_selector(base, member);
        let num_returns = self.get_member_function_return_count(base, member);

        // Calculate calldata size
        let num_args = args.exprs().count();
        let calldata_size_bytes = 4 + num_args * 32;

        // Evaluate all arguments FIRST
        let arg_vals: Vec<crate::mir::ValueId> =
            args.exprs().map(|arg| self.lower_expr(builder, arg)).collect();

        // Evaluate the address
        let addr = self.lower_expr(builder, base);

        // Write selector to memory
        let selector_word = U256::from(selector) << 224;
        let selector_val = builder.imm_u256(selector_word);
        let mem_start = builder.imm_u64(0);
        builder.mstore(mem_start, selector_val);

        // Write arguments after selector
        let mut arg_offset = 4u64;
        for arg_val in arg_vals {
            let offset = builder.imm_u64(arg_offset);
            builder.mstore(offset, arg_val);
            arg_offset += 32;
        }

        let calldata_size = builder.imm_u64(calldata_size_bytes as u64);
        let args_offset = builder.imm_u64(0);
        let ret_offset = builder.imm_u64(0);
        let ret_size = builder.imm_u64((num_returns * 32) as u64);
        let gas = builder.gas();
        let value = self.extract_call_value(builder, call_opts);

        // Emit the CALL instruction and return the success flag
        builder.call(gas, addr, value, args_offset, calldata_size, ret_offset, ret_size)
    }
}
