//! The walk of the HIR that builds the coverage map.
//!
//! A decision is a tree of leaf conditions under `&&`, `||`, and `!`. The walk numbers the tree in
//! source order and assigns three kinds of counters:
//! - `V`, after each leaf condition is evaluated, before the branch on it;
//! - `R`, at the start of the right operand of each `&&` and `||`, which counts how often the left
//!   operand sends evaluation on;
//! - `T`, at the start of the path the decision takes when it holds, or after its value when no
//!   branch consumes it.
//!
//! Those counters give every leaf its true and false counts without a counter on a false edge. The
//! evaluations of a subtree complete `V(a) - R + V(b)` times for `a && b` and `a || b`, as the
//! evaluations of `a` that do not enter `b` and those of `b`. A leaf's true count comes from the
//! top down: the true count of a root is `T`, of the left operand of `&&` is `R`, of the right
//! operand of `&&` the true count of the `&&`, and of the right operand of `||` the true count of
//! the `||` minus that of its left operand, which is the completions of the left operand minus
//! `R`. The false count of a leaf is its completions minus its true count, so a leaf that reverts
//! takes neither side.
//!
//! A constant left operand removes the operator: `true && b` is `b`, and `false && b` is the
//! constant `false`, whose right operand is skipped. A constant decision has no branch, since
//! lowering folds it.

use super::{
    Count, CoverageMap, Decision, DecisionId, Record, RecordOwner, Region, RegionKind, Scope, Site,
    SourceCoverage,
    mcdc::{self, Cond},
};
use solar_ast::{BinOpKind, UnOpKind};
use solar_data_structures::{Never, index::IndexVec, map::FxHashMap};
use solar_interface::{BytePos, Span};
use solar_sema::{
    Gcx,
    builtins::Builtin,
    hir::{self, ExprKind, LoopSource, StmtKind, Visit},
};
use std::{cmp::Reverse, mem, ops::ControlFlow};

/// Counters of a dry walk, which never reach lowering, start here.
const VIRTUAL_COUNTER: u32 = 1 << 30;

pub(super) fn walk(gcx: Gcx<'_>) -> CoverageMap {
    let mut functions = gcx
        .hir
        .function_ids()
        .filter(|&id| {
            let function = gcx.hir.function(id);
            function.body.is_some() && !function.is_getter()
        })
        .collect::<Vec<_>>();
    functions.sort_by_key(|&id| {
        let function = gcx.hir.function(id);
        (function.source, function.span.lo())
    });
    let mut derived = FxHashMap::<hir::ContractId, Vec<hir::ContractId>>::default();
    for id in gcx.hir.contract_ids() {
        for &base in gcx.hir.contract(id).linearized_bases {
            derived.entry(base).or_default().push(id);
        }
    }
    let mut walker = Walker {
        gcx,
        sources: IndexVec::from_vec(
            (0..gcx.hir.source_ids().len()).map(|_| SourceCoverage::default()).collect(),
        ),
        sites: FxHashMap::default(),
        next: 1,
        scope: Scope::default(),
        derived,
        next_decision: gcx.sess.opts.unstable.coverage_mcdc.then_some(0),
        dry: false,
        virtual_next: VIRTUAL_COUNTER,
        rec: Rec::default(),
        expansions: FxHashMap::default(),
    };
    for &id in &functions {
        if !gcx.hir.function(id).kind.is_modifier() && walker.function_enabled(id) {
            walker.function(id);
        }
    }
    let mut contracts = gcx.hir.contract_ids().collect::<Vec<_>>();
    contracts.sort_by_key(|&id| gcx.hir.contract(id).span.lo());
    for id in contracts {
        walker.init(id);
    }
    let mut constants = gcx
        .hir
        .variable_ids()
        .filter(|&id| {
            let var = gcx.hir.variable(id);
            var.is_constant() && var.contract.is_none() && var.parent.is_none()
        })
        .collect::<Vec<_>>();
    constants.sort_by_key(|&id| {
        let var = gcx.hir.variable(id);
        (var.source, var.span.lo())
    });
    for group in
        constants.chunk_by(|&a, &b| gcx.hir.variable(a).source == gcx.hir.variable(b).source)
    {
        walker.constants(gcx.hir.variable(group[0]).source, group);
    }
    for &id in &functions {
        if gcx.hir.function(id).kind.is_modifier() && walker.function_enabled(id) {
            walker.modifier(id);
        }
    }
    let mut map = CoverageMap { sources: walker.sources, sites: walker.sites };
    for source in map.sources.iter_mut() {
        source.records.sort_by_key(|record| record.regions.first().map(|region| region.span.lo()));
    }
    map
}

/// A record under construction.
#[derive(Default)]
struct Rec {
    files: Vec<hir::SourceId>,
    /// The index of the file that regions are in.
    file: u32,
    first: u32,
    regions: Vec<Region>,
    decisions: Vec<Decision>,
    /// The count at the start of the statement that is being walked.
    start: Option<Count>,
    /// The counts of the code regions that enclose the current statement.
    enclosing: Vec<Count>,
}

/// The tree of a decision.
enum Node<'gcx> {
    /// A constant, which lowering folds.
    Const(bool),
    /// A leaf condition and the counter after its evaluation.
    Leaf(&'gcx hir::Expr<'gcx>, Count),
    Not(Box<Self>),
    And(Box<Self>, Box<Self>, Operator<'gcx>),
    Or(Box<Self>, Box<Self>, Operator<'gcx>),
}

/// An `&&` or `||` of a decision.
struct Operator<'gcx> {
    expr: &'gcx hir::Expr<'gcx>,
    rhs: &'gcx hir::Expr<'gcx>,
    /// The counter at the start of the right operand.
    r: Count,
}

