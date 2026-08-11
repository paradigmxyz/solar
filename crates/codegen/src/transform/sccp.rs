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
//! After reaching a fixed point, the rewrite phase replaces constant values
//! with immediates and rewrites branches with known-constant conditions.

use crate::{
    mir::{
        BlockId, Function, Immediate, InstId, InstKind, Module, Terminator, Value, ValueId,
        utils::{self as mir_utils, repair_reachability_phis},
    },
    pass::{MirPass, run_function_pass},
    utils::eval,
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
        analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| SccpCx::new().run(func) != 0)
    }
}

/// Lattice element for a single SSA value.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    fn meet(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Top, x) | (x, Self::Top) => x.clone(),
            (Self::Bottom, _) | (_, Self::Bottom) => Self::Bottom,
            (Self::Constant(a), Self::Constant(b)) => {
                if a == b {
                    Self::Constant(*a)
                } else {
                    Self::Bottom
                }
            }
        }
    }
}

/// SCCP statistics.
#[derive(Debug, Default, Clone)]
struct SccpStats {
    /// Number of instructions replaced with constants.
    constants_folded: usize,
    /// Number of branches replaced with unconditional jumps.
    branches_folded: usize,
    /// Number of switches replaced with unconditional jumps.
    switches_folded: usize,
    /// Number of unreachable blocks emptied and marked invalid.
    blocks_invalidated: usize,
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
                for operand in func.inst(inst_id).kind.operands() {
                    instructions[operand].push(inst_id);
                }
            }
            if let Some(terminator) = &block.terminator {
                for operand in terminator.operands() {
                    terminators[operand].push(block_id);
                }
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

/// Sparse Conditional Constant Propagation pass.
#[derive(Debug, Default)]
struct SccpCx {
    /// Statistics from the last run.
    stats: SccpStats,
}

impl SccpCx {
    /// Creates a new SCCP pass.
    fn new() -> Self {
        Self::default()
    }

    /// Runs SCCP on a function. Returns the total number of mutations,
    /// including unreachable-block cleanup and phi repairs.
    fn run(&mut self, func: &mut Function) -> usize {
        self.stats = SccpStats::default();

        let num_values = func.num_values();

        let users = ValueUsers::new(func);

        // Initialize lattice: all values start as Top.
        let mut lattice = index_vec![LatticeValue::Top; num_values];

        // Initialize non-instruction operands referenced by active MIR.
        let mut initialize = |value| {
            match func.value(value) {
                Value::Arg(_) => lattice[value] = LatticeValue::Bottom,
                Value::Immediate(imm) => {
                    if let Some(v) = imm.as_u256() {
                        lattice[value] = LatticeValue::Constant(v);
                    } else {
                        lattice[value] = LatticeValue::Bottom;
                    }
                }
                Value::Undef(_) | Value::Error(_) => lattice[value] = LatticeValue::Bottom,
                Value::Inst(_) => {} // stays Top
            }
        };
        for value in func.live_values() {
            initialize(value);
        }

        // Track which blocks are executable.
        let mut executable_blocks = DenseBitSet::new_empty(func.blocks.len());
        // Track which CFG edges have been taken.
        let mut executable_edges: FxHashSet<(BlockId, BlockId)> = FxHashSet::default();

        // Two worklists.
        let mut cfg_worklist: VecDeque<(BlockId, BlockId)> = VecDeque::new(); // (from, to) edges
        let mut ssa_worklist: VecDeque<ValueId> = VecDeque::new();

        // Seed: entry block is executable.
        executable_blocks.insert(BlockId::ENTRY);
        self.evaluate_phis_in_block(
            func,
            BlockId::ENTRY,
            &mut lattice,
            &executable_edges,
            &mut ssa_worklist,
        );
        // Evaluate all instructions in the entry block.
        self.evaluate_block(
            func,
            BlockId::ENTRY,
            &mut lattice,
            &executable_blocks,
            &executable_edges,
            &mut cfg_worklist,
            &mut ssa_worklist,
        );

        // Main loop: process both worklists until empty.
        loop {
            let mut made_progress = false;

            // Process CFG edges.
            while let Some((from, to)) = cfg_worklist.pop_front() {
                if !executable_edges.insert((from, to)) {
                    continue; // Already processed this edge.
                }
                made_progress = true;

                let newly_executable = executable_blocks.insert(to);

                // Re-evaluate phi-like values in the target block.
                self.evaluate_phis_in_block(
                    func,
                    to,
                    &mut lattice,
                    &executable_edges,
                    &mut ssa_worklist,
                );

                if newly_executable {
                    // First time this block is executable — evaluate all its instructions.
                    self.evaluate_block(
                        func,
                        to,
                        &mut lattice,
                        &executable_blocks,
                        &executable_edges,
                        &mut cfg_worklist,
                        &mut ssa_worklist,
                    );
                }
            }

            // Process SSA value changes.
            while let Some(vid) = ssa_worklist.pop_front() {
                made_progress = true;
                // Find all users of this value and re-evaluate them.
                self.propagate_value(
                    func,
                    vid,
                    &users,
                    &mut lattice,
                    &executable_blocks,
                    &executable_edges,
                    &mut cfg_worklist,
                    &mut ssa_worklist,
                );
            }

            if !made_progress {
                break;
            }
        }

        // Rewrite phase: apply the lattice results to the function.
        self.rewrite(func, &lattice, &executable_blocks, &executable_edges)
    }

    /// Evaluates all instructions in a block.
    #[allow(clippy::too_many_arguments)]
    fn evaluate_block(
        &self,
        func: &Function,
        block_id: BlockId,
        lattice: &mut IndexVec<ValueId, LatticeValue>,
        _executable_blocks: &DenseBitSet<BlockId>,
        _executable_edges: &FxHashSet<(BlockId, BlockId)>,
        cfg_worklist: &mut VecDeque<(BlockId, BlockId)>,
        ssa_worklist: &mut VecDeque<ValueId>,
    ) {
        let block = &func.blocks[block_id];

        for &inst_id in &block.instructions {
            if matches!(func.inst(inst_id).kind, InstKind::Phi(_)) {
                continue;
            }
            if let Some(vid) = func.inst_result_value(inst_id) {
                let new_val = self.evaluate_instruction(func, &func.inst(inst_id).kind, lattice);
                if self.update_lattice(lattice, vid, new_val) {
                    ssa_worklist.push_back(vid);
                }
            }
        }

        // Evaluate the terminator to determine outgoing edges.
        if let Some(term) = &block.terminator {
            self.evaluate_terminator(term, block_id, lattice, cfg_worklist);
        }
    }

    /// Evaluates phi instructions (`InstKind::Phi`) at the entry of a block.
    fn evaluate_phis_in_block(
        &self,
        func: &Function,
        block_id: BlockId,
        lattice: &mut IndexVec<ValueId, LatticeValue>,
        executable_edges: &FxHashSet<(BlockId, BlockId)>,
        ssa_worklist: &mut VecDeque<ValueId>,
    ) {
        let block = &func.blocks[block_id];
        for &inst_id in &block.instructions {
            if matches!(func.inst(inst_id).kind, InstKind::Phi(_)) {
                self.evaluate_phi(func, block_id, inst_id, lattice, executable_edges, ssa_worklist);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_phi(
        &self,
        func: &Function,
        block_id: BlockId,
        inst_id: InstId,
        lattice: &mut IndexVec<ValueId, LatticeValue>,
        executable_edges: &FxHashSet<(BlockId, BlockId)>,
        ssa_worklist: &mut VecDeque<ValueId>,
    ) {
        let InstKind::Phi(incoming) = &func.inst(inst_id).kind else { return };
        let Some(vid) = func.inst_result_value(inst_id) else { return };
        let mut result = LatticeValue::Top;
        for &(pred, operand) in incoming {
            if executable_edges.contains(&(pred, block_id)) {
                result = result.meet(&lattice[operand]);
            }
        }
        if self.update_lattice(lattice, vid, result) {
            ssa_worklist.push_back(vid);
        }
    }

    /// Evaluates a single instruction and returns its lattice value.
    fn evaluate_instruction(
        &self,
        _func: &Function,
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
                    lattice[chosen].clone()
                }
                LatticeValue::Bottom => lattice[then_value].meet(&lattice[else_value]),
                LatticeValue::Top => match (get_const(then_value), get_const(else_value)) {
                    (Some(then_value), Some(else_value)) if then_value == else_value => {
                        LatticeValue::Constant(then_value)
                    }
                    _ => LatticeValue::Top,
                },
            };
        }

        match eval::eval_inst(kind, |value| get_const(value).ok_or(())) {
            Ok(Some(value)) => LatticeValue::Constant(value),
            Ok(None) => LatticeValue::Bottom,
            Err(()) => match *kind {
                InstKind::Div(_, divisor)
                | InstKind::SDiv(_, divisor)
                | InstKind::Mod(_, divisor)
                | InstKind::SMod(_, divisor)
                    if get_const(divisor).is_some_and(|divisor| divisor.is_zero()) =>
                {
                    LatticeValue::Constant(U256::ZERO)
                }
                InstKind::AddMod(_, _, modulus) | InstKind::MulMod(_, _, modulus)
                    if get_const(modulus).is_some_and(|modulus| modulus.is_zero()) =>
                {
                    LatticeValue::Constant(U256::ZERO)
                }
                _ => self.check_any_bottom(&kind.operands(), lattice),
            },
        }
    }

    /// Helper: if any operand is Bottom, return Bottom; otherwise Top (still waiting).
    fn check_any_bottom(
        &self,
        operands: &[ValueId],
        lattice: &IndexVec<ValueId, LatticeValue>,
    ) -> LatticeValue {
        for &op in operands {
            if matches!(lattice[op], LatticeValue::Bottom) {
                return LatticeValue::Bottom;
            }
        }
        LatticeValue::Top
    }

    /// Evaluates a terminator to determine which outgoing edges are taken.
    fn evaluate_terminator(
        &self,
        term: &Terminator,
        block_id: BlockId,
        lattice: &IndexVec<ValueId, LatticeValue>,
        cfg_worklist: &mut VecDeque<(BlockId, BlockId)>,
    ) {
        match term {
            Terminator::Jump(target) => {
                cfg_worklist.push_back((block_id, *target));
            }
            Terminator::Branch { condition, then_block, else_block } => {
                match &lattice[*condition] {
                    LatticeValue::Constant(v) => {
                        if !v.is_zero() {
                            cfg_worklist.push_back((block_id, *then_block));
                        } else {
                            cfg_worklist.push_back((block_id, *else_block));
                        }
                    }
                    LatticeValue::Bottom => {
                        // Both edges might be taken.
                        cfg_worklist.push_back((block_id, *then_block));
                        cfg_worklist.push_back((block_id, *else_block));
                    }
                    LatticeValue::Top => {}
                }
            }
            Terminator::Switch { value, default, cases } => {
                match &lattice[*value] {
                    LatticeValue::Constant(v) => {
                        // Cases are tested in order at runtime, so a constant
                        // case match is definitive only if every earlier case
                        // is a known constant that differs from the scrutinee.
                        // Overdefined earlier cases stay feasible; an
                        // unresolved case defers the remaining edges.
                        for &(case_val, target) in cases {
                            match &lattice[case_val] {
                                LatticeValue::Constant(cv) if cv == v => {
                                    cfg_worklist.push_back((block_id, target));
                                    return;
                                }
                                LatticeValue::Constant(_) => {}
                                LatticeValue::Bottom => {
                                    cfg_worklist.push_back((block_id, target));
                                }
                                LatticeValue::Top => return,
                            }
                        }
                        cfg_worklist.push_back((block_id, *default));
                    }
                    LatticeValue::Bottom => {
                        // All edges might be taken.
                        cfg_worklist.push_back((block_id, *default));
                        for &(_, target) in cases {
                            cfg_worklist.push_back((block_id, target));
                        }
                    }
                    LatticeValue::Top => {}
                }
            }
            Terminator::TailCall { .. }
            | Terminator::Return { .. }
            | Terminator::Revert { .. }
            | Terminator::ReturnData { .. }
            | Terminator::Stop
            | Terminator::SelfDestruct { .. }
            | Terminator::Invalid => {
                // No outgoing edges.
            }
        }
    }

    /// Updates the lattice value for a ValueId. Returns true if it changed.
    /// Lattice values can only move downward: Top → Constant → Bottom.
    fn update_lattice(
        &self,
        lattice: &mut IndexVec<ValueId, LatticeValue>,
        vid: ValueId,
        new_val: LatticeValue,
    ) -> bool {
        let old = &lattice[vid];
        let merged = old.meet(&new_val);
        if merged != *old {
            lattice[vid] = merged;
            true
        } else {
            false
        }
    }

    /// Propagates a value change to all users of that value.
    #[allow(clippy::too_many_arguments)]
    fn propagate_value(
        &self,
        func: &Function,
        vid: ValueId,
        users: &ValueUsers,
        lattice: &mut IndexVec<ValueId, LatticeValue>,
        executable_blocks: &DenseBitSet<BlockId>,
        executable_edges: &FxHashSet<(BlockId, BlockId)>,
        cfg_worklist: &mut VecDeque<(BlockId, BlockId)>,
        ssa_worklist: &mut VecDeque<ValueId>,
    ) {
        for &inst_id in &users.instructions[vid] {
            let block_id = users.inst_blocks[inst_id];
            if !executable_blocks.contains(block_id) {
                continue;
            }
            let inst = func.inst(inst_id);
            if matches!(inst.kind, InstKind::Phi(_)) {
                self.evaluate_phi(func, block_id, inst_id, lattice, executable_edges, ssa_worklist);
            } else if let Some(result_vid) = func.inst_result_value(inst_id) {
                let new_val = self.evaluate_instruction(func, &inst.kind, lattice);
                if self.update_lattice(lattice, result_vid, new_val) {
                    ssa_worklist.push_back(result_vid);
                }
            }
        }

        for &block_id in &users.terminators[vid] {
            if executable_blocks.contains(block_id)
                && let Some(terminator) = &func.blocks[block_id].terminator
            {
                self.evaluate_terminator(terminator, block_id, lattice, cfg_worklist);
            }
        }
    }

    /// Rewrite phase: replace constant values with immediates and fold branches.
    fn rewrite(
        &mut self,
        func: &mut Function,
        lattice: &IndexVec<ValueId, LatticeValue>,
        executable_blocks: &DenseBitSet<BlockId>,
        executable_edges: &FxHashSet<(BlockId, BlockId)>,
    ) -> usize {
        // Phase 1: Replace instructions whose results are constant with
        // immediate values, and remove the instruction from the block.
        let mut const_values: FxHashMap<ValueId, ValueId> = FxHashMap::default();
        let mut dead_insts = DenseBitSet::new_empty(func.num_insts());

        let value_insts = func
            .instructions()
            .filter_map(|inst_id| func.inst_result_value(inst_id).map(|value| (inst_id, value)))
            .collect::<Vec<_>>();
        for (inst_id, vid) in value_insts {
            if let LatticeValue::Constant(c) = &lattice[vid] {
                // Don't fold side-effecting instructions.
                if func.inst(inst_id).kind.has_side_effects() {
                    continue;
                }
                // Create an immediate replacement of the instruction's result type.
                let imm = Immediate::for_type(func.inst(inst_id).result_ty, *c);
                let imm_vid = func.alloc_value(Value::Immediate(imm));
                const_values.insert(vid, imm_vid);
                dead_insts.insert(inst_id);
                self.stats.constants_folded += 1;
            }
        }

        // Phase 2: Collect branch rewrites BEFORE operand replacement, because
        // replacement may allocate new ValueIds that don't have lattice entries.
        let block_ids: Vec<BlockId> = func.blocks.indices().collect();
        let mut control_rewrites: Vec<(BlockId, BlockId)> = Vec::new();
        let mut executable_successors = DenseBitSet::new_empty(func.blocks.len());
        for &block_id in &block_ids {
            if !executable_blocks.contains(block_id) {
                continue;
            }
            let Some(term) = &func.blocks[block_id].terminator else {
                continue;
            };
            if !matches!(term, Terminator::Branch { .. } | Terminator::Switch { .. }) {
                continue;
            }

            executable_successors.clear();
            for successor in term.successors() {
                if executable_edges.contains(&(block_id, successor)) {
                    executable_successors.insert(successor);
                }
            }
            if executable_successors.count() == 1 {
                let target = executable_successors.iter().next().expect("checked count");
                control_rewrites.push((block_id, target));
            }
        }

        // Phase 3: Replace all uses of folded values with immediates.
        if !const_values.is_empty() {
            let all_insts: Vec<InstId> =
                func.instructions().filter(|&id| !dead_insts.contains(id)).collect();
            for inst_id in all_insts {
                mir_utils::replace_inst_uses(&mut func.inst_mut(inst_id).kind, &const_values);
            }
            for &block_id in &block_ids {
                if let Some(term) = &mut func.blocks[block_id].terminator {
                    mir_utils::replace_terminator_uses(term, &const_values);
                }
            }
        }

        // Phase 4: Remove dead (folded) instructions from blocks.
        for &block_id in &block_ids {
            func.blocks[block_id].instructions.retain(|&id| !dead_insts.contains(id));
        }

        // Phase 5: Apply branch/switch rewrites.
        for (block_id, target) in control_rewrites {
            let old_successors = func.blocks[block_id]
                .terminator
                .as_ref()
                .map(Terminator::successors)
                .unwrap_or_default();
            let was_switch =
                matches!(func.blocks[block_id].terminator, Some(Terminator::Switch { .. }));
            for successor in old_successors {
                func.blocks[successor].predecessors.retain(|pred| *pred != block_id);
            }
            if !func.blocks[target].predecessors.contains(&block_id) {
                func.blocks[target].predecessors.push(block_id);
            }
            func.blocks[block_id].terminator = Some(Terminator::Jump(target));
            if was_switch {
                self.stats.switches_folded += 1;
            } else {
                self.stats.branches_folded += 1;
            }
        }

        // Phase 6: Mark non-executable blocks as invalid.
        for &block_id in &block_ids {
            if executable_blocks.contains(block_id) {
                continue;
            }
            let block = &mut func.blocks[block_id];
            // Predecessor lists are rebuilt from terminators by
            // `repair_reachability_phis` below, so a never-taken switch target
            // keeps a predecessor entry; checking it here would re-count the
            // block as invalidated on every run.
            let already_invalid = block.instructions.is_empty()
                && matches!(block.terminator, Some(Terminator::Invalid));
            if already_invalid {
                continue;
            }
            block.instructions.clear();
            block.terminator = Some(Terminator::Invalid);
            block.predecessors.clear();
            self.stats.blocks_invalidated += 1;
        }

        let reachability_repaired = repair_reachability_phis(func);

        self.stats.constants_folded
            + self.stats.branches_folded
            + self.stats.switches_folded
            + self.stats.blocks_invalidated
            + usize::from(reachability_repaired)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{MirType, TypeSize};

    #[test]
    fn immediate_for_type_preserves_result_types() {
        let one = U256::from(1);
        let i256 = TypeSize::new_int_bits(256);
        let i64 = TypeSize::new_int_bits(64);
        let i8 = TypeSize::new_int_bits(8);
        assert_eq!(Immediate::for_type(Some(MirType::Bool), one), Immediate::Bool(true));
        assert_eq!(Immediate::for_type(Some(MirType::Bool), U256::ZERO), Immediate::Bool(false));
        assert_eq!(Immediate::for_type(Some(MirType::Int(i256)), one), Immediate::Int(one, i256));
        assert_eq!(Immediate::for_type(Some(MirType::UInt(i64)), one), Immediate::UInt(one, i64));
        // Non-integer payloads and missing types fall back to uint256.
        assert_eq!(Immediate::for_type(Some(MirType::Address), one), Immediate::uint256(one));
        assert_eq!(Immediate::for_type(None, one), Immediate::uint256(one));
        // A bool-typed result that is not 0/1 keeps its numeric value.
        let two = U256::from(2);
        assert_eq!(Immediate::for_type(Some(MirType::Bool), two), Immediate::uint256(two));
        // Out-of-range values fall back to uint256 instead of lying about the width.
        let wide = U256::from(0x1ff);
        assert_eq!(Immediate::for_type(Some(MirType::UInt(i8)), wide), Immediate::uint256(wide));
        assert_eq!(Immediate::for_type(Some(MirType::Int(i8)), wide), Immediate::uint256(wide));
        // Negative values are representable when the upper bits match the sign bit.
        let minus_one = U256::MAX;
        assert_eq!(
            Immediate::for_type(Some(MirType::Int(i8)), minus_one),
            Immediate::Int(minus_one, i8)
        );
        let i8_min = U256::MAX - U256::from(0x7f);
        assert_eq!(Immediate::for_type(Some(MirType::Int(i8)), i8_min), Immediate::Int(i8_min, i8));
        let i8_under = i8_min - U256::from(1);
        assert_eq!(
            Immediate::for_type(Some(MirType::Int(i8)), i8_under),
            Immediate::uint256(i8_under)
        );
    }
}
