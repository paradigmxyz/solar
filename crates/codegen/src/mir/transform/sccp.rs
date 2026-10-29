//! Sparse Conditional Constant Propagation (SCCP).
//!
//! Implements the Wegman-Zadeck SCCP algorithm on MIR. This is more powerful
//! than simple constant folding because it:
//! - Propagates constants through the CFG using SSA def-use chains
//! - Evaluates branch conditions to discover unreachable paths
//! - Folds phi nodes when all executable incoming values agree
//!
//! The algorithm uses a three-valued lattice per SSA value:
//! - **Top** (⊤): not yet evaluated
//! - **Constant(v)**: known compile-time constant
//! - **Bottom** (⊥): overdefined (not a constant)
//!
//! After reaching a fixed point, the rewrite phase replaces constant results with immediates,
//! folds branches and switches with one distinct executable successor into jumps, and
//! invalidates blocks that are never executable. Functions without a local constant seed are
//! skipped.

use crate::mir::{
    BlockId, Function, Immediate, InstId, InstKind, Module, Terminator, Value, ValueId,
    pass::{MirPass, run_function_pass},
    utils::{eval, fold_terminator_to_jump, invalidate_unreachable_blocks},
};
use alloy_primitives::U256;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::{FxHashMap, FxHashSet},
};
use std::collections::VecDeque;

/// Function pass for sparse conditional constant propagation.
pub(crate) struct Sccp;

impl MirPass for Sccp {
    fn name(&self) -> &'static str {
        "sccp"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| run(func) != 0)
    }
}

/// Lattice element for a single SSA value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LatticeValue {
    /// Not yet evaluated.
    Top,
    /// Known constant.
    Constant(U256),
    /// Overdefined — not a constant.
    Bottom,
}

impl LatticeValue {
    /// Meet operation: merges two lattice values.
    /// Top ∧ x = x, Bottom ∧ x = Bottom, Const(a) ∧ Const(b) = if a==b Const(a) else Bottom.
    fn meet(self, other: Self) -> Self {
        match (self, other) {
            (Self::Top, x) | (x, Self::Top) => x,
            (Self::Bottom, _) | (_, Self::Bottom) => Self::Bottom,
            (Self::Constant(a), Self::Constant(b)) => {
                if a == b {
                    Self::Constant(a)
                } else {
                    Self::Bottom
                }
            }
        }
    }

    /// Meets in `new`. Returns true if the value changed.
    /// Lattice values can only move downward: Top → Constant → Bottom.
    fn lower(&mut self, new: Self) -> bool {
        let merged = self.meet(new);
        let changed = merged != *self;
        *self = merged;
        changed
    }
}

/// Active instruction and terminator users of each value.
struct ValueUsers {
    inst_blocks: IndexVec<InstId, BlockId>,
    instructions: IndexVec<ValueId, Vec<InstId>>,
    terminators: IndexVec<ValueId, Vec<BlockId>>,
}

impl ValueUsers {
    fn new(func: &Function) -> Self {
        let mut inst_blocks = index_vec![BlockId::MAX; func.num_insts()];
        let mut instructions = index_vec![Vec::new(); func.num_values()];
        let mut terminators = index_vec![Vec::new(); func.num_values()];
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                inst_blocks[inst_id] = block_id;
                func.inst(inst_id).kind.visit_operands(|operand| {
                    instructions[operand].push(inst_id);
                });
            }
            if let Some(terminator) = &block.terminator {
                terminator.visit_operands(|operand| {
                    terminators[operand].push(block_id);
                });
            }
        }
        for users in &mut instructions {
            users.sort_unstable();
            users.dedup();
        }
        for users in &mut terminators {
            users.sort_unstable();
            users.dedup();
        }
        Self { inst_blocks, instructions, terminators }
    }
}

/// Runs SCCP on a function. Returns the total number of mutations,
/// including unreachable-block cleanup and phi repairs.
fn run(func: &mut Function) -> usize {
    if !can_change(func) {
        return 0;
    }

    let users = ValueUsers::new(func);
    let Solver { lattice, executable_blocks, executable_edges, .. } = Solver::solve(func, &users);

    // Rewrite phase: apply the lattice results to the function.
    rewrite(func, &lattice, &executable_blocks, &executable_edges)
}