impl Node<'_> {
    /// Returns how often the evaluation of the tree completed.
    fn completed(&self) -> Count {
        match self {
            Self::Const(_) => Count::Zero,
            Self::Leaf(_, v) => v.clone(),
            Self::Not(node) => node.completed(),
            Self::And(lhs, rhs, op) | Self::Or(lhs, rhs, op) => {
                lhs.completed().sub(op.r.clone()).add(rhs.completed())
            }
        }
    }

    /// Returns the shape of the tree for MC/DC.
    fn cond(&self) -> Cond {
        match self {
            Self::Const(_) => unreachable!("a constant is no condition"),
            Self::Leaf(expr, _) => Cond::Leaf(Site::key(*expr)),
            Self::Not(node) => Cond::Not(Box::new(node.cond())),
            Self::And(lhs, rhs, _) => Cond::And(Box::new(lhs.cond()), Box::new(rhs.cond())),
            Self::Or(lhs, rhs, _) => Cond::Or(Box::new(lhs.cond()), Box::new(rhs.cond())),
        }
    }
}

/// The MC/DC conditions of a decision, by the key of their leaves.
struct Mcdc(FxHashMap<usize, mcdc::Condition>);

/// The entry count, the branch regions and the decisions of one expansion of a modifier.
struct Expansion {
    entry: Count,
    branches: Vec<Region>,
    decisions: Vec<Decision>,
    /// The index of the first of its decisions in the record of the invoking function.
    first_decision: u32,
}

/// What a decision leaves for its consumer.
enum Outcome {
    /// Lowering folds the condition to this constant.
    Folded(bool),
    /// How often the evaluation of the condition completed, and how often it held.
    Branch { completed: Count, taken: Count },
}

struct Walker<'gcx> {
    gcx: Gcx<'gcx>,
    sources: IndexVec<hir::SourceId, SourceCoverage>,
    sites: FxHashMap<(Scope, Site), u32>,
    next: u32,
    /// The modifier expansion that is being walked, keyed by its invocation, or `0`.
    scope: Scope,
    /// The contracts that inherit each contract, itself included.
    derived: FxHashMap<hir::ContractId, Vec<hir::ContractId>>,
    /// The ID of the next MC/DC decision, or `None` without `-Zcoverage-mcdc`.
    next_decision: Option<u32>,
    /// Whether to build the regions without counters, to learn their shape.
    dry: bool,
    virtual_next: u32,
    rec: Rec,
    /// The regions of each expansion of each modifier.
    expansions: FxHashMap<hir::FunctionId, Vec<Expansion>>,
}

impl<'gcx> Walker<'gcx> {
    fn counter(&mut self, site: Site) -> Count {
        if self.dry {
            self.virtual_next += 1;
            return Count::Counter(self.virtual_next);
        }
        let id = self.next;
        self.next += 1;
        let previous = self.sites.insert((self.scope, site), id);
        debug_assert!(previous.is_none(), "coverage site {site:?} has two counters");
        Count::Counter(id)
    }

    /// Returns whether the innermost `@custom:solar-coverage` tag on `item` or its `contract`
    /// leaves coverage on, which no tag does too.
    fn enabled(&self, item: hir::ItemId, contract: Option<hir::ContractId>) -> bool {
        self.gcx
            .solar_tags(item)
            .coverage
            .or_else(|| {
                contract.and_then(|id| self.gcx.solar_tags(hir::ItemId::Contract(id)).coverage)
            })
            .unwrap_or(true)
    }

    /// Returns whether coverage is on for a function, a modifier, or a Yul function, which
    /// follows the function it is written in.
    fn function_enabled(&self, id: hir::FunctionId) -> bool {
        let function = self.gcx.hir.function(id);
        if function.is_yul
            && let Some(parent) = self.gcx.hir.function_ids().find(|&parent| {
                let parent = self.gcx.hir.function(parent);
                !parent.is_yul
                    && parent.source == function.source
                    && parent.span.contains(function.span)
            })
        {
            return self.function_enabled(parent);
        }
        self.enabled(hir::ItemId::Function(id), function.contract)
    }

    fn begin(&mut self, source: hir::SourceId) {
        self.rec = Rec { files: vec![source], first: self.next, ..Rec::default() };
    }

    fn end(&mut self, owner: RecordOwner, source: hir::SourceId) {
        let rec = mem::take(&mut self.rec);
        let mut regions = rec.regions;
        // A region comes before the regions nested in it.
        regions.sort_by_key(|region| (region.file, region.span.lo(), Reverse(region.span.hi())));
        let counters = [rec.first, self.next - rec.first];
        self.sources[source].records.push(Record {
            owner,
            files: rec.files,
            counters,
            regions,
            decisions: rec.decisions,
        });
    }

    fn region(&mut self, span: Span, kind: RegionKind) {
        let file = self.rec.file;
        self.rec.regions.push(Region { file, span, kind });
    }

    fn skipped(&mut self, span: Span) {
        self.region(span, RegionKind::Skipped);
    }

