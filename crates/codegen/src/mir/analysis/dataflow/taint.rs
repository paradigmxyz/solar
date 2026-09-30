//! Data-dependence (taint) analysis with storage-path-sensitive sources.
//!
//! Each SSA value is mapped to the set of [`Source`]s it may depend on through data flow:
//! arguments, environment reads such as `caller` or `timestamp`, values loaded from a
//! storage path, immutables, and results of external calls. The forward analysis runs on the
//! generic worklist engine; values are SSA and never change once defined, while memory is a
//! flow-sensitive, weakly updated summary of everything stored to memory so far.
//!
//! Loads depend on their slot computation, so a mapping entry read with `msg.sender` depends
//! on `caller` as well as on the entry's path. Internal calls instantiate the callee's
//! summary, whose return taint is expressed in terms of its formal parameters: two callers
//! of a shared identity function keep their own argument dependencies (crytic/slither#1742).
//! With `k > 0` the actual argument taint becomes the callee's context instead.
//!
//! A summary also records which taint flows into each written storage path. Module-level
//! clients close storage sources over those writes, with field- and index-sensitive paths,
//! so a timestamp stored in one struct field does not reach an array length
//! (crytic/slither#1436).

use super::{
    engine::{self, Analysis, ProgramPoint},
    interproc::{CallSite, Context, InterproceduralAnalysis, SummaryEngine},
    lattice::{JoinSemiLattice, MapLattice},
    storage::{FunctionStorage, StorageAnalysis},
    storage_path::{PathId, PathTable},
};
use crate::mir::{
    ArgIdx, BlockId, Callee, EffectKind, Function, FunctionId, ImmutableId, InstId, InstKind,
    Module, Terminator, Value, ValueId,
    analysis::{AddressSpace, AliasAnalysis, CfgInfo},
};
use smallvec::SmallVec;
use solar_data_structures::map::FxHashMap;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    rc::Rc,
};

/// Maximum sources in one set before it widens to [`Source::Unknown`].
const MAX_SOURCES: usize = 16;

/// Something a value may depend on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Source {
    /// A formal parameter of the summarized function.
    Arg(ArgIdx),
    /// A read of the call or block environment, named by its operation.
    Env(&'static str),
    /// A value loaded from a storage path.
    Storage(PathId),
    /// A value loaded from a transient storage path.
    Transient(PathId),
    /// An immutable.
    Immutable(ImmutableId),
    /// The success flag, returndata, or created address of an external call or creation.
    External,
    /// Calldata read directly.
    Calldata,
    /// Too many or unrepresentable sources.
    Unknown,
}

/// A set of sources; the empty set is bottom.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct TaintSet(pub(crate) BTreeSet<Source>);

impl TaintSet {
    fn insert(&mut self, source: Source) -> bool {
        if self.0.contains(&Source::Unknown) {
            return false;
        }
        if self.0.len() >= MAX_SOURCES && !self.0.contains(&source) {
            self.0.clear();
            self.0.insert(Source::Unknown);
            return true;
        }
        self.0.insert(source)
    }

    /// Returns whether the set is empty.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterates over the sources in order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = Source> + '_ {
        self.0.iter().copied()
    }

    /// Displays the set.
    pub(crate) fn display<'a>(
        &'a self,
        table: &'a PathTable,
        func: Option<&'a Function>,
    ) -> impl fmt::Display + 'a {
        fmt::from_fn(move |f| {
            write!(f, "{{")?;
            for (i, source) in self.0.iter().enumerate() {
                if i != 0 {
                    write!(f, ", ")?;
                }
                match *source {
                    Source::Arg(index) => write!(f, "arg{}", index.index())?,
                    Source::Env(name) => write!(f, "{name}")?,
                    Source::Storage(path) => write!(f, "sload({})", table.display(path, func))?,
                    Source::Transient(path) => {
                        write!(f, "tload({})", table.display(path, func))?;
                    }
                    Source::Immutable(id) => write!(f, "immutable{}", id.index())?,
                    Source::External => write!(f, "external")?,
                    Source::Calldata => write!(f, "calldata")?,
                    Source::Unknown => write!(f, "?")?,
                }
            }
            write!(f, "}}")
        })
    }
}

impl JoinSemiLattice for TaintSet {
    fn join(&mut self, other: &Self) -> bool {
        let mut changed = false;
        for &source in &other.0 {
            changed |= self.insert(source);
        }
        changed
    }
}

/// Entry abstraction: the taint of each argument.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TaintEntry(pub(crate) Box<[TaintSet]>);

/// A function's taint summary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TaintSummary {
    /// Taint of each returned component.
    pub(crate) returns: SmallVec<[TaintSet; 1]>,
    /// Taint flowing into each written storage path.
    pub(crate) stores: BTreeMap<PathId, TaintSet>,
}

