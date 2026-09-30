//! Security facts derived from MIR (SecIR).
//!
//! SecIR is a read-only view over a contract's semantic MIR, intended for security tooling such as
//! fuzzers, invariant generators, and static analyzers. It answers questions that are hard to
//! recover from source text or bytecode alone:
//!
//! - which storage slots each function reads and writes, and which values key mapping and array
//!   accesses (for example, `balances[msg.sender]`);
//! - which external calls a function makes, what their target and value depend on, and whether they
//!   can send value;
//! - which conditions guard a revert, and what those conditions depend on (`msg.sender`,
//!   `tx.origin`, arguments, storage, call results, ...);
//! - which arithmetic was written in an `unchecked` or inline assembly block;
//! - which storage writes can execute after an external call in the same function, directly or
//!   through an internal call;
//! - which storage and external calls each function reaches transitively through internal calls.
//!
//! Facts are computed from the MIR built by [`lower_contract`](crate::mir::lower::lower_contract)
//! before any optimization or representation-lowering pass runs, so they describe the program as
//! written, independent of the optimization mode. Each fact carries the source span recorded by
//! lowering when one is available.
//!
//! # Precision
//!
//! The analysis is a best-effort, flow-insensitive dependence analysis over SSA values:
//!
//! - Value dependencies follow SSA operands and phis. Values loaded from memory are reported as
//!   [`Source::Memory`] without tracking the stored value, so dependencies that round-trip through
//!   memory (for example, decoded return data) are approximated.
//! - Storage slots are resolved through constant slots, mapping and array slot derivations, and
//!   constant offsets. Slots computed any other way, such as in inline assembly, are reported as
//!   [`StorageSlot::Unknown`].
//! - Internal call results depend on their arguments only; callee effects are reported through the
//!   transitive [`FunctionFacts::summary`] instead.
//! - "After an external call" means reachable in the control-flow graph from an external call in
//!   the same function, including through internal calls that transitively make one. It does not
//!   prove that an exploitable reentrancy exists.
//!
//! Consumers should treat facts as hints for search and triage, not as proofs.

use crate::mir::{
    self, AddressCallKind, BlockId, Builtin, Callee, Function, FunctionId, InstId, InstKind,
    Module, RevertKind, Terminator, Value, ValueId,
};
use alloy_primitives::U256;
use solar_data_structures::{
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};
use solar_interface::{Span, Symbol};
use solar_sema::{
    Gcx,
    hir::{self, ContractId, StateMutability, Visibility},
};
use std::collections::BTreeSet;

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
    /// Wrapping arithmetic written in `unchecked` or inline assembly blocks.
    pub unchecked_arithmetic: Vec<Span>,
    /// Storage writes that can execute after an external call in this function.
    ///
    /// Writes made by internal callees are reported at the internal call's span.
    pub writes_after_external_call: Vec<StorageAccess>,
    /// Whether this function can self-destruct the contract.
    pub self_destructs: bool,
    /// Effects reachable through this function and its internal callees.
    pub summary: EffectSummary,
}

/// Effects reachable through a function and its internal callees.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EffectSummary {
    /// Storage slots read.
    pub storage_reads: BTreeSet<StorageSlot>,
    /// Storage slots written.
    pub storage_writes: BTreeSet<StorageSlot>,
    /// Kinds of external calls made.
    pub external_calls: BTreeSet<CallKind>,
    /// Whether any reachable function can self-destruct the contract.
    pub self_destructs: bool,
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
    /// Whether the access is to transient storage.
    pub transient: bool,
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
    /// An immutable variable.
    Immutable,
    /// The success flag, return data, or returned value of an external call.
    CallResult,
    /// Raw calldata.
    Calldata,
    /// Block, transaction, or account environment, such as `block.timestamp` or balances.
    Environment,
    /// A value loaded from memory, whose origin is not tracked.
    Memory,
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
    /// What the call target depends on. Empty for constant targets and contract creations.
    pub target: BTreeSet<Source>,
    /// What the transferred value depends on. Empty when no value can be sent.
    pub value: BTreeSet<Source>,
    /// Whether the call can transfer a nonzero value.
    pub sends_value: bool,
    /// The source span of the call.
    pub span: Option<Span>,
}