/// Worklist solver holding the lattice and the executable CFG.
struct Solver<'a> {
    func: &'a Function,
    users: &'a ValueUsers,
    lattice: IndexVec<ValueId, LatticeValue>,
    /// Blocks reached by an executable edge, plus the entry.
    executable_blocks: DenseBitSet<BlockId>,
    /// CFG edges that have been taken.
    executable_edges: FxHashSet<(BlockId, BlockId)>,
    /// `(from, to)` edges waiting to be taken.
    cfg_worklist: VecDeque<(BlockId, BlockId)>,
    /// Values whose lattice entry changed and whose users need re-evaluation.
    ssa_worklist: VecDeque<ValueId>,
}

impl<'a> Solver<'a> {
    /// Propagates constants to a fixed point.
    fn solve(func: &'a Function, users: &'a ValueUsers) -> Self {
        // Initialize lattice: all values start as Top. Non-instruction operands referenced by
        // active MIR are known.
        let mut lattice = index_vec![LatticeValue::Top; func.num_values()];
        for value in func.live_values() {
            lattice[value] = match func.value(value) {
                Value::Immediate(imm) => {
                    imm.as_u256().map_or(LatticeValue::Bottom, LatticeValue::Constant)
                }
                Value::Arg(_) | Value::Undef(_) | Value::Error(_) => LatticeValue::Bottom,
                Value::Inst(_) => continue,
            };
        }
        let mut solver = Self {
            func,
            users,
            lattice,
            executable_blocks: DenseBitSet::new_empty(func.blocks.len()),
            executable_edges: FxHashSet::default(),
            cfg_worklist: VecDeque::new(),
            ssa_worklist: VecDeque::new(),
        };

        // Seed: entry block is executable.
        solver.executable_blocks.insert(BlockId::ENTRY);
        solver.visit_phis(BlockId::ENTRY);
        solver.visit_block(BlockId::ENTRY);

        // Main loop: process both worklists until empty.
        while !solver.cfg_worklist.is_empty() || !solver.ssa_worklist.is_empty() {
            // Process CFG edges.
            while let Some((from, to)) = solver.cfg_worklist.pop_front() {
                if !solver.executable_edges.insert((from, to)) {
                    continue; // Already processed this edge.
                }

                let newly_executable = solver.executable_blocks.insert(to);

                // Re-evaluate phi-like values in the target block.
                solver.visit_phis(to);

                if newly_executable {
                    // First time this block is executable — evaluate all its instructions.
                    solver.visit_block(to);
                }
            }

            // Process SSA value changes.
            while let Some(value) = solver.ssa_worklist.pop_front() {
                solver.propagate_value(value);
            }
        }
        solver
    }

    /// Evaluates the phis at the entry of a block.
    fn visit_phis(&mut self, block_id: BlockId) {
        let func = self.func;
        for &inst_id in &func.blocks[block_id].instructions {
            if matches!(func.inst(inst_id).kind, InstKind::Phi(_)) {
                self.visit_inst(inst_id, block_id);
            }
        }
    }

    /// Evaluates all non-phi instructions and the terminator of a block.
    fn visit_block(&mut self, block_id: BlockId) {
        let func = self.func;
        let block = &func.blocks[block_id];
        for &inst_id in &block.instructions {
            if !matches!(func.inst(inst_id).kind, InstKind::Phi(_)) {
                self.visit_inst(inst_id, block_id);
            }
        }
        if let Some(term) = &block.terminator {
            self.visit_terminator(term, block_id);
        }
    }

    /// Evaluates an instruction and lowers its result in the lattice.
    fn visit_inst(&mut self, inst_id: InstId, block_id: BlockId) {
        let func = self.func;
        let Some(value) = func.inst_result_value(inst_id) else { return };
        let new = match &func.inst(inst_id).kind {
            InstKind::Phi(incoming) => incoming
                .iter()
                .filter(|&&(pred, _)| self.executable_edges.contains(&(pred, block_id)))
                .fold(LatticeValue::Top, |result, &(_, operand)| {
                    result.meet(self.lattice[operand])
                }),
            kind => evaluate_instruction(func, kind, &self.lattice),
        };
        if self.lattice[value].lower(new) {
            self.ssa_worklist.push_back(value);
        }
    }

