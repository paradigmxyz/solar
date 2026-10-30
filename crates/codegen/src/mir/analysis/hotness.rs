//! Hotness of MIR functions and blocks.
//!
//! The analysis writes `Function::attributes.hotness` and `BasicBlock::hotness`. The pass
//! manager reruns it before the first pass and after every pass that changes the module, so a
//! pass and the backend read current values and never recompute them.
//!
//! A function is cold when every exit it can reach aborts: a revert, an invalid instruction, or
//! a call or tail call to another cold function. The analysis iterates to a fixpoint over the
//! module, so chains of panic helpers become cold together. A module built with `-O none` has no
//! cold functions.
//!
//! A block is cold by the rules of its function's objective, the module's unless the function
//! has one of its own:
//! - Every objective but `-O none` marks the blocks that abort, and then the blocks whose
//!   successors are all cold, backward to a fixpoint. `-O none` marks only the blocks that abort.
//! - An objective that marks failure paths cold ([`Target::marks_failure_paths_cold`]) also marks
//!   every block with no path to a normal exit over blocks that are not cold, such as a loop whose
//!   only exits lead to aborts. A size build keeps the code around a failure path in place: moving
//!   it saves no bytes, and the layout it changes can widen jump labels.

use crate::{
    mir::{BlockId, Callee, Function, FunctionId, Hotness, InstKind, Module, Terminator},
    target::Target,
};
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    index::IndexVec,
};
use solar_sema::Gcx;

/// Computes function and block hotness with reusable buffers.
pub(crate) struct HotnessAnalysis {
    cold_functions: DenseBitSet<FunctionId>,
    cold_blocks: DenseBitSet<BlockId>,
    hot_blocks: DenseBitSet<BlockId>,
    worklist: Vec<BlockId>,
    visited: GrowableBitSet<BlockId>,
    /// The functions whose walk met a call to each function while it was not cold.
    callers: IndexVec<FunctionId, Vec<FunctionId>>,
    queue: Vec<FunctionId>,
    queued: DenseBitSet<FunctionId>,
    /// Functions with a path to a normal exit that makes no call, so no callee's coldness can
    /// change theirs. Valid until the function changes.
    always_hot: GrowableBitSet<FunctionId>,
}

impl HotnessAnalysis {
    pub(crate) fn new() -> Self {
        Self {
            cold_functions: DenseBitSet::new_empty(0),
            cold_blocks: DenseBitSet::new_empty(0),
            hot_blocks: DenseBitSet::new_empty(0),
            worklist: Vec::new(),
            visited: GrowableBitSet::new_empty(),
            callers: IndexVec::new(),
            queue: Vec::new(),
            queued: DenseBitSet::new_empty(0),
            always_hot: GrowableBitSet::new_empty(),
        }
    }

    /// Recomputes the hotness of every function and block of `module`.
    pub(crate) fn run(&mut self, gcx: Gcx<'_>, module: &mut Module) {
        self.always_hot.clear();
        self.run_functions(module);
        for function in module.functions.indices() {
            self.run_function(gcx, module, function);
        }
    }

    /// Recomputes the hotness after a pass that edited only the `changed` functions: the blocks
    /// of those, and of every function if a function's coldness changed, since its callers' blocks
    /// call it.
    pub(crate) fn update(
        &mut self,
        gcx: Gcx<'_>,
        module: &mut Module,
        changed: &GrowableBitSet<FunctionId>,
    ) {
        for function in changed.iter() {
            self.always_hot.remove(function);
        }
        if self.run_functions(module) {
            for function in module.functions.indices() {
                self.run_function(gcx, module, function);
            }
        } else {
            for function in changed.iter() {
                self.run_function(gcx, module, function);
            }
        }
    }

    /// Recomputes which functions are cold, writing their attributes. Returns whether any
    /// changed.
    pub(crate) fn run_functions(&mut self, module: &mut Module) -> bool {
        self.compute_functions(module);
        let mut changed = false;
        for (id, func) in module.functions.iter_mut_enumerated() {
            let new = hotness(self.cold_functions.contains(id));
            changed |= func.attributes.hotness != new;
            func.attributes.hotness = new;
        }
        changed
    }

