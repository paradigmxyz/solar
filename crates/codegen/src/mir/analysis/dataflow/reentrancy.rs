//! Reentrancy, state-inconsistency, and transaction-order analysis.
//!
//! The analysis follows the state-inconsistency model of Sailfish (Bose et al., S&P 2022)
//! and the compositional design of RacerD (Blackshear et al., OOPSLA 2018). Each function's
//! summary lists its storage accesses, external calls, and event emissions as events. An
//! event carries its storage path (see [`storage_path`](super::storage_path)), the external
//! calls that may precede it, and a guard: must facts about entry slot values and `caller`
//! under which it executes, computed by the path-sensitive
//! [`SlotState`](super::slot_state::SlotState) domain. Internal calls instantiate callee
//! events: paths through the storage-pointer arguments, guards and call-site slot values
//! through the caller's current state, and the callee's exit state and normal-return
//! precondition into the caller's state. An OpenZeppelin `_nonReentrantBefore()` therefore
//! leaves `_status == ENTERED` in its caller and `_status != ENTERED` as a precondition.
//!
//! The checker treats every external call that may run untrusted code as an interleaving
//! point, like RacerD treats concurrent threads. A public entry is reentrantly callable at
//! the call when its events are feasible in the state at the call: a held lock contradicts
//! the guard of a function that requires it to be free, and an owner check excludes
//! functions only privileged accounts can call. For each callable entry `g` it reports:
//!
//! - a stale read, when the caller writes a path after the call that `g` reads;
//! - a destructive write, when `g` writes a path that the caller accesses after the call;
//! - read-only reentrancy, a stale read by a view function, which a static call can reach;
//! - event reordering, when both the caller after the call and `g` emit events.
//!
//! Calls are trusted when their target runs no code that can call back: precompiles, code
//! compiled here without call instructions (created with `new` and held in immutables or in
//! storage written only by the constructor), and value transfers limited to the 2300 gas
//! stipend, which cannot write storage. Targets chosen by the deployer or an owner still run
//! their own code, and tokens with transfer hooks call back even when their address is
//! trusted, so they remain reentrancy vectors. Delegate calls to this contract run its own
//! entries with the original caller; creations run the created constructor, which may call
//! back.
//!
//! Transaction-order dependence (Sailfish's event-ordering bugs) is reported when the amount
//! or recipient of a value transfer depends on storage written by a public entry that is not
//! owner-only.
//!
//! Limitations: storage paths use word granularity, so packed fields in one slot conflict;
//! library calls through `delegatecall` and unknown tail calls are opaque; and guards only
//! capture equalities over fixed slots and `caller`.

use super::{
    engine::{self, Analysis, Edge, EdgeCondition},
    interproc::{Context, ContextPolicy, InterproceduralAnalysis, SummaryEngine},
    lattice::{JoinSemiLattice, Reachable},
    slot_state::{
        CallId, Clobber, Constraint, Guard, Origin, Pred, SlotKey, SlotState, SymWord, Val,
    },
    storage::{FunctionStorage, StorageEntry},
    storage_path::{Activation, PathId, PathSet, PathTable},
    taint::{FunctionTaint, Source, TaintAnalysis, TaintSet},
};
use crate::{
    backend::evm,
    mir::{
        AddressCallKind, ArgIdx, BlockId, Builtin, Callee, EffectKind, Function, FunctionId,
        ImmutableId, InstId, InstKind, Module, Terminator, Value, ValueId, analysis::CallGraphInfo,
    },
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_config::EvmVersion;
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_interface::Span;
use solar_sema::hir::StateMutability;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{self, Write as _},
    rc::Rc,
};

/// Maximum events of one kind in a summary; beyond it events merge per path.
const MAX_EVENTS: usize = 256;

/// How an external call transfers control.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum CallKind {
    /// `call`, possibly with value.
    Call,
    /// `staticcall`: the callee cannot write state.
    StaticCall,
    /// `delegatecall`: the callee's code runs in this contract's context.
    DelegateCall,
    /// `callcode`: like `delegatecall` with the callee's value semantics.
    CallCode,
    /// Contract creation, which runs the created constructor.
    Create,
    /// `transfer`/`send`: a call limited to the 2300 gas stipend.
    Stipend,
}

impl fmt::Display for CallKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Call => "call",
            Self::StaticCall => "staticcall",
            Self::DelegateCall => "delegatecall",
            Self::CallCode => "callcode",
            Self::Create => "create",
            Self::Stipend => "stipend",
        })
    }
}

/// Where a call's target address comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Provenance {
    /// A hard-coded address.
    Constant(U256),
    /// The executing contract.
    This,
    /// An immutable.
    Immutable(ImmutableId),
    /// A value loaded from storage.
    Storage(PathId),
    /// Code created from compiled initcode, and whether that code may call other contracts.
    Created {
        /// Whether the initcode contains an instruction that runs other code.
        calls_out: bool,
    },
    /// A linked library.
    Library,
    /// A formal parameter of the summarized function.
    Arg(ArgIdx),
    /// `msg.sender`.
    Caller,
    /// Anything else.
    Unknown,
}

/// How much the analysis trusts a call target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Trust {
    /// No contract code runs, or the code is known to make no calls.
    NoCallback,
    /// Chosen by the deployer or a privileged account; trusted for reporting only.
    Privileged,
    /// This contract or compiled code that may call out.
    Known,
    /// Attacker-controlled or unknown code.
    Untrusted,
}

impl fmt::Display for Trust {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoCallback => "no-callback",
            Self::Privileged => "privileged",
            Self::Known => "known-code",
            Self::Untrusted => "untrusted",
        })
    }
}

/// A storage access event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AccessEvent {
    /// The accessed path, relative to the summarized function's parameters.
    pub(crate) path: PathId,
    /// Whether the access writes.
    pub(crate) write: bool,
    /// Whether the access is to transient storage.
    pub(crate) transient: bool,
    /// Facts that hold whenever the access executes.
    pub(crate) guard: Guard,
    /// External calls that may execute before the access.
    pub(crate) calls: BTreeSet<CallId>,
    /// The function and instruction that perform the access.
    pub(crate) origin: (FunctionId, InstId),
    /// Source location of the access.
    pub(crate) span: Span,
}

/// An external call event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CallEvent {
    /// The call instruction.
    pub(crate) id: CallId,
    /// How the call transfers control.
    pub(crate) kind: CallKind,
    /// Where the target address comes from.
    pub(crate) target: Provenance,
    /// Whether the call may transfer value.
    pub(crate) sends_value: bool,
    /// Taint of the transferred value.
    pub(crate) amount_taint: TaintSet,
    /// Taint of the target address.
    pub(crate) recipient_taint: TaintSet,
    /// Facts that hold whenever the call executes.
    pub(crate) guard: Guard,
    /// Words of tracked slots at the call.
    pub(crate) slots: BTreeMap<SlotKey, SymWord>,
    /// How much untracked storage may have changed before the call.
    pub(crate) clobber: Clobber,
    /// External calls that may execute before this one.
    pub(crate) calls: BTreeSet<CallId>,
    /// Source location of the call.
    pub(crate) span: Span,
}

/// An event emission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LogEvent {
    /// The emitting instruction.
    pub(crate) origin: (FunctionId, InstId),
    /// Facts that hold whenever the event is emitted.
    pub(crate) guard: Guard,
    /// External calls that may execute before the emission.
    pub(crate) calls: BTreeSet<CallId>,
    /// Source location of the emission.
    pub(crate) span: Span,
}

/// The state at a normal return.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExitState {
    /// Words of written slots.
    pub(crate) slots: BTreeMap<SlotKey, SymWord>,
    /// How much untracked storage may have changed.
    pub(crate) clobber: Clobber,
    /// Facts that hold on every normal return: the function's precondition.
    pub(crate) guard: Guard,
    /// External calls that may have executed.
    pub(crate) calls: BTreeSet<CallId>,
    /// Facts about each returned component.
    pub(crate) returns: SmallVec<[Option<Val>; 1]>,
}

/// A function's reentrancy summary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ReentrancySummary {
    /// Storage accesses.
    pub(crate) accesses: Vec<AccessEvent>,
    /// External calls.
    pub(crate) calls: Vec<CallEvent>,
    /// Event emissions.
    pub(crate) logs: Vec<LogEvent>,
    /// The joined state of normal returns; `None` if the function never returns.
    pub(crate) exit: Option<ExitState>,
    /// What holds whenever execution ends successfully inside the function or a callee,
    /// through `stop`, `return`, or `selfdestruct`.
    pub(crate) success: Option<Guard>,
}

