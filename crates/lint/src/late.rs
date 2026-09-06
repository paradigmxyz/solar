use crate::LintContext;
use solar_interface::data_structures::Never;
use solar_sema::{Gcx, hir};
use std::ops::ControlFlow;

/// A lint pass that runs on Solar's analyzed HIR.
///
/// Every hook receives the global context; the HIR is available as `gcx.hir`.
pub trait LateLintPass<'gcx>: Send + Sync {
    fn check_nested_source(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _gcx: Gcx<'gcx>,
        _id: hir::SourceId,
    ) {
    }
    fn check_nested_item(&mut self, _ctx: &LintContext<'_, '_>, _gcx: Gcx<'gcx>, _id: hir::ItemId) {
    }
    fn check_nested_contract(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _gcx: Gcx<'gcx>,
        _id: hir::ContractId,
    ) {
    }
    fn check_nested_function(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _gcx: Gcx<'gcx>,
        _id: hir::FunctionId,
    ) {
    }
    fn check_nested_var(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _gcx: Gcx<'gcx>,
        _id: hir::VariableId,
    ) {
    }
    fn check_item(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _gcx: Gcx<'gcx>,
        _item: hir::Item<'gcx, 'gcx>,
    ) {
    }
    fn check_contract(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _gcx: Gcx<'gcx>,
        _contract: &'gcx hir::Contract<'gcx>,
    ) {
    }
    fn check_function(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _gcx: Gcx<'gcx>,
        _function: &'gcx hir::Function<'gcx>,
    ) {
    }
    fn check_modifier(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _gcx: Gcx<'gcx>,
        _modifier: &'gcx hir::Modifier<'gcx>,
    ) {
    }
    fn check_var(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _gcx: Gcx<'gcx>,
        _var: &'gcx hir::Variable<'gcx>,
    ) {
    }
    fn check_expr(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _gcx: Gcx<'gcx>,
        _expr: &'gcx hir::Expr<'gcx>,
    ) {
    }
    fn check_call_args(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _gcx: Gcx<'gcx>,
        _args: &'gcx hir::CallArgs<'gcx>,
    ) {
    }
    fn check_stmt(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _gcx: Gcx<'gcx>,
        _stmt: &'gcx hir::Stmt<'gcx>,
    ) {
    }
    fn check_ty(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _gcx: Gcx<'gcx>,
        _ty: &'gcx hir::Type<'gcx>,
    ) {
    }
}

/// Dispatches a HIR traversal to a collection of late lint passes.
pub struct LateLintVisitor<'a, 's, 'gcx> {
    ctx: &'a LintContext<'s, 'a>,
    passes: &'a mut [Box<dyn LateLintPass<'gcx> + 's>],
    gcx: Gcx<'gcx>,
}

impl<'a, 's, 'gcx> LateLintVisitor<'a, 's, 'gcx>
where
    's: 'gcx,
{
    pub fn new(
        ctx: &'a LintContext<'s, 'a>,
        passes: &'a mut [Box<dyn LateLintPass<'gcx> + 's>],
        gcx: Gcx<'gcx>,
    ) -> Self {
        Self { ctx, passes, gcx }
    }
}

impl<'s, 'gcx> hir::Visit<'gcx> for LateLintVisitor<'_, 's, 'gcx>
where
    's: 'gcx,
{
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_nested_source(&mut self, id: hir::SourceId) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_nested_source(self.ctx, self.gcx, id);
        }
        self.walk_nested_source(id)
    }

    fn visit_nested_item(&mut self, id: hir::ItemId) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_nested_item(self.ctx, self.gcx, id);
        }
        self.walk_nested_item(id)
    }

    fn visit_nested_contract(&mut self, id: hir::ContractId) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_nested_contract(self.ctx, self.gcx, id);
        }
        self.walk_nested_contract(id)
    }

    fn visit_nested_function(&mut self, id: hir::FunctionId) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_nested_function(self.ctx, self.gcx, id);
        }
        self.walk_nested_function(id)
    }

    fn visit_nested_var(&mut self, id: hir::VariableId) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_nested_var(self.ctx, self.gcx, id);
        }
        self.walk_nested_var(id)
    }

    fn visit_contract(
        &mut self,
        contract: &'gcx hir::Contract<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_contract(self.ctx, self.gcx, contract);
        }
        self.walk_contract(contract)
    }

    fn visit_function(
        &mut self,
        function: &'gcx hir::Function<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_function(self.ctx, self.gcx, function);
        }
        self.walk_function(function)
    }

    fn visit_modifier(
        &mut self,
        modifier: &'gcx hir::Modifier<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_modifier(self.ctx, self.gcx, modifier);
        }
        self.walk_modifier(modifier)
    }

    fn visit_item(&mut self, item: hir::Item<'gcx, 'gcx>) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_item(self.ctx, self.gcx, item);
        }
        self.walk_item(item)
    }

    fn visit_var(&mut self, var: &'gcx hir::Variable<'gcx>) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_var(self.ctx, self.gcx, var);
        }
        self.walk_var(var)
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_expr(self.ctx, self.gcx, expr);
        }
        self.walk_expr(expr)
    }

    fn visit_call_args(
        &mut self,
        args: &'gcx hir::CallArgs<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_call_args(self.ctx, self.gcx, args);
        }
        self.walk_call_args(args)
    }

    fn visit_stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_stmt(self.ctx, self.gcx, stmt);
        }
        self.walk_stmt(stmt)
    }

    fn visit_ty(&mut self, ty: &'gcx hir::Type<'gcx>) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_ty(self.ctx, self.gcx, ty);
        }
        self.walk_ty(ty)
    }
}