    /// Recomputes the hotness of the blocks of `function`, after its callees' attributes are
    /// current.
    pub(crate) fn run_function(&mut self, gcx: Gcx<'_>, module: &mut Module, function: FunctionId) {
        self.compute_blocks(gcx, module, function);
        for (block, body) in module.functions[function].blocks.iter_mut_enumerated() {
            body.hotness = hotness(self.cold_blocks.contains(block));
        }
    }

    /// Returns whether every hotness attribute of `module` matches a fresh computation.
    pub(crate) fn is_current(&mut self, gcx: Gcx<'_>, module: &Module) -> bool {
        self.always_hot.clear();
        self.compute_functions(module);
        module
            .functions
            .iter_enumerated()
            .all(|(id, func)| func.attributes.hotness.is_cold() == self.cold_functions.contains(id))
            && module.functions.iter_enumerated().all(|(id, func)| {
                self.compute_blocks(gcx, module, id);
                func.blocks
                    .iter_enumerated()
                    .all(|(block, body)| body.hotness.is_cold() == self.cold_blocks.contains(block))
            })
    }

    /// Finds the cold functions of `module`, into `self.cold_functions`.
    fn compute_functions(&mut self, module: &Module) {
        self.cold_functions.clear_to(module.functions.len());
        if !module.optimization.is_none() {
            self.collect_cold_functions(module);
        }
    }

    /// Finds the cold blocks of `function`, into `self.cold_blocks`, by the rules of its
    /// objective.
    fn compute_blocks(&mut self, gcx: Gcx<'_>, module: &Module, function: FunctionId) {
        let func = &module.functions[function];
        let objective = func.attributes.optimization.unwrap_or(module.optimization);
        let marks_failure_paths_cold = Target::new(gcx, objective).marks_failure_paths_cold();
        self.collect_cold_blocks(module, func, !objective.is_none(), marks_failure_paths_cold);
    }

    /// Finds the functions whose reachable exits all abort, into `self.cold_functions`.
    ///
    /// Coldness only grows, so a worklist reaches the fixpoint: each function is walked once,
    /// and again only after a callee that its walk met becomes cold.
    fn collect_cold_functions(&mut self, module: &Module) {
        let functions = module.functions.len();
        for callers in &mut self.callers {
            callers.clear();
        }
        self.callers.resize_with(functions.max(self.callers.len()), Vec::new);
        self.queued.clear_to(functions);
        self.queue.clear();
        // Popped in function order.
        self.queue
            .extend(module.functions.indices().rev().filter(|&f| !self.always_hot.contains(f)));
        for &function in &self.queue {
            self.queued.insert(function);
        }
        while let Some(function) = self.queue.pop() {
            self.queued.remove(function);
            if self.is_cold_function(module, function) {
                self.cold_functions.insert(function);
                for &caller in &self.callers[function] {
                    if !self.cold_functions.contains(caller) && self.queued.insert(caller) {
                        self.queue.push(caller);
                    }
                }
            }
        }
    }

    /// Whether every exit reachable in `function` aborts, given the cold functions found so far.
    /// Records `function` as a caller of each function whose walk met and found not cold.
    fn is_cold_function(&mut self, module: &Module, function: FunctionId) -> bool {
        let func = &module.functions[function];
        self.worklist.clear();
        self.worklist.push(BlockId::ENTRY);
        self.visited.clear();
        let mut saw_exit = false;
        let mut all_exits_cold = true;
        let mut calls = false;
        while let Some(block_id) = self.worklist.pop()
            && all_exits_cold
        {
            if !self.visited.insert(block_id) {
                continue;
            }
            let block = &func.blocks[block_id];
            let mut calls_cold = false;
            for &inst in &block.instructions {
                if let InstKind::ICall { function: Callee::Function(callee), .. } =
                    func.inst(inst).kind
                {
                    calls = true;
                    if self.cold_functions.contains(callee) {
                        calls_cold = true;
                    } else {
                        self.add_caller(callee, function);
                    }
                }
            }
            if calls_cold {
                saw_exit = true;
                continue;
            }
            let Some(term) = block.terminator.as_ref() else {
                all_exits_cold = false;
                continue;
            };
            match term {
                Terminator::Revert { .. } | Terminator::RevertReturndata | Terminator::Invalid => {
                    saw_exit = true;
                }
                Terminator::TailCall { function: callee, .. } => {
                    calls = true;
                    if self.cold_functions.contains(*callee) {
                        saw_exit = true;
                    } else {
                        self.add_caller(*callee, function);
                        all_exits_cold = false;
                    }
                }
                _ => {
                    let successors = term.successors();
                    if successors.is_empty() {
                        all_exits_cold = false;
                    } else {
                        self.worklist.extend(successors);
                    }
                }
            }
        }
        if !all_exits_cold && !calls {
            self.always_hot.insert(function);
        }
        saw_exit && all_exits_cold
    }

