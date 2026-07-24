//! Late MIR instruction ordering for EVM stack locality.
//!
//! Ordinary expression lowering already emits most trees in a useful depth-first order, but
//! optimization can create independent SSA subgraphs and long-lived shared values. Emitting those
//! instructions in their stored order can leave a producer far from its consumer, forcing the
//! physical stack scheduler to preserve, duplicate, or spill it. This pass reorders instructions
//! inside each basic block immediately before EVM codegen.
//!
//! The pass deliberately moves only computations and reads. Memory and state writes, calls,
//! creation, logs, `gas`, `msize`, and phis are barriers, so the transformation cannot move work
//! across an observable mutation, gas observation, call-gas boundary, or phi definition. Within
//! each barrier-delimited segment, a deterministic dependency-first traversal emits operand
//! producers in EVM push order and places values consumed by the following barrier or terminator
//! last. The pass commits a changed segment only when its active instruction results are
//! single-use; references left in the arena by eliminated instructions do not count. Shared
//! results retain their existing order because moving one use changes which physical copy should
//! survive for later consumers. It also preserves the producer order of binary operations whose
//! lowering already costs both equivalent operand orientations. Instruction and value identities
//! do not change; codegen recomputes liveness from the resulting order before stack scheduling.
//!
//! The dependency-first shape is adapted from [Vyper Venom's DFT pass]. Venom makes shared values
//! movable with a preceding single-use expansion pass; this implementation instead rejects those
//! segments so the late transform cannot inflate MIR or bytecode.
//!
//! [Vyper Venom's DFT pass]: https://github.com/vyperlang/vyper/blob/730a2d36f1fca90be059c75681de5c942560ce0b/vyper/venom/passes/dft.py

use crate::{
    mir::{Function, InstId, InstKind, Instruction, Module, Terminator, Value, ValueId},
    pass::{MirPass, ModuleAnalyses, run_function_pass},
};
use smallvec::SmallVec;
use solar_data_structures::bit_set::DenseBitSet;
use solar_sema::Gcx;

/// Orders movable MIR instructions for the EVM stack scheduler.
pub(crate) struct EvmInstSchedule;

impl MirPass for EvmInstSchedule {
    fn name(&self) -> &'static str {
        "evm-inst-schedule"
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        run_function_pass(module, analyses, |func, _| Self::run_on_function(func))
    }
}

impl EvmInstSchedule {
    fn run_on_function(func: &mut Function) -> bool {
        let mut changed = false;
        let block_ids = func.blocks.indices().collect::<Vec<_>>();
        let shared_results = Self::shared_results(func);
        let mut scratch = ScheduleScratch::new(func.instructions.len());

        for block_id in block_ids {
            let original = std::mem::take(&mut func.blocks[block_id].instructions);
            if original.len() < 2 {
                func.blocks[block_id].instructions = original;
                continue;
            }

            let mut ordered = Vec::with_capacity(original.len());
            let mut segment_start = 0;
            for (index, &inst_id) in original.iter().enumerate() {
                let inst = &func.instructions[inst_id];
                if Self::is_movable(inst) {
                    continue;
                }

                let consumer = Self::stack_input_order(&inst.kind);
                Self::schedule_segment(
                    func,
                    &original[segment_start..index],
                    &consumer,
                    &shared_results,
                    &mut scratch,
                    &mut ordered,
                );
                ordered.push(inst_id);
                segment_start = index + 1;
            }

            let terminator_inputs = func.blocks[block_id]
                .terminator
                .as_ref()
                .map(Self::terminator_stack_input_order)
                .unwrap_or_default();
            Self::schedule_segment(
                func,
                &original[segment_start..],
                &terminator_inputs,
                &shared_results,
                &mut scratch,
                &mut ordered,
            );

            if ordered != original {
                func.blocks[block_id].instructions = ordered;
                changed = true;
            } else {
                func.blocks[block_id].instructions = original;
            }
        }

        changed
    }
}

impl EvmInstSchedule {
    /// Whether an instruction may move among other read-only instructions in the same segment.
    fn is_movable(inst: &Instruction) -> bool {
        if matches!(inst.kind, InstKind::Phi(_) | InstKind::Gas | InstKind::MSize) {
            return false;
        }

        matches!(
            inst.metadata.effect().unwrap_or_else(|| inst.kind.effect_kind()),
            crate::mir::EffectKind::Pure
                | crate::mir::EffectKind::MemoryRead
                | crate::mir::EffectKind::StorageRead
                | crate::mir::EffectKind::TransientRead
                | crate::mir::EffectKind::EnvironmentRead
        )
    }

