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

use crate::{
    analysis::LoopAnalyzer,
    mir::{
        BlockId, Function, FunctionId as MirFunctionId, Immediate, InstKind, Instruction, MirType,
        Module, Terminator, Value, ValueId,
    },
    pass::ModulePass,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    map::FxHashMap,
};
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
    call_graph: FxHashMap<FunctionId, GrowableBitSet<FunctionId>>,
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
    fn collect_callees(&self, block: &hir::Block<'_>) -> GrowableBitSet<FunctionId> {
        let mut callees = GrowableBitSet::new_empty();
        for stmt in block.stmts {
            self.collect_callees_stmt(stmt, &mut callees);
        }
        callees
    }

    /// Collects callees from a statement.
    fn collect_callees_stmt(&self, stmt: &hir::Stmt<'_>, callees: &mut GrowableBitSet<FunctionId>) {
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
    fn collect_callees_expr(&self, expr: &hir::Expr<'_>, callees: &mut GrowableBitSet<FunctionId>) {
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
            if self.is_recursive(func_id, &mut GrowableBitSet::new_empty())
                && let Some(info) = self.info.get_mut(&func_id)
            {
                info.is_recursive = true;
            }
        }
    }

    /// Checks if a function is recursive (directly or indirectly).
    fn is_recursive(&self, func_id: FunctionId, visited: &mut GrowableBitSet<FunctionId>) -> bool {
        if !visited.insert(func_id) {
            return true; // Cycle detected
        }

        if let Some(callees) = self.call_graph.get(&func_id) {
            for callee in callees {
                if self.is_recursive(callee, visited) {
                    return true;
                }
            }
        }

        visited.remove(func_id);
        false
    }

    /// Counts call sites for each function.
    fn count_call_sites(&mut self) {
        // Count how many times each function is called
        let mut call_counts: FxHashMap<FunctionId, usize> = FxHashMap::default();

        for callees in self.call_graph.values() {
            for callee in callees {
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

/// Configuration for MIR-level internal-call inlining.
#[derive(Clone, Debug)]
pub struct MirInlineConfig {
    /// Maximum instruction count for ordinary inline candidates.
    pub max_instructions: usize,
    /// Maximum instruction count for functions that have exactly one call site.
    pub max_single_call_instructions: usize,
    /// Hard sanity limit for single-call-site callees. These bypass the normal
    /// size and block caps because function DCE removes their original body.
    pub max_single_call_sanity_instructions: usize,
    /// Maximum number of blocks to clone from one callee.
    pub max_blocks: usize,
    /// Whether a single call site may use the larger threshold.
    pub inline_single_call: bool,
    /// Maximum estimated runtime bytecode growth for a cold call site.
    pub max_cold_code_growth: usize,
    /// Maximum estimated runtime bytecode growth for a call site inside a loop.
    pub max_hot_code_growth: usize,
    /// Maximum number of instructions a single caller may gain from inlining
    /// multi-use callees, bounding total code growth per function.
    pub max_caller_inlined_instructions: usize,
    /// Minimum estimated internal-call protocol gas saved before inlining.
    pub min_call_savings: u64,
    /// Size-aware backstop: once a module's estimated runtime bytecode reaches
    /// this many bytes, stop inlining (which grows code) so the contract stays
    /// under the EIP-170 deployable-code limit. Small contracts never reach it
    /// and inline normally.
    pub max_module_code_size: usize,
}

impl Default for MirInlineConfig {
    fn default() -> Self {
        Self {
            max_instructions: 96,
            max_single_call_instructions: 96,
            max_single_call_sanity_instructions: 4096,
            max_blocks: 10,
            inline_single_call: true,
            max_cold_code_growth: 256,
            max_hot_code_growth: 512,
            max_caller_inlined_instructions: 64,
            min_call_savings: 120,
            // Budget in `estimated_code_size` units (a per-instruction proxy that
            // runs well below final bytecode because it does not model stack
            // scheduling/spills). Calibrated as a conservative backstop: a module
            // already this large has little headroom under the EIP-170 24576-byte
            // limit, so further (growth-only) inlining is skipped to keep it
            // deployable. Ordinary contracts are far smaller and inline normally.
            max_module_code_size: 7450,
        }
    }
}

impl MirInlineConfig {
    /// Configuration for `-O size`: a module budget of zero disables all MIR
    /// inlining, which only ever grows emitted code on real contracts (both
    /// multi-use duplication and the cascades that single-call inlining sets
    /// off were measured to increase size). Lowering-time inlining of tiny
    /// single-return helpers is deliberately kept: solar's internal-call
    /// protocol (memory frame setup) costs more bytes than those bodies, so
    /// sharing them was measured to *increase* code size as well.
    #[must_use]
    pub fn for_size() -> Self {
        Self { max_module_code_size: 0, ..Self::default() }
    }
}

/// Statistics for MIR-level inlining.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MirInlineStats {
    /// Number of internal call sites considered.
    pub call_sites: usize,
    /// Number of call sites inlined.
    pub inlined: usize,
    /// Number of call sites skipped because the callee was not inlineable.
    pub skipped: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct MirInlineSummary {
    instruction_count: usize,
    block_count: usize,
    return_count: usize,
    param_count: usize,
    estimated_code_size: usize,
    estimated_runtime_gas: u64,
    internal_frame_size: u64,
    has_internal_call: bool,
    has_phi: bool,
    has_external_call: bool,
    has_storage_write: bool,
    has_log: bool,
    has_control_flow: bool,
    has_unsupported_terminator: bool,
    is_entry_point: bool,
    is_constructor: bool,
    no_inline: bool,
}

/// Module-level MIR internal-call inliner.
///
/// This pass clones small internal/private callees into their callers. Each
/// inline expansion gets a fresh internal-frame range so copied local slots do
/// not overlap caller locals.
pub struct MirInliner {
    config: MirInlineConfig,
}

/// Module pass for metadata-backed MIR inlining.
pub struct InlinePass;

impl ModulePass for InlinePass {
    fn name(&self) -> &str {
        "inline"
    }

    fn run(&mut self, module: &mut Module) -> bool {
        let config = if module.optimize_for_size {
            MirInlineConfig::for_size()
        } else {
            MirInlineConfig::default()
        };
        MirInliner::new(config).run(module).inlined != 0
    }
}

impl Default for MirInliner {
    fn default() -> Self {
        Self::new(MirInlineConfig::default())
    }
}

impl MirInliner {
    /// Creates a new MIR inliner with the given configuration.
    #[must_use]
    pub fn new(config: MirInlineConfig) -> Self {
        Self { config }
    }

    /// Runs the inliner over the whole module.
    pub fn run(&mut self, module: &mut Module) -> MirInlineStats {
        let mut stats = MirInlineStats::default();
        let mut summaries = self.summarize_module(module);
        let mut call_counts = self.call_counts(module);
        let recursive_functions = self.recursive_functions(module);

        // Size-aware backstop: inlining grows emitted code, so track the module's
        // estimated runtime bytecode and stop inlining once it reaches the budget,
        // keeping large contracts under the EIP-170 deployable-code limit. Small
        // contracts never reach the budget and inline normally.
        let mut module_code_size: usize = summaries.values().map(|s| s.estimated_code_size).sum();

        for caller_id in module.functions.indices().collect::<Vec<_>>() {
            let loop_depths = block_loop_depths(module.function(caller_id));
            // Bound how much each caller may grow from inlining so a function
            // calling many internal helpers (e.g. a large verifier) cannot
            // balloon past the deployable code-size limit.
            let base_instructions =
                summaries.get(&caller_id).map(|s| s.instruction_count).unwrap_or_default();
            let mut cursor = (0, 0);
            while let Some(site) =
                self.find_next_call(module.function(caller_id), cursor, &loop_depths)
            {
                stats.call_sites += 1;
                cursor = (site.block.index(), site.inst_index + 1);

                let Some(summary) = summaries.get(&site.callee).copied() else {
                    stats.skipped += 1;
                    continue;
                };
                let call_count = call_counts.get(&site.callee).copied().unwrap_or_default();
                let grew_too_much = summaries.get(&caller_id).is_some_and(|s| {
                    s.instruction_count.saturating_sub(base_instructions)
                        > self.config.max_caller_inlined_instructions
                });
                if module_code_size >= self.config.max_module_code_size
                    || grew_too_much
                    || recursive_functions.contains(site.callee)
                    || !self.is_inlineable(caller_id, site, summary, call_count)
                {
                    stats.skipped += 1;
                    continue;
                }

                let callee = module.function(site.callee).clone();
                let old_size =
                    summaries.get(&caller_id).map(|s| s.estimated_code_size).unwrap_or_default();
                let caller = module.function_mut(caller_id);
                if inline_call(caller, site.block, site.inst_index, &callee) {
                    stats.inlined += 1;
                    let new_summary = summarize_function(module.function(caller_id));
                    module_code_size = module_code_size
                        .saturating_sub(old_size)
                        .saturating_add(new_summary.estimated_code_size);
                    summaries.insert(caller_id, new_summary);
                    call_counts = self.call_counts(module);
                    cursor = (site.block.index(), 0);
                } else {
                    stats.skipped += 1;
                }
            }
        }

        stats
    }

    fn summarize_module(&self, module: &Module) -> FxHashMap<MirFunctionId, MirInlineSummary> {
        module
            .functions
            .iter_enumerated()
            .map(|(id, func)| (id, summarize_function(func)))
            .collect()
    }

    fn call_counts(&self, module: &Module) -> FxHashMap<MirFunctionId, usize> {
        let mut counts = FxHashMap::default();
        for func in module.functions.iter() {
            for block in func.blocks.iter() {
                for &inst_id in &block.instructions {
                    if let InstKind::InternalCall { function, .. } = func.instructions[inst_id].kind
                    {
                        *counts.entry(function).or_default() += 1;
                    }
                }
            }
        }
        counts
    }

    fn recursive_functions(&self, module: &Module) -> DenseBitSet<MirFunctionId> {
        let mut recursive = DenseBitSet::new_empty(module.functions.len());
        for func_id in module.functions.indices() {
            let mut visiting = DenseBitSet::new_empty(module.functions.len());
            if self.function_reaches(module, func_id, func_id, &mut visiting) {
                recursive.insert(func_id);
            }
        }
        recursive
    }

    fn function_reaches(
        &self,
        module: &Module,
        current: MirFunctionId,
        target: MirFunctionId,
        visiting: &mut DenseBitSet<MirFunctionId>,
    ) -> bool {
        if !visiting.insert(current) {
            return false;
        }

        for callee in self.function_callees(module.function(current)) {
            if callee == target || self.function_reaches(module, callee, target, visiting) {
                return true;
            }
        }

        false
    }

    fn function_callees(&self, func: &Function) -> Vec<MirFunctionId> {
        let mut callees = Vec::new();
        for block in func.blocks.iter() {
            for &inst_id in &block.instructions {
                if let InstKind::InternalCall { function, .. } = func.instructions[inst_id].kind {
                    callees.push(function);
                }
            }
        }
        callees
    }

    fn find_next_call(
        &self,
        func: &Function,
        start: (usize, usize),
        loop_depths: &FxHashMap<BlockId, usize>,
    ) -> Option<CallSite> {
        for (block, bb) in func.blocks.iter_enumerated().skip(start.0) {
            let start_inst = if block.index() == start.0 { start.1 } else { 0 };
            for (inst_index, &inst_id) in bb.instructions.iter().enumerate().skip(start_inst) {
                if let InstKind::InternalCall { function, ref args, returns } =
                    func.instructions[inst_id].kind
                {
                    return Some(CallSite {
                        block,
                        inst_index,
                        callee: function,
                        args_len: args.len(),
                        returns: returns as usize,
                        loop_depth: loop_depths.get(&block).copied().unwrap_or_default(),
                    });
                }
            }
        }
        None
    }

    fn is_inlineable(
        &self,
        caller: MirFunctionId,
        site: CallSite,
        summary: MirInlineSummary,
        call_count: usize,
    ) -> bool {
        let single_call = self.config.inline_single_call && call_count == 1;

        // `no_inline` prevents cloning a shared helper into every caller; with
        // a single call site there is nothing to duplicate, and absorbing the
        // helper removes the call protocol around its only use.
        if caller == site.callee
            || (summary.no_inline && !single_call)
            || summary.is_entry_point
            || summary.is_constructor
            || summary.has_phi
            || summary.has_unsupported_terminator
            || summary.return_count == 0
        {
            return false;
        }

        if single_call {
            if summary.instruction_count > self.config.max_single_call_sanity_instructions {
                return false;
            }
        } else if summary.block_count > self.config.max_blocks
            || summary.instruction_count > self.config.max_instructions
        {
            return false;
        }

        // Multi-use stateful callees are usually not worth cloning unless the
        // call is hot or the body is no larger than the internal-call protocol
        // it replaces. Single-call callees disappear from emitted runtime
        // bytecode after inlining, so they are allowed through the normal
        // code-growth check below.
        if !single_call
            && site.loop_depth == 0
            && (summary.has_storage_write || summary.has_external_call || summary.has_log)
            && summary.estimated_code_size
                > estimated_internal_call_code_size(site)
                    + estimated_internal_return_code_size(summary, site)
        {
            return false;
        }

        let code_growth = estimated_inline_code_growth(summary, site, single_call);
        let max_growth = if site.loop_depth > 0 {
            self.config.max_hot_code_growth
        } else {
            self.config.max_cold_code_growth
        };
        if code_growth > max_growth {
            return false;
        }

        let savings = estimated_internal_call_savings(site, summary);
        savings >= self.config.min_call_savings
    }
}

#[derive(Clone, Copy)]
struct CallSite {
    block: BlockId,
    inst_index: usize,
    callee: MirFunctionId,
    args_len: usize,
    returns: usize,
    loop_depth: usize,
}

fn summarize_function(func: &Function) -> MirInlineSummary {
    let mut summary = MirInlineSummary {
        block_count: func.blocks.len(),
        param_count: func.params.len(),
        internal_frame_size: func.internal_frame_size,
        is_entry_point: func.is_public()
            || func.attributes.is_fallback
            || func.attributes.is_receive
            || func.selector.is_some(),
        is_constructor: func.attributes.is_constructor,
        no_inline: func.attributes.no_inline,
        ..MirInlineSummary::default()
    };

    for block in func.blocks.iter() {
        for &inst_id in &block.instructions {
            let kind = &func.instructions[inst_id].kind;
            summary.instruction_count += match kind {
                InstKind::MappingSlot(..) => 3,
                InstKind::MappingSlotMemory(..) => 8,
                InstKind::MappingSlotCalldata(..) => 9,
                _ => 1,
            };
            let inst_cost = estimate_inst_cost(kind);
            summary.estimated_code_size += inst_cost.code_size;
            summary.estimated_runtime_gas += inst_cost.runtime_gas;
            match kind {
                InstKind::InternalCall { .. } => summary.has_internal_call = true,
                InstKind::Phi(_) => summary.has_phi = true,
                InstKind::Call { .. }
                | InstKind::StaticCall { .. }
                | InstKind::DelegateCall { .. }
                | InstKind::Create(..)
                | InstKind::Create2(..) => {
                    summary.has_external_call = true;
                }
                InstKind::SStore(..) | InstKind::TStore(..) => summary.has_storage_write = true,
                InstKind::Log0(..)
                | InstKind::Log1(..)
                | InstKind::Log2(..)
                | InstKind::Log3(..)
                | InstKind::Log4(..) => summary.has_log = true,
                _ => {}
            }
        }
        match block.terminator.as_ref() {
            Some(term @ Terminator::Return { .. }) => {
                summary.return_count += 1;
                let term_cost = estimate_terminator_cost(term);
                summary.estimated_code_size += term_cost.code_size;
                summary.estimated_runtime_gas += term_cost.runtime_gas;
            }
            Some(term @ Terminator::Revert { .. }) => {
                let term_cost = estimate_terminator_cost(term);
                summary.estimated_code_size += term_cost.code_size;
                summary.estimated_runtime_gas += term_cost.runtime_gas;
            }
            // A void internal function returns via `Stop` (the backend lowers it
            // to an internal return). Treat it as a return point so void callees
            // can be inlined.
            Some(Terminator::Stop) if func.returns.is_empty() => {
                summary.return_count += 1;
            }
            Some(Terminator::Jump(_))
            | Some(Terminator::Branch { .. })
            | Some(Terminator::Switch { .. }) => {
                summary.has_control_flow = true;
                let term_cost = estimate_terminator_cost(block.terminator.as_ref().unwrap());
                summary.estimated_code_size += term_cost.code_size;
                summary.estimated_runtime_gas += term_cost.runtime_gas;
            }
            Some(Terminator::ReturnData { .. })
            | Some(Terminator::Stop)
            | Some(Terminator::SelfDestruct { .. })
            | Some(Terminator::TailCall { .. })
            | None => summary.has_unsupported_terminator = true,
            Some(Terminator::Invalid) => {}
        }
    }

    summary
}

#[derive(Clone, Copy, Debug, Default)]
struct MirCost {
    runtime_gas: u64,
    code_size: usize,
}

fn estimate_inst_cost(kind: &InstKind) -> MirCost {
    let (runtime_gas, code_size) = match kind {
        InstKind::Add(..)
        | InstKind::Sub(..)
        | InstKind::Lt(..)
        | InstKind::Gt(..)
        | InstKind::SLt(..)
        | InstKind::SGt(..)
        | InstKind::Eq(..)
        | InstKind::IsZero(..)
        | InstKind::And(..)
        | InstKind::Or(..)
        | InstKind::Xor(..)
        | InstKind::Not(..)
        | InstKind::Byte(..)
        | InstKind::Shl(..)
        | InstKind::Shr(..)
        | InstKind::Sar(..)
        | InstKind::SignExtend(..)
        | InstKind::MLoad(..)
        | InstKind::MStore(..)
        | InstKind::MStore8(..)
        | InstKind::CalldataLoad(..)
        | InstKind::CalldataSize
        | InstKind::Caller
        | InstKind::CallValue
        | InstKind::Origin
        | InstKind::GasPrice
        | InstKind::Coinbase
        | InstKind::Timestamp
        | InstKind::BlockNumber
        | InstKind::PrevRandao
        | InstKind::GasLimit
        | InstKind::ChainId
        | InstKind::Address
        | InstKind::SelfBalance
        | InstKind::Gas
        | InstKind::BaseFee
        | InstKind::BlobBaseFee => (3, 1),
        InstKind::Mul(..)
        | InstKind::Div(..)
        | InstKind::SDiv(..)
        | InstKind::Mod(..)
        | InstKind::SMod(..) => (5, 1),
        InstKind::Exp(..) => (50, 1),
        InstKind::AddMod(..) | InstKind::MulMod(..) => (8, 1),
        InstKind::SLoad(..) | InstKind::TLoad(..) => (100, 1),
        InstKind::SStore(..) | InstKind::TStore(..) => (5_000, 1),
        InstKind::MCopy(..)
        | InstKind::CalldataCopy(..)
        | InstKind::CodeCopy(..)
        | InstKind::ExtCodeCopy(..)
        | InstKind::ReturnDataCopy(..) => (12, 1),
        InstKind::MSize | InstKind::CodeSize | InstKind::ReturnDataSize => (2, 1),
        InstKind::InternalFrameAddr(_) => (6, 3),
        // PUSH32 placeholder patched at deploy time.
        InstKind::LoadImmutable(_) => (3, 33),
        InstKind::ExtCodeSize(..)
        | InstKind::ExtCodeHash(..)
        | InstKind::Balance(..)
        | InstKind::BlockHash(..)
        | InstKind::BlobHash(..)
        | InstKind::Keccak256(..) => (30, 1),
        InstKind::MappingSlot(..) => (36, 3),
        InstKind::MappingSlotMemory(..) => (60, 8),
        InstKind::MappingSlotCalldata(..) => (63, 9),
        InstKind::Call { .. } | InstKind::StaticCall { .. } | InstKind::DelegateCall { .. } => {
            (700, 1)
        }
        InstKind::InternalCall { args, returns, .. } => {
            let returns = *returns as usize;
            (80 + ((args.len() + returns) as u64) * 20, 16 + (args.len() + returns) * 4)
        }
        InstKind::Create(..) | InstKind::Create2(..) => (32_000, 1),
        InstKind::Log0(..) => (375, 1),
        InstKind::Log1(..) => (750, 1),
        InstKind::Log2(..) => (1_125, 1),
        InstKind::Log3(..) => (1_500, 1),
        InstKind::Log4(..) => (1_875, 1),
        InstKind::Phi(_) | InstKind::Select(..) => (3, 1),
    };
    MirCost { runtime_gas, code_size }
}

fn estimate_terminator_cost(term: &Terminator) -> MirCost {
    let (runtime_gas, code_size) = match term {
        Terminator::Jump(_) => (8, 3),
        Terminator::Branch { .. } => (13, 4),
        Terminator::Switch { cases, .. } => (13 + (cases.len() as u64) * 10, 4 + cases.len() * 4),
        Terminator::Return { values } => (20 + (values.len() as u64) * 12, 8),
        Terminator::Revert { .. } | Terminator::ReturnData { .. } => (20, 4),
        Terminator::Stop => (0, 1),
        Terminator::SelfDestruct { .. } => (5_000, 1),
        Terminator::TailCall { args, .. } => (8 + 3 * args.len() as u64, 4 + args.len()),
        Terminator::Invalid => (0, 1),
    };
    MirCost { runtime_gas, code_size }
}

fn estimated_internal_call_savings(site: CallSite, summary: MirInlineSummary) -> u64 {
    let frame_words =
        (summary.internal_frame_size / 32) + 2 + (site.args_len + site.returns) as u64;
    let protocol = 90 + ((site.args_len + site.returns) as u64) * 24 + frame_words * 6;
    let return_protocol = 24 + (summary.param_count as u64 + site.returns as u64) * 8;
    let loop_multiplier = (site.loop_depth as u64).saturating_add(1);
    (protocol + return_protocol) * loop_multiplier
}

fn estimated_internal_call_code_size(site: CallSite) -> usize {
    18 + (site.args_len + site.returns) * 5
}

fn estimated_internal_return_code_size(summary: MirInlineSummary, site: CallSite) -> usize {
    8 + (summary.param_count + site.returns) * 4
}

fn estimated_inline_code_growth(
    summary: MirInlineSummary,
    site: CallSite,
    single_call: bool,
) -> usize {
    let removed_call = estimated_internal_call_code_size(site);
    if single_call {
        let removed_callee =
            summary.estimated_code_size + estimated_internal_return_code_size(summary, site);
        summary.estimated_code_size.saturating_sub(removed_call + removed_callee)
    } else {
        summary.estimated_code_size.saturating_sub(removed_call)
    }
}

fn block_loop_depths(func: &Function) -> FxHashMap<BlockId, usize> {
    let mut analyzer = LoopAnalyzer::new();
    let loop_info = analyzer.analyze(func);
    let mut depths = FxHashMap::default();
    for loop_data in loop_info.all_loops() {
        for block in &loop_data.blocks {
            *depths.entry(block).or_default() += 1;
        }
    }
    depths
}

fn inline_call(
    caller: &mut Function,
    call_block: BlockId,
    call_inst_index: usize,
    callee: &Function,
) -> bool {
    let snapshot = caller.clone();
    if inline_call_impl(caller, call_block, call_inst_index, callee).is_some() {
        true
    } else {
        *caller = snapshot;
        false
    }
}

fn inline_call_impl(
    caller: &mut Function,
    call_block: BlockId,
    call_inst_index: usize,
    callee: &Function,
) -> Option<()> {
    let call_inst = caller.blocks[call_block].instructions[call_inst_index];
    let InstKind::InternalCall { args, returns, .. } = caller.instructions[call_inst].kind.clone()
    else {
        return None;
    };
    let returns = returns as usize;
    if returns != callee.returns.len() {
        return None;
    }

    let call_result = caller.inst_result_value(call_inst);
    if returns > 0 && call_result.is_none() {
        return None;
    }

    let continuation = caller.alloc_block();
    let old_terminator = caller.blocks[call_block].terminator.take();
    let old_successors = old_terminator.as_ref().map(Terminator::successors).unwrap_or_default();
    let suffix = {
        let block = &mut caller.blocks[call_block];
        block.instructions.split_off(call_inst_index + 1)
    };
    caller.blocks[call_block].instructions.pop();
    caller.blocks[continuation].instructions = suffix;
    caller.blocks[continuation].terminator = old_terminator;
    redirect_phi_predecessors(caller, &old_successors, call_block, continuation);

    let frame_base = caller.internal_frame_size;
    caller.internal_frame_size += callee.internal_frame_size;

    let mut cloner = InlineCloner::new(caller, callee, frame_base, &args);
    let cloned_entry = cloner.clone_blocks(continuation)?;
    cloner.caller.blocks[call_block].terminator = Some(Terminator::Jump(cloned_entry));

    let mut replacements = FxHashMap::default();
    if returns > 0 {
        let return_values = build_return_values(
            cloner.caller,
            continuation,
            &callee.returns,
            &cloner.return_edges,
        )?;
        replacements.insert(call_result?, return_values[0]);
        insert_extra_return_stores(cloner.caller, continuation, &return_values[1..]);
    }

    cloner.caller.replace_uses(&replacements);
    recompute_cfg(cloner.caller);
    prune_phi_incoming_to_predecessors(cloner.caller);
    Some(())
}

struct InlineCloner<'a> {
    caller: &'a mut Function,
    callee: &'a Function,
    frame_base: u64,
    value_map: FxHashMap<ValueId, ValueId>,
    block_map: FxHashMap<BlockId, BlockId>,
    return_edges: Vec<(BlockId, SmallVec<[ValueId; 2]>)>,
}

impl<'a> InlineCloner<'a> {
    fn new(
        caller: &'a mut Function,
        callee: &'a Function,
        frame_base: u64,
        args: &[ValueId],
    ) -> Self {
        let mut value_map = FxHashMap::default();
        for (callee_value, value) in callee.values.iter_enumerated() {
            if let Value::Arg { index, .. } = value
                && let Some(&arg) = args.get(*index as usize)
            {
                value_map.insert(callee_value, arg);
            }
        }
        Self {
            caller,
            callee,
            frame_base,
            value_map,
            block_map: FxHashMap::default(),
            return_edges: Vec::new(),
        }
    }

    fn clone_blocks(&mut self, continuation: BlockId) -> Option<BlockId> {
        for block_id in self.callee.blocks.indices() {
            self.block_map.insert(block_id, self.caller.alloc_block());
        }

        for (callee_block, block) in self.callee.blocks.iter_enumerated() {
            let caller_block = self.block_map[&callee_block];
            let mut instructions = Vec::with_capacity(block.instructions.len());
            for &inst_id in &block.instructions {
                let inst = self.callee.instructions[inst_id].clone();
                let kind = self.clone_inst_kind(inst.kind)?;
                let new_inst = self.caller.alloc_inst(Instruction::new(kind, inst.result_ty));
                instructions.push(new_inst);
                if let Some(callee_result) = self.callee.inst_result_value(inst_id) {
                    let new_result = self.caller.alloc_value(Value::Inst(new_inst));
                    self.value_map.insert(callee_result, new_result);
                }
            }
            self.caller.blocks[caller_block].instructions = instructions;
        }

        for (callee_block, block) in self.callee.blocks.iter_enumerated() {
            let caller_block = self.block_map[&callee_block];
            let term =
                self.clone_terminator(block.terminator.as_ref()?, caller_block, continuation)?;
            self.caller.blocks[caller_block].terminator = Some(term);
        }

        Some(self.block_map[&self.callee.entry_block])
    }

    fn clone_value(&mut self, value: ValueId) -> Option<ValueId> {
        if let Some(&mapped) = self.value_map.get(&value) {
            return Some(mapped);
        }

        let cloned = match self.callee.values[value].clone() {
            Value::Immediate(imm) => self.caller.alloc_value(Value::Immediate(imm)),
            Value::Undef(ty) => self.caller.alloc_value(Value::Undef(ty)),
            Value::Error(guar) => self.caller.alloc_value(Value::Error(guar)),
            Value::Arg { .. } | Value::Inst(_) => return None,
        };
        self.value_map.insert(value, cloned);
        Some(cloned)
    }

    fn clone_block(&self, block: BlockId) -> Option<BlockId> {
        self.block_map.get(&block).copied()
    }

    #[allow(clippy::too_many_lines)]
    fn clone_inst_kind(&mut self, kind: InstKind) -> Option<InstKind> {
        Some(match kind {
            InstKind::Add(a, b) => InstKind::Add(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Sub(a, b) => InstKind::Sub(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Mul(a, b) => InstKind::Mul(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Div(a, b) => InstKind::Div(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::SDiv(a, b) => InstKind::SDiv(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Mod(a, b) => InstKind::Mod(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::SMod(a, b) => InstKind::SMod(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Exp(a, b) => InstKind::Exp(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::AddMod(a, b, c) => {
                InstKind::AddMod(self.clone_value(a)?, self.clone_value(b)?, self.clone_value(c)?)
            }
            InstKind::MulMod(a, b, c) => {
                InstKind::MulMod(self.clone_value(a)?, self.clone_value(b)?, self.clone_value(c)?)
            }
            InstKind::And(a, b) => InstKind::And(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Or(a, b) => InstKind::Or(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Xor(a, b) => InstKind::Xor(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Not(a) => InstKind::Not(self.clone_value(a)?),
            InstKind::Shl(a, b) => InstKind::Shl(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Shr(a, b) => InstKind::Shr(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Sar(a, b) => InstKind::Sar(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Byte(a, b) => InstKind::Byte(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Lt(a, b) => InstKind::Lt(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Gt(a, b) => InstKind::Gt(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::SLt(a, b) => InstKind::SLt(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::SGt(a, b) => InstKind::SGt(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Eq(a, b) => InstKind::Eq(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::IsZero(a) => InstKind::IsZero(self.clone_value(a)?),
            InstKind::MLoad(a) => InstKind::MLoad(self.clone_value(a)?),
            InstKind::MStore(a, b) => InstKind::MStore(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::MStore8(a, b) => {
                InstKind::MStore8(self.clone_value(a)?, self.clone_value(b)?)
            }
            InstKind::MSize => InstKind::MSize,
            InstKind::MCopy(a, b, c) => {
                InstKind::MCopy(self.clone_value(a)?, self.clone_value(b)?, self.clone_value(c)?)
            }
            InstKind::SLoad(a) => InstKind::SLoad(self.clone_value(a)?),
            InstKind::SStore(a, b) => InstKind::SStore(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::TLoad(a) => InstKind::TLoad(self.clone_value(a)?),
            InstKind::TStore(a, b) => InstKind::TStore(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::CalldataLoad(a) => InstKind::CalldataLoad(self.clone_value(a)?),
            InstKind::CalldataCopy(a, b, c) => InstKind::CalldataCopy(
                self.clone_value(a)?,
                self.clone_value(b)?,
                self.clone_value(c)?,
            ),
            InstKind::CalldataSize => InstKind::CalldataSize,
            InstKind::InternalFrameAddr(offset) => {
                InstKind::InternalFrameAddr(self.frame_base + offset)
            }
            InstKind::CodeSize => InstKind::CodeSize,
            InstKind::LoadImmutable(offset) => InstKind::LoadImmutable(offset),
            InstKind::CodeCopy(a, b, c) => {
                InstKind::CodeCopy(self.clone_value(a)?, self.clone_value(b)?, self.clone_value(c)?)
            }
            InstKind::ExtCodeSize(a) => InstKind::ExtCodeSize(self.clone_value(a)?),
            InstKind::ExtCodeCopy(a, b, c, d) => InstKind::ExtCodeCopy(
                self.clone_value(a)?,
                self.clone_value(b)?,
                self.clone_value(c)?,
                self.clone_value(d)?,
            ),
            InstKind::ExtCodeHash(a) => InstKind::ExtCodeHash(self.clone_value(a)?),
            InstKind::ReturnDataSize => InstKind::ReturnDataSize,
            InstKind::ReturnDataCopy(a, b, c) => InstKind::ReturnDataCopy(
                self.clone_value(a)?,
                self.clone_value(b)?,
                self.clone_value(c)?,
            ),
            InstKind::Caller => InstKind::Caller,
            InstKind::CallValue => InstKind::CallValue,
            InstKind::Origin => InstKind::Origin,
            InstKind::GasPrice => InstKind::GasPrice,
            InstKind::BlockHash(a) => InstKind::BlockHash(self.clone_value(a)?),
            InstKind::Coinbase => InstKind::Coinbase,
            InstKind::Timestamp => InstKind::Timestamp,
            InstKind::BlockNumber => InstKind::BlockNumber,
            InstKind::PrevRandao => InstKind::PrevRandao,
            InstKind::GasLimit => InstKind::GasLimit,
            InstKind::ChainId => InstKind::ChainId,
            InstKind::Address => InstKind::Address,
            InstKind::Balance(a) => InstKind::Balance(self.clone_value(a)?),
            InstKind::SelfBalance => InstKind::SelfBalance,
            InstKind::Gas => InstKind::Gas,
            InstKind::BaseFee => InstKind::BaseFee,
            InstKind::BlobBaseFee => InstKind::BlobBaseFee,
            InstKind::BlobHash(a) => InstKind::BlobHash(self.clone_value(a)?),
            InstKind::Keccak256(a, b) => {
                InstKind::Keccak256(self.clone_value(a)?, self.clone_value(b)?)
            }
            InstKind::MappingSlot(a, b) => {
                InstKind::MappingSlot(self.clone_value(a)?, self.clone_value(b)?)
            }
            InstKind::MappingSlotMemory(a, b) => {
                InstKind::MappingSlotMemory(self.clone_value(a)?, self.clone_value(b)?)
            }
            InstKind::MappingSlotCalldata(a, b) => {
                InstKind::MappingSlotCalldata(self.clone_value(a)?, self.clone_value(b)?)
            }
            InstKind::Call { gas, addr, value, args_offset, args_size, ret_offset, ret_size } => {
                InstKind::Call {
                    gas: self.clone_value(gas)?,
                    addr: self.clone_value(addr)?,
                    value: self.clone_value(value)?,
                    args_offset: self.clone_value(args_offset)?,
                    args_size: self.clone_value(args_size)?,
                    ret_offset: self.clone_value(ret_offset)?,
                    ret_size: self.clone_value(ret_size)?,
                }
            }
            InstKind::StaticCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                InstKind::StaticCall {
                    gas: self.clone_value(gas)?,
                    addr: self.clone_value(addr)?,
                    args_offset: self.clone_value(args_offset)?,
                    args_size: self.clone_value(args_size)?,
                    ret_offset: self.clone_value(ret_offset)?,
                    ret_size: self.clone_value(ret_size)?,
                }
            }
            InstKind::DelegateCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                InstKind::DelegateCall {
                    gas: self.clone_value(gas)?,
                    addr: self.clone_value(addr)?,
                    args_offset: self.clone_value(args_offset)?,
                    args_size: self.clone_value(args_size)?,
                    ret_offset: self.clone_value(ret_offset)?,
                    ret_size: self.clone_value(ret_size)?,
                }
            }
            InstKind::InternalCall { function, args, returns } => InstKind::InternalCall {
                function,
                args: args
                    .into_iter()
                    .map(|arg| self.clone_value(arg))
                    .collect::<Option<Vec<_>>>()?
                    .into(),
                returns,
            },
            InstKind::Create(a, b, c) => {
                InstKind::Create(self.clone_value(a)?, self.clone_value(b)?, self.clone_value(c)?)
            }
            InstKind::Create2(a, b, c, d) => InstKind::Create2(
                self.clone_value(a)?,
                self.clone_value(b)?,
                self.clone_value(c)?,
                self.clone_value(d)?,
            ),
            InstKind::Log0(a, b) => InstKind::Log0(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Log1(a, b, c) => {
                InstKind::Log1(self.clone_value(a)?, self.clone_value(b)?, self.clone_value(c)?)
            }
            InstKind::Log2(a, b, c, d) => InstKind::Log2(
                self.clone_value(a)?,
                self.clone_value(b)?,
                self.clone_value(c)?,
                self.clone_value(d)?,
            ),
            InstKind::Log3(a, b, c, d, e) => InstKind::Log3(
                self.clone_value(a)?,
                self.clone_value(b)?,
                self.clone_value(c)?,
                self.clone_value(d)?,
                self.clone_value(e)?,
            ),
            InstKind::Log4(a, b, c, d, e, f) => InstKind::Log4(
                self.clone_value(a)?,
                self.clone_value(b)?,
                self.clone_value(c)?,
                self.clone_value(d)?,
                self.clone_value(e)?,
                self.clone_value(f)?,
            ),
            InstKind::Phi(_) => return None,
            InstKind::Select(a, b, c) => {
                InstKind::Select(self.clone_value(a)?, self.clone_value(b)?, self.clone_value(c)?)
            }
            InstKind::SignExtend(a, b) => {
                InstKind::SignExtend(self.clone_value(a)?, self.clone_value(b)?)
            }
        })
    }

    fn clone_terminator(
        &mut self,
        term: &Terminator,
        cloned_block: BlockId,
        continuation: BlockId,
    ) -> Option<Terminator> {
        Some(match term {
            Terminator::Jump(target) => Terminator::Jump(self.clone_block(*target)?),
            Terminator::Branch { condition, then_block, else_block } => Terminator::Branch {
                condition: self.clone_value(*condition)?,
                then_block: self.clone_block(*then_block)?,
                else_block: self.clone_block(*else_block)?,
            },
            Terminator::Switch { value, default, cases } => Terminator::Switch {
                value: self.clone_value(*value)?,
                default: self.clone_block(*default)?,
                cases: cases
                    .iter()
                    .map(|(value, block)| {
                        Some((self.clone_value(*value)?, self.clone_block(*block)?))
                    })
                    .collect::<Option<Vec<_>>>()?,
            },
            Terminator::Return { values } => {
                let mapped = values
                    .iter()
                    .map(|value| self.clone_value(*value))
                    .collect::<Option<SmallVec<[ValueId; 2]>>>()?;
                self.return_edges.push((cloned_block, mapped));
                Terminator::Jump(continuation)
            }
            // A void callee's `Stop` is an internal return with no values.
            Terminator::Stop if self.callee.returns.is_empty() => {
                self.return_edges.push((cloned_block, SmallVec::new()));
                Terminator::Jump(continuation)
            }
            Terminator::Revert { offset, size } => Terminator::Revert {
                offset: self.clone_value(*offset)?,
                size: self.clone_value(*size)?,
            },
            Terminator::ReturnData { .. }
            | Terminator::Stop
            | Terminator::SelfDestruct { .. }
            | Terminator::TailCall { .. } => {
                return None;
            }
            Terminator::Invalid => Terminator::Invalid,
        })
    }
}

fn build_return_values(
    caller: &mut Function,
    continuation: BlockId,
    return_tys: &[MirType],
    return_edges: &[(BlockId, SmallVec<[ValueId; 2]>)],
) -> Option<Vec<ValueId>> {
    let mut values = Vec::with_capacity(return_tys.len());
    for (index, &ty) in return_tys.iter().enumerate() {
        let incoming = return_edges
            .iter()
            .map(|(block, edge_values)| Some((*block, *edge_values.get(index)?)))
            .collect::<Option<Vec<_>>>()?;
        let phi = caller.alloc_inst(Instruction::new(InstKind::Phi(incoming), Some(ty)));
        caller.blocks[continuation].instructions.insert(index, phi);
        values.push(caller.alloc_value(Value::Inst(phi)));
    }
    Some(values)
}

fn insert_extra_return_stores(caller: &mut Function, continuation: BlockId, values: &[ValueId]) {
    if values.is_empty() {
        return;
    }

    // Insert the stores right after the continuation block's leading phis.
    let phi_count = caller.blocks[continuation]
        .instructions
        .iter()
        .take_while(|&&inst_id| matches!(caller.instructions[inst_id].kind, InstKind::Phi(_)))
        .count();

    for (index, &value) in values.iter().enumerate() {
        let offset = caller
            .alloc_value(Value::Immediate(Immediate::uint256(U256::from((index as u64 + 1) * 32))));
        let store = caller.alloc_inst(Instruction::new(InstKind::MStore(offset, value), None));
        caller.blocks[continuation].instructions.insert(phi_count + index, store);
    }
}

fn redirect_phi_predecessors(
    func: &mut Function,
    successors: &[BlockId],
    old_pred: BlockId,
    new_pred: BlockId,
) {
    if successors.is_empty() {
        return;
    }

    for &succ in successors {
        for &inst_id in &func.blocks[succ].instructions {
            if let InstKind::Phi(incoming) = &mut func.instructions[inst_id].kind {
                for (pred, _) in incoming {
                    if *pred == old_pred {
                        *pred = new_pred;
                    }
                }
            }
        }
    }
}

fn recompute_cfg(func: &mut Function) {
    let mut edges = Vec::new();
    for (block, bb) in func.blocks.iter_enumerated() {
        if let Some(term) = &bb.terminator {
            edges.push((block, term.successors()));
        }
    }

    for block in func.blocks.iter_mut() {
        block.predecessors.clear();
    }

    for (block, successors) in edges {
        for succ in successors {
            func.blocks[succ].predecessors.push(block);
        }
    }
}

fn prune_phi_incoming_to_predecessors(func: &mut Function) {
    for block_id in func.blocks.indices() {
        let predecessors = func.blocks[block_id].predecessors.clone();
        for &inst_id in &func.blocks[block_id].instructions {
            if let InstKind::Phi(incoming) = &mut func.instructions[inst_id].kind {
                incoming.retain(|(pred, _)| predecessors.contains(pred));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::FunctionBuilder;
    use solar_interface::Ident;
    use solar_sema::hir::Visibility;

    #[test]
    fn inlines_loop_return_through_continuation_phi() {
        let mut module = Module::new(Ident::DUMMY);

        let mut callee = Function::new(Ident::DUMMY);
        {
            let mut builder = FunctionBuilder::new(&mut callee);
            let limit = builder.add_param(MirType::uint256());
            builder.add_return(MirType::uint256());

            let header = builder.create_block();
            let body = builder.create_block();
            let exit = builder.create_block();

            let frame = builder.internal_frame_addr(0);
            let zero = builder.imm_u64(0);
            builder.mstore(frame, zero);
            builder.jump(header);

            builder.switch_to_block(header);
            let frame = builder.internal_frame_addr(0);
            let current = builder.mload(frame);
            let cond = builder.lt(current, limit);
            builder.branch(cond, body, exit);

            builder.switch_to_block(body);
            let frame = builder.internal_frame_addr(0);
            let current = builder.mload(frame);
            let one = builder.imm_u64(1);
            let next = builder.add(current, one);
            let frame = builder.internal_frame_addr(0);
            builder.mstore(frame, next);
            builder.jump(header);

            builder.switch_to_block(exit);
            let frame = builder.internal_frame_addr(0);
            let result = builder.mload(frame);
            builder.ret([result]);
        }
        callee.internal_frame_size = 32;
        let callee_id = module.add_function(callee);

        let mut caller = Function::new(Ident::DUMMY);
        caller.attributes.visibility = Visibility::Public;
        {
            let mut builder = FunctionBuilder::new(&mut caller);
            let limit = builder.imm_u64(4);
            let value = builder.internal_call(callee_id, vec![limit], MirType::uint256(), 1);
            let one = builder.imm_u64(1);
            let result = builder.add(value, one);
            builder.ret([result]);
        }
        let caller_id = module.add_function(caller);

        let mut inliner = MirInliner::default();
        let stats = inliner.run(&mut module);
        assert_eq!(stats.inlined, 1);

        let caller = module.function(caller_id);
        assert!(caller.blocks.iter().all(|block| {
            block.instructions.iter().all(|&inst| {
                !matches!(caller.instructions[inst].kind, InstKind::InternalCall { .. })
            })
        }));
        assert!(caller.blocks.iter().any(|block| {
            block
                .instructions
                .iter()
                .any(|&inst| matches!(caller.instructions[inst].kind, InstKind::Phi(_)))
        }));
    }
}