/// An internal call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InternalCall {
    /// The callee, as an index into [`ContractFacts::functions`].
    pub callee: usize,
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
        self.sources.contains(&Source::Caller) || self.sources.contains(&Source::Origin)
    }
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

    let unchecked = unchecked_regions(gcx);
    let mut functions = module
        .functions
        .iter()
        .map(|function| FunctionAnalysis::new(function).direct_facts(&unchecked))
        .collect::<IndexVec<FunctionId, _>>();
    let summaries = summarize(module, &functions);
    for (id, function) in module.functions.iter_enumerated() {
        let facts = &mut functions[id];
        facts.writes_after_external_call =
            FunctionAnalysis::new(function).writes_after_external_call(&summaries);
        facts.summary = summaries[id].clone();
    }

    ContractFacts { name: module.name.name, state_variables, functions: functions.raw }
}

/// Propagates direct effects through the internal call graph to a fixpoint.
fn summarize(
    module: &Module,
    functions: &IndexVec<FunctionId, FunctionFacts>,
) -> IndexVec<FunctionId, EffectSummary> {
    let callees = module
        .functions
        .iter()
        .map(|function| {
            let mut callees = FxHashSet::default();
            for block in function.blocks.iter() {
                for &inst in &block.instructions {
                    if let InstKind::ICall { function: Callee::Function(callee), .. } =
                        &function.inst(inst).kind
                    {
                        callees.insert(*callee);
                    }
                }
                if let Some(Terminator::TailCall { function: callee, .. }) = &block.terminator {
                    callees.insert(*callee);
                }
            }
            callees.into_iter().collect::<Vec<_>>()
        })
        .collect::<IndexVec<FunctionId, _>>();

    let mut summaries = functions
        .iter()
        .map(|facts| EffectSummary {
            storage_reads: facts.storage_reads.iter().map(|access| access.slot).collect(),
            storage_writes: facts.storage_writes.iter().map(|access| access.slot).collect(),
            external_calls: facts.external_calls.iter().map(|call| call.kind).collect(),
            self_destructs: facts.self_destructs,
        })
        .collect::<IndexVec<FunctionId, _>>();

    let mut changed = true;
    while changed {
        changed = false;
        for (id, callees) in callees.iter_enumerated() {
            for &callee in callees {
                if callee == id {
                    continue;
                }
                let callee = summaries[callee].clone();
                let summary = &mut summaries[id];
                let before = (
                    summary.storage_reads.len(),
                    summary.storage_writes.len(),
                    summary.external_calls.len(),
                    summary.self_destructs,
                );
                summary.storage_reads.extend(callee.storage_reads);
                summary.storage_writes.extend(callee.storage_writes);
                summary.external_calls.extend(callee.external_calls);
                summary.self_destructs |= callee.self_destructs;
                let after = (
                    summary.storage_reads.len(),
                    summary.storage_writes.len(),
                    summary.external_calls.len(),
                    summary.self_destructs,
                );
                changed |= before != after;
            }
        }
    }
    summaries
}

/// Per-function analysis state.
struct FunctionAnalysis<'a> {
    function: &'a Function,
    sources: FxHashMap<ValueId, BTreeSet<Source>>,
    slots: FxHashMap<ValueId, (StorageSlot, BTreeSet<Source>)>,
}

impl<'a> FunctionAnalysis<'a> {
    fn new(function: &'a Function) -> Self {
        Self { function, sources: FxHashMap::default(), slots: FxHashMap::default() }
    }

    /// Collects the facts that do not depend on other functions.
    ///
    /// `unchecked` holds the source regions where arithmetic wraps.
    fn direct_facts(mut self, unchecked: &[Span]) -> FunctionFacts {
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
            storage_reads: Vec::new(),
            storage_writes: Vec::new(),
            external_calls: Vec::new(),
            internal_calls: Vec::new(),
            guards: Vec::new(),
            unchecked_arithmetic: Vec::new(),
            writes_after_external_call: Vec::new(),
            self_destructs: false,
            summary: EffectSummary::default(),
        };

