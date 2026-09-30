//! Plug-in value domains and the generic analysis that runs them.
//!
//! A [`ValueDomain`] abstracts one SSA value: an interval, a rounding direction, or a
//! physical unit. It supplies constants, transfer functions over the schema-generated
//! [`Op`] view, branch refinement, and optional seeds from annotations. [`ValueAnalysis`]
//! turns any such domain into an interprocedural, path-sensitive analysis:
//!
//! - values are mapped in a [`MapLattice`] carried through the worklist engine, and phis join their
//!   incoming values per edge;
//! - branch conditions and passing checks refine the operands of the comparison that produced them,
//!   and a comparison refined to the empty value makes the edge unreachable;
//! - internal calls use the callee's summary, the join of its returned values, computed for the
//!   calling context. With `k = 0` each function has one summary over its seeded or unknown
//!   arguments; with `k > 0` the argument values become the entry abstraction, which is how
//!   non-relational domains gain precision from their calling contexts.
//!
//! Domains record findings through [`ValueDomain::check`] while the analysis replays the
//! fixed point, so each finding is reported once per instruction.

use super::{
    engine::{self, Analysis, Edge, EdgeCondition},
    interproc::{CallSite, Context, ContextPolicy, InterproceduralAnalysis, SummaryEngine},
    lattice::{JoinSemiLattice, MapLattice, Reachable},
};
use crate::mir::{
    ArgIdx, BlockId, Builtin, Callee, Function, FunctionId, InstId, InstKind, Module, Op,
    Terminator, Value, ValueId, analysis::CfgInfo,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::map::FxHashMap;
use std::{fmt, hash::Hash, rc::Rc};

/// An abstract domain for single SSA values.
pub(crate) trait ValueDomain:
    JoinSemiLattice + Eq + Hash + fmt::Debug + fmt::Display
{
    /// Name selecting the domain with `-Zdataflow`.
    const NAME: &'static str;

    /// Returns the value about which nothing is known.
    fn top() -> Self;

    /// Returns the abstraction of a constant word.
    fn constant(value: U256) -> Self;

    /// Returns the abstraction of an instruction's result from its operands' abstractions,
    /// given in the order of `op`'s value operands.
    fn transfer(cx: &DomainCx<'_>, op: Op, operands: &[Self]) -> Self;

    /// Narrows the operands of comparison `op`, known to evaluate to `taken`.
    ///
    /// Returns `false` when no operand values satisfy the outcome.
    fn refine(_op: Op, _operands: &mut [Self], _taken: bool) -> bool {
        true
    }

    /// Returns the abstraction of a call's result from the callee's name, if it implies one.
    fn call_seed(_callee: &str) -> Option<Self> {
        None
    }

    /// Returns whether the value is empty, so the point holding it cannot execute.
    fn is_empty(&self) -> bool {
        false
    }

    /// Records findings about an instruction and its operands.
    fn check(_cx: &DomainCx<'_>, _op: Op, _operands: &[Self], _findings: &mut Vec<String>) {}
}

/// Context passed to domain transfer functions.
pub(crate) struct DomainCx<'a> {
    /// The module being analyzed.
    pub(crate) module: &'a Module,
    /// The function containing the instruction.
    pub(crate) func: &'a Function,
    /// The instruction being evaluated.
    pub(crate) inst: InstId,
}

impl DomainCx<'_> {
    /// Returns the defining instruction of `value`, if any.
    pub(crate) fn def(&self, value: ValueId) -> Option<&InstKind> {
        match self.func.value(value) {
            Value::Inst(inst) => Some(&self.func.inst(*inst).kind),
            _ => None,
        }
    }
}

/// Annotation seeds: textual facts about parameters and results of functions.
#[derive(Clone, Debug, Default)]
pub(crate) struct Seeds {
    /// Annotation text for each function parameter.
    pub(crate) params: FxHashMap<(FunctionId, ArgIdx), String>,
    /// Annotation text for each function result component.
    pub(crate) returns: FxHashMap<(FunctionId, usize), String>,
}

/// Domains that can read annotation seeds.
pub(crate) trait Seeded: ValueDomain {
    /// Parses one annotation.
    fn parse_seed(_text: &str) -> Option<Self> {
        None
    }
}

/// Entry abstraction: the values of the arguments.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ValueEntry<D>(pub(crate) Box<[D]>);

/// A function's value summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValueSummary<D> {
    /// Joined abstraction of each returned component; `None` until a return is reached.
    pub(crate) returns: SmallVec<[Option<D>; 1]>,
}