impl ReentrancySummary {
    fn add_access(&mut self, event: AccessEvent) {
        let same_access = |existing: &AccessEvent| {
            existing.path == event.path
                && existing.write == event.write
                && existing.transient == event.transient
        };
        // Beyond the bound, events of one path merge into their weakest guard.
        let full = self.accesses.len() >= MAX_EVENTS;
        if let Some(existing) = self.accesses.iter_mut().find(|existing| {
            same_access(existing)
                && (full || existing.guard == event.guard && existing.calls == event.calls)
        }) {
            existing.calls.extend(event.calls);
            existing.guard.weaken(&event.guard);
            return;
        }
        self.accesses.push(event);
    }

    fn add_call(&mut self, event: CallEvent) {
        if let Some(existing) = self.calls.iter_mut().find(|existing| existing.id == event.id) {
            existing.guard.weaken(&event.guard);
            existing.calls.extend(event.calls);
            existing.sends_value |= event.sends_value;
            existing.amount_taint.join(&event.amount_taint);
            existing.recipient_taint.join(&event.recipient_taint);
            existing.clobber = existing.clobber.max(event.clobber);
            let keys = existing.slots.keys().chain(event.slots.keys()).copied().collect::<Vec<_>>();
            for key in keys {
                let theirs = event.slots.get(&key).copied().unwrap_or(SymWord::UNKNOWN);
                existing.slots.entry(key).or_insert(SymWord::UNKNOWN).join(&theirs);
            }
            if existing.target != event.target {
                existing.target = Provenance::Unknown;
            }
            return;
        }
        self.calls.push(event);
    }

    fn add_log(&mut self, event: LogEvent) {
        if let Some(existing) =
            self.logs.iter_mut().find(|existing| existing.origin == event.origin)
        {
            existing.guard.weaken(&event.guard);
            existing.calls.extend(event.calls);
            return;
        }
        self.logs.push(event);
    }
}

impl JoinSemiLattice for ReentrancySummary {
    fn join(&mut self, other: &Self) -> bool {
        let before = self.clone();
        for event in &other.accesses {
            self.add_access(event.clone());
        }
        for event in &other.calls {
            self.add_call(event.clone());
        }
        for event in &other.logs {
            self.add_log(event.clone());
        }
        match (&mut self.success, &other.success) {
            (_, None) => {}
            (None, Some(guard)) => self.success = Some(guard.clone()),
            (Some(mine), Some(theirs)) => mine.weaken(theirs),
        }
        match (&mut self.exit, &other.exit) {
            (_, None) => {}
            (None, Some(exit)) => self.exit = Some(exit.clone()),
            (Some(mine), Some(theirs)) => {
                let mut state = exit_as_state(mine);
                state.join(&exit_as_state(theirs));
                let returns = mine
                    .returns
                    .iter()
                    .zip(&theirs.returns)
                    .map(|(a, b)| if a == b { *a } else { None })
                    .collect();
                *mine = state_as_exit(&state, returns);
            }
        }
        *self != before
    }
}

fn exit_as_state(exit: &ExitState) -> SlotState {
    SlotState {
        slots: exit.slots.clone(),
        clobber: exit.clobber,
        guard: exit.guard.clone(),
        values: FxHashMap::default(),
        calls: exit.calls.clone(),
    }
}

fn state_as_exit(state: &SlotState, returns: SmallVec<[Option<Val>; 1]>) -> ExitState {
    ExitState {
        slots: state.slots.clone(),
        clobber: state.clobber,
        guard: state.guard.clone(),
        calls: state.calls.clone(),
        returns,
    }
}

/// Module-level facts needed while summarizing functions.
#[derive(Debug, Default)]
pub(crate) struct ModuleFacts {
    /// Runtime entry points.
    pub(crate) entries: Vec<FunctionId>,
    /// View and pure entries.
    pub(crate) views: FxHashSet<FunctionId>,
    /// Paths each runtime entry may write, with keys erased.
    pub(crate) entry_writes: FxHashMap<FunctionId, Vec<PathId>>,
    /// Exact slots that some runtime entry may write.
    pub(crate) runtime_written: BTreeSet<SlotKey>,
    /// Whether some runtime entry writes unknown storage.
    pub(crate) runtime_writes_unknown: bool,
    /// Trust of immutables, from constructor assignments.
    pub(crate) immutables: FxHashMap<ImmutableId, Trust>,
    /// Trust of values stored by the constructor in exact slots.
    pub(crate) constructor_slots: FxHashMap<U256, Trust>,
}

impl ModuleFacts {
    /// Returns the trust of a target that does not depend on privileged accounts, if known.
    pub(crate) fn code_trust(&self, table: &mut PathTable, target: Provenance) -> Option<Trust> {
        match target {
            Provenance::Constant(address) if address.is_zero() || is_precompile(address) => {
                Some(Trust::NoCallback)
            }
            Provenance::This | Provenance::Library => Some(Trust::Known),
            Provenance::Immutable(id) => self.immutables.get(&id).copied(),
            Provenance::Created { calls_out } => {
                Some(if calls_out { Trust::Known } else { Trust::NoCallback })
            }
            Provenance::Storage(path) => {
                let erased = table.erase_keys(path);
                let written = self.entries.iter().any(|entry| {
                    self.entry_writes[entry]
                        .iter()
                        .any(|&written| table.may_alias(written, erased, Activation::Different))
                });
                if written {
                    return None;
                }
                let slot = table.as_slot(path)?;
                // A slot the constructor never writes holds zero, which runs no code.
                Some(self.constructor_slots.get(&slot).copied().unwrap_or(Trust::NoCallback))
            }
            Provenance::Constant(_) | Provenance::Arg(_) | Provenance::Caller => None,
            Provenance::Unknown => Some(Trust::Untrusted),
        }
    }
}

/// The reentrancy analysis.
pub(crate) struct ReentrancyAnalysis<'m> {
    /// Taint and storage analyses the events refer to.
    pub(crate) taint: SummaryEngine<'m, TaintAnalysis<'m>>,
    /// Module facts.
    pub(crate) facts: Rc<ModuleFacts>,
    /// Per-instruction facts recorded for dumps, keyed by function.
    pub(crate) inst_facts: FxHashMap<FunctionId, Rc<FxHashMap<InstId, String>>>,
    /// The EVM version, which decides how bytecode decodes.
    pub(crate) evm_version: EvmVersion,
}

impl<'m> ReentrancyAnalysis<'m> {
    fn storage_of(&mut self, func: FunctionId) -> Rc<FunctionStorage> {
        self.taint.analysis.storage_of(func)
    }

    fn taint_of(&mut self, func: FunctionId) -> Rc<FunctionTaint> {
        let context = self.taint.general_context(func);
        let _ = self.taint.summary(func, &context);
        self.taint.analysis.functions.get(&(func, context)).cloned().unwrap_or_default()
    }

    fn table(&mut self) -> &mut PathTable {
        &mut self.taint.analysis.storage.analysis.table
    }
}

