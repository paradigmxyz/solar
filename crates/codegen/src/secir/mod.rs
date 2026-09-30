//! Security facts derived from MIR (SecIR).
//!
//! SecIR is a read-only view over a contract's semantic MIR, intended for security tooling such as
//! fuzzers, invariant generators, and static analyzers. It answers questions that are hard to
//! recover from source text or bytecode alone:
//!
//! - which storage slots each function reads and writes, which values key mapping and array
//!   accesses (for example, `balances[msg.sender]`), and what the written values depend on;
//! - which external calls a function makes, their ABI selector and arguments, what their target and
//!   value depend on, whether they can send value, and whether a low-level call's success flag is
//!   ignored;
//! - which conditions guard a revert, which other conditions steer control flow, and what those
//!   conditions depend on (`msg.sender`, `tx.origin`, arguments, storage, call results, block
//!   values, ...);
//! - which values every successful execution has checked on the way out, and so whether a function
//!   is access controlled; each storage write, call, event, and `selfdestruct` records whether a
//!   check on the caller dominates it;
//! - which events a function emits and what each event argument depends on;
//! - which effects run inside a loop or after an external call, and which slots are read before and
//!   written after an external call;
//! - which constants the source compares values against, for fuzzing dictionaries;
//! - which arithmetic was written in an `unchecked` or inline assembly block, and a few local
//!   hazard patterns (strict equality on balances or block values, division before multiplication,
//!   modulo of block values, `msg.value` in a loop);
//! - which storage, calls, events, and `selfdestruct`s each function reaches transitively through
//!   internal calls, with callee dependencies rewritten in terms of the caller's own values;
//! - for each storage slot, which entry points write it, from what, and whether an unguarded entry
//!   point can store externally controlled data in it.
//!
//! Facts are computed from the MIR built by [`lower_contract`](crate::mir::lower::lower_contract)
//! before any optimization or representation-lowering pass runs, so they describe the program as
//! written, independent of the optimization mode. Each fact carries the source span recorded by
//! lowering when one is available.
//!
//! # Dependencies
//!
//! A value's dependencies are the [`Source`]s it is computed from. They follow SSA operands and
//! phis to a fixpoint. A storage load depends on the loaded slot and, through
//! [`Source::KeyedByCaller`] and [`Source::KeyedByArgument`], on the caller or arguments that
//! select a mapping entry or array element. Internal calls are summarized: a call's result depends
//! on what the callee's return values depend on, with the callee's arguments replaced by the
//! caller's argument values. Callee effects reported in [`FunctionFacts::summary`] are rewritten
//! the same way, so a `transfer` inside `_send(to, amount)` called as `_send(msg.sender, balance)`
//! is reported to `msg.sender` in the caller's summary.
//!
//! Dependencies through storage cross transactions: [`ContractFacts::storage_flows`] records, per
//! slot, what each entry point writes into it. A slot is externally controlled when an entry point
//! that is not access controlled writes a value depending on its arguments, calldata, `msg.sender`,
//! `tx.origin`, `msg.value`, or another externally controlled slot. External calls whose target
//! depends on an argument, calldata, or such a slot are marked
//! [`target_controlled`](ExternalCall::target_controlled).
//!
//! # Checks and access control
//!
//! A revert guard (a `require`, a checked `revert_if`, or a branch whose other side always reverts)
//! checks its condition's sources at every point it dominates. An internal call checks whatever the
//! callee checks on every successful exit, so helpers such as `_checkOwner()` count. The sources
//! checked at every successful exit are [`FunctionFacts::entry_checks`]; a function is access
//! controlled when they include `msg.sender` or `tx.origin`, or a storage entry selected by
//! `msg.sender` ([`Source::KeyedByCaller`], as in `admins[msg.sender]` or a `hasRole(role,
//! msg.sender)` helper) without also depending on arguments, calldata, or `msg.value`, which
//! would indicate a balance check. An effect is `guarded` when such a caller check dominates it,
//! directly or at the internal call that reaches it.
//!
//! # Precision
//!
//! The analysis is best-effort and flow-insensitive for values:
//!
//! - Values loaded from memory are reported as [`Source::Memory`] plus the dependencies of the
//!   address they are loaded from; stored values are not tracked through memory, so decoded return
//!   data and memory structs are approximated. Event data stored to scratch memory right before a
//!   `log` is the one memory round-trip that is followed.
//! - Writes to variables narrower than a word merge the value into the slot's other bits; the
//!   preserved bits are not reported as part of the written value.
//! - Storage slots are resolved through constant slots, mapping and array slot derivations, and
//!   constant offsets. Slots computed any other way, such as in inline assembly, are reported as
//!   [`StorageSlot::Unknown`]. All entries of a mapping or array share one [`StorageSlot::Derived`]
//!   slot, and variables packed into one slot share it too.
//! - Checks are control-flow facts: a check on a local copy of `msg.sender` counts, but a check
//!   that only returns early without reverting does not.
//! - "After an external call" means reachable in the control-flow graph from an external call,
//!   including through internal calls that transitively make one. It does not prove that an
//!   exploitable reentrancy exists.
//! - Loops are control-flow cycles within one function; recursion is not treated as a loop.
//! - Constants come from source comparisons, in the function and its modifiers, whose other side is
//!   not constant. Negative values are reported as two's-complement words.
//! - Dependencies are data dependencies only. A value selected by control flow, such as the result
//!   of `a || b`, does not depend on the conditions that selected it.
//! - Hazards are local syntactic patterns over MIR values, not proofs of a bug.
//!
//! Consumers should treat facts as hints for search and triage, not as proofs.

use crate::mir::{
    self, AddressCallKind, BlockId, Builtin, Callee, CheckedOp, EffectKind, Function, FunctionId,
    InstId, InstKind, MirType, Module, RevertKind, Terminator, Value, ValueId, analysis::CfgInfo,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::{
    Never,
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};
use solar_interface::{Span, Symbol};
use solar_sema::{
    Gcx,
    hir::{self, ContractId, StateMutability, Visibility, Visit},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::ControlFlow,
};

mod display;

/// Security facts for one contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractFacts {
    /// The contract name.
    pub name: Symbol,
    /// State variables visible to the contract, ordered by storage location.
    pub state_variables: Vec<StateVariableFacts>,
    /// Facts for each function in the contract's MIR module, in module order.
    pub functions: Vec<FunctionFacts>,
    /// What entry points read and write in each storage slot, ordered by slot.
    pub storage_flows: Vec<SlotFlow>,
    /// Whether a payable entry point other than the constructor can receive value.
    pub receives_value: bool,
    /// Whether an entry point can send value out through a call, `selfdestruct`, or a
    /// `delegatecall`/`callcode` that runs other code with this contract's balance.
    pub sends_value: bool,
}

impl ContractFacts {
    /// Returns whether the contract can receive value but has no way to send it out.
    pub fn locks_value(&self) -> bool {
        self.receives_value && !self.sends_value
    }

    /// Returns the declared state variables that no entry point writes and that have no
    /// initializer, which is run by the constructor.
    ///
    /// Packed variables share a slot, so writing one counts as writing the others.
    pub fn unwritten_state_variables(&self) -> impl Iterator<Item = &StateVariableFacts> {
        self.state_variables.iter().filter(|variable| {
            !variable.transient
                && !self
                    .storage_flows
                    .iter()
                    .any(|flow| flow.slot.base() == Some(variable.slot) && !flow.writers.is_empty())
        })
    }
}

/// The storage location of a state variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateVariableFacts {
    /// The variable name.
    pub name: Option<Symbol>,
    /// The storage slot.
    pub slot: U256,
    /// The byte offset of the variable inside its slot.
    pub offset: u8,
    /// Whether the variable lives in transient storage.
    pub transient: bool,
    /// The declaration span.
    pub span: Span,
}

/// What entry points read from and write into one storage slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotFlow {
    /// The slot. Mapping and array entries share their declared slot's [`StorageSlot::Derived`].
    pub slot: StorageSlot,
    /// Entry points that read the slot, directly or through internal calls, as indices into
    /// [`ContractFacts::functions`].
    pub readers: Vec<usize>,
    /// Entry points that write the slot, directly or through internal calls.
    pub writers: Vec<SlotWriter>,
    /// Whether an entry point that is neither the constructor nor guarded by a caller check can
    /// store a value that depends on its arguments, calldata, `msg.sender`, `tx.origin`,
    /// `msg.value`, or another externally controlled slot.
    pub externally_controlled: bool,
}

/// An entry point that writes a storage slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotWriter {
    /// The writing entry point, as an index into [`ContractFacts::functions`].
    pub function: usize,
    /// What the written values depend on, in terms of the entry point's own values.
    pub value: BTreeSet<Source>,
    /// Whether every write is guarded by a caller check.
    pub guarded: bool,
}

/// The role of a function in the contract's interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FunctionKind {
    /// A function reachable through the ABI dispatcher.
    External,
    /// The constructor.
    Constructor,
    /// The fallback function.
    Fallback,
    /// The receive function.
    Receive,
    /// A function only reachable through internal calls.
    Internal,
}

