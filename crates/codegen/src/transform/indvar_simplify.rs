//! Induction-variable simplification and strength reduction.
//!
//! This pass recognizes loop-local address expressions of the form
//! `base + iv * stride + constant` and replaces their loop uses with a
//! loop-carried pointer phi:
//!
//! ```text
//! ptr = phi [preheader: base + init * stride + constant], [latch: ptr + step * stride]
//! ```
//!
//! The initial implementation is deliberately narrow. It requires canonical
//! loops with a dedicated preheader, a single latch, and a single additive
//! induction-variable update. That gives later loop optimizations a real
//! ScalarEvolution-backed transform without guessing from ad hoc instruction
//! patterns.
//!
//! Safety contract:
//! - require canonical loops with a preheader and a single latch
//! - rewrite only affine address expressions derived from the recognized induction variable
//! - preserve the original address value when it is still used outside the loop

use crate::{
    analysis::{AffineExpr, Loop, LoopAnalyzer, ScalarEvolution},
    mir::{
        BlockId, Function, Immediate, InstId, InstKind, Instruction, MirType, Value, ValueId,
        utils as mir_utils,
    },
    pass::FunctionPass,
};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashMap;

/// Statistics from induction-variable simplification.
#[derive(Clone, Debug, Default)]
pub struct IndVarSimplifyStats {
    /// Number of loop-carried pointer phis inserted.
    pub pointer_phis_inserted: usize,
    /// Number of loop-local address uses replaced.
    pub address_uses_replaced: usize,
}

impl IndVarSimplifyStats {
    /// Returns the total number of MIR changes performed.
    #[must_use]
    pub const fn total(&self) -> usize {
        self.pointer_phis_inserted + self.address_uses_replaced
    }
}

/// Performs conservative induction-variable strength reduction.
#[derive(Debug, Default)]
pub struct IndVarSimplifier {
    stats: IndVarSimplifyStats,
}

/// Function pass for induction-variable simplification and strength reduction.
pub struct IndVarSimplifyPass;

impl FunctionPass for IndVarSimplifyPass {
    fn name(&self) -> &str {
        "indvar-simplify"
    }

