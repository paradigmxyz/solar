//! Storage alias and footprint analysis over symbolic storage paths.
//!
//! Every SSA value that computes a storage slot is mapped to the [`PathSet`] it may denote.
//! Slot computations are pure, so a value's paths do not depend on the program point and
//! one memoized walk over its definition suffices; phis and selects join their inputs, and
//! loop-carried slot arithmetic widens to the unknown path. Formal parameters start as
//! [`PathNode::Param`](super::storage_path::PathNode::Param) in the most general context,
//! so a callee's summary is relative to its storage-pointer parameters and each caller
//! substitutes its actual paths: `setA(s.x)` writes `slot(0).1` and `L.pick(x, y, c).b`
//! writes `{slot(0).1, slot(4).1}`. Returned storage pointers are summarized the same way.
//! With call-string sensitivity (`k > 0`) the actual paths become the callee's entry
//! abstraction instead, the object-sensitive variant, and the summary is concrete.
//!
//! The summary of a function lists the persistent and transient paths it may read and write,
//! including through callees. Accesses hidden in semantic storage operations come from the
//! shared ModRef effects; byte arrays and dynamic arrays accessed as a whole become a
//! region covering the root slot and everything derived from it. External calls do not
//! access this contract's storage directly: their reentrant effects are the reentrancy
//! analysis' concern. Delegate calls run foreign code in this storage context and write
//! the unknown path.

use super::{
    interproc::{CallSite, Context, InterproceduralAnalysis, SummaryEngine},
    lattice::JoinSemiLattice,
    storage_path::{KeyTerm, PathId, PathNode, PathSet, PathTable},
};
use crate::mir::{
    AddressCallKind, ArgIdx, BlockId, Callee, EffectKind, Function, FunctionId, InstId, InstKind,
    Module, StorageAlias, Terminator, Value, ValueId,
    analysis::{Access, AddressSpace, AliasAnalysis, CfgInfo, Location},
};
use smallvec::SmallVec;
use solar_data_structures::map::{FxHashMap, FxHashSet};
use std::{collections::BTreeSet, fmt, rc::Rc};

/// Entry abstraction: the paths and key terms of each argument.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct StorageEntry {
    /// Paths each argument may denote when used as a storage pointer.
    pub(crate) args: Box<[PathSet]>,
    /// Key term of each argument when used as a mapping key or index.
    pub(crate) keys: Box<[KeyTerm]>,
}

impl fmt::Display for StorageEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} args", self.args.len())
    }
}

/// The storage accesses of one instruction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Accesses {
    /// Persistent paths read.
    pub(crate) reads: SmallVec<[PathId; 2]>,
    /// Persistent paths written.
    pub(crate) writes: SmallVec<[PathId; 2]>,
    /// Transient paths read.
    pub(crate) transient_reads: SmallVec<[PathId; 2]>,
    /// Transient paths written.
    pub(crate) transient_writes: SmallVec<[PathId; 2]>,
}

impl Accesses {
    /// Returns whether the instruction accesses no storage.
    pub(crate) fn is_empty(&self) -> bool {
        self.reads.is_empty()
            && self.writes.is_empty()
            && self.transient_reads.is_empty()
            && self.transient_writes.is_empty()
    }
}

/// Per-function results kept for clients and fact dumps.
#[derive(Clone, Debug, Default)]
pub(crate) struct FunctionStorage {
    /// Storage accesses of each instruction that has any.
    pub(crate) accesses: FxHashMap<InstId, Accesses>,
    /// Actual argument paths and keys of each internal call.
    pub(crate) call_args: FxHashMap<InstId, StorageEntry>,
}

/// A function's storage footprint.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StorageSummary {
    /// Persistent paths possibly read.
    pub(crate) reads: BTreeSet<PathId>,
    /// Persistent paths possibly written.
    pub(crate) writes: BTreeSet<PathId>,
    /// Transient paths possibly read.
    pub(crate) transient_reads: BTreeSet<PathId>,
    /// Transient paths possibly written.
    pub(crate) transient_writes: BTreeSet<PathId>,
    /// Paths of each returned component.
    pub(crate) returns: SmallVec<[PathSet; 1]>,
}