    /// Evaluates a terminator to determine which outgoing edges are taken.
    fn visit_terminator(&mut self, term: &Terminator, block_id: BlockId) {
        let mut take = |target| self.cfg_worklist.push_back((block_id, target));
        match term {
            Terminator::Jump(target) => take(*target),
            Terminator::Branch { condition, then_block, else_block } => {
                match self.lattice[*condition] {
                    LatticeValue::Constant(v) => {
                        take(if v.is_zero() { *else_block } else { *then_block })
                    }
                    LatticeValue::Bottom => {
                        // Both edges might be taken.
                        take(*then_block);
                        take(*else_block);
                    }
                    LatticeValue::Top => {}
                }
            }
            Terminator::Switch { value, default, cases } => {
                match self.lattice[*value] {
                    LatticeValue::Constant(v) => {
                        // Cases are tested in order at runtime, so a constant
                        // case match is definitive only if every earlier case
                        // is a known constant that differs from the scrutinee.
                        // Overdefined earlier cases stay feasible; an
                        // unresolved case defers the remaining edges.
                        for &(case_val, target) in cases {
                            match self.lattice[case_val] {
                                LatticeValue::Constant(cv) if cv == v => {
                                    take(target);
                                    return;
                                }
                                LatticeValue::Constant(_) => {}
                                LatticeValue::Bottom => take(target),
                                LatticeValue::Top => return,
                            }
                        }
                        take(*default);
                    }
                    LatticeValue::Bottom => {
                        // All edges might be taken.
                        take(*default);
                        for &(_, target) in cases {
                            take(target);
                        }
                    }
                    LatticeValue::Top => {}
                }
            }
            Terminator::TailCall { .. }
            | Terminator::Return { .. }
            | Terminator::Revert { .. }
            | Terminator::RevertReturndata
            | Terminator::ReturnData { .. }
            | Terminator::Stop
            | Terminator::SelfDestruct { .. }
            | Terminator::Invalid => {
                // No outgoing edges.
            }
        }
    }

    /// Propagates a value change to all users of that value.
    fn propagate_value(&mut self, value: ValueId) {
        let func = self.func;
        for &inst_id in &self.users.instructions[value] {
            let block_id = self.users.inst_blocks[inst_id];
            if self.executable_blocks.contains(block_id) {
                self.visit_inst(inst_id, block_id);
            }
        }

        for &block_id in &self.users.terminators[value] {
            if self.executable_blocks.contains(block_id)
                && let Some(terminator) = &func.blocks[block_id].terminator
            {
                self.visit_terminator(terminator, block_id);
            }
        }
    }
}

/// Evaluates a single instruction and returns its lattice value.
fn evaluate_instruction(
    func: &Function,
    kind: &InstKind,
    lattice: &IndexVec<ValueId, LatticeValue>,
) -> LatticeValue {
    let get_const = |value| match lattice[value] {
        LatticeValue::Constant(value) => Some(value),
        LatticeValue::Top | LatticeValue::Bottom => None,
    };

    if let InstKind::Select(condition, then_value, else_value) = *kind {
        return match lattice[condition] {
            LatticeValue::Constant(condition) => {
                let chosen = if condition.is_zero() { else_value } else { then_value };
                lattice[chosen]
            }
            LatticeValue::Bottom => lattice[then_value].meet(lattice[else_value]),
            LatticeValue::Top => match (get_const(then_value), get_const(else_value)) {
                (Some(then_value), Some(else_value)) if then_value == else_value => {
                    LatticeValue::Constant(then_value)
                }
                _ => LatticeValue::Top,
            },
        };
    }

    match eval::eval_typed_inst(func, kind, |value| get_const(value).ok_or(())) {
        Ok(Some(value)) => LatticeValue::Constant(value),
        Ok(None) => LatticeValue::Bottom,
        Err(()) if zero_divisor(kind, get_const) => LatticeValue::Constant(U256::ZERO),
        // If any operand is Bottom, the result is Bottom; otherwise it is still waiting.
        Err(()) if kind.any_operand(|value| lattice[value] == LatticeValue::Bottom) => {
            LatticeValue::Bottom
        }
        Err(()) => LatticeValue::Top,
    }
}

/// Returns whether `kind` divides or reduces by a known zero, which evaluates to zero whatever
/// its other operands are.
fn zero_divisor(kind: &InstKind, get: impl Fn(ValueId) -> Option<U256>) -> bool {
    let (InstKind::Div(_, divisor)
    | InstKind::SDiv(_, divisor)
    | InstKind::Mod(_, divisor)
    | InstKind::SMod(_, divisor)
    | InstKind::AddMod(_, _, divisor)
    | InstKind::MulMod(_, _, divisor)) = *kind
    else {
        return false;
    };
    get(divisor).is_some_and(|divisor| divisor.is_zero())
}