impl InterproceduralAnalysis for ReentrancyAnalysis<'_> {
    type Entry = ();
    type Summary = ReentrancySummary;

    fn general_entry(&self, _module: &Module, _func: FunctionId) {}

    fn bottom_summary(&self, _module: &Module, _func: FunctionId) -> ReentrancySummary {
        ReentrancySummary::default()
    }

    fn unknown_summary(&self, _module: &Module, _func: FunctionId) -> ReentrancySummary {
        // A body-less function: unknown storage effects, but no calls this analysis can see.
        ReentrancySummary {
            exit: Some(ExitState {
                slots: BTreeMap::new(),
                clobber: Clobber::All,
                guard: Guard::default(),
                calls: BTreeSet::new(),
                returns: SmallVec::new(),
            }),
            ..ReentrancySummary::default()
        }
    }

    fn summarize(
        engine: &mut SummaryEngine<'_, Self>,
        func: FunctionId,
        _context: &Context<()>,
    ) -> ReentrancySummary {
        let module = engine.module;
        let function = module.function(func);
        let storage = engine.analysis.storage_of(func);
        let taint = engine.analysis.taint_of(func);
        let cfg = crate::mir::analysis::CfgInfo::new(function);
        let mut transfer = Transfer {
            engine,
            func_id: func,
            storage: &storage,
            taint: &taint,
            summary: ReentrancySummary::default(),
            recording: false,
            facts: FxHashMap::default(),
        };
        let results = engine::solve(function, &cfg, &mut transfer);
        let mut exits = ReentrancySummary::default();
        for &block in cfg.rpo() {
            if !results.is_visited(block) {
                continue;
            }
            let exit = engine::block_exit_state(function, &mut transfer, &results, block);
            let Reachable::State(state) = exit else { continue };
            if matches!(
                function.blocks[block].terminator,
                Some(
                    Terminator::Stop
                        | Terminator::ReturnData { .. }
                        | Terminator::SelfDestruct { .. }
                )
            ) {
                exits.join(&ReentrancySummary {
                    success: Some(state.guard.clone()),
                    ..ReentrancySummary::default()
                });
            }
            if let Some(Terminator::Return { values }) = &function.blocks[block].terminator {
                let returns = values
                    .iter()
                    .map(|&value| state.value(function, value))
                    .collect::<SmallVec<[_; 1]>>();
                let exit = state_as_exit(&state, returns);
                exits.join(&ReentrancySummary { exit: Some(exit), ..ReentrancySummary::default() });
            }
        }
        // Record events once, over the fixed point.
        transfer.recording = true;
        engine::replay(function, &cfg, &mut transfer, &results, |_, _, _| {});
        let mut summary = transfer.summary;
        summary.exit = exits.exit;
        summary.join(&ReentrancySummary { success: exits.success, ..ReentrancySummary::default() });
        let facts = transfer.facts;
        let table = engine.analysis.table();
        for event in &mut summary.accesses {
            event.path = table.generalize(event.path, func);
        }
        for event in &mut summary.calls {
            if let Provenance::Storage(path) = event.target {
                event.target = Provenance::Storage(table.generalize(path, func));
            }
        }
        engine.analysis.inst_facts.insert(func, Rc::new(facts));
        summary
    }
}

/// The per-function transfer functions.
struct Transfer<'a, 'e, 'm> {
    engine: &'a mut SummaryEngine<'e, ReentrancyAnalysis<'m>>,
    func_id: FunctionId,
    storage: &'a FunctionStorage,
    taint: &'a FunctionTaint,
    summary: ReentrancySummary,
    recording: bool,
    facts: FxHashMap<InstId, String>,
}

/// Operands of one external call.
pub(crate) struct CallOperands {
    /// How the call transfers control.
    pub(crate) kind: CallKind,
    /// The target address, absent for creations.
    pub(crate) target: Option<ValueId>,
    /// The transferred value, if any.
    pub(crate) value: Option<ValueId>,
}

/// Returns the call operands of an external call or creation.
///
/// NOTE: Classification uses [`EffectKind`]; this match only extracts the operands whose
/// meaning differs between call forms.
pub(crate) fn call_operands(kind: &InstKind) -> Option<CallOperands> {
    if !matches!(kind.effect_kind(), EffectKind::ExternalCall | EffectKind::Create) {
        return None;
    }
    Some(match *kind {
        InstKind::Call { addr, value, .. } => {
            CallOperands { kind: CallKind::Call, target: Some(addr), value: Some(value) }
        }
        InstKind::CallCode { addr, value, .. } => {
            CallOperands { kind: CallKind::CallCode, target: Some(addr), value: Some(value) }
        }
        InstKind::StaticCall { addr, .. } => {
            CallOperands { kind: CallKind::StaticCall, target: Some(addr), value: None }
        }
        InstKind::DelegateCall { addr, .. } => {
            CallOperands { kind: CallKind::DelegateCall, target: Some(addr), value: None }
        }
        InstKind::AddressCall { kind, address, value, .. } => CallOperands {
            kind: match kind {
                AddressCallKind::Call => CallKind::Call,
                AddressCallKind::Static => CallKind::StaticCall,
                AddressCallKind::Delegate => CallKind::DelegateCall,
            },
            target: Some(address),
            value,
        },
        InstKind::Create(value, ..) | InstKind::Create2(value, ..) => {
            CallOperands { kind: CallKind::Create, target: None, value: Some(value) }
        }
        InstKind::ICall {
            function: Callee::Builtin(Builtin::Send | Builtin::Transfer),
            ref args,
        } => CallOperands {
            kind: CallKind::Stipend,
            target: args.first().copied(),
            value: args.get(1).copied(),
        },
        // Precompile builtins run no contract code.
        _ => return None,
    })
}

/// Returns where a call target address comes from.
pub(crate) fn target_provenance(
    module: &Module,
    storage: &FunctionStorage,
    func: &Function,
    value: ValueId,
    evm_version: EvmVersion,
    depth: usize,
) -> Provenance {
    if depth > 16 {
        return Provenance::Unknown;
    }
    let recurse = |value| target_provenance(module, storage, func, value, evm_version, depth + 1);
    match func.value(value) {
        Value::Immediate(imm) => imm.as_u256().map_or(Provenance::Unknown, Provenance::Constant),
        Value::Arg(index) => Provenance::Arg(*index),
        Value::Undef(_) | Value::Error(_) => Provenance::Unknown,
        Value::Inst(inst) => match func.inst(*inst).kind {
            InstKind::Zext(x)
            | InstKind::Trunc(x, _)
            | InstKind::Bitcast(x)
            | InstKind::IntToPtr(x)
            | InstKind::PtrToInt(x, _) => recurse(x),
            InstKind::And(x, y) if func.value_u256(y).is_some() => recurse(x),
            InstKind::And(x, y) if func.value_u256(x).is_some() => recurse(y),
            InstKind::Address => Provenance::This,
            InstKind::Caller => Provenance::Caller,
            InstKind::LoadImmutable(id) => Provenance::Immutable(id),
            InstKind::LibraryAddress(_) => Provenance::Library,
            InstKind::SLoad(_) => match storage.accesses.get(inst) {
                Some(access) if access.reads.len() == 1 => Provenance::Storage(access.reads[0]),
                _ => Provenance::Unknown,
            },
            InstKind::Create(_, offset, _) | InstKind::Create2(_, offset, _, _) => {
                created_provenance(module, func, offset, evm_version)
            }
            _ => Provenance::Unknown,
        },
    }
}

/// Returns whether `address` is a precompile: its call runs no contract code.
pub(crate) fn is_precompile(address: U256) -> bool {
    (U256::from(1)..=U256::from(0x11)).contains(&address) || address == U256::from(0x100)
}

