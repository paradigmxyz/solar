use solar_ast::{visit::Visit, Stmt};
use solar_data_structures::Never;
use solar_interface::diagnostics::DiagCtxt;
use std::ops::ControlFlow;

struct VariableDeclarationAsLoopBodyChecker<'ast, 'sess> {
    visited_block: bool,
    #[allow(dead_code)]
    body: &'ast &'ast mut Stmt<'ast>,
    dcx: &'sess DiagCtxt,
}

impl<'ast, 'sess> VariableDeclarationAsLoopBodyChecker<'ast, 'sess> {
    pub(crate) fn new(body: &'ast &'ast mut Stmt<'ast>, dcx: &'sess DiagCtxt) -> Self {
        Self { visited_block: false, body, dcx }
    }

    /// Returns the diagnostics context.
    #[inline]
    fn dcx(&self) -> &'sess DiagCtxt {
        self.dcx
    }
}

impl<'ast, 'sess> Visit<'ast> for VariableDeclarationAsLoopBodyChecker<'ast, 'sess> {
    type BreakValue = Never;

    fn visit_block(
        &mut self,
        block: &'ast solar_ast::Block<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.visited_block = true;
        self.walk_block(block)
    }

    fn visit_variable_definition(
        &mut self,
        var: &'ast solar_ast::VariableDefinition<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        if !self.visited_block {
            self.dcx()
                .err("variable declarations are not allowed as the body of a loop")
                .span(var.span)
                .help("wrap the statement in a block (`{ ... }`)")
                .emit();
        }
        self.walk_variable_definition(var)
    }

    fn visit_stmt(&mut self, stmt: &'ast solar_ast::Stmt<'ast>) -> ControlFlow<Self::BreakValue> {
        match &stmt.kind {
            solar_ast::StmtKind::While(..)
            | solar_ast::StmtKind::DoWhile(..)
            | solar_ast::StmtKind::For { .. } => {
                return ControlFlow::Continue(());
            }
            _ => {}
        };
        self.walk_stmt(stmt)
    }
}

pub(crate) fn check_if_loop_body_is_a_variable_declaration<'ast, 'sess>(
    body: &'ast &'ast mut Stmt<'ast>,
    dcx: &'sess DiagCtxt,
) {
    // Check and emit
    let mut checker = VariableDeclarationAsLoopBodyChecker::new(body, dcx);
    checker.visit_stmt(body);
}