impl JoinSemiLattice for TaintSummary {
    fn join(&mut self, other: &Self) -> bool {
        let mut changed = false;
        if self.returns.len() < other.returns.len() {
            self.returns.resize(other.returns.len(), TaintSet::default());
            changed = true;
        }
        for (mine, theirs) in self.returns.iter_mut().zip(&other.returns) {
            changed |= mine.join(theirs);
        }
        for (&path, taint) in &other.stores {
            match self.stores.get_mut(&path) {
                Some(existing) => changed |= existing.join(taint),
                None => {
                    self.stores.insert(path, taint.clone());
                    changed = true;
                }
            }
        }
        changed
    }
}

/// Per-function results kept for dumps.
#[derive(Clone, Debug, Default)]
pub(crate) struct FunctionTaint {
    /// Taint of every value with a nonempty set.
    pub(crate) values: FxHashMap<ValueId, TaintSet>,
}

/// The interprocedural taint analysis.
pub(crate) struct TaintAnalysis<'m> {
    /// Storage paths used to name storage sources.
    pub(crate) storage: SummaryEngine<'m, StorageAnalysis>,
    /// Per-function results, keyed like summaries.
    pub(crate) functions: FxHashMap<(FunctionId, Context<TaintEntry>), Rc<FunctionTaint>>,
}

impl<'m> TaintAnalysis<'m> {
    /// Creates the analysis for `module`.
    pub(crate) fn new(module: &'m Module) -> Self {
        Self {
            storage: SummaryEngine::new(
                module,
                StorageAnalysis::new(module),
                super::interproc::ContextPolicy::INSENSITIVE,
            ),
            functions: FxHashMap::default(),
        }
    }

    /// Returns the storage results of `func` in its general context.
    pub(crate) fn storage_of(&mut self, func: FunctionId) -> Rc<FunctionStorage> {
        let context = self.storage.general_context(func);
        let _ = self.storage.summary(func, &context);
        self.storage.analysis.functions.get(&(func, context)).cloned().unwrap_or_default()
    }
}

impl InterproceduralAnalysis for TaintAnalysis<'_> {
    type Entry = TaintEntry;
    type Summary = TaintSummary;

    fn general_entry(&self, module: &Module, func: FunctionId) -> TaintEntry {
        let function = module.function(func);
        TaintEntry(
            (0..function.params.len())
                .map(|i| {
                    let mut set = TaintSet::default();
                    set.insert(Source::Arg(ArgIdx::from_usize(i)));
                    set
                })
                .collect(),
        )
    }

    fn bottom_summary(&self, _module: &Module, _func: FunctionId) -> TaintSummary {
        TaintSummary::default()
    }

    fn unknown_summary(&self, _module: &Module, _func: FunctionId) -> TaintSummary {
        let mut unknown = TaintSet::default();
        unknown.insert(Source::Unknown);
        let mut stores = BTreeMap::new();
        stores.insert(PathTable::UNKNOWN, unknown.clone());
        TaintSummary { returns: smallvec::smallvec![unknown], stores }
    }

    fn summarize(
        engine: &mut SummaryEngine<'_, Self>,
        func: FunctionId,
        context: &Context<TaintEntry>,
    ) -> TaintSummary {
        let module = engine.module;
        let function = module.function(func);
        let storage = engine.analysis.storage_of(func);
        let cfg = CfgInfo::new(function);
        let alias = AliasAnalysis::new(function);
        let mut analysis = TaintTransfer {
            engine,
            func_id: func,
            context,
            storage: &storage,
            alias: &alias,
            summary: TaintSummary::default(),
            recording: false,
        };
        let results = engine::solve(function, &cfg, &mut analysis);
        let mut values = FxHashMap::default();
        for &block in cfg.rpo() {
            if results.is_visited(block) {
                let exit = engine::block_exit_state(function, &mut analysis, &results, block);
                for (&value, taint) in &exit.0.0 {
                    values.entry(value).or_insert_with(TaintSet::default).join(taint);
                }
            }
        }
        // Record summary facts once, over the fixed point.
        analysis.recording = true;
        engine::replay(function, &cfg, &mut analysis, &results, |analysis, point, state| {
            if let ProgramPoint::Terminator(block) = point {
                analysis.record_terminator(function, block, state);
            }
        });
        let mut summary = analysis.summary;
        let table = &mut engine.analysis.storage.analysis.table;
        summary.stores = summary
            .stores
            .into_iter()
            .map(|(path, mut taint)| {
                taint.0 =
                    taint.0.into_iter().map(|source| generalize(table, source, func)).collect();
                (table.generalize(path, func), taint)
            })
            .collect();
        for taint in &mut summary.returns {
            taint.0 = std::mem::take(&mut taint.0)
                .into_iter()
                .map(|source| generalize(table, source, func))
                .collect();
        }
        engine
            .analysis
            .functions
            .insert((func, context.clone()), Rc::new(FunctionTaint { values }));
        summary
    }
}