impl<D> Default for ValueSummary<D> {
    fn default() -> Self {
        Self { returns: SmallVec::new() }
    }
}

impl<D: ValueDomain> JoinSemiLattice for ValueSummary<D> {
    fn join(&mut self, other: &Self) -> bool {
        let mut changed = false;
        if self.returns.len() < other.returns.len() {
            self.returns.resize(other.returns.len(), None);
            changed = true;
        }
        for (mine, theirs) in self.returns.iter_mut().zip(&other.returns) {
            match (mine.as_mut(), theirs) {
                (_, None) => {}
                (None, Some(theirs)) => {
                    *mine = Some(theirs.clone());
                    changed = true;
                }
                (Some(mine), Some(theirs)) => changed |= mine.join(theirs),
            }
        }
        changed
    }

    fn widen(&mut self, other: &Self) -> bool {
        let mut changed = false;
        if self.returns.len() < other.returns.len() {
            self.returns.resize(other.returns.len(), None);
            changed = true;
        }
        for (mine, theirs) in self.returns.iter_mut().zip(&other.returns) {
            match (mine.as_mut(), theirs) {
                (_, None) => {}
                (None, Some(theirs)) => {
                    *mine = Some(theirs.clone());
                    changed = true;
                }
                (Some(mine), Some(theirs)) => changed |= mine.widen(theirs),
            }
        }
        changed
    }
}

/// Per-function results kept for dumps.
#[derive(Clone, Debug)]
pub(crate) struct FunctionValues<D> {
    /// The abstraction of every value defined by an instruction.
    pub(crate) values: FxHashMap<ValueId, D>,
    /// Findings, by instruction.
    pub(crate) findings: Vec<(InstId, String)>,
}

/// Per-function results keyed by function and calling context.
type ResultsByContext<D> = FxHashMap<(FunctionId, Context<ValueEntry<D>>), Rc<FunctionValues<D>>>;

/// The generic interprocedural value analysis.
pub(crate) struct ValueAnalysis<D: ValueDomain> {
    /// Driver-supplied seeds.
    pub(crate) seeds: Seeds,
    /// Per-function results, keyed like summaries.
    pub(crate) functions: ResultsByContext<D>,
}

impl<D: Seeded> InterproceduralAnalysis for ValueAnalysis<D> {
    type Entry = ValueEntry<D>;
    type Summary = ValueSummary<D>;

    fn general_entry(&self, module: &Module, func: FunctionId) -> ValueEntry<D> {
        let count = module.function(func).params.len();
        ValueEntry(
            (0..count)
                .map(|i| {
                    self.seeds
                        .params
                        .get(&(func, ArgIdx::from_usize(i)))
                        .and_then(|text| D::parse_seed(text))
                        .unwrap_or_else(D::top)
                })
                .collect(),
        )
    }

    fn bottom_summary(&self, _module: &Module, _func: FunctionId) -> ValueSummary<D> {
        ValueSummary::default()
    }

    fn unknown_summary(&self, _module: &Module, _func: FunctionId) -> ValueSummary<D> {
        ValueSummary { returns: smallvec::smallvec![Some(D::top())] }
    }

    fn summarize(
        engine: &mut SummaryEngine<'_, Self>,
        func: FunctionId,
        context: &Context<ValueEntry<D>>,
    ) -> ValueSummary<D> {
        let module = engine.module;
        let function = module.function(func);
        let cfg = CfgInfo::new(function);
        let mut transfer = Transfer { engine, func_id: func, context, findings: None };
        let mut results = engine::solve(function, &cfg, &mut transfer);
        engine::narrow(function, &cfg, &mut transfer, &mut results, 2);
        let mut summary = ValueSummary::default();
        let mut values = FxHashMap::default();
        for &block in cfg.rpo() {
            if !results.is_visited(block) {
                continue;
            }
            let exit = engine::block_exit_state(function, &mut transfer, &results, block);
            let Reachable::State(state) = exit else { continue };
            for (&value, abstraction) in &state.0 {
                values.entry(value).or_insert_with(|| abstraction.clone()).join(abstraction);
            }
            if let Some(Terminator::Return { values: returned }) =
                &function.blocks[block].terminator
            {
                let components = ValueSummary {
                    returns: returned
                        .iter()
                        .map(|&value| Some(transfer.value(function, &state, value)))
                        .collect(),
                };
                summary.join(&components);
            }
        }
        if let Some(seeded) = transfer
            .engine
            .analysis
            .seeds
            .returns
            .get(&(func, 0))
            .and_then(|text| D::parse_seed(text))
        {
            summary.returns = smallvec::smallvec![Some(seeded)];
        }
        // Findings are recorded once, over the fixed point.
        transfer.findings = Some(Vec::new());
        engine::replay(function, &cfg, &mut transfer, &results, |_, _, _| {});
        let findings = transfer.findings.take().unwrap_or_default();
        engine
            .analysis
            .functions
            .insert((func, context.clone()), Rc::new(FunctionValues { values, findings }));
        summary
    }
}

