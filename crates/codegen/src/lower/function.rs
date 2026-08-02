//! Function-level HIR to MIR lowering.

use super::{contract, storage::StorageLayout, types};
use crate::mir::{AbiLayout, AbiParamLayout, BlockId, Function, FunctionBuilder, Module, ValueId};
use alloy_primitives::U256;
use solar_ast::{BinOpKind, LitKind, UnOpKind};
use solar_data_structures::map::FxHashMap;
use solar_interface::Span;
use solar_sema::{
    Gcx,
    eval::ConstValue,
    hir::{self, ExprKind, LoopSource, StmtKind, VariableId},
};

/// Lowers one HIR function into a typed MIR function.
pub(super) fn lower(
    gcx: Gcx<'_>,
    module: &mut Module,
    storage: &StorageLayout,
    id: hir::FunctionId,
) -> Option<Function> {
    let hir_function = gcx.hir.function(id);
    let mut mir = contract::declaration(gcx, id, hir_function);
    let mut type_lowerer = types::TypeLowerer::new(gcx);

    let input_shapes = hir_function
        .parameters
        .iter()
        .map(|&param| type_lowerer.abi_param_type(gcx.type_of_item(param.into())))
        .collect::<Option<Vec<_>>>();
    let output_shapes = hir_function
        .returns
        .iter()
        .map(|&ret| type_lowerer.abi_type(gcx.type_of_item(ret.into())))
        .collect::<Option<Vec<_>>>();

    if mir.selector.is_some() {
        let Some(input_shapes) = input_shapes else {
            return report_unsupported(gcx, hir_function.span, "function parameter shape");
        };
        let Some(output_shapes) = output_shapes else {
            return report_unsupported(gcx, hir_function.span, "function return shape");
        };
        mir.abi_params = Some(AbiParamLayout::new(input_shapes.into_boxed_slice()));
        mir.abi_returns =
            Some(module.intern_abi_layout(AbiLayout::new(output_shapes.into_boxed_slice())));
        mir.abi_args_lazy = true;
    }

    let mut lowerer = FunctionLowerer::new(gcx, storage, &mut mir);
    lowerer.bind_signature(hir_function);
    if let Some(body) = hir_function.body {
        lowerer.lower_block(body)?;
    }
    if !lowerer.is_terminated() {
        lowerer.finish(hir_function.returns);
    }
    Some(mir)
}

/// The mutable state for one function lowering.
///
/// Keeping the HIR context, variable environment, loop targets, and builder in
/// one object makes scope changes explicit. Child lowering methods do not need
/// to pass a growing collection of loosely related maps and flags.
struct FunctionLowerer<'gcx, 'mir> {
    gcx: Gcx<'gcx>,
    storage: &'mir StorageLayout,
    builder: FunctionBuilder<'mir>,
    values: FxHashMap<VariableId, ValueId>,
    returns: Vec<VariableId>,
    loops: Vec<LoopTargets>,
}

#[derive(Clone, Copy)]
struct LoopTargets {
    break_block: BlockId,
    continue_block: BlockId,
}

impl<'gcx, 'mir> FunctionLowerer<'gcx, 'mir> {
    fn new(gcx: Gcx<'gcx>, storage: &'mir StorageLayout, function: &'mir mut Function) -> Self {
        Self {
            gcx,
            storage,
            builder: FunctionBuilder::new(function),
            values: FxHashMap::default(),
            returns: Vec::new(),
            loops: Vec::new(),
        }
    }

    fn bind_signature(&mut self, function: &hir::Function<'_>) {
        for &param in function.parameters {
            let value = self
                .builder
                .add_param(types::TypeLowerer::mir_type(self.gcx.type_of_item(param.into())));
            self.values.insert(param, value);
        }
        for &ret in function.returns {
            self.builder
                .add_return(types::TypeLowerer::mir_type(self.gcx.type_of_item(ret.into())));
            let zero = self.builder.imm_u256(U256::ZERO);
            self.values.insert(ret, zero);
        }
        self.returns.extend_from_slice(function.returns);
    }

    fn finish(&mut self, returns: &[VariableId]) {
        if returns.is_empty() {
            self.builder.stop();
        } else {
            self.builder.ret(returns.iter().filter_map(|id| self.values.get(id).copied()));
        }
    }

    fn is_terminated(&self) -> bool {
        self.builder.func().block(self.builder.current_block()).terminator.is_some()
    }