fn generalize(table: &mut PathTable, source: Source, func: FunctionId) -> Source {
    match source {
        Source::Storage(path) => Source::Storage(table.generalize(path, func)),
        Source::Transient(path) => Source::Transient(table.generalize(path, func)),
        source => source,
    }
}

/// The taint state: taint per SSA value, plus everything stored to memory so far.
type TaintState = (MapLattice<ValueId, TaintSet>, TaintSet);

struct TaintTransfer<'a, 'e, 'm> {
    engine: &'a mut SummaryEngine<'e, TaintAnalysis<'m>>,
    func_id: FunctionId,
    context: &'a Context<TaintEntry>,
    storage: &'a FunctionStorage,
    alias: &'a AliasAnalysis,
    summary: TaintSummary,
    recording: bool,
}

impl TaintTransfer<'_, '_, '_> {
    fn value_taint(&self, func: &Function, state: &TaintState, value: ValueId) -> TaintSet {
        match func.value(value) {
            Value::Arg(index) => {
                self.context.entry.0.get(index.index()).cloned().unwrap_or_default()
            }
            Value::Inst(_) => state.0.0.get(&value).cloned().unwrap_or_default(),
            Value::Immediate(_) | Value::Undef(_) | Value::Error(_) => TaintSet::default(),
        }
    }

    fn record_terminator(&mut self, func: &Function, block: BlockId, state: &TaintState) {
        if let Some(Terminator::Return { values }) = &func.blocks[block].terminator {
            let components = return_components(func, values)
                .into_iter()
                .map(|value| match value {
                    Some(value) => self.value_taint(func, state, value),
                    None => {
                        let mut unknown = TaintSet::default();
                        unknown.insert(Source::Unknown);
                        unknown
                    }
                })
                .collect::<SmallVec<[_; 1]>>();
            if self.summary.returns.len() < components.len() {
                self.summary.returns.resize(components.len(), TaintSet::default());
            }
            for (slot, taint) in self.summary.returns.iter_mut().zip(components) {
                slot.join(&taint);
            }
        }
    }
}

/// Returns the value of each returned component, following `insert_value` chains.
fn return_components(func: &Function, values: &[ValueId]) -> SmallVec<[Option<ValueId>; 1]> {
    if let &[value] = values
        && let Value::Inst(inst) = func.value(value)
        && matches!(func.inst(*inst).kind, InstKind::InsertValue { .. })
    {
        let mut components = SmallVec::<[Option<ValueId>; 1]>::new();
        let mut current = value;
        while let Value::Inst(inst) = func.value(current)
            && let InstKind::InsertValue { aggregate, index, value, .. } = func.inst(*inst).kind
        {
            let index = index as usize;
            if components.len() <= index {
                components.resize(index + 1, None);
            }
            if components[index].is_none() {
                components[index] = Some(value);
            }
            current = aggregate;
        }
        return components;
    }
    values.iter().map(|&value| Some(value)).collect()
}