/// Rewrite phase: replace constant values with immediates and fold branches. Returns the number
/// of mutations.
fn rewrite(
    func: &mut Function,
    lattice: &IndexVec<ValueId, LatticeValue>,
    executable_blocks: &DenseBitSet<BlockId>,
    executable_edges: &FxHashSet<(BlockId, BlockId)>,
) -> usize {
    // Phase 1: Replace instructions whose results are constant with
    // immediate values, and remove the instruction from the block.
    let mut const_values = FxHashMap::default();
    let mut dead_insts = DenseBitSet::new_empty(func.num_insts());

    let value_insts = func
        .instructions()
        .filter_map(|inst_id| func.inst_result_value(inst_id).map(|value| (inst_id, value)))
        .collect::<Vec<_>>();
    for (inst_id, vid) in value_insts {
        if let LatticeValue::Constant(c) = lattice[vid] {
            // Don't fold side-effecting instructions.
            if func.inst(inst_id).kind.has_side_effects() {
                continue;
            }
            // Create an immediate replacement of the instruction's result type.
            let imm = Immediate::for_type(func.inst(inst_id).result_ty, c);
            let imm_vid = func.alloc_value(Value::Immediate(imm));
            const_values.insert(vid, imm_vid);
            dead_insts.insert(inst_id);
        }
    }

    // Phase 2: Remove dead (folded) instructions from blocks.
    // %v = op ... -> removed
    for block in func.blocks.iter_mut() {
        block.instructions.retain(|&id| !dead_insts.contains(id));
    }

    // Phase 3: Replace all uses of folded values with immediates.
    // use %v -> use constant
    func.replace_uses(&const_values);

    // Phase 4: Fold branches and switches with one distinct executable successor.
    let mut changes = const_values.len();
    for block_id in func.blocks.indices() {
        if !executable_blocks.contains(block_id) {
            continue;
        }
        let Some(term @ (Terminator::Branch { .. } | Terminator::Switch { .. })) =
            &func.blocks[block_id].terminator
        else {
            continue;
        };
        let mut targets =
            term.successors().into_iter().filter(|&to| executable_edges.contains(&(block_id, to)));
        if let Some(target) = targets.next()
            && targets.all(|other| other == target)
        {
            // branch/switch ..., target, ... -> jump target
            fold_terminator_to_jump(func, block_id, target);
            changes += 1;
        }
    }

    // Phase 5: Mark non-executable blocks as invalid.
    changes + invalidate_unreachable_blocks(func, executable_blocks)
}

/// Returns whether SCCP can discover a constant or structurally unreachable
/// block. Every derived constant starts at one of these local seeds, so a
/// function without a seed can skip user lists, lattices, and worklists.
fn can_change(func: &Function) -> bool {
    for inst_id in func.instructions() {
        let inst = func.inst(inst_id);
        if inst.kind.has_side_effects() {
            continue;
        }
        let directly_constant = match &inst.kind {
            InstKind::Phi(incoming) => incoming
                .first()
                .and_then(|&(_, value)| func.value_u256(value))
                .is_some_and(|first| {
                    incoming.iter().all(|&(_, value)| func.value_u256(value) == Some(first))
                }),
            InstKind::Select(condition, then_value, else_value) => {
                func.value_u256(*condition)
                    .and_then(|condition| {
                        func.value_u256(if condition.is_zero() { *else_value } else { *then_value })
                    })
                    .is_some()
                    || func
                        .value_u256(*then_value)
                        .is_some_and(|then_value| func.value_u256(*else_value) == Some(then_value))
            }
            kind => {
                zero_divisor(kind, |value| func.value_u256(value))
                    || eval::eval_typed_inst(func, kind, |value| func.value_u256(value).ok_or(()))
                        .ok()
                        .flatten()
                        .is_some()
            }
        };
        if directly_constant {
            return true;
        }
    }

    for block in &func.blocks {
        match block.terminator.as_ref() {
            Some(Terminator::Branch { condition, .. }) if func.value_u256(*condition).is_some() => {
                return true;
            }
            Some(Terminator::Switch { value, .. }) if func.value_u256(*value).is_some() => {
                return true;
            }
            _ => {}
        }
    }

    let mut reachable = DenseBitSet::new_empty(func.blocks.len());
    let mut pending = vec![BlockId::ENTRY];
    while let Some(block) = pending.pop() {
        if reachable.insert(block)
            && let Some(terminator) = &func.blocks[block].terminator
        {
            pending.extend(terminator.successors());
        }
    }
    func.blocks.indices().any(|block| {
        !reachable.contains(block)
            && !matches!(func.blocks[block].terminator, Some(Terminator::Invalid))
    })
}