impl JoinSemiLattice for StorageSummary {
    fn join(&mut self, other: &Self) -> bool {
        let before = (
            self.reads.len(),
            self.writes.len(),
            self.transient_reads.len(),
            self.transient_writes.len(),
        );
        self.reads.extend(other.reads.iter().copied());
        self.writes.extend(other.writes.iter().copied());
        self.transient_reads.extend(other.transient_reads.iter().copied());
        self.transient_writes.extend(other.transient_writes.iter().copied());
        let mut changed = before
            != (
                self.reads.len(),
                self.writes.len(),
                self.transient_reads.len(),
                self.transient_writes.len(),
            );
        if self.returns.len() < other.returns.len() {
            self.returns.resize(other.returns.len(), PathSet::default());
            changed = true;
        }
        for (mine, theirs) in self.returns.iter_mut().zip(&other.returns) {
            changed |= mine.join(theirs);
        }
        changed
    }
}

impl StorageSummary {
    /// Displays the summary.
    pub(crate) fn display<'a>(&'a self, table: &'a PathTable) -> impl fmt::Display + 'a {
        fmt::from_fn(move |f| {
            let set = |f: &mut fmt::Formatter<'_>, name: &str, paths: &BTreeSet<PathId>| {
                if paths.is_empty() {
                    return Ok(());
                }
                write!(f, " {name}={{")?;
                for (i, &path) in paths.iter().enumerate() {
                    if i != 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", table.display(path, None))?;
                }
                write!(f, "}}")
            };
            set(f, "reads", &self.reads)?;
            set(f, "writes", &self.writes)?;
            set(f, "treads", &self.transient_reads)?;
            set(f, "twrites", &self.transient_writes)?;
            for (i, paths) in self.returns.iter().enumerate() {
                if !paths.is_empty() && !paths.is_unknown() {
                    write!(f, " ret{i}={}", paths.display(table, None))?;
                }
            }
            Ok(())
        })
    }
}

/// The storage path analysis.
#[derive(Debug, Default)]
pub(crate) struct StorageAnalysis {
    /// Interned paths shared by every function of the module.
    pub(crate) table: PathTable,
    /// Per-function results, keyed like summaries.
    pub(crate) functions: FxHashMap<(FunctionId, Context<StorageEntry>), Rc<FunctionStorage>>,
}

impl InterproceduralAnalysis for StorageAnalysis {
    type Entry = StorageEntry;
    type Summary = StorageSummary;

    fn general_entry(&self, module: &Module, func: FunctionId) -> StorageEntry {
        let function = module.function(func);
        let count = function.params.len();
        // External entries receive words from calldata, never storage pointers.
        let entry = function.is_external_entry();
        // `new` interns every parameter path, so the general entry needs no mutation.
        let args = (0..count)
            .map(|i| {
                let param = PathNode::Param(ArgIdx::from_usize(i));
                match self.table.find(param) {
                    Some(path) if !entry => PathSet::single(path),
                    _ => PathSet::unknown(),
                }
            })
            .collect();
        StorageEntry {
            args,
            keys: (0..count).map(|i| KeyTerm::Arg(ArgIdx::from_usize(i))).collect(),
        }
    }

    fn bottom_summary(&self, _module: &Module, _func: FunctionId) -> StorageSummary {
        StorageSummary::default()
    }

    fn unknown_summary(&self, _module: &Module, _func: FunctionId) -> StorageSummary {
        let unknown = [PathTable::UNKNOWN].into_iter().collect::<BTreeSet<_>>();
        StorageSummary {
            reads: unknown.clone(),
            writes: unknown.clone(),
            transient_reads: unknown.clone(),
            transient_writes: unknown,
            returns: smallvec::smallvec![PathSet::unknown()],
        }
    }

