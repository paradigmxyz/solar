//! Function inlining optimization pass.
//!
//! This module provides heuristics and analysis for deciding when to inline function calls.
//! Inlining small functions saves JUMP gas and enables further optimizations.
//!
//! ## Inlining Heuristics
//!
//! A function is considered inlineable if:
//! - Body is < N instructions (configurable, default 10)
//! - Only called once (always inline)
//! - Called in hot loop (inline if small)
//! - No complex control flow (single return path preferred)
//! - Not recursive
//!
//! ## Optimization Levels
//!
//! - `O0`: No inlining
//! - `O1`: Conservative inlining (very small functions, single-call)
//! - `O2`: Aggressive inlining (larger threshold, loop-aware)

use rustc_hash::{FxHashMap, FxHashSet};
use solar_sema::hir::{self, FunctionId, StmtKind};

/// Optimization level for the compiler.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum OptLevel {
    /// No optimizations.
    O0 = 0,
    /// Conservative optimizations (default).
    #[default]
    O1 = 1,
    /// Aggressive optimizations.
    O2 = 2,
    /// Maximum optimizations (size may increase).
    O3 = 3,
}

impl OptLevel {
    /// Returns true if any optimizations are enabled.
    #[must_use]
    pub fn is_optimizing(self) -> bool {
        self != Self::O0
    }

    /// Returns true if inlining is enabled at this optimization level.
    #[must_use]
    pub fn inline_enabled(self) -> bool {
        self >= Self::O1
    }
}

/// Configuration for function inlining.
#[derive(Clone, Debug)]
pub struct InlineConfig {
    /// Maximum number of statements in a function to consider for inlining.
    /// Functions with more statements than this are not inlined.
    pub max_statements: usize,
    /// Maximum number of statements for aggressive inlining.
    pub max_statements_aggressive: usize,
    /// Always inline functions called only once.
    pub inline_single_call: bool,
    /// Inline threshold multiplier for functions called in loops.
    pub loop_inline_multiplier: f32,
    /// Maximum inline depth (to prevent excessive code bloat).
    pub max_inline_depth: usize,
}

impl Default for InlineConfig {
    fn default() -> Self {
        Self::for_opt_level(OptLevel::O1)
    }
}

impl InlineConfig {
    /// Creates an inline configuration for the given optimization level.
    #[must_use]
    pub fn for_opt_level(level: OptLevel) -> Self {
        match level {
            OptLevel::O0 => Self {
                max_statements: 0,
                max_statements_aggressive: 0,
                inline_single_call: false,
                loop_inline_multiplier: 1.0,
                max_inline_depth: 0,
            },
            OptLevel::O1 => Self {
                max_statements: 5,
                max_statements_aggressive: 10,
                inline_single_call: true,
                loop_inline_multiplier: 1.5,
                max_inline_depth: 16,
            },
            OptLevel::O2 => Self {
                max_statements: 10,
                max_statements_aggressive: 20,
                inline_single_call: true,
                loop_inline_multiplier: 2.0,
                max_inline_depth: 32,
            },
            OptLevel::O3 => Self {
                max_statements: 20,
                max_statements_aggressive: 50,
                inline_single_call: true,
                loop_inline_multiplier: 3.0,
                max_inline_depth: 64,
            },
        }
    }

    /// Returns true if inlining is disabled.
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        self.max_statements == 0 && !self.inline_single_call
    }
}

/// Inlining decision for a function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineDecision {
    /// Always inline this function.
    Always,
    /// Inline if call site is in a hot context (e.g., loop).
    IfHot,
    /// Never inline this function.
    Never,
}

/// Analysis result for a function's inlineability.
#[derive(Clone, Debug)]
pub struct FunctionInlineInfo {
    /// Number of statements in the function body.
    pub statement_count: usize,
    /// Number of call sites for this function.
    pub call_count: usize,
    /// Whether the function is recursive (directly or indirectly).
    pub is_recursive: bool,
    /// Whether the function has complex control flow.
    pub has_complex_control_flow: bool,
    /// Whether the function has multiple return points.
    pub has_multiple_returns: bool,
    /// The inlining decision for this function.
    pub decision: InlineDecision,
}

impl Default for FunctionInlineInfo {
    fn default() -> Self {
        Self {
            statement_count: 0,
            call_count: 0,
            is_recursive: false,
            has_complex_control_flow: false,
            has_multiple_returns: false,
            decision: InlineDecision::Never,
        }
    }
}

