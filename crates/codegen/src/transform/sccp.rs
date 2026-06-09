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
    mir::{BlockId, Function, Immediate, InstId, InstKind, Terminator, Value, ValueId},
    pass::FunctionPass,
    transform::repair_reachability_phis,
};
use alloy_primitives::U256;
use solar_data_structures::map::{FxHashMap, FxHashSet};
use std::collections::VecDeque;

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
pub struct SccpStats {
    /// Number of instructions replaced with constants.
    pub constants_folded: usize,
    /// Number of branches replaced with unconditional jumps.
    pub branches_folded: usize,
}

/// Sparse Conditional Constant Propagation pass.
#[derive(Debug, Default)]
pub struct SccpPass {
    /// Statistics from the last run.
    pub stats: SccpStats,
}

/// Function pass adapter for sparse conditional constant propagation.
pub struct SccpTransformPass;

impl FunctionPass for SccpTransformPass {
    fn name(&self) -> &str {
        "sccp"
    }

    fn run_on_function(&mut self, func: &mut Function) -> bool {
        let changed = SccpPass::new().run(func) != 0;
        repair_reachability_phis(func);
        changed
    }
}

impl SccpPass {
    /// Creates a new SCCP pass.
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs SCCP on a function. Returns total number of rewrites.
    pub fn run(&mut self, func: &mut Function) -> usize {
        self.stats = SccpStats::default();

        let num_values = func.values.len();
        let _num_blocks = func.blocks.len();

        // Precompute InstId → ValueId map.
        let inst_to_value: FxHashMap<InstId, ValueId> = func
            .values
            .iter_enumerated()
            .filter_map(
                |(vid, val)| {
                    if let Value::Inst(iid) = val { Some((*iid, vid)) } else { None }
                },
            )
            .collect();

        // Initialize lattice: all values start as Top.
        let mut lattice: Vec<LatticeValue> = vec![LatticeValue::Top; num_values];

        // Arguments are overdefined (we don't know their runtime values).
        for (vid, val) in func.values.iter_enumerated() {
            match val {
                Value::Arg { .. } => lattice[vid.index()] = LatticeValue::Bottom,
                Value::Immediate(imm) => {
                    if let Some(v) = imm.as_u256() {
                        lattice[vid.index()] = LatticeValue::Constant(v);
                    } else {
                        lattice[vid.index()] = LatticeValue::Bottom;
                    }
                }
                Value::Undef(_) => lattice[vid.index()] = LatticeValue::Bottom,
                Value::Phi { .. } | Value::Inst(_) => {} // stays Top
            }
        }

        // Track which blocks are executable.
        let mut executable_blocks: FxHashSet<BlockId> = FxHashSet::default();
        // Track which CFG edges have been taken.
        let mut executable_edges: FxHashSet<(BlockId, BlockId)> = FxHashSet::default();

        // Two worklists.
        let mut cfg_worklist: VecDeque<(BlockId, BlockId)> = VecDeque::new(); // (from, to) edges
        let mut ssa_worklist: VecDeque<ValueId> = VecDeque::new();

        // Seed: entry block is executable.
        executable_blocks.insert(func.entry_block);
        // Evaluate all instructions in the entry block.
        self.evaluate_block(
            func,
            func.entry_block,
            &inst_to_value,
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
                        &inst_to_value,
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
                    &inst_to_value,
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
        self.rewrite(func, &lattice, &inst_to_value, &executable_blocks)
    }

    /// Evaluates all instructions in a block.
    #[allow(clippy::too_many_arguments)]
    fn evaluate_block(
        &self,
        func: &Function,
        block_id: BlockId,
        inst_to_value: &FxHashMap<InstId, ValueId>,
        lattice: &mut [LatticeValue],
        _executable_blocks: &FxHashSet<BlockId>,
        _executable_edges: &FxHashSet<(BlockId, BlockId)>,
        cfg_worklist: &mut VecDeque<(BlockId, BlockId)>,
        ssa_worklist: &mut VecDeque<ValueId>,
    ) {
        let block = &func.blocks[block_id];

        for &inst_id in &block.instructions {
            if let Some(&vid) = inst_to_value.get(&inst_id) {
                let new_val =
                    self.evaluate_instruction(func, &func.instructions[inst_id].kind, lattice);
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

    /// Evaluates phi-like values (Value::Phi) at the entry of a block.
    fn evaluate_phis_in_block(
        &self,
        func: &Function,
        block_id: BlockId,
        lattice: &mut [LatticeValue],
        executable_edges: &FxHashSet<(BlockId, BlockId)>,
        ssa_worklist: &mut VecDeque<ValueId>,
    ) {
        // Check Value::Phi entries that target this block.
        for (vid, val) in func.values.iter_enumerated() {
            if let Value::Phi { incoming, .. } = val {
                // Is this phi for this block? Check if any incoming pred targets this block.
                let is_for_block = incoming
                    .iter()
                    .any(|(pred, _)| func.blocks[*pred].successors.contains(&block_id));
                if !is_for_block {
                    continue;
                }

                // Meet over all executable incoming edges.
                let mut result = LatticeValue::Top;
                for &(pred, operand) in incoming {
                    if executable_edges.contains(&(pred, block_id)) {
                        result = result.meet(&lattice[operand.index()]);
                    }
                }
                if self.update_lattice(lattice, vid, result) {
                    ssa_worklist.push_back(vid);
                }
            }
        }

        // Also check InstKind::Phi instructions in the block.
        // Build a local inst→value map for this block's phis only.
        let block = &func.blocks[block_id];
        for &inst_id in &block.instructions {
            let inst = &func.instructions[inst_id];
            if let InstKind::Phi(incoming) = &inst.kind {
                // Find the ValueId for this instruction.
                let vid = func
                    .values
                    .iter_enumerated()
                    .find(|(_, v)| matches!(v, Value::Inst(id) if *id == inst_id))
                    .map(|(vid, _)| vid);
                if let Some(vid) = vid {
                    let mut result = LatticeValue::Top;
                    for &(pred, operand) in incoming {
                        if executable_edges.contains(&(pred, block_id)) {
                            result = result.meet(&lattice[operand.index()]);
                        }
                    }
                    if self.update_lattice(lattice, vid, result) {
                        ssa_worklist.push_back(vid);
                    }
                }
            }
        }
    }

    /// Evaluates a single instruction and returns its lattice value.
    fn evaluate_instruction(
        &self,
        _func: &Function,
        kind: &InstKind,
        lattice: &[LatticeValue],
    ) -> LatticeValue {
        // Helper: get the constant value of a ValueId, or None if not constant.
        let get_const = |v: ValueId| -> Option<U256> {
            match &lattice[v.index()] {
                LatticeValue::Constant(c) => Some(*c),
                _ => None,
            }
        };

        // If any operand is Bottom, the result is Bottom (overdefined).
        // If any operand is Top, the result is Top (not yet known).
        let _check_operands = |operands: &[ValueId]| -> Option<()> {
            for &op in operands {
                match &lattice[op.index()] {
                    LatticeValue::Bottom => return None, // Will be Bottom
                    LatticeValue::Top => return None,    // Wait for more info
                    LatticeValue::Constant(_) => {}
                }
            }
            Some(())
        };

        match kind {
            // Arithmetic — fold if both operands are constant.
            InstKind::Add(a, b) => match (get_const(*a), get_const(*b)) {
                (Some(a), Some(b)) => LatticeValue::Constant(a.wrapping_add(b)),
                _ => self.check_any_bottom(&[*a, *b], lattice),
            },
            InstKind::Sub(a, b) => match (get_const(*a), get_const(*b)) {
                (Some(a), Some(b)) => LatticeValue::Constant(a.wrapping_sub(b)),
                _ => self.check_any_bottom(&[*a, *b], lattice),
            },
            InstKind::Mul(a, b) => match (get_const(*a), get_const(*b)) {
                (Some(a), Some(b)) => LatticeValue::Constant(a.wrapping_mul(b)),
                _ => self.check_any_bottom(&[*a, *b], lattice),
            },
            InstKind::Div(a, b) => match (get_const(*a), get_const(*b)) {
                (Some(_), Some(b)) if b.is_zero() => LatticeValue::Constant(U256::ZERO),
                (Some(a), Some(b)) => LatticeValue::Constant(a / b),
                _ => self.check_any_bottom(&[*a, *b], lattice),
            },
            InstKind::Mod(a, b) => match (get_const(*a), get_const(*b)) {
                (Some(_), Some(b)) if b.is_zero() => LatticeValue::Constant(U256::ZERO),
                (Some(a), Some(b)) => LatticeValue::Constant(a % b),
                _ => self.check_any_bottom(&[*a, *b], lattice),
            },
            InstKind::Exp(a, b) => match (get_const(*a), get_const(*b)) {
                (Some(base), Some(exp)) => {
                    // Use wrapping exponentiation.
                    LatticeValue::Constant(base.wrapping_pow(exp))
                }
                _ => self.check_any_bottom(&[*a, *b], lattice),
            },

            // Comparison — fold to bool (0 or 1).
            InstKind::Lt(a, b) => match (get_const(*a), get_const(*b)) {
                (Some(a), Some(b)) => LatticeValue::Constant(U256::from(a < b)),
                _ => self.check_any_bottom(&[*a, *b], lattice),
            },
            InstKind::Gt(a, b) => match (get_const(*a), get_const(*b)) {
                (Some(a), Some(b)) => LatticeValue::Constant(U256::from(a > b)),
                _ => self.check_any_bottom(&[*a, *b], lattice),
            },
            InstKind::Eq(a, b) => match (get_const(*a), get_const(*b)) {
                (Some(a), Some(b)) => LatticeValue::Constant(U256::from(a == b)),
                _ => self.check_any_bottom(&[*a, *b], lattice),
            },
            InstKind::IsZero(a) => match get_const(*a) {
                Some(a) => LatticeValue::Constant(U256::from(a.is_zero())),
                None => self.check_any_bottom(&[*a], lattice),
            },

            // Bitwise.
            InstKind::And(a, b) => match (get_const(*a), get_const(*b)) {
                (Some(a), Some(b)) => LatticeValue::Constant(a & b),
                _ => self.check_any_bottom(&[*a, *b], lattice),
            },
            InstKind::Or(a, b) => match (get_const(*a), get_const(*b)) {
                (Some(a), Some(b)) => LatticeValue::Constant(a | b),
                _ => self.check_any_bottom(&[*a, *b], lattice),
            },
            InstKind::Xor(a, b) => match (get_const(*a), get_const(*b)) {
                (Some(a), Some(b)) => LatticeValue::Constant(a ^ b),
                _ => self.check_any_bottom(&[*a, *b], lattice),
            },
            InstKind::Not(a) => match get_const(*a) {
                Some(a) => LatticeValue::Constant(!a),
                None => self.check_any_bottom(&[*a], lattice),
            },
            InstKind::Shl(shift, val) => match (get_const(*shift), get_const(*val)) {
                (Some(s), Some(v)) => {
                    if s >= U256::from(256) {
                        LatticeValue::Constant(U256::ZERO)
                    } else {
                        LatticeValue::Constant(v << s.to::<usize>())
                    }
                }
                _ => self.check_any_bottom(&[*shift, *val], lattice),
            },
            InstKind::Shr(shift, val) => match (get_const(*shift), get_const(*val)) {
                (Some(s), Some(v)) => {
                    if s >= U256::from(256) {
                        LatticeValue::Constant(U256::ZERO)
                    } else {
                        LatticeValue::Constant(v >> s.to::<usize>())
                    }
                }
                _ => self.check_any_bottom(&[*shift, *val], lattice),
            },

            // Everything else (memory, storage, calls, environment, etc.) is
            // conservatively overdefined — we can't evaluate them at compile time.
            _ => LatticeValue::Bottom,
        }
    }

    /// Helper: if any operand is Bottom, return Bottom; otherwise Top (still waiting).
    fn check_any_bottom(&self, operands: &[ValueId], lattice: &[LatticeValue]) -> LatticeValue {
        for &op in operands {
            if matches!(lattice[op.index()], LatticeValue::Bottom) {
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
        lattice: &[LatticeValue],
        cfg_worklist: &mut VecDeque<(BlockId, BlockId)>,
    ) {
        match term {
            Terminator::Jump(target) => {
                cfg_worklist.push_back((block_id, *target));
            }
            Terminator::Branch { condition, then_block, else_block } => {
                match &lattice[condition.index()] {
                    LatticeValue::Constant(v) => {
                        if !v.is_zero() {
                            cfg_worklist.push_back((block_id, *then_block));
                        } else {
                            cfg_worklist.push_back((block_id, *else_block));
                        }
                    }
                    _ => {
                        // Both edges might be taken.
                        cfg_worklist.push_back((block_id, *then_block));
                        cfg_worklist.push_back((block_id, *else_block));
                    }
                }
            }
            Terminator::Switch { value, default, cases } => {
                match &lattice[value.index()] {
                    LatticeValue::Constant(v) => {
                        // Find the matching case.
                        let mut found = false;
                        for &(case_val, target) in cases {
                            if let LatticeValue::Constant(cv) = &lattice[case_val.index()]
                                && cv == v
                            {
                                cfg_worklist.push_back((block_id, target));
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            cfg_worklist.push_back((block_id, *default));
                        }
                    }
                    _ => {
                        // All edges might be taken.
                        cfg_worklist.push_back((block_id, *default));
                        for &(_, target) in cases {
                            cfg_worklist.push_back((block_id, target));
                        }
                    }
                }
            }
            Terminator::Return { .. }
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
        lattice: &mut [LatticeValue],
        vid: ValueId,
        new_val: LatticeValue,
    ) -> bool {
        let old = &lattice[vid.index()];
        let merged = old.meet(&new_val);
        if merged != *old {
            lattice[vid.index()] = merged;
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
        inst_to_value: &FxHashMap<InstId, ValueId>,
        lattice: &mut [LatticeValue],
        executable_blocks: &FxHashSet<BlockId>,
        _executable_edges: &FxHashSet<(BlockId, BlockId)>,
        cfg_worklist: &mut VecDeque<(BlockId, BlockId)>,
        ssa_worklist: &mut VecDeque<ValueId>,
    ) {
        // Find all instructions that use this value and re-evaluate them.
        for (block_id, block) in func.blocks.iter_enumerated() {
            if !executable_blocks.contains(&block_id) {
                continue;
            }
            for &inst_id in &block.instructions {
                let inst = &func.instructions[inst_id];
                let operands = inst.kind.operands();
                if operands.contains(&vid)
                    && let Some(&result_vid) = inst_to_value.get(&inst_id)
                {
                    let new_val = self.evaluate_instruction(func, &inst.kind, lattice);
                    if self.update_lattice(lattice, result_vid, new_val) {
                        ssa_worklist.push_back(result_vid);
                    }
                }
            }
            // Re-evaluate terminators that use this value.
            if let Some(term) = &block.terminator {
                let term_ops = term.operands();
                if term_ops.contains(&vid) {
                    self.evaluate_terminator(term, block_id, lattice, cfg_worklist);
                }
            }
        }
    }

    /// Rewrite phase: replace constant values with immediates and fold branches.
    fn rewrite(
        &mut self,
        func: &mut Function,
        lattice: &[LatticeValue],
        inst_to_value: &FxHashMap<InstId, ValueId>,
        executable_blocks: &FxHashSet<BlockId>,
    ) -> usize {
        // Phase 1: Replace instructions whose results are constant with
        // immediate values, and remove the instruction from the block.
        let mut const_values: FxHashMap<ValueId, ValueId> = FxHashMap::default();
        let mut dead_insts: FxHashSet<InstId> = FxHashSet::default();

        for (&inst_id, &vid) in inst_to_value {
            if let LatticeValue::Constant(c) = &lattice[vid.index()] {
                // Don't fold side-effecting instructions.
                if func.instructions[inst_id].kind.has_side_effects() {
                    continue;
                }
                // Create an immediate replacement.
                let imm_vid = func.alloc_value(Value::Immediate(Immediate::uint256(*c)));
                const_values.insert(vid, imm_vid);
                dead_insts.insert(inst_id);
                self.stats.constants_folded += 1;
            }
        }

        // Phase 2: Collect branch rewrites BEFORE operand replacement, because
        // replacement may allocate new ValueIds that don't have lattice entries.
        let block_ids: Vec<BlockId> = func.blocks.indices().collect();
        let mut branch_rewrites: Vec<(BlockId, BlockId)> = Vec::new();
        for &block_id in &block_ids {
            if !executable_blocks.contains(&block_id) {
                continue;
            }
            if let Some(Terminator::Branch { condition, then_block, else_block }) =
                &func.blocks[block_id].terminator
                && let LatticeValue::Constant(v) = &lattice[condition.index()]
            {
                let target = if !v.is_zero() { *then_block } else { *else_block };
                branch_rewrites.push((block_id, target));
            }
        }

        // Phase 3: Replace all uses of folded values with immediates.
        if !const_values.is_empty() {
            let all_insts: Vec<InstId> = func
                .blocks
                .iter()
                .flat_map(|block| block.instructions.iter().copied())
                .filter(|id| !dead_insts.contains(id))
                .collect();
            for inst_id in all_insts {
                replace_inst_operands(&mut func.instructions[inst_id].kind, &const_values);
            }
            for &block_id in &block_ids {
                if let Some(term) = &mut func.blocks[block_id].terminator {
                    replace_terminator_operands(term, &const_values);
                }
            }
        }

        // Phase 4: Remove dead (folded) instructions from blocks.
        for &block_id in &block_ids {
            func.blocks[block_id].instructions.retain(|id| !dead_insts.contains(id));
        }

        // Phase 5: Apply branch rewrites.
        for (block_id, target) in branch_rewrites {
            func.blocks[block_id].terminator = Some(Terminator::Jump(target));
            func.blocks[block_id].successors.clear();
            func.blocks[block_id].successors.push(target);
            self.stats.branches_folded += 1;
        }

        // Phase 5: Mark non-executable blocks as invalid.
        for &block_id in &block_ids {
            if !executable_blocks.contains(&block_id) {
                func.blocks[block_id].instructions.clear();
                func.blocks[block_id].terminator = Some(Terminator::Invalid);
                func.blocks[block_id].predecessors.clear();
                func.blocks[block_id].successors.clear();
            }
        }

        self.stats.constants_folded + self.stats.branches_folded
    }
}

/// Replace operands in an instruction kind with constant replacements.
fn replace_inst_operands(kind: &mut InstKind, replacements: &FxHashMap<ValueId, ValueId>) {
    kind.visit_operands_mut(|v| {
        if let Some(&new_v) = replacements.get(v) {
            *v = new_v;
        }
    });
}

/// Replace operands in a terminator.
fn replace_terminator_operands(term: &mut Terminator, replacements: &FxHashMap<ValueId, ValueId>) {
    let replace = |v: &mut ValueId| {
        if let Some(&new_v) = replacements.get(v) {
            *v = new_v;
        }
    };
    match term {
        Terminator::Jump(_) | Terminator::Stop | Terminator::Invalid => {}
        Terminator::Branch { condition, .. } => replace(condition),
        Terminator::Switch { value, cases, .. } => {
            replace(value);
            for (v, _) in cases {
                replace(v);
            }
        }
        Terminator::Return { values } => {
            for v in values {
                replace(v);
            }
        }
        Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
            replace(offset);
            replace(size);
        }
        Terminator::SelfDestruct { recipient } => replace(recipient),
    }
}