    /// Fills the gap from `from` to `to` with `count`, at region index `at`, as Clang's
    /// `fillGapAreaWithCount` does for a body that follows its condition.
    ///
    /// The gap is the wrapped segment of the line on which the body starts, so without it a body
    /// on a line of its own would show the count of the enclosing region. A gap on one line, or
    /// with the count of the enclosing region, has no use.
    fn gap_before_body(&mut self, at: usize, from: Option<BytePos>, to: BytePos, count: &Count) {
        let Some(from) = from.filter(|&from| from < to) else { return };
        if self.rec.enclosing.last() == Some(count) {
            return;
        }
        let files = self.gcx.sess.source_map().files();
        let multiline =
            files.span_to_snippet(Span::new(from, to)).is_ok_and(|text| text.contains('\n'));
        drop(files);
        if multiline {
            let file = self.rec.file;
            self.rec.regions.insert(
                at,
                Region { file, span: Span::new(from, to), kind: RegionKind::Gap(count.clone()) },
            );
        }
    }

    /// Returns the position after the `)` that follows `from`, past whitespace and comments, if the
    /// text up to `limit` has one.
    fn after_paren(&self, from: BytePos, limit: BytePos) -> Option<BytePos> {
        let files = self.gcx.sess.source_map().files();
        let text = files.span_to_snippet(Span::new(from, limit)).ok()?;
        let mut rest = text;
        loop {
            rest = rest.trim_start();
            if let Some(comment) = rest.strip_prefix("//") {
                rest = comment.split_once('\n').map_or("", |(_, rest)| rest);
            } else if let Some(comment) = rest.strip_prefix("/*") {
                rest = comment.split_once("*/")?.1;
            } else {
                break;
            }
        }
        rest.starts_with(')').then(|| BytePos(from.0 + (text.len() - rest.len()) as u32 + 1))
    }

    /// Opens a code region unless an enclosing one has the same count.
    fn open(&mut self, span: Span, count: &Count) {
        if self.rec.enclosing.last() != Some(count) {
            self.region(span, RegionKind::Code(count.clone()));
        }
        self.rec.enclosing.push(count.clone());
    }

    fn close(&mut self) {
        self.rec.enclosing.pop();
    }

    fn function(&mut self, id: hir::FunctionId) {
        let function = self.gcx.hir.function(id);
        self.begin(function.source);
        let entry = self.counter(Site::Function(id));
        self.open(function.span, &entry);
        // Modifier and base constructor arguments run before the body.
        for modifier in function.modifiers {
            for arg in modifier.args.exprs() {
                self.expr(arg);
            }
            for target in self.modifier_targets(function, modifier) {
                if self.function_enabled(target) {
                    self.expansion(modifier, target);
                }
            }
        }
        if let Some(body) = function.body {
            // A constructor runs its state variable initializers first.
            let known = (function.modifiers.is_empty()
                && function.kind != hir::FunctionKind::Constructor)
                .then_some(entry);
            self.stmts(body.stmts, known);
            self.body_skips(body.span, !function.is_yul);
        }
        self.close();
        self.end(RecordOwner::Function(id), function.source);
    }

    /// Returns the modifiers that an invocation in `function` dispatches to: the one it names, and
    /// the overrides that derived contracts select.
    fn modifier_targets(
        &self,
        function: &hir::Function<'gcx>,
        modifier: &hir::Modifier<'gcx>,
    ) -> Vec<hir::FunctionId> {
        let mut targets = Vec::new();
        let Some(contract) = function.contract else { return targets };
        for &derived in self.derived.get(&contract).map_or(&[][..], Vec::as_slice) {
            if let Some(target) = self.gcx.resolve_modifier_target(derived, modifier)
                && self.gcx.hir.function(target).kind.is_modifier()
                && !targets.contains(&target)
            {
                targets.push(target);
            }
        }
        targets
    }

    /// Walks the body of `modifier_id` as invoked by `invocation`, in a virtual file of its own.
    fn expansion(&mut self, invocation: &'gcx hir::Modifier<'gcx>, modifier_id: hir::FunctionId) {
        let modifier = self.gcx.hir.function(modifier_id);
        let Some(body) = modifier.body else { return };
        let file = self.rec.files.len() as u32;
        self.rec.files.push(modifier.source);
        let scope =
            mem::replace(&mut self.scope, (Site::key(invocation), modifier_id.index() as u32));
        let entry = self.counter(Site::Function(modifier_id));
        self.region(invocation.span, RegionKind::Expansion(entry.clone(), file));
        let previous_file = mem::replace(&mut self.rec.file, file);
        let enclosing = mem::take(&mut self.rec.enclosing);
        let start = self.rec.regions.len();
        let first_decision = self.rec.decisions.len();
        self.open(modifier.span, &entry);
        self.stmts(body.stmts, Some(entry.clone()));
        self.body_skips(body.span, true);
        self.close();
        // The branches and decisions of the body live in the record of the modifier only, because
        // `llvm-cov` sums the branches of a record and of its expansions.
        let mut regions = self.rec.regions.split_off(start);
        let branches = regions.iter().filter(|region| region.kind.is_branch()).cloned().collect();
        regions.retain(|region| !region.kind.is_branch());
        self.rec.regions.extend(regions);
        let expansion = Expansion {
            entry,
            branches,
            decisions: self.rec.decisions.drain(first_decision..).collect(),
            first_decision: first_decision as u32,
        };
        self.expansions.entry(modifier_id).or_default().push(expansion);
        self.rec.enclosing = enclosing;
        self.rec.file = previous_file;
        self.scope = scope;
    }

