//! Global stack layouts for cross-block values and resident arguments.

use super::super::super::{
    ArgIdx, BlockId, CfgInfo, DenseBitSet, Function, FxHashMap, GLOBAL_STACK_LAYOUT_LIMIT,
    InstKind, Liveness, OptimizationMode, StackPhiPlan, Terminator, ValueId, WORD_BYTES,
};

const GLOBAL_STACK_DENSE_AMORTIZATION_BLOCKS: usize = 16;

const GLOBAL_STACK_MIN_ARG_USES: usize = 6;

const GLOBAL_STACK_MIN_BLOCKS: usize = 8;

const GLOBAL_STACK_MAX_ARGS: usize = 3;

/// Canonical argument layouts carried between MIR basic blocks.
///
/// A block-local scheduler normally discards its model at every join. Function
/// arguments are special: they have one identity on every incoming edge and can
/// always be rematerialized as a safe fallback. Agreeing on one layout for all
/// predecessors lets the first load remain stack-resident through diamonds and
/// loops instead of repeating `CALLDATALOAD` or frame `MLOAD` in every block.
#[derive(Clone, Debug, Default)]
pub(in crate::backend::evm::codegen) struct GlobalStackPlan {
    pub(in crate::backend::evm::codegen) entries: FxHashMap<BlockId, Vec<ValueId>>,
    pub(in crate::backend::evm::codegen) aliases: FxHashMap<ValueId, ValueId>,
    /// Whether terminal successors need an explicit, exact entry layout.
    /// External arguments can reload in revert blocks; resident internal
    /// arguments have no memory fallback and therefore cannot ignore them.
    pub(in crate::backend::evm::codegen) terminal_sensitive: bool,
}