    fn lower_block(&mut self, block: hir::Block<'_>) -> Option<()> {
        for stmt in block.stmts {
            if self.is_terminated() {
                break;
            }
            self.lower_stmt(stmt)?;
        }
        Some(())
    }

    fn lower_stmt(&mut self, stmt: &hir::Stmt<'_>) -> Option<()> {
        match &stmt.kind {
            StmtKind::DeclSingle(id) => {
                let initializer = self.gcx.hir.variable(*id).initializer;
                let value = if let Some(expr) = initializer {
                    self.lower_expr(expr)?
                } else {
                    self.builder.imm_u256(U256::ZERO)
                };
                self.values.insert(*id, value);
            }
            StmtKind::DeclMulti(ids, expr) => {
                let values = self.lower_values(expr)?;
                for (id, value) in ids.iter().flatten().zip(values) {
                    self.values.insert(*id, value);
                }
            }
            StmtKind::Expr(expr) => {
                self.lower_expr(expr)?;
            }
            StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => self.lower_block(*block)?,
            StmtKind::If(cond, then_stmt, else_stmt) => {
                self.lower_if(cond, then_stmt, *else_stmt)?;
            }
            StmtKind::Loop(block, source) => self.lower_loop(*block, *source)?,
            StmtKind::Break => {
                let Some(targets) = self.loops.last().copied() else {
                    return report_unsupported(self.gcx, stmt.span, "break outside loop");
                };
                self.builder.jump(targets.break_block);
            }
            StmtKind::Continue => {
                let Some(targets) = self.loops.last().copied() else {
                    return report_unsupported(self.gcx, stmt.span, "continue outside loop");
                };
                self.builder.jump(targets.continue_block);
            }
            StmtKind::Return(expr) => {
                let values =
                    expr.map_or_else(|| Some(Vec::new()), |expr| self.lower_values(expr))?;
                self.builder.ret(values);
            }
            StmtKind::Revert(_) => {
                let zero = self.builder.imm_u256(U256::ZERO);
                self.builder.revert(zero, zero);
            }
            StmtKind::AssemblyBlock(block) => self.lower_block(*block)?,
            StmtKind::Placeholder => {
                return report_unsupported(self.gcx, stmt.span, "modifier placeholder");
            }
            StmtKind::Emit(_) | StmtKind::Switch(_) | StmtKind::Try(_) | StmtKind::Err(_) => {
                return report_unsupported(self.gcx, stmt.span, "statement");
            }
        }
        Some(())
    }

    fn lower_if(
        &mut self,
        condition: &hir::Expr<'_>,
        then_stmt: &hir::Stmt<'_>,
        else_stmt: Option<&hir::Stmt<'_>>,
    ) -> Option<()> {
        let condition = self.lower_expr(condition)?;
        let then_block = self.builder.create_block();
        let else_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        let before = self.values.clone();
        self.builder.branch(condition, then_block, else_block);

        self.builder.switch_to_block(then_block);
        self.lower_stmt(then_stmt)?;
        let then_terminated = self.is_terminated();
        let then_values = self.values.clone();
        if !then_terminated {
            self.builder.jump(merge_block);
        }

        self.values = before.clone();
        self.builder.switch_to_block(else_block);
        if let Some(stmt) = else_stmt {
            self.lower_stmt(stmt)?;
        }
        let else_terminated = self.is_terminated();
        let else_values = self.values.clone();
        if !else_terminated {
            self.builder.jump(merge_block);
        }

        self.builder.switch_to_block(merge_block);
        self.values = merge_values(
            &mut self.builder,
            before,
            then_block,
            then_values,
            then_terminated,
            else_block,
            else_values,
            else_terminated,
        );
        Some(())
    }