/// Analyzes functions for inlining opportunities.
pub struct InlineAnalyzer<'a> {
    /// The HIR context.
    hir: &'a hir::Hir<'a>,
    /// Inline configuration.
    config: InlineConfig,
    /// Analysis results for each function.
    info: FxHashMap<FunctionId, FunctionInlineInfo>,
    /// Call graph: maps caller -> set of callees.
    call_graph: FxHashMap<FunctionId, FxHashSet<FunctionId>>,
}

impl<'a> InlineAnalyzer<'a> {
    /// Creates a new inline analyzer.
    pub fn new(hir: &'a hir::Hir<'a>, config: InlineConfig) -> Self {
        Self { hir, config, info: FxHashMap::default(), call_graph: FxHashMap::default() }
    }

    /// Analyzes all functions and returns inlining decisions.
    pub fn analyze(&mut self) -> &FxHashMap<FunctionId, FunctionInlineInfo> {
        if self.config.is_disabled() {
            return &self.info;
        }

        // First pass: collect basic info for each function
        for func_id in self.hir.function_ids() {
            let info = self.analyze_function(func_id);
            self.info.insert(func_id, info);
        }

        // Second pass: detect recursive functions
        self.detect_recursion();

        // Third pass: count call sites
        self.count_call_sites();

        // Fourth pass: make inlining decisions
        self.make_decisions();

        &self.info
    }

    /// Analyzes a single function.
    fn analyze_function(&mut self, func_id: FunctionId) -> FunctionInlineInfo {
        let func = self.hir.function(func_id);
        let mut info = FunctionInlineInfo::default();

        if let Some(body) = &func.body {
            info.statement_count = self.count_statements(body);
            info.has_multiple_returns = self.count_returns(body) > 1;
            info.has_complex_control_flow = self.has_complex_control_flow(body);

            // Build call graph
            let callees = self.collect_callees(body);
            if !callees.is_empty() {
                self.call_graph.insert(func_id, callees);
            }
        }

        info
    }

    /// Counts statements in a block (recursive).
    fn count_statements(&self, block: &hir::Block<'_>) -> usize {
        let mut count = 0;
        for stmt in block.stmts {
            count += self.count_statement(stmt);
        }
        count
    }

    /// Counts a single statement (may be nested).
    fn count_statement(&self, stmt: &hir::Stmt<'_>) -> usize {
        match &stmt.kind {
            StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => {
                self.count_statements(block)
            }
            StmtKind::If(_, then_stmt, else_stmt) => {
                let mut count = 1;
                count += self.count_statement(then_stmt);
                if let Some(else_s) = else_stmt {
                    count += self.count_statement(else_s);
                }
                count
            }
            StmtKind::Loop(block, _) => 1 + self.count_statements(block),
            StmtKind::Try(try_stmt) => {
                let mut count = 1;
                for clause in try_stmt.clauses {
                    count += self.count_statements(&clause.block);
                }
                count
            }
            _ => 1,
        }
    }

    /// Counts return statements in a block.
    fn count_returns(&self, block: &hir::Block<'_>) -> usize {
        let mut count = 0;
        for stmt in block.stmts {
            count += self.count_returns_stmt(stmt);
        }
        count
    }

    /// Counts returns in a statement.
    fn count_returns_stmt(&self, stmt: &hir::Stmt<'_>) -> usize {
        match &stmt.kind {
            StmtKind::Return(_) => 1,
            StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => self.count_returns(block),
            StmtKind::If(_, then_stmt, else_stmt) => {
                let mut count = self.count_returns_stmt(then_stmt);
                if let Some(else_s) = else_stmt {
                    count += self.count_returns_stmt(else_s);
                }
                count
            }
            StmtKind::Loop(block, _) => self.count_returns(block),
            StmtKind::Try(try_stmt) => {
                let mut count = 0;
                for clause in try_stmt.clauses {
                    count += self.count_returns(&clause.block);
                }
                count
            }
            _ => 0,
        }
    }

    /// Checks if a block has complex control flow.
    fn has_complex_control_flow(&self, block: &hir::Block<'_>) -> bool {
        for stmt in block.stmts {
            if self.stmt_has_complex_control_flow(stmt) {
                return true;
            }
        }
        false
    }

    /// Checks if a statement has complex control flow.
    fn stmt_has_complex_control_flow(&self, stmt: &hir::Stmt<'_>) -> bool {
        match &stmt.kind {
            // Loops add complexity
            StmtKind::Loop(_, _) => true,
            // Try/catch is complex
            StmtKind::Try(_) => true,
            // Nested blocks may have complex control flow
            StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => {
                self.has_complex_control_flow(block)
            }
            // Deeply nested if-else is considered complex
            StmtKind::If(_, then_stmt, else_stmt) => {
                self.stmt_has_complex_control_flow(then_stmt)
                    || else_stmt.is_some_and(|s| self.stmt_has_complex_control_flow(s))
            }
            _ => false,
        }
    }