    fn run_on_function(&mut self, func: &mut Function) -> bool {
        IndVarSimplifier::new().run(func).total() != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct AddressKey {
    base: ValueId,
    iv: ValueId,
    scale: i128,
    constant: i128,
}

impl IndVarSimplifier {
    /// Creates a new induction-variable simplifier.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns statistics from the last run.
    #[must_use]
    pub const fn stats(&self) -> &IndVarSimplifyStats {
        &self.stats
    }

    /// Runs induction-variable simplification once over `func`.
    pub fn run(&mut self, func: &mut Function) -> &IndVarSimplifyStats {
        self.stats = IndVarSimplifyStats::default();

        let mut analyzer = LoopAnalyzer::new();
        let loop_info = analyzer.analyze(func);
        let mut loops: Vec<_> = loop_info.loops.values().cloned().collect();
        loops.sort_by_key(|loop_data| loop_data.header.index());

        for loop_data in loops {
            self.run_loop(func, &loop_data);
        }

        &self.stats
    }

    fn run_loop(&mut self, func: &mut Function, loop_data: &Loop) {
        let Some(preheader) = loop_data.preheader else { return };
        let [latch] = loop_data.back_edges.as_slice() else { return };
        let [iv] = loop_data.induction_vars.as_slice() else { return };
        let Some(step) = self.additive_step(func, iv.value, iv.update_inst) else {
            return;
        };

        let scev = ScalarEvolution::analyze(func, loop_data);
        let inst_results = func.inst_results();
        let mut candidates: FxHashMap<AddressKey, Vec<ValueId>> = FxHashMap::default();

        let mut blocks: Vec<_> = loop_data.blocks.iter().collect();
        blocks.sort_by_key(|block| block.index());
        for block in blocks {
            let insts = func.blocks[block].instructions.clone();
            for inst_id in insts {
                let Some(&value) = inst_results.get(&inst_id) else { continue };
                if !self.is_reducible_result(func, inst_id) {
                    continue;
                }
                let Some(key) = self.address_key(&scev, value, iv.value) else {
                    continue;
                };
                let Some(delta) = key.scale.checked_mul(step) else { continue };
                if delta <= 0 || !self.has_non_address_loop_use(func, loop_data, value) {
                    continue;
                }
                candidates.entry(key).or_default().push(value);
            }
        }

        if candidates.is_empty() {
            return;
        }

        let mut replacements = FxHashMap::default();
        for (key, values) in candidates {
            let Some(pointer) =
                self.materialize_pointer_phi(func, loop_data, preheader, *latch, key)
            else {
                continue;
            };
            for value in values {
                replacements.insert(value, pointer);
            }
        }

        if replacements.is_empty() {
            return;
        }

        self.stats.address_uses_replaced += self.replace_loop_uses(func, loop_data, &replacements);
    }

    fn additive_step(
        &self,
        func: &Function,
        iv_value: ValueId,
        update_inst: Option<InstId>,
    ) -> Option<i128> {
        let update_inst = update_inst?;
        let InstKind::Add(a, b) = func.instructions[update_inst].kind else {
            return None;
        };
        let step = if a == iv_value {
            b
        } else if b == iv_value {
            a
        } else {
            return None;
        };
        self.value_i128(func, step)
    }

    fn address_key(
        &self,
        scev: &ScalarEvolution,
        value: ValueId,
        iv_value: ValueId,
    ) -> Option<AddressKey> {
        let AffineExpr { base, constant, terms } = scev.get(value)?.clone();
        let base = base?;
        let [term] = terms.as_slice() else { return None };
        if term.value != iv_value || term.scale <= 0 {
            return None;
        }
        if constant < 0 {
            return None;
        }
        Some(AddressKey { base, iv: iv_value, scale: term.scale, constant })
    }

    fn materialize_pointer_phi(
        &mut self,
        func: &mut Function,
        loop_data: &Loop,
        preheader: BlockId,
        latch: BlockId,
        key: AddressKey,
    ) -> Option<ValueId> {
        let iv = loop_data.induction_vars.iter().find(|iv| iv.value == key.iv)?;
        let init = self.value_i128(func, iv.init)?;
        let init_offset = init.checked_mul(key.scale)?.checked_add(key.constant)?;
        let delta = self.additive_step(func, key.iv, iv.update_inst)?.checked_mul(key.scale)?;
        if delta <= 0 {
            return None;
        }

        let initial = self.build_base_plus_offset(func, preheader, key.base, init_offset)?;
        let phi_inst = func.alloc_inst(Instruction::new(
            InstKind::Phi(vec![(preheader, initial)]),
            Some(MirType::uint256()),
        ));
        let phi_value = func.alloc_value(Value::Inst(phi_inst));
        self.insert_header_phi(func, loop_data.header, phi_inst);

        let delta = self.offset_value(func, delta)?;
        let next = self.append_inst_value(
            func,
            latch,
            InstKind::Add(phi_value, delta),
            Some(MirType::uint256()),
        );
        let InstKind::Phi(incoming) = &mut func.instructions[phi_inst].kind else {
            return None;
        };
        incoming.push((latch, next));
        self.stats.pointer_phis_inserted += 1;
        Some(phi_value)
    }

    fn build_base_plus_offset(
        &self,
        func: &mut Function,
        block: BlockId,
        base: ValueId,
        offset: i128,
    ) -> Option<ValueId> {
        if offset == 0 {
            return Some(base);
        }
        let offset = self.offset_value(func, offset)?;
        Some(self.append_inst_value(
            func,
            block,
            InstKind::Add(base, offset),
            Some(MirType::uint256()),
        ))
    }

    fn offset_value(&self, func: &mut Function, offset: i128) -> Option<ValueId> {
        if offset < 0 {
            return None;
        }
        Some(func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(offset as u128)))))
    }

    fn append_inst_value(
        &self,
        func: &mut Function,
        block: BlockId,
        kind: InstKind,
        ty: Option<MirType>,
    ) -> ValueId {
        let inst = func.alloc_inst(Instruction::new(kind, ty));
        func.blocks[block].instructions.push(inst);
        func.alloc_value(Value::Inst(inst))
    }

    fn insert_header_phi(&self, func: &mut Function, header: BlockId, phi_inst: InstId) {
        let insert_pos = func.blocks[header]
            .instructions
            .iter()
            .take_while(|&&inst_id| matches!(func.instructions[inst_id].kind, InstKind::Phi(_)))
            .count();
        func.blocks[header].instructions.insert(insert_pos, phi_inst);
    }

    fn is_reducible_result(&self, func: &Function, inst_id: InstId) -> bool {
        if func.instructions[inst_id].result_ty != Some(MirType::uint256()) {
            return false;
        }
        matches!(
            func.instructions[inst_id].kind,
            InstKind::Add(_, _) | InstKind::Sub(_, _) | InstKind::Mul(_, _) | InstKind::Shl(_, _)
        )
    }

    fn value_i128(&self, func: &Function, value: ValueId) -> Option<i128> {
        let value = match func.value(value) {
            Value::Immediate(imm) => imm.as_u256()?,
            _ => return None,
        };
        if value <= U256::from(i128::MAX as u128) { Some(value.to::<u128>() as i128) } else { None }
    }

    fn has_non_address_loop_use(&self, func: &Function, loop_data: &Loop, value: ValueId) -> bool {
        for block in loop_data.blocks.iter() {
            for &inst_id in &func.blocks[block].instructions {
                let kind = &func.instructions[inst_id].kind;
                if kind.operands().contains(&value) && !Self::is_address_builder(kind) {
                    return true;
                }
            }
            if func.blocks[block]
                .terminator
                .as_ref()
                .is_some_and(|term| term.operands().contains(&value))
            {
                return true;
            }
        }
        false
    }

    fn is_address_builder(kind: &InstKind) -> bool {
        matches!(
            kind,
            InstKind::Add(_, _) | InstKind::Sub(_, _) | InstKind::Mul(_, _) | InstKind::Shl(_, _)
        )
    }

    fn replace_loop_uses(
        &self,
        func: &mut Function,
        loop_data: &Loop,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> usize {
        let mut replaced = 0;
        for block in loop_data.blocks.iter() {
            let insts = func.blocks[block].instructions.clone();
            for inst_id in insts {
                replaced += mir_utils::replace_inst_uses(
                    &mut func.instructions[inst_id].kind,
                    replacements,
                );
            }
            if let Some(term) = &mut func.blocks[block].terminator {
                replaced += mir_utils::replace_terminator_uses(term, replacements);
            }
        }
        replaced
    }
}
