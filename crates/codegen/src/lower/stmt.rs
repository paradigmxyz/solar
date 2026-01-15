//! Statement lowering.

use super::{LoopContext, Lowerer};
use crate::mir::FunctionBuilder;
use alloy_primitives::U256;
use solar_sema::hir::{self, StmtKind};

impl<'gcx> Lowerer<'gcx> {
    /// Lowers a block of statements.
    pub(super) fn lower_block(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        block: &hir::Block<'_>,
    ) {
        for stmt in block.stmts {
            self.lower_stmt(builder, stmt);
        }
    }

    /// Lowers a statement to MIR.
    fn lower_stmt(&mut self, builder: &mut FunctionBuilder<'_>, stmt: &hir::Stmt<'_>) {
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

            StmtKind::Try(_try_stmt) => {}

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

            StmtKind::Err(_) => {}
        }
    }

    /// Lowers a single variable declaration.
    /// Local variables are stored in memory to support mutation in loops.
    fn lower_single_var_decl(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        var_id: hir::VariableId,
    ) {
        let var = self.gcx.hir.variable(var_id);
        let _ty = self.lower_type_from_var(var);

        let initial_value = if let Some(init) = var.initializer {
            self.lower_expr(builder, init)
        } else {
            builder.imm_u256(U256::ZERO)
        };

        // Allocate memory slot and store initial value
        let offset = self.alloc_local_memory(var_id);
        let offset_val = builder.imm_u64(offset);
        builder.mstore(offset_val, initial_value);
    }

    /// Lowers a multi-variable declaration.
    fn lower_multi_var_decl(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        var_ids: &[Option<hir::VariableId>],
        init: &hir::Expr<'_>,
    ) {
        let init_val = self.lower_expr(builder, init);

        for (i, var_id_opt) in var_ids.iter().enumerate() {
            if let Some(var_id) = var_id_opt {
                let val = if i == 0 { init_val } else { builder.imm_u256(U256::ZERO) };
                // Allocate memory slot and store value
                let offset = self.alloc_local_memory(*var_id);
                let offset_val = builder.imm_u64(offset);
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
        _source: hir::LoopSource,
    ) {
        let loop_block = builder.create_block();
        let exit_block = builder.create_block();

        // Push loop context for break/continue
        self.push_loop(LoopContext { break_target: exit_block, continue_target: loop_block });

        builder.jump(loop_block);

        builder.switch_to_block(loop_block);
        self.lower_block(builder, block);
        if !builder.func().block(builder.current_block()).is_terminated() {
            builder.jump(loop_block);
        }

        // Pop loop context
        self.pop_loop();

        builder.switch_to_block(exit_block);
    }

    /// Lowers a return statement.
    fn lower_return(&mut self, builder: &mut FunctionBuilder<'_>, value: Option<&hir::Expr<'_>>) {
        if let Some(expr) = value {
            let ret_val = self.lower_expr(builder, expr);
            builder.ret([ret_val]);
        } else {
            builder.ret([]);
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
}