    fn lower_loop(&mut self, block: hir::Block<'_>, _source: LoopSource) -> Option<()> {
        let header = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);
        self.builder.switch_to_block(header);
        self.loops.push(LoopTargets { break_block: exit, continue_block: header });
        self.lower_block(block)?;
        self.loops.pop();
        if !self.is_terminated() {
            self.builder.jump(header);
        }
        self.builder.switch_to_block(exit);
        Some(())
    }

    fn lower_values(&mut self, expr: &hir::Expr<'_>) -> Option<Vec<ValueId>> {
        match &expr.kind {
            ExprKind::Tuple(values) => {
                values.iter().flatten().map(|expr| self.lower_expr(expr)).collect()
            }
            _ => Some(vec![self.lower_expr(expr)?]),
        }
    }

    fn lower_expr(&mut self, expr: &hir::Expr<'_>) -> Option<ValueId> {
        match &expr.kind {
            ExprKind::Lit(lit) => self.lower_literal(lit.kind, expr.span),
            ExprKind::Ident(_) => {
                let id = self.gcx.resolved_variable(expr)?;
                self.load_variable(id, expr.span)
            }
            ExprKind::Binary(lhs, op, rhs) => {
                let lhs = self.lower_expr(lhs)?;
                let rhs = self.lower_expr(rhs)?;
                Some(self.binary(op.kind, lhs, rhs))
            }
            ExprKind::Unary(op, value) => {
                if matches!(
                    op.kind,
                    UnOpKind::PreInc | UnOpKind::PostInc | UnOpKind::PreDec | UnOpKind::PostDec
                ) {
                    let id = self.gcx.resolved_variable(value)?;
                    let old = self.load_variable(id, value.span)?;
                    let one = self.builder.imm_u256(U256::from(1));
                    let kind = if matches!(op.kind, UnOpKind::PreInc | UnOpKind::PostInc) {
                        BinOpKind::Add
                    } else {
                        BinOpKind::Sub
                    };
                    let new = self.binary(kind, old, one);
                    self.store_variable(id, new, expr.span)?;
                    return Some(if matches!(op.kind, UnOpKind::PreInc | UnOpKind::PreDec) {
                        new
                    } else {
                        old
                    });
                }
                let value = self.lower_expr(value)?;
                self.unary(op.kind, value, expr.span)
            }
            ExprKind::Assign(lhs, op, rhs) => {
                let id = self.gcx.resolved_variable(lhs)?;
                let rhs = self.lower_expr(rhs)?;
                let value = if let Some(kind) = op.map(|op| op.kind) {
                    self.binary_assign(kind, id, rhs, expr.span)?
                } else {
                    rhs
                };
                self.store_variable(id, value, expr.span)?;
                Some(value)
            }
            ExprKind::Ternary(cond, then_expr, else_expr) => {
                let cond = self.lower_expr(cond)?;
                let then_value = self.lower_expr(then_expr)?;
                let else_value = self.lower_expr(else_expr)?;
                Some(self.builder.select(cond, then_value, else_value))
            }
            ExprKind::Tuple([Some(inner)]) => self.lower_expr(inner),
            _ => report_unsupported(self.gcx, expr.span, "expression"),
        }
    }

    fn lower_literal(&mut self, kind: LitKind<'_>, span: Span) -> Option<ValueId> {
        match kind {
            LitKind::Number(value) => Some(self.builder.imm_u256(value)),
            LitKind::Bool(value) => Some(self.builder.imm_bool(value)),
            LitKind::Address(value) => {
                Some(self.builder.imm_u256(U256::from_be_slice(value.as_slice())))
            }
            LitKind::Rational(value) if *value.denom() == U256::from(1) => {
                Some(self.builder.imm_u256(*value.numer()))
            }
            _ => report_unsupported(self.gcx, span, "literal"),
        }
    }

    fn load_variable(&mut self, id: VariableId, span: Span) -> Option<ValueId> {
        if let Some(value) = self.values.get(&id).copied() {
            return Some(value);
        }
        if let Some(location) = self.storage.get(id) {
            return Some(self.storage.load(&mut self.builder, location));
        }
        let var = self.gcx.hir.variable(id);
        if var.is_constant() {
            return self.lower_constant(var.initializer, span);
        }
        report_unsupported(self.gcx, span, "identifier")
    }

    fn store_variable(&mut self, id: VariableId, value: ValueId, span: Span) -> Option<()> {
        if self.values.contains_key(&id) {
            self.values.insert(id, value);
            return Some(());
        }
        if let Some(location) = self.storage.get(id) {
            self.storage.store(&mut self.builder, location, value);
            return Some(());
        }
        report_unsupported(self.gcx, span, "assignment target")
    }

    fn lower_constant(
        &mut self,
        initializer: Option<&hir::Expr<'_>>,
        span: Span,
    ) -> Option<ValueId> {
        let Some(initializer) = initializer else {
            return report_unsupported(self.gcx, span, "constant initializer");
        };
        match self.gcx.try_eval_const_value(initializer).ok()? {
            ConstValue::Bool(value) => Some(self.builder.imm_bool(*value)),
            ConstValue::Integer(value) => Some(self.builder.imm_u256(value.as_u256()?)),
            _ => report_unsupported(self.gcx, span, "constant value"),
        }
    }

    fn binary(&mut self, op: BinOpKind, lhs: ValueId, rhs: ValueId) -> ValueId {
        match op {
            BinOpKind::Add => self.builder.add(lhs, rhs),
            BinOpKind::Sub => self.builder.sub(lhs, rhs),
            BinOpKind::Mul => self.builder.mul(lhs, rhs),
            BinOpKind::Div => self.builder.div(lhs, rhs),
            BinOpKind::Rem => self.builder.mod_(lhs, rhs),
            BinOpKind::Lt => self.builder.lt(lhs, rhs),
            BinOpKind::Gt => self.builder.gt(lhs, rhs),
            BinOpKind::Eq => self.builder.eq(lhs, rhs),
            BinOpKind::Ne => {
                let eq = self.builder.eq(lhs, rhs);
                self.builder.iszero(eq)
            }
            BinOpKind::Le => {
                let gt = self.builder.gt(lhs, rhs);
                self.builder.iszero(gt)
            }
            BinOpKind::Ge => {
                let lt = self.builder.lt(lhs, rhs);
                self.builder.iszero(lt)
            }
            BinOpKind::And | BinOpKind::BitAnd => self.builder.and(lhs, rhs),
            BinOpKind::Or | BinOpKind::BitOr => self.builder.or(lhs, rhs),
            BinOpKind::BitXor => self.builder.xor(lhs, rhs),
            BinOpKind::Shl => self.builder.shl(lhs, rhs),
            BinOpKind::Shr => self.builder.shr(lhs, rhs),
            BinOpKind::Sar => self.builder.sar(lhs, rhs),
            BinOpKind::Pow => self.builder.exp(lhs, rhs),
        }
    }

    fn binary_assign(
        &mut self,
        op: BinOpKind,
        id: VariableId,
        rhs: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        let lhs = self.load_variable(id, span)?;
        Some(self.binary(op, lhs, rhs))
    }

    fn unary(&mut self, op: UnOpKind, value: ValueId, span: Span) -> Option<ValueId> {
        Some(match op {
            UnOpKind::Not => self.builder.iszero(value),
            UnOpKind::Neg => {
                let zero = self.builder.imm_u256(U256::ZERO);
                self.builder.sub(zero, value)
            }
            UnOpKind::BitNot => self.builder.not(value),
            UnOpKind::PreInc | UnOpKind::PostInc | UnOpKind::PreDec | UnOpKind::PostDec => {
                return report_unsupported(self.gcx, span, "increment or decrement");
            }
        })
    }
}