impl GlobalStackPlan {
    pub(in crate::backend::evm::codegen) fn analyze(
        func: &Function,
        liveness: &Liveness,
        stack_phi_plan: &StackPhiPlan,
        optimization: OptimizationMode,
    ) -> Self {
        if func.selector.is_none() {
            return Self::default();
        }

        let mut entries = FxHashMap::default();
        let arg_uses = func.arg_uses();
        let used_args = arg_uses.iter().filter(|uses| !uses.is_empty()).count();
        if !(2..=GLOBAL_STACK_MAX_ARGS).contains(&used_args) {
            return Self::default();
        }

        let cfg = CfgInfo::new(func);
        if cfg.reachable().count() < GLOBAL_STACK_MIN_BLOCKS {
            return Self::default();
        }
        let mut decode_blocks = FxHashMap::default();
        let mut aliases = FxHashMap::default();
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                let InstKind::CalldataLoad(offset) = &func.inst(inst_id).kind else {
                    continue;
                };
                let Some(offset) = func.value_u64(*offset) else {
                    continue;
                };
                if offset >= 4
                    && (offset - 4) % WORD_BYTES as u64 == 0
                    && let Ok(index) = u32::try_from((offset - 4) / WORD_BYTES as u64)
                    && let Some(&arg) =
                        arg_uses.get(ArgIdx::new(index as usize)).and_then(|uses| uses.first())
                {
                    decode_blocks.entry(arg).or_insert(block_id);
                    if let Some(result) = func.inst_result_value(inst_id) {
                        aliases.insert(result, arg);
                    }
                }
            }
        }

        for block_id in func.blocks.indices() {
            if !cfg.is_reachable(block_id)
                || func.blocks[block_id].predecessors.is_empty()
                || stack_phi_plan.entries.contains_key(&block_id)
                || Self::is_terminal_block(func, block_id)
            {
                continue;
            }

            let values = liveness
                .live_in(block_id)
                .iter()
                .filter(|&value| {
                    matches!(func.value(value), crate::mir::Value::Arg(_))
                        && decode_blocks.get(&value).is_none_or(|&decode| {
                            decode != block_id && cfg.dominators().dominates(decode, block_id)
                        })
                })
                .take(GLOBAL_STACK_LAYOUT_LIMIT)
                .collect::<Vec<_>>();
            if !values.is_empty() {
                entries.insert(block_id, values);
            }
        }

        // A branch leaves one physical stack for both outgoing edges after its
        // condition is consumed. Its successors therefore have to agree on the
        // same canonical layout. Use the union so an argument needed by either
        // live successor remains available. Terminal siblings are excluded:
        // carried words are harmless below their abort operands. Iterate
        // because sibling constraints can connect several diamonds.
        let mut changed = true;
        while changed {
            changed = false;
            for block_id in func.blocks.indices() {
                let Some(Terminator::Branch { then_block, else_block, .. }) =
                    func.blocks[block_id].terminator.as_ref()
                else {
                    continue;
                };
                if Self::is_terminal_block(func, *then_block)
                    || Self::is_terminal_block(func, *else_block)
                {
                    continue;
                }
                let mut common = entries.get(then_block).cloned().unwrap_or_default();
                for &value in entries.get(else_block).into_iter().flatten() {
                    if common.len() == GLOBAL_STACK_LAYOUT_LIMIT {
                        break;
                    }
                    if !common.contains(&value) {
                        common.push(value);
                    }
                }
                common.sort_unstable_by_key(|value| value.index());
                changed |= Self::set_entry(&mut entries, *then_block, &common);
                changed |= Self::set_entry(&mut entries, *else_block, &common);
            }
        }

        // Switch lowering owns the selector stack, and stack-phi entries
        // own their edge layouts. Disable their whole branch-sibling component
        // so every predecessor of every affected block still agrees.
        let mut disabled = DenseBitSet::new_empty(func.blocks.len());
        for &block in stack_phi_plan.entries.keys() {
            disabled.insert(block);
        }
        for block_id in func.blocks.indices() {
            if let Some(Terminator::Switch { default, cases, .. }) =
                func.blocks[block_id].terminator.as_ref()
            {
                disabled.insert(*default);
                for &(_, target) in cases {
                    disabled.insert(target);
                }
            }
        }
        let mut changed = true;
        while changed {
            changed = false;
            for block_id in func.blocks.indices() {
                let Some(Terminator::Branch { then_block, else_block, .. }) =
                    func.blocks[block_id].terminator.as_ref()
                else {
                    continue;
                };
                if Self::is_terminal_block(func, *then_block)
                    || Self::is_terminal_block(func, *else_block)
                {
                    continue;
                }
                if disabled.contains(*then_block) || disabled.contains(*else_block) {
                    changed |= disabled.insert(*then_block);
                    changed |= disabled.insert(*else_block);
                }
            }
        }
        entries.retain(|block, _| !disabled.contains(*block));

        // Canonicalization pays DUP/SWAP/POP traffic on every planned edge.
        // Require enough real argument reuse to recover that fixed cost, and
        // reject dense layout plans unless a long CFG can amortize them. Size mode
        // permits dense acyclic plans: carrying arguments avoids repeated calldata
        // loads without charging stack cleanup on every loop iteration.
        let arg_use_count = arg_uses.iter().map(Vec::len).sum::<usize>();
        if arg_use_count < GLOBAL_STACK_MIN_ARG_USES
            || (entries.len() * 2 > cfg.reachable().count()
                && (!optimization.is_size() || !cfg.cyclic_blocks().is_empty())
                && cfg.reachable().count() < GLOBAL_STACK_DENSE_AMORTIZATION_BLOCKS)
        {
            entries.clear();
        }
        aliases.retain(|_, arg| entries.values().any(|entry| entry.contains(arg)));
        Self { entries, aliases, terminal_sensitive: false }
    }

    /// Plans a single physical layout for stack-passed arguments that never
    /// receive a static-frame home. Unlike the calldata layout above, this is
    /// an ABI invariant: every live edge must carry the value because there is
    /// no legal reload fallback.
    pub(in crate::backend::evm::codegen) fn analyze_resident_args(
        func: &Function,
        liveness: &Liveness,
        values: &[ValueId],
        preserve_across_calls: bool,
    ) -> Option<Self> {
        if values.is_empty() {
            return None;
        }
        // Nested calls are eligible only when runtime emission can retain the live resident prefix
        // below their return address. Stack-phi edges compose their changing values above this
        // invariant prefix. The analysis remains deliberately all-or-nothing because resident
        // arguments cannot fall back to memory on just one edge.
        if func.blocks.iter().any(|block| {
            block.instructions.iter().any(|&inst_id| {
                !preserve_across_calls && matches!(func.inst(inst_id).kind, InstKind::ICall { .. })
            })
        }) {
            return None;
        }

        let cfg = CfgInfo::new(func);
        let mut entries = FxHashMap::default();
        for block_id in func.blocks.indices() {
            if block_id == BlockId::ENTRY
                || !cfg.is_reachable(block_id)
                || func.blocks[block_id].predecessors.is_empty()
            {
                continue;
            }
            let entry: Vec<_> = values
                .iter()
                .copied()
                .filter(|&value| liveness.live_in(block_id).contains(value))
                .collect();
            if entry.len() > GLOBAL_STACK_LAYOUT_LIMIT {
                return None;
            }
            if !entry.is_empty() {
                entries.insert(block_id, entry);
            }
        }

        // A branch initially inherits one physical stack on both arms, but codegen can clean up a
        // resident superset on either edge before entering its target. Keep each target's actual
        // live-in layout here; forcing their union would make an unrelated join require values its
        // other predecessors cannot rematerialize.
        for block in &func.blocks {
            let Some(Terminator::Branch { then_block, else_block, .. }) = &block.terminator else {
                continue;
            };
            if entries.get(then_block) != entries.get(else_block) {
                // Private successors can inherit the same union and discard dead words at entry.
                // Preserve that cheaper fallthrough-aware shape; edge cleanup is needed only when
                // padding a shared successor would impose values its other predecessors cannot
                // materialize.
                if func.blocks[*then_block].predecessors.len() == 1
                    && func.blocks[*else_block].predecessors.len() == 1
                {
                    let union: Vec<_> = values
                        .iter()
                        .copied()
                        .filter(|value| {
                            entries.get(then_block).is_some_and(|entry| entry.contains(value))
                                || entries
                                    .get(else_block)
                                    .is_some_and(|entry| entry.contains(value))
                        })
                        .collect();
                    entries.insert(*then_block, union.clone());
                    entries.insert(*else_block, union);
                    continue;
                }
                let mut union = entries.get(then_block).cloned().unwrap_or_default();
                for &value in entries.get(else_block).into_iter().flatten() {
                    if !union.contains(&value) {
                        union.push(value);
                    }
                }
                if union.len() > GLOBAL_STACK_LAYOUT_LIMIT {
                    return None;
                }
            }
        }

        // A switch initially carries one physical stack through its dispatch, but codegen can
        // route each target through a cleanup trampoline. Keep the exact target layouts here and
        // only require their union to remain within the globally schedulable prefix.
        for block in &func.blocks {
            let Some(Terminator::Switch { default, cases, .. }) = &block.terminator else {
                continue;
            };
            let mut targets = Vec::with_capacity(cases.len() + 1);
            targets.push(*default);
            for &(_, target) in cases {
                if !targets.contains(&target) {
                    targets.push(target);
                }
            }
            let mut union = Vec::new();
            for target in targets {
                for &value in entries.get(&target).into_iter().flatten() {
                    if !union.contains(&value) {
                        union.push(value);
                    }
                }
            }
            if union.len() > GLOBAL_STACK_LAYOUT_LIMIT {
                return None;
            }
        }

        let plan = Self { entries, aliases: FxHashMap::default(), terminal_sensitive: true };
        // Prove that every live-in is represented and every predecessor can
        // establish precisely the target layout. This is what makes omitting
        // the argument's frame store sound rather than merely profitable.
        for block_id in func.blocks.indices() {
            if block_id != BlockId::ENTRY {
                for &value in values {
                    if liveness.live_in(block_id).contains(value)
                        && plan.entry(block_id).is_none_or(|entry| !entry.contains(&value))
                    {
                        return None;
                    }
                }
            }
            let Some(expected) = plan.entry(block_id) else { continue };
            for &pred in &func.blocks[block_id].predecessors {
                let term = func.blocks[pred].terminator.as_ref()?;
                let establishes_layout = match term {
                    Terminator::Branch { then_block, else_block, .. }
                        if *then_block == block_id || *else_block == block_id =>
                    {
                        plan.entry(block_id) == Some(expected)
                            && plan.branch_layouts(term).is_some()
                    }
                    Terminator::Switch { default, cases, .. }
                        if *default == block_id
                            || cases.iter().any(|(_, target)| *target == block_id) =>
                    {
                        plan.switch_layouts(term).is_some()
                    }
                    _ => plan.edge_layout(func, term) == Some(expected),
                };
                if !establishes_layout {
                    return None;
                }
            }
        }

        Some(plan)
    }

    pub(in crate::backend::evm::codegen) fn set_entry(
        entries: &mut FxHashMap<BlockId, Vec<ValueId>>,
        block: BlockId,
        layout: &[ValueId],
    ) -> bool {
        if entries.get(&block).map_or(layout.is_empty(), |old| old == layout) {
            return false;
        }
        if layout.is_empty() {
            entries.remove(&block);
        } else {
            entries.insert(block, layout.to_vec());
        }
        true
    }

    pub(in crate::backend::evm::codegen) fn entry(&self, block: BlockId) -> Option<&[ValueId]> {
        self.entries.get(&block).map(Vec::as_slice)
    }

    pub(in crate::backend::evm::codegen) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(in crate::backend::evm::codegen) fn edge_layout(
        &self,
        func: &Function,
        term: &Terminator,
    ) -> Option<&[ValueId]> {
        match term {
            Terminator::Jump(target) => self.entry(*target),
            Terminator::Branch { then_block, else_block, .. } => {
                if !self.terminal_sensitive {
                    if Self::is_terminal_block(func, *then_block) {
                        return self.entry(*else_block);
                    }
                    if Self::is_terminal_block(func, *else_block) {
                        return self.entry(*then_block);
                    }
                }
                let then_layout = self.entry(*then_block)?;
                (self.entry(*else_block) == Some(then_layout)).then_some(then_layout)
            }
            Terminator::Switch { default, cases, .. } => {
                let mut layout = None;
                for target in std::iter::once(default).chain(cases.iter().map(|(_, target)| target))
                {
                    if !self.terminal_sensitive
                        && Self::is_terminal_block(func, *target)
                        && self.entry(*target).is_none()
                    {
                        continue;
                    }
                    let target_layout = self.entry(*target)?;
                    if let Some(layout) = layout {
                        if layout != target_layout {
                            return None;
                        }
                    } else {
                        layout = Some(target_layout);
                    }
                }
                layout
            }
            _ => None,
        }
    }

    pub(in crate::backend::evm::codegen) fn branch_layouts(
        &self,
        term: &Terminator,
    ) -> Option<(&[ValueId], &[ValueId])> {
        let Terminator::Branch { then_block, else_block, .. } = term else { return None };
        let then_layout = self.entry(*then_block).unwrap_or(&[]);
        let else_layout = self.entry(*else_block).unwrap_or(&[]);
        if then_layout.is_empty() && else_layout.is_empty() {
            return None;
        }
        let union_len = then_layout.len()
            + else_layout.iter().filter(|value| !then_layout.contains(value)).count();
        (union_len <= GLOBAL_STACK_LAYOUT_LIMIT).then_some((then_layout, else_layout))
    }

    pub(in crate::backend::evm::codegen) fn switch_layouts(
        &self,
        term: &Terminator,
    ) -> Option<Vec<(BlockId, &[ValueId])>> {
        let Terminator::Switch { default, cases, .. } = term else { return None };
        let mut layouts = Vec::with_capacity(cases.len() + 1);
        for target in std::iter::once(default).chain(cases.iter().map(|(_, target)| target)) {
            if !layouts.iter().any(|(existing, _)| *existing == *target) {
                layouts.push((*target, self.entry(*target).unwrap_or(&[])));
            }
        }
        let mut union = Vec::new();
        for &(_, layout) in &layouts {
            for &value in layout {
                if !union.contains(&value) {
                    union.push(value);
                }
            }
        }
        (!union.is_empty() && union.len() <= GLOBAL_STACK_LAYOUT_LIMIT).then_some(layouts)
    }

    /// Returns values present in every physical successor layout of `term`.
    ///
    /// Requires a terminal-sensitive plan: `edge_layout`'s terminal shortcut would
    /// otherwise report one-sided carriage for edges the layout does not cover,
    /// and spill-elision consumers rely on an every-edge guarantee.
    pub(in crate::backend::evm::codegen) fn uniformly_carried_values(
        &self,
        func: &Function,
        term: &Terminator,
    ) -> Vec<ValueId> {
        debug_assert!(
            self.terminal_sensitive,
            "carried-value queries require terminal-sensitive plans"
        );
        if let Some((then_layout, else_layout)) = self.branch_layouts(term) {
            return then_layout
                .iter()
                .copied()
                .filter(|value| else_layout.contains(value))
                .collect();
        }
        if let Some(layouts) = self.switch_layouts(term) {
            let mut layouts = layouts.into_iter().map(|(_, layout)| layout);
            let mut values = layouts.next().unwrap_or_default().to_vec();
            for layout in layouts {
                values.retain(|value| layout.contains(value));
            }
            return values;
        }
        self.edge_layout(func, term).unwrap_or_default().to_vec()
    }

    pub(in crate::backend::evm::codegen) fn is_terminal_block(
        func: &Function,
        block: BlockId,
    ) -> bool {
        matches!(
            func.blocks[block].terminator,
            Some(Terminator::Revert { .. } | Terminator::RevertReturndata | Terminator::Invalid)
        )
    }
}