/// Security facts for one function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionFacts {
    /// The MIR function name.
    pub name: String,
    /// The role of the function.
    pub kind: FunctionKind,
    /// The ABI selector, for externally callable functions.
    pub selector: Option<[u8; 4]>,
    /// The declared visibility.
    pub visibility: Visibility,
    /// The declared state mutability.
    pub state_mutability: StateMutability,
    /// The span of the function name.
    pub span: Span,
    /// Names of the modifiers applied to the function, in source order.
    pub modifiers: Vec<Symbol>,
    /// Sources that a revert guard has checked before every successful exit.
    ///
    /// For example, a function with `require(msg.sender == owner)` at its start checks
    /// `msg.sender` and `owner`.
    pub entry_checks: BTreeSet<Source>,
    /// What the function's return values depend on.
    pub returns: BTreeSet<Source>,
    /// Storage reads made directly by this function.
    pub storage_reads: Vec<StorageAccess>,
    /// Storage writes made directly by this function.
    pub storage_writes: Vec<StorageAccess>,
    /// External calls and contract creations made directly by this function.
    pub external_calls: Vec<ExternalCall>,
    /// Internal calls made directly by this function.
    pub internal_calls: Vec<InternalCall>,
    /// Conditions that make this function revert or panic.
    pub guards: Vec<Guard>,
    /// Conditional branches that do not directly revert, including loop conditions.
    pub branches: Vec<Branch>,
    /// Events emitted directly by this function.
    pub events: Vec<EventEmission>,
    /// `selfdestruct`s executed directly by this function.
    pub self_destructs: Vec<SelfDestruct>,
    /// Constants that the source compares non-constant values against.
    pub constants: BTreeSet<U256>,
    /// Wrapping arithmetic written in `unchecked` or inline assembly blocks.
    pub unchecked_arithmetic: Vec<Span>,
    /// Local patterns that often indicate a bug.
    pub hazards: Vec<Hazard>,
    /// Effects reachable through this function and its internal callees.
    pub summary: EffectSummary,
}

impl FunctionFacts {
    /// Returns whether every successful execution checks `msg.sender` or `tx.origin`.
    pub fn is_access_controlled(&self) -> bool {
        is_caller_check(&self.entry_checks)
    }

    /// Returns whether the function or a callee writes storage without emitting any event.
    pub fn writes_without_event(&self) -> bool {
        !self.summary.storage_writes.is_empty() && self.summary.events.is_empty()
    }

    /// Returns the sources read by revert guards and branch conditions.
    pub fn condition_sources(&self) -> BTreeSet<Source> {
        let guards = self.guards.iter().filter(|guard| guard.kind == GuardKind::Revert);
        let guards = guards.flat_map(|guard| guard.sources.iter().copied());
        guards
            .chain(self.branches.iter().flat_map(|branch| branch.sources.iter().copied()))
            .collect()
    }
}

/// Effects reachable through a function and its internal callees.
///
/// Callee facts keep their own spans. Their dependencies are rewritten in terms of the caller's
/// values, and they are marked guarded, in a loop, or after an external call when the internal
/// call that reaches them is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EffectSummary {
    /// Storage reads.
    pub storage_reads: Vec<StorageAccess>,
    /// Storage writes.
    pub storage_writes: Vec<StorageAccess>,
    /// External calls and contract creations.
    pub external_calls: Vec<ExternalCall>,
    /// Emitted events.
    pub events: Vec<EventEmission>,
    /// Executed `selfdestruct`s.
    pub self_destructs: Vec<SelfDestruct>,
}

impl EffectSummary {
    /// Returns whether any reachable call can transfer a nonzero value.
    pub fn sends_value(&self) -> bool {
        self.external_calls.iter().any(|call| call.sends_value)
    }

    /// Returns the slots that can be read before an external call and written after one, the
    /// pattern behind most reentrancy bugs.
    pub fn reentrancy_slots(&self) -> BTreeSet<StorageSlot> {
        let read_before = self
            .storage_reads
            .iter()
            .filter(|access| !access.after_external_call)
            .map(|access| access.slot)
            .collect::<BTreeSet<_>>();
        self.storage_writes
            .iter()
            .filter(|access| access.after_external_call && read_before.contains(&access.slot))
            .map(|access| access.slot)
            .collect()
    }

    /// Returns the storage writes that can execute after an external call.
    pub fn writes_after_external_call(&self) -> impl Iterator<Item = &StorageAccess> {
        self.storage_writes.iter().filter(|access| access.after_external_call)
    }
}

/// A storage slot, resolved to the declared slot it derives from where possible.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StorageSlot {
    /// A known absolute slot.
    Exact(U256),
    /// A slot derived from a declared slot through mapping keys, array indices, or offsets.
    Derived(U256),
    /// A slot whose derivation could not be resolved.
    Unknown,
}

impl StorageSlot {
    /// Returns the declared slot this slot derives from, if known.
    pub const fn base(self) -> Option<U256> {
        match self {
            Self::Exact(slot) | Self::Derived(slot) => Some(slot),
            Self::Unknown => None,
        }
    }
}

/// A storage access.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageAccess {
    /// The accessed slot.
    pub slot: StorageSlot,
    /// What the mapping keys and array indices used to derive the slot depend on.
    pub keys: BTreeSet<Source>,
    /// What the written value depends on. Empty for reads and constant values.
    pub value: BTreeSet<Source>,
    /// Whether the access is to transient storage.
    pub transient: bool,
    /// Whether a check on `msg.sender` or `tx.origin` dominates the access.
    pub guarded: bool,
    /// Whether the access is inside a loop.
    pub in_loop: bool,
    /// Whether the access can execute after an external call.
    pub after_external_call: bool,
    /// The source span of the access.
    pub span: Option<Span>,
}

/// A value that another value can depend on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Source {
    /// `msg.sender`.
    Caller,
    /// `tx.origin`.
    Origin,
    /// `msg.value`.
    CallValue,
    /// A function argument, by index.
    Argument(u32),
    /// A value loaded from storage.
    Storage(StorageSlot),
    /// A value loaded from transient storage.
    TransientStorage(StorageSlot),
    /// A storage value selected by a mapping key or array index that depends on `msg.sender` or
    /// `tx.origin`, such as `admins[msg.sender]`.
    KeyedByCaller,
    /// A storage value selected by a mapping key or array index that depends on an argument.
    ///
    /// Internal call summaries use it to recognize helpers such as `hasRole(role, account)`
    /// called with `msg.sender`.
    KeyedByArgument(u32),
    /// An immutable variable.
    Immutable,
    /// The success flag, return data, or returned value of an external call.
    CallResult,
    /// Raw calldata.
    Calldata,
    /// `block.timestamp`.
    Timestamp,
    /// `block.number`.
    BlockNumber,
    /// `block.prevrandao`, `blockhash`, or `blobhash`, which are often misused as randomness.
    Randomness,
    /// An account balance, such as `address(this).balance`.
    Balance,
    /// Other block, transaction, or account environment, such as `block.chainid` or
    /// `extcodesize`.
    Environment,
    /// A value loaded from memory, whose stored value is not tracked.
    Memory,
}

impl Source {
    /// Returns whether an external account chooses this value when calling an entry point.
    const fn is_external_input(self) -> bool {
        matches!(
            self,
            Self::Caller | Self::Origin | Self::CallValue | Self::Argument(_) | Self::Calldata
        )
    }
}

/// The kind of an external call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CallKind {
    /// `CALL`, including high-level calls and `address.call`.
    Call,
    /// `STATICCALL`.
    StaticCall,
    /// `DELEGATECALL`.
    DelegateCall,
    /// `CALLCODE`.
    CallCode,
    /// `address.transfer`.
    Transfer,
    /// `address.send`.
    Send,
    /// `CREATE`, including `new C()`.
    Create,
    /// `CREATE2`, including `new C{salt: s}()`.
    Create2,
}

/// An external call or contract creation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalCall {
    /// The kind of call.
    pub kind: CallKind,
    /// The ABI selector of a high-level call.
    pub selector: Option<[u8; 4]>,
    /// What the call target depends on. Empty for constant targets and contract creations.
    pub target: BTreeSet<Source>,
    /// What the transferred value depends on. Empty when no value can be sent.
    pub value: BTreeSet<Source>,
    /// What each ABI-encoded argument of a high-level call depends on, or what the payload of a
    /// low-level call depends on.
    pub args: Vec<BTreeSet<Source>>,
    /// Whether the call can transfer a nonzero value.
    pub sends_value: bool,
    /// Whether the target depends on an argument, calldata, or an externally controlled slot.
    pub target_controlled: bool,
    /// Whether the success flag of a low-level call or `send` is never used.
    pub result_unchecked: bool,
    /// Whether a check on `msg.sender` or `tx.origin` dominates the call.
    pub guarded: bool,
    /// Whether the call is inside a loop.
    pub in_loop: bool,
    /// Whether the call can execute after another external call.
    pub after_external_call: bool,
    /// The source span of the call.
    pub span: Option<Span>,
}

/// An internal call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InternalCall {
    /// The callee, as an index into [`ContractFacts::functions`].
    pub callee: usize,
    /// What each argument depends on.
    pub args: Vec<BTreeSet<Source>>,
    /// Whether a check on `msg.sender` or `tx.origin` dominates the call.
    pub guarded: bool,
    /// Whether the call is inside a loop.
    pub in_loop: bool,
    /// Whether the call can execute after an external call.
    pub after_external_call: bool,
    /// The source span of the call.
    pub span: Option<Span>,
}

/// How a guard stops execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GuardKind {
    /// A user-visible revert, such as `require`, `revert`, or a failed call check.
    Revert,
    /// A compiler-inserted panic, such as an arithmetic overflow check.
    Panic,
}

/// A condition that stops execution when it fails.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Guard {
    /// How the guard stops execution.
    pub kind: GuardKind,
    /// What the guarded condition depends on.
    pub sources: BTreeSet<Source>,
    /// The source span of the guard.
    pub span: Option<Span>,
}

