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
//! last. Shared-result producers stay at their original positions because moving one use changes
//! which physical copy should survive for later consumers. Single-use islands between those pinned
//! producers are still scheduled independently; references left in the arena by eliminated
//! instructions do not count as sharing. The pass also preserves the producer order of binary
//! operations whose lowering already costs both equivalent operand orientations.
//! In functions with unrestricted assembly memory, a single-use bitwise reduction of more than
//! sixteen gas observations may move from a dominated acyclic block to just before a call in the
//! observations' block. Only total bitwise operations move, after every gas observation in that
//! block; neither reads nor calls move. This lets the backend carry one result across the call
//! without private memory. Cyclic blocks and shared intermediate values are excluded.
//! In modules requiring stack-owned state, constant offset additions may also move into their
//! use blocks when an incoming layout exceeds sixteen live values and already carries the base.
//! Each use block receives its own addition; phi operands and bases not already live at entry
//! are excluded. This removes redundant incoming words without extending the base across edges.
//! Codegen recomputes liveness after these rewrites before stack scheduling.
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
    BlockId, Function, InstId, InstKind, Instruction, Module, Terminator, Value, ValueId,
    analysis::{CfgInfo, Liveness},
    pass::{MirPass, ModuleAnalyses, run_function_pass},
};
use smallvec::SmallVec;
use solar_config::OptimizationMode;
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

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        let needs_stack_memory =
            module.functions.iter().any(|func| func.attributes.unrestricted_memory);
        run_function_pass(module, analyses, |func, _| {
            let changed = needs_stack_memory && Self::sink_resident_offsets(func);
            changed
                | if matches!(gcx.sess.opts.optimization, OptimizationMode::None) {
                    Self::hoist_bitwise_reductions(func)
                } else {
                    Self::run_on_function(func)
                }
        })
    }
}

impl EvmInstSchedule {
    fn run_on_function(func: &mut Function) -> bool {
        let mut changed = false;
        let block_ids = func.blocks.indices();
        let shared_results = Self::shared_results(func);
        let mut scratch = ScheduleScratch::new(func.num_insts());

        for block_id in block_ids {
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

        changed | Self::hoist_bitwise_reductions(func)
    }
}

impl EvmInstSchedule {
    /// Rebuilds constant offsets where their base already occupies an incoming stack word.
    fn sink_resident_offsets(func: &mut Function) -> bool {
        let liveness = Liveness::compute(func);
        let mut candidates = Vec::new();
        for (block, data) in func.blocks.iter_enumerated() {
            for &id in &data.instructions {
                let inst = func.inst(id);
                if let InstKind::Add(a, b) = inst.kind
                    && inst
                        .metadata
                        .effect()
                        .is_none_or(|effect| effect == crate::mir::EffectKind::Pure)
                    && let Some(value) = func.inst_result_value(id)
                {
                    let base = if func.value_u256(a).is_some() {
                        b
                    } else if func.value_u256(b).is_some() {
                        a
                    } else {
                        continue;
                    };
                    candidates.push((block, id, value, base, inst.clone()));
                }
            }
        }
        let mut changed = false;
        for (definition, id, value, base, original) in candidates {
            if func.inst(id).kind != original.kind {
                continue;
            }
            let mut uses = Vec::new();
            let mut eligible = true;
            let mut pressured = false;
            for (block, data) in func.blocks.iter_enumerated() {
                let mut first = None;
                for (index, &inst) in data.instructions.iter().enumerate() {
                    if func.inst(inst).kind.operands().contains(&value) {
                        if matches!(func.inst(inst).kind, InstKind::Phi(_)) {
                            eligible = false;
                        }
                        first.get_or_insert(index);
                    }
                }
                if data.terminator.as_ref().is_some_and(|term| term.operands().contains(&value)) {
                    first.get_or_insert(data.instructions.len());
                }
                if let Some(first) = first
                    && block != definition
                {
                    eligible &= liveness.live_in(block).contains(base);
                    pressured |= liveness.live_in(block).count() > 16;
                    uses.push((block, first));
                }
            }
            if !eligible || !pressured || uses.is_empty() {
                continue;
            }
            for (block, first) in uses {
                let mut cloned = original.clone();
                cloned.set_result(None);
                // local = add base, offset
                // use(local)
                let (cloned, local) = func.alloc_value_inst(cloned);
                let instructions = func.blocks[block].instructions.clone();
                for inst in instructions {
                    func.inst_mut(inst).kind.visit_operands_mut(|operand| {
                        if *operand == value {
                            *operand = local;
                        }
                    });
                }
                if let Some(term) = &mut func.blocks[block].terminator {
                    term.visit_operands_mut(|operand| {
                        if *operand == value {
                            *operand = local;
                        }
                    });
                }
                func.blocks[block].instructions.insert(first, cloned);
            }
            if !func.blocks[definition]
                .instructions
                .iter()
                .any(|&inst| func.inst(inst).kind.operands().contains(&value))
                && !func.blocks[definition]
                    .terminator
                    .as_ref()
                    .is_some_and(|term| term.operands().contains(&value))
            {
                // Remove the offset whose uses now compute their own local value.
                func.blocks[definition].instructions.retain(|&inst| inst != id);
            }
            changed = true;
        }
        changed
    }