    /// Builds the record of a modifier.
    ///
    /// The expansions carry the code regions of the body, and `llvm-cov` adds up the code regions
    /// that share a span, so a modifier that something invokes has one code region, over its
    /// header, which no expansion covers, counted as often as its expansions are entered. The
    /// branches live only in this record, because `llvm-cov` sums the branches of a record and of
    /// its expansions: the branch regions of the body, counted over all expansions, and its
    /// decisions, which list the IDs they have in each expansion. A modifier that nothing invokes
    /// keeps the regions of its body, counted zero.
    fn modifier(&mut self, id: hir::FunctionId) {
        let modifier = self.gcx.hir.function(id);
        let Some(body) = modifier.body else { return };
        self.begin(modifier.source);
        match self.expansions.remove(&id) {
            Some(expansions) => {
                let mut count = Count::Zero;
                let mut branches = Vec::<Region>::new();
                let mut decisions = Vec::<Decision>::new();
                for (index, expansion) in expansions.into_iter().enumerate() {
                    let Expansion { entry, branches: regions, decisions: expanded, first_decision } =
                        expansion;
                    count = count.add(entry);
                    // The expansions share one structure, so their branches line up.
                    for (position, mut region) in regions.into_iter().enumerate() {
                        if index == 0 {
                            region.file = 0;
                            if let RegionKind::Decision(decision) = &mut region.kind {
                                *decision -= first_decision;
                            }
                            branches.push(region);
                        } else if let Some(Region { kind: sum, .. }) = branches.get_mut(position)
                            && let (
                                RegionKind::Branch(holds, fails)
                                | RegionKind::McdcBranch { holds, fails, .. },
                                RegionKind::Branch(other_holds, other_fails)
                                | RegionKind::McdcBranch {
                                    holds: other_holds,
                                    fails: other_fails,
                                    ..
                                },
                            ) = (sum, region.kind)
                        {
                            *holds = mem::replace(holds, Count::Zero).add(other_holds);
                            *fails = mem::replace(fails, Count::Zero).add(other_fails);
                        }
                    }
                    for (position, decision) in expanded.into_iter().enumerate() {
                        let DecisionId::Probe(id) = decision.id else { continue };
                        match decisions.get_mut(position) {
                            Some(Decision { id: DecisionId::Expansions(ids), .. }) => ids.push(id),
                            _ => decisions.push(Decision {
                                id: DecisionId::Expansions(vec![id]),
                                conditions: decision.conditions,
                                vectors: decision.vectors,
                            }),
                        }
                    }
                }
                self.region(modifier.span.with_hi(body.span.lo()), RegionKind::Code(count));
                self.rec.regions.extend(branches);
                self.rec.decisions = decisions;
            }
            None => {
                self.dry = true;
                let entry = self.counter(Site::Function(id));
                self.open(modifier.span, &entry);
                self.stmts(body.stmts, Some(entry));
                self.body_skips(body.span, true);
                self.close();
                self.dry = false;
                // Nothing counts the body, so no code takes the count of a gap.
                self.rec.regions.retain(|region| !matches!(region.kind, RegionKind::Gap(_)));
                for region in &mut self.rec.regions {
                    match &mut region.kind {
                        RegionKind::Code(count) => *count = Count::Zero,
                        RegionKind::Branch(holds, fails) => {
                            *holds = Count::Zero;
                            *fails = Count::Zero;
                        }
                        _ => {}
                    }
                }
            }
        }
        self.end(RecordOwner::Function(id), modifier.source);
    }

    /// Walks the code of `id` that runs outside its functions: the initializers of its state
    /// variables, except constants, which lowering folds or inlines at each use, and the base
    /// constructor arguments of its inheritance specifiers.
    fn init(&mut self, id: hir::ContractId) {
        let contract = self.gcx.hir.contract(id);
        if !self.enabled(hir::ItemId::Contract(id), None) {
            return;
        }
        self.begin(contract.source);
        for base in contract.bases_args {
            if self.has_regions(|this| {
                for arg in base.args.exprs() {
                    this.expr(arg);
                }
            }) {
                for arg in base.args.exprs() {
                    self.expr(arg);
                }
            }
        }
        for var in contract.variables() {
            self.initializer(var);
        }
        if self.rec.regions.is_empty() {
            self.rec = Rec::default();
        } else {
            self.end(RecordOwner::Init(id), contract.source);
        }
    }

    /// Walks the initializers of the constants of `source` that belong to no contract.
    fn constants(&mut self, source: hir::SourceId, constants: &[hir::VariableId]) {
        self.begin(source);
        for &var in constants {
            self.initializer(var);
        }
        if self.rec.regions.is_empty() {
            self.rec = Rec::default();
        } else {
            self.end(RecordOwner::Constants(source), source);
        }
    }

    /// Walks the initializer of `id` when it runs: for a state variable at creation, and for a
    /// constant each time the code uses it, unless lowering folds it to a value.
    fn initializer(&mut self, id: hir::VariableId) {
        let var = self.gcx.hir.variable(id);
        let Some(initializer) = var.initializer else { return };
        let runs = if var.is_constant() {
            self.gcx.const_eval().eval_var(id).is_err()
        } else {
            var.is_state_variable()
        };
        if runs && self.has_regions(|this| this.expr(initializer)) {
            let count = self.counter(Site::Init(Site::key(var)));
            self.open(var.span, &count);
            self.expr(initializer);
            self.close();
        }
    }

    /// Returns whether `f` adds regions, without leaving any regions or counters behind.
    fn has_regions(&mut self, f: impl FnOnce(&mut Self)) -> bool {
        let len = self.rec.regions.len();
        let dry = mem::replace(&mut self.dry, true);
        f(self);
        self.dry = dry;
        let found = self.rec.regions.len() > len;
        self.rec.regions.truncate(len);
        found
    }