        let reverting = self.reverting_blocks();
        for (block_id, block) in function.blocks.iter_enumerated() {
            if reverting.contains(&block_id) {
                // Reverting blocks only build revert payloads; their effects never commit.
                continue;
            }
            for &inst_id in &block.instructions {
                self.inst_facts(inst_id, unchecked, &mut facts);
            }
            match &block.terminator {
                Some(Terminator::Branch { condition, then_block, else_block }) => {
                    let then_reverts = reverting.contains(then_block);
                    let else_reverts = reverting.contains(else_block);
                    if then_reverts != else_reverts {
                        let failing = if then_reverts { *then_block } else { *else_block };
                        facts.guards.push(Guard {
                            kind: self.block_guard_kind(failing),
                            sources: self.sources(*condition),
                            span: block.terminator_metadata.source_span(),
                        });
                    }
                }
                Some(Terminator::SelfDestruct { .. }) => facts.self_destructs = true,
                _ => {}
            }
        }
        facts
    }

    /// Records the facts contributed by one instruction.
    fn inst_facts(&mut self, inst_id: InstId, unchecked: &[Span], facts: &mut FunctionFacts) {
        let inst = self.function.inst(inst_id);
        let span = inst.metadata.source_span();
        if let Some((slot, write, transient)) = storage_access(&inst.kind) {
            let (slot, keys) = self.slot(slot);
            let access = StorageAccess { slot, keys, transient, span };
            if write {
                facts.storage_writes.push(access);
            } else {
                facts.storage_reads.push(access);
            }
        }
        if let Some(call) = self.external_call(&inst.kind, span) {
            facts.external_calls.push(call);
        }
        match &inst.kind {
            InstKind::ICall { function: Callee::Function(callee), .. } => {
                facts.internal_calls.push(InternalCall { callee: callee.index(), span });
            }
            InstKind::ICall { function: Callee::Builtin(builtin), args } => {
                if let Some((kind, condition)) = self.builtin_guard(builtin, args) {
                    facts.guards.push(Guard { kind, sources: self.sources(condition), span });
                }
            }
            // Checked source arithmetic lowers to `checked_*` operations, so wrapping operations
            // inside an unchecked region were written as unchecked.
            InstKind::Add(..) | InstKind::Sub(..) | InstKind::Mul(..) | InstKind::Exp(..)
                if let Some(span) = span
                    && unchecked.iter().any(|region| region.contains(span)) =>
            {
                facts.unchecked_arithmetic.push(span);
            }
            _ => {}
        }
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
    fn external_call(&mut self, kind: &InstKind, span: Option<Span>) -> Option<ExternalCall> {
        let (kind, target, value) = match kind {
            InstKind::AddressCall { kind, address, value, .. } => {
                let kind = match kind {
                    AddressCallKind::Call => CallKind::Call,
                    AddressCallKind::Static => CallKind::StaticCall,
                    AddressCallKind::Delegate => CallKind::DelegateCall,
                };
                (kind, Some(*address), *value)
            }
            InstKind::Call { addr, value, .. } => (CallKind::Call, Some(*addr), Some(*value)),
            InstKind::CallCode { addr, value, .. } => {
                (CallKind::CallCode, Some(*addr), Some(*value))
            }
            InstKind::StaticCall { addr, .. } => (CallKind::StaticCall, Some(*addr), None),
            InstKind::DelegateCall { addr, .. } => (CallKind::DelegateCall, Some(*addr), None),
            InstKind::Create(value, ..) => (CallKind::Create, None, Some(*value)),
            InstKind::Create2(value, ..) => (CallKind::Create2, None, Some(*value)),
            InstKind::ICall { function: Callee::Builtin(Builtin::Transfer), args } => {
                (CallKind::Transfer, args.first().copied(), args.get(1).copied())
            }
            InstKind::ICall { function: Callee::Builtin(Builtin::Send), args } => {
                (CallKind::Send, args.first().copied(), args.get(1).copied())
            }
            _ => return None,
        };
        let sends_value =
            value.is_some_and(|value| self.function.value_u256(value) != Some(U256::ZERO));
        Some(ExternalCall {
            kind,
            target: target.map(|target| self.sources(target)).unwrap_or_default(),
            value: value
                .filter(|_| sends_value)
                .map(|value| self.sources(value))
                .unwrap_or_default(),
            sends_value,
            span,
        })
    }

    /// Returns the storage writes that can execute after an external call.
    fn writes_after_external_call(
        mut self,
        summaries: &IndexVec<FunctionId, EffectSummary>,
    ) -> Vec<StorageAccess> {
        let function = self.function;
        let reverting = self.reverting_blocks();
        let calls_out = |kind: &InstKind| match kind {
            InstKind::ICall { function: Callee::Function(callee), .. } => {
                !summaries[*callee].external_calls.is_empty()
            }
            kind => external_call_kind(kind),
        };

        // Blocks reachable from the end of a block containing an external call.
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

        let mut writes = Vec::new();
        for (block_id, block) in function.blocks.iter_enumerated() {
            if reverting.contains(&block_id) {
                continue;
            }
            let mut called = after.contains(&block_id);
            for &inst_id in &block.instructions {
                let inst = function.inst(inst_id);
                let span = inst.metadata.source_span();
                if called {
                    if let Some((slot, true, transient)) = storage_access(&inst.kind) {
                        let (slot, keys) = self.slot(slot);
                        writes.push(StorageAccess { slot, keys, transient, span });
                    } else if let InstKind::ICall { function: Callee::Function(callee), .. } =
                        &inst.kind
                    {
                        writes.extend(summaries[*callee].storage_writes.iter().map(|&slot| {
                            StorageAccess { slot, keys: BTreeSet::new(), transient: false, span }
                        }));
                    }
                }
                called |= calls_out(&inst.kind);
            }
        }
        writes
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

    /// Returns what `value` depends on.
    fn sources(&mut self, value: ValueId) -> BTreeSet<Source> {
        if let Some(sources) = self.sources.get(&value) {
            return sources.clone();
        }
        // Break phi cycles: a value being computed contributes nothing to itself.
        self.sources.insert(value, BTreeSet::new());
        let function = self.function;
        let mut sources = BTreeSet::new();
        match function.value(value) {
            Value::Arg(index) => {
                sources.insert(Source::Argument(index.index() as u32));
            }
            Value::Inst(inst_id) => {
                let kind = &function.inst(*inst_id).kind;
                if let Some(source) = self.inst_source(kind) {
                    sources.insert(source);
                } else {
                    for operand in kind.operands() {
                        sources.extend(self.sources(operand));
                    }
                }
            }
            Value::Immediate(_) | Value::Undef(_) | Value::Error(_) => {}
        }
        self.sources.insert(value, sources.clone());
        sources
    }

    /// Returns the source introduced by an instruction, if it does not merely propagate its
    /// operands.
    fn inst_source(&mut self, kind: &InstKind) -> Option<Source> {
        if let Some((slot, false, transient)) = storage_access(kind) {
            let (slot, _) = self.slot(slot);
            return Some(if transient {
                Source::TransientStorage(slot)
            } else {
                Source::Storage(slot)
            });
        }
        if external_call_kind(kind) {
            return Some(Source::CallResult);
        }
        Some(match kind {
            InstKind::Caller => Source::Caller,
            InstKind::Origin => Source::Origin,
            InstKind::CallValue => Source::CallValue,
            InstKind::LoadImmutable(..) => Source::Immutable,
            InstKind::ReturnDataSize | InstKind::ReturnDataCopy(..) => Source::CallResult,
            InstKind::CalldataLoad(..)
            | InstKind::CalldataSize
            | InstKind::CalldataCopy(..)
            | InstKind::CalldataSliceLoadWord { .. } => Source::Calldata,
            InstKind::MLoad(..)
            | InstKind::MemoryObjectLoadField { .. }
            | InstKind::MemoryObjectLoadElement { .. }
            | InstKind::MemoryObjectLoadByte { .. }
            | InstKind::MemorySliceLoadWord { .. }
            | InstKind::FrameLoad { .. } => Source::Memory,
            InstKind::GasPrice
            | InstKind::BlockHash(..)
            | InstKind::Coinbase
            | InstKind::Timestamp
            | InstKind::BlockNumber
            | InstKind::PrevRandao
            | InstKind::GasLimit
            | InstKind::SlotNum
            | InstKind::ChainId
            | InstKind::Balance(..)
            | InstKind::SelfBalance
            | InstKind::Gas
            | InstKind::BaseFee
            | InstKind::BlobBaseFee
            | InstKind::BlobHash(..)
            | InstKind::ExtCodeSize(..)
            | InstKind::ExtCodeHash(..) => Source::Environment,
            _ => return None,
        })
    }

    /// Resolves a storage slot value to the declared slot it derives from, together with what its
    /// mapping keys and array indices depend on.
    fn slot(&mut self, value: ValueId) -> (StorageSlot, BTreeSet<Source>) {
        if let Some(slot) = self.slots.get(&value) {
            return slot.clone();
        }
        self.slots.insert(value, (StorageSlot::Unknown, BTreeSet::new()));
        let function = self.function;
        let resolved = if let Some(slot) = function.value_u256(value) {
            (StorageSlot::Exact(slot), BTreeSet::new())
        } else if let Value::Inst(inst_id) = function.value(value) {
            match &function.inst(*inst_id).kind {
                InstKind::MappingSlot(key, slot)
                | InstKind::MappingSlotMemory(key, slot)
                | InstKind::MappingSlotCalldata(key, slot) => {
                    let (slot, mut keys) = self.slot(*slot);
                    keys.extend(self.sources(*key));
                    (derived(slot), keys)
                }
                InstKind::StorageArrayDataSlot(slot) => {
                    let (slot, keys) = self.slot(*slot);
                    (derived(slot), keys)
                }
                InstKind::StorageArrayElementSlot { slot, index, .. } => {
                    let (slot, mut keys) = self.slot(*slot);
                    keys.extend(self.sources(*index));
                    (derived(slot), keys)
                }
                InstKind::Add(a, b) => self.offset_slot(*a, *b),
                _ => (StorageSlot::Unknown, BTreeSet::new()),
            }
        } else {
            (StorageSlot::Unknown, BTreeSet::new())
        };
        self.slots.insert(value, resolved.clone());
        resolved
    }

    /// Resolves `a + b` where one operand is a slot and the other an offset or index.
    fn offset_slot(&mut self, a: ValueId, b: ValueId) -> (StorageSlot, BTreeSet<Source>) {
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
            (Some(a), Some(b)) => (StorageSlot::Exact(a.wrapping_add(b)), BTreeSet::new()),
            (None, None) => {
                let (slot, mut keys, index) = match self.slot(a) {
                    (StorageSlot::Unknown, _) => {
                        let (slot, keys) = self.slot(b);
                        (slot, keys, a)
                    }
                    (slot, keys) => (slot, keys, b),
                };
                if slot == StorageSlot::Unknown {
                    return (StorageSlot::Unknown, BTreeSet::new());
                }
                keys.extend(self.sources(index));
                (derived(slot), keys)
            }
        }
    }
}