impl Transfer<'_, '_, '_> {
    fn slot_of(&mut self, path: PathId, transient: bool) -> Option<SlotKey> {
        self.engine.analysis.table().as_slot(path).map(|slot| SlotKey { transient, slot })
    }

    /// Applies a write of `word` to each path.
    fn write_paths(
        &mut self,
        paths: &[PathId],
        transient: bool,
        word: SymWord,
        state: &mut SlotState,
    ) {
        let exact = paths.len() == 1;
        for &path in paths {
            if let Some(slot) = self.slot_of(path, transient) {
                state.write(slot, if exact { word } else { SymWord::UNKNOWN });
                continue;
            }
            let table = self.engine.analysis.table();
            if path == PathTable::UNKNOWN {
                state.clobber(Clobber::All);
            } else if table.is_relative(path) {
                state.clobber(Clobber::Relative);
            } else if !table.is_hashed(path) {
                // An element of an absolute array may overlap any tracked slot.
                state.clobber(Clobber::All);
            }
        }
    }

    /// Returns the provenance of a call target address.
    fn provenance(&mut self, func: &Function, value: ValueId, depth: usize) -> Provenance {
        target_provenance(
            self.engine.module,
            self.storage,
            func,
            value,
            self.engine.analysis.evm_version,
            depth,
        )
    }

    fn record(&mut self, inst: InstId, fact: impl fmt::Display) {
        if self.recording {
            let entry = self.facts.entry(inst).or_default();
            if !entry.is_empty() {
                entry.push_str("; ");
            }
            let _ = write!(entry, "{fact}");
        }
    }

    fn access(
        &mut self,
        func: &Function,
        inst: InstId,
        path: PathId,
        write: bool,
        transient: bool,
        state: &SlotState,
    ) {
        if !self.recording {
            return;
        }
        let table = &self.engine.analysis.taint.analysis.storage.analysis.table;
        let text = format!(
            "{} {}{}{}",
            if write { "write" } else { "read" },
            if transient { "transient " } else { "" },
            table.display(path, Some(func)),
            display_context(&state.guard, &state.calls),
        );
        self.record(inst, text);
        let span = func.inst(inst).metadata.source_span().unwrap_or(Span::DUMMY);
        self.summary.add_access(AccessEvent {
            path,
            write,
            transient,
            guard: state.guard.clone(),
            calls: state.calls.clone(),
            origin: (self.func_id, inst),
            span,
        });
    }

    fn external_call(
        &mut self,
        func: &Function,
        inst: InstId,
        call: CallOperands,
        state: &mut SlotState,
    ) {
        let target = match call.kind {
            CallKind::Create => match func.inst(inst).kind {
                InstKind::Create(_, offset, _) | InstKind::Create2(_, offset, _, _) => {
                    created_provenance(
                        self.engine.module,
                        func,
                        offset,
                        self.engine.analysis.evm_version,
                    )
                }
                _ => Provenance::Unknown,
            },
            _ => match call.target {
                Some(target) => self.provenance(func, target, 0),
                None => Provenance::Unknown,
            },
        };
        if let Provenance::Constant(address) = target
            && is_precompile(address)
        {
            return;
        }
        let sends_value =
            call.value.is_some_and(|value| func.value_u256(value) != Some(U256::ZERO));
        let taint_of = |value: Option<ValueId>| {
            value.and_then(|value| self.taint.values.get(&value)).cloned().unwrap_or_default()
        };
        let amount_taint = taint_of(call.value);
        let recipient_taint = taint_of(call.target);
        let id = CallId { func: self.func_id, inst };
        let facts = Rc::clone(&self.engine.analysis.facts);
        let may_run_code =
            facts.code_trust(self.engine.analysis.table(), target) != Some(Trust::NoCallback);
        if self.recording {
            let text = format!(
                "{} {}{}{}",
                call.kind,
                display_provenance(
                    target,
                    &self.engine.analysis.taint.analysis.storage.analysis.table,
                    func
                ),
                display_slots(&state.slots),
                display_context(&state.guard, &state.calls),
            );
            self.record(inst, text);
            let span = func.inst(inst).metadata.source_span().unwrap_or(Span::DUMMY);
            self.summary.add_call(CallEvent {
                id,
                kind: call.kind,
                target,
                sends_value,
                amount_taint,
                recipient_taint,
                guard: state.guard.clone(),
                slots: state.slots.clone(),
                clobber: state.clobber,
                calls: state.calls.clone(),
                span,
            });
        }
        state.calls.insert(id);
        // Reentrant entries may change any slot some runtime entry writes.
        if may_run_code && !matches!(call.kind, CallKind::StaticCall | CallKind::Stipend) {
            if facts.runtime_writes_unknown {
                state.clobber(Clobber::All);
            } else {
                for &slot in &facts.runtime_written {
                    state.write(slot, SymWord::UNKNOWN);
                }
            }
        }
    }

    fn internal_call(
        &mut self,
        func: &Function,
        inst: Option<InstId>,
        callee: FunctionId,
        args: &[ValueId],
        state: &mut Reachable<SlotState>,
    ) {
        let context = self.engine.general_context(callee);
        let summary = self.engine.summary(callee, &context);
        let Reachable::State(current) = state else { return };
        let entry = inst.and_then(|inst| self.storage.call_args.get(&inst).cloned());
        let (arg_paths, arg_keys) = match &entry {
            Some(StorageEntry { args, keys }) => (args.to_vec(), keys.to_vec()),
            None => (
                vec![PathSet::unknown(); args.len()],
                vec![super::storage_path::KeyTerm::Any; args.len()],
            ),
        };
        if self.recording {
            for event in &summary.accesses {
                let Some(guard) = compose_guard(&event.guard, current) else { continue };
                let paths =
                    self.engine.analysis.table().instantiate(event.path, &arg_paths, &arg_keys);
                let mut calls = event.calls.clone();
                calls.extend(current.calls.iter().copied());
                for path in paths.iter() {
                    self.summary.add_access(AccessEvent {
                        path,
                        guard: conjoin(&current.guard, &guard),
                        calls: calls.clone(),
                        ..event.clone()
                    });
                }
            }
            for event in &summary.calls {
                let Some(guard) = compose_guard(&event.guard, current) else { continue };
                let slots = event
                    .slots
                    .iter()
                    .map(|(&slot, word)| (slot, word.compose(|s| current.current(s))))
                    .chain(
                        current
                            .slots
                            .iter()
                            .filter(|(slot, _)| !event.slots.contains_key(slot))
                            .map(|(&slot, &word)| (slot, word)),
                    )
                    .collect();
                let target = match event.target {
                    Provenance::Arg(index) => match args.get(index.index()) {
                        Some(&arg) => self.provenance(func, arg, 0),
                        None => Provenance::Unknown,
                    },
                    Provenance::Storage(path) => {
                        match self
                            .engine
                            .analysis
                            .table()
                            .instantiate(path, &arg_paths, &arg_keys)
                            .as_single()
                        {
                            Some(path) => Provenance::Storage(path),
                            None => Provenance::Unknown,
                        }
                    }
                    target => target,
                };
                let amount_taint =
                    self.instantiate_taint(&event.amount_taint, args, &arg_paths, &arg_keys);
                let recipient_taint =
                    self.instantiate_taint(&event.recipient_taint, args, &arg_paths, &arg_keys);
                let mut calls = event.calls.clone();
                calls.extend(current.calls.iter().copied());
                self.summary.add_call(CallEvent {
                    target,
                    amount_taint,
                    recipient_taint,
                    guard: conjoin(&current.guard, &guard),
                    slots,
                    clobber: current.clobber.max(event.clobber),
                    calls,
                    ..event.clone()
                });
            }
            if let Some(success) = &summary.success
                && let Some(guard) = compose_guard(success, current)
            {
                self.summary.join(&ReentrancySummary {
                    success: Some(conjoin(&current.guard, &guard)),
                    ..ReentrancySummary::default()
                });
            }
            for event in &summary.logs {
                let Some(guard) = compose_guard(&event.guard, current) else { continue };
                let mut calls = event.calls.clone();
                calls.extend(current.calls.iter().copied());
                self.summary.add_log(LogEvent {
                    guard: conjoin(&current.guard, &guard),
                    calls,
                    ..event.clone()
                });
            }
        }
        let Some(exit) = &summary.exit else {
            *state = Reachable::Unreachable;
            return;
        };
        // The callee's precondition constrains the state at the call.
        for ((slot, mask), constraint) in &exit.guard.conds {
            let preds: SmallVec<[Pred; 2]> = match constraint {
                Constraint::Eq(value) => {
                    smallvec::smallvec![Pred::Field {
                        slot: *slot,
                        mask: *mask,
                        value: *value,
                        eq: true
                    }]
                }
                Constraint::Ne(values) => values
                    .iter()
                    .map(|&value| Pred::Field { slot: *slot, mask: *mask, value, eq: false })
                    .collect(),
            };
            for pred in preds {
                if let Some(pred) = pred.compose(current)
                    && !current.assume(pred)
                {
                    *state = Reachable::Unreachable;
                    return;
                }
            }
        }
        for &origin in &exit.guard.caller_is {
            if let Some(pred) = (Pred::Caller { origin, eq: true }).compose(current)
                && !current.assume(pred)
            {
                *state = Reachable::Unreachable;
                return;
            }
        }
        let before = current.clone();
        if exit.clobber == Clobber::All {
            current.clobber(Clobber::All);
        }
        if let Some(inst) = inst
            && let Some(access) = self.storage.accesses.get(&inst)
        {
            let written = access.writes.clone();
            for path in written {
                if let Some(slot) = self.slot_of(path, false)
                    && exit.slots.contains_key(&slot)
                {
                    continue;
                }
                self.write_paths(&[path], false, SymWord::UNKNOWN, current);
            }
            let written = access.transient_writes.clone();
            for path in written {
                if let Some(slot) = self.slot_of(path, true)
                    && exit.slots.contains_key(&slot)
                {
                    continue;
                }
                self.write_paths(&[path], true, SymWord::UNKNOWN, current);
            }
        }
        for (&slot, &word) in &exit.slots {
            current.write(slot, word.compose(|s| before.current(s)));
        }
        current.calls.extend(exit.calls.iter().copied());
        if let Some(inst) = inst
            && let Some(result) = func.inst_result_value(inst)
            && let Some(Some(value)) = exit.returns.first()
            && let Some(value) = compose_val(*value, &before)
        {
            current.values.insert(result, value);
        }
    }
}

