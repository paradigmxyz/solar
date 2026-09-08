//! Stack-resident phi planning for loops, branches, and live joins.

use super::super::super::{
    BlockId, DenseBitSet, Function, FunctionId, FxHashMap, FxHashSet, GlobalStackPlan, IndexVec,
    InstId, InstKind, Liveness, Loop, LoopAnalyzer, MAX_STACK_ACCESS, OptimizationMode,
    STACK_PHI_LAYOUT_LIMIT, SmallVec, Terminator, ValueId, index_vec,
    rematerializable_nullary_opcode,
};

#[derive(Clone, Default)]
pub(in crate::backend::evm::codegen) struct StackPhiPlan {
    pub(in crate::backend::evm::codegen) entries: FxHashMap<BlockId, Vec<ValueId>>,
    pub(in crate::backend::evm::codegen) edges: FxHashMap<BlockId, StackPhiEdge>,
    pub(in crate::backend::evm::codegen) branch_edges: FxHashMap<BlockId, StackPhiBranch>,
    pub(in crate::backend::evm::codegen) phi_edge_sources: FxHashMap<BlockId, Vec<ValueId>>,
}

#[derive(Clone, Debug)]
pub(in crate::backend::evm::codegen) struct StackPhiEdge {
    pub(in crate::backend::evm::codegen) sources: Vec<ValueId>,
    pub(in crate::backend::evm::codegen) results: Vec<ValueId>,
}

#[derive(Clone, Debug)]
pub(in crate::backend::evm::codegen) struct StackPhiBranch {
    pub(in crate::backend::evm::codegen) then_edge: StackPhiEdge,
    pub(in crate::backend::evm::codegen) else_edge: StackPhiEdge,
    pub(in crate::backend::evm::codegen) union: Vec<ValueId>,
}

/// Returns true when a planned entry layout hands `value` to `block` on the stack, so the block
/// reads it from there and it needs no spill slot.
pub(in crate::backend::evm::codegen) fn planned_entry_carries(
    stack_phi_plan: &StackPhiPlan,
    global_stack_plan: &GlobalStackPlan,
    block: BlockId,
    value: ValueId,
) -> bool {
    stack_phi_plan.entries.get(&block).is_some_and(|entry| entry.contains(&value))
        || global_stack_plan.entry(block).is_some_and(|entry| entry.contains(&value))
}

struct BranchPhiShape {
    then_results: Vec<ValueId>,
    else_results: Vec<ValueId>,
    edges: Vec<(BlockId, StackPhiEdge, StackPhiEdge)>,
}

fn union_values(first: &[ValueId], second: &[ValueId]) -> Vec<ValueId> {
    let mut union = first.to_vec();
    let mut available = FxHashMap::default();
    for &value in first {
        *available.entry(value).or_insert(0usize) += 1;
    }
    for &value in second {
        let count = available.entry(value).or_default();
        if *count == 0 {
            union.push(value);
        } else {
            *count -= 1;
        }
    }
    union
}

impl StackPhiPlan {
    pub(in crate::backend::evm::codegen) fn analyze(
        func: &Function,
        liveness: &Liveness,
        cold_functions: &DenseBitSet<FunctionId>,
        optimization: OptimizationMode,
    ) -> Self {
        StackPhiPlanner::new(func, cold_functions, optimization).plan(liveness)
    }

    pub(in crate::backend::evm::codegen) fn edge_fits(
        edge: &StackPhiEdge,
        values: &[ValueId],
    ) -> bool {
        let source_additions = values.iter().filter(|value| !edge.sources.contains(value)).count();
        let result_additions = values.iter().filter(|value| !edge.results.contains(value)).count();
        edge.sources.len().saturating_add(source_additions) <= MAX_STACK_ACCESS
            && edge.results.len().saturating_add(result_additions) <= MAX_STACK_ACCESS
    }

    pub(in crate::backend::evm::codegen) fn merge_edge(
        edge: &mut StackPhiEdge,
        values: &[ValueId],
    ) {
        let source_additions: Vec<_> =
            values.iter().copied().filter(|value| !edge.sources.contains(value)).collect();
        let result_additions: Vec<_> =
            values.iter().copied().filter(|value| !edge.results.contains(value)).collect();
        edge.sources.extend(source_additions);
        edge.results.extend(result_additions);
    }

    pub(in crate::backend::evm::codegen) fn edge_sources(
        &self,
    ) -> FxHashMap<BlockId, Vec<ValueId>> {
        let mut sources: FxHashMap<BlockId, Vec<ValueId>> = self
            .edges
            .iter()
            .map(|(&block, edge)| (block, edge.sources.clone()))
            .chain(self.branch_edges.iter().map(|(&block, branch)| (block, branch.union.clone())))
            .collect();
        for (&block, phi_sources) in &self.phi_edge_sources {
            sources.insert(block, phi_sources.clone());
        }
        sources
    }

    /// Extends planned phi edges with the resident argument prefix required by
    /// the same target. Phi values stay nearest the top, preserving the
    /// existing loop/join schedule, while invariant arguments ride below them.
    pub(in crate::backend::evm::codegen) fn merge_resident(
        &mut self,
        func: &Function,
        resident: &GlobalStackPlan,
    ) -> bool {
        for (&block, entry) in &self.entries {
            if let Some(values) = resident.entry(block) {
                let additions = values.iter().filter(|value| !entry.contains(value)).count();
                if entry.len().saturating_add(additions) > MAX_STACK_ACCESS {
                    return false;
                }
            }
        }
        for (&pred, edge) in &self.edges {
            let Some(term) = func.blocks[pred].terminator.as_ref() else { return false };
            if let Some(values) = resident.edge_layout(func, term)
                && !Self::edge_fits(edge, values)
            {
                return false;
            }
        }
        for (&pred, branch) in &self.branch_edges {
            let Some(term) = func.blocks[pred].terminator.as_ref() else { return false };
            let (then_values, else_values) =
                if let Some((then, else_)) = resident.branch_layouts(term) {
                    (then, else_)
                } else if let Some(values) = resident.edge_layout(func, term) {
                    (values, values)
                } else {
                    continue;
                };
            for (edge, values) in
                [(&branch.then_edge, then_values), (&branch.else_edge, else_values)]
            {
                if !Self::edge_fits(edge, values) {
                    return false;
                }
            }
        }

        for (&block, entry) in &mut self.entries {
            if let Some(values) = resident.entry(block) {
                let additions: Vec<_> =
                    values.iter().copied().filter(|value| !entry.contains(value)).collect();
                entry.extend(additions);
            }
        }

        for (&pred, edge) in &mut self.edges {
            let Some(term) = func.blocks[pred].terminator.as_ref() else { return false };
            if let Some(values) = resident.edge_layout(func, term) {
                Self::merge_edge(edge, values);
            }
        }
        for (&pred, branch) in &mut self.branch_edges {
            let Some(term) = func.blocks[pred].terminator.as_ref() else { return false };
            let (then_values, else_values) =
                if let Some((then, else_)) = resident.branch_layouts(term) {
                    (then, else_)
                } else if let Some(values) = resident.edge_layout(func, term) {
                    (values, values)
                } else {
                    continue;
                };
            for (edge, values) in
                [(&mut branch.then_edge, then_values), (&mut branch.else_edge, else_values)]
            {
                Self::merge_edge(edge, values);
            }
            branch.union = union_values(&branch.then_edge.sources, &branch.else_edge.sources);
        }
        true
    }
}