    fn summarize(
        engine: &mut SummaryEngine<'_, Self>,
        func: FunctionId,
        context: &Context<StorageEntry>,
    ) -> StorageSummary {
        let module = engine.module;
        let function = module.function(func);
        let mut cx = FunctionCx {
            func_id: func,
            func: function,
            context,
            values: FxHashMap::default(),
            active: FxHashSet::default(),
            call_args: FxHashMap::default(),
            call_returns: FxHashMap::default(),
        };
        let cfg = CfgInfo::new(function);
        let alias = AliasAnalysis::new(function);
        let mut accesses = FxHashMap::<InstId, Accesses>::default();
        let mut summary = StorageSummary::default();
        for &block in cfg.rpo() {
            for &inst in &function.blocks[block].instructions {
                let access = cx.instruction_accesses(engine, &alias, block, inst);
                if !access.is_empty() {
                    accesses.insert(inst, access);
                }
            }
            if let Some(Terminator::TailCall { function: callee, args }) =
                &function.blocks[block].terminator
            {
                let site = CallSite { caller: func, inst: None, block };
                let (callee_summary, instantiate) = cx.callee_summary(engine, site, *callee, args);
                let access = cx.instantiate_accesses(engine, &callee_summary, args, instantiate);
                merge_accesses(&mut summary, &access);
            }
            if let Some(Terminator::Return { values }) = &function.blocks[block].terminator {
                let components = cx.return_components(engine, values);
                if summary.returns.len() < components.len() {
                    summary.returns.resize(components.len(), PathSet::default());
                }
                for (slot, paths) in summary.returns.iter_mut().zip(components) {
                    slot.join(&paths);
                }
            }
        }
        for access in accesses.values() {
            merge_accesses(&mut summary, access);
        }
        let table = &mut engine.analysis.table;
        let generalize = |table: &mut PathTable, set: &BTreeSet<PathId>| {
            set.iter().map(|&path| table.generalize(path, func)).collect::<BTreeSet<_>>()
        };
        summary.reads = generalize(table, &summary.reads);
        summary.writes = generalize(table, &summary.writes);
        summary.transient_reads = generalize(table, &summary.transient_reads);
        summary.transient_writes = generalize(table, &summary.transient_writes);
        for paths in &mut summary.returns {
            *paths = paths.iter().map(|path| table.generalize(path, func)).collect();
        }
        let results = FunctionStorage { accesses, call_args: cx.call_args };
        engine.analysis.functions.insert((func, context.clone()), Rc::new(results));
        summary
    }
}

impl StorageAnalysis {
    /// Creates the analysis with the parameters of `module` pre-interned.
    pub(crate) fn new(module: &Module) -> Self {
        let mut table = PathTable::default();
        let max_params =
            module.functions.iter().map(|function| function.params.len()).max().unwrap_or(0);
        for i in 0..max_params {
            table.param(ArgIdx::from_usize(i));
        }
        Self { table, functions: FxHashMap::default() }
    }
}

fn merge_accesses(summary: &mut StorageSummary, access: &Accesses) {
    summary.reads.extend(access.reads.iter().copied());
    summary.writes.extend(access.writes.iter().copied());
    summary.transient_reads.extend(access.transient_reads.iter().copied());
    summary.transient_writes.extend(access.transient_writes.iter().copied());
}

struct FunctionCx<'a> {
    func_id: FunctionId,
    func: &'a Function,
    context: &'a Context<StorageEntry>,
    values: FxHashMap<ValueId, PathSet>,
    active: FxHashSet<ValueId>,
    call_args: FxHashMap<InstId, StorageEntry>,
    call_returns: FxHashMap<InstId, SmallVec<[PathSet; 1]>>,
}