type ValueState<D> = Reachable<MapLattice<ValueId, D>>;

struct Transfer<'a, 'e, D: Seeded> {
    engine: &'a mut SummaryEngine<'e, ValueAnalysis<D>>,
    func_id: FunctionId,
    context: &'a Context<ValueEntry<D>>,
    findings: Option<Vec<(InstId, String)>>,
}

impl<D: Seeded> Transfer<'_, '_, D> {
    fn value(&self, func: &Function, state: &MapLattice<ValueId, D>, value: ValueId) -> D {
        if let Some(known) = state.0.get(&value) {
            return known.clone();
        }
        match func.value(value) {
            Value::Immediate(imm) => imm.as_u256().map_or_else(D::top, D::constant),
            Value::Arg(index) => {
                self.context.entry.0.get(index.index()).cloned().unwrap_or_else(D::top)
            }
            Value::Inst(_) | Value::Undef(_) | Value::Error(_) => D::top(),
        }
    }

    /// Narrows the operands of the comparison defining `condition`.
    fn refine(
        &self,
        func: &Function,
        state: &mut MapLattice<ValueId, D>,
        condition: ValueId,
        taken: bool,
        depth: usize,
    ) -> bool {
        let Value::Inst(inst) = func.value(condition) else { return true };
        let kind = &func.inst(*inst).kind;
        let op = kind.op();
        let operands = kind.operands();
        // eq(x, 0) negates a boolean x.
        if depth < 4
            && let Op::Eq { a, b } = op
            && func.value_u256(b) == Some(U256::ZERO)
            && func.value_ty(a) == Some(crate::mir::MirType::I1)
        {
            return self.refine(func, state, a, !taken, depth + 1);
        }
        let mut values = operands
            .iter()
            .map(|&value| self.value(func, state, value))
            .collect::<SmallVec<[_; 2]>>();
        if !D::refine(op, &mut values, taken) {
            return false;
        }
        for (&operand, refined) in operands.iter().zip(values) {
            if refined.is_empty() {
                return false;
            }
            if matches!(func.value(operand), Value::Inst(_) | Value::Arg(_)) {
                state.0.insert(operand, refined);
            }
        }
        true
    }
}

impl<D: Seeded> Analysis for Transfer<'_, '_, D> {
    type Domain = ValueState<D>;

    fn bottom(&self, _func: &Function) -> Self::Domain {
        Reachable::Unreachable
    }

    fn initialize_boundary(&mut self, _func: &Function, _block: BlockId, state: &mut Self::Domain) {
        *state = Reachable::State(MapLattice::default());
    }

    fn apply_instruction(
        &mut self,
        func: &Function,
        block: BlockId,
        inst: InstId,
        state: &mut Self::Domain,
    ) {
        let Reachable::State(current) = state else { return };
        let kind = &func.inst(inst).kind;
        let operands = kind.operands();
        let values = operands
            .iter()
            .map(|&value| self.value(func, current, value))
            .collect::<SmallVec<[_; 4]>>();
        let cx = DomainCx { module: self.engine.module, func, inst };
        if let Some(findings) = &mut self.findings {
            let mut found = Vec::new();
            D::check(&cx, kind.op(), &values, &mut found);
            findings.extend(found.into_iter().map(|finding| (inst, finding)));
        }
        let result = match kind {
            InstKind::ICall { function: Callee::Function(callee), .. } => {
                let seeded = self
                    .engine
                    .analysis
                    .seeds
                    .returns
                    .get(&(*callee, 0))
                    .and_then(|text| D::parse_seed(text))
                    .or_else(|| {
                        let callee = self.engine.module.function(*callee);
                        D::call_seed(callee.debug_identifier.unwrap_or(callee.name.symbol).as_str())
                    });
                match seeded {
                    Some(seeded) => seeded,
                    None => {
                        let site = CallSite { caller: self.func_id, inst: Some(inst), block };
                        let entry = ValueEntry(values.iter().cloned().collect());
                        let context =
                            self.engine.callee_context(self.context, site, *callee, entry);
                        let summary = self.engine.summary(*callee, &context);
                        summary.returns.first().cloned().flatten().unwrap_or_else(D::top)
                    }
                }
            }
            // A passing check or requirement refines its condition.
            InstKind::ICall { function: Callee::Builtin(Builtin::Check { is_zero, .. }), args } => {
                if let Some(&condition) = args.first()
                    && !self.refine(func, current, condition, *is_zero, 0)
                {
                    *state = Reachable::Unreachable;
                }
                return;
            }
            InstKind::ICall { function: Callee::Builtin(Builtin::Require(_)), args } => {
                if let Some(&condition) = args.first()
                    && !self.refine(func, current, condition, true, 0)
                {
                    *state = Reachable::Unreachable;
                }
                return;
            }
            _ => D::transfer(&cx, kind.op(), &values),
        };
        if let Some(value) = func.inst_result_value(inst) {
            current.0.insert(value, result);
        }
    }