struct StackPhiPlanner<'a> {
    optimization: OptimizationMode,
    func: &'a Function,
    loops: Vec<Loop>,
    header_results: FxHashMap<BlockId, Vec<ValueId>>,
    definitions: IndexVec<ValueId, Option<BlockId>>,
    /// Functions whose every exit aborts; a tail call into one never returns to the words a
    /// carried stack leaves beneath it.
    cold_functions: &'a DenseBitSet<FunctionId>,
}

/// Longest entry layout `plan_live_joins` carries into a block.
const LIVE_JOIN_LAYOUT_LIMIT: usize = 12;

/// Most forward-and-backward rounds `plan_live_joins` spends converging its layouts.
const LIVE_JOIN_ROUNDS: usize = 64;

/// What one `plan_live_joins` run knows about the function before any layout exists: the
/// blocks it plans and the per-block facts every round reads and none changes.
struct LiveJoinFacts {
    /// The joins being planned.
    is_join: FxHashSet<BlockId>,
    /// Sibling arms of planned branches, with the branch and the join it enters.
    arms: FxHashMap<BlockId, (BlockId, BlockId)>,
    /// Branches that enter a planned join.
    planned_branches: DenseBitSet<BlockId>,
    /// The latches of every loop header among the joins.
    back_edges: FxHashMap<BlockId, Vec<BlockId>>,
    /// The non-phi operands a block reads that are live into it.
    own_uses: IndexVec<BlockId, Vec<ValueId>>,
    /// The values live both into and out of a block: what it may carry onward.
    live_through: IndexVec<BlockId, DenseBitSet<ValueId>>,
    /// The carriable results a block defines after its last internal call and keeps live at
    /// its exit, top of the stack first.
    defs: IndexVec<BlockId, Vec<ValueId>>,
    /// Blocks with an internal call, which drains whatever they entered with.
    has_call: DenseBitSet<BlockId>,
    /// The headers of the loops containing each block.
    loop_headers_of: IndexVec<BlockId, SmallVec<[BlockId; 2]>>,
    /// Blocks a branch carries its stack into: a single-predecessor block without phis or a
    /// junk-tolerant terminal.
    carries_arm: DenseBitSet<BlockId>,
    /// Every value a planned join's own instructions read.
    join_uses: FxHashMap<BlockId, FxHashSet<ValueId>>,
    /// A planned join's phi results, in instruction order.
    join_phis: FxHashMap<BlockId, Vec<ValueId>>,
}

/// The converging state of one `plan_live_joins` run: the layouts so far, what every block
/// carries out under them, and what every block wants carried in.
struct LiveJoinState {
    layouts: FxHashMap<BlockId, Vec<ValueId>>,
    resident_out: FxHashMap<BlockId, Vec<ValueId>>,
    wanted: IndexVec<BlockId, DenseBitSet<ValueId>>,
    /// The next `wanted` set under construction, swapped in when it differs.
    scratch: DenseBitSet<ValueId>,
    /// A successor's wants masked to what the block carries through.
    mask: DenseBitSet<ValueId>,
}

impl LiveJoinState {
    fn new(num_blocks: usize, num_values: usize) -> Self {
        Self {
            layouts: FxHashMap::default(),
            resident_out: FxHashMap::default(),
            wanted: IndexVec::from_vec(vec![DenseBitSet::new_empty(num_values); num_blocks]),
            scratch: DenseBitSet::new_empty(num_values),
            mask: DenseBitSet::new_empty(num_values),
        }
    }
}