impl FunctionCx<'_> {
    /// Returns the key term of `value`.
    fn key(&self, value: ValueId) -> KeyTerm {
        match self.func.value(value) {
            Value::Immediate(imm) => imm.as_u256().map_or(KeyTerm::Any, KeyTerm::Const),
            Value::Arg(index) => {
                self.context.entry.keys.get(index.index()).copied().unwrap_or(KeyTerm::Any)
            }
            Value::Inst(inst) => match self.func.inst(*inst).kind {
                InstKind::Caller => KeyTerm::Caller,
                InstKind::And(x, y) | InstKind::Or(x, y) | InstKind::Xor(x, y)
                    if let (Some(x), Some(y)) =
                        (self.func.value_u256(x), self.func.value_u256(y)) =>
                {
                    let word = match self.func.inst(*inst).kind {
                        InstKind::And(..) => x & y,
                        InstKind::Or(..) => x | y,
                        _ => x ^ y,
                    };
                    KeyTerm::Const(word)
                }
                InstKind::Zext(inner) | InstKind::Trunc(inner, _) | InstKind::Bitcast(inner) => {
                    self.key(inner)
                }
                _ => KeyTerm::Local(self.func_id, value),
            },
            Value::Undef(_) | Value::Error(_) => KeyTerm::Any,
        }
    }

    /// Returns the storage paths `value` may denote.
    fn paths(
        &mut self,
        engine: &mut SummaryEngine<'_, StorageAnalysis>,
        value: ValueId,
    ) -> PathSet {
        if let Some(paths) = self.values.get(&value) {
            return paths.clone();
        }
        if !self.active.insert(value) {
            // A cyclic slot computation, such as a pointer stepped in a loop.
            return PathSet::unknown();
        }
        let paths = self.compute_paths(engine, value);
        self.active.remove(&value);
        self.values.insert(value, paths.clone());
        paths
    }

    fn compute_paths(
        &mut self,
        engine: &mut SummaryEngine<'_, StorageAnalysis>,
        value: ValueId,
    ) -> PathSet {
        let inst = match self.func.value(value) {
            Value::Immediate(imm) => {
                return imm.as_u256().map_or_else(PathSet::unknown, |slot| {
                    PathSet::single(engine.analysis.table.slot(slot))
                });
            }
            Value::Arg(index) => {
                return self
                    .context
                    .entry
                    .args
                    .get(index.index())
                    .cloned()
                    .unwrap_or_else(PathSet::unknown);
            }
            Value::Inst(inst) => *inst,
            Value::Undef(_) | Value::Error(_) => return PathSet::unknown(),
        };
        match &self.func.inst(inst).kind {
            &InstKind::MappingSlot(key, slot)
            | &InstKind::MappingSlotMemory(key, slot)
            | &InstKind::MappingSlotCalldata(key, slot) => {
                let key = self.key(key);
                self.map_paths(engine, slot, |base| PathNode::Mapping { base, key })
            }
            &InstKind::StorageArrayDataSlot(slot) => {
                self.map_paths(engine, slot, |base| PathNode::ArrayData { base })
            }
            &InstKind::StorageArrayElementSlot { slot, index, element_slots } => {
                let index = self.key(index);
                let bases = self.paths(engine, slot);
                let table = &mut engine.analysis.table;
                bases
                    .iter()
                    .map(|base| {
                        let data = table.intern(PathNode::ArrayData { base });
                        table.intern(PathNode::Element { base: data, index, stride: element_slots })
                    })
                    .collect()
            }
            &InstKind::Add(a, b) => self.add_paths(engine, a, b),
            &InstKind::Zext(inner) | &InstKind::Trunc(inner, _) | &InstKind::Bitcast(inner) => {
                self.paths(engine, inner)
            }
            InstKind::Phi(incoming) => {
                let incoming = incoming.clone();
                let mut paths = PathSet::default();
                for (_, input) in incoming {
                    paths.join(&self.paths(engine, input));
                }
                paths
            }
            &InstKind::Select(_, a, b) => {
                let mut paths = self.paths(engine, a);
                paths.join(&self.paths(engine, b));
                paths
            }
            &InstKind::ExtractValue { aggregate, index, .. } => {
                if let Value::Inst(call) = self.func.value(aggregate)
                    && let Some(returns) = self.call_returns.get(call)
                {
                    return returns.get(index as usize).cloned().unwrap_or_else(PathSet::unknown);
                }
                self.insert_value_component(engine, aggregate, index)
            }
            InstKind::ICall { function: Callee::Function(_), .. } => self
                .call_returns
                .get(&inst)
                .and_then(|returns| returns.first().cloned())
                .unwrap_or_else(PathSet::unknown),
            _ => PathSet::unknown(),
        }
    }

    fn map_paths(
        &mut self,
        engine: &mut SummaryEngine<'_, StorageAnalysis>,
        base: ValueId,
        build: impl Fn(PathId) -> PathNode,
    ) -> PathSet {
        let bases = self.paths(engine, base);
        let table = &mut engine.analysis.table;
        bases.iter().map(|base| table.intern(build(base))).collect()
    }

    /// `a + b`: a field offset, an element, or unknown arithmetic.
    fn add_paths(
        &mut self,
        engine: &mut SummaryEngine<'_, StorageAnalysis>,
        a: ValueId,
        b: ValueId,
    ) -> PathSet {
        if let Some(offset) = self.func.value_u256(b) {
            return self.map_paths(engine, a, |base| PathNode::Field { base, offset });
        }
        if let Some(offset) = self.func.value_u256(a) {
            return self.map_paths(engine, b, |base| PathNode::Field { base, offset });
        }
        // base + index * stride, in either operand order.
        for (base, scaled) in [(a, b), (b, a)] {
            if let Some((index, stride)) = self.scaled_index(scaled)
                && self.slot_rank(base) > 0
            {
                let index = self.key(index);
                return self.map_paths(engine, base, |base| PathNode::Element {
                    base,
                    index,
                    stride,
                });
            }
        }
        // base + index: the operand computed like a slot is the base.
        // Two possible pointers, such as `self[key]` over a storage array parameter: source
        // lowering adds the index to the base, so prefer the left operand. Choosing the
        // wrong one stays sound because a non-pointer actual argument instantiates to the
        // unknown path.
        let (base, index) = match self.slot_rank(a).cmp(&self.slot_rank(b)) {
            std::cmp::Ordering::Less => (b, a),
            std::cmp::Ordering::Greater | std::cmp::Ordering::Equal => (a, b),
        };
        if self.slot_rank(base) > 0 {
            let index = self.key(index);
            return self.map_paths(engine, base, |base| PathNode::Element {
                base,
                index,
                stride: 1,
            });
        }
        PathSet::unknown()
    }

    fn scaled_index(&self, value: ValueId) -> Option<(ValueId, u64)> {
        let Value::Inst(inst) = self.func.value(value) else { return None };
        match self.func.inst(*inst).kind {
            InstKind::Mul(x, y) => {
                if let Some(stride) = self.func.value_u64(y) {
                    Some((x, stride))
                } else {
                    self.func.value_u64(x).map(|stride| (y, stride))
                }
            }
            InstKind::Shl(shift, x) => {
                let shift = self.func.value_u64(shift)?;
                (shift < 64).then(|| (x, 1u64 << shift))
            }
            _ => None,
        }
    }

    /// Ranks how likely `value` is a storage location rather than a plain index: `2` for
    /// slot computations, `1` for arguments that may be storage pointers, `0` otherwise.
    fn slot_rank(&self, value: ValueId) -> u8 {
        match self.func.value(value) {
            Value::Arg(index) => u8::from(
                self.context
                    .entry
                    .args
                    .get(index.index())
                    .is_some_and(|set| !set.is_unknown() && !set.is_empty()),
            ),
            Value::Inst(inst) => match self.func.inst(*inst).kind {
                InstKind::MappingSlot(..)
                | InstKind::MappingSlotMemory(..)
                | InstKind::MappingSlotCalldata(..)
                | InstKind::StorageArrayDataSlot(..)
                | InstKind::StorageArrayElementSlot { .. } => 2,
                InstKind::Add(x, y)
                    if self.func.value_u256(x).is_some() || self.func.value_u256(y).is_some() =>
                {
                    let other = if self.func.value_u256(x).is_some() { y } else { x };
                    self.slot_rank(other)
                }
                InstKind::ICall { .. }
                | InstKind::Phi(_)
                | InstKind::Select(..)
                | InstKind::ExtractValue { .. } => {
                    u8::from(self.values.get(&value).is_some_and(|set| !set.is_unknown()))
                }
                _ => 0,
            },
            Value::Immediate(_) | Value::Undef(_) | Value::Error(_) => 0,
        }
    }

    fn insert_value_component(
        &mut self,
        engine: &mut SummaryEngine<'_, StorageAnalysis>,
        aggregate: ValueId,
        index: u32,
    ) -> PathSet {
        let mut current = aggregate;
        loop {
            let Value::Inst(inst) = self.func.value(current) else { return PathSet::unknown() };
            match self.func.inst(*inst).kind {
                InstKind::InsertValue { aggregate, index: inserted, value, .. } => {
                    if inserted == index {
                        return self.paths(engine, value);
                    }
                    current = aggregate;
                }
                _ => return PathSet::unknown(),
            }
        }
    }

    /// Returns the path components of a return terminator's values.
    fn return_components(
        &mut self,
        engine: &mut SummaryEngine<'_, StorageAnalysis>,
        values: &[ValueId],
    ) -> SmallVec<[PathSet; 1]> {
        if let &[value] = values
            && let Value::Inst(inst) = self.func.value(value)
            && matches!(self.func.inst(*inst).kind, InstKind::InsertValue { .. })
        {
            let mut components = SmallVec::<[PathSet; 1]>::new();
            let mut current = value;
            while let Value::Inst(inst) = self.func.value(current)
                && let InstKind::InsertValue { aggregate, index, value, .. } =
                    self.func.inst(*inst).kind
            {
                let index = index as usize;
                if components.len() <= index {
                    components.resize(index + 1, PathSet::default());
                }
                if components[index].is_empty() {
                    components[index] = self.paths(engine, value);
                }
                current = aggregate;
            }
            return components;
        }
        values.iter().map(|&value| self.paths(engine, value)).collect()
    }

    fn callee_summary(
        &mut self,
        engine: &mut SummaryEngine<'_, StorageAnalysis>,
        site: CallSite,
        callee: FunctionId,
        args: &[ValueId],
    ) -> (StorageSummary, bool) {
        let entry = StorageEntry {
            args: args.iter().map(|&arg| self.paths(engine, arg)).collect(),
            keys: args.iter().map(|&arg| self.key(arg)).collect(),
        };
        if let Some(inst) = site.inst {
            self.call_args.insert(inst, entry.clone());
        }
        let context = engine.callee_context(self.context, site, callee, entry);
        // Specific contexts already carry the actual paths; general ones are relative to
        // the callee's formal parameters.
        let general = context.call_string.is_empty();
        (engine.summary(callee, &context), general)
    }

    fn instantiate_accesses(
        &mut self,
        engine: &mut SummaryEngine<'_, StorageAnalysis>,
        summary: &StorageSummary,
        args: &[ValueId],
        instantiate: bool,
    ) -> Accesses {
        let actual_args = args.iter().map(|&arg| self.paths(engine, arg)).collect::<Vec<_>>();
        let actual_keys = args.iter().map(|&arg| self.key(arg)).collect::<Vec<_>>();
        let table = &mut engine.analysis.table;
        let mut map = |paths: &BTreeSet<PathId>| {
            let mut out = SmallVec::new();
            for &path in paths {
                if instantiate {
                    for path in table.instantiate(path, &actual_args, &actual_keys).iter() {
                        if !out.contains(&path) {
                            out.push(path);
                        }
                    }
                } else if !out.contains(&path) {
                    out.push(path);
                }
            }
            out
        };
        Accesses {
            reads: map(&summary.reads),
            writes: map(&summary.writes),
            transient_reads: map(&summary.transient_reads),
            transient_writes: map(&summary.transient_writes),
        }
    }

    fn instruction_accesses(
        &mut self,
        engine: &mut SummaryEngine<'_, StorageAnalysis>,
        alias: &AliasAnalysis,
        block: BlockId,
        inst: InstId,
    ) -> Accesses {
        let mut accesses = Accesses::default();
        let kind = &self.func.inst(inst).kind;
        match kind {
            &InstKind::SLoad(slot) => accesses.reads.extend(self.paths(engine, slot).iter()),
            &InstKind::SStore(slot, _) => accesses.writes.extend(self.paths(engine, slot).iter()),
            &InstKind::TLoad(slot) => {
                accesses.transient_reads.extend(self.paths(engine, slot).iter());
            }
            &InstKind::TStore(slot, _) => {
                accesses.transient_writes.extend(self.paths(engine, slot).iter());
            }
            InstKind::ICall { function: Callee::Function(callee), args } => {
                let (callee, args) = (*callee, args.clone());
                let site = CallSite { caller: self.func_id, inst: Some(inst), block };
                let (summary, instantiate) = self.callee_summary(engine, site, callee, &args);
                let returns = if instantiate {
                    let actual_args =
                        args.iter().map(|&arg| self.paths(engine, arg)).collect::<Vec<_>>();
                    let actual_keys = args.iter().map(|&arg| self.key(arg)).collect::<Vec<_>>();
                    let table = &mut engine.analysis.table;
                    summary
                        .returns
                        .iter()
                        .map(|paths| {
                            let mut result = PathSet::default();
                            for path in paths.iter() {
                                result.join(&table.instantiate(path, &actual_args, &actual_keys));
                            }
                            result
                        })
                        .collect()
                } else {
                    summary.returns.clone()
                };
                self.call_returns.insert(inst, returns);
                accesses = self.instantiate_accesses(engine, &summary, &args, instantiate);
            }
            InstKind::DelegateCall { .. }
            | InstKind::CallCode { .. }
            | InstKind::AddressCall { kind: AddressCallKind::Delegate, .. } => {
                // Foreign code runs against this contract's storage.
                accesses.reads.push(PathTable::UNKNOWN);
                accesses.writes.push(PathTable::UNKNOWN);
                accesses.transient_reads.push(PathTable::UNKNOWN);
                accesses.transient_writes.push(PathTable::UNKNOWN);
            }
            _ if matches!(kind.effect_kind(), EffectKind::ExternalCall | EffectKind::Create) => {}
            _ => {
                let root = storage_region_root(kind);
                let effects = alias.instruction_mod_ref(self.func, inst);
                for (write, list) in [(false, effects.reads()), (true, effects.writes())] {
                    for &access in list {
                        let paths = match access {
                            Access::Location(Location::Storage(alias)) => {
                                self.alias_paths(engine, alias)
                            }
                            Access::Location(Location::Transient(alias)) => {
                                let paths = self.alias_paths(engine, alias);
                                let target = if write {
                                    &mut accesses.transient_writes
                                } else {
                                    &mut accesses.transient_reads
                                };
                                target.extend(paths.iter());
                                continue;
                            }
                            Access::Any(AddressSpace::Storage) => match root {
                                Some(root) => {
                                    let bases = self.paths(engine, root);
                                    let table = &mut engine.analysis.table;
                                    bases
                                        .iter()
                                        .map(|base| table.intern(PathNode::Region { base }))
                                        .collect()
                                }
                                None => PathSet::unknown(),
                            },
                            Access::Any(AddressSpace::Transient) => {
                                let target = if write {
                                    &mut accesses.transient_writes
                                } else {
                                    &mut accesses.transient_reads
                                };
                                target.push(PathTable::UNKNOWN);
                                continue;
                            }
                            _ => continue,
                        };
                        let target = if write { &mut accesses.writes } else { &mut accesses.reads };
                        for path in paths.iter() {
                            if !target.contains(&path) {
                                target.push(path);
                            }
                        }
                    }
                }
            }
        }
        accesses
    }

    fn alias_paths(
        &mut self,
        engine: &mut SummaryEngine<'_, StorageAnalysis>,
        alias: StorageAlias,
    ) -> PathSet {
        match alias {
            StorageAlias::Slot(slot) => PathSet::single(engine.analysis.table.slot(slot)),
            StorageAlias::Symbolic(value) => self.paths(engine, value),
            StorageAlias::Offset { base, offset } => {
                self.map_paths(engine, base, |base| PathNode::Field { base, offset })
            }
        }
    }
}