impl Guard {
    /// Returns whether the guard depends on the caller or transaction origin, which usually
    /// indicates access control.
    pub fn is_access_control(&self) -> bool {
        is_caller_check(&self.sources)
    }
}

/// A conditional branch that does not directly revert.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    /// What the branch condition depends on.
    pub sources: BTreeSet<Source>,
    /// Whether the branch is inside a loop, including a loop's own condition.
    pub in_loop: bool,
    /// The source span of the branch.
    pub span: Option<Span>,
}

/// An event emission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventEmission {
    /// The event name, when the emission resolves to a declared event.
    pub name: Option<Symbol>,
    /// What each argument depends on, in declaration order.
    ///
    /// If the non-indexed arguments cannot be told apart, each of them reports the dependencies of
    /// the whole event data.
    pub args: Vec<BTreeSet<Source>>,
    /// Whether a check on `msg.sender` or `tx.origin` dominates the emission.
    pub guarded: bool,
    /// Whether the emission is inside a loop.
    pub in_loop: bool,
    /// Whether the emission can execute after an external call.
    pub after_external_call: bool,
    /// The source span of the emission.
    pub span: Option<Span>,
}

/// A `selfdestruct`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelfDestruct {
    /// What the beneficiary depends on.
    pub beneficiary: BTreeSet<Source>,
    /// Whether a check on `msg.sender` or `tx.origin` dominates the `selfdestruct`.
    pub guarded: bool,
    /// The source span of the `selfdestruct`.
    pub span: Option<Span>,
}

/// The kind of a [`Hazard`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HazardKind {
    /// `==` or `!=` on a balance, `block.timestamp`, or `block.number`, which an attacker can
    /// often make fail by moving the value past the compared constant.
    StrictEquality,
    /// A multiplication of a division result written in the source, which loses precision.
    DivideBeforeMultiply,
    /// A modulo of a value derived from block values, a common source of weak randomness.
    WeakRandomness,
    /// `msg.value` read inside a loop, where the same value is often counted more than once.
    CallValueInLoop,
}

/// A local pattern that often indicates a bug.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hazard {
    /// The pattern.
    pub kind: HazardKind,
    /// What the values involved in the pattern depend on.
    pub sources: BTreeSet<Source>,
    /// The source span of the pattern.
    pub span: Option<Span>,
}

/// Computes security facts for a contract's MIR module.
///
/// `module` should be the MIR built for `contract_id` before any pass pipeline runs. Modules that
/// have already been optimized or lowered produce facts that describe the transformed program and
/// may lose semantic operations, such as mapping slot derivations.
pub fn analyze(gcx: Gcx<'_>, contract_id: ContractId, module: &Module) -> ContractFacts {
    let state_variables = mir::lower::state_variable_slots(gcx, contract_id)
        .into_iter()
        .map(|slot| {
            let variable = gcx.hir.variable(slot.variable);
            StateVariableFacts {
                name: variable.name.map(|name| name.name),
                slot: slot.slot,
                offset: slot.offset,
                transient: slot.transient,
                span: variable.span,
            }
        })
        .collect();

    let source = SourceFacts::collect(gcx);
    let calls_out = calls_out(module);
    let returns = return_summaries(module);
    let mut analyses = module
        .functions
        .iter()
        .map(|function| FunctionAnalysis::new(function, &returns))
        .collect::<IndexVec<FunctionId, _>>();
    let entry_checks = entry_checks(&analyses);
    let mut functions = analyses
        .iter_mut_enumerated()
        .map(|(id, analysis)| {
            let checks = analysis.checks(&entry_checks);
            let mut facts = analysis.direct_facts(&source, &checks, &calls_out);
            facts.entry_checks = entry_checks[id].clone();
            facts.returns = returns[id].clone();
            facts.modifiers = source.modifiers(analysis.function);
            facts.constants = source.constants(analysis.function);
            facts
        })
        .collect::<IndexVec<FunctionId, _>>();

    let summaries = summarize(&functions);
    for (facts, summary) in functions.iter_mut().zip(summaries) {
        facts.summary = summary;
    }
    let storage_flows = storage_flows(&functions);
    let controlled = storage_flows
        .iter()
        .filter(|flow| flow.externally_controlled)
        .map(|flow| flow.slot)
        .collect::<BTreeSet<_>>();
    for facts in functions.iter_mut() {
        let calls = facts.external_calls.iter_mut().chain(&mut facts.summary.external_calls);
        for call in calls {
            call.target_controlled = call.target.iter().any(|source| match *source {
                Source::Storage(slot) | Source::TransientStorage(slot) => {
                    controlled.contains(&slot)
                }
                Source::Argument(_) | Source::Calldata => true,
                _ => false,
            });
        }
    }

    let entry_points = functions.iter().filter(|facts| facts.kind != FunctionKind::Internal);
    let receives_value = entry_points.clone().any(|facts| {
        facts.kind != FunctionKind::Constructor
            && facts.state_mutability == StateMutability::Payable
    });
    let sends_value = entry_points.clone().any(|facts| {
        let summary = &facts.summary;
        summary.sends_value()
            || !summary.self_destructs.is_empty()
            || summary
                .external_calls
                .iter()
                .any(|call| matches!(call.kind, CallKind::DelegateCall | CallKind::CallCode))
    });

    ContractFacts {
        name: module.name.name,
        state_variables,
        functions: functions.raw,
        storage_flows,
        receives_value,
        sends_value,
    }
}

/// Returns whether `sources` include the caller or transaction origin.
///
/// A check on storage selected by the caller, such as `admins[msg.sender]`, counts unless it also
/// depends on arguments, calldata, or `msg.value`, which usually indicates a balance check such as
/// `balances[msg.sender] >= amount`.
fn is_caller_check(sources: &BTreeSet<Source>) -> bool {
    sources.contains(&Source::Caller)
        || sources.contains(&Source::Origin)
        || (sources.contains(&Source::KeyedByCaller)
            && !sources.iter().any(|source| {
                matches!(source, Source::Argument(_) | Source::Calldata | Source::CallValue)
            }))
}

/// Returns the key dependencies of a storage value selected by keys with the given `sources`.
fn key_dependencies(sources: impl IntoIterator<Item = Source>) -> impl Iterator<Item = Source> {
    sources.into_iter().filter_map(|source| match source {
        Source::Caller | Source::Origin | Source::KeyedByCaller => Some(Source::KeyedByCaller),
        Source::Argument(index) | Source::KeyedByArgument(index) => {
            Some(Source::KeyedByArgument(index))
        }
        _ => None,
    })
}

/// Returns the callees of each internal call and tail call in `function`.
fn internal_callees(function: &Function) -> impl Iterator<Item = (FunctionId, &[ValueId])> {
    function.blocks.iter().flat_map(|block| {
        let calls = block.instructions.iter().filter_map(|&inst| match &function.inst(inst).kind {
            InstKind::ICall { function: Callee::Function(callee), args } => {
                Some((*callee, &args[..]))
            }
            _ => None,
        });
        let tail = match &block.terminator {
            Some(Terminator::TailCall { function: callee, args }) => Some((*callee, &args[..])),
            _ => None,
        };
        calls.chain(tail)
    })
}

/// Returns, for each function, whether it or a transitive internal callee makes an external call.
fn calls_out(module: &Module) -> IndexVec<FunctionId, bool> {
    let mut calls_out = module
        .functions
        .iter()
        .map(|function| {
            function.blocks.iter().any(|block| {
                block.instructions.iter().any(|&inst| external_call_kind(&function.inst(inst).kind))
            })
        })
        .collect::<IndexVec<FunctionId, _>>();
    let mut changed = true;
    while changed {
        changed = false;
        for (id, function) in module.functions.iter_enumerated() {
            if !calls_out[id] && internal_callees(function).any(|(callee, _)| calls_out[callee]) {
                calls_out[id] = true;
                changed = true;
            }
        }
    }
    calls_out
}

/// Computes what each function's return values depend on, to a fixpoint over internal calls.
fn return_summaries(module: &Module) -> IndexVec<FunctionId, BTreeSet<Source>> {
    let mut returns =
        module.functions.iter().map(|_| BTreeSet::new()).collect::<IndexVec<FunctionId, _>>();
    let mut changed = true;
    while changed {
        changed = false;
        for (id, function) in module.functions.iter_enumerated() {
            let sources = FunctionAnalysis::new(function, &returns).return_sources();
            if sources != returns[id] {
                returns[id] = sources;
                changed = true;
            }
        }
    }
    returns
}

/// Computes the sources each function checks before every successful exit, to a fixpoint over
/// internal calls.
fn entry_checks(
    analyses: &IndexVec<FunctionId, FunctionAnalysis<'_>>,
) -> IndexVec<FunctionId, BTreeSet<Source>> {
    let mut entry_checks =
        analyses.iter().map(|_| BTreeSet::new()).collect::<IndexVec<FunctionId, _>>();
    let mut changed = true;
    while changed {
        changed = false;
        for (id, analysis) in analyses.iter_enumerated() {
            let checks = analysis.checks(&entry_checks);
            let mut exits = analysis.exits().map(|point| checks.at(&analysis.cfg, point));
            let Some(first) = exits.next() else { continue };
            let checked = exits.fold(first, |checked, sources| &checked & &sources);
            if checked != entry_checks[id] {
                entry_checks[id] = checked;
                changed = true;
            }
        }
    }
    entry_checks
}

