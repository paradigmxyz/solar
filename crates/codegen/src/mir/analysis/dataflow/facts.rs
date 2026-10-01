//! Storage facts exported to optimization passes.
//!
//! [`StorageFacts`] snapshots the storage path analysis for one module so that function
//! passes can ask, for any instruction, which storage paths it may read or write. Internal
//! calls answer with their callee's footprint instantiated at the call site instead of the
//! whole address space. External calls answer with what they can cause:
//!
//! - a static call cannot change state, but a view it re-enters may read storage;
//! - a call to an active precompile touches none of this contract's storage;
//! - any other call may read and write arbitrary storage through callbacks. This includes internal
//!   helpers that transitively call foreign code, and execution via delegatecall where a callback
//!   can reach code outside this module.
//!
//! Delegate calls run foreign code against this storage and access everything. The facts
//! use the storage layout assumptions of [`storage_path`](super::storage_path) and are only
//! valid for the MIR they were computed from; experimental passes compute them at entry and must
//! not consult them for instructions they create.

use super::{
    interproc::{ContextPolicy, SummaryEngine},
    reentrancy::{self, CallKind},
    storage::{FunctionStorage, StorageAnalysis},
    storage_path::{Activation, PathId, PathTable},
};
use crate::mir::{
    Callee, Function, FunctionId, InstId, InstKind, MangledSymbol, Module, analysis::CallGraphInfo,
};
use solar_config::EvmVersion;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use std::sync::Arc;

/// The storage an instruction may access.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Footprint<'a> {
    /// Paths possibly read, or `None` for every path.
    pub(crate) reads: Option<&'a [PathId]>,
    /// Paths possibly written, or `None` for every path.
    pub(crate) writes: Option<&'a [PathId]>,
    /// Whether the instruction may end the transaction successfully, so no later write
    /// overwrites anything written before it.
    pub(crate) terminates: bool,
    /// Whether the accesses belong to another activation of this contract.
    pub(crate) reentrant: bool,
}

/// Precomputed call behavior of one function's instructions.
#[derive(Clone, Debug)]
struct FunctionFacts {
    storage: FunctionStorage,
    /// External calls and whether their target may run code that calls back.
    external_calls: FxHashMap<InstId, (CallKind, bool)>,
    /// Internal calls that can transitively transfer control to foreign code.
    callback_calls: DenseBitSet<InstId>,
}

/// Storage facts of a module, shared by function passes.
#[derive(Debug)]
pub(crate) struct StorageFacts {
    table: PathTable,
    functions: FxHashMap<MangledSymbol, FunctionFacts>,
}

impl StorageFacts {
    /// Analyzes `module`.
    pub(crate) fn compute(
        module: &Module,
        evm_version: EvmVersion,
        selected: &DenseBitSet<FunctionId>,
    ) -> Arc<Self> {
        let mut storage =
            SummaryEngine::new(module, StorageAnalysis::new(module), ContextPolicy::INSENSITIVE);
        let graph = CallGraphInfo::new(module);
        let mut callbacks = DenseBitSet::new_empty(module.functions.len());
        for (func, function) in module.iter_functions() {
            if function.blocks.is_empty()
                || function
                    .instructions()
                    .any(|inst| reentrancy::call_operands(&function.inst(inst).kind).is_some())
            {
                callbacks.insert(func);
            }
        }
        loop {
            let mut changed = false;
            for (func, _) in module.iter_functions() {
                if !callbacks.contains(func) && graph.callees(func).any(|f| callbacks.contains(f)) {
                    changed |= callbacks.insert(func);
                }
            }
            if !changed {
                break;
            }
        }
        let mut functions = FxHashMap::default();
        for (func, function) in module.iter_functions() {
            if !selected.contains(func) || function.blocks.is_empty() {
                continue;
            }
            let context = storage.general_context(func);
            let _ = storage.summary(func, &context);
            let Some(mut results) =
                storage.analysis.functions.get(&(func, context)).map(|results| (**results).clone())
            else {
                continue;
            };
            // Footprints conservatively combine the two address spaces. Ignoring transient
            // accesses is unsound for guard elimination across internal calls.
            let external_calls = external_calls(function, evm_version);
            let mut callback_calls = DenseBitSet::new_empty(function.num_insts());
            for inst in function.instructions() {
                if let InstKind::ICall { function: Callee::Function(callee), .. } =
                    function.inst(inst).kind
                    && callbacks.contains(callee)
                {
                    callback_calls.insert(inst);
                }
                if let Some(access) = results.accesses.get_mut(&inst)
                    && !matches!(
                        function.inst(inst).kind,
                        InstKind::SLoad(..)
                            | InstKind::SStore(..)
                            | InstKind::TLoad(..)
                            | InstKind::TStore(..)
                    )
                {
                    access.reads.extend_from_slice(&access.transient_reads);
                    access.writes.extend_from_slice(&access.transient_writes);
                }
            }
            functions.insert(
                function.name,
                FunctionFacts { storage: results, external_calls, callback_calls },
            );
        }
        Arc::new(Self { table: storage.analysis.table, functions })
    }

