//! Late MIR instruction ordering for EVM stack locality.
//!
//! Ordinary expression lowering already emits most trees in a useful depth-first order, but
//! optimization can create independent SSA subgraphs and long-lived shared values. Emitting those
//! instructions in their stored order can leave a producer far from its consumer, forcing the
//! physical stack scheduler to preserve, duplicate, or spill it. This pass reorders instructions
//! inside each basic block immediately before EVM codegen.
//!
//! The pass deliberately moves only computations and reads. Memory and state writes, calls,
//! creation, logs, control effects, `gas`, `msize`, and phis are barriers, so the pass cannot move
//! work across an observable mutation, gas observation, call-gas boundary, or phi definition.
//! Within each barrier-delimited segment, a deterministic dependency-first traversal emits operand
//! producers in EVM push order and places values consumed by the following barrier or terminator
//! last. Shared-result producers stay at their original positions because moving one use changes
//! which physical copy should survive for later consumers. Single-use islands between those pinned
//! producers are still scheduled independently; references left in the arena by eliminated
//! instructions do not count as sharing. The pass also preserves the producer order of binary
//! operations whose lowering already costs both equivalent operand orientations.
//! Instruction and value identities do not change; codegen recomputes liveness from the resulting
//! order before stack scheduling.
//!
//! One move crosses barriers. A pure instruction whose single reader follows memory writes or logs
//! in the same block moves to just before that reader when every word it reads stays live there
//! anyway: rebuilt where it is read, live out of the block, or read again later. Its result then
//! no longer occupies the stack across the writes, and no operand lives longer, as an unrolled
//! copy's store address `add p, 2` computed ahead of the copy's other stores shows. A pure
//! instruction reads no memory, so no write changes its result. It never crosses a `gas`
//! reading, a call, a creation, or a storage write, since the moved work would change the gas
//! they observe, forward, or check. Range checks of calldata arguments move next to the branch
//! that reads them instead of riding the stack, or a spill slot, across the decoder's stores.
//!
//! This is a locality heuristic, not a whole-function profitability search. It does not price the
//! residual physical stack left by each possible order, so an isolated function can grow even when
//! aggregate corpus output improves. Keeping the pass separate from physical scheduling makes that
//! tradeoff measurable and leaves room for a later cost-aware selector.
//!
//! The dependency-first shape is adapted from [Vyper Venom's DFT pass]. Venom makes shared values
//! movable with a preceding single-use expansion pass; this implementation instead pins shared
//! producers so the late transform does not clone shared expressions or inflate MIR.
//!
//! [solx's EVM single-use-expression pass] likewise moves one-use definitions next to their uses,
//! using machine live intervals and stackification metadata. This pass works before machine
//! lowering, keeps MIR identities unchanged, and leaves physical stack decisions to the backend.
//!
//! [Vyper Venom's DFT pass]: https://github.com/vyperlang/vyper/blob/730a2d36f1fca90be059c75681de5c942560ce0b/vyper/venom/passes/dft.py
//! [solx's EVM single-use-expression pass]: https://github.com/NomicFoundation/solx-llvm/blob/a2a603232892c9824f8783b55b49d5655d77a62c/llvm/lib/Target/EVM/EVMSingleUseExpression.cpp

