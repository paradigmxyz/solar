use crate::LintContext;
use solar_ast::{self as ast, visit::Visit};
use solar_interface::data_structures::Never;
use std::ops::ControlFlow;

/// A lint pass that runs directly on the parsed AST.
pub trait EarlyLintPass<'ast>: Send + Sync {
    fn check_expr(&mut self, _ctx: &LintContext<'_, '_>, _expr: &'ast ast::Expr<'ast>) {}
    fn check_item_struct(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _item: &'ast ast::ItemStruct<'ast>,
    ) {
    }
    fn check_item_function(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _item: &'ast ast::ItemFunction<'ast>,
    ) {
    }
    fn check_variable_definition(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _var: &'ast ast::VariableDefinition<'ast>,
    ) {
    }
    fn check_import_directive(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _import: &'ast ast::ImportDirective<'ast>,
    ) {
    }
    fn check_using_directive(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _using: &'ast ast::UsingDirective<'ast>,
    ) {
    }
    fn check_item_contract(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _contract: &'ast ast::ItemContract<'ast>,
    ) {
    }
    fn check_doc_comment(
        &mut self,
        _ctx: &LintContext<'_, '_>,
        _comment: &'ast ast::DocComment<'ast>,
    ) {
    }
    fn check_item(&mut self, _ctx: &LintContext<'_, '_>, _item: &'ast ast::Item<'ast>) {}
    fn check_stmt(&mut self, _ctx: &LintContext<'_, '_>, _stmt: &'ast ast::Stmt<'ast>) {}
    fn check_path(&mut self, _ctx: &LintContext<'_, '_>, _path: &'ast ast::PathSlice) {}
    fn check_ty(&mut self, _ctx: &LintContext<'_, '_>, _ty: &'ast ast::Type<'ast>) {}

    /// Runs after the complete source unit has been visited.
    fn check_full_source_unit(
        &mut self,
        _ctx: &LintContext<'ast, '_>,
        _ast: &'ast ast::SourceUnit<'ast>,
    ) {
    }
}

/// Dispatches an AST traversal to a collection of early lint passes.
pub struct EarlyLintVisitor<'a, 's, 'ast> {
    pub ctx: &'a LintContext<'s, 'a>,
    pub passes: &'a mut [Box<dyn EarlyLintPass<'ast> + 's>],
}

impl<'a, 's, 'ast> EarlyLintVisitor<'a, 's, 'ast>
where
    's: 'ast,
{
    pub fn new(
        ctx: &'a LintContext<'s, 'a>,
        passes: &'a mut [Box<dyn EarlyLintPass<'ast> + 's>],
    ) -> Self {
        Self { ctx, passes }
    }

    /// Runs the post-traversal hook for every pass.
    pub fn post_source_unit(&mut self, ast: &'ast ast::SourceUnit<'ast>) {
        for pass in self.passes.iter_mut() {
            pass.check_full_source_unit(self.ctx, ast);
        }
    }
}

impl<'s, 'ast> Visit<'ast> for EarlyLintVisitor<'_, 's, 'ast>
where
    's: 'ast,
{
    type BreakValue = Never;

    fn visit_doc_comment(
        &mut self,
        comment: &'ast ast::DocComment<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_doc_comment(self.ctx, comment);
        }
        self.walk_doc_comment(comment)
    }

    fn visit_expr(&mut self, expr: &'ast ast::Expr<'ast>) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_expr(self.ctx, expr);
        }
        self.walk_expr(expr)
    }

    fn visit_variable_definition(
        &mut self,
        var: &'ast ast::VariableDefinition<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_variable_definition(self.ctx, var);
        }
        self.walk_variable_definition(var)
    }

    fn visit_item_struct(
        &mut self,
        item: &'ast ast::ItemStruct<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_item_struct(self.ctx, item);
        }
        self.walk_item_struct(item)
    }

    fn visit_item_function(
        &mut self,
        item: &'ast ast::ItemFunction<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_item_function(self.ctx, item);
        }
        self.walk_item_function(item)
    }

    fn visit_import_directive(
        &mut self,
        import: &'ast ast::ImportDirective<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_import_directive(self.ctx, import);
        }
        self.walk_import_directive(import)
    }

    fn visit_using_directive(
        &mut self,
        using: &'ast ast::UsingDirective<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_using_directive(self.ctx, using);
        }
        self.walk_using_directive(using)
    }

    fn visit_item_contract(
        &mut self,
        contract: &'ast ast::ItemContract<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_item_contract(self.ctx, contract);
        }
        self.walk_item_contract(contract)
    }

    fn visit_item(&mut self, item: &'ast ast::Item<'ast>) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_item(self.ctx, item);
        }
        self.walk_item(item)
    }

    fn visit_stmt(&mut self, stmt: &'ast ast::Stmt<'ast>) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_stmt(self.ctx, stmt);
        }
        self.walk_stmt(stmt)
    }

    fn visit_path(&mut self, path: &'ast ast::PathSlice) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_path(self.ctx, path);
        }
        self.walk_path(path)
    }

    fn visit_ty(&mut self, ty: &'ast ast::Type<'ast>) -> ControlFlow<Self::BreakValue> {
        for pass in self.passes.iter_mut() {
            pass.check_ty(self.ctx, ty);
        }
        self.walk_ty(ty)
    }
}