impl Transfer<'_, '_, '_> {
    /// Restates callee taint in terms of the caller's arguments and paths.
    fn instantiate_taint(
        &mut self,
        taint: &TaintSet,
        args: &[ValueId],
        arg_paths: &[PathSet],
        arg_keys: &[super::storage_path::KeyTerm],
    ) -> TaintSet {
        let mut out = TaintSet::default();
        for source in taint.iter() {
            match source {
                Source::Arg(index) => {
                    if let Some(taint) =
                        args.get(index.index()).and_then(|arg| self.taint.values.get(arg))
                    {
                        out.join(taint);
                    }
                }
                Source::Storage(path) => {
                    let paths = self.engine.analysis.table().instantiate(path, arg_paths, arg_keys);
                    out.join(&TaintSet(paths.iter().map(Source::Storage).collect()));
                }
                source => {
                    out.join(&TaintSet([source].into()));
                }
            }
        }
        out
    }
}

fn conjoin(a: &Guard, b: &Guard) -> Guard {
    let mut guard = a.clone();
    for (key, constraint) in &b.conds {
        guard.conds.entry(*key).or_insert_with(|| constraint.clone());
    }
    guard.caller_is.extend(b.caller_is.iter().copied());
    guard
}

/// Restates a callee guard over its entry in terms of the caller's `state`.
///
/// Returns `None` when the guard contradicts the state, so the event cannot happen.
fn compose_guard(guard: &Guard, state: &SlotState) -> Option<Guard> {
    let mut composed = Guard::default();
    for (&(slot, mask), constraint) in &guard.conds {
        let current = state.current(slot);
        if current.known & mask == mask {
            if !constraint.admits(current.value & mask) {
                return None;
            }
            continue;
        }
        if current.is_entry_identity(slot, mask) {
            if let Some(existing) = state.guard.conds.get(&(slot, mask))
                && !existing.compatible(constraint)
            {
                return None;
            }
            composed.conds.insert((slot, mask), constraint.clone());
        }
    }
    for &origin in &guard.caller_is {
        if let Some(Pred::Caller { origin, .. }) =
            (Pred::Caller { origin, eq: true }).compose(state)
        {
            composed.caller_is.insert(origin);
        }
    }
    Some(composed)
}

fn compose_val(value: Val, state: &SlotState) -> Option<Val> {
    match value {
        Val::Word(word) => Some(Val::Word(word.compose(|slot| state.current(slot)))),
        Val::Pred(pred) => pred.compose(state).map(Val::Pred),
        Val::Caller | Val::Immutable(_) => Some(value),
    }
}

/// Reconstructs the initcode a creation deploys from the constant bytes written to the start
/// of its input buffer.
///
/// Source lowering copies initcode with `data_copy`, or stores short initcode as constant
/// words. Only the contiguous prefix of constant bytes is decoded; constructor arguments
/// follow it and are never executed as code, because compiled constructors only jump to
/// their own labels.
pub(crate) fn created_provenance(
    module: &Module,
    func: &Function,
    offset: ValueId,
    evm_version: EvmVersion,
) -> Provenance {
    let root = |value: ValueId| {
        let mut value = value;
        loop {
            let Value::Inst(inst) = func.value(value) else { return value };
            match func.inst(*inst).kind {
                InstKind::PtrToInt(x, _) | InstKind::IntToPtr(x) | InstKind::Bitcast(x) => {
                    value = x
                }
                _ => return value,
            }
        }
    };
    let target = root(offset);
    let offset_of = |dest: ValueId| -> Option<u64> {
        let dest = root(dest);
        if dest == target {
            return Some(0);
        }
        let Value::Inst(inst) = func.value(dest) else { return None };
        match func.inst(*inst).kind {
            InstKind::Add(x, y) if root(x) == target => func.value_u64(y),
            InstKind::Add(x, y) if root(y) == target => func.value_u64(x),
            _ => None,
        }
    };
    let mut bytes = BTreeMap::<u64, u8>::new();
    for inst in func.instructions() {
        match func.inst(inst).kind {
            InstKind::DataCopy(data, dest, size) => {
                if let (Some(start), Some(size), Some(content)) =
                    (offset_of(dest), func.value_u64(size), module.get_data(data.id))
                {
                    let from = data.offset as usize;
                    for (i, &byte) in content.iter().skip(from).take(size as usize).enumerate() {
                        bytes.insert(start + i as u64, byte);
                    }
                }
            }
            InstKind::MStore(dest, value) => {
                if let (Some(start), Some(word)) = (offset_of(dest), func.value_u256(value)) {
                    for (i, byte) in word.to_be_bytes::<32>().into_iter().enumerate() {
                        bytes.insert(start + i as u64, byte);
                    }
                }
            }
            _ => {}
        }
    }
    let prefix = bytes
        .iter()
        .enumerate()
        .take_while(|&(index, (&offset, _))| offset == index as u64)
        .map(|(_, (_, &byte))| byte)
        .collect::<Vec<_>>();
    if prefix.is_empty() {
        return Provenance::Unknown;
    }
    Provenance::Created { calls_out: evm::may_execute_code(&prefix, evm_version) }
}

fn display_context<'a>(guard: &'a Guard, calls: &'a BTreeSet<CallId>) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| {
        let guard_text = guard.display().to_string();
        if !guard_text.is_empty() {
            write!(f, " if {guard_text}")?;
        }
        if !calls.is_empty() {
            write!(f, " after")?;
            for call in calls {
                write!(f, " call#{}.{}", call.func.index(), call.inst.index())?;
            }
        }
        Ok(())
    })
}

fn display_slots(slots: &BTreeMap<SlotKey, SymWord>) -> impl fmt::Display + '_ {
    fmt::from_fn(move |f| {
        for (slot, word) in slots {
            write!(f, " {slot}={word}")?;
        }
        Ok(())
    })
}

fn display_provenance<'a>(
    target: Provenance,
    table: &'a PathTable,
    func: &'a Function,
) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| match target {
        Provenance::Constant(address) => write!(f, "to {address:#x}"),
        Provenance::This => write!(f, "to this"),
        Provenance::Immutable(id) => write!(f, "to immutable{}", id.index()),
        Provenance::Storage(path) => write!(f, "to sload({})", table.display(path, Some(func))),
        Provenance::Created { calls_out: true } => write!(f, "of code that calls out"),
        Provenance::Created { calls_out: false } => write!(f, "of code without calls"),
        Provenance::Library => write!(f, "to library"),
        Provenance::Arg(index) => write!(f, "to arg{}", index.index()),
        Provenance::Caller => write!(f, "to caller"),
        Provenance::Unknown => write!(f, "to ?"),
    })
}