    /// Returns operands in the order their producers should run for EVM emission: deepest stack
    /// input first and the eventual top-of-stack input last.
    fn stack_input_order(kind: &InstKind) -> SmallVec<[ValueId; 8]> {
        let mut operands = kind.operands();
        operands.reverse();
        operands
    }

    fn terminator_stack_input_order(term: &Terminator) -> SmallVec<[ValueId; 8]> {
        let mut operands = SmallVec::from_iter(term.operands());
        operands.reverse();
        operands
    }

    fn schedule_segment(
        func: &Function,
        segment: &[InstId],
        consumer_inputs: &[ValueId],
        shared_results: &DenseBitSet<InstId>,
        scratch: &mut ScheduleScratch,
        ordered: &mut Vec<InstId>,
    ) {
        if segment.len() < 2 {
            ordered.extend_from_slice(segment);
            return;
        }

        // Depth-first ordering is predictable for single-use expression trees. Shared instruction
        // results need a cost model for deciding which physical copy should survive after moving
        // one use. Venom handles that by expanding shared values before DFT; keep our established
        // order instead and reject those segments before building their dependency order. Most
        // optimized MIR has some sharing, so this is also the common compile-time fast path.
        if segment.iter().any(|&inst_id| shared_results.contains(inst_id)) {
            ordered.extend_from_slice(segment);
            return;
        }

        let output_start = ordered.len();
        scratch.clear_segment();
        for &inst_id in segment {
            scratch.members.insert(inst_id);
            scratch.active_members.push(inst_id);
        }
        for &inst_id in segment {
            for operand in func.instructions[inst_id].kind.operands() {
                if let Value::Inst(dependency) = func.value(operand)
                    && scratch.members.contains(*dependency)
                {
                    scratch.dependencies.insert(*dependency);
                }
            }
        }

        let consumer_roots = consumer_inputs
            .iter()
            .filter_map(|&value| match func.value(value) {
                Value::Inst(inst_id) if scratch.members.contains(*inst_id) => Some(*inst_id),
                _ => None,
            })
            .collect::<SmallVec<[InstId; 8]>>();
        for &inst_id in &consumer_roots {
            scratch.consumer_roots.insert(inst_id);
        }

        // Values not consumed by the immediate barrier or terminator stay below its operands.
        // Emit those roots first, then arrange the immediate consumer's roots in stack-input order.
        for &inst_id in segment {
            if !scratch.dependencies.contains(inst_id) && !scratch.consumer_roots.contains(inst_id)
            {
                Self::visit_dependencies(func, inst_id, scratch, ordered);
            }
        }
        for inst_id in consumer_roots {
            Self::visit_dependencies(func, inst_id, scratch, ordered);
        }

        // The final stable sweep handles disconnected, malformed, or result-less movable MIR
        // conservatively without dropping an instruction.
        for &inst_id in segment {
            Self::visit_dependencies(func, inst_id, scratch, ordered);
        }

        // The backend already considers both operand orders for commutative operations and
        // comparison/opcode pairs. Preserve their existing producer order too: reversing it cannot
        // expose a new operand plan, but can disturb useful copies below the local expression tree.
        let changed = ordered[output_start..] != *segment;
        if changed
            && !Self::preserves_reorderable_operand_order(
                func,
                segment,
                &ordered[output_start..],
                scratch,
            )
        {
            ordered.truncate(output_start);
            ordered.extend_from_slice(segment);
        }
    }

    fn preserves_reorderable_operand_order(
        func: &Function,
        original: &[InstId],
        candidate: &[InstId],
        scratch: &mut ScheduleScratch,
    ) -> bool {
        for (position, &inst_id) in original.iter().enumerate() {
            scratch.original_positions[inst_id.index()] = position;
        }
        for (position, &inst_id) in candidate.iter().enumerate() {
            scratch.candidate_positions[inst_id.index()] = position;
        }

        for &inst_id in original {
            let Some((a, b)) = Self::reorderable_binary_operands(&func.instructions[inst_id].kind)
            else {
                continue;
            };
            let (Value::Inst(a), Value::Inst(b)) = (func.value(a), func.value(b)) else {
                continue;
            };
            if !scratch.members.contains(*a) || !scratch.members.contains(*b) {
                continue;
            }
            let original_a = scratch.original_positions[a.index()];
            let original_b = scratch.original_positions[b.index()];
            let candidate_a = scratch.candidate_positions[a.index()];
            let candidate_b = scratch.candidate_positions[b.index()];
            if original_a.cmp(&original_b) != candidate_a.cmp(&candidate_b) {
                return false;
            }
        }
        true
    }

