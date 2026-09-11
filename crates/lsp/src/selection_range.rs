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
    borrow::Borrow,
    cmp::Reverse,
    ops::{ControlFlow, Range as ByteRange},
    sync::{Arc, OnceLock},
};

#[cfg(test)]
mod tests;

pub(crate) fn selection_ranges(
    source: String,
    positions: &[Position],
) -> Option<Vec<SelectionRange>> {
    let rope = Rope::from(source.as_str());
    let index = proto::LspPositionIndex::new(&rope);
    let cursors = checked_cursors(&index, positions)?;
    if cursors.is_empty() {
        return Some(Vec::new());
    }
    let candidates = collect_ranges(SourceCode::Owned(source), &rope);
    selection_ranges_for_cursors(&index, &candidates, cursors)
}

pub(crate) struct SelectionRangeIndex {
    source: Arc<String>,
    positions: proto::LspPositionIndex<Rope>,
    candidates: OnceLock<CandidateRanges>,
}

impl SelectionRangeIndex {
    pub(crate) fn new(source: Arc<String>, rope: Rope) -> Self {
        let positions = proto::LspPositionIndex::from_rope(rope);
        Self { source, positions, candidates: OnceLock::new() }
    }

    pub(crate) fn selection_ranges(&self, positions: &[Position]) -> Option<Vec<SelectionRange>> {
        let index = &self.positions;
        let cursors = checked_cursors(index, positions)?;
        if cursors.is_empty() {
            return Some(Vec::new());
        }

        let candidates = self.candidates.get_or_init(|| {
            CandidateRanges::new(collect_ranges(
                SourceCode::Shared(self.source.clone()),
                index.rope(),
            ))
        });
        cursors
            .into_iter()
            .map(|cursor| selection_range_for_cursor(index, candidates.at(cursor), cursor))
            .collect()
    }
}

/// Syntax ranges grouped by traversal order, with a bounding interval for each block.
///
/// AST traversal keeps most neighboring ranges close in the source, so point queries can
/// skip unrelated blocks. Blocks are indexed by start offset and a prefix maximum end allows a
/// point query to stop once all earlier blocks end before the cursor; individual ranges are still
/// checked for exact containment.
struct CandidateRanges {
    ranges: Vec<ByteRange<usize>>,
    bounds: Vec<ByteRange<usize>>,
    /// Blocks ordered by start offset for logarithmic point-query narrowing.
    block_order: Vec<usize>,
    /// Maximum end offset of each prefix in `block_order`, used to stop scanning old blocks.
    prefix_max_end: Vec<usize>,
}

impl CandidateRanges {
    const BLOCK_SIZE: usize = 64;

    fn new(ranges: Vec<ByteRange<usize>>) -> Self {
        let bounds: Vec<ByteRange<usize>> = ranges
            .chunks(Self::BLOCK_SIZE)
            .map(|block| {
                block.iter().fold(block[0].clone(), |bounds, range| {
                    bounds.start.min(range.start)..bounds.end.max(range.end)
                })
            })
            .collect();
        let mut block_order = (0..bounds.len()).collect::<Vec<_>>();
        block_order.sort_unstable_by_key(|&index| bounds[index].start);
        let mut max_end = 0;
        let prefix_max_end = block_order
            .iter()
            .map(|&index| {
                max_end = max_end.max(bounds[index].end);
                max_end
            })
            .collect();
        Self { ranges, bounds, block_order, prefix_max_end }
    }

    fn at(&self, cursor: usize) -> Vec<ByteRange<usize>> {
        let block_count =
            self.block_order.partition_point(|&index| self.bounds[index].start <= cursor);
        let mut candidates = Vec::new();
        for order_index in (0..block_count).rev() {
            // All blocks in this prefix end before the cursor, so older blocks cannot match.
            if self.prefix_max_end[order_index] <= cursor {
                break;
            }
            let block_index = self.block_order[order_index];
            let bounds = &self.bounds[block_index];
            if bounds.contains(&cursor) {
                let block = &self.ranges[block_index * Self::BLOCK_SIZE
                    ..((block_index + 1) * Self::BLOCK_SIZE).min(self.ranges.len())];
                candidates.extend(block.iter().filter(|range| range.contains(&cursor)).cloned());
            }
        }
        candidates
    }
}