/// Returns the source regions of `unchecked` and inline assembly blocks.
fn unchecked_regions(gcx: Gcx<'_>) -> Vec<Span> {
    fn collect(stmts: &[hir::Stmt<'_>], regions: &mut Vec<Span>) {
        for stmt in stmts {
            match &stmt.kind {
                hir::StmtKind::UncheckedBlock(_) | hir::StmtKind::AssemblyBlock(_) => {
                    regions.push(stmt.span);
                }
                hir::StmtKind::Block(block) => collect(block.stmts, regions),
                hir::StmtKind::Loop(block, source) => {
                    collect(block.stmts, regions);
                    if let hir::LoopSource::For { update: Some(update) } = source {
                        collect(std::slice::from_ref(*update), regions);
                    }
                }
                hir::StmtKind::If(_, then_stmt, else_stmt) => {
                    collect(std::slice::from_ref(*then_stmt), regions);
                    if let Some(else_stmt) = else_stmt {
                        collect(std::slice::from_ref(*else_stmt), regions);
                    }
                }
                hir::StmtKind::Switch(switch) => {
                    for case in switch.cases {
                        collect(case.body.stmts, regions);
                    }
                }
                hir::StmtKind::Try(try_) => {
                    for clause in try_.clauses {
                        collect(clause.block.stmts, regions);
                    }
                }
                _ => {}
            }
        }
    }

    let mut regions = Vec::new();
    for id in gcx.hir.function_ids() {
        if let Some(body) = &gcx.hir.function(id).body {
            collect(body.stmts, &mut regions);
        }
    }
    regions
}

/// Marks a slot as derived from its declared base.
fn derived(slot: StorageSlot) -> StorageSlot {
    match slot {
        StorageSlot::Exact(slot) | StorageSlot::Derived(slot) => StorageSlot::Derived(slot),
        StorageSlot::Unknown => StorageSlot::Unknown,
    }
}

/// Returns `(slot, is_write, is_transient)` for a storage access instruction.
fn storage_access(kind: &InstKind) -> Option<(ValueId, bool, bool)> {
    Some(match kind {
        InstKind::SLoad(slot) => (*slot, false, false),
        InstKind::SStore(slot, _) => (*slot, true, false),
        InstKind::TLoad(slot) => (*slot, false, true),
        InstKind::TStore(slot, _) => (*slot, true, true),
        InstKind::ValidateStorageBytes(slot) | InstKind::StorageBytesLoad(slot) => {
            (*slot, false, false)
        }
        InstKind::StorageArrayLoad { slot, .. } => (*slot, false, false),
        InstKind::StorageToMemory { storage, .. } => (*storage, false, false),
        InstKind::StorageBytesStore(slot, _)
        | InstKind::StorageClearWords(slot, ..)
        | InstKind::StorageBytesStoreLiteral { slot, .. } => (*slot, true, false),
        InstKind::MemoryToStorage { storage, .. } | InstKind::ClearStorage { storage, .. } => {
            (*storage, true, false)
        }
        _ => return None,
    })
}

/// Returns whether an instruction transfers control to another contract.
fn external_call_kind(kind: &InstKind) -> bool {
    matches!(
        kind,
        InstKind::AddressCall { .. }
            | InstKind::Call { .. }
            | InstKind::CallCode { .. }
            | InstKind::StaticCall { .. }
            | InstKind::DelegateCall { .. }
            | InstKind::Create(..)
            | InstKind::Create2(..)
            | InstKind::ICall { function: Callee::Builtin(Builtin::Transfer | Builtin::Send), .. }
    )
}