    /// Returns the storage `inst` of `func` may access, or `None` when the facts do not
    /// describe it.
    pub(crate) fn footprint(&self, func: &Function, inst: InstId) -> Option<Footprint<'_>> {
        let facts = self.functions.get(&func.name)?;
        if facts.callback_calls.contains(inst) {
            return Some(Footprint {
                reads: None,
                writes: None,
                terminates: facts.storage.terminating_calls.contains(&inst),
                reentrant: true,
            });
        }
        if let Some(&(kind, calls_back)) = facts.external_calls.get(&inst) {
            if matches!(kind, CallKind::DelegateCall | CallKind::CallCode) {
                return Some(Footprint {
                    reads: None,
                    writes: None,
                    terminates: false,
                    reentrant: true,
                });
            }
            if !calls_back {
                return Some(Footprint {
                    reads: Some(&[]),
                    writes: Some(&[]),
                    terminates: false,
                    reentrant: true,
                });
            }
            // A stipend is not a static context: TSTORE is allowed below 2300 gas.
            let static_call = matches!(kind, CallKind::StaticCall);
            return Some(Footprint {
                reads: None,
                writes: if static_call { Some(&[]) } else { None },
                terminates: false,
                reentrant: true,
            });
        }
        let accesses = facts.storage.accesses.get(&inst);
        Some(Footprint {
            reads: Some(accesses.map_or(&[][..], |accesses| accesses.reads.as_slice())),
            writes: Some(accesses.map_or(&[][..], |accesses| accesses.writes.as_slice())),
            terminates: facts.storage.terminating_calls.contains(&inst),
            reentrant: false,
        })
    }

    /// Returns the paths a persistent storage load or store of `func` accesses.
    pub(crate) fn slot_paths(&self, func: &Function, inst: InstId) -> Option<&[PathId]> {
        self.slot_access(func, inst).filter(|&(_, transient)| !transient).map(|(paths, _)| paths)
    }

    /// Returns the paths a persistent or transient load or store accesses, and whether they
    /// are transient.
    pub(crate) fn slot_access(&self, func: &Function, inst: InstId) -> Option<(&[PathId], bool)> {
        let accesses = self.functions.get(&func.name)?.storage.accesses.get(&inst)?;
        [
            (&accesses.writes, false),
            (&accesses.reads, false),
            (&accesses.transient_writes, true),
            (&accesses.transient_reads, true),
        ]
        .into_iter()
        .find(|(paths, _)| !paths.is_empty())
        .map(|(paths, transient)| (paths.as_slice(), transient))
    }

    /// Returns whether two accesses always touch the same slot within one activation, even
    /// through different SSA slot values: each has one stable path and the paths are equal.
    pub(crate) fn same_location(&self, a: &[PathId], b: &[PathId]) -> bool {
        matches!((a, b), (&[x], &[y]) if x == y && self.table.is_stable(x))
    }

    /// Returns the absolute slot of `path`, if it is one.
    pub(crate) fn exact_slot(&self, path: PathId) -> Option<alloy_primitives::U256> {
        self.table.as_slot(path)
    }

    /// Returns whether a path accessed at one point may alias any path of a footprint.
    pub(crate) fn may_touch(
        &self,
        paths: &[PathId],
        footprint: Option<&[PathId]>,
        reentrant: bool,
    ) -> bool {
        let Some(footprint) = footprint else { return true };
        let activation = if reentrant { Activation::Different } else { Activation::Same };
        paths.iter().any(|&path| {
            path == PathTable::UNKNOWN
                || footprint.iter().any(|&other| self.table.may_alias(path, other, activation))
        })
    }
}

/// Classifies the external calls of `function` by whether their target may call back.
fn external_calls(
    function: &Function,
    evm_version: EvmVersion,
) -> FxHashMap<InstId, (CallKind, bool)> {
    let mut calls = FxHashMap::default();
    for inst in function.instructions() {
        let kind = &function.inst(inst).kind;
        let Some(operands) = reentrancy::call_operands(kind) else { continue };
        // Initcode prefixes and constructor assignments do not prove deployed runtime
        // behavior. Only active precompiles have an independently known implementation.
        let calls_back = !operands
            .target
            .and_then(|target| function.value_u256(target))
            .is_some_and(|address| reentrancy::is_precompile_at(address, evm_version));
        calls.insert(inst, (operands.kind, calls_back));
    }
    calls
}