/// Returns the root slot operand of a semantic operation that accesses a whole storage
/// byte array or dynamic array.
///
/// NOTE: ModRef reports these accesses as address-space-wide; the root operand lets the
/// path analysis bound them to the region derived from that slot.
fn storage_region_root(kind: &InstKind) -> Option<ValueId> {
    match *kind {
        InstKind::StorageBytesLoad(slot)
        | InstKind::StorageArrayLoad { slot, .. }
        | InstKind::StorageBytesStore(slot, _)
        | InstKind::StorageBytesStoreLiteral { slot, .. }
        | InstKind::StorageClearWords(slot, _, _)
        | InstKind::ValidateStorageBytes(slot) => Some(slot),
        _ => None,
    }
}

/// Computes storage summaries for every bodied function of `module`.
pub(crate) fn analyze_module(
    module: &Module,
    policy: super::interproc::ContextPolicy,
) -> SummaryEngine<'_, StorageAnalysis> {
    let mut engine = SummaryEngine::new(module, StorageAnalysis::new(module), policy);
    for (func, function) in module.iter_functions() {
        if !function.blocks.is_empty() {
            let context = engine.general_context(func);
            let _ = engine.summary(func, &context);
        }
    }
    engine
}

/// Writes per-instruction storage facts and summaries for `module`.
pub(crate) fn dump(engine: &SummaryEngine<'_, StorageAnalysis>, out: &mut String) {
    use std::fmt::Write as _;
    let module = engine.module;
    let table = &engine.analysis.table;
    for (func, context, summary) in engine.final_summaries() {
        let function = module.function(func);
        let Some(results) = engine.analysis.functions.get(&(func, context.clone())) else {
            continue;
        };
        let general = context.call_string.is_empty();
        let _ = write!(out, "fn @{}", function.name);
        if !general {
            let _ = write!(out, " [");
            for (i, site) in context.call_string.iter().enumerate() {
                if i != 0 {
                    let _ = write!(out, " ");
                }
                let caller = module.function(site.caller);
                let _ = write!(out, "@{}", caller.name);
            }
            let _ = write!(out, "](");
            for (i, arg) in context.entry.args.iter().enumerate() {
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
                let Some(access) = results.accesses.get(&inst) else { continue };
                if !labeled {
                    let _ = writeln!(out, "  bb{}:", block.index());
                    labeled = true;
                }
                let _ = write!(
                    out,
                    "    {}  ;",
                    crate::mir::display::display_instruction(function, Some(module), inst)
                );
                let list = |out: &mut String, name: &str, paths: &[PathId]| {
                    if paths.is_empty() {
                        return;
                    }
                    let _ = write!(out, " {name}=");
                    for (i, &path) in paths.iter().enumerate() {
                        if i != 0 {
                            let _ = write!(out, "|");
                        }
                        let _ = write!(out, "{}", table.display(path, Some(function)));
                    }
                };
                list(out, "read", &access.reads);
                list(out, "write", &access.writes);
                list(out, "tread", &access.transient_reads);
                list(out, "twrite", &access.transient_writes);
                let _ = writeln!(out);
            }
        }
        let _ = writeln!(out, "  summary:{}", summary.display(table));
    }
}