impl Analysis for Transfer<'_, '_, '_> {
    type Domain = Reachable<SlotState>;

    fn bottom(&self, _func: &Function) -> Self::Domain {
        Reachable::Unreachable
    }

    fn initialize_boundary(&mut self, _func: &Function, _block: BlockId, state: &mut Self::Domain) {
        *state = Reachable::State(SlotState::default());
    }

    fn apply_instruction(
        &mut self,
        func: &Function,
        _block: BlockId,
        inst: InstId,
        state: &mut Self::Domain,
    ) {
        let kind = &func.inst(inst).kind;
        if let InstKind::ICall { function: Callee::Function(callee), args } = kind {
            let args = args.clone();
            self.internal_call(func, Some(inst), *callee, &args, state);
            if let Reachable::State(current) = state {
                self.record_accesses(func, inst, current, true);
            }
            return;
        }
        let Reachable::State(current) = state else { return };
        let result = func.inst_result_value(inst);
        let value = |current: &SlotState, value| current.value(func, value);
        let loaded = match *kind {
            InstKind::SLoad(_) | InstKind::TLoad(_) => {
                let transient = matches!(kind, InstKind::TLoad(_));
                let access = self.storage.accesses.get(&inst);
                let paths = access.map(|access| {
                    if transient { access.transient_reads.clone() } else { access.reads.clone() }
                });
                match paths.as_deref() {
                    Some(&[path]) => self.slot_of(path, transient),
                    _ => None,
                }
            }
            _ => None,
        };
        let fact = current.evaluate(func, inst, loaded);
        if let (Some(result), Some(fact)) = (result, fact) {
            current.values.insert(result, fact);
        }

        // Storage effects.
        match *kind {
            InstKind::SStore(_, stored) | InstKind::TStore(_, stored) => {
                let transient = matches!(kind, InstKind::TStore(..));
                self.record_accesses(func, inst, current, false);
                let word = value(current, stored).and_then(Val::word).unwrap_or(SymWord::UNKNOWN);
                if let Some(access) = self.storage.accesses.get(&inst) {
                    let paths = if transient {
                        access.transient_writes.clone()
                    } else {
                        access.writes.clone()
                    };
                    self.write_paths(&paths, transient, word, current);
                }
            }
            _ => {
                if let Some(call) = call_operands(kind) {
                    // A delegate call into this contract runs its own entries, which the
                    // reentrancy model covers; its opaque footprint is not the caller's.
                    let into_self = call.kind == CallKind::DelegateCall
                        && call.target.is_some_and(|target| {
                            self.provenance(func, target, 0) == Provenance::This
                        });
                    if !into_self {
                        self.record_accesses(func, inst, current, false);
                    }
                    self.external_call(func, inst, call, current);
                    // A delegate call's storage effects are in its accesses.
                    if let Some(access) = self.storage.accesses.get(&inst) {
                        let writes = access.writes.clone();
                        self.write_paths(&writes, false, SymWord::UNKNOWN, current);
                    }
                    return;
                }
                self.record_accesses(func, inst, current, false);
                if let Some(access) = self.storage.accesses.get(&inst) {
                    let writes = access.writes.clone();
                    self.write_paths(&writes, false, SymWord::UNKNOWN, current);
                    let writes = access.transient_writes.clone();
                    self.write_paths(&writes, true, SymWord::UNKNOWN, current);
                }
            }
        }

        // Checks and requirements continue only when they pass.
        if let Some(pred) = current.check_assumption(func, kind)
            && !current.assume(pred)
        {
            *state = Reachable::Unreachable;
            return;
        }

        if kind.effect_kind() == EffectKind::Log && self.recording {
            self.record(
                inst,
                format_args!("log{}", display_context(&current.guard, &current.calls)),
            );
            let span = func.inst(inst).metadata.source_span().unwrap_or(Span::DUMMY);
            self.summary.add_log(LogEvent {
                origin: (self.func_id, inst),
                guard: current.guard.clone(),
                calls: current.calls.clone(),
                span,
            });
        }
    }

    fn apply_terminator(&mut self, func: &Function, block: BlockId, state: &mut Self::Domain) {
        if let Some(Terminator::TailCall { function, args }) = &func.blocks[block].terminator {
            let args = args.clone();
            self.internal_call(func, None, *function, &args, state);
        }
    }

    fn apply_edge(&mut self, func: &Function, edge: &Edge, state: &mut Self::Domain) {
        let Reachable::State(current) = state else { return };
        if let EdgeCondition::Branch { condition, taken } = edge.condition
            && let Some(pred) = current.condition(func, condition)
            && !current.assume(if taken { pred } else { pred.negate() })
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
        if let (Some(result), Some(value)) =
            (func.inst_result_value(phi), current.value(func, incoming))
        {
            current.values.insert(result, value);
        }
    }
}

impl Transfer<'_, '_, '_> {
    /// Records the storage accesses of `inst` as events.
    fn record_accesses(&mut self, func: &Function, inst: InstId, state: &SlotState, call: bool) {
        if !self.recording || call {
            // Internal calls record their callees' instantiated events instead.
            return;
        }
        let Some(access) = self.storage.accesses.get(&inst) else { return };
        let access = access.clone();
        for &path in &access.reads {
            self.access(func, inst, path, false, false, state);
        }
        for &path in &access.transient_reads {
            self.access(func, inst, path, false, true, state);
        }
        for &path in &access.writes {
            self.access(func, inst, path, true, false, state);
        }
        for &path in &access.transient_writes {
            self.access(func, inst, path, true, true, state);
        }
    }
}

/// A reported problem.
#[derive(Clone, Debug)]
pub(crate) struct Finding {
    /// The kind of problem, used to report each kind once per entry.
    pub(crate) category: &'static str,
    /// Diagnostic message.
    pub(crate) message: String,
    /// Primary location.
    pub(crate) span: Span,
    /// Secondary locations and their explanations.
    pub(crate) notes: Vec<(Span, String)>,
    /// A line for fact dumps.
    pub(crate) summary: String,
}

/// Contract-level trust facts derived from summaries.
struct Model {
    owner_only: FxHashSet<FunctionId>,
    privileged: BTreeSet<Origin>,
    facts: Rc<ModuleFacts>,
}

/// Runs the reentrancy analysis over `module` and returns its engine and findings.
pub(crate) fn analyze_module<'m>(
    module: &'m Module,
    policy: ContextPolicy,
    evm_version: EvmVersion,
) -> (SummaryEngine<'m, ReentrancyAnalysis<'m>>, Vec<Finding>) {
    let mut taint = super::taint::analyze_module(module, policy);
    let entries = runtime_entries(module);
    let facts = module_facts(module, &mut taint.analysis.storage, &entries, evm_version);
    let analysis = ReentrancyAnalysis {
        taint,
        facts: Rc::new(facts),
        inst_facts: FxHashMap::default(),
        evm_version,
    };
    let mut engine = SummaryEngine::new(module, analysis, ContextPolicy::INSENSITIVE);
    for (func, function) in module.iter_functions() {
        if !function.blocks.is_empty() {
            let context = engine.general_context(func);
            let _ = engine.summary(func, &context);
        }
    }
    let model = build_model(&mut engine);
    let findings = check(&mut engine, &model);
    (engine, findings)
}

pub(crate) fn runtime_entries(module: &Module) -> Vec<FunctionId> {
    module
        .iter_functions()
        .filter(|(_, function)| {
            !function.blocks.is_empty()
                && !function.attributes.is_constructor
                && (function.selector.is_some()
                    || function.attributes.is_fallback
                    || function.attributes.is_receive)
        })
        .map(|(id, _)| id)
        .collect()
}

pub(crate) fn module_facts(
    module: &Module,
    storage: &mut SummaryEngine<'_, super::storage::StorageAnalysis>,
    entries: &[FunctionId],
    evm_version: EvmVersion,
) -> ModuleFacts {
    let mut facts = ModuleFacts { entries: entries.to_vec(), ..ModuleFacts::default() };
    for &entry in entries {
        let context = storage.general_context(entry);
        let summary = storage.summary(entry, &context);
        let table = &mut storage.analysis.table;
        for (transient, paths) in [(false, &summary.writes), (true, &summary.transient_writes)] {
            for &path in paths {
                match table.as_slot(path) {
                    Some(slot) => {
                        facts.runtime_written.insert(SlotKey { transient, slot });
                    }
                    None => facts.runtime_writes_unknown |= !table.is_hashed(path),
                }
            }
        }
        let writes = summary
            .writes
            .iter()
            .chain(&summary.transient_writes)
            .map(|&path| table.erase_keys(path))
            .collect::<Vec<_>>();
        facts.entry_writes.insert(entry, writes);
        if matches!(
            module.function(entry).attributes.state_mutability,
            StateMutability::View | StateMutability::Pure
        ) {
            facts.views.insert(entry);
        }
    }

    // Constructor-only code decides immutables and initial slot contents.
    let graph = CallGraphInfo::new(module);
    let constructors = module
        .iter_functions()
        .filter(|(_, function)| function.attributes.is_constructor)
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    let mut init = graph.reachable_callees_from(constructors.iter().copied());
    for &constructor in &constructors {
        init.insert(constructor);
    }
    let runtime = graph.reachable_callees_from(entries.iter().copied());
    for func in init.iter() {
        if runtime.contains(func) && !constructors.contains(&func) {
            continue;
        }
        let function = module.function(func);
        for inst in function.instructions() {
            match function.inst(inst).kind {
                InstKind::StoreImmutable(id, value) => {
                    let trust = init_value_trust(module, function, value, evm_version);
                    let entry = facts.immutables.entry(id).or_insert(trust);
                    *entry = (*entry).max(trust);
                }
                InstKind::SStore(slot, value) => {
                    if let Some(slot) = function.value_u256(slot) {
                        let value = stored_field(function, value);
                        let trust = init_value_trust(module, function, value, evm_version);
                        let entry = facts.constructor_slots.entry(slot).or_insert(trust);
                        *entry = (*entry).max(trust);
                    }
                }
                _ => {}
            }
        }
    }
    facts
}

