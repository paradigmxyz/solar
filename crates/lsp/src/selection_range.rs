//! Syntax-based selection-range construction.

use crate::{
    proto,
    utils::{checked_span_range, parse_recovering},
};
use crop::Rope;
use lsp_types::{Position, Range, SelectionRange};
use solar_interface::{Span, data_structures::Never, source_map::SourceFile};
use solar_parse::ast::{self, visit::Visit};
use std::{
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
    SelectionRangeIndex::new(Arc::new(source), rope).selection_ranges(positions)
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

        let candidates =
            self.candidates.get_or_init(|| CandidateRanges::new(collect_ranges(&self.source)));
        cursors
            .into_iter()
            .map(|cursor| selection_range_for_cursor(index, candidates.at(cursor), cursor))
            .collect()
    }
}

/// Syntax ranges grouped by traversal order, with a bounding interval for each block.
///
/// AST traversal keeps most neighboring ranges close in the source, so point queries can
/// skip unrelated blocks. An implicit balanced interval tree orders blocks by start offset and
/// tracks each subtree's maximum end; individual ranges are still checked for exact containment.
struct CandidateRanges {
    ranges: Vec<ByteRange<usize>>,
    bounds: Vec<ByteRange<usize>>,
    /// Blocks ordered by start offset for logarithmic point-query narrowing.
    block_order: Vec<usize>,
    /// Maximum end offset in each implicit subtree of `block_order`.
    ///
    /// A prefix maximum cannot prune when an early, broad AST range (such as a contract) spans
    /// the whole file. Subtree maxima let queries skip disjoint branches while retaining those
    /// broad ranges in their original blocks.
    subtree_max_end: Vec<usize>,
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
        let subtree_max_end = vec![0; bounds.len()];
        let mut index = Self { ranges, bounds, block_order, subtree_max_end };
        if !index.block_order.is_empty() {
            index.build_subtree_max_end(0, index.block_order.len());
        }
        index
    }

    fn at(&self, cursor: usize) -> Vec<ByteRange<usize>> {
        let mut candidates = Vec::new();
        self.collect_candidates(0, self.block_order.len(), cursor, &mut candidates);
        candidates
    }

    fn build_subtree_max_end(&mut self, start: usize, end: usize) -> usize {
        let mid = start + (end - start) / 2;
        let block_index = self.block_order[mid];
        let mut max_end = self.bounds[block_index].end;
        if start < mid {
            max_end = max_end.max(self.build_subtree_max_end(start, mid));
        }
        if mid + 1 < end {
            max_end = max_end.max(self.build_subtree_max_end(mid + 1, end));
        }
        self.subtree_max_end[mid] = max_end;
        max_end
    }

    fn collect_candidates(
        &self,
        start: usize,
        end: usize,
        cursor: usize,
        candidates: &mut Vec<ByteRange<usize>>,
    ) {
        if start >= end {
            return;
        }
        let mid = start + (end - start) / 2;
        if self.subtree_max_end[mid] <= cursor {
            return;
        }
        let block_index = self.block_order[mid];
        let bounds = &self.bounds[block_index];
        if bounds.start <= cursor && bounds.end > cursor {
            let block = &self.ranges[block_index * Self::BLOCK_SIZE
                ..((block_index + 1) * Self::BLOCK_SIZE).min(self.ranges.len())];
            candidates.extend(block.iter().filter(|range| range.contains(&cursor)).cloned());
        }
        // Blocks are sorted by start; once a node starts after the cursor, its right subtree
        // cannot contain a match, while a left subtree may still contain earlier broad ranges.
        if start < mid {
            self.collect_candidates(start, mid, cursor, candidates);
        }
        if bounds.start <= cursor && mid + 1 < end {
            self.collect_candidates(mid + 1, end, cursor, candidates);
        }
    }
}

fn collect_ranges(source: &Arc<String>) -> Vec<ByteRange<usize>> {
    parse_recovering("lsp-selection-range.sol", source.clone(), |_, file, source_unit| {
        let mut collector = RangeCollector { file, ranges: Vec::new() };
        if let Some(source_unit) = source_unit {
            let _ = collector.visit_source_unit(source_unit);
        }
        collector.ranges
    })
    .unwrap_or_default()
}

fn checked_cursors(
    index: &proto::LspPositionIndex<Rope>,
    positions: &[Position],
) -> Option<Vec<usize>> {
    positions
        .iter()
        .map(|&position| {
            index.checked_text_range(Range::new(position, position)).map(|range| range.start)
        })
        .collect()
}

fn selection_range_for_cursor(
    index: &proto::LspPositionIndex<Rope>,
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

    let mut selection = None;
    for range in chain.into_iter().rev() {
        selection = Some(SelectionRange {
            range: Range::new(
                index.position_at_byte(range.start)?,
                index.position_at_byte(range.end)?,
            ),
            parent: selection.map(Box::new),
        });
    }
    selection
}

fn strictly_contains(outer: &ByteRange<usize>, inner: &ByteRange<usize>) -> bool {
    outer != inner && outer.start <= inner.start && inner.end <= outer.end
}

struct RangeCollector<'a> {
    file: &'a SourceFile,
    ranges: Vec<ByteRange<usize>>,
}

impl RangeCollector<'_> {
    fn push(&mut self, span: Span) {
        // All syntax comes from this parsed file. Validate directly against its source to avoid
        // source-map lookups and rope traversals for every AST node, including duplicate spans.
        self.ranges.extend(checked_span_range(self.file, span));
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
