//! Syntax-based selection-range construction.

use crate::proto;
use crop::Rope;
use lsp_types::{Position, Range, SelectionRange};
use solar_config::CompileOpts;
use solar_interface::{Session, SourceMap, Span, data_structures::Never, source_map::FileName};
use solar_parse::{
    Parser,
    ast::{self, visit::Visit},
};
use std::{
    cmp::Reverse,
    ops::{ControlFlow, Range as ByteRange},
};

pub(crate) fn selection_ranges(
    source: &str,
    positions: &[Position],
) -> Option<Vec<SelectionRange>> {
    let rope = Rope::from(source);
    let cursors = positions
        .iter()
        .map(|&position| {
            proto::checked_text_range(&rope, Range::new(position, position))
                .map(|range| range.start)
        })
        .collect::<Option<Vec<_>>>()?;
    if cursors.is_empty() {
        return Some(Vec::new());
    }

    let candidates = collect_ranges(source);
    cursors
        .into_iter()
        .map(|cursor| selection_range_for_cursor(&rope, &candidates, cursor))
        .collect()
}

fn collect_ranges(source: &str) -> Vec<ByteRange<usize>> {
    let mut opts = CompileOpts::default();
    opts.unstable.recover_incomplete_input = true;
    let sess = Session::builder().opts(opts).with_silent_emitter(None).single_threaded().build();

    sess.enter_sequential(|| {
        let arena = ast::Arena::new();
        let Ok(mut parser) = Parser::from_source_code(
            &sess,
            &arena,
            FileName::Custom("lsp-selection-range.sol".into()),
            source,
        ) else {
            return Vec::new();
        };
        let source_unit = match parser.parse_file() {
            Ok(source_unit) => source_unit,
            Err(error) => {
                error.emit();
                return Vec::new();
            }
        };
        drop(parser);

        let mut collector = RangeCollector::new(sess.source_map(), source);
        let _ = collector.visit_source_unit(&source_unit);
        collector.ranges
    })
}

fn selection_range_for_cursor(
    rope: &Rope,
    candidates: &[ByteRange<usize>],
    cursor: usize,
) -> Option<SelectionRange> {
    let mut candidates = candidates
        .iter()
        .filter(|range| range.start <= cursor && cursor < range.end)
        .cloned()
        .collect::<Vec<_>>();
    candidates
        .sort_unstable_by_key(|range| (range.end - range.start, Reverse(range.start), range.end));
    candidates.dedup();

    let document = 0..rope.byte_len();
    let mut chain = Vec::new();
    if let Some(mut current) = candidates.first().cloned() {
        chain.push(current.clone());
        while let Some(parent) =
            candidates.iter().find(|candidate| strictly_contains(candidate, &current)).cloned()
        {
            chain.push(parent.clone());
            current = parent;
        }
    } else {
        chain.push(cursor..cursor);
    }
    if chain.last() != Some(&document) {
        chain.push(document);
    }

    let mut parent = None;
    for range in chain.into_iter().rev() {
        let range = Range::new(
            proto::position_at_byte(rope, range.start)?,
            proto::position_at_byte(rope, range.end)?,
        );
        parent = Some(Box::new(SelectionRange { range, parent }));
    }
    parent.map(|range| *range)
}

fn strictly_contains(outer: &ByteRange<usize>, inner: &ByteRange<usize>) -> bool {
    outer != inner && outer.start <= inner.start && inner.end <= outer.end
}

struct RangeCollector<'a> {
    source_map: &'a SourceMap,
    source: &'a str,
    ranges: Vec<ByteRange<usize>>,
}

impl<'a> RangeCollector<'a> {
    fn new(source_map: &'a SourceMap, source: &'a str) -> Self {
        Self { source_map, source, ranges: Vec::new() }
    }

    fn push(&mut self, span: Span) {
        if span.is_dummy() {
            return;
        }
        let Ok(range) = self.source_map.span_to_range(span) else { return };
        if !range.is_empty()
            && range.end <= self.source.len()
            && self.source.is_char_boundary(range.start)
            && self.source.is_char_boundary(range.end)
        {
            self.ranges.push(range);
        }
    }
}

impl<'ast> Visit<'ast> for RangeCollector<'_> {
    type BreakValue = Never;

    fn visit_item(&mut self, item: &'ast ast::Item<'ast>) -> ControlFlow<Self::BreakValue> {
        self.push(item.span);
        self.walk_item(item)
    }

    fn visit_variable_definition(
        &mut self,
        variable: &'ast ast::VariableDefinition<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.push(variable.span);
        self.walk_variable_definition(variable)
    }

    fn visit_ty(&mut self, ty: &'ast ast::Type<'ast>) -> ControlFlow<Self::BreakValue> {
        self.push(ty.span);
        self.walk_ty(ty)
    }

    fn visit_call_args(
        &mut self,
        arguments: &'ast ast::CallArgs<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.push(arguments.span);
        self.walk_call_args(arguments)
    }

    fn visit_stmt(&mut self, statement: &'ast ast::Stmt<'ast>) -> ControlFlow<Self::BreakValue> {
        self.push(statement.span);
        self.walk_stmt(statement)
    }

    fn visit_try_catch_clause(
        &mut self,
        clause: &'ast ast::TryCatchClause<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.push(clause.span);
        self.walk_try_catch_clause(clause)
    }

    fn visit_block(&mut self, block: &'ast ast::Block<'ast>) -> ControlFlow<Self::BreakValue> {
        self.push(block.span);
        self.walk_block(block)
    }

    fn visit_expr(&mut self, expression: &'ast ast::Expr<'ast>) -> ControlFlow<Self::BreakValue> {
        self.push(expression.span);
        self.walk_expr(expression)
    }

    fn visit_parameter_list(
        &mut self,
        parameters: &'ast ast::ParameterList<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.push(parameters.span);
        self.walk_parameter_list(parameters)
    }

    fn visit_lit(&mut self, literal: &'ast ast::Lit<'_>) -> ControlFlow<Self::BreakValue> {
        self.push(literal.span);
        self.walk_lit(literal)
    }

    fn visit_yul_stmt(
        &mut self,
        statement: &'ast ast::yul::Stmt<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.push(statement.span);
        self.walk_yul_stmt(statement)
    }

    fn visit_yul_block(
        &mut self,
        block: &'ast ast::yul::Block<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.push(block.span);
        self.walk_yul_block(block)
    }

    fn visit_yul_stmt_case(
        &mut self,
        case: &'ast ast::yul::StmtSwitchCase<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.push(case.span);
        self.walk_yul_stmt_case(case)
    }

    fn visit_yul_expr(
        &mut self,
        expression: &'ast ast::yul::Expr<'ast>,
    ) -> ControlFlow<Self::BreakValue> {
        self.push(expression.span);
        self.walk_yul_expr(expression)
    }

    fn visit_path(&mut self, path: &'ast ast::PathSlice) -> ControlFlow<Self::BreakValue> {
        self.push(path.span());
        self.walk_path(path)
    }

    fn visit_ident(
        &mut self,
        identifier: &'ast solar_interface::Ident,
    ) -> ControlFlow<Self::BreakValue> {
        self.push(identifier.span);
        self.walk_ident(identifier)
    }
}
