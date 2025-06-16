//! Private span visitor for debugging.

use solar_ast::visit::Visit;
use solar_interface::{Session, Span};
use std::ops::ControlFlow;

/// A visitor that emits a diagnostic for each span it encounters.
pub(crate) struct SpanVisitor<'sess> {
    sess: &'sess Session,
    count: usize,
}

impl<'sess> SpanVisitor<'sess> {
    /// Creates a new span visitor.
    pub(crate) fn new(sess: &'sess Session) -> Self {
        Self { sess, count: 0 }
    }

    /// Returns the number of spans visited.
    pub(crate) fn count(&self) -> usize {
        self.count
    }
}

impl<'ast, 'sess> Visit<'ast> for SpanVisitor<'sess> {
    type BreakValue = ();

    fn visit_span(&mut self, span: &'ast Span) -> ControlFlow<Self::BreakValue> {
        self.count += 1;
        self.sess.dcx.note(format!("span #{}", self.count)).span(*span).emit();
        ControlFlow::Continue(())
    }
}
