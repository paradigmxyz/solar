//! Backward liveness of storage writes over symbolic paths.
//!
//! A storage path is live at a program point when some later execution may read it before
//! overwriting it. The backward analysis starts from each exit: a revert discards every
//! write, so nothing is live there, while a successful end of the transaction and a return
//! to an unknown caller keep everything observable. Reads and calls generate liveness; an
//! exact write to a single path kills the paths it must overwrite.
//!
//! Internal calls use the callee's storage footprint instantiated at the call site. External
//! calls that may run other code make everything live, because reentrant entries may read
//! any storage; static calls can only re-enter views, which read storage too, so they are
//! treated the same way. A write whose path is dead immediately after it is never observed,
//! which is the fact interprocedural dead-store elimination consumes.

use super::{
    engine::{self, Analysis, Direction},
    lattice::JoinSemiLattice,
    storage::FunctionStorage,
    storage_path::{Activation, PathId, PathTable},
};
use crate::mir::{
    BlockId, Function, FunctionId, InstId, InstKind, Module, Terminator,
    analysis::{AliasResult, CfgInfo},
};
use solar_data_structures::map::FxHashMap;
use std::{collections::BTreeSet, fmt::Write as _};

/// Paths that may be read later.
///
/// With `all`, every path is live except those in `paths`, which are definitely
/// overwritten first; otherwise exactly the paths in `paths` are live.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Live {
    /// Whether the set is a complement.
    pub(crate) all: bool,
    /// Live paths, or overwritten paths when `all` is set.
    pub(crate) paths: BTreeSet<PathId>,
}

impl Live {
    fn everything() -> Self {
        Self { all: true, paths: BTreeSet::new() }
    }

    /// Records a read of `path`.
    fn read(&mut self, table: &PathTable, path: PathId) {
        if self.all {
            self.paths.retain(|&killed| !table.may_alias(killed, path, Activation::Same));
        } else if path == PathTable::UNKNOWN {
            *self = Self::everything();
        } else {
            self.paths.insert(path);
        }
    }

    /// Records a write that definitely overwrites `path`.
    fn kill(&mut self, table: &PathTable, path: PathId) {
        if self.all {
            self.paths.insert(path);
        } else {
            self.paths.retain(|&live| {
                table.alias(live, path, Activation::Same) != AliasResult::MustAlias
            });
        }
    }

    /// Returns whether a write to `path` may be read later.
    pub(crate) fn is_live(&self, table: &PathTable, path: PathId) -> bool {
        if self.all {
            !self.paths.iter().any(|&killed| {
                table.alias(killed, path, Activation::Same) == AliasResult::MustAlias
            })
        } else {
            self.paths.iter().any(|&live| table.may_alias(live, path, Activation::Same))
        }
    }

    fn join_with(&mut self, table: &PathTable, other: &Self) -> bool {
        let before = self.clone();
        match (self.all, other.all) {
            (false, false) => self.paths.extend(other.paths.iter().copied()),
            (true, true) => self.paths.retain(|path| other.paths.contains(path)),
            (true, false) => self.paths.retain(|&killed| !other.is_live(table, killed)),
            (false, true) => {
                let killed = other
                    .paths
                    .iter()
                    .copied()
                    .filter(|&killed| !self.is_live(table, killed))
                    .collect();
                *self = Self { all: true, paths: killed };
            }
        }
        *self != before
    }
}

/// A [`Live`] set paired with the table that interprets it, so joins can compare paths.
#[derive(Clone, Debug)]
struct LiveState<'a> {
    live: Live,
    table: &'a PathTable,
}

impl JoinSemiLattice for LiveState<'_> {
    fn join(&mut self, other: &Self) -> bool {
        self.live.join_with(self.table, &other.live)
    }
}

struct Transfer<'a> {
    storage: &'a FunctionStorage,
    table: &'a PathTable,
}

impl<'a> Analysis for Transfer<'a> {
    type Domain = LiveState<'a>;

    const DIRECTION: Direction = Direction::Backward;

    fn bottom(&self, _func: &Function) -> LiveState<'a> {
        LiveState { live: Live::default(), table: self.table }
    }

    fn initialize_boundary(&mut self, func: &Function, block: BlockId, state: &mut LiveState<'a>) {
        state.live = match func.blocks[block].terminator {
            Some(
                Terminator::Revert { .. } | Terminator::RevertReturndata | Terminator::Invalid,
            ) => Live::default(),
            // A successful end keeps writes; a return hands them to an unknown caller.
            _ => Live::everything(),
        };
    }

    fn apply_instruction(
        &mut self,
        func: &Function,
        _block: BlockId,
        inst: InstId,
        state: &mut LiveState<'a>,
    ) {
        let table = self.table;
        let state = &mut state.live;
        let kind = &func.inst(inst).kind;
        let accesses = self.storage.accesses.get(&inst);
        // Code that may run elsewhere, including reentrant views, may read anything.
        if super::reentrancy::call_operands(kind).is_some() {
            *state = Live::everything();
            return;
        }
        let Some(accesses) = accesses else { return };
        // A write to one exact path overwrites it; anything else may leave old values.
        if let InstKind::SStore(..) = kind
            && let &[path] = accesses.writes.as_slice()
            && path != PathTable::UNKNOWN
        {
            state.kill(table, path);
        }
        for &path in &accesses.reads {
            state.read(table, path);
        }
    }

    fn apply_terminator(&mut self, _func: &Function, _block: BlockId, _state: &mut LiveState<'a>) {}
}

/// Writes the storage writes of every function and whether they may be observed.
pub(crate) fn dump(module: &Module, out: &mut String) {
    let mut storage =
        super::storage::analyze_module(module, super::interproc::ContextPolicy::INSENSITIVE);
    let mut results = FxHashMap::<FunctionId, Vec<(InstId, bool)>>::default();
    for (func, function) in module.iter_functions() {
        if function.blocks.is_empty() {
            continue;
        }
        let context = storage.general_context(func);
        let _ = storage.summary(func, &context);
        let Some(facts) = storage.analysis.functions.get(&(func, context)).cloned() else {
            continue;
        };
        let cfg = CfgInfo::new(function);
        let mut transfer = Transfer { storage: &facts, table: &storage.analysis.table };
        let solved = engine::solve(function, &cfg, &mut transfer);
        let mut stores = Vec::new();
        for &block in cfg.rpo() {
            // Walk the block backward from its exit state to see liveness after each write.
            let mut state = solved.state(block).clone();
            let instructions = &function.blocks[block].instructions;
            let mut facts_in_block = Vec::new();
            for &inst in instructions.iter().rev() {
                if let InstKind::SStore(..) = function.inst(inst).kind
                    && let Some(access) = facts.accesses.get(&inst)
                {
                    let live =
                        access.writes.iter().any(|&path| state.live.is_live(transfer.table, path));
                    facts_in_block.push((inst, live));
                }
                if !matches!(function.inst(inst).kind, InstKind::Phi(_)) {
                    transfer.apply_instruction(function, block, inst, &mut state);
                }
            }
            facts_in_block.reverse();
            stores.extend(facts_in_block);
        }
        results.insert(func, stores);
    }
    for (func, function) in module.iter_functions() {
        let Some(stores) = results.get(&func) else { continue };
        let _ = writeln!(out, "fn @{}:", function.name);
        for &(inst, live) in stores {
            let _ = writeln!(
                out,
                "    {}  ; {}",
                crate::mir::display::display_instruction(function, Some(module), inst),
                if live { "live" } else { "dead" }
            );
        }
    }
}