    fn stmt_count(&mut self, stmt: &'gcx hir::Stmt<'gcx>, known: Option<Count>) -> Count {
        let count = known.unwrap_or_else(|| self.counter(Site::Stmt(Site::key(stmt))));
        self.rec.start.get_or_insert_with(|| count.clone());
        count
    }

    /// Walks statements, the first of which starts with count `known`, when the code before it
    /// counted.
    ///
    /// The code between two statements, which a region of the enclosing statement would count,
    /// counts as the statement that follows when its count differs, as Clang's gap regions do, so
    /// that a line that starts after a statement that may revert has the count of the statement
    /// that starts on it.
    fn stmts(&mut self, stmts: &'gcx [hir::Stmt<'gcx>], mut known: Option<Count>) {
        let mut previous_end = None;
        for stmt in stmts {
            let count = self.stmt(stmt, known.take());
            if let Some(count) = count {
                if previous_end.is_none() {
                    self.rec.start.get_or_insert_with(|| count.clone());
                }
                if let Some(end) = previous_end
                    && end < stmt.span.lo()
                    && self.rec.enclosing.last() != Some(&count)
                {
                    self.region(Span::new(end, stmt.span.lo()), RegionKind::Gap(count));
                }
                // A statement without a count, such as an empty block, leaves the gap to the one
                // that follows the last statement that has one.
                previous_end = Some(stmt.span.hi());
            }
        }
    }

    /// Walks a statement and returns its count at its start.
    fn stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>, known: Option<Count>) -> Option<Count> {
        let outer = self.rec.start.take();
        self.stmt_inner(stmt, known);
        mem::replace(&mut self.rec.start, outer)
    }

    /// Adds skipped regions to the body of a function or a modifier: each run of whole lines that
    /// hold only whitespace and comments, and a closing `}` that stands on a line of its own.
    ///
    /// A run covers whole lines, from the first byte of its first line to the newline that ends its
    /// last, so it starts and ends on its own lines: `llvm-cov` maps none of its lines, and the
    /// line before and the line after it keep the counts of the code on them. The body of a Yul
    /// function has only the closing `}`, since the runs of the enclosing body cover its other
    /// lines.
    fn body_skips(&mut self, body: Span, runs: bool) {
        let files = self.gcx.sess.source_map().files();
        let Ok(text) = files.span_to_snippet(body) else { return };
        let lo = body.lo().0;
        let at = |offset: usize| solar_interface::BytePos(lo + offset as u32);
        let mut skips = Vec::new();
        // The first byte of a run and the end of its last line, with its newline.
        let mut run: Option<(usize, usize)> = None;
        let mut block_comment = false;
        let mut offset = 0;
        let mut lines = text.split_inclusive('\n').peekable();
        let mut first = true;
        while let Some(line) = lines.next() {
            let last = lines.peek().is_none();
            let code = has_code(line, &mut block_comment);
            // The lines with the braces of the body hold code.
            if runs && (code || first || last) {
                if let Some((start, end)) = run.take() {
                    skips.push(Span::new(at(start), at(end)));
                }
            } else if runs {
                run = Some((run.map_or(offset, |(start, _)| start), offset + line.len()));
            }
            if last
                && let Some(before) = line.strip_suffix('}')
                && before.trim().is_empty()
            {
                skips.push(Span::new(at(offset + before.len()), at(offset + line.len())));
            }
            first = false;
            offset += line.len();
        }
        drop(files);
        for span in skips {
            self.skipped(span);
        }
    }

    fn stmt_inner(&mut self, stmt: &'gcx hir::Stmt<'gcx>, known: Option<Count>) {
        match stmt.kind {
            StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => {
                // A `for` loop with an initializer desugars to `{ <init>; loop { ... } }`, which
                // ends where the loop does.
                if let [init, loop_stmt] = block.stmts
                    && let StmtKind::Loop(_, LoopSource::For { .. }) = loop_stmt.kind
                    && loop_stmt.span.hi() == stmt.span.hi()
                {
                    let count = self.stmt_count(stmt, known);
                    self.open(stmt.span, &count);
                    self.stmt(init, Some(count));
                    self.loop_body(loop_stmt, stmt.span);
                    self.close();
                } else if let Some(count) = known {
                    self.open(stmt.span, &count);
                    self.stmts(block.stmts, Some(count));
                    self.close();
                } else {
                    self.stmts(block.stmts, None);
                }
            }
            StmtKind::AssemblyBlock(block, _) => {
                let count = self.stmt_count(stmt, known);
                self.open(stmt.span, &count);
                self.stmts(block.stmts, Some(count));
                self.close();
            }
            StmtKind::Loop(..) => {
                let count = self.stmt_count(stmt, known);
                self.open(stmt.span, &count);
                self.loop_body(stmt, stmt.span);
                self.close();
            }
            StmtKind::If(cond, then_stmt, else_stmt) => {
                let count = self.stmt_count(stmt, known);
                self.open(stmt.span, &count);
                match self.decision(cond, Site::Arm(Site::key(stmt))) {
                    Outcome::Branch { completed, taken } => {
                        // if (<cond>) <gap> <then> <gap> else <else>
                        let from = self.after_paren(cond.span.hi(), then_stmt.span.lo());
                        let at = self.rec.regions.len();
                        self.gap_before_body(at, from, then_stmt.span.lo(), &taken);
                        self.stmt(then_stmt, Some(taken.clone()));
                        if let Some(else_stmt) = else_stmt {
                            let remaining = completed.sub(taken);
                            let at = self.rec.regions.len();
                            let from = Some(then_stmt.span.hi());
                            self.gap_before_body(at, from, else_stmt.span.lo(), &remaining);
                            self.stmt(else_stmt, Some(remaining));
                        }
                    }
                    // The condition evaluates nothing, so the path taken starts as the statement
                    // does.
                    Outcome::Folded(holds) => {
                        let (live, dead) = if holds {
                            (Some(then_stmt), else_stmt)
                        } else {
                            (else_stmt, Some(then_stmt))
                        };
                        if let Some(live) = live {
                            self.stmt(live, Some(count));
                        }
                        if let Some(dead) = dead {
                            self.skipped(dead.span);
                        }
                    }
                }
                self.close();
            }
            StmtKind::Switch(switch) => {
                let count = self.stmt_count(stmt, known);
                self.open(stmt.span, &count);
                self.expr(switch.selector);
                let selected = self.counter(Site::Decision(Site::key(switch)));
                let mut cases = Vec::with_capacity(switch.cases.len());
                for case in switch.cases {
                    let taken = self.counter(Site::Arm(Site::key(case)));
                    let remaining = selected.clone().sub(taken.clone());
                    self.region(case.span, RegionKind::Branch(taken.clone(), remaining));
                    cases.push(taken);
                }
                for (case, taken) in switch.cases.iter().zip(cases) {
                    self.open(case.body.span, &taken);
                    self.stmts(case.body.stmts, Some(taken));
                    self.close();
                }
                self.close();
            }
            StmtKind::Try(try_stmt) => {
                let count = self.stmt_count(stmt, known);
                self.open(stmt.span, &count);
                self.expr(&try_stmt.expr);
                // A clause fails the calls that returned, not those that reverted while their
                // arguments were evaluated or before they ran.
                let called = self.counter(Site::Decision(Site::key(try_stmt)));
                let mut clauses = Vec::with_capacity(try_stmt.clauses.len());
                for (index, clause) in try_stmt.clauses.iter().enumerate() {
                    let taken = self.counter(Site::Arm(Site::key(clause)));
                    // The success clause holds when the call returned, which decoding its return
                    // data can still revert; its body counts only the decodes that completed.
                    let holds = if index == 0 {
                        self.counter(Site::Success(Site::key(try_stmt)))
                    } else {
                        taken.clone()
                    };
                    let remaining = called.clone().sub(holds.clone());
                    self.region(clause.span, RegionKind::Branch(holds, remaining));
                    clauses.push(taken);
                }
                for (clause, taken) in try_stmt.clauses.iter().zip(clauses) {
                    self.open(clause.block.span, &taken);
                    self.stmts(clause.block.stmts, Some(taken));
                    self.close();
                }
                self.close();
            }
            StmtKind::DeclSingle(id) => {
                let count = self.stmt_count(stmt, known);
                self.open(stmt.span, &count);
                if let Some(initializer) = self.gcx.hir.variable(id).initializer {
                    self.expr(initializer);
                }
                self.close();
            }
            StmtKind::DeclMulti(_, expr)
            | StmtKind::Expr(expr)
            | StmtKind::Emit(expr)
            | StmtKind::Revert(expr) => {
                let count = self.stmt_count(stmt, known);
                self.open(stmt.span, &count);
                self.expr(expr);
                self.close();
            }
            StmtKind::Return(expr) => {
                let count = self.stmt_count(stmt, known);
                self.open(stmt.span, &count);
                if let Some(expr) = expr {
                    self.expr(expr);
                }
                self.close();
            }
            StmtKind::Break | StmtKind::Continue | StmtKind::Placeholder => {
                let count = self.stmt_count(stmt, known);
                self.open(stmt.span, &count);
                self.close();
            }
            StmtKind::Err(_) => {}
        }
    }

    /// Walks the body of a desugared loop whose statement spans `span`.
    fn loop_body(&mut self, loop_stmt: &'gcx hir::Stmt<'gcx>, span: Span) {
        let StmtKind::Loop(block, source) = loop_stmt.kind else { unreachable!() };
        match (source, block.stmts) {
            // loop { if (<cond>) <body> else break; }
            (LoopSource::While | LoopSource::For { .. }, [check])
                if let StmtKind::If(cond, body, Some(exit)) = check.kind
                    && matches!(exit.kind, StmtKind::Break)
                    && (exit.span == span || exit.span == cond.span) =>
            {
                // The `)` that ends the header follows the condition or the update of a `for`.
                let header = match source {
                    LoopSource::For { update: Some(update) } => {
                        update.span.hi().max(cond.span.hi())
                    }
                    _ => cond.span.hi(),
                };
                let from = self.after_paren(header, body.span.lo());
                match self.decision(cond, Site::Arm(Site::key(check))) {
                    Outcome::Branch { taken, .. } => {
                        // while (<cond>) <gap> <body>
                        let at = self.rec.regions.len();
                        self.gap_before_body(at, from, body.span.lo(), &taken);
                        self.stmt(body, Some(taken));
                    }
                    // The body still runs once for each iteration.
                    Outcome::Folded(true) => {
                        let taken = self.counter(Site::Arm(Site::key(check)));
                        let at = self.rec.regions.len();
                        self.gap_before_body(at, from, body.span.lo(), &taken);
                        self.stmt(body, Some(taken));
                    }
                    Outcome::Folded(false) => self.skipped(body.span),
                }
            }
            // loop { <body>; if (<cond>) continue else break; }
            (LoopSource::DoWhile, [body, check]) if let StmtKind::If(cond, _, _) = check.kind => {
                // do <gap> <body> while (<cond>)
                let at = self.rec.regions.len();
                if let Some(count) = self.stmt(body, None) {
                    let lo = loop_stmt.span.lo();
                    let files = self.gcx.sess.source_map().files();
                    let is_do = files
                        .span_to_snippet(Span::new(lo, BytePos(lo.0 + 2)))
                        .is_ok_and(|text| text == "do");
                    drop(files);
                    let from = is_do.then_some(BytePos(lo.0 + 2));
                    self.gap_before_body(at, from, body.span.lo(), &count);
                }
                let _ = self.decision(cond, Site::Arm(Site::key(check)));
            }
            (_, stmts) => self.stmts(stmts, None),
        }
        if let LoopSource::For { update: Some(update) } = source {
            self.stmt(update, None);
        }
    }

    fn expr(&mut self, expr: &'gcx hir::Expr<'gcx>) {
        let _ = Visit::visit_expr(self, expr);
    }

    /// Returns the constant a boolean expression folds to.
    fn constant(&self, expr: &hir::Expr<'_>) -> Option<bool> {
        self.gcx
            .type_of_expr(expr.id)
            .filter(|ty| ty.is_value_type())
            .and_then(|_| self.gcx.const_eval().eval_expr(expr).ok())
            .and_then(|value| value.as_bool())
    }

    /// Builds the tree of the decision `expr`. A constant is a [`Node::Const`] only where `fold`:
    /// a constant right operand stays a leaf, since lowering still branches before it.
    fn tree(&mut self, expr: &'gcx hir::Expr<'gcx>, fold: bool) -> Node<'gcx> {
        let expr = expr.peel_parens();
        if let Some(value) = self.constant(expr) {
            return if fold { Node::Const(value) } else { self.leaf(expr) };
        }
        match expr.kind {
            ExprKind::Unary(op, inner)
                if op.kind == UnOpKind::Not && self.is_logical(inner.peel_parens()) =>
            {
                match self.tree(inner, fold) {
                    Node::Const(value) => Node::Const(!value),
                    node => Node::Not(Box::new(node)),
                }
            }
            ExprKind::Binary(lhs, op, rhs) if op.kind.is_logical() => {
                let is_and = op.kind == BinOpKind::And;
                match self.tree(lhs, true) {
                    // `true && rhs` is `rhs`, and `false || rhs` is `rhs`.
                    Node::Const(value) if value == is_and => self.tree(rhs, fold),
                    // `false && rhs` and `true || rhs` skip `rhs`.
                    Node::Const(value) => {
                        self.skipped(rhs.span);
                        Node::Const(value)
                    }
                    lhs => {
                        let r = self.counter(Site::Arm(Site::expr(rhs)));
                        let operator = Operator { expr, rhs, r };
                        let rhs = match self.tree(rhs, false) {
                            // A right operand that folds because it skips an operand stays a leaf.
                            Node::Const(_) => self.leaf(rhs.peel_parens()),
                            rhs => rhs,
                        };
                        let (lhs, rhs) = (Box::new(lhs), Box::new(rhs));
                        if is_and {
                            Node::And(lhs, rhs, operator)
                        } else {
                            Node::Or(lhs, rhs, operator)
                        }
                    }
                }
            }
            _ => self.leaf(expr),
        }
    }

    fn leaf(&mut self, expr: &'gcx hir::Expr<'gcx>) -> Node<'gcx> {
        Node::Leaf(expr, self.counter(Site::Leaf(Site::key(expr))))
    }

    fn is_logical(&self, expr: &hir::Expr<'_>) -> bool {
        match expr.kind {
            ExprKind::Binary(_, op, _) => op.kind.is_logical(),
            ExprKind::Unary(op, inner) => {
                op.kind == UnOpKind::Not && self.is_logical(inner.peel_parens())
            }
            _ => false,
        }
    }

    /// Walks the decision `root`, whose consumer counts the paths it takes at `consumer`.
    fn decision(&mut self, root: &'gcx hir::Expr<'gcx>, consumer: Site) -> Outcome {
        let tree = self.tree(root, true);
        if let Node::Const(value) = tree {
            return Outcome::Folded(value);
        }
        let taken = self.counter(consumer);
        let completed = tree.completed();
        let mcdc = self.mcdc(&tree);
        self.branches(&tree, taken.clone(), mcdc.as_ref());
        Outcome::Branch { completed, taken }
    }

    /// Gives the decision of `tree` MC/DC probes and a region, when it has two or more conditions
    /// and few enough test vectors.
    ///
    /// The decision ends where its value is known, which a leading `!` does not change, so it
    /// starts at the first `&&` or `||`.
    fn mcdc(&mut self, tree: &Node<'gcx>) -> Option<Mcdc> {
        let id = self.next_decision.filter(|_| !self.dry)?;
        let mut root = tree;
        while let Node::Not(node) = root {
            root = node;
        }
        let (Node::And(_, _, op) | Node::Or(_, _, op)) = root else { return None };
        let Some(graph) = mcdc::graph(&root.cond()) else {
            self.gcx
                .dcx()
                .note("decision has too many test vectors for MC/DC, so it only has branches")
                .span(op.expr.span)
                .emit();
            return None;
        };
        if id >= 1 << 21 {
            return None;
        }
        self.next_decision = Some(id + 1);
        // COVERAGE; PUSH4 1<operation:2><decision:21><weight:8>
        let probe = |operation: u32, weight: u32| 1 << 31 | operation << 29 | id << 8 | weight;
        let key = Site::expr(op.expr);
        self.sites.insert((self.scope, Site::McdcBegin(key)), probe(0, 0));
        self.sites.insert((self.scope, Site::McdcEnd(key)), probe(2, 0));
        let mut conditions = FxHashMap::default();
        for (leaf, condition) in graph.iter() {
            if condition.weight > 0 {
                self.sites.insert((self.scope, Site::McdcAdd(leaf)), probe(1, condition.weight));
            }
            conditions.insert(leaf, condition);
        }
        let index = self.rec.decisions.len() as u32;
        self.rec.decisions.push(Decision {
            id: DecisionId::Probe(id),
            conditions: graph.conditions() as u32,
            vectors: graph.vectors,
        });
        self.region(op.expr.span, RegionKind::Decision(index));
        Some(Mcdc(conditions))
    }

    /// Adds the branch regions of the leaves of `node`, which holds `holds` times.
    fn branches(&mut self, node: &Node<'gcx>, holds: Count, mcdc: Option<&Mcdc>) {
        match node {
            Node::Const(_) => {}
            Node::Leaf(expr, completed) => {
                let fails = completed.clone().sub(holds.clone());
                let kind = match mcdc.and_then(|mcdc| mcdc.0.get(&Site::key(*expr))) {
                    Some(condition) => RegionKind::McdcBranch {
                        holds,
                        fails,
                        condition: condition.id,
                        false_next: condition.false_next,
                        true_next: condition.true_next,
                    },
                    None => RegionKind::Branch(holds, fails),
                };
                self.region(expr.span, kind);
                self.expr(expr);
            }
            Node::Not(node) => self.branches(node, node.completed().sub(holds), mcdc),
            Node::And(lhs, rhs, op) => {
                self.branches(lhs, op.r.clone(), mcdc);
                self.region(op.rhs.span, RegionKind::Code(op.r.clone()));
                self.branches(rhs, holds, mcdc);
            }
            Node::Or(lhs, rhs, op) => {
                let lhs_holds = lhs.completed().sub(op.r.clone());
                self.branches(lhs, lhs_holds.clone(), mcdc);
                self.region(op.rhs.span, RegionKind::Code(op.r.clone()));
                self.branches(rhs, holds.sub(lhs_holds), mcdc);
            }
        }
    }

    fn ternary(
        &mut self,
        cond: &'gcx hir::Expr<'gcx>,
        then_expr: &'gcx hir::Expr<'gcx>,
        else_expr: &'gcx hir::Expr<'gcx>,
    ) {
        match self.decision(cond, Site::Arm(Site::expr(then_expr))) {
            Outcome::Branch { completed, taken } => {
                self.region(then_expr.span, RegionKind::Code(taken.clone()));
                self.region(else_expr.span, RegionKind::Code(completed.sub(taken)));
                self.expr(then_expr);
                self.expr(else_expr);
            }
            // The path taken counts for itself, as a constant condition has no count to share.
            Outcome::Folded(holds) => {
                let (live, dead) =
                    if holds { (then_expr, else_expr) } else { (else_expr, then_expr) };
                let count = self.counter(Site::Arm(Site::expr(live)));
                self.region(live.span, RegionKind::Code(count));
                self.skipped(dead.span);
                self.expr(live);
            }
        }
    }
}