fn merge_values(
    builder: &mut FunctionBuilder<'_>,
    before: FxHashMap<VariableId, ValueId>,
    then_block: BlockId,
    then_values: FxHashMap<VariableId, ValueId>,
    then_terminated: bool,
    else_block: BlockId,
    else_values: FxHashMap<VariableId, ValueId>,
    else_terminated: bool,
) -> FxHashMap<VariableId, ValueId> {
    let mut values = before;
    let mut ids = values.keys().copied().collect::<Vec<_>>();
    ids.extend(then_values.keys().copied());
    ids.extend(else_values.keys().copied());
    ids.sort_unstable();
    ids.dedup();
    for id in ids {
        let then_value = then_values.get(&id).copied();
        let else_value = else_values.get(&id).copied();
        let value = match (then_terminated, else_terminated, then_value, else_value) {
            (true, false, _, value) | (false, true, value, _) => value,
            (_, _, Some(lhs), Some(rhs)) if lhs == rhs => Some(lhs),
            (false, false, Some(lhs), Some(rhs)) => {
                Some(builder.phi(vec![(then_block, lhs), (else_block, rhs)]))
            }
            _ => then_value.or(else_value),
        };
        if let Some(value) = value {
            values.insert(id, value);
        }
    }
    values
}

fn report_unsupported<T>(gcx: Gcx<'_>, span: Span, what: &str) -> Option<T> {
    gcx.dcx().err(format!("codegen rewrite does not support this {what} yet")).span(span).emit();
    None
}