/// Rewrites callee `sources` in terms of the caller, given what each call argument depends on.
fn substitute(sources: &BTreeSet<Source>, args: &[BTreeSet<Source>]) -> BTreeSet<Source> {
    let mut substituted = BTreeSet::new();
    for &source in sources {
        match source {
            Source::Argument(index) => {
                substituted.extend(args.get(index as usize).into_iter().flatten().copied());
            }
            Source::KeyedByArgument(index) => {
                let arg = args.get(index as usize).into_iter().flatten().copied();
                substituted.extend(key_dependencies(arg));
            }
            source => {
                substituted.insert(source);
            }
        }
    }
    substituted
}

/// The context of an internal call site, applied to the callee's effects.
struct CallSite<'a> {
    args: &'a [BTreeSet<Source>],
    guarded: bool,
    in_loop: bool,
    after_external_call: bool,
}

impl CallSite<'_> {
    fn access(&self, access: &StorageAccess) -> StorageAccess {
        StorageAccess {
            slot: access.slot,
            keys: substitute(&access.keys, self.args),
            value: substitute(&access.value, self.args),
            transient: access.transient,
            guarded: access.guarded || self.guarded,
            in_loop: access.in_loop || self.in_loop,
            after_external_call: access.after_external_call || self.after_external_call,
            span: access.span,
        }
    }

    fn external_call(&self, call: &ExternalCall) -> ExternalCall {
        ExternalCall {
            kind: call.kind,
            selector: call.selector,
            target: substitute(&call.target, self.args),
            value: substitute(&call.value, self.args),
            args: call.args.iter().map(|arg| substitute(arg, self.args)).collect(),
            sends_value: call.sends_value,
            target_controlled: false,
            result_unchecked: call.result_unchecked,
            guarded: call.guarded || self.guarded,
            in_loop: call.in_loop || self.in_loop,
            after_external_call: call.after_external_call || self.after_external_call,
            span: call.span,
        }
    }

    fn event(&self, event: &EventEmission) -> EventEmission {
        EventEmission {
            name: event.name,
            args: event.args.iter().map(|arg| substitute(arg, self.args)).collect(),
            guarded: event.guarded || self.guarded,
            in_loop: event.in_loop || self.in_loop,
            after_external_call: event.after_external_call || self.after_external_call,
            span: event.span,
        }
    }

    fn self_destruct(&self, self_destruct: &SelfDestruct) -> SelfDestruct {
        SelfDestruct {
            beneficiary: substitute(&self_destruct.beneficiary, self.args),
            guarded: self_destruct.guarded || self.guarded,
            span: self_destruct.span,
        }
    }
}

/// Propagates direct effects through the internal call graph to a fixpoint, rewriting callee
/// effects in terms of each call site.
fn summarize(
    functions: &IndexVec<FunctionId, FunctionFacts>,
) -> IndexVec<FunctionId, EffectSummary> {
    fn merge<T: PartialEq>(into: &mut Vec<T>, items: impl IntoIterator<Item = T>) -> bool {
        let mut changed = false;
        for item in items {
            if !into.contains(&item) {
                into.push(item);
                changed = true;
            }
        }
        changed
    }

    let mut summaries = functions
        .iter()
        .map(|facts| EffectSummary {
            storage_reads: facts.storage_reads.clone(),
            storage_writes: facts.storage_writes.clone(),
            external_calls: facts.external_calls.clone(),
            events: facts.events.clone(),
            self_destructs: facts.self_destructs.clone(),
        })
        .collect::<IndexVec<FunctionId, _>>();

    let mut changed = true;
    while changed {
        changed = false;
        for (id, facts) in functions.iter_enumerated() {
            for call in &facts.internal_calls {
                let callee = summaries[FunctionId::from_usize(call.callee)].clone();
                let site = CallSite {
                    args: &call.args,
                    guarded: call.guarded,
                    in_loop: call.in_loop,
                    after_external_call: call.after_external_call,
                };
                let summary = &mut summaries[id];
                changed |= merge(
                    &mut summary.storage_reads,
                    callee.storage_reads.iter().map(|a| site.access(a)),
                );
                changed |= merge(
                    &mut summary.storage_writes,
                    callee.storage_writes.iter().map(|a| site.access(a)),
                );
                changed |= merge(
                    &mut summary.external_calls,
                    callee.external_calls.iter().map(|c| site.external_call(c)),
                );
                changed |= merge(&mut summary.events, callee.events.iter().map(|e| site.event(e)));
                changed |= merge(
                    &mut summary.self_destructs,
                    callee.self_destructs.iter().map(|s| site.self_destruct(s)),
                );
            }
        }
    }
    summaries
}

/// Collects what entry points read from and write into each storage slot, and which slots are
/// externally controlled.
fn storage_flows(functions: &IndexVec<FunctionId, FunctionFacts>) -> Vec<SlotFlow> {
    fn flow(flows: &mut BTreeMap<StorageSlot, SlotFlow>, slot: StorageSlot) -> &mut SlotFlow {
        flows.entry(slot).or_insert_with(|| SlotFlow {
            slot,
            readers: Vec::new(),
            writers: Vec::new(),
            externally_controlled: false,
        })
    }

    let mut flows = BTreeMap::new();
    for (id, facts) in functions.iter_enumerated() {
        if facts.kind == FunctionKind::Internal {
            continue;
        }
        let index = id.index();
        for access in &facts.summary.storage_reads {
            let flow = flow(&mut flows, access.slot);
            if !flow.readers.contains(&index) {
                flow.readers.push(index);
            }
        }
        for access in &facts.summary.storage_writes {
            let flow = flow(&mut flows, access.slot);
            if let Some(writer) = flow.writers.iter_mut().find(|writer| writer.function == index) {
                writer.value.extend(access.value.iter().copied());
                writer.guarded &= access.guarded;
            } else {
                flow.writers.push(SlotWriter {
                    function: index,
                    value: access.value.clone(),
                    guarded: access.guarded,
                });
            }
        }
    }

    let mut controlled = BTreeSet::new();
    let mut changed = true;
    while changed {
        changed = false;
        for flow in flows.values_mut() {
            if flow.externally_controlled {
                continue;
            }
            flow.externally_controlled = flow.writers.iter().any(|writer| {
                !writer.guarded
                    && functions[FunctionId::from_usize(writer.function)].kind
                        != FunctionKind::Constructor
                    && writer.value.iter().any(|&source| match source {
                        Source::Storage(slot) | Source::TransientStorage(slot) => {
                            controlled.contains(&slot)
                        }
                        source => source.is_external_input(),
                    })
            });
            if flow.externally_controlled {
                controlled.insert(flow.slot);
                changed = true;
            }
        }
    }
    flows.into_values().collect()
}

/// A program point: an instruction index in a block, where the block's instruction count denotes
/// its terminator.
type Point = (BlockId, usize);

/// The revert-guard checks in a function.
#[derive(Default)]
struct Checks {
    /// Each check holds from `(block, index)` to the end of the block and in every block that
    /// `block` strictly dominates.
    checks: Vec<(Point, BTreeSet<Source>)>,
}

impl Checks {
    /// Returns the sources checked at `point`.
    fn at(&self, cfg: &CfgInfo, (block, index): Point) -> BTreeSet<Source> {
        let mut checked = BTreeSet::new();
        for ((check_block, from), sources) in &self.checks {
            let holds = if *check_block == block {
                *from <= index
            } else {
                cfg.dominators().dominates(*check_block, block)
            };
            if holds {
                checked.extend(sources.iter().copied());
            }
        }
        checked
    }

    /// Returns whether a check on the caller or transaction origin holds at `point`.
    fn guarded(&self, cfg: &CfgInfo, point: Point) -> bool {
        is_caller_check(&self.at(cfg, point))
    }
}

/// Source-level facts shared by every function of a contract.
struct SourceFacts<'gcx> {
    gcx: Gcx<'gcx>,
    /// Regions of `unchecked` and inline assembly blocks.
    unchecked: Vec<Span>,
    /// The span of each `emit` expression and the event it emits.
    emits: Vec<(Span, hir::EventId)>,
    /// The HIR function declared at each span.
    functions: FxHashMap<Span, hir::FunctionId>,
    /// Constants compared against non-constant values in each HIR function body.
    constants: FxHashMap<hir::FunctionId, BTreeSet<U256>>,
}

impl<'gcx> SourceFacts<'gcx> {
    fn collect(gcx: Gcx<'gcx>) -> Self {
        struct Collector<'gcx> {
            facts: SourceFacts<'gcx>,
            function: Option<hir::FunctionId>,
        }

        impl<'gcx> Visit<'gcx> for Collector<'gcx> {
            type BreakValue = Never;

            fn hir(&self) -> &'gcx hir::Hir<'gcx> {
                &self.facts.gcx.hir
            }

            fn visit_stmt(&mut self, stmt: &'gcx hir::Stmt<'gcx>) -> ControlFlow<Self::BreakValue> {
                match &stmt.kind {
                    hir::StmtKind::UncheckedBlock(_) | hir::StmtKind::AssemblyBlock(_) => {
                        self.facts.unchecked.push(stmt.span);
                    }
                    hir::StmtKind::Emit(expr)
                        if let Some((callee, ..)) = expr.peel_parens().as_call()
                            && let Some(hir::Res::Item(hir::ItemId::Event(event))) =
                                self.facts.gcx.resolved_expr(callee) =>
                    {
                        self.facts.emits.push((expr.span, event));
                    }
                    _ => {}
                }
                self.walk_stmt(stmt)
            }

            fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
                if let hir::ExprKind::Binary(lhs, op, rhs) = &expr.kind
                    && op.kind.is_cmp()
                    && let Some(function) = self.function
                {
                    let gcx = self.facts.gcx;
                    let lhs = gcx.try_eval_const(lhs).ok();
                    let rhs = gcx.try_eval_const(rhs).ok();
                    if let (Some(value), None) | (None, Some(value)) = (lhs, rhs) {
                        let constants = self.facts.constants.entry(function).or_default();
                        constants.insert(value.as_evm_word());
                    }
                }
                self.walk_expr(expr)
            }
        }

        let facts = Self {
            gcx,
            unchecked: Vec::new(),
            emits: Vec::new(),
            functions: FxHashMap::default(),
            constants: FxHashMap::default(),
        };
        let mut collector = Collector { facts, function: None };
        for id in gcx.hir.function_ids() {
            let function = gcx.hir.function(id);
            collector.facts.functions.insert(function.span, id);
            if let Some(body) = &function.body {
                collector.function = Some(id);
                for stmt in body.stmts {
                    let _ = collector.visit_stmt(stmt);
                }
            }
        }
        collector.facts
    }

    /// Returns the HIR function a MIR function was lowered from.
    fn hir_function(&self, function: &Function) -> Option<&'gcx hir::Function<'gcx>> {
        let id = self.functions.get(&function.declaration_span)?;
        Some(self.gcx.hir.function(*id))
    }

    /// Returns the modifiers applied to a HIR function.
    fn modifier_ids(
        &self,
        function: &'gcx hir::Function<'gcx>,
    ) -> impl Iterator<Item = hir::FunctionId> + 'gcx {
        function.modifiers.iter().filter_map(|modifier| match modifier.id {
            hir::ItemId::Function(id) => Some(id),
            _ => None,
        })
    }

    /// Returns the names of the modifiers applied to a MIR function.
    fn modifiers(&self, function: &Function) -> Vec<Symbol> {
        let Some(function) = self.hir_function(function) else { return Vec::new() };
        self.modifier_ids(function)
            .filter_map(|id| self.gcx.hir.function(id).name.map(|name| name.name))
            .collect()
    }

    /// Returns the constants compared in a MIR function's source and in its modifiers.
    fn constants(&self, function: &Function) -> BTreeSet<U256> {
        let Some(id) = self.functions.get(&function.declaration_span) else {
            return BTreeSet::new();
        };
        let hir_function = self.gcx.hir.function(*id);
        std::iter::once(*id)
            .chain(self.modifier_ids(hir_function))
            .filter_map(|id| self.constants.get(&id))
            .flatten()
            .copied()
            .collect()
    }

    /// Returns the event emitted by the innermost `emit` containing `span`.
    fn event(&self, span: Span) -> Option<&'gcx hir::Event<'gcx>> {
        self.emits
            .iter()
            .filter(|(emit, _)| emit.contains(span))
            .min_by_key(|(emit, _)| emit.hi() - emit.lo())
            .map(|&(_, event)| self.gcx.hir.event(event))
    }

    /// Returns whether `span` is inside an `unchecked` or inline assembly block.
    fn is_unchecked(&self, span: Span) -> bool {
        self.unchecked.iter().any(|region| region.contains(span))
    }
}

/// Per-function analysis state.
struct FunctionAnalysis<'a> {
    function: &'a Function,
    returns: &'a IndexVec<FunctionId, BTreeSet<Source>>,
    cfg: CfgInfo,
    reverting: FxHashSet<BlockId>,
    predecessors: IndexVec<BlockId, u32>,
    sources: IndexVec<ValueId, BTreeSet<Source>>,
    slots: FxHashMap<ValueId, (StorageSlot, SmallVec<[ValueId; 2]>)>,
}

impl<'a> FunctionAnalysis<'a> {
    fn new(function: &'a Function, returns: &'a IndexVec<FunctionId, BTreeSet<Source>>) -> Self {
        let mut predecessors = function.blocks.iter().map(|_| 0).collect::<IndexVec<BlockId, _>>();
        for block in function.blocks.iter() {
            if let Some(terminator) = &block.terminator {
                terminator.for_each_successor(|successor| predecessors[successor] += 1);
            }
        }
        let mut analysis = Self {
            function,
            returns,
            cfg: CfgInfo::new(function),
            reverting: FxHashSet::default(),
            predecessors,
            sources: IndexVec::new(),
            slots: FxHashMap::default(),
        };
        analysis.reverting = analysis.reverting_blocks();
        analysis.compute_sources();
        analysis
    }

    /// Computes the dependencies of every value to a fixpoint.
    fn compute_sources(&mut self) {
        let function = self.function;
        let mut sources = (0..function.num_values())
            .map(|_| BTreeSet::new())
            .collect::<IndexVec<ValueId, BTreeSet<Source>>>();
        let mut changed = true;
        while changed {
            changed = false;
            for value in (0..function.num_values()).map(ValueId::from_usize) {
                let new = match function.value(value) {
                    Value::Arg(index) => BTreeSet::from([Source::Argument(index.index() as u32)]),
                    Value::Inst(inst) => self.inst_sources(&function.inst(*inst).kind, &sources),
                    Value::Immediate(_) | Value::Undef(_) | Value::Error(_) => continue,
                };
                if new != sources[value] {
                    sources[value] = new;
                    changed = true;
                }
            }
        }
        self.sources = sources;
    }

    /// Returns what the result of an instruction depends on, given the current dependencies of
    /// its operands.
    fn inst_sources(
        &mut self,
        kind: &InstKind,
        sources: &IndexVec<ValueId, BTreeSet<Source>>,
    ) -> BTreeSet<Source> {
        let operands = || kind.operands().into_iter().flat_map(|operand| sources[operand].clone());
        if let Some(access) = storage_access(kind)
            && !access.write
        {
            let (slot, keys) = self.slot(access.slot);
            let keys = keys.iter().flat_map(|&key| sources[key].iter().copied());
            let loaded = if access.transient {
                Source::TransientStorage(slot)
            } else {
                Source::Storage(slot)
            };
            return key_dependencies(keys).chain([loaded]).collect();
        }
        if external_call_kind(kind) {
            return BTreeSet::from([Source::CallResult]);
        }
        let source = match kind {
            InstKind::ICall { function: Callee::Function(callee), args } => {
                let args = args.iter().map(|&arg| sources[arg].clone()).collect::<Vec<_>>();
                return substitute(&self.returns[*callee], &args);
            }
            InstKind::ICall { function: Callee::Builtin(Builtin::ReturndataBytes), .. }
            | InstKind::ReturnDataSize => Source::CallResult,
            InstKind::Caller => Source::Caller,
            InstKind::Origin => Source::Origin,
            InstKind::CallValue => Source::CallValue,
            InstKind::Timestamp => Source::Timestamp,
            InstKind::BlockNumber => Source::BlockNumber,
            InstKind::PrevRandao | InstKind::BlockHash(..) | InstKind::BlobHash(..) => {
                Source::Randomness
            }
            InstKind::Balance(..) | InstKind::SelfBalance => Source::Balance,
            // Calldata reads also depend on the slice or offset they read from, which usually
            // derives from an argument.
            InstKind::CalldataLoad(..)
            | InstKind::CalldataSize
            | InstKind::CalldataSliceLoadWord { .. } => {
                return operands().chain([Source::Calldata]).collect();
            }
            _ => match kind.effect_kind() {
                EffectKind::EnvironmentRead => Source::Environment,
                EffectKind::ImmutableRead => Source::Immutable,
                EffectKind::MemoryRead => return operands().chain([Source::Memory]).collect(),
                _ => return operands().collect(),
            },
        };
        BTreeSet::from([source])
    }

    /// Returns what the function's return values depend on.
    fn return_sources(&self) -> BTreeSet<Source> {
        let mut sources = BTreeSet::new();
        for (block_id, block) in self.function.blocks.iter_enumerated() {
            if let Some(Terminator::Return { values }) = &block.terminator
                && !self.reverting.contains(&block_id)
            {
                for &value in values {
                    sources.extend(self.sources[value].iter().copied());
                }
            }
        }
        sources
    }