impl<'a> StackPhiPlanner<'a> {
    fn new(
        func: &'a Function,
        cold_functions: &'a DenseBitSet<FunctionId>,
        optimization: OptimizationMode,
    ) -> Self {
        let mut loop_analyzer = LoopAnalyzer::new();
        let loop_info = loop_analyzer.analyze(func);
        let loops = loop_info.all_loops().cloned().collect();

        let mut definitions = index_vec![None; func.num_values()];
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                if let Some(value) = func.inst_result_value(inst_id) {
                    definitions[value] = Some(block_id);
                }
            }
        }
        let mut planner = Self {
            optimization,
            func,
            loops,
            header_results: FxHashMap::default(),
            definitions,
            cold_functions,
        };
        planner.collect_header_results();
        planner
    }

    fn plan(&self, liveness: &Liveness) -> StackPhiPlan {
        let mut plan = StackPhiPlan::default();
        self.plan_live_joins(liveness, &mut plan);
        for loop_info in &self.loops {
            self.plan_loop(loop_info, liveness, &mut plan);
        }
        self.plan_branch_phi_joins(&mut plan);
        for block in self.func.blocks.indices() {
            self.plan_join(block, &mut plan);
        }
        plan
    }

    /// Plans entry layouts for the acyclic joins the phi planners left alone. A join keeps its
    /// phi results and the live-in values that are already on the stack at the exit of every
    /// predecessor, so a value defined before a diamond crosses it without a spill store and a
    /// reload on the far side, and no edge has to load anything it did not have. Residency is a
    /// static fixpoint over the planned layouts: a carried value stays resident through
    /// single-predecessor chains and planned joins until a call drains the stack. The sibling
    /// arm of a planned branch gets a layout of its own; a sibling that only aborts is entered
    /// with the carried words beneath it, and any other sibling starts from an empty stack.
    fn plan_live_joins(&self, liveness: &Liveness, plan: &mut StackPhiPlan) {
        let func = self.func;
        let mut loop_headers = DenseBitSet::new_empty(func.blocks.len());
        for loop_info in &self.loops {
            loop_headers.insert(loop_info.header);
        }
        let mut joins = Vec::new();
        for (block_id, block) in func.blocks.iter_enumerated() {
            if block.predecessors.len() < 2
                || plan.entries.contains_key(&block_id)
                || GlobalStackPlan::is_terminal_block(func, block_id)
            {
                continue;
            }
            let preds_ok = block.predecessors.iter().all(|&pred| {
                !plan.edges.contains_key(&pred)
                    && !plan.branch_edges.contains_key(&pred)
                    && match func.blocks[pred].terminator.as_ref() {
                        Some(Terminator::Jump(_)) => true,
                        Some(Terminator::Branch { then_block, else_block, .. }) => {
                            then_block != else_block
                        }
                        _ => false,
                    }
            });
            let phis = self.phi_insts(block);
            let phi_source_is_live_in = block.predecessors.iter().any(|&pred| {
                self.phi_sources_for_pred(&phis, pred).is_some_and(|sources| {
                    sources.iter().any(|&source| liveness.live_in(block_id).contains(source))
                })
            });
            if preds_ok
                && !phi_source_is_live_in
                && phis.len() <= LIVE_JOIN_LAYOUT_LIMIT
                && self.phi_result_values(&phis).is_some()
            {
                joins.push(block_id);
            }
        }
        if joins.is_empty() {
            return;
        }
        let is_join = joins.iter().copied().collect::<FxHashSet<_>>();

        // Branches into a planned join carry into their sibling arm too when only that branch
        // enters it.
        // A sibling arm receives exactly the words its branch sends to the join, so the branch
        // reaches it straight from the `JUMPI` with no cleanup of its own.
        let mut arms = FxHashMap::default();
        let mut planned_branches = DenseBitSet::new_empty(func.blocks.len());
        for &join in &joins {
            for &pred in func.blocks[join].predecessors.iter() {
                let Some(Terminator::Branch { then_block, else_block, .. }) =
                    func.blocks[pred].terminator.as_ref()
                else {
                    continue;
                };
                planned_branches.insert(pred);
                for arm in [*then_block, *else_block] {
                    if arm != join
                        && !is_join.contains(&arm)
                        && !loop_headers.contains(arm)
                        && !plan.entries.contains_key(&arm)
                        && func.blocks[arm].predecessors.as_slice() == [pred]
                        && self.phi_insts(&func.blocks[arm]).is_empty()
                        && !self.junk_tolerant_terminal(liveness, arm)
                    {
                        arms.insert(arm, (pred, join));
                    }
                }
            }
        }

        // A loop invariant is resident at the latch only once the header carries it, so a
        // header's layout is seeded from its forward predecessors alone; a word that then
        // fails to reach a latch is banned and the fixpoint reruns.
        let mut back_edges: FxHashMap<BlockId, Vec<BlockId>> = FxHashMap::default();
        for loop_info in &self.loops {
            back_edges
                .entry(loop_info.header)
                .or_default()
                .extend(loop_info.back_edges.iter().copied());
        }
        // Two phases: an optimistic one where a successor asks for everything it wants, so a
        // word can enter a chain of layouts that each depend on the next, then a precise one
        // where a successor asks only for its converged layout, pruning what nothing keeps.
        // A round sweeps the blocks forward, refreshing what each carries out and the layouts
        // that residency feeds, then backward, refreshing what each wants and the layouts
        // those wants prune. Every refresh reads the newest neighbors, so a word crosses a
        // whole chain of joins in one round rather than one join per round.
        let facts =
            self.live_join_facts(liveness, &joins, is_join, arms, planned_branches, back_edges);
        let order = func.blocks.indices().collect::<Vec<_>>();
        let mut banned: FxHashMap<BlockId, FxHashSet<ValueId>> = FxHashMap::default();
        let mut state = LiveJoinState::new(func.blocks.len(), func.num_values());
        let mut precise = false;
        for _ in 0..LIVE_JOIN_ROUNDS {
            let mut changed = false;
            for &block_id in &order {
                changed |= self.refresh_live_join_layout(
                    liveness, block_id, &banned, precise, &facts, &mut state,
                );
                changed |= self.refresh_resident_out(liveness, plan, block_id, &facts, &mut state);
            }
            for &block_id in order.iter().rev() {
                changed |= self.refresh_wanted(plan, block_id, precise, &facts, &mut state);
                changed |= self.refresh_live_join_layout(
                    liveness, block_id, &banned, precise, &facts, &mut state,
                );
            }
            if changed {
                continue;
            }
            // Under the converged layouts, every latch must deliver the header's words.
            for (&header, latches) in &facts.back_edges {
                let Some(layout) = state.layouts.get(&header) else { continue };
                let phis = facts.join_phis.get(&header).map(Vec::as_slice).unwrap_or_default();
                for &value in layout {
                    if !phis.contains(&value)
                        && !latches.iter().all(|latch| {
                            state.resident_out.get(latch).is_some_and(|list| list.contains(&value))
                        })
                        && banned.entry(header).or_default().insert(value)
                    {
                        changed = true;
                    }
                }
            }
            if changed {
                continue;
            }
            if precise {
                break;
            }
            precise = true;
        }
        let mut layouts = state.layouts;
        layouts.retain(|_, layout| !layout.is_empty());
        let mut join_layouts = layouts.clone();
        join_layouts.retain(|block, _| facts.is_join.contains(block));
        let mut arm_layouts = layouts;
        arm_layouts.retain(|block, _| facts.arms.contains_key(block));
        if join_layouts.is_empty() {
            return;
        }

        // A predecessor that cannot produce a join's layout drops that join; dropping only
        // shrinks the plan, so this converges.
        loop {
            let mut edges = FxHashMap::default();
            let mut branches = FxHashMap::default();
            let mut dropped = Vec::new();
            let mut planned = join_layouts.keys().copied().collect::<Vec<_>>();
            planned.sort_unstable_by_key(|block| block.index());
            'joins: for join in planned {
                let layout = &join_layouts[&join];
                for &pred in func.blocks[join].predecessors.iter() {
                    match func.blocks[pred].terminator.as_ref() {
                        Some(Terminator::Jump(_)) => {
                            let Some(sources) = self.layout_sources(join, layout, pred) else {
                                dropped.push(join);
                                continue 'joins;
                            };
                            edges.insert(pred, StackPhiEdge { sources, results: layout.clone() });
                        }
                        Some(Terminator::Branch { then_block, else_block, .. }) => {
                            if branches.contains_key(&pred) {
                                continue;
                            }
                            let mut planned_arms = [None, None];
                            for (slot, &arm) in
                                planned_arms.iter_mut().zip(&[*then_block, *else_block])
                            {
                                let Some(edge) =
                                    self.arm_edge(liveness, pred, arm, &join_layouts, &arm_layouts)
                                else {
                                    dropped.push(join);
                                    continue 'joins;
                                };
                                *slot = edge;
                            }
                            let [then_edge, else_edge] = planned_arms;
                            let junk = |other: &Option<StackPhiEdge>| {
                                let sources = other
                                    .as_ref()
                                    .map(|edge| edge.sources.clone())
                                    .unwrap_or_default();
                                StackPhiEdge { results: sources.clone(), sources }
                            };
                            let then_edge = then_edge.unwrap_or_else(|| junk(&else_edge));
                            let else_edge =
                                else_edge.unwrap_or_else(|| junk(&Some(then_edge.clone())));
                            // An arm holding every word of the other is the identity edge; its
                            // order is the one the branch shuffles to, so keep it verbatim.
                            let covers = |outer: &[ValueId], inner: &[ValueId]| {
                                inner.iter().all(|value| outer.contains(value))
                            };
                            let union = if covers(&else_edge.sources, &then_edge.sources) {
                                else_edge.sources.clone()
                            } else if covers(&then_edge.sources, &else_edge.sources) {
                                then_edge.sources.clone()
                            } else {
                                union_values(&then_edge.sources, &else_edge.sources)
                            };
                            if union.is_empty() || union.len() > MAX_STACK_ACCESS {
                                dropped.push(join);
                                continue 'joins;
                            }
                            branches.insert(pred, StackPhiBranch { then_edge, else_edge, union });
                        }
                        _ => {
                            dropped.push(join);
                            continue 'joins;
                        }
                    }
                }
            }
            if !dropped.is_empty() {
                for block in dropped {
                    join_layouts.remove(&block);
                }
                if join_layouts.is_empty() {
                    return;
                }
                continue;
            }

            for (join, layout) in join_layouts {
                plan.entries.insert(join, layout);
            }
            for &pred in branches.keys() {
                let Some(Terminator::Branch { then_block, else_block, .. }) =
                    func.blocks[pred].terminator.as_ref()
                else {
                    continue;
                };
                for arm in [*then_block, *else_block] {
                    if let Some(layout) = arm_layouts.get(&arm) {
                        plan.entries.insert(arm, layout.clone());
                    }
                }
            }
            for (pred, edge) in edges {
                plan.phi_edge_sources.insert(pred, Self::phi_sources_of(&edge));
                plan.edges.insert(pred, edge);
            }
            for (pred, branch) in branches {
                let mut sources = Self::phi_sources_of(&branch.then_edge);
                for value in Self::phi_sources_of(&branch.else_edge) {
                    if !sources.contains(&value) {
                        sources.push(value);
                    }
                }
                plan.phi_edge_sources.insert(pred, sources);
                plan.branch_edges.insert(pred, branch);
            }
            return;
        }
    }

    /// Gathers what every round of `plan_live_joins` reads and none changes.
    fn live_join_facts(
        &self,
        liveness: &Liveness,
        joins: &[BlockId],
        is_join: FxHashSet<BlockId>,
        arms: FxHashMap<BlockId, (BlockId, BlockId)>,
        planned_branches: DenseBitSet<BlockId>,
        back_edges: FxHashMap<BlockId, Vec<BlockId>>,
    ) -> LiveJoinFacts {
        let func = self.func;
        let count = func.blocks.len();
        let num_values = func.num_values();
        let mut own_uses = IndexVec::with_capacity(count);
        let mut live_through = IndexVec::with_capacity(count);
        let mut defs = IndexVec::with_capacity(count);
        let mut has_call = DenseBitSet::new_empty(count);
        let mut carries_arm = DenseBitSet::new_empty(count);
        for (block_id, block) in func.blocks.iter_enumerated() {
            let live_in = liveness.live_in(block_id);
            let live_out = liveness.live_out(block_id);
            let mut uses = Vec::new();
            let mut kept = Vec::new();
            for &inst in &block.instructions {
                let kind = &func.inst(inst).kind;
                if matches!(kind, InstKind::ICall { .. }) {
                    has_call.insert(block_id);
                    kept.clear();
                }
                if !matches!(kind, InstKind::Phi(_)) {
                    uses.extend(
                        kind.operands().into_iter().filter(|value| live_in.contains(*value)),
                    );
                }
                if let Some(result) = func.inst_result_value(inst)
                    && self.carriable(result)
                    && live_out.contains(result)
                {
                    kept.push(result);
                }
            }
            if let Some(term) = &block.terminator {
                uses.extend(term.operands().into_iter().filter(|value| live_in.contains(*value)));
            }
            uses.sort_unstable_by_key(|value| value.index());
            uses.dedup();
            let mut through = DenseBitSet::new_empty(num_values);
            for value in live_in.iter().filter(|&value| live_out.contains(value)) {
                through.insert(value);
            }
            live_through.push(through);
            // Layouts list the top of the stack first; a new definition lands on top.
            kept.reverse();
            own_uses.push(uses);
            defs.push(kept);
            if block.predecessors.len() == 1 && self.phi_insts(block).is_empty()
                || self.junk_tolerant_terminal(liveness, block_id)
            {
                carries_arm.insert(block_id);
            }
        }
        let mut loop_headers_of = IndexVec::from_vec(vec![SmallVec::new(); count]);
        for loop_info in &self.loops {
            for block in loop_info.blocks.iter() {
                loop_headers_of[block].push(loop_info.header);
            }
        }
        let join_uses = joins.iter().map(|&join| (join, self.block_uses(join))).collect();
        let join_phis = joins
            .iter()
            .map(|&join| {
                let phis = self.phi_insts(&func.blocks[join]);
                (join, self.phi_result_values(&phis).unwrap_or_default())
            })
            .collect();
        LiveJoinFacts {
            is_join,
            arms,
            planned_branches,
            back_edges,
            own_uses,
            live_through,
            defs,
            has_call,
            loop_headers_of,
            carries_arm,
            join_uses,
            join_phis,
        }
    }

    /// Refreshes the layout of a planned join or sibling arm from the newest residency and
    /// wants; returns whether it changed.
    fn refresh_live_join_layout(
        &self,
        liveness: &Liveness,
        block_id: BlockId,
        banned: &FxHashMap<BlockId, FxHashSet<ValueId>>,
        precise: bool,
        facts: &LiveJoinFacts,
        state: &mut LiveJoinState,
    ) -> bool {
        let func = self.func;
        let layout = if facts.is_join.contains(&block_id) {
            let join = block_id;
            let block = &func.blocks[join];
            let live_in = liveness.live_in(join);
            let latches = facts.back_edges.get(&join).map(Vec::as_slice).unwrap_or(&[]);
            let forward = block.predecessors.iter().copied().filter(|pred| !latches.contains(pred));
            // The first forward predecessor's stack order is the layout order, so that edge
            // needs no shuffle and the others usually little.
            let Some(first) = forward.clone().next() else { return false };
            // A wide join shuffles every predecessor into one order; a word the join only
            // passes on rarely pays that there. A loop header is different: its latches
            // return with the header's own order, so a word riding around the loop
            // shuffles nowhere.
            let wide = block.predecessors.len() > 2 && !facts.back_edges.contains_key(&join);
            let used_here = &facts.join_uses[&join];
            let wanted = &state.wanted[join];
            let mut carried = state
                .resident_out
                .get(&first)
                .into_iter()
                .flatten()
                .copied()
                .filter(|&value| {
                    live_in.contains(value)
                        && self.carriable(value)
                        && !banned.get(&join).is_some_and(|set| set.contains(&value))
                        && wanted.contains(value)
                        && (!precise || !wide || used_here.contains(&value))
                        && forward.clone().all(|pred| {
                            state.resident_out.get(&pred).is_some_and(|list| list.contains(&value))
                        })
                })
                .collect::<Vec<_>>();
            // Phi sources are the newest words of a predecessor, so the results ride on top.
            let mut phis = facts.join_phis[&join].clone();
            carried.truncate(LIVE_JOIN_LAYOUT_LIMIT - phis.len());
            phis.extend(carried);
            phis
        } else if let Some(&(pred, join)) = facts.arms.get(&block_id) {
            let arm = block_id;
            // The join's words plus whatever else the arm reads that the branch already
            // holds, in the order the predecessor is expected to hold them: the arm edge is
            // the branch's identity edge, so this order is what the branch shuffles to, and
            // matching the resident order keeps that shuffle empty on every execution. The
            // join edge reorders on its own path only.
            let mut sources = state
                .layouts
                .get(&join)
                .and_then(|layout| self.layout_sources(join, layout, pred))
                .unwrap_or_default();
            let live_in = liveness.live_in(arm);
            // branch; arm-local uses; join-only immediates on the join edge
            if self.optimization == OptimizationMode::Gas {
                sources.retain(|&value| {
                    !matches!(self.func.value(value), crate::mir::Value::Immediate(_))
                        || live_in.contains(value)
                });
            }
            let resident = state.resident_out.get(&pred).map(Vec::as_slice).unwrap_or_default();
            let wanted = &state.wanted[arm];
            let mut carried = resident
                .iter()
                .copied()
                .filter(|&value| {
                    sources.contains(&value)
                        || (live_in.contains(value)
                            && self.carriable(value)
                            && wanted.contains(value))
                })
                .collect::<Vec<_>>();
            // A two-word layout needs only one swap to put the condition above its reload.
            let reload_on_top = carried.len() == 1
                && sources.iter().filter(|value| !carried.contains(value)).count() == 1;
            for &value in &sources {
                if !carried.contains(&value) {
                    // [resident]; push value -> [value, resident]
                    if reload_on_top {
                        carried.insert(0, value);
                    } else {
                        carried.push(value);
                    }
                }
            }
            carried.truncate(LIVE_JOIN_LAYOUT_LIMIT.max(sources.len()));
            carried
        } else {
            return false;
        };
        if state.layouts.get(&block_id) == Some(&layout) {
            return false;
        }
        state.layouts.insert(block_id, layout);
        true
    }

    /// Refreshes the values on the stack at a block's exit under the newest layouts: a block
    /// starts from its planned entry, from what a single predecessor carries across a jump or
    /// a fully preserved branch, or from nothing; keeps what it defines; and drains everything
    /// but its later definitions at an internal call. Returns whether the set changed.
    fn refresh_resident_out(
        &self,
        liveness: &Liveness,
        plan: &StackPhiPlan,
        block_id: BlockId,
        facts: &LiveJoinFacts,
        state: &mut LiveJoinState,
    ) -> bool {
        let func = self.func;
        let block = &func.blocks[block_id];
        let incoming: &[ValueId] = if let Some(layout) = state.layouts.get(&block_id) {
            layout
        } else if let Some(entry) = plan.entries.get(&block_id) {
            entry
        } else if let [pred] = block.predecessors.as_slice()
            && let Some(term) = func.blocks[*pred].terminator.as_ref()
        {
            let carried = match term {
                Terminator::Jump(_) => true,
                Terminator::Branch { then_block, else_block, .. } => {
                    !facts.planned_branches.contains(*pred)
                        && facts.carries_arm.contains(*then_block)
                        && facts.carries_arm.contains(*else_block)
                }
                _ => false,
            };
            if carried {
                state.resident_out.get(pred).map(Vec::as_slice).unwrap_or_default()
            } else {
                &[]
            }
        } else {
            &[]
        };
        let defs = &facts.defs[block_id];
        let mut resident = Vec::with_capacity(defs.len() + incoming.len());
        if facts.has_call.contains(block_id) {
            resident.extend_from_slice(defs);
        } else {
            let live_out = liveness.live_out(block_id);
            resident.extend(defs.iter().copied().filter(|def| !incoming.contains(def)));
            resident.extend(incoming.iter().copied().filter(|value| live_out.contains(*value)));
        }
        // An unplanned branch consumes its condition. Its spill home may still exist, but
        // the join planner must not count a reload as an already-resident stack word.
        if !facts.planned_branches.contains(block_id)
            && !plan.branch_edges.contains_key(&block_id)
            && let Some(Terminator::Branch { condition, .. }) = &block.terminator
        {
            resident.retain(|value| value != condition);
        }
        if state.resident_out.get(&block_id) == Some(&resident) {
            return false;
        }
        state.resident_out.insert(block_id, resident);
        true
    }

    /// Refreshes the live-in values a block reads itself or carries on to a successor that
    /// reads them, so a layout never pays a shuffle for a word nothing downstream consumes on
    /// the stack. Returns whether the set changed.
    fn refresh_wanted(
        &self,
        plan: &StackPhiPlan,
        block_id: BlockId,
        precise: bool,
        facts: &LiveJoinFacts,
        state: &mut LiveJoinState,
    ) -> bool {
        let func = self.func;
        let block = &func.blocks[block_id];
        let live_through = &facts.live_through[block_id];
        let LiveJoinState { layouts, wanted, scratch, mask, .. } = state;
        scratch.clear();
        for &value in &facts.own_uses[block_id] {
            scratch.insert(value);
        }
        let want = |scratch: &mut DenseBitSet<ValueId>, layout: &[ValueId]| {
            for &value in layout {
                if live_through.contains(value) {
                    scratch.insert(value);
                }
            }
        };
        if let Some(term) = &block.terminator {
            let carried_succs: SmallVec<[BlockId; 2]> = match term {
                Terminator::Jump(target) => {
                    let target_block = &func.blocks[*target];
                    (target_block.predecessors.len() == 1
                        || layouts.contains_key(target)
                        || plan.entries.contains_key(target))
                    .then_some(*target)
                    .into_iter()
                    .collect()
                }
                Terminator::Branch { then_block, else_block, .. } => {
                    let arms = [*then_block, *else_block];
                    if facts.planned_branches.contains(block_id) {
                        arms.into_iter()
                            .filter(|arm| {
                                layouts.contains_key(arm) || plan.entries.contains_key(arm)
                            })
                            .collect()
                    } else if arms.iter().all(|&arm| facts.carries_arm.contains(arm)) {
                        arms.into_iter().collect()
                    } else {
                        SmallVec::new()
                    }
                }
                _ => SmallVec::new(),
            };
            for succ in carried_succs {
                // A planned join carries exactly its layout; a chained block carries
                // whatever it wants. The optimistic phase asks for wants everywhere, a
                // loop header included: its layout can only hold what the preheader
                // and the latches deliver, and those only carry what the header asks
                // for, so asking with the layout alone never bootstraps a loop-carried
                // word. The latch check and the precise phase prune what it costs.
                if let Some(layout) = layouts.get(&succ).filter(|_| precise) {
                    want(scratch, layout);
                } else if let Some(entry) = plan.entries.get(&succ) {
                    want(scratch, entry);
                } else {
                    mask.clone_from(&wanted[succ]);
                    mask.intersect(live_through);
                    scratch.union(mask);
                }
            }
        }
        // A word the enclosing loop carries around is wanted everywhere inside it:
        // the joins on the way to a latch must carry it, or the latch cannot deliver
        // it back to the header and the header drops it.
        for header in &facts.loop_headers_of[block_id] {
            if let Some(layout) = layouts.get(header) {
                want(scratch, layout);
            }
        }
        if wanted[block_id] == *scratch {
            return false;
        }
        std::mem::swap(&mut wanted[block_id], scratch);
        true
    }

    /// The words of an edge that feed phis rather than ride through unchanged. Only these skip
    /// their definition-time store: a carried live-in keeps its store, so a later block that
    /// finds it dropped can still reload it.
    fn phi_sources_of(edge: &StackPhiEdge) -> Vec<ValueId> {
        edge.sources
            .iter()
            .zip(&edge.results)
            .filter(|(source, result)| source != result)
            .map(|(&source, _)| source)
            .collect()
    }

    /// The edge a planned branch `pred` uses for one of its arms: the arm's join layout, the
    /// arm's own layout when only `pred` enters it, `Some(None)` for an aborting arm that
    /// tolerates the carried words, and an empty edge for anything else. A planned branch owns
    /// the phi copies of both arms, so an arm with phis must be a planned join whose layout
    /// this predecessor can produce; otherwise the branch cannot be planned.
    fn arm_edge(
        &self,
        liveness: &Liveness,
        pred: BlockId,
        arm: BlockId,
        join_layouts: &FxHashMap<BlockId, Vec<ValueId>>,
        arm_layouts: &FxHashMap<BlockId, Vec<ValueId>>,
    ) -> Option<Option<StackPhiEdge>> {
        if let Some(layout) = join_layouts.get(&arm) {
            let sources = self.layout_sources(arm, layout, pred)?;
            return Some(Some(StackPhiEdge { sources, results: layout.clone() }));
        }
        if let Some(layout) = arm_layouts.get(&arm) {
            return Some(Some(StackPhiEdge { sources: layout.clone(), results: layout.clone() }));
        }
        if self.junk_tolerant_terminal(liveness, arm) {
            return Some(None);
        }
        if !self.phi_insts(&self.func.blocks[arm]).is_empty() {
            return None;
        }
        Some(Some(StackPhiEdge { sources: Vec::new(), results: Vec::new() }))
    }

    /// Whether a block may be entered with arbitrary words beneath the stack it expects: it
    /// reads no live-in value and aborts, directly or through a cold tail call.
    fn junk_tolerant_terminal(&self, liveness: &Liveness, block: BlockId) -> bool {
        liveness
            .live_in(block)
            .iter()
            .all(|value| matches!(self.func.value(value), crate::mir::Value::Immediate(_)))
            && match &self.func.blocks[block].terminator {
                Some(
                    Terminator::Revert { .. } | Terminator::RevertReturndata | Terminator::Invalid,
                ) => true,
                Some(Terminator::TailCall { function, .. }) => {
                    self.cold_functions.contains(*function)
                }
                _ => false,
            }
    }

    /// The values a block's own instructions and terminator read.
    fn block_uses(&self, block_id: BlockId) -> FxHashSet<ValueId> {
        let block = &self.func.blocks[block_id];
        let mut uses = FxHashSet::default();
        for &inst in &block.instructions {
            let kind = &self.func.inst(inst).kind;
            if !matches!(kind, InstKind::Phi(_)) {
                uses.extend(kind.operands());
            }
        }
        if let Some(term) = &block.terminator {
            uses.extend(term.operands());
        }
        uses
    }

    /// Whether a layout may carry `value`: an instruction result that is cheaper to keep than
    /// to recompute.
    fn carriable(&self, value: ValueId) -> bool {
        matches!(
            self.func.value(value),
            crate::mir::Value::Inst(inst)
                if rematerializable_nullary_opcode(&self.func.inst(*inst).kind).is_none()
        )
    }

    /// The words `pred` places for `block`'s layout: a phi result comes from its incoming
    /// value for `pred`, every other value is itself.
    fn layout_sources(
        &self,
        block_id: BlockId,
        layout: &[ValueId],
        pred: BlockId,
    ) -> Option<Vec<ValueId>> {
        let block = &self.func.blocks[block_id];
        let phi_insts = self.phi_insts(block);
        let results = self.phi_result_values(&phi_insts)?;
        let incoming = self.phi_sources_for_pred(&phi_insts, pred)?;
        layout
            .iter()
            .map(|&value| match results.iter().position(|&result| result == value) {
                Some(index) => Some(incoming[index]),
                None => Some(value),
            })
            .collect()
    }

    /// Plans branch edges whose two destinations are phi-only blocks in one loop-shaped CFG.
    /// Other conditional joins keep the conservative spill path.
    fn plan_branch_phi_joins(&self, plan: &mut StackPhiPlan) {
        for branch_id in self.func.blocks.indices() {
            let Some(loop_info) = self.loops.iter().find(|loop_info| {
                loop_info.blocks.contains(branch_id)
                    && loop_info.header != branch_id
                    && loop_info.blocks.iter().any(|block| {
                        matches!(
                            self.func.blocks[block].terminator,
                            Some(Terminator::Branch { .. })
                        )
                    })
            }) else {
                continue;
            };
            let Some(Terminator::Branch { then_block, else_block, .. }) =
                self.func.blocks[branch_id].terminator.as_ref()
            else {
                continue;
            };
            if plan.entries.contains_key(then_block) || plan.entries.contains_key(else_block) {
                continue;
            }
            let Some(shape) = self.branch_phi_shape(loop_info, *then_block, *else_block) else {
                continue;
            };
            if shape.edges.iter().any(|(pred, _, _)| {
                plan.edges.contains_key(pred) || plan.branch_edges.contains_key(pred)
            }) {
                continue;
            }

            plan.entries.insert(*then_block, shape.then_results.clone());
            plan.entries.insert(*else_block, shape.else_results.clone());
            for (pred, then_edge, else_edge) in shape.edges {
                let branch = StackPhiBranch {
                    union: union_values(&then_edge.sources, &else_edge.sources),
                    then_edge,
                    else_edge,
                };
                plan.branch_edges.insert(pred, branch);
            }
        }
    }

    fn phi_results_for_only_block(&self, block_id: BlockId) -> Option<Vec<ValueId>> {
        let block = &self.func.blocks[block_id];
        let phi_insts = self.phi_insts(block);
        if phi_insts.is_empty() || phi_insts.len() != block.instructions.len() {
            return None;
        }
        self.phi_result_values(&phi_insts)
    }

    fn phi_sources_for_block_pred(&self, block_id: BlockId, pred: BlockId) -> Option<Vec<ValueId>> {
        let phi_insts = self.phi_insts(&self.func.blocks[block_id]);
        self.phi_sources_for_pred(&phi_insts, pred)
    }

    fn branch_phi_shape(
        &self,
        loop_info: &Loop,
        then_block: BlockId,
        else_block: BlockId,
    ) -> Option<BranchPhiShape> {
        if then_block == else_block
            || loop_info.blocks.contains(then_block) == loop_info.blocks.contains(else_block)
        {
            return None;
        }
        let then_results = self.phi_results_for_only_block(then_block)?;
        let else_results = self.phi_results_for_only_block(else_block)?;
        if then_results.is_empty()
            || else_results.is_empty()
            || then_results.len() > STACK_PHI_LAYOUT_LIMIT
            || else_results.len() > STACK_PHI_LAYOUT_LIMIT
        {
            return None;
        }

        let mut predecessors = self.func.blocks[then_block].predecessors.clone();
        for &pred in &self.func.blocks[else_block].predecessors {
            if !predecessors.contains(&pred) {
                predecessors.push(pred);
            }
        }
        if predecessors.is_empty() {
            return None;
        }
        let mut edges = Vec::with_capacity(predecessors.len());
        for pred in predecessors {
            let Some(Terminator::Branch { then_block: pred_then, else_block: pred_else, .. }) =
                self.func.blocks[pred].terminator.as_ref()
            else {
                return None;
            };
            if !loop_info.blocks.contains(pred)
                || !((*pred_then == then_block && *pred_else == else_block)
                    || (*pred_then == else_block && *pred_else == then_block))
            {
                return None;
            }
            // Emission applies `then_edge` to the predecessor's own `then_block`, so a
            // predecessor whose arms are reversed relative to the first branch carries its
            // layouts in its own orientation.
            let (pred_then_results, pred_else_results) = if *pred_then == then_block {
                (then_results.clone(), else_results.clone())
            } else {
                (else_results.clone(), then_results.clone())
            };
            let then_sources = self.phi_sources_for_block_pred(*pred_then, pred)?;
            let else_sources = self.phi_sources_for_block_pred(*pred_else, pred)?;
            if then_sources.len() > MAX_STACK_ACCESS || else_sources.len() > MAX_STACK_ACCESS {
                return None;
            }
            edges.push((
                pred,
                StackPhiEdge { sources: then_sources, results: pred_then_results },
                StackPhiEdge { sources: else_sources, results: pred_else_results },
            ));
        }
        Some(BranchPhiShape { then_results, else_results, edges })
    }

    fn collect_header_results(&mut self) {
        for loop_info in &self.loops {
            let block = &self.func.blocks[loop_info.header];
            let phi_insts = self.phi_insts(block);
            if let Some(results) = self.phi_result_values(&phi_insts) {
                self.header_results.insert(loop_info.header, results);
            }
        }
    }

    fn plan_loop(&self, loop_info: &Loop, liveness: &Liveness, plan: &mut StackPhiPlan) {
        let Some(preheader) = loop_info.preheader else {
            return;
        };
        if loop_info.back_edges.is_empty() {
            return;
        }
        if !matches!(self.func.blocks[preheader].terminator, Some(Terminator::Jump(target)) if target == loop_info.header)
        {
            return;
        }
        if let [latch] = loop_info.back_edges.as_slice()
            && *latch == loop_info.header
            && self.plan_conditional_self_loop(loop_info, preheader, liveness, plan)
        {
            return;
        }
        if loop_info.back_edges.iter().any(|&latch| {
            !matches!(self.func.blocks[latch].terminator, Some(Terminator::Jump(target)) if target == loop_info.header)
        }) {
            return;
        }
        if plan.edges.contains_key(&preheader)
            || loop_info.back_edges.iter().any(|latch| plan.edges.contains_key(latch))
        {
            return;
        }
        let has_branching_body = loop_info.blocks.iter().any(|block_id| {
            block_id != loop_info.header
                && matches!(self.func.blocks[block_id].terminator, Some(Terminator::Branch { .. }))
        });
        let has_nested_loop = self.loops.iter().any(|other| {
            other.header != loop_info.header && loop_info.blocks.contains(other.header)
        });
        if has_branching_body && !self.can_plan_branching_loop(loop_info) {
            return;
        }
        let block = &self.func.blocks[loop_info.header];
        let phi_insts = self.phi_insts(block);
        if phi_insts.is_empty() || phi_insts.len() > STACK_PHI_LAYOUT_LIMIT {
            return;
        }

        let Some(results) = self.phi_result_values(&phi_insts) else {
            return;
        };
        if results.len() > STACK_PHI_LAYOUT_LIMIT {
            return;
        }

        let mut carry_through = self.carry_through_values(loop_info);
        if has_branching_body {
            if has_nested_loop {
                carry_through.clear();
            } else {
                self.extend_live_across_exits(loop_info, liveness, &mut carry_through);
            }
        } else {
            self.extend_live_through_values(loop_info, &mut carry_through);
        }
        if carry_through.len() + results.len() > STACK_PHI_LAYOUT_LIMIT {
            return;
        }
        let mut entry = carry_through.clone();
        entry.extend(results.iter().copied());

        let mut edges = Vec::with_capacity(loop_info.back_edges.len() + 1);
        for pred in std::iter::once(preheader).chain(loop_info.back_edges.iter().copied()) {
            let Some(phi_sources) = self.phi_sources_for_pred(&phi_insts, pred) else {
                return;
            };
            if pred != preheader
                && !has_branching_body
                && phi_sources.iter().any(|&source| {
                    self.is_phi_value(source)
                        && !results.contains(&source)
                        && !self.is_loop_header_phi(source)
                })
            {
                return;
            }
            let mut sources = carry_through.clone();
            sources.extend(phi_sources);
            debug_assert_eq!(sources.len(), entry.len());
            edges.push((pred, sources));
        }

        plan.entries.insert(loop_info.header, entry.clone());
        for (pred, sources) in edges {
            plan.edges.insert(pred, StackPhiEdge { sources, results: entry.clone() });
        }
    }

    fn plan_conditional_self_loop(
        &self,
        loop_info: &Loop,
        preheader: BlockId,
        liveness: &Liveness,
        plan: &mut StackPhiPlan,
    ) -> bool {
        let header = loop_info.header;
        if loop_info.blocks.iter().any(|block| block != header)
            || plan.entries.contains_key(&header)
            || plan.edges.contains_key(&preheader)
            || plan.edges.contains_key(&header)
            || plan.branch_edges.contains_key(&header)
            || !self.loop_instructions_are_stack_safe(loop_info)
        {
            return false;
        }
        let Some(Terminator::Branch { then_block, else_block, .. }) =
            self.func.blocks[header].terminator.as_ref()
        else {
            return false;
        };
        let (self_is_then, exit) = match (*then_block == header, *else_block == header) {
            (true, false) => (true, *else_block),
            (false, true) => (false, *then_block),
            _ => return false,
        };
        if loop_info.blocks.contains(exit)
            || self.func.blocks[exit].predecessors.as_slice() != [header]
            || !self.phi_insts(&self.func.blocks[exit]).is_empty()
            || plan.entries.contains_key(&exit)
        {
            return false;
        }

        let phi_insts = self.phi_insts(&self.func.blocks[header]);
        if phi_insts.is_empty() || phi_insts.len() > STACK_PHI_LAYOUT_LIMIT {
            return false;
        }
        let Some(results) = self.phi_result_values(&phi_insts) else { return false };
        let mut carry_through = self.carry_through_values(loop_info);
        if carry_through.is_empty() {
            // Keep enclosing-loop phis resident; other live-outs can still spill.
            self.extend_live_across_exits(loop_info, liveness, &mut carry_through);
        }
        let mut entry = carry_through.clone();
        entry.extend(results.iter().copied());
        if entry.len() > STACK_PHI_LAYOUT_LIMIT {
            return false;
        }

        let Some(initial_phi_sources) = self.phi_sources_for_pred(&phi_insts, preheader) else {
            return false;
        };
        let Some(backedge_phi_sources) = self.phi_sources_for_pred(&phi_insts, header) else {
            return false;
        };
        let mut initial_sources = carry_through.clone();
        initial_sources.extend(initial_phi_sources);
        let mut backedge_sources = carry_through;
        backedge_sources.extend(backedge_phi_sources);
        if initial_sources.len() != entry.len()
            || backedge_sources.len() != entry.len()
            || initial_sources.len() > MAX_STACK_ACCESS
            || backedge_sources.len() > MAX_STACK_ACCESS
        {
            return false;
        }

        let exit_values = entry
            .iter()
            .copied()
            .filter(|value| liveness.live_in(exit).contains(*value))
            .collect::<Vec<_>>();
        let backedge = StackPhiEdge { sources: backedge_sources, results: entry.clone() };
        let exit_edge = StackPhiEdge { sources: exit_values.clone(), results: exit_values.clone() };
        let (then_edge, else_edge) =
            if self_is_then { (backedge, exit_edge) } else { (exit_edge, backedge) };
        let union = union_values(&then_edge.sources, &else_edge.sources);
        if union.is_empty() || union.len() > MAX_STACK_ACCESS {
            return false;
        }

        plan.entries.insert(header, entry.clone());
        if !exit_values.is_empty() {
            plan.entries.insert(exit, exit_values);
        }
        plan.edges.insert(preheader, StackPhiEdge { sources: initial_sources, results: entry });
        plan.branch_edges.insert(header, StackPhiBranch { then_edge, else_edge, union });
        true
    }

    fn can_plan_branching_loop(&self, loop_info: &Loop) -> bool {
        let mut nesting_depth = 0;
        for other in &self.loops {
            if other.header == loop_info.header {
                continue;
            }
            nesting_depth += usize::from(other.blocks.contains(loop_info.header));
        }
        if nesting_depth != 0 && nesting_depth != 2 {
            return false;
        }
        if !self.loop_instructions_are_stack_safe(loop_info) {
            return false;
        }
        let branch_shapes_safe = loop_info
            .blocks
            .iter()
            .filter(|&block_id| block_id != loop_info.header)
            .all(|block_id| {
                let Some(Terminator::Branch { then_block, else_block, .. }) =
                    self.func.blocks[block_id].terminator.as_ref()
                else {
                    return true;
                };
                (loop_info.blocks.contains(*then_block) == loop_info.blocks.contains(*else_block))
                    || (!loop_info.blocks.contains(*then_block)
                        && self.is_noreturn_block(*then_block))
                    || (!loop_info.blocks.contains(*else_block)
                        && self.is_noreturn_block(*else_block))
                    || self.branch_phi_shape(loop_info, *then_block, *else_block).is_some()
            });
        branch_shapes_safe && self.phi_insts(&self.func.blocks[loop_info.header]).len() >= 2
    }

    fn loop_instructions_are_stack_safe(&self, loop_info: &Loop) -> bool {
        for block_id in loop_info.blocks.iter() {
            for &inst_id in &self.func.blocks[block_id].instructions {
                let kind = &self.func.inst(inst_id).kind;
                if !matches!(
                    kind,
                    InstKind::Add(_, _)
                        | InstKind::Sub(_, _)
                        | InstKind::Mul(_, _)
                        | InstKind::Div(_, _)
                        | InstKind::SDiv(_, _)
                        | InstKind::Mod(_, _)
                        | InstKind::SMod(_, _)
                        | InstKind::Exp(_, _)
                        | InstKind::AddMod(_, _, _)
                        | InstKind::MulMod(_, _, _)
                        | InstKind::And(_, _)
                        | InstKind::Or(_, _)
                        | InstKind::Xor(_, _)
                        | InstKind::Not(_)
                        | InstKind::Clz(_)
                        | InstKind::Shl(_, _)
                        | InstKind::Shr(_, _)
                        | InstKind::Sar(_, _)
                        | InstKind::Byte(_, _)
                        | InstKind::Lt(_, _)
                        | InstKind::Gt(_, _)
                        | InstKind::SLt(_, _)
                        | InstKind::SGt(_, _)
                        | InstKind::Eq(_, _)
                        | InstKind::IsZero(_)
                        | InstKind::MLoad(_)
                        | InstKind::MStore(_, _)
                        | InstKind::MStore8(_, _)
                        | InstKind::CalldataLoad(_)
                        | InstKind::CalldataSize
                        | InstKind::CalldataCopy(_, _, _)
                        | InstKind::MSize
                        | InstKind::Fmp
                        | InstKind::Keccak256(_, _)
                        | InstKind::Phi(_)
                        | InstKind::Select(_, _, _)
                        | InstKind::SignExtend(_, _)
                ) {
                    return false;
                }
            }
        }
        true
    }

    fn is_noreturn_block(&self, block_id: BlockId) -> bool {
        GlobalStackPlan::is_terminal_block(self.func, block_id)
            || matches!(
                self.func.blocks[block_id].terminator.as_ref(),
                Some(Terminator::TailCall { args, .. }) if args.is_empty()
            )
    }

    fn plan_join(&self, block_id: BlockId, plan: &mut StackPhiPlan) {
        let block = &self.func.blocks[block_id];
        if plan.entries.contains_key(&block_id)
            || self.loops.iter().any(|loop_info| loop_info.header == block_id)
            || block.predecessors.len() < 2
        {
            return;
        }

        let phi_insts = self.phi_insts(block);
        if phi_insts.is_empty() || phi_insts.len() > STACK_PHI_LAYOUT_LIMIT {
            return;
        }
        let Some(results) = self.phi_result_values(&phi_insts) else {
            return;
        };
        if block.predecessors.iter().any(|pred| {
            plan.edges.contains_key(pred)
                || !matches!(
                    self.func.blocks[*pred].terminator,
                    Some(Terminator::Jump(target)) if target == block_id
                )
        }) {
            return;
        }

        let mut edges = Vec::with_capacity(block.predecessors.len());
        for &pred in &block.predecessors {
            let Some(sources) = self.phi_sources_for_pred(&phi_insts, pred) else {
                return;
            };
            edges.push((pred, sources));
        }

        plan.entries.insert(block_id, results.clone());
        for (pred, sources) in edges {
            plan.edges.insert(pred, StackPhiEdge { sources, results: results.clone() });
        }
    }

    fn phi_insts(&self, block: &crate::mir::BasicBlock) -> Vec<InstId> {
        block
            .instructions
            .iter()
            .copied()
            .filter(|&inst| matches!(self.func.inst(inst).kind, InstKind::Phi(_)))
            .collect()
    }

    fn carry_through_values(&self, loop_info: &Loop) -> Vec<ValueId> {
        let mut carry_through = Vec::new();
        for outer in &self.loops {
            if outer.header == loop_info.header || !outer.blocks.contains(loop_info.header) {
                continue;
            }
            let Some(results) = self.header_results.get(&outer.header) else {
                continue;
            };
            for &value in results {
                if carry_through.contains(&value)
                    || !self.value_used_in_blocks(&loop_info.blocks, value)
                {
                    continue;
                }
                carry_through.push(value);
            }
        }
        carry_through
    }

    fn value_used_in_blocks(&self, blocks: &DenseBitSet<BlockId>, value: ValueId) -> bool {
        for block_id in blocks {
            let block = &self.func.blocks[block_id];
            for &inst_id in &block.instructions {
                if matches!(self.func.inst(inst_id).kind, InstKind::Phi(_)) {
                    continue;
                }
                if self.func.inst(inst_id).kind.operands().contains(&value) {
                    return true;
                }
            }
            if block.terminator.as_ref().is_some_and(|term| term.operands().contains(&value)) {
                return true;
            }
        }
        false
    }

    fn extend_live_through_values(&self, loop_info: &Loop, values: &mut Vec<ValueId>) {
        for block_id in &loop_info.blocks {
            let block = &self.func.blocks[block_id];
            for &inst_id in &block.instructions {
                let inst = self.func.inst(inst_id);
                if matches!(inst.kind, InstKind::Phi(_)) {
                    continue;
                }
                for value in inst.kind.operands() {
                    self.push_live_through_value(loop_info, value, values);
                }
            }
            if let Some(term) = &block.terminator {
                for value in term.operands() {
                    self.push_live_through_value(loop_info, value, values);
                }
            }
        }
    }

    fn extend_live_across_exits(
        &self,
        loop_info: &Loop,
        liveness: &Liveness,
        values: &mut Vec<ValueId>,
    ) {
        for block_id in &loop_info.blocks {
            let Some(terminator) = &self.func.blocks[block_id].terminator else { continue };
            for successor in terminator
                .successors()
                .into_iter()
                .filter(|successor| !loop_info.blocks.contains(*successor))
            {
                for value in liveness.live_in(successor) {
                    self.push_live_through_value(loop_info, value, values);
                }
            }
        }
    }

    fn push_live_through_value(&self, loop_info: &Loop, value: ValueId, values: &mut Vec<ValueId>) {
        let crate::mir::Value::Inst(_) = self.func.value(value) else { return };
        let Some(definition) = self.definitions[value] else { return };
        if !loop_info.blocks.contains(definition) && !values.contains(&value) {
            values.push(value);
        }
    }

    fn phi_result_values(&self, phi_insts: &[InstId]) -> Option<Vec<ValueId>> {
        phi_insts.iter().map(|&inst| self.func.inst_result_value(inst)).collect()
    }

    fn phi_sources_for_pred(&self, phi_insts: &[InstId], pred: BlockId) -> Option<Vec<ValueId>> {
        phi_insts
            .iter()
            .map(|&inst| {
                let InstKind::Phi(incoming) = &self.func.inst(inst).kind else {
                    return None;
                };
                incoming.iter().find_map(|&(block, value)| (block == pred).then_some(value))
            })
            .collect()
    }

    fn is_phi_value(&self, value: ValueId) -> bool {
        matches!(self.func.value(value), crate::mir::Value::Inst(inst) if matches!(self.func.inst(*inst).kind, InstKind::Phi(_)))
    }

    fn is_loop_header_phi(&self, value: ValueId) -> bool {
        let crate::mir::Value::Inst(inst) = self.func.value(value) else {
            return false;
        };
        self.loops
            .iter()
            .any(|loop_info| self.func.blocks[loop_info.header].instructions.contains(inst))
    }
}
