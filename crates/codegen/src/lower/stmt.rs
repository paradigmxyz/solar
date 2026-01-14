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

            StmtKind::Emit(_expr) => {}

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
        self.push_loop(LoopContext {
            break_target: exit_block,
            continue_target: loop_block,
        });

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
}