enum SourceCode {
    Owned(String),
    Shared(Arc<String>),
}

fn collect_ranges(source: SourceCode, rope: &Rope) -> Vec<ByteRange<usize>> {
    let mut opts = CompileOpts::default();
    opts.unstable.recover_incomplete_input = true;
    let sess = Session::builder().opts(opts).with_silent_emitter(None).single_threaded().build();

    sess.enter_sequential(|| {
        let arena = ast::Arena::new();
        let filename = FileName::Custom("lsp-selection-range.sol".into());
        let source_file = match source {
            SourceCode::Owned(source) => sess.source_map().new_source_file(filename, source),
            SourceCode::Shared(source) => {
                sess.source_map().new_source_file_shared(filename, source)
            }
        };
        let Ok(source_file) = source_file else {
            return Vec::new();
        };
        let mut parser = Parser::from_source_file(&sess, &arena, &source_file);
        let source_unit = match parser.parse_file() {
            Ok(source_unit) => source_unit,
            Err(error) => {
                error.emit();
                return Vec::new();
            }
        };
        drop(parser);

        let mut collector = RangeCollector::new(sess.source_map(), rope);
        let _ = collector.visit_source_unit(&source_unit);
        collector.ranges
    })
}

fn checked_cursors<R: Borrow<Rope>>(
    index: &proto::LspPositionIndex<R>,
    positions: &[Position],
) -> Option<Vec<usize>> {
    positions
        .iter()
        .map(|&position| {
            index.checked_text_range(Range::new(position, position)).map(|range| range.start)
        })
        .collect()
}

fn selection_ranges_for_cursors<R: Borrow<Rope>>(
    index: &proto::LspPositionIndex<R>,
    candidates: &[ByteRange<usize>],
    cursors: Vec<usize>,
) -> Option<Vec<SelectionRange>> {
    cursors
        .into_iter()
        .map(|cursor| {
            let candidates =
                candidates.iter().filter(|range| range.contains(&cursor)).cloned().collect();
            selection_range_for_cursor(index, candidates, cursor)
        })
        .collect()
}

fn selection_range_for_cursor<R: Borrow<Rope>>(
    index: &proto::LspPositionIndex<R>,
    mut candidates: Vec<ByteRange<usize>>,
    cursor: usize,
) -> Option<SelectionRange> {
    candidates
        .sort_unstable_by_key(|range| (range.end - range.start, Reverse(range.start), range.end));
    candidates.dedup();

    let document = 0..index.byte_len();
    let mut chain = Vec::with_capacity(candidates.len() + 1);
    let mut candidates = candidates.into_iter();
    if let Some(current) = candidates.next() {
        chain.push(current);
        for candidate in candidates {
            if strictly_contains(&candidate, chain.last().unwrap()) {
                chain.push(candidate);
            }
        }
    } else {
        chain.push(cursor..cursor);
    }
    if chain.last() != Some(&document) {
        chain.push(document);
    }

    let mut chain = chain.into_iter().rev();
    let outer = chain.next()?;
    let mut selection = SelectionRange {
        range: Range::new(index.position_at_byte(outer.start)?, index.position_at_byte(outer.end)?),
        parent: None,
    };
    for range in chain {
        selection = SelectionRange {
            range: Range::new(
                index.position_at_byte(range.start)?,
                index.position_at_byte(range.end)?,
            ),
            parent: Some(Box::new(selection)),
        };
    }
    Some(selection)
}

fn strictly_contains(outer: &ByteRange<usize>, inner: &ByteRange<usize>) -> bool {
    outer != inner && outer.start <= inner.start && inner.end <= outer.end
}

struct RangeCollector<'a> {
    source_map: &'a SourceMap,
    rope: &'a Rope,
    ranges: Vec<ByteRange<usize>>,
}

impl<'a> RangeCollector<'a> {
    fn new(source_map: &'a SourceMap, rope: &'a Rope) -> Self {
        Self { source_map, rope, ranges: Vec::new() }
    }

    fn push(&mut self, span: Span) {
        if span.is_dummy() {
            return;
        }
        let Ok(range) = self.source_map.span_to_range(span) else { return };
        if !range.is_empty()
            && range.end <= self.rope.byte_len()
            && self.rope.is_char_boundary(range.start)
            && self.rope.is_char_boundary(range.end)
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