fn build_model(engine: &mut SummaryEngine<'_, ReentrancyAnalysis<'_>>) -> Model {
    let facts = Rc::clone(&engine.analysis.facts);
    let entries = facts.entries.as_slice();
    let entry_writes = &facts.entry_writes;
    let immutables = &facts.immutables;
    // Owner-only entries and privileged origins form a greatest fixed point: an owner slot
    // that only the owner can change stays privileged. Start from every origin a guard
    // compares `caller` with and drop origins whose slot another entry can write.
    let summaries = entries
        .iter()
        .map(|&entry| {
            let context = engine.general_context(entry);
            (entry, engine.summary(entry, &context))
        })
        .collect::<Vec<_>>();
    let mut privileged =
        immutables.keys().map(|&id| Origin::Immutable(id)).collect::<BTreeSet<_>>();
    for (_, summary) in &summaries {
        let guards = summary
            .accesses
            .iter()
            .map(|event| &event.guard)
            .chain(summary.calls.iter().map(|event| &event.guard));
        for guard in guards {
            privileged.extend(guard.caller_is.iter().copied());
        }
    }
    loop {
        let owner_only = owner_only_entries(&summaries, &privileged);
        let table = engine.analysis.table();
        let next = privileged
            .iter()
            .copied()
            .filter(|&origin| match origin {
                Origin::Immutable(_) | Origin::Constant(_) => true,
                Origin::Slot { slot, .. } if slot.transient => false,
                Origin::Slot { slot, .. } => {
                    let path = table.slot(slot.slot);
                    entries.iter().all(|entry| {
                        owner_only.contains(entry)
                            || entry_writes[entry].iter().all(|&written| {
                                !table.may_alias(written, path, Activation::Different)
                            })
                    })
                }
            })
            .collect::<BTreeSet<_>>();
        if next == privileged {
            return Model { owner_only, privileged, facts };
        }
        privileged = next;
    }
}

/// Returns the entries whose every write and call requires a privileged caller.
fn owner_only_entries(
    summaries: &[(FunctionId, ReentrancySummary)],
    privileged: &BTreeSet<Origin>,
) -> FxHashSet<FunctionId> {
    let guarded = |guard: &Guard| guard.caller_is.iter().any(|origin| privileged.contains(origin));
    summaries
        .iter()
        .filter(|(_, summary)| {
            let mut writes = summary.accesses.iter().filter(|event| event.write).peekable();
            let has_effects = writes.peek().is_some() || !summary.calls.is_empty();
            has_effects
                && writes.all(|event| guarded(&event.guard))
                && summary.calls.iter().all(|event| guarded(&event.guard))
        })
        .map(|&(entry, _)| entry)
        .collect()
}

/// Peels a read-modify-write of a packed field down to the stored field value.
fn stored_field(func: &Function, value: ValueId) -> ValueId {
    if let Value::Inst(inst) = func.value(value)
        && let InstKind::Or(a, b) = func.inst(*inst).kind
    {
        let is_preserved = |value: ValueId| {
            matches!(func.value(value), Value::Inst(inst)
                if matches!(func.inst(*inst).kind, InstKind::And(x, _)
                    if matches!(func.value(x), Value::Inst(load) if matches!(func.inst(*load).kind, InstKind::SLoad(_)))))
        };
        if is_preserved(a) {
            return b;
        }
        if is_preserved(b) {
            return a;
        }
    }
    value
}

/// Classifies a value assigned during construction.
fn init_value_trust(
    module: &Module,
    func: &Function,
    value: ValueId,
    evm_version: EvmVersion,
) -> Trust {
    let mut value = value;
    for _ in 0..16 {
        match func.value(value) {
            Value::Immediate(imm) => {
                return match imm.as_u256() {
                    Some(address) if address.is_zero() || is_precompile(address) => {
                        Trust::NoCallback
                    }
                    _ => Trust::Privileged,
                };
            }
            // Constructor arguments are chosen by the deployer.
            Value::Arg(_) => return Trust::Privileged,
            Value::Undef(_) | Value::Error(_) => return Trust::Untrusted,
            Value::Inst(inst) => match func.inst(*inst).kind {
                InstKind::Zext(x) | InstKind::Trunc(x, _) | InstKind::Bitcast(x) => value = x,
                InstKind::And(x, y) if func.value_u256(y).is_some() => value = x,
                InstKind::And(x, y) if func.value_u256(x).is_some() => value = y,
                InstKind::Caller | InstKind::Origin => return Trust::Privileged,
                InstKind::Address => return Trust::Known,
                InstKind::Create(_, offset, _) | InstKind::Create2(_, offset, _, _) => {
                    return match created_provenance(module, func, offset, evm_version) {
                        Provenance::Created { calls_out: false } => Trust::NoCallback,
                        Provenance::Created { calls_out: true } => Trust::Known,
                        _ => Trust::Untrusted,
                    };
                }
                InstKind::CalldataLoad(_) => return Trust::Privileged,
                _ => return Trust::Untrusted,
            },
        }
    }
    Trust::Untrusted
}

impl Model {
    fn trust(
        &self,
        engine: &mut SummaryEngine<'_, ReentrancyAnalysis<'_>>,
        target: Provenance,
    ) -> Trust {
        if let Some(trust) = self.facts.code_trust(engine.analysis.table(), target) {
            return trust;
        }
        match target {
            Provenance::Constant(_) => Trust::Privileged,
            Provenance::Storage(path) => {
                // Written at runtime: trusted only when every writer is owner-only.
                let erased = engine.analysis.table().erase_keys(path);
                let facts = &self.facts;
                let table = engine.analysis.table();
                let all_owner_only = facts.entries.iter().all(|entry| {
                    self.owner_only.contains(entry)
                        || facts.entry_writes[entry].iter().all(|&written| {
                            !table.may_alias(written, erased, Activation::Different)
                        })
                });
                if all_owner_only { Trust::Privileged } else { Trust::Untrusted }
            }
            _ => Trust::Untrusted,
        }
    }
}

/// Builds the state a reentrant activation starts in.
fn state_at_call(call: &CallEvent) -> SlotState {
    SlotState {
        slots: call.slots.clone(),
        clobber: call.clobber,
        guard: call.guard.clone(),
        values: FxHashMap::default(),
        calls: BTreeSet::new(),
    }
}

fn check(engine: &mut SummaryEngine<'_, ReentrancyAnalysis<'_>>, model: &Model) -> Vec<Finding> {
    let module = engine.module;
    let mut findings = Vec::new();
    let mut reported_tod = FxHashSet::default();
    // One finding of each kind per entry keeps reports about one function together.
    let mut reported = FxHashSet::default();
    let summaries = model
        .facts
        .entries
        .iter()
        .map(|&entry| {
            let context = engine.general_context(entry);
            (entry, engine.summary(entry, &context))
        })
        .collect::<Vec<_>>();
    for (entry, summary) in &summaries {
        if model.owner_only.contains(entry) {
            continue;
        }
        let name = module.function(*entry).name;
        for call in &summary.calls {
            if call.guard.caller_is.iter().any(|origin| model.privileged.contains(origin)) {
                continue;
            }
            let trust = model.trust(engine, call.target);
            // Stipend transfers cannot write storage and are not reentrancy vectors. Targets
            // chosen by privileged accounts still run their own code, and tokens with transfer
            // hooks call back even when their address is trusted.
            let reentrant = call.kind != CallKind::Stipend && trust != Trust::NoCallback;
            if reentrant
                && let Some(finding) =
                    check_call(engine, model, *entry, summary, call, trust, &summaries)
                && reported.insert((*entry, finding.category))
            {
                findings.push(finding);
            }
            // Transaction-order dependence of the transferred value or its recipient.
            if call.sends_value
                && call.kind != CallKind::StaticCall
                && !reported_tod.contains(entry)
            {
                'sources: for (what, taint) in
                    [("value", &call.amount_taint), ("recipient", &call.recipient_taint)]
                {
                    for source in taint.iter() {
                        let Source::Storage(path) = source else { continue };
                        let erased = engine.analysis.table().erase_keys(path);
                        // Privileged writers still front-run, as in Sailfish's owner-set prices.
                        // Entries keyed by the sender belong to one account, which cannot be
                        // front-run by another sender's transaction.
                        let writer = model.facts.entries.iter().copied().find(|&writer| {
                            writer != *entry
                                && model.facts.entry_writes[&writer].iter().any(|&written| {
                                    engine.analysis.table().may_alias(
                                        written,
                                        erased,
                                        Activation::DifferentSenders,
                                    )
                                })
                        });
                        let Some(writer) = writer else { continue };
                        reported_tod.insert(*entry);
                        let table = &engine.analysis.taint.analysis.storage.analysis.table;
                        let writer_name = module.function(writer).name;
                        findings.push(Finding {
                            category: "tod",
                            message: format!(
                                "possible transaction-order dependence: the {what} of a transfer in `{name}` depends on storage written by `{writer_name}`"
                            ),
                            span: call.span,
                            notes: Vec::new(),
                            summary: format!(
                                "tod @{name} call#{}.{}: {what} depends on {} written by @{writer_name}",
                                call.id.func.index(),
                                call.id.inst.index(),
                                table.display(path, None)
                            ),
                        });
                        break 'sources;
                    }
                }
            }
        }
    }
    findings
}