impl<'gcx> Visit<'gcx> for Walker<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        // Lowering folds constant value expressions, so their decisions never run.
        if matches!(expr.kind, ExprKind::Binary(..) | ExprKind::Ternary(..) | ExprKind::Call(..))
            && self.gcx.type_of_expr(expr.id).is_some_and(|ty| ty.is_value_type())
            && self.gcx.const_eval().eval_expr(expr).is_ok()
        {
            return ControlFlow::Continue(());
        }
        match expr.kind {
            ExprKind::Ternary(cond, then_expr, else_expr) => {
                self.ternary(cond, then_expr, else_expr);
                ControlFlow::Continue(())
            }
            ExprKind::Binary(_, op, _) if op.kind.is_logical() => {
                let _ = self.decision(expr, Site::Value(Site::expr(expr)));
                ControlFlow::Continue(())
            }
            ExprKind::Call(callee, ref args)
                if self.gcx.resolved_builtin(callee) == Some(Builtin::Require)
                    && let Some(condition) = args.exprs().next() =>
            {
                self.visit_expr(callee)?;
                for arg in args.exprs().skip(1) {
                    self.visit_expr(arg)?;
                }
                let _ = self.decision(condition, Site::Arm(Site::expr(condition)));
                ControlFlow::Continue(())
            }
            _ => self.walk_expr(expr),
        }
    }
}

/// Returns whether `line` holds code, which is anything but whitespace and comments. A block
/// comment that does not end on the line continues on the next, which `block_comment` tracks. The
/// scan runs to the end of the line, past code, to find the comment that the line opens.
fn has_code(line: &str, block_comment: &mut bool) -> bool {
    let bytes = line.as_bytes();
    let mut code = false;
    let mut index = 0;
    while index < bytes.len() {
        if *block_comment {
            match line[index..].find("*/") {
                Some(end) => {
                    index += end + 2;
                    *block_comment = false;
                }
                None => return code,
            }
            continue;
        }
        match bytes[index] {
            b if b.is_ascii_whitespace() => index += 1,
            b'/' if bytes.get(index + 1) == Some(&b'/') => return code,
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                *block_comment = true;
                index += 2;
            }
            quote @ (b'"' | b'\'') => {
                code = true;
                index += 1;
                while index < bytes.len() && bytes[index] != quote {
                    index += if bytes[index] == b'\\' { 2 } else { 1 };
                }
                index += 1;
            }
            _ => {
                code = true;
                index += 1;
            }
        }
    }
    code
}