impl Analysis for TaintTransfer<'_, '_, '_> {
    type Domain = TaintState;

    fn bottom(&self, _func: &Function) -> TaintState {
        (MapLattice::default(), TaintSet::default())
    }

    fn initialize_boundary(&mut self, _func: &Function, _block: BlockId, _state: &mut TaintState) {}

    fn apply_instruction(
        &mut self,
        func: &Function,
        block: BlockId,
        inst: InstId,
        state: &mut TaintState,
    ) {
        let kind = &func.inst(inst).kind;
        let mut taint = TaintSet::default();
        for operand in kind.operands() {
            taint.join(&self.value_taint(func, state, operand));
        }
        let accesses = self.storage.accesses.get(&inst);
        match kind {
            InstKind::ICall { function: Callee::Function(callee), args } => {
                let entry = TaintEntry(
                    args.iter().map(|&arg| self.value_taint(func, state, arg)).collect(),
                );
                let site = CallSite { caller: self.func_id, inst: Some(inst), block };
                let context = self.engine.callee_context(self.context, site, *callee, entry);
                // Specific contexts already carry the actual taint; general ones need it
                // substituted for the formal parameters.
                let general = context.call_string.is_empty();
                let summary = self.engine.summary(*callee, &context);
                let instantiate = |source_set: &TaintSet| {
                    let mut out = TaintSet::default();
                    for source in source_set.iter() {
                        match source {
                            Source::Arg(index) if general => {
                                out.join(
                                    &args
                                        .get(index.index())
                                        .map(|&arg| self.value_taint(func, state, arg))
                                        .unwrap_or_default(),
                                );
                            }
                            source => {
                                out.insert(source);
                            }
                        }
                    }
                    out
                };
                taint = summary.returns.first().map(instantiate).unwrap_or_default();
                if self.recording
                    && let Some(accesses) = accesses
                {
                    let mut stored = TaintSet::default();
                    for store in summary.stores.values() {
                        stored.join(&instantiate(store));
                    }
                    for &path in &accesses.writes {
                        self.summary.stores.entry(path).or_default().join(&stored);
                    }
                }
            }
            &InstKind::SLoad(_) | &InstKind::TLoad(_) => {
                let transient = matches!(kind, InstKind::TLoad(_));
                for path in accesses
                    .into_iter()
                    .flat_map(|a| if transient { a.transient_reads.iter() } else { a.reads.iter() })
                {
                    taint.insert(if transient {
                        Source::Transient(*path)
                    } else {
                        Source::Storage(*path)
                    });
                }
            }
            &InstKind::SStore(_, value) | &InstKind::TStore(_, value) => {
                if self.recording
                    && let Some(accesses) = accesses
                {
                    let value = self.value_taint(func, state, value);
                    for &path in accesses.writes.iter().chain(&accesses.transient_writes) {
                        self.summary.stores.entry(path).or_default().join(&value);
                    }
                }
            }
            &InstKind::LoadImmutable(id) => {
                taint.insert(Source::Immutable(id));
            }
            &InstKind::CalldataLoad(_) | &InstKind::CalldataCopy(..) => {
                taint.insert(Source::Calldata);
            }
            _ => {
                match kind.effect_kind() {
                    EffectKind::ExternalCall | EffectKind::Create => {
                        taint.insert(Source::External);
                    }
                    EffectKind::EnvironmentRead if kind.operands().is_empty() => {
                        taint.insert(Source::Env(kind.mnemonic()));
                    }
                    _ => {}
                }
                if let Some(accesses) = accesses {
                    for &path in &accesses.reads {
                        taint.insert(Source::Storage(path));
                    }
                }
            }
        }
        let effects = self.alias.instruction_mod_ref(func, inst);
        if effects.reads_space(AddressSpace::Memory) {
            taint.join(&state.1);
        }
        if effects.writes_space(AddressSpace::Memory) {
            let mut stored = TaintSet::default();
            for operand in kind.operands() {
                stored.join(&self.value_taint(func, state, operand));
            }
            state.1.join(&stored);
        }
        if let Some(result) = func.inst_result_value(inst)
            && !taint.is_empty()
        {
            state.0.0.insert(result, taint);
        }
    }

    fn apply_phi(
        &mut self,
        func: &Function,
        phi: InstId,
        incoming: ValueId,
        _edge: &engine::Edge,
        source: &TaintState,
        state: &mut TaintState,
    ) {
        let taint = self.value_taint(func, source, incoming);
        if let Some(result) = func.inst_result_value(phi)
            && !taint.is_empty()
        {
            state.0.0.entry(result).or_default().join(&taint);
        }
    }
}

/// Computes taint summaries for every bodied function of `module`.
pub(crate) fn analyze_module(
    module: &Module,
    policy: super::interproc::ContextPolicy,
) -> SummaryEngine<'_, TaintAnalysis<'_>> {
    let mut engine = SummaryEngine::new(module, TaintAnalysis::new(module), policy);
    for (func, function) in module.iter_functions() {
        if !function.blocks.is_empty() {
            let context = engine.general_context(func);
            let _ = engine.summary(func, &context);
        }
    }
    engine
}

/// Writes per-instruction taint facts and summaries for `module`.
pub(crate) fn dump(engine: &SummaryEngine<'_, TaintAnalysis<'_>>, out: &mut String) {
    use std::fmt::Write as _;
    let module = engine.module;
    let table = &engine.analysis.storage.analysis.table;
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
                let _ = write!(out, "{}", arg.display(table, None));
            }
            let _ = write!(out, ")");
        }
        let _ = writeln!(out, ":");
        let cfg = CfgInfo::new(function);
        for &block in cfg.rpo() {
            let mut labeled = false;
            for &inst in &function.blocks[block].instructions {
                let Some(taint) =
                    function.inst_result_value(inst).and_then(|value| results.values.get(&value))
                else {
                    continue;
                };
                if !labeled {
                    let _ = writeln!(out, "  bb{}:", block.index());
                    labeled = true;
                }
                let _ = writeln!(
                    out,
                    "    {}  ; taint={}",
                    crate::mir::display::display_instruction(function, Some(module), inst),
                    taint.display(table, Some(function))
                );
            }
        }
        let _ = write!(out, "  summary:");
        for (i, taint) in summary.returns.iter().enumerate() {
            let _ = write!(out, " ret{i}={}", taint.display(table, None));
        }
        for (&path, taint) in &summary.stores {
            let _ = write!(out, " {}<-{}", table.display(path, None), taint.display(table, None));
        }
        let _ = writeln!(out);
    }
}