    fn add_caller(&mut self, callee: FunctionId, caller: FunctionId) {
        let callers = &mut self.callers[callee];
        if callers.last() != Some(&caller) {
            callers.push(caller);
        }
    }

    /// Finds the cold blocks of `func`, into `self.cold_blocks`, reading its callees' attributes.
    fn collect_cold_blocks(
        &mut self,
        module: &Module,
        func: &Function,
        propagate: bool,
        marks_failure_paths_cold: bool,
    ) {
        let is_cold =
            |function: FunctionId| module.functions[function].attributes.hotness.is_cold();
        self.cold_blocks.clear_to(func.blocks.len());
        self.worklist.clear();
        for block_id in func.blocks.indices() {
            if block_aborts(func, block_id, is_cold) {
                self.cold_blocks.insert(block_id);
                self.worklist.push(block_id);
            }
        }
        if !propagate {
            return;
        }

        while let Some(block_id) = self.worklist.pop() {
            for &predecessor in &func.blocks[block_id].predecessors {
                if self.cold_blocks.contains(predecessor) {
                    continue;
                }
                let Some(term) = func.blocks[predecessor].terminator.as_ref() else {
                    continue;
                };
                let successors = term.successors();
                if !successors.is_empty()
                    && successors.iter().all(|&successor| self.cold_blocks.contains(successor))
                {
                    self.cold_blocks.insert(predecessor);
                    self.worklist.push(predecessor);
                }
            }
        }
        if !marks_failure_paths_cold {
            return;
        }

        // Backward reachability from the blocks that leave the function without aborting (return,
        // stop, selfdestruct, or a tail call to a function that can return), over blocks that do
        // not abort, marks the hot blocks.
        self.hot_blocks.clear_to(func.blocks.len());
        for (block_id, block) in func.blocks.iter_enumerated() {
            let exits = block.terminator.as_ref().is_none_or(|term| !term.has_successors());
            if exits && !self.cold_blocks.contains(block_id) {
                self.hot_blocks.insert(block_id);
                self.worklist.push(block_id);
            }
        }
        while let Some(block_id) = self.worklist.pop() {
            for &predecessor in &func.blocks[block_id].predecessors {
                if !self.hot_blocks.contains(predecessor) && !self.cold_blocks.contains(predecessor)
                {
                    self.hot_blocks.insert(predecessor);
                    self.worklist.push(predecessor);
                }
            }
        }
        self.cold_blocks.union_not(&self.hot_blocks);
    }
}

/// Whether a block aborts instead of continuing: it reverts, or calls a cold function.
pub(crate) fn aborts(func: &Function, block: BlockId, cold: &DenseBitSet<FunctionId>) -> bool {
    block_aborts(func, block, |function| cold.contains(function))
}

fn block_aborts(func: &Function, block: BlockId, is_cold: impl Fn(FunctionId) -> bool) -> bool {
    let body = &func.blocks[block];
    body.instructions.iter().any(|&inst| {
        matches!(
            func.inst(inst).kind,
            InstKind::ICall { function: Callee::Function(function), .. } if is_cold(function)
        )
    }) || body.terminator.as_ref().is_some_and(Terminator::reverts)
        || matches!(body.terminator, Some(Terminator::TailCall { function, .. }) if is_cold(function))
}

const fn hotness(cold: bool) -> Hotness {
    if cold { Hotness::Cold } else { Hotness::Hot }
}
