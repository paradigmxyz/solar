//! Storage facts exported to optimization passes.
//!
//! [`StorageFacts`] snapshots the storage path analysis for one module so that function
//! passes can ask, for any instruction, which storage paths it may read or write. Internal
//! calls answer with their callee's footprint instantiated at the call site instead of the
//! whole address space. External calls answer with what they can cause:
//!
//! - a static call cannot change state, but a view it re-enters may read storage;
//! - a call whose target runs no code that can call back, such as a precompile or code compiled
//!   here without call instructions, touches none of this contract's storage;
//! - any other call may re-enter this contract, which can then read and write whatever some
//!   function of the module reads and writes.
//!
//! Delegate calls run foreign code against this storage and access everything. The facts
//! use the storage layout assumptions of [`storage_path`](super::storage_path) and are only
//! valid for the MIR they were computed from; passes compute them at entry and must not
//! consult them for instructions they create.

use super::{
    interproc::ContextPolicy,
    reentrancy::{self, CallKind, Trust},
    storage::FunctionStorage,
    storage_path::{Activation, PathId, PathTable},
};
use crate::mir::{Function, InstId, MangledSymbol, Module};
use solar_config::EvmVersion;
use solar_data_structures::map::FxHashMap;
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
#[derive(Clone, Debug, Default)]
struct FunctionFacts {
    storage: FunctionStorage,
    /// External calls and whether their target may run code that calls back.
    external_calls: FxHashMap<InstId, (CallKind, bool)>,
}

/// Storage facts of a module, shared by function passes.
#[derive(Debug)]
pub(crate) struct StorageFacts {
    table: PathTable,
    functions: FxHashMap<MangledSymbol, FunctionFacts>,
    /// Everything any function of the module may read or write, for reentrant calls.
    all_reads: Option<Vec<PathId>>,
    all_writes: Option<Vec<PathId>>,
}

impl StorageFacts {
    /// Analyzes `module`.
    pub(crate) fn compute(module: &Module, evm_version: EvmVersion) -> Arc<Self> {
        let mut storage = super::storage::analyze_module(module, ContextPolicy::INSENSITIVE);
        let entries = reentrancy::runtime_entries(module);
        let module_facts = reentrancy::module_facts(module, &mut storage, &entries, evm_version);
        // Reentrant calls run external entries, whose summaries include their callees.
        let mut roots = entries.clone();
        roots.extend(module.dispatch_entry());
        if roots.is_empty() {
            roots.extend(module.iter_functions().map(|(func, _)| func));
        }
        let mut all_reads = Some(Vec::new());
        let mut all_writes = Some(Vec::new());
        for &root in &roots {
            if module.function(root).blocks.is_empty() {
                continue;
            }
            let context = storage.general_context(root);
            let summary = storage.summary(root, &context);
            let table = &mut storage.analysis.table;
            for (all, paths) in [
                (&mut all_reads, summary.reads.iter().chain(&summary.transient_reads)),
                (&mut all_writes, summary.writes.iter().chain(&summary.transient_writes)),
            ] {
                for &path in paths {
                    let erased = table.erase_keys(path);
                    if erased == PathTable::UNKNOWN || table.is_relative(erased) {
                        *all = None;
                    } else if let Some(all) = all
                        && !all.contains(&erased)
                    {
                        all.push(erased);
                    }
                }
            }
        }
        let mut functions = FxHashMap::default();
        for (func, function) in module.iter_functions() {
            if function.blocks.is_empty() {
                continue;
            }
            let context = storage.general_context(func);
            let _ = storage.summary(func, &context);
            let table = &mut storage.analysis.table;
            let Some(results) =
                storage.analysis.functions.get(&(func, context)).map(|results| (**results).clone())
            else {
                continue;
            };
            let external_calls =
                external_calls(module, &module_facts, table, function, &results, evm_version);
            functions.insert(function.name, FunctionFacts { storage: results, external_calls });
        }
        Arc::new(Self { table: storage.analysis.table, functions, all_reads, all_writes })
    }

    /// Returns the storage `inst` of `func` may access, or `None` when the facts do not
    /// describe it.
    pub(crate) fn footprint(&self, func: &Function, inst: InstId) -> Option<Footprint<'_>> {
        let facts = self.functions.get(&func.name)?;
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
            let static_call = matches!(kind, CallKind::StaticCall | CallKind::Stipend);
            return Some(Footprint {
                reads: self.all_reads.as_deref(),
                writes: if static_call { Some(&[]) } else { self.all_writes.as_deref() },
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

    /// Returns whether `path` lies in a hashed area, which never overlaps absolute slots.
    pub(crate) fn is_hashed(&self, path: PathId) -> bool {
        self.table.is_hashed(path)
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
    module: &Module,
    facts: &reentrancy::ModuleFacts,
    table: &mut PathTable,
    function: &Function,
    storage: &FunctionStorage,
    evm_version: EvmVersion,
) -> FxHashMap<InstId, (CallKind, bool)> {
    let mut calls = FxHashMap::default();
    for inst in function.instructions() {
        let kind = &function.inst(inst).kind;
        let Some(operands) = reentrancy::call_operands(kind) else { continue };
        let target = match (operands.kind, operands.target) {
            (CallKind::Create, _) | (_, None) => reentrancy::Provenance::Unknown,
            (_, Some(target)) => {
                reentrancy::target_provenance(module, storage, function, target, evm_version, 0)
            }
        };
        let target = match (operands.kind, kind) {
            (CallKind::Create, crate::mir::InstKind::Create(_, offset, _))
            | (CallKind::Create, crate::mir::InstKind::Create2(_, offset, _, _)) => {
                reentrancy::created_provenance(module, function, *offset, evm_version)
            }
            _ => target,
        };
        let calls_back = match target {
            reentrancy::Provenance::Constant(address)
                if address.is_zero() || reentrancy::is_precompile(address) =>
            {
                false
            }
            // A function argument may be anything; `This` re-enters by definition.
            target => facts.code_trust(table, target) != Some(Trust::NoCallback),
        };
        calls.insert(inst, (operands.kind, calls_back));
    }
    calls
}