    fn reorderable_binary_operands(kind: &InstKind) -> Option<(ValueId, ValueId)> {
        match kind {
            InstKind::Add(a, b)
            | InstKind::Mul(a, b)
            | InstKind::And(a, b)
            | InstKind::Or(a, b)
            | InstKind::Xor(a, b)
            | InstKind::Eq(a, b)
            | InstKind::Lt(a, b)
            | InstKind::Gt(a, b)
            | InstKind::SLt(a, b)
            | InstKind::SGt(a, b) => Some((*a, *b)),
            _ => None,
        }
    }

    fn shared_results(func: &Function) -> DenseBitSet<InstId> {
        let mut counts = vec![0u32; func.values.len()];
        // Instruction arenas retain replaced and eliminated instructions, but only instructions
        // still present in a block reach codegen. Retired uses must not make a live single-use tree
        // look shared and disable scheduling for its whole segment.
        for block in &func.blocks {
            for &inst_id in &block.instructions {
                for operand in func.instructions[inst_id].kind.operands() {
                    counts[operand.index()] += 1;
                }
            }
            if let Some(terminator) = &block.terminator {
                for operand in terminator.operands() {
                    counts[operand.index()] += 1;
                }
            }
        }

        // Rewrites can leave multiple value aliases for one instruction in the arena. Match
        // `Function::inst_result_value` and codegen by classifying the first, canonical result;
        // later aliases are retired bookkeeping rather than extra physical stack results.
        let mut seen_results = DenseBitSet::new_empty(func.instructions.len());
        let mut shared = DenseBitSet::new_empty(func.instructions.len());
        for (value_id, value) in func.values.iter_enumerated() {
            if let Value::Inst(inst_id) = value
                && seen_results.insert(*inst_id)
                && counts[value_id.index()] > 1
            {
                shared.insert(*inst_id);
            }
        }
        shared
    }

    fn visit_dependencies(
        func: &Function,
        root: InstId,
        scratch: &mut ScheduleScratch,
        ordered: &mut Vec<InstId>,
    ) {
        scratch.work.clear();
        scratch.work.push((root, false));
        while let Some((inst_id, emit)) = scratch.work.pop() {
            if emit {
                ordered.push(inst_id);
                continue;
            }
            if !scratch.visited.insert(inst_id) {
                continue;
            }

            scratch.work.push((inst_id, true));
            let inputs = Self::stack_input_order(&func.instructions[inst_id].kind);
            for value in inputs.iter().rev() {
                if let Value::Inst(dependency) = func.value(*value)
                    && scratch.members.contains(*dependency)
                    && !scratch.visited.contains(*dependency)
                {
                    scratch.work.push((*dependency, false));
                }
            }
        }
    }
}

struct ScheduleScratch {
    members: DenseBitSet<InstId>,
    dependencies: DenseBitSet<InstId>,
    consumer_roots: DenseBitSet<InstId>,
    visited: DenseBitSet<InstId>,
    original_positions: Vec<usize>,
    candidate_positions: Vec<usize>,
    active_members: Vec<InstId>,
    work: Vec<(InstId, bool)>,
}

impl ScheduleScratch {
    fn new(instruction_count: usize) -> Self {
        Self {
            members: DenseBitSet::new_empty(instruction_count),
            dependencies: DenseBitSet::new_empty(instruction_count),
            consumer_roots: DenseBitSet::new_empty(instruction_count),
            visited: DenseBitSet::new_empty(instruction_count),
            original_positions: vec![0; instruction_count],
            candidate_positions: vec![0; instruction_count],
            active_members: Vec::new(),
            work: Vec::new(),
        }
    }

    fn clear_segment(&mut self) {
        for inst_id in self.active_members.drain(..) {
            self.members.remove(inst_id);
            self.dependencies.remove(inst_id);
            self.consumer_roots.remove(inst_id);
            self.visited.remove(inst_id);
        }
    }
}