use crate::mir::{
    BlockId, EffectKind, Function, InstId, InstKind, Instruction, Module, Terminator, Value,
    ValueId,
    analysis::Liveness,
    pass::{MirPass, ModuleAnalyses, run_function_pass},
};
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
};
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
        let block_ids = func.blocks.indices();
        let shared_results = Self::shared_results(func);
        let mut scratch = ScheduleScratch::new(func.num_insts());
        let liveness = Liveness::compute_live_sets(func);
        let users = Self::user_counts(func);

        for block_id in block_ids {
            changed |= Self::sink_past_barriers(func, block_id, &liveness, &users);
            let original = std::mem::take(&mut func.blocks[block_id].instructions);
            if original.len() < 2 {
                func.blocks[block_id].instructions = original;
                continue;
            }

            let mut ordered = Vec::with_capacity(original.len());
            let mut segment_start = 0;
            for (index, &inst_id) in original.iter().enumerate() {
                let inst = func.inst(inst_id);
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
    /// Moves each pure instruction whose single user follows a barrier in the same block to just
    /// before that user, when every word it reads stays live past the user anyway: its result no
    /// longer occupies the stack across the barrier, and no operand lives longer. A pure
    /// instruction reads no memory or state, so a write cannot change its result. Only memory
    /// writes and logs are crossed: work moved past a `gas` reading or a call would change the
    /// gas they observe or forward, and past a storage write the gas its sentry checks.
    ///
    /// ```text
    /// v1 = add v0, 2            mstore8 v3, v4
    /// mstore8 v3, v4      =>    v1 = add v0, 2
    /// mstore8 v1, v5            mstore8 v1, v5
    /// ```
    fn sink_past_barriers(
        func: &mut Function,
        block: BlockId,
        liveness: &Liveness,
        users: &IndexVec<ValueId, u32>,
    ) -> bool {
        let instructions = &func.blocks[block].instructions;
        let len = instructions.len();
        if len < 3 {
            return false;
        }
        let terminator_operands =
            func.blocks[block].terminator.as_ref().map(Terminator::operands).unwrap_or_default();
        // The user each sunk instruction moves in front of, by position, and the barriers, and
        // the barriers no instruction may cross, from each position to the end of the block.
        let mut target = vec![None::<usize>; len];
        let mut barriers_after = vec![0usize; len + 1];
        let mut walls_after = vec![0usize; len + 1];
        for index in (0..len).rev() {
            let inst = func.inst(instructions[index]);
            let barrier = !Self::is_movable(inst);
            barriers_after[index] = barriers_after[index + 1] + usize::from(barrier);
            walls_after[index] =
                walls_after[index + 1] + usize::from(barrier && !Self::is_crossable(inst));
        }
        // The position each instruction ends up just in front of: its own, or its user's anchor.
        let mut anchor: Vec<usize> = (0..=len).collect();
        let live_out = liveness.live_out(block);
        for index in (0..len).rev() {
            let inst_id = instructions[index];
            let inst = func.inst(inst_id);
            if inst.kind.effect_kind() != EffectKind::Pure
                || matches!(inst.kind, InstKind::Phi(_))
                || inst.kind.effects().control.any()
                || inst.metadata.effect().is_some_and(|effect| effect != EffectKind::Pure)
            {
                continue;
            }
            let Some(result) = func.inst_result_value(inst_id) else { continue };
            if users[result] != 1 || live_out.contains(result) {
                continue;
            }
            // The single user: a later instruction of this block or its terminator.
            let user = if terminator_operands.contains(&result) {
                len
            } else {
                match (index + 1..len)
                    .find(|&later| func.inst(instructions[later]).kind.operands().contains(&result))
                {
                    Some(later) => later,
                    None => continue,
                }
            };
            let destination = anchor[user];
            if barriers_after[index + 1] == barriers_after[destination]
                || walls_after[index + 1] != walls_after[destination]
            {
                continue;
            }
            // Every operand stays live past the destination: rebuilt where it is read, live out
            // of the block, or read again at or after the destination.
            let rebuilt =
                |operand| matches!(func.value(operand), Value::Immediate(_) | Value::Arg(_));
            let stays_live = inst.kind.operands().into_iter().all(|operand| {
                rebuilt(operand)
                    || live_out.contains(operand)
                    || terminator_operands.contains(&operand)
                    || (destination..len).any(|later| {
                        anchor[later] == later
                            && func.inst(instructions[later]).kind.operands().contains(&operand)
                    })
            });
            if !stays_live {
                continue;
            }
            target[index] = Some(user);
            anchor[index] = destination;
        }
        if target.iter().all(Option::is_none) {
            return false;
        }
        // Emit each kept instruction after the instructions sunk in front of it, in order, and
        // each of those after the ones sunk in front of it in turn.
        let mut attached = vec![SmallVec::<[usize; 2]>::new(); len + 1];
        for (index, user) in target.iter().enumerate() {
            if let Some(user) = *user {
                attached[user].push(index);
            }
        }
        let mut ordered = Vec::with_capacity(len);
        let mut work = Vec::new();
        for (index, sunk) in target.iter().map(Option::is_some).chain([false]).enumerate() {
            if sunk {
                continue;
            }
            work.push((index, false));
            while let Some((index, expanded)) = work.pop() {
                if expanded {
                    if index < len {
                        ordered.push(instructions[index]);
                    }
                    continue;
                }
                work.push((index, true));
                work.extend(attached[index].iter().rev().map(|&before| (before, false)));
            }
        }
        debug_assert_eq!(ordered.len(), len);
        func.blocks[block].instructions = ordered;
        true
    }

    /// Whether a pure instruction may move past this barrier: a memory write or a log, which
    /// observe neither the gas left nor the words a pure instruction computes.
    fn is_crossable(inst: &Instruction) -> bool {
        let crossable = |effect| matches!(effect, EffectKind::MemoryWrite | EffectKind::Log);
        !matches!(inst.kind, InstKind::Gas | InstKind::MSize | InstKind::Phi(_))
            && !inst.kind.effects().control.any()
            && crossable(inst.kind.effect_kind())
            && inst.metadata.effect().is_none_or(crossable)
    }

    fn user_counts(func: &Function) -> IndexVec<ValueId, u32> {
        let mut counts = index_vec![0u32; func.num_values()];
        for block in &func.blocks {
            for &inst_id in &block.instructions {
                func.inst(inst_id).kind.visit_operands(|operand| counts[operand] += 1);
            }
            if let Some(terminator) = &block.terminator {
                terminator.operands().into_iter().for_each(|operand| counts[operand] += 1);
            }
        }
        counts
    }

    /// Whether an instruction may move among other read-only instructions in the same segment.
    fn is_movable(inst: &Instruction) -> bool {
        if matches!(inst.kind, InstKind::Phi(_) | InstKind::Gas | InstKind::MSize) {
            return false;
        }

        !inst.kind.effects().control.any()
            && Self::is_movable_effect(inst.kind.effect_kind())
            && inst.metadata.effect().is_none_or(Self::is_movable_effect)
    }

    fn is_movable_effect(effect: crate::mir::EffectKind) -> bool {
        matches!(
            effect,
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

        let mut island_start = 0;
        for (index, &inst_id) in segment.iter().enumerate() {
            if !shared_results.contains(inst_id) {
                continue;
            }
            let inputs = Self::stack_input_order(&func.inst(inst_id).kind);
            Self::schedule_single_use_segment(
                func,
                &segment[island_start..index],
                &inputs,
                scratch,
                ordered,
            );
            ordered.push(inst_id);
            island_start = index + 1;
        }
        Self::schedule_single_use_segment(
            func,
            &segment[island_start..],
            consumer_inputs,
            scratch,
            ordered,
        );
    }

    fn schedule_single_use_segment(
        func: &Function,
        segment: &[InstId],
        consumer_inputs: &[ValueId],
        scratch: &mut ScheduleScratch,
        ordered: &mut Vec<InstId>,
    ) {
        if segment.len() < 2 {
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
            func.inst(inst_id).kind.visit_operands(|operand| {
                if let Value::Inst(dependency) = func.value(operand)
                    && scratch.members.contains(*dependency)
                {
                    scratch.dependencies.insert(*dependency);
                }
            });
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
            scratch.original_positions[inst_id] = position;
        }
        for (position, &inst_id) in candidate.iter().enumerate() {
            scratch.candidate_positions[inst_id] = position;
        }

        for &inst_id in original {
            let Some((a, b)) = func.inst(inst_id).kind.reorderable_binary_operands() else {
                continue;
            };
            let (Value::Inst(a), Value::Inst(b)) = (func.value(a), func.value(b)) else {
                continue;
            };
            if !scratch.members.contains(*a) || !scratch.members.contains(*b) {
                continue;
            }
            let original_a = scratch.original_positions[*a];
            let original_b = scratch.original_positions[*b];
            let candidate_a = scratch.candidate_positions[*a];
            let candidate_b = scratch.candidate_positions[*b];
            if original_a.cmp(&original_b) != candidate_a.cmp(&candidate_b) {
                return false;
            }
        }
        true
    }

    fn shared_results(func: &Function) -> DenseBitSet<InstId> {
        let mut user_counts = index_vec![0u32; func.num_values()];
        let mut seen = index_vec![0usize; func.num_values()];
        let mut generation = 0usize;
        // Instruction arenas retain replaced and eliminated instructions, but only instructions
        // still present in a block reach codegen. Retired uses must not make a live single-use tree
        // look shared and disable scheduling for its whole segment. Repeated operands in one
        // consumer count as one user.
        for block in &func.blocks {
            for &inst_id in &block.instructions {
                generation += 1;
                count_distinct_users(
                    func.inst(inst_id).kind.operands(),
                    &mut user_counts,
                    &mut seen,
                    generation,
                );
            }
            if let Some(terminator) = &block.terminator {
                generation += 1;
                count_distinct_users(
                    terminator.operands(),
                    &mut user_counts,
                    &mut seen,
                    generation,
                );
            }
        }

        let mut shared = DenseBitSet::new_empty(func.num_insts());
        for block in &func.blocks {
            for &inst_id in &block.instructions {
                if let Some(result) = func.inst_result_value(inst_id)
                    && user_counts[result] > 1
                {
                    shared.insert(inst_id);
                }
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
            let inputs = Self::stack_input_order(&func.inst(inst_id).kind);
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

fn count_distinct_users(
    operands: impl IntoIterator<Item = ValueId>,
    counts: &mut IndexVec<ValueId, u32>,
    seen: &mut IndexVec<ValueId, usize>,
    generation: usize,
) {
    for operand in operands {
        if seen[operand] != generation {
            seen[operand] = generation;
            counts[operand] += 1;
        }
    }
}

struct ScheduleScratch {
    members: DenseBitSet<InstId>,
    dependencies: DenseBitSet<InstId>,
    consumer_roots: DenseBitSet<InstId>,
    visited: DenseBitSet<InstId>,
    original_positions: IndexVec<InstId, usize>,
    candidate_positions: IndexVec<InstId, usize>,
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
            original_positions: index_vec![0; instruction_count],
            candidate_positions: index_vec![0; instruction_count],
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