fn check_call(
    engine: &mut SummaryEngine<'_, ReentrancyAnalysis<'_>>,
    model: &Model,
    entry: FunctionId,
    summary: &ReentrancySummary,
    call: &CallEvent,
    trust: Trust,
    summaries: &[(FunctionId, ReentrancySummary)],
) -> Option<Finding> {
    let module = engine.module;
    let at_call = state_at_call(call);
    // Report stale reads of writes after the call before destructive writes.
    let mut after =
        summary.accesses.iter().filter(|event| event.calls.contains(&call.id)).collect::<Vec<_>>();
    // A write whose path was read before the call is the classic check-then-act order.
    let read_before = |path: PathId| {
        summary
            .accesses
            .iter()
            .any(|event| !event.write && !event.calls.contains(&call.id) && event.path == path)
    };
    after.sort_by_key(|event| (!event.write, !read_before(event.path)));
    let logs_after = summary.logs.iter().any(|event| event.calls.contains(&call.id));
    let static_call = call.kind == CallKind::StaticCall;
    let table = &engine.analysis.taint.analysis.storage.analysis.table;
    let name = module.function(entry).name;
    // Prefer explaining view functions, then other entries, then the caller itself.
    let mut order = summaries.iter().collect::<Vec<_>>();
    order.sort_by_key(|(reentered, _)| {
        (!model.facts.views.contains(reentered), *reentered == entry, reentered.index())
    });
    for (reentered, reentered_summary) in order {
        if model.owner_only.contains(reentered) {
            continue;
        }
        let feasible = |guard: &Guard| {
            !guard.caller_is.iter().any(|origin| model.privileged.contains(origin))
                && compose_guard(guard, &at_call).is_some()
        };
        // A reentrant call that must revert commits nothing.
        let commits = reentered_summary.exit.as_ref().is_some_and(|exit| feasible(&exit.guard))
            || reentered_summary.success.as_ref().is_some_and(feasible);
        if !commits {
            continue;
        }
        let mut accesses = reentered_summary
            .accesses
            .iter()
            .filter(|event| feasible(&event.guard))
            .collect::<Vec<_>>();
        accesses.sort_by_key(|event| event.write);
        // A static context reverts at the first write, so only write-free entries return.
        if static_call && accesses.iter().any(|event| event.write) {
            continue;
        }
        // The interleaving is equivalent to running the reentrant entry first when it neither
        // reads what the caller wrote before the call nor writes what the caller read then.
        let before_call = |event: &&AccessEvent| !event.calls.contains(&call.id);
        let conflicts = |a: &AccessEvent, b: &AccessEvent| {
            a.transient == b.transient && table.may_alias(a.path, b.path, Activation::Different)
        };
        let serializable_first = !summary.accesses.iter().filter(before_call).any(|mine| {
            accesses.iter().any(|theirs| (mine.write || theirs.write) && conflicts(mine, theirs))
        });
        let reentered_name = module.function(*reentered).name;
        let view = model.facts.views.contains(reentered);
        // A view observes an inconsistent state only if it also reads something the caller
        // already updated before the call, as when a price mixes new assets and old shares.
        let sees_update = |early: &AccessEvent| {
            early.path != PathTable::UNKNOWN
                && summary.accesses.iter().any(|written| {
                    written.write
                        && !written.calls.contains(&call.id)
                        && written.path != PathTable::UNKNOWN
                        && written.transient == early.transient
                        && table.may_alias(written.path, early.path, Activation::Different)
                })
        };
        let inconsistent = accesses.iter().any(|early| !early.write && sees_update(early));
        // A stale read matters only if the reentrant call can act on it: a view returns what
        // it read, and other functions need a feasible write, call, or event.
        let acts = view
            || accesses.iter().any(|event| event.write)
            || reentered_summary.calls.iter().any(|event| feasible(&event.guard))
            || reentered_summary.logs.iter().any(|event| feasible(&event.guard));
        for late in after.iter().filter(|_| !serializable_first) {
            for early in &accesses {
                if !(late.write || early.write)
                    || (!early.write && !acts)
                    || late.transient != early.transient
                    || !table.may_alias(late.path, early.path, Activation::Different)
                {
                    continue;
                }
                let read_only = view && !early.write;
                if read_only
                    && (!inconsistent
                        || late.path == PathTable::UNKNOWN
                        || early.path == PathTable::UNKNOWN)
                {
                    continue;
                }
                let kind = match call.kind {
                    CallKind::DelegateCall | CallKind::CallCode => "delegatecall",
                    CallKind::Create => "contract creation",
                    _ if read_only => "read-only",
                    _ if trust == Trust::Known => "cross-contract",
                    _ if *reentered == entry => "single-function",
                    _ => "cross-function",
                };
                let hazard = if early.write { "destructive write" } else { "stale read" };
                let late_verb = if late.write { "writes" } else { "reads" };
                let early_verb = if early.write { "write" } else { "read" };
                let path = table.display(late.path, None).to_string();
                return Some(Finding {
                    category: kind,
                    message: format!(
                        "possible {kind} reentrancy: `{name}` {late_verb} storage after an external call"
                    ),
                    span: call.span,
                    notes: vec![
                        (late.span, format!("`{name}` {late_verb} `{path}` after the call")),
                        (
                            early.span,
                            format!(
                                "a reentrant call to `{reentered_name}` can {early_verb} it here ({hazard})"
                            ),
                        ),
                    ],
                    summary: format!(
                        "reentrancy {kind} @{name} call#{}.{}: {late_verb} {path} after the call; reentrant @{reentered_name} {early_verb}s {} ({hazard})",
                        call.id.func.index(),
                        call.id.inst.index(),
                        table.display(early.path, None)
                    ),
                });
            }
        }
        if logs_after
            && !static_call
            && reentered_summary.logs.iter().any(|event| feasible(&event.guard))
        {
            return Some(Finding {
                category: "event-ordering",
                message: format!(
                    "possible event reordering: `{name}` emits an event after an external call"
                ),
                span: call.span,
                notes: Vec::new(),
                summary: format!(
                    "event-ordering @{name} call#{}.{}: reentrant @{reentered_name} emits events",
                    call.id.func.index(),
                    call.id.inst.index()
                ),
            });
        }
    }
    None
}

/// Writes per-instruction reentrancy facts, summaries, and findings.
pub(crate) fn dump(
    engine: &SummaryEngine<'_, ReentrancyAnalysis<'_>>,
    findings: &[Finding],
    out: &mut String,
) {
    let module = engine.module;
    let facts = &engine.analysis.inst_facts;
    {
        for (func, _, summary) in engine.final_summaries() {
            let function = module.function(func);
            let _ = writeln!(out, "fn @{}:", function.name);
            let cfg = crate::mir::analysis::CfgInfo::new(function);
            if let Some(func_facts) = facts.get(&func) {
                for &block in cfg.rpo() {
                    let mut labeled = false;
                    for &inst in &function.blocks[block].instructions {
                        let Some(fact) = func_facts.get(&inst) else { continue };
                        if !labeled {
                            let _ = writeln!(out, "  bb{}:", block.index());
                            labeled = true;
                        }
                        let _ = writeln!(
                            out,
                            "    {}  ; {fact}",
                            crate::mir::display::display_instruction(function, Some(module), inst)
                        );
                    }
                }
            }
            match &summary.exit {
                Some(exit) => {
                    let _ = write!(out, "  exit:{}", display_slots(&exit.slots));
                    let guard = exit.guard.display().to_string();
                    if !guard.is_empty() {
                        let _ = write!(out, " requires {guard}");
                    }
                    if exit.clobber != Clobber::None {
                        let _ = write!(out, " clobbers");
                    }
                    let _ = writeln!(out);
                }
                None => {
                    let _ = writeln!(out, "  exit: never returns");
                }
            }
        }
    }
    for finding in findings {
        let _ = writeln!(out, "finding: {}", finding.summary);
    }
}