    /// Collects all function callees in a block.
    fn collect_callees(&self, block: &hir::Block<'_>) -> FxHashSet<FunctionId> {
        let mut callees = FxHashSet::default();
        for stmt in block.stmts {
            self.collect_callees_stmt(stmt, &mut callees);
        }
        callees
    }

    /// Collects callees from a statement.
    fn collect_callees_stmt(&self, stmt: &hir::Stmt<'_>, callees: &mut FxHashSet<FunctionId>) {
        match &stmt.kind {
            StmtKind::Expr(expr) => self.collect_callees_expr(expr, callees),
            StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => {
                for s in block.stmts {
                    self.collect_callees_stmt(s, callees);
                }
            }
            StmtKind::If(cond, then_stmt, else_stmt) => {
                self.collect_callees_expr(cond, callees);
                self.collect_callees_stmt(then_stmt, callees);
                if let Some(else_s) = else_stmt {
                    self.collect_callees_stmt(else_s, callees);
                }
            }
            StmtKind::Loop(block, _) => {
                for s in block.stmts {
                    self.collect_callees_stmt(s, callees);
                }
            }
            StmtKind::Return(Some(expr)) | StmtKind::Revert(expr) | StmtKind::Emit(expr) => {
                self.collect_callees_expr(expr, callees);
            }
            StmtKind::DeclSingle(var_id) => {
                let var = self.hir.variable(*var_id);
                if let Some(init) = var.initializer {
                    self.collect_callees_expr(init, callees);
                }
            }
            _ => {}
        }
    }

    /// Collects callees from an expression.
    fn collect_callees_expr(&self, expr: &hir::Expr<'_>, callees: &mut FxHashSet<FunctionId>) {
        match &expr.kind {
            hir::ExprKind::Call(callee, args, _) => {
                // Check if callee is a function reference
                if let hir::ExprKind::Ident(res_slice) = &callee.kind {
                    for res in res_slice.iter() {
                        if let hir::Res::Item(hir::ItemId::Function(func_id)) = res {
                            callees.insert(*func_id);
                        }
                    }
                }
                self.collect_callees_expr(callee, callees);
                for arg in args.kind.exprs() {
                    self.collect_callees_expr(arg, callees);
                }
            }
            hir::ExprKind::Binary(lhs, _, rhs) => {
                self.collect_callees_expr(lhs, callees);
                self.collect_callees_expr(rhs, callees);
            }
            hir::ExprKind::Unary(_, operand) => {
                self.collect_callees_expr(operand, callees);
            }
            hir::ExprKind::Ternary(cond, then_expr, else_expr) => {
                self.collect_callees_expr(cond, callees);
                self.collect_callees_expr(then_expr, callees);
                self.collect_callees_expr(else_expr, callees);
            }
            hir::ExprKind::Member(base, _) => {
                self.collect_callees_expr(base, callees);
            }
            hir::ExprKind::Index(base, idx) => {
                self.collect_callees_expr(base, callees);
                if let Some(i) = idx {
                    self.collect_callees_expr(i, callees);
                }
            }
            hir::ExprKind::Array(elems) => {
                for elem in elems.iter() {
                    self.collect_callees_expr(elem, callees);
                }
            }
            hir::ExprKind::Tuple(elems) => {
                for elem in elems.iter().flatten() {
                    self.collect_callees_expr(elem, callees);
                }
            }
            hir::ExprKind::Assign(lhs, _, rhs) => {
                self.collect_callees_expr(lhs, callees);
                self.collect_callees_expr(rhs, callees);
            }
            _ => {}
        }
    }

    /// Detects recursive functions using DFS on the call graph.
    fn detect_recursion(&mut self) {
        let func_ids: Vec<_> = self.info.keys().copied().collect();
        for func_id in func_ids {
            if self.is_recursive(func_id, &mut FxHashSet::default())
                && let Some(info) = self.info.get_mut(&func_id)
            {
                info.is_recursive = true;
            }
        }
    }

    /// Checks if a function is recursive (directly or indirectly).
    fn is_recursive(&self, func_id: FunctionId, visited: &mut FxHashSet<FunctionId>) -> bool {
        if !visited.insert(func_id) {
            return true; // Cycle detected
        }

        if let Some(callees) = self.call_graph.get(&func_id) {
            for &callee in callees {
                if self.is_recursive(callee, visited) {
                    return true;
                }
            }
        }

        visited.remove(&func_id);
        false
    }