    /// Returns the points where the function exits successfully.
    fn exits(&self) -> impl Iterator<Item = Point> + '_ {
        self.function.blocks.iter_enumerated().filter_map(|(block_id, block)| {
            let exits = matches!(
                block.terminator,
                Some(
                    Terminator::Return { .. }
                        | Terminator::ReturnData { .. }
                        | Terminator::Stop
                        | Terminator::SelfDestruct { .. }
                        | Terminator::TailCall { .. }
                )
            );
            (exits && self.cfg.is_reachable(block_id) && !self.reverting.contains(&block_id))
                .then_some((block_id, block.instructions.len()))
        })
    }

    /// Collects the revert-guard checks, including those made by internal callees.
    fn checks(&self, entry_checks: &IndexVec<FunctionId, BTreeSet<Source>>) -> Checks {
        let function = self.function;
        let mut checks = Checks::default();
        for (block_id, block) in function.blocks.iter_enumerated() {
            if self.reverting.contains(&block_id) {
                continue;
            }
            for (index, &inst) in block.instructions.iter().enumerate() {
                let checked = match &function.inst(inst).kind {
                    InstKind::ICall { function: Callee::Builtin(builtin), args } => {
                        match self.builtin_guard(builtin, args) {
                            Some((GuardKind::Revert, condition)) => self.sources[condition].clone(),
                            _ => continue,
                        }
                    }
                    InstKind::ICall { function: Callee::Function(callee), args } => {
                        substitute(&entry_checks[*callee], &self.arg_sources(args))
                    }
                    _ => continue,
                };
                if !checked.is_empty() {
                    checks.checks.push(((block_id, index + 1), checked));
                }
            }
            // The check holds on the non-reverting side only if every path into it comes from
            // this branch.
            if let Some((condition, GuardKind::Revert, passing)) = self.branch_guard(block_id)
                && self.predecessors[passing] == 1
            {
                checks.checks.push(((passing, 0), self.sources[condition].clone()));
            }
        }
        checks
    }

    /// Returns the condition, failure kind, and passing successor of a branch with exactly one
    /// reverting side.
    fn branch_guard(&self, block: BlockId) -> Option<(ValueId, GuardKind, BlockId)> {
        let Some(Terminator::Branch { condition, then_block, else_block }) =
            &self.function.block(block).terminator
        else {
            return None;
        };
        let then_reverts = self.reverting.contains(then_block);
        let else_reverts = self.reverting.contains(else_block);
        if then_reverts == else_reverts {
            return None;
        }
        let (failing, passing) =
            if then_reverts { (*then_block, *else_block) } else { (*else_block, *then_block) };
        Some((*condition, self.block_guard_kind(failing), passing))
    }

    /// Returns what each argument value depends on.
    fn arg_sources(&self, args: &[ValueId]) -> Vec<BTreeSet<Source>> {
        args.iter().map(|&arg| self.sources[arg].clone()).collect()
    }

    /// Collects the facts that only depend on this function and on precomputed interprocedural
    /// summaries.
    fn direct_facts(
        &mut self,
        source: &SourceFacts<'_>,
        checks: &Checks,
        calls_out: &IndexVec<FunctionId, bool>,
    ) -> FunctionFacts {
        let function = self.function;
        let attributes = &function.attributes;
        let kind = if attributes.is_constructor {
            FunctionKind::Constructor
        } else if attributes.is_fallback {
            FunctionKind::Fallback
        } else if attributes.is_receive {
            FunctionKind::Receive
        } else if function.selector.is_some() {
            FunctionKind::External
        } else {
            FunctionKind::Internal
        };

        let mut facts = FunctionFacts {
            name: function.name.to_string(),
            kind,
            selector: function.selector,
            visibility: attributes.visibility,
            state_mutability: attributes.state_mutability,
            span: if function.name_span.is_dummy() {
                function.declaration_span
            } else {
                function.name_span
            },
            modifiers: Vec::new(),
            entry_checks: BTreeSet::new(),
            returns: BTreeSet::new(),
            storage_reads: Vec::new(),
            storage_writes: Vec::new(),
            external_calls: Vec::new(),
            internal_calls: Vec::new(),
            guards: Vec::new(),
            branches: Vec::new(),
            events: Vec::new(),
            self_destructs: Vec::new(),
            constants: BTreeSet::new(),
            unchecked_arithmetic: Vec::new(),
            hazards: Vec::new(),
            summary: EffectSummary::default(),
        };

        let calls_out = |kind: &InstKind| match kind {
            InstKind::ICall { function: Callee::Function(callee), .. } => calls_out[*callee],
            kind => external_call_kind(kind),
        };
        let after = self.blocks_after(calls_out);
        let used = self.used_values();
        for (block_id, block) in function.blocks.iter_enumerated() {
            if self.reverting.contains(&block_id) {
                // Reverting blocks only build revert payloads; their effects never commit.
                continue;
            }
            let in_loop = self.cfg.cyclic_blocks().contains(block_id);
            let mut after_call = after.contains(&block_id);
            for (index, &inst_id) in block.instructions.iter().enumerate() {
                let context = Context {
                    guarded: checks.guarded(&self.cfg, (block_id, index)),
                    in_loop,
                    after_external_call: after_call,
                };
                self.inst_facts(inst_id, context, source, &used, &mut facts);
                after_call |= calls_out(&function.inst(inst_id).kind);
            }
            let span = block.terminator_metadata.source_span();
            match &block.terminator {
                Some(Terminator::Branch { condition, .. }) => {
                    let sources = self.sources[*condition].clone();
                    if let Some((_, kind, _)) = self.branch_guard(block_id) {
                        facts.guards.push(Guard { kind, sources, span });
                    } else {
                        facts.branches.push(Branch { sources, in_loop, span });
                    }
                }
                Some(Terminator::Switch { value, .. }) => {
                    let sources = self.sources[*value].clone();
                    facts.branches.push(Branch { sources, in_loop, span });
                }
                Some(Terminator::SelfDestruct { recipient }) => {
                    facts.self_destructs.push(SelfDestruct {
                        beneficiary: self.sources[*recipient].clone(),
                        guarded: checks.guarded(&self.cfg, (block_id, block.instructions.len())),
                        span,
                    });
                }
                Some(Terminator::TailCall { function: callee, args }) => {
                    facts.internal_calls.push(InternalCall {
                        callee: callee.index(),
                        args: self.arg_sources(args),
                        guarded: checks.guarded(&self.cfg, (block_id, block.instructions.len())),
                        in_loop,
                        after_external_call: after_call,
                        span,
                    });
                }
                _ => {}
            }
        }
        facts
    }

    /// Records the facts contributed by one instruction.
    fn inst_facts(
        &mut self,
        inst_id: InstId,
        context: Context,
        source: &SourceFacts<'_>,
        used: &FxHashSet<ValueId>,
        facts: &mut FunctionFacts,
    ) {
        let function = self.function;
        let inst = function.inst(inst_id);
        let span = inst.metadata.source_span();
        if let Some(access) = storage_access(&inst.kind) {
            let (slot, keys) = self.slot(access.slot);
            let access = StorageAccess {
                slot,
                keys: self.key_sources(&keys),
                value: access
                    .value
                    .map(|value| self.written_sources(access.slot, value))
                    .unwrap_or_default(),
                transient: access.transient,
                guarded: context.guarded,
                in_loop: context.in_loop,
                after_external_call: context.after_external_call,
                span,
            };
            if storage_access(&inst.kind).is_some_and(|access| access.write) {
                facts.storage_writes.push(access);
            } else {
                facts.storage_reads.push(access);
            }
        }
        if let Some(mut call) = self.external_call(&inst.kind, span) {
            call.guarded = context.guarded;
            call.in_loop = context.in_loop;
            call.after_external_call = context.after_external_call;
            call.result_unchecked = matches!(
                inst.kind,
                InstKind::AddressCall { .. }
                    | InstKind::ICall { function: Callee::Builtin(Builtin::Send), .. }
            ) && function
                .inst_result_value(inst_id)
                .is_some_and(|result| !used.contains(&result));
            facts.external_calls.push(call);
        }
        if inst.kind.effect_kind() == EffectKind::Log {
            let event = self.event(&inst.kind, inst_id, span, source, context);
            facts.events.push(event);
        }
        let hazard = |kind, sources| Hazard { kind, sources, span };
        match &inst.kind {
            InstKind::ICall { function: Callee::Function(callee), args } => {
                facts.internal_calls.push(InternalCall {
                    callee: callee.index(),
                    args: self.arg_sources(args),
                    guarded: context.guarded,
                    in_loop: context.in_loop,
                    after_external_call: context.after_external_call,
                    span,
                });
            }
            InstKind::ICall { function: Callee::Builtin(builtin), args } => {
                if let Some((kind, condition)) = self.builtin_guard(builtin, args) {
                    let sources = self.sources[condition].clone();
                    facts.guards.push(Guard { kind, sources, span });
                }
            }
            InstKind::CallValue if context.in_loop => {
                facts.hazards.push(hazard(HazardKind::CallValueInLoop, BTreeSet::new()));
            }
            InstKind::Eq(a, b) | InstKind::Ne(a, b)
                if function.value_ty(*a) != Some(MirType::I1)
                    && [a, b].iter().any(|&&operand| {
                        self.sources[operand].iter().any(|source| {
                            matches!(
                                source,
                                Source::Balance | Source::Timestamp | Source::BlockNumber
                            )
                        })
                    }) =>
            {
                let sources = &self.sources[*a] | &self.sources[*b];
                facts.hazards.push(hazard(HazardKind::StrictEquality, sources));
            }
            _ => {}
        }
        let user_arithmetic = |kind: &InstKind| match kind {
            InstKind::CheckedBinary { op, lhs, rhs, .. } => Some((*op, *lhs, *rhs)),
            InstKind::Add(lhs, rhs) => Some((CheckedOp::Add, *lhs, *rhs)),
            InstKind::Sub(lhs, rhs) => Some((CheckedOp::Sub, *lhs, *rhs)),
            InstKind::Mul(lhs, rhs) => Some((CheckedOp::Mul, *lhs, *rhs)),
            InstKind::Div(lhs, rhs) | InstKind::SDiv(lhs, rhs) => {
                Some((CheckedOp::WrappingDiv, *lhs, *rhs))
            }
            InstKind::Mod(lhs, rhs) | InstKind::SMod(lhs, rhs) => {
                Some((CheckedOp::Rem, *lhs, *rhs))
            }
            InstKind::Exp(lhs, rhs) => Some((CheckedOp::Pow, *lhs, *rhs)),
            _ => None,
        };
        // Checked source arithmetic lowers to `checked_binary`, so wrapping operations inside an
        // unchecked region were written as unchecked.
        let wrapping = !matches!(inst.kind, InstKind::CheckedBinary { .. });
        let Some((op, lhs, rhs)) = user_arithmetic(&inst.kind) else { return };
        let Some(span) = span.filter(|&span| !wrapping || source.is_unchecked(span)) else {
            return;
        };
        if wrapping
            && matches!(op, CheckedOp::Add | CheckedOp::Sub | CheckedOp::Mul | CheckedOp::Pow)
        {
            facts.unchecked_arithmetic.push(span);
        }
        match op {
            CheckedOp::Mul
                if [lhs, rhs].into_iter().any(|operand| {
                    let operand = self.peel_casts(operand);
                    let Value::Inst(def) = function.value(operand) else { return false };
                    user_arithmetic(&function.inst(*def).kind).is_some_and(|(op, ..)| {
                        matches!(op, CheckedOp::Div | CheckedOp::WrappingDiv)
                    })
                }) =>
            {
                let sources = &self.sources[lhs] | &self.sources[rhs];
                facts.hazards.push(hazard(HazardKind::DivideBeforeMultiply, sources));
            }
            CheckedOp::Rem
                if self.sources[lhs].iter().any(|source| {
                    matches!(source, Source::Timestamp | Source::BlockNumber | Source::Randomness)
                }) =>
            {
                let sources = self.sources[lhs].clone();
                facts.hazards.push(hazard(HazardKind::WeakRandomness, sources));
            }
            _ => {}
        }
    }

    /// Builds the fact for a `log` instruction.
    fn event(
        &self,
        kind: &InstKind,
        inst_id: InstId,
        span: Option<Span>,
        source: &SourceFacts<'_>,
        context: Context,
    ) -> EventEmission {
        let function = self.function;
        let operands = kind.operands();
        let (offset, size, topics) = (operands[0], operands[1], &operands[2..]);
        let event = span.and_then(|span| source.event(span));

        // The data is either ABI-encoded into a fresh buffer or a single word stored to scratch
        // memory right before the log.
        let data = match function.value(offset) {
            Value::Inst(slice)
                if let InstKind::SlicePtr(encoded) = &function.inst(*slice).kind
                    && let Value::Inst(encode) = function.value(*encoded)
                    && let InstKind::AbiEncode { args, .. } = &function.inst(*encode).kind =>
            {
                Some(args.iter().map(|&arg| self.sources[arg].clone()).collect::<Vec<_>>())
            }
            _ if let Some(offset) = function.value_u256(offset)
                && function.value_u256(size) == Some(U256::from(32)) =>
            {
                self.stored_before(inst_id, offset).map(|value| vec![self.sources[value].clone()])
            }
            _ => None,
        };

        let mut topics = topics.iter().map(|&topic| self.sources[topic].clone());
        let args = match event {
            Some(event) => {
                if !event.anonymous {
                    topics.next();
                }
                let hir = &source.gcx.hir;
                let indexed =
                    event.parameters.iter().map(|&id| hir.variable(id).indexed).collect::<Vec<_>>();
                let data_count = indexed.iter().filter(|&&indexed| !indexed).count();
                let mut whole = &self.sources[offset] | &self.sources[size];
                whole.extend(data.iter().flatten().flatten().copied());
                let mut data = data.filter(|data| data.len() == data_count).map(Vec::into_iter);
                indexed
                    .iter()
                    .map(|&indexed| {
                        if indexed {
                            topics.next().unwrap_or_default()
                        } else {
                            data.as_mut().and_then(Iterator::next).unwrap_or_else(|| whole.clone())
                        }
                    })
                    .collect()
            }
            None => {
                let mut data_sources = &self.sources[offset] | &self.sources[size];
                data_sources.extend(data.into_iter().flatten().flatten());
                topics.chain([data_sources]).collect()
            }
        };
        EventEmission {
            name: event.map(|event| event.name.name),
            args,
            guarded: context.guarded,
            in_loop: context.in_loop,
            after_external_call: context.after_external_call,
            span,
        }
    }

    /// Returns the value most recently stored at the constant memory `offset` before `inst` in the
    /// same block.
    fn stored_before(&self, inst: InstId, offset: U256) -> Option<ValueId> {
        let function = self.function;
        let block = function.blocks.iter().find(|block| block.instructions.contains(&inst))?;
        let position = block.instructions.iter().position(|&i| i == inst)?;
        block.instructions[..position].iter().rev().find_map(|&inst| {
            match function.inst(inst).kind {
                InstKind::MStore(address, value)
                    if function.value_u256(address) == Some(offset) =>
                {
                    Some(value)
                }
                _ => None,
            }
        })
    }

    /// Returns what a value stored to `slot` depends on.
    ///
    /// Writes to a variable narrower than a word merge the new value into the other bits of the
    /// slot, `(sload(slot) & mask) | value`; the preserved bits are not part of the written value.
    fn written_sources(&mut self, slot: ValueId, value: ValueId) -> BTreeSet<Source> {
        let function = self.function;
        let (target, _) = self.slot(slot);
        let preserves_slot = |this: &mut Self, value: ValueId| {
            let Value::Inst(inst) = function.value(value) else { return false };
            let InstKind::And(a, b) = function.inst(*inst).kind else { return false };
            [a, b].into_iter().any(|operand| {
                let Value::Inst(load) = function.value(operand) else { return false };
                match function.inst(*load).kind {
                    InstKind::SLoad(loaded) | InstKind::TLoad(loaded) => {
                        this.slot(loaded).0 == target
                    }
                    _ => false,
                }
            })
        };
        if let Value::Inst(inst) = function.value(value)
            && let InstKind::Or(a, b) = function.inst(*inst).kind
        {
            match (preserves_slot(self, a), preserves_slot(self, b)) {
                (true, false) => return self.sources[b].clone(),
                (false, true) => return self.sources[a].clone(),
                _ => {}
            }
        }
        self.sources[value].clone()
    }

    /// Follows integer width conversions back to the converted value.
    fn peel_casts(&self, mut value: ValueId) -> ValueId {
        while let Value::Inst(inst) = self.function.value(value)
            && let InstKind::Zext(operand)
            | InstKind::Trunc(operand, _)
            | InstKind::Sext(operand, ..) = self.function.inst(*inst).kind
        {
            value = operand;
        }
        value
    }

    /// Returns the values used by an instruction or terminator in a block.
    fn used_values(&self) -> FxHashSet<ValueId> {
        let function = self.function;
        let mut used = FxHashSet::default();
        for block in function.blocks.iter() {
            for &inst in &block.instructions {
                used.extend(function.inst(inst).kind.operands());
            }
            if let Some(terminator) = &block.terminator {
                used.extend(terminator.operands());
            }
        }
        used
    }

    /// Returns the blocks reachable from the end of a block containing an external call.
    fn blocks_after(&self, calls_out: impl Fn(&InstKind) -> bool) -> FxHashSet<BlockId> {
        let function = self.function;
        let mut after = FxHashSet::default();
        let mut worklist = Vec::new();
        for block in function.blocks.iter() {
            if block.instructions.iter().any(|&inst| calls_out(&function.inst(inst).kind))
                && let Some(terminator) = &block.terminator
            {
                terminator.for_each_successor(|successor| worklist.push(successor));
            }
        }
        while let Some(block) = worklist.pop() {
            if after.insert(block)
                && let Some(terminator) = &function.block(block).terminator
            {
                terminator.for_each_successor(|successor| worklist.push(successor));
            }
        }
        after
    }

    /// Returns the kind and condition of a builtin that can fail on a runtime condition.
    fn builtin_guard(&self, builtin: &Builtin, args: &[ValueId]) -> Option<(GuardKind, ValueId)> {
        let &condition = args.first()?;
        if self.function.value_u256(condition).is_some() {
            // Constant conditions are unconditional reverts, handled as reverting blocks.
            return None;
        }
        match builtin {
            Builtin::Require(_) => Some((GuardKind::Revert, condition)),
            Builtin::Check { failure: RevertKind::Panic(_), .. } => {
                Some((GuardKind::Panic, condition))
            }
            Builtin::Check { failure: RevertKind::Reason(_), .. } => {
                Some((GuardKind::Revert, condition))
            }
            _ => None,
        }
    }

    /// Classifies an external call instruction.
    fn external_call(&self, kind: &InstKind, span: Option<Span>) -> Option<ExternalCall> {
        let (kind, target, value, input) = match kind {
            InstKind::AddressCall { kind, address, value, input, .. } => {
                let kind = match kind {
                    AddressCallKind::Call => CallKind::Call,
                    AddressCallKind::Static => CallKind::StaticCall,
                    AddressCallKind::Delegate => CallKind::DelegateCall,
                };
                (kind, Some(*address), *value, Some(*input))
            }
            InstKind::Call { addr, value, args_offset, .. } => {
                (CallKind::Call, Some(*addr), Some(*value), Some(*args_offset))
            }
            InstKind::CallCode { addr, value, args_offset, .. } => {
                (CallKind::CallCode, Some(*addr), Some(*value), Some(*args_offset))
            }
            InstKind::StaticCall { addr, args_offset, .. } => {
                (CallKind::StaticCall, Some(*addr), None, Some(*args_offset))
            }
            InstKind::DelegateCall { addr, args_offset, .. } => {
                (CallKind::DelegateCall, Some(*addr), None, Some(*args_offset))
            }
            InstKind::Create(value, ..) => (CallKind::Create, None, Some(*value), None),
            InstKind::Create2(value, ..) => (CallKind::Create2, None, Some(*value), None),
            InstKind::ICall { function: Callee::Builtin(Builtin::Transfer), args } => {
                (CallKind::Transfer, args.first().copied(), args.get(1).copied(), None)
            }
            InstKind::ICall { function: Callee::Builtin(Builtin::Send), args } => {
                (CallKind::Send, args.first().copied(), args.get(1).copied(), None)
            }
            _ => return None,
        };
        let function = self.function;
        let sends_value = value.is_some_and(|value| function.value_u256(value) != Some(U256::ZERO));

        // High-level calls pass a pointer into an ABI encoding with a constant selector.
        let encoding = input.and_then(|input| {
            let Value::Inst(slice) = function.value(input) else { return None };
            let InstKind::SlicePtr(encoded) = function.inst(*slice).kind else { return None };
            let Value::Inst(encode) = function.value(encoded) else { return None };
            match &function.inst(*encode).kind {
                InstKind::AbiEncode { selector: Some(selector), args, .. } => {
                    let selector = function.value_u256(*selector)?;
                    Some((selector.to_be_bytes::<32>()[..4].try_into().unwrap(), args))
                }
                _ => None,
            }
        });
        let (selector, args) = match (encoding, input) {
            (Some((selector, args)), _) => (Some(selector), self.arg_sources(args)),
            (None, Some(input))
                if matches!(
                    kind,
                    CallKind::Call
                        | CallKind::StaticCall
                        | CallKind::DelegateCall
                        | CallKind::CallCode
                ) =>
            {
                (None, vec![self.sources[input].clone()])
            }
            _ => (None, Vec::new()),
        };
        Some(ExternalCall {
            kind,
            selector,
            target: target.map(|target| self.sources[target].clone()).unwrap_or_default(),
            value: value
                .filter(|_| sends_value)
                .map(|value| self.sources[value].clone())
                .unwrap_or_default(),
            args,
            sends_value,
            target_controlled: false,
            result_unchecked: false,
            guarded: false,
            in_loop: false,
            after_external_call: false,
            span,
        })
    }

    /// Returns the blocks whose every path ends in a revert or panic.
    fn reverting_blocks(&self) -> FxHashSet<BlockId> {
        let function = self.function;
        let mut reverting = FxHashSet::default();
        // Iterate to a fixpoint so jump chains into reverting blocks are recognized.
        let mut changed = true;
        while changed {
            changed = false;
            for (block_id, block) in function.blocks.iter_enumerated() {
                if reverting.contains(&block_id) {
                    continue;
                }
                let fails_unconditionally = block.instructions.iter().any(|&inst| {
                    let InstKind::ICall { function: Callee::Builtin(builtin), args } =
                        &function.inst(inst).kind
                    else {
                        return false;
                    };
                    let Some(condition) = args.first().and_then(|&c| function.value_u256(c)) else {
                        return false;
                    };
                    match builtin {
                        Builtin::Require(_) => condition.is_zero(),
                        Builtin::Check { is_zero, .. } => condition.is_zero() == *is_zero,
                        _ => false,
                    }
                });
                let reverts = fails_unconditionally
                    || match &block.terminator {
                        Some(Terminator::Revert { .. } | Terminator::RevertReturndata) => true,
                        Some(Terminator::Jump(target)) => reverting.contains(target),
                        _ => false,
                    };
                if reverts {
                    reverting.insert(block_id);
                    changed = true;
                }
            }
        }
        reverting
    }

    /// Returns whether a reverting block panics rather than reverts.
    fn block_guard_kind(&self, block: BlockId) -> GuardKind {
        let function = self.function;
        let mut block = block;
        let mut visited = FxHashSet::default();
        while visited.insert(block) {
            let data = function.block(block);
            for &inst in &data.instructions {
                if let InstKind::ICall { function: Callee::Builtin(builtin), .. } =
                    &function.inst(inst).kind
                {
                    match builtin {
                        Builtin::Check { failure: RevertKind::Panic(_), .. } => {
                            return GuardKind::Panic;
                        }
                        Builtin::Check { .. } | Builtin::Require(_) => return GuardKind::Revert,
                        _ => {}
                    }
                }
            }
            match data.terminator {
                Some(Terminator::Jump(target)) => block = target,
                _ => break,
            }
        }
        GuardKind::Revert
    }

    /// Returns what the given mapping keys and array indices depend on.
    fn key_sources(&self, keys: &[ValueId]) -> BTreeSet<Source> {
        keys.iter().flat_map(|&key| self.sources[key].iter().copied()).collect()
    }

    /// Resolves a storage slot value to the declared slot it derives from, together with the
    /// mapping keys and array indices used to derive it.
    fn slot(&mut self, value: ValueId) -> (StorageSlot, SmallVec<[ValueId; 2]>) {
        if let Some(slot) = self.slots.get(&value) {
            return slot.clone();
        }
        self.slots.insert(value, (StorageSlot::Unknown, SmallVec::new()));
        let function = self.function;
        let resolved = if let Some(slot) = function.value_u256(value) {
            (StorageSlot::Exact(slot), SmallVec::new())
        } else if let Value::Inst(inst_id) = function.value(value) {
            match &function.inst(*inst_id).kind {
                InstKind::MappingSlot(key, slot)
                | InstKind::MappingSlotMemory(key, slot)
                | InstKind::MappingSlotCalldata(key, slot) => {
                    let (slot, mut keys) = self.slot(*slot);
                    keys.push(*key);
                    (derived(slot), keys)
                }
                InstKind::StorageArrayDataSlot(slot) => {
                    let (slot, keys) = self.slot(*slot);
                    (derived(slot), keys)
                }
                InstKind::StorageArrayElementSlot { slot, index, .. } => {
                    let (slot, mut keys) = self.slot(*slot);
                    keys.push(*index);
                    (derived(slot), keys)
                }
                InstKind::Add(a, b) => self.offset_slot(*a, *b),
                _ => (StorageSlot::Unknown, SmallVec::new()),
            }
        } else {
            (StorageSlot::Unknown, SmallVec::new())
        };
        self.slots.insert(value, resolved.clone());
        resolved
    }

    /// Resolves `a + b` where one operand is a slot and the other an offset or index.
    fn offset_slot(&mut self, a: ValueId, b: ValueId) -> (StorageSlot, SmallVec<[ValueId; 2]>) {
        let function = self.function;
        match (function.value_u256(a), function.value_u256(b)) {
            (Some(offset), None) | (None, Some(offset)) => {
                let base = if function.value_u256(a).is_some() { b } else { a };
                match self.slot(base) {
                    (StorageSlot::Exact(slot), keys) => {
                        (StorageSlot::Exact(slot.wrapping_add(offset)), keys)
                    }
                    resolved => resolved,
                }
            }
            (Some(a), Some(b)) => (StorageSlot::Exact(a.wrapping_add(b)), SmallVec::new()),
            (None, None) => {
                let (slot, mut keys, index) = match self.slot(a) {
                    (StorageSlot::Unknown, _) => {
                        let (slot, keys) = self.slot(b);
                        (slot, keys, a)
                    }
                    (slot, keys) => (slot, keys, b),
                };
                if slot == StorageSlot::Unknown {
                    return (StorageSlot::Unknown, SmallVec::new());
                }
                keys.push(index);
                (derived(slot), keys)
            }
        }
    }
}

