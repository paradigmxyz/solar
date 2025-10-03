use super::*;
use std::ops::ControlFlow;

solar_macros::declare_visitors! {
/// HIR traversal.
pub trait Visit<'hir> {
    /// The value returned when breaking from the traversal.
    ///
    /// This can be [`Never`](solar_data_structures::Never) to indicate that the traversal
    /// should never break.
    type BreakValue;

    /// Returns the HIR map.
    fn hir(&self) -> &'hir Hir<'hir>;

    fn visit_nested_source(&mut self, id: SourceId) -> ControlFlow<Self::BreakValue>{
        visit_nested_items(self, self.hir().source(id).items)
    }

    fn visit_nested_item(&mut self, id: ItemId) -> ControlFlow<Self::BreakValue> {
        match id {
            ItemId::Contract(id) => self.visit_nested_contract(id),
            ItemId::Function(id) => self.visit_nested_function(id),
            ItemId::Struct(id) => self.visit_nested_struct(id),
            ItemId::Enum(id) => self.visit_nested_enum(id),
            ItemId::Udvt(_id) => ControlFlow::Continue(()), // TODO
            ItemId::Error(_id) => ControlFlow::Continue(()), // TODO
            ItemId::Event(_id) => ControlFlow::Continue(()), // TODO
            ItemId::Variable(id) => self.visit_nested_var(id),
        }
    }

    fn visit_item(&mut self, item: Item<'hir, 'hir>) -> ControlFlow<Self::BreakValue> {
        match item {
            Item::Contract(item) => self.visit_contract(item),
            Item::Function(item) => self.visit_function(item),
            Item::Struct(item) => self.visit_struct(item),
            Item::Enum(item) => self.visit_enum(item),
            Item::Udvt(_item) => ControlFlow::Continue(()), // TODO
            Item::Error(_item) => ControlFlow::Continue(()), // TODO
            Item::Event(_item) => ControlFlow::Continue(()), // TODO
            Item::Variable(item) => self.visit_var(item),
        }
    }

    fn visit_nested_contract(&mut self, id: ContractId) -> ControlFlow<Self::BreakValue> {
        self.visit_contract(self.hir().contract(id))
    }

    fn visit_contract(&mut self, contract: &'hir Contract<'hir>) -> ControlFlow<Self::BreakValue> {
        for base in contract.bases_args {
            self.visit_modifier(base)?;
        }
        visit_nested_items(self, contract.items)
    }

    fn visit_nested_function(&mut self, id: FunctionId) -> ControlFlow<Self::BreakValue> {
        self.visit_function(self.hir().function(id))
    }

    fn visit_function(&mut self, func: &'hir Function<'hir>) -> ControlFlow<Self::BreakValue> {
        let Function { source: _, contract: _, span: _, name: _, kind: _, visibility: _, state_mutability: _, modifiers, marked_virtual: _, virtual_: _, override_: _, overrides: _, parameters, returns, body, body_span: _, gettee: _ } = func;
        for &param in parameters.iter() {
            self.visit_nested_var(param)?;
        }
        for modifier in modifiers.iter() {
            self.visit_modifier(modifier)?;
        }
        for &ret in returns.iter() {
            self.visit_nested_var(ret)?;
        }
        if let Some(body) = body {
            for stmt in body.iter() {
                self.visit_stmt(stmt)?;
            }
        }
        ControlFlow::Continue(())
    }

    fn visit_modifier(&mut self, modifier: &'hir Modifier<'hir>) -> ControlFlow<Self::BreakValue> {
        let Modifier { span: _, id: _, args } = modifier;
        self.visit_call_args(args)
    }

    fn visit_nested_struct(&mut self, id: StructId) -> ControlFlow<Self::BreakValue> {
        self.visit_struct(self.hir().strukt(id))
    }

    fn visit_struct(&mut self, strukt: &'hir Struct<'hir>) -> ControlFlow<Self::BreakValue> {
        for &field in strukt.fields {
            self.visit_nested_var(field)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_nested_enum(&mut self, id: EnumId) -> ControlFlow<Self::BreakValue> {
        self.visit_enum(self.hir().enumm(id))
    }

    fn visit_enum(&mut self, _enumm: &'hir Enum<'hir>) -> ControlFlow<Self::BreakValue> {
        ControlFlow::Continue(())
    }

    fn visit_nested_var(&mut self, id: VariableId) -> ControlFlow<Self::BreakValue> {
        self.visit_var(self.hir().variable(id))
    }

    fn visit_var(&mut self, var: &'hir Variable<'hir>) -> ControlFlow<Self::BreakValue> {
        self.visit_ty(&var.ty)?;
        if let Some(expr) = var.initializer {
            self.visit_expr(expr)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_expr(&mut self, expr: &'hir Expr<'hir>) -> ControlFlow<Self::BreakValue> {
        match expr.kind {
            ExprKind::Call(expr, ref args, opts) => {
                self.visit_expr(expr)?;
                if let Some(opts) = opts {
                    for opt in opts {
                        self.visit_expr(&opt.value)?;
                    }
                }
                self.visit_call_args(args)?;
            }
            ExprKind::Delete(expr)
            | ExprKind::Member(expr, _)
            | ExprKind::Payable(expr)
            | ExprKind::Unary(_, expr) => self.visit_expr(expr)?,
            ExprKind::Assign(lhs, _, rhs) | ExprKind::Binary(lhs, _, rhs) => {
                self.visit_expr(lhs)?;
                self.visit_expr(rhs)?;
            }
            ExprKind::Index(expr, index) => {
                self.visit_expr(expr)?;
                if let Some(index) = index {
                    self.visit_expr(index)?;
                }
            }
            ExprKind::Slice(expr, start, end) => {
                self.visit_expr(expr)?;
                if let Some(start) = start {
                    self.visit_expr(start)?;
                }
                if let Some(end) = end {
                    self.visit_expr(end)?;
                }
            }
            ExprKind::Ternary(cond, true_, false_) => {
                self.visit_expr(cond)?;
                self.visit_expr(true_)?;
                self.visit_expr(false_)?;
            }
            ExprKind::Array(exprs) => {
                for expr in exprs {
                    self.visit_expr(expr)?;
                }
            }
            ExprKind::Tuple(exprs) => {
                exprs.iter().copied().flatten().try_for_each(|expr| self.visit_expr(expr))?;
            }
            ExprKind::Ident(_) => {}
            ExprKind::Lit(_) => {}
            ExprKind::New(ref ty) | ExprKind::TypeCall(ref ty) | ExprKind::Type(ref ty) => {
                self.visit_ty(ty)?;
            }
            ExprKind::Err(_guar) => {}
        }
        ControlFlow::Continue(())
    }

    fn visit_call_args(&mut self, args: &'hir CallArgs<'hir>) -> ControlFlow<Self::BreakValue> {
        let CallArgs { span: _, kind } = args;
        for expr in kind.exprs() {
            self.visit_expr(expr)?;
        }
        ControlFlow::Continue(())
    }

    fn visit_stmt(&mut self, stmt: &'hir Stmt<'hir>) -> ControlFlow<Self::BreakValue> {
        match stmt.kind {
            StmtKind::DeclSingle(var) => self.visit_nested_var(var)?,
            StmtKind::DeclMulti(vars, expr) => {
                for &var in vars {
                    if let Some(var) = var {
                        self.visit_nested_var(var)?;
                    }
                }
                self.visit_expr(expr)?;
            }
            StmtKind::Block(block) | StmtKind::UncheckedBlock(block) | StmtKind::Loop(block, _) => {
                for stmt in block.stmts {
                    self.visit_stmt(stmt)?;
                }
            }
            StmtKind::Emit(expr) => self.visit_expr(expr)?,
            StmtKind::Revert(expr) => self.visit_expr(expr)?,
            StmtKind::Return(expr) => {
                if let Some(expr) = expr {
                    self.visit_expr(expr)?;
                }
            }
            StmtKind::Break => {}
            StmtKind::Continue => {}
            StmtKind::If(cond, true_, false_) => {
                self.visit_expr(cond)?;
                self.visit_stmt(true_)?;
                if let Some(false_) = false_ {
                    self.visit_stmt(false_)?;
                }
            }
            StmtKind::Try(try_) => {
                self.visit_expr(&try_.expr)?;
                for clause in try_.clauses {
                    for &var in clause.args {
                        self.visit_nested_var(var)?;
                    }
                    for stmt in clause.block.iter() {
                        self.visit_stmt(stmt)?;
                    }
                }
            }
            StmtKind::Expr(expr) => self.visit_expr(expr)?,
            StmtKind::Placeholder => {}
            StmtKind::Err(_guar) => {}
        }
        ControlFlow::Continue(())
    }

    fn visit_ty(&mut self, ty: &'hir Type<'hir>) -> ControlFlow<Self::BreakValue> {
        match ty.kind {
            TypeKind::Elementary(_) => {}
            TypeKind::Array(arr) => {
                self.visit_ty(&arr.element)?;
                if let Some(len) = arr.size {
                    self.visit_expr(len)?;
                }
            }
            TypeKind::Function(func) => {
                for &param in func.parameters {
                    self.visit_nested_var(param)?;
                }
                for &ret in func.returns {
                    self.visit_nested_var(ret)?;
                }
            }
            TypeKind::Mapping(map) => {
                self.visit_ty(&map.key)?;
                self.visit_ty(&map.value)?;
            }
            TypeKind::Custom(_) => {}
            TypeKind::Err(_guar) => {}
        }
        ControlFlow::Continue(())
    }
}
}

fn visit_nested_items<'hir, V: Visit<'hir> + ?Sized>(
    v: &mut V,
    ids: &[ItemId],
) -> ControlFlow<V::BreakValue> {
    ids.iter().try_for_each(|&id| v.visit_nested_item(id))
}