    /// Counts call sites for each function.
    fn count_call_sites(&mut self) {
        // Count how many times each function is called
        let mut call_counts: FxHashMap<FunctionId, usize> = FxHashMap::default();

        for callees in self.call_graph.values() {
            for &callee in callees {
                *call_counts.entry(callee).or_default() += 1;
            }
        }

        for (func_id, count) in call_counts {
            if let Some(info) = self.info.get_mut(&func_id) {
                info.call_count = count;
            }
        }
    }

    /// Makes inlining decisions for all functions.
    fn make_decisions(&mut self) {
        let func_ids: Vec<_> = self.info.keys().copied().collect();
        for func_id in func_ids {
            let decision = self.decide_inline(func_id);
            if let Some(info) = self.info.get_mut(&func_id) {
                info.decision = decision;
            }
        }
    }

    /// Decides whether to inline a function.
    fn decide_inline(&self, func_id: FunctionId) -> InlineDecision {
        let Some(info) = self.info.get(&func_id) else {
            return InlineDecision::Never;
        };

        // Never inline recursive functions
        if info.is_recursive {
            return InlineDecision::Never;
        }

        // Check if function is external/public - don't inline entry points
        let func = self.hir.function(func_id);
        if matches!(func.visibility, hir::Visibility::External | hir::Visibility::Public) {
            return InlineDecision::Never;
        }

        // Always inline single-call functions if enabled
        if self.config.inline_single_call && info.call_count == 1 {
            return InlineDecision::Always;
        }

        // Check statement count threshold
        if info.statement_count <= self.config.max_statements {
            // Small function - always inline
            return InlineDecision::Always;
        }

        // Medium-sized functions - inline in hot contexts
        if info.statement_count <= self.config.max_statements_aggressive
            && !info.has_complex_control_flow
        {
            return InlineDecision::IfHot;
        }

        InlineDecision::Never
    }

    /// Returns the inlining decision for a function.
    #[must_use]
    pub fn get_decision(&self, func_id: FunctionId) -> InlineDecision {
        self.info.get(&func_id).map_or(InlineDecision::Never, |i| i.decision)
    }

    /// Returns the inline info for a function.
    #[must_use]
    pub fn get_info(&self, func_id: FunctionId) -> Option<&FunctionInlineInfo> {
        self.info.get(&func_id)
    }
}

/// Statistics about inlining performed.
#[derive(Clone, Debug, Default)]
pub struct InlineStats {
    /// Number of functions analyzed.
    pub functions_analyzed: usize,
    /// Number of functions marked for always-inline.
    pub always_inline: usize,
    /// Number of functions marked for conditional inline.
    pub if_hot_inline: usize,
    /// Number of functions that won't be inlined.
    pub never_inline: usize,
    /// Number of recursive functions detected.
    pub recursive_functions: usize,
}

impl InlineStats {
    /// Collects statistics from an analyzer.
    pub fn from_analyzer(analyzer: &InlineAnalyzer<'_>) -> Self {
        let mut stats = Self::default();

        for info in analyzer.info.values() {
            stats.functions_analyzed += 1;

            match info.decision {
                InlineDecision::Always => stats.always_inline += 1,
                InlineDecision::IfHot => stats.if_hot_inline += 1,
                InlineDecision::Never => stats.never_inline += 1,
            }

            if info.is_recursive {
                stats.recursive_functions += 1;
            }
        }

        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opt_level_ordering() {
        assert!(OptLevel::O0 < OptLevel::O1);
        assert!(OptLevel::O1 < OptLevel::O2);
        assert!(OptLevel::O2 < OptLevel::O3);
    }

    #[test]
    fn test_opt_level_inline_enabled() {
        assert!(!OptLevel::O0.inline_enabled());
        assert!(OptLevel::O1.inline_enabled());
        assert!(OptLevel::O2.inline_enabled());
        assert!(OptLevel::O3.inline_enabled());
    }

    #[test]
    fn test_inline_config_o0_disabled() {
        let config = InlineConfig::for_opt_level(OptLevel::O0);
        assert!(config.is_disabled());
    }

    #[test]
    fn test_inline_config_o1() {
        let config = InlineConfig::for_opt_level(OptLevel::O1);
        assert!(!config.is_disabled());
        assert_eq!(config.max_statements, 5);
        assert!(config.inline_single_call);
    }

    #[test]
    fn test_inline_config_o2() {
        let config = InlineConfig::for_opt_level(OptLevel::O2);
        assert!(!config.is_disabled());
        assert_eq!(config.max_statements, 10);
        assert_eq!(config.max_inline_depth, 32);
    }

    #[test]
    fn test_inline_decision_default() {
        let info = FunctionInlineInfo::default();
        assert_eq!(info.decision, InlineDecision::Never);
        assert!(!info.is_recursive);
    }
}