    /// Carries a single reduction result across a call instead of its inaccessible inputs.
    fn hoist_bitwise_reductions(func: &mut Function) -> bool {
        if !func.attributes.unrestricted_memory {
            return false;
        }
        let cfg = CfgInfo::new(func);
        let shared = Self::shared_results(func);
        let mut locations = index_vec![None; func.num_insts()];
        for (block, data) in func.blocks.iter_enumerated() {
            for &inst in &data.instructions {
                locations[inst] = Some(block);
            }
        }
        let mut changed = false;
        for block in func.blocks.indices() {
            if cfg.cyclic_blocks().contains(block) {
                continue;
            }
            let candidates = func.blocks[block].instructions.clone();
            for root in candidates {
                if locations[root] != Some(block) {
                    continue;
                }
                let mut tree = Vec::new();
                let mut leaves = Vec::new();
                if !Self::collect_bitwise_reduction(
                    func,
                    root,
                    block,
                    &locations,
                    &shared,
                    &mut tree,
                    &mut leaves,
                ) || leaves.len() <= 16
                {
                    continue;
                }
                let Some(source) = locations[leaves[0]] else { continue };
                if source == block
                    || cfg.cyclic_blocks().contains(source)
                    || !cfg.dominators().dominates(source, block)
                    || leaves.iter().any(|&leaf| locations[leaf] != Some(source))
                {
                    continue;
                }
                let source_insts = &func.blocks[source].instructions;
                let Some(last_gas) = source_insts
                    .iter()
                    .rposition(|&id| matches!(func.inst(id).kind, InstKind::Gas))
                else {
                    continue;
                };
                let Some(call) =
                    source_insts.iter().enumerate().skip(last_gas + 1).find_map(|(index, &id)| {
                        matches!(
                            func.inst(id).kind.effect_kind(),
                            crate::mir::EffectKind::ExternalCall
                        )
                        .then_some(index)
                    })
                else {
                    continue;
                };

                // gas0, ..., gasN; call; and-tree(gas0, ..., gasN)
                // gas0, ..., gasN; and-tree(gas0, ..., gasN); call
                let mut moved = DenseBitSet::new_empty(func.num_insts());
                for &id in &tree {
                    moved.insert(id);
                }
                func.blocks[block].instructions.retain(|&id| !moved.contains(id));
                for &id in &tree {
                    locations[id] = Some(source);
                }
                func.blocks[source].instructions.splice(call..call, tree);
                changed = true;
            }
        }
        changed
    }

    fn collect_bitwise_reduction(
        func: &Function,
        root: InstId,
        block: BlockId,
        locations: &IndexVec<InstId, Option<BlockId>>,
        shared: &DenseBitSet<InstId>,
        tree: &mut Vec<InstId>,
        leaves: &mut Vec<InstId>,
    ) -> bool {
        let mut seen = DenseBitSet::new_empty(func.num_insts());
        let mut work = vec![(root, false)];
        while let Some((inst, emit)) = work.pop() {
            if emit {
                tree.push(inst);
                continue;
            }
            if !seen.insert(inst) || (inst != root && shared.contains(inst)) {
                return false;
            }
            let instruction = func.inst(inst);
            if matches!(instruction.kind, InstKind::Gas) {
                leaves.push(inst);
                continue;
            }
            if locations[inst] != Some(block)
                || !matches!(
                    instruction.kind,
                    InstKind::And(..) | InstKind::Or(..) | InstKind::Xor(..)
                )
                || instruction
                    .metadata
                    .effect()
                    .is_some_and(|effect| effect != crate::mir::EffectKind::Pure)
            {
                return false;
            }
            work.push((inst, true));
            for operand in instruction.kind.operands() {
                let Value::Inst(dependency) = func.value(operand) else { return false };
                work.push((*dependency, false));
            }
        }
        true
    }

    /// Whether an instruction may move among other read-only instructions in the same segment.
    fn is_movable(inst: &Instruction) -> bool {
        if matches!(inst.kind, InstKind::Phi(_) | InstKind::Gas | InstKind::MSize) {
            return false;
        }

        Self::is_movable_effect(inst.kind.effect_kind())
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
            for operand in func.inst(inst_id).kind.operands() {
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
