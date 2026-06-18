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
    mir::{BlockId, Function, Immediate, InstId, InstKind, MirType, Terminator, Value, ValueId},
    pass::FunctionPass,
    utils::{evm_word, repair_reachability_phis},
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
    /// Number of switches replaced with unconditional jumps.
    pub switches_folded: usize,
    /// Number of unreachable blocks emptied and marked invalid.
    pub blocks_invalidated: usize,
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
        SccpPass::new().run(func) != 0
    }
}

impl SccpPass {
    /// Creates a new SCCP pass.
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs SCCP on a function. Returns the total number of mutations,
    /// including unreachable-block cleanup and phi repairs.
    pub fn run(&mut self, func: &mut Function) -> usize {
        self.stats = SccpStats::default();

        let num_values = func.values.len();

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
                Value::Inst(_) => {} // stays Top
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
        self.evaluate_phis_in_block(
            func,
            func.entry_block,
            &inst_to_value,
            &mut lattice,
            &executable_edges,
            &mut ssa_worklist,
        );
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
                    &inst_to_value,
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
        self.rewrite(func, &lattice, &inst_to_value, &executable_blocks, &executable_edges)
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
            if matches!(func.instructions[inst_id].kind, crate::mir::InstTag::Phi) {
                continue;
            }
            if let Some(&vid) = inst_to_value.get(&inst_id) {
                let new_val =
                    self.evaluate_instruction(func, &func.instructions[inst_id].kind(), lattice);
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
        inst_to_value: &FxHashMap<InstId, ValueId>,
        lattice: &mut [LatticeValue],
        executable_edges: &FxHashSet<(BlockId, BlockId)>,
        ssa_worklist: &mut VecDeque<ValueId>,
    ) {
        let block = &func.blocks[block_id];
        for &inst_id in &block.instructions {
            let inst = &func.instructions[inst_id];
            if let InstKind::Phi(incoming) = &inst.kind()
                && let Some(&vid) = inst_to_value.get(&inst_id)
            {
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
            // A known-zero divisor folds to 0 even when the dividend is unknown.
            InstKind::Div(a, b) => match (get_const(*a), get_const(*b)) {
                (_, Some(b)) if b.is_zero() => LatticeValue::Constant(U256::ZERO),
                (Some(a), Some(b)) => LatticeValue::Constant(a / b),
                _ => self.check_any_bottom(&[*a, *b], lattice),
            },
            InstKind::SDiv(a, b) => match (get_const(*a), get_const(*b)) {
                (_, Some(b)) if b.is_zero() => LatticeValue::Constant(U256::ZERO),
                (Some(a), Some(b)) => LatticeValue::Constant(evm_word::signed_div(a, b)),
                _ => self.check_any_bottom(&[*a, *b], lattice),
            },
            InstKind::Mod(a, b) => match (get_const(*a), get_const(*b)) {
                (_, Some(b)) if b.is_zero() => LatticeValue::Constant(U256::ZERO),
                (Some(a), Some(b)) => LatticeValue::Constant(a % b),
                _ => self.check_any_bottom(&[*a, *b], lattice),
            },
            InstKind::SMod(a, b) => match (get_const(*a), get_const(*b)) {
                (_, Some(b)) if b.is_zero() => LatticeValue::Constant(U256::ZERO),
                (Some(a), Some(b)) => LatticeValue::Constant(evm_word::signed_mod(a, b)),
                _ => self.check_any_bottom(&[*a, *b], lattice),
            },
            // A known-zero modulus folds to 0 even when the operands are unknown.
            InstKind::AddMod(a, b, n) => match (get_const(*a), get_const(*b), get_const(*n)) {
                (_, _, Some(n)) if n.is_zero() => LatticeValue::Constant(U256::ZERO),
                (Some(a), Some(b), Some(n)) => LatticeValue::Constant(a.add_mod(b, n)),
                _ => self.check_any_bottom(&[*a, *b, *n], lattice),
            },
            InstKind::MulMod(a, b, n) => match (get_const(*a), get_const(*b), get_const(*n)) {
                (_, _, Some(n)) if n.is_zero() => LatticeValue::Constant(U256::ZERO),
                (Some(a), Some(b), Some(n)) => LatticeValue::Constant(a.mul_mod(b, n)),
                _ => self.check_any_bottom(&[*a, *b, *n], lattice),
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
            InstKind::SLt(a, b) => match (get_const(*a), get_const(*b)) {
                (Some(a), Some(b)) => LatticeValue::Constant(U256::from(evm_word::signed_lt(a, b))),
                _ => self.check_any_bottom(&[*a, *b], lattice),
            },
            InstKind::SGt(a, b) => match (get_const(*a), get_const(*b)) {
                (Some(a), Some(b)) => LatticeValue::Constant(U256::from(evm_word::signed_gt(a, b))),
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
            InstKind::Sar(shift, val) => match (get_const(*shift), get_const(*val)) {
                (Some(s), Some(v)) => LatticeValue::Constant(evm_word::sar(v, s)),
                _ => self.check_any_bottom(&[*shift, *val], lattice),
            },
            InstKind::Byte(index, val) => match (get_const(*index), get_const(*val)) {
                (Some(i), Some(v)) => LatticeValue::Constant(evm_word::byte(i, v)),
                _ => self.check_any_bottom(&[*index, *val], lattice),
            },
            InstKind::SignExtend(size, val) => match (get_const(*size), get_const(*val)) {
                (Some(s), Some(v)) => LatticeValue::Constant(evm_word::signextend(s, v)),
                _ => self.check_any_bottom(&[*size, *val], lattice),
            },

            InstKind::Select(condition, then_value, else_value) => {
                match &lattice[condition.index()] {
                    LatticeValue::Constant(c) => {
                        let chosen = if c.is_zero() { *else_value } else { *then_value };
                        lattice[chosen.index()].clone()
                    }
                    // Unknown condition: the result is whatever both arms agree on.
                    LatticeValue::Bottom => {
                        lattice[then_value.index()].meet(&lattice[else_value.index()])
                    }
                    LatticeValue::Top => match (get_const(*then_value), get_const(*else_value)) {
                        (Some(t), Some(e)) if t == e => LatticeValue::Constant(t),
                        _ => LatticeValue::Top,
                    },
                }
            }

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
                    LatticeValue::Bottom => {
                        // Both edges might be taken.
                        cfg_worklist.push_back((block_id, *then_block));
                        cfg_worklist.push_back((block_id, *else_block));
                    }
                    LatticeValue::Top => {}
                }
            }
            Terminator::Switch { value, default, cases } => {
                match &lattice[value.index()] {
                    LatticeValue::Constant(v) => {
                        // Cases are tested in order at runtime, so a constant
                        // case match is definitive only if every earlier case
                        // is a known constant that differs from the scrutinee.
                        // Overdefined earlier cases stay feasible; an
                        // unresolved case defers the remaining edges.
                        for &(case_val, target) in cases {
                            match &lattice[case_val.index()] {
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
        executable_edges: &FxHashSet<(BlockId, BlockId)>,
        cfg_worklist: &mut VecDeque<(BlockId, BlockId)>,
        ssa_worklist: &mut VecDeque<ValueId>,
    ) {
        for block_id in executable_blocks {
            self.evaluate_phis_in_block(
                func,
                *block_id,
                inst_to_value,
                lattice,
                executable_edges,
                ssa_worklist,
            );
        }

        // Find all instructions that use this value and re-evaluate them.
        for (block_id, block) in func.blocks.iter_enumerated() {
            if !executable_blocks.contains(&block_id) {
                continue;
            }
            for &inst_id in &block.instructions {
                let inst = &func.instructions[inst_id];
                if matches!(inst.kind, crate::mir::InstTag::Phi) {
                    continue;
                }
                let operands = inst.operands();
                if operands.contains(&vid)
                    && let Some(&result_vid) = inst_to_value.get(&inst_id)
                {
                    let new_val = self.evaluate_instruction(func, &inst.kind(), lattice);
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
        executable_edges: &FxHashSet<(BlockId, BlockId)>,
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
                // Create an immediate replacement of the instruction's result type.
                let imm = immediate_for_type(func.instructions[inst_id].result_ty, *c);
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
        for &block_id in &block_ids {
            if !executable_blocks.contains(&block_id) {
                continue;
            }
            let Some(term) = &func.blocks[block_id].terminator else {
                continue;
            };
            if !matches!(term, Terminator::Branch { .. } | Terminator::Switch { .. }) {
                continue;
            }

            let executable_successors: FxHashSet<_> = term
                .successors()
                .into_iter()
                .filter(|&successor| executable_edges.contains(&(block_id, successor)))
                .collect();
            if executable_successors.len() == 1 {
                let target = executable_successors.into_iter().next().expect("checked len");
                control_rewrites.push((block_id, target));
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
                let inst = &mut func.instructions[inst_id];
                inst.visit_operands_mut(|value| {
                    if let Some(&replacement) = const_values.get(value) {
                        *value = replacement;
                    }
                });
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
            if executable_blocks.contains(&block_id) {
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

        let phis_repaired = repair_reachability_phis(func);

        self.stats.constants_folded
            + self.stats.branches_folded
            + self.stats.switches_folded
            + self.stats.blocks_invalidated
            + usize::from(phis_repaired)
    }
}

/// Builds an immediate carrying `value` with the type the folded instruction
/// produced, falling back to `uint256` for types whose payload is not a plain
/// integer or whose range cannot represent the folded value (the lattice folds
/// at 256 bits, so a narrow-typed op can produce an out-of-range word). The
/// numeric value is identical in all cases.
fn immediate_for_type(ty: Option<MirType>, value: U256) -> Immediate {
    match ty {
        Some(MirType::Bool) if value <= U256::from(1) => Immediate::Bool(!value.is_zero()),
        Some(MirType::UInt(bits)) if fits_unsigned(value, bits) => Immediate::UInt(value, bits),
        Some(MirType::Int(bits)) if fits_signed(value, bits) => Immediate::Int(value, bits),
        _ => Immediate::uint256(value),
    }
}

fn fits_unsigned(value: U256, bits: u16) -> bool {
    bits >= 256 || value.bit_len() <= usize::from(bits)
}

fn fits_signed(value: U256, bits: u16) -> bool {
    if bits >= 256 || bits == 0 {
        return bits >= 256;
    }
    let bits = usize::from(bits);
    // Representable iff bits 255..=bits-1 of the 256-bit two's-complement word
    // all equal the sign bit.
    if value.bit(bits - 1) { (!value).bit_len() < bits } else { value.bit_len() < bits }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn immediate_for_type_preserves_result_types() {
        let one = U256::from(1);
        assert_eq!(immediate_for_type(Some(MirType::Bool), one), Immediate::Bool(true));
        assert_eq!(immediate_for_type(Some(MirType::Bool), U256::ZERO), Immediate::Bool(false));
        assert_eq!(immediate_for_type(Some(MirType::Int(256)), one), Immediate::Int(one, 256));
        assert_eq!(immediate_for_type(Some(MirType::UInt(64)), one), Immediate::UInt(one, 64));
        // Non-integer payloads and missing types fall back to uint256.
        assert_eq!(immediate_for_type(Some(MirType::Address), one), Immediate::uint256(one));
        assert_eq!(immediate_for_type(None, one), Immediate::uint256(one));
        // A bool-typed result that is not 0/1 keeps its numeric value.
        let two = U256::from(2);
        assert_eq!(immediate_for_type(Some(MirType::Bool), two), Immediate::uint256(two));
        // Out-of-range values fall back to uint256 instead of lying about the width.
        let wide = U256::from(0x1ff);
        assert_eq!(immediate_for_type(Some(MirType::UInt(8)), wide), Immediate::uint256(wide));
        assert_eq!(immediate_for_type(Some(MirType::Int(8)), wide), Immediate::uint256(wide));
        // Negative values are representable when the upper bits match the sign bit.
        let minus_one = U256::MAX;
        assert_eq!(
            immediate_for_type(Some(MirType::Int(8)), minus_one),
            Immediate::Int(minus_one, 8)
        );
        let i8_min = U256::MAX - U256::from(0x7f);
        assert_eq!(immediate_for_type(Some(MirType::Int(8)), i8_min), Immediate::Int(i8_min, 8));
        let i8_under = i8_min - U256::from(1);
        assert_eq!(
            immediate_for_type(Some(MirType::Int(8)), i8_under),
            Immediate::uint256(i8_under)
        );
    }
}