/// Context flags for an effect at one program point.
#[derive(Clone, Copy)]
struct Context {
    guarded: bool,
    in_loop: bool,
    after_external_call: bool,
}

/// Marks a slot as derived from its declared base.
fn derived(slot: StorageSlot) -> StorageSlot {
    match slot {
        StorageSlot::Exact(slot) | StorageSlot::Derived(slot) => StorageSlot::Derived(slot),
        StorageSlot::Unknown => StorageSlot::Unknown,
    }
}

/// The operands of a storage access instruction.
struct Access {
    slot: ValueId,
    value: Option<ValueId>,
    write: bool,
    transient: bool,
}

/// Returns the operands of a storage access instruction.
fn storage_access(kind: &InstKind) -> Option<Access> {
    let (slot, value) = match *kind {
        InstKind::SLoad(slot) | InstKind::TLoad(slot) => (slot, None),
        InstKind::SStore(slot, value) | InstKind::TStore(slot, value) => (slot, Some(value)),
        InstKind::ValidateStorageBytes(slot) | InstKind::StorageBytesLoad(slot) => (slot, None),
        InstKind::StorageArrayLoad { slot, .. } => (slot, None),
        InstKind::StorageToMemory { storage, .. } => (storage, None),
        InstKind::StorageBytesStore(slot, value) => (slot, Some(value)),
        InstKind::StorageClearWords(slot, ..) | InstKind::StorageBytesStoreLiteral { slot, .. } => {
            (slot, None)
        }
        InstKind::MemoryToStorage { storage, memory, .. } => (storage, Some(memory)),
        InstKind::ClearStorage { storage, .. } => (storage, None),
        _ => return None,
    };
    let effect = kind.effect_kind();
    Some(Access {
        slot,
        value,
        write: matches!(effect, EffectKind::StorageWrite | EffectKind::TransientWrite),
        transient: matches!(effect, EffectKind::TransientRead | EffectKind::TransientWrite),
    })
}

/// Returns whether an instruction transfers control to another contract.
fn external_call_kind(kind: &InstKind) -> bool {
    matches!(kind.effect_kind(), EffectKind::ExternalCall | EffectKind::Create)
        && !matches!(
            kind,
            InstKind::ICall {
                function: Callee::Builtin(
                    Builtin::Sha256 | Builtin::Ripemd160 | Builtin::EcRecover
                ),
                ..
            }
        )
}