    fn apply_edge(&mut self, func: &Function, edge: &Edge, state: &mut Self::Domain) {
        let Reachable::State(current) = state else { return };
        if let EdgeCondition::Branch { condition, taken } = edge.condition
            && !self.refine(func, current, condition, taken, 0)
        {
            *state = Reachable::Unreachable;
        }
    }

    fn apply_phi(
        &mut self,
        func: &Function,
        phi: InstId,
        incoming: ValueId,
        _edge: &Edge,
        state: &mut Self::Domain,
    ) {
        let Reachable::State(current) = state else { return };
        let value = self.value(func, current, incoming);
        if let Some(result) = func.inst_result_value(phi) {
            current.0.insert(result, value);
        }
    }
}

/// Runs a value domain over every bodied function of `module`.
pub(crate) fn analyze_module<D: Seeded>(
    module: &Module,
    policy: ContextPolicy,
    seeds: Seeds,
) -> SummaryEngine<'_, ValueAnalysis<D>> {
    let analysis = ValueAnalysis { seeds, functions: FxHashMap::default() };
    let mut engine = SummaryEngine::new(module, analysis, policy);
    for (func, function) in module.iter_functions() {
        if !function.blocks.is_empty() {
            let context = engine.general_context(func);
            let _ = engine.summary(func, &context);
        }
    }
    engine
}

/// Writes per-instruction values, summaries, and findings for a value domain.
pub(crate) fn dump<D: Seeded>(engine: &SummaryEngine<'_, ValueAnalysis<D>>, out: &mut String) {
    use std::fmt::Write as _;
    let module = engine.module;
    for (func, context, summary) in engine.final_summaries() {
        let function = module.function(func);
        let Some(results) = engine.analysis.functions.get(&(func, context.clone())) else {
            continue;
        };
        let _ = write!(out, "fn @{}", function.name);
        if !context.call_string.is_empty() {
            let _ = write!(out, " [");
            for (i, site) in context.call_string.iter().enumerate() {
                if i != 0 {
                    let _ = write!(out, " ");
                }
                let _ = write!(out, "@{}", module.function(site.caller).name);
            }
            let _ = write!(out, "](");
            for (i, arg) in context.entry.0.iter().enumerate() {
                if i != 0 {
                    let _ = write!(out, ", ");
                }
                let _ = write!(out, "{arg}");
            }
            let _ = write!(out, ")");
        }
        let _ = writeln!(out, ":");
        let cfg = CfgInfo::new(function);
        for &block in cfg.rpo() {
            let mut labeled = false;
            for &inst in &function.blocks[block].instructions {
                let Some(value) =
                    function.inst_result_value(inst).and_then(|value| results.values.get(&value))
                else {
                    continue;
                };
                if *value == D::top() {
                    continue;
                }
                if !labeled {
                    let _ = writeln!(out, "  bb{}:", block.index());
                    labeled = true;
                }
                let _ = writeln!(
                    out,
                    "    {}  ; {value}",
                    crate::mir::display::display_instruction(function, Some(module), inst)
                );
            }
        }
        let _ = write!(out, "  summary:");
        for (i, value) in summary.returns.iter().enumerate() {
            match value {
                Some(value) => {
                    let _ = write!(out, " ret{i}={value}");
                }
                None => {
                    let _ = write!(out, " ret{i}=never");
                }
            }
        }
        let _ = writeln!(out);
        for (inst, finding) in &results.findings {
            let _ = writeln!(out, "finding: {} @{}: {finding}", D::NAME, function.name);
            let _ = inst;
        }
    }
}
