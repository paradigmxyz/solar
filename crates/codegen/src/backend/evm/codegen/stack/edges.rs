//! Emission of stack-resident CFG edges and phi transitions.

use super::super::{
    BlockId, EvmCodegen, Function, FxHashMap, GLOBAL_STACK_LAYOUT_LIMIT, GlobalStackPlan,
    MAX_STACK_ACCESS, StackModel, StackPhiBranch, StackPhiEdge, TargetSlot, Terminator, ValueId,
    op,
};

impl<'gcx> EvmCodegen<'gcx> {
    pub(in crate::backend::evm::codegen) fn set_stack_to_values(&mut self, values: &[ValueId]) {
        self.scheduler.stack.clear();
        for &value in values.iter().rev() {
            self.scheduler.stack.push(value);
        }
    }

    pub(in crate::backend::evm::codegen) fn try_emit_global_stack_edge(
        &mut self,
        func: &Function,
        term: &Terminator,
        layout: &[ValueId],
    ) -> bool {
        if layout.is_empty() || layout.len() > GLOBAL_STACK_LAYOUT_LIMIT {
            return false;
        }

        let mut needed = Vec::with_capacity(layout.len() + 1);
        match term {
            Terminator::Branch { condition, .. } => needed.push(*condition),
            Terminator::Switch { value, .. } => needed.push(*value),
            _ => {}
        }
        needed.extend_from_slice(layout);

        self.pop_stack_values_not_needed_by(&needed);
        for value in Self::missing_stack_phi_sources(&self.scheduler.stack, &needed) {
            self.emit_operand(func, value);
        }

        let target: Vec<_> = needed.iter().copied().map(TargetSlot::Value).collect();
        let shuffle = self
            .scheduler
            .shuffle_to_layout(&target)
            .unwrap_or_else(|| panic!("could not construct global stack edge layout"));
        assert_eq!(self.scheduler.depth(), needed.len(), "global-stack edge depth mismatch");
        assert!(
            self.scheduler.stack.iter().eq(needed.iter().copied().map(Some)),
            "global-stack edge layout mismatch"
        );
        for op in shuffle.ops {
            self.asm.emit_stack_op(op);
        }

        true
    }

    pub(in crate::backend::evm::codegen) fn global_branch_union(
        then_layout: &[ValueId],
        else_layout: &[ValueId],
    ) -> Vec<ValueId> {
        let mut union = then_layout.to_vec();
        for &value in else_layout {
            if !union.contains(&value) {
                union.push(value);
            }
        }
        union
    }

    pub(in crate::backend::evm::codegen) fn try_emit_global_stack_branch(
        &mut self,
        func: &Function,
        condition: ValueId,
        then_layout: &[ValueId],
        else_layout: &[ValueId],
    ) -> Option<Vec<ValueId>> {
        let union = Self::global_branch_union(then_layout, else_layout);
        if union.is_empty() || union.len() > GLOBAL_STACK_LAYOUT_LIMIT {
            return None;
        }
        let mut needed = Vec::with_capacity(union.len() + 1);
        needed.push(condition);
        needed.extend_from_slice(&union);
        self.pop_stack_values_not_needed_by(&needed);
        for value in Self::missing_stack_phi_sources(&self.scheduler.stack, &needed) {
            self.emit_operand(func, value);
        }
        let target: Vec<_> = needed.iter().copied().map(TargetSlot::Value).collect();
        let shuffle = self
            .scheduler
            .shuffle_to_layout(&target)
            .unwrap_or_else(|| panic!("could not construct edge-specific branch layout"));
        for op in shuffle.ops {
            self.asm.emit_stack_op(op);
        }
        Some(union)
    }

    pub(in crate::backend::evm::codegen) fn global_switch_union(
        layouts: &[(BlockId, Vec<ValueId>)],
    ) -> Vec<ValueId> {
        let mut union = Vec::new();
        for (_, layout) in layouts {
            for &value in layout {
                if !union.contains(&value) {
                    union.push(value);
                }
            }
        }
        union
    }

    fn emit_global_branch_cleanup(&mut self, layout: &[ValueId]) {
        self.pop_stack_values_not_needed_by(layout);
        let target: Vec<_> = layout.iter().copied().map(TargetSlot::Value).collect();
        let shuffle = self
            .scheduler
            .shuffle_to_layout(&target)
            .unwrap_or_else(|| panic!("could not construct edge-specific resident stack layout"));
        for op in shuffle.ops {
            self.asm.emit_stack_op(op);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::backend::evm::codegen) fn emit_global_stack_branch(
        &mut self,
        func: &Function,
        condition: ValueId,
        then_block: BlockId,
        else_block: BlockId,
        then_layout: &[ValueId],
        else_layout: &[ValueId],
        union: &[ValueId],
        fallthrough: Option<BlockId>,
    ) {
        debug_assert_eq!(self.scheduler.stack.top(), Some(condition));

        // Values below a direct revert/invalid are inert. Preserve the union into that terminal
        // arm and let the ordinary branch emitter retain its hot-edge and fallthrough choices,
        // rather than paying cleanup operations on a path that cannot return.
        let terminal_cleanup = (then_layout.is_empty()
            && else_layout == union
            && GlobalStackPlan::is_terminal_block(func, then_block))
            || (else_layout.is_empty()
                && then_layout == union
                && GlobalStackPlan::is_terminal_block(func, else_block));
        if terminal_cleanup {
            self.generate_terminator(
                func,
                &Terminator::Branch { condition, then_block, else_block },
                fallthrough,
                true,
            );
            return;
        }

        let then_is_union = then_layout == union;
        let else_is_union = else_layout == union;

        if then_is_union || else_is_union {
            let (direct, cleanup, cleanup_layout, invert) = if then_is_union {
                (then_block, else_block, else_layout, false)
            } else {
                (else_block, then_block, then_layout, true)
            };
            if invert {
                self.asm.emit_op(op::ISZERO);
                self.scheduler.instruction_executed_untracked(1);
            }
            self.emit_push_label(self.block_labels[&direct]);
            self.asm.emit_op(op::JUMPI);
            self.scheduler.stack.pop();
            self.emit_global_branch_cleanup(cleanup_layout);
            if Some(cleanup) != fallthrough {
                self.emit_push_label(self.block_labels[&cleanup]);
                self.asm.emit_op(op::JUMP);
            }
            return;
        }

        // Neither target wants the complete incoming union. Route one edge through a local
        // cleanup label and clean the fallthrough edge inline.
        let then_cleanup = self.asm.new_label();
        self.emit_push_label(then_cleanup);
        self.asm.emit_op(op::JUMPI);
        self.scheduler.stack.pop();
        let union_stack = self.scheduler.stack.clone();

        self.emit_global_branch_cleanup(else_layout);
        self.emit_push_label(self.block_labels[&else_block]);
        self.asm.emit_op(op::JUMP);

        self.asm.define_label(then_cleanup);
        self.scheduler.stack = union_stack;
        self.emit_global_branch_cleanup(then_layout);
        if Some(then_block) != fallthrough {
            self.emit_push_label(self.block_labels[&then_block]);
            self.asm.emit_op(op::JUMP);
        }
    }

    pub(in crate::backend::evm::codegen) fn emit_global_stack_switch(
        &mut self,
        func: &Function,
        value: ValueId,
        default: BlockId,
        cases: &[(ValueId, BlockId)],
        layouts: &[(BlockId, Vec<ValueId>)],
        union: &[ValueId],
    ) {
        debug_assert_eq!(self.scheduler.stack.top(), Some(value));

        // Switch lowering may use linear tests, trees, hashes, or indexed jumps. Redirecting its
        // target labels keeps those lowering strategies oblivious to the ABI cleanup while giving
        // each successor precisely its proven entry tuple.
        let mut trampolines = Vec::new();
        for (target, layout) in layouts {
            if layout == union {
                continue;
            }
            let actual = self.block_labels[target];
            let trampoline = self.asm.new_label();
            self.block_labels.insert(*target, trampoline);
            trampolines.push((*target, actual, trampoline, layout.clone()));
        }

        // Cleanup trampolines occupy the lexical fallthrough position, so force the switch default
        // to jump even when its real target is the next MIR block.
        self.emit_switch_terminator(func, value, default, cases, None, true);
        for &(target, actual, _, _) in &trampolines {
            self.block_labels.insert(target, actual);
        }

        for (_, actual, trampoline, layout) in trampolines {
            self.asm.define_label(trampoline);
            self.set_stack_to_values(union);
            self.emit_global_branch_cleanup(&layout);
            self.emit_push_label(actual);
            self.asm.emit_op(op::JUMP);
        }
    }

    pub(in crate::backend::evm::codegen) fn try_emit_stack_phi_edge(
        &mut self,
        func: &Function,
        edge: &StackPhiEdge,
    ) -> bool {
        if edge.sources.len() != edge.results.len()
            || edge.sources.is_empty()
            || edge.sources.len() > MAX_STACK_ACCESS
        {
            return false;
        }
        if !self.stack_contains_only_phi_sources(&edge.sources) {
            return false;
        }

        for &source in Self::missing_stack_phi_sources(&self.scheduler.stack, &edge.sources).iter()
        {
            if !self.scheduler.can_emit_value(source, func) {
                return false;
            }
            self.emit_operand(func, source);
        }
        assert!(
            self.stack_contains_only_phi_sources(&edge.sources),
            "prepared stack-phi edge contains unexpected values"
        );

        let target: Vec<_> = edge.sources.iter().copied().map(TargetSlot::Value).collect();
        let Some(shuffle) = self.scheduler.shuffle_to_layout(&target) else { return false };
        assert_eq!(self.scheduler.depth(), edge.sources.len(), "stack-phi edge depth mismatch");
        assert!(
            self.scheduler.stack.iter().eq(edge.sources.iter().copied().map(Some)),
            "stack-phi edge layout mismatch"
        );
        for op in shuffle.ops {
            self.asm.emit_stack_op(op);
        }

        self.set_stack_to_values(&edge.results);
        true
    }

    pub(in crate::backend::evm::codegen) fn can_prepare_stack_phi_edge(
        &self,
        func: &Function,
        edge: &StackPhiEdge,
    ) -> bool {
        if edge.sources.len() != edge.results.len()
            || edge.sources.is_empty()
            || edge.sources.len() > MAX_STACK_ACCESS
        {
            return false;
        }

        let present =
            Self::stack_phi_source_counts_after_trim(&self.scheduler.stack, &edge.sources);
        if present.len() > MAX_STACK_ACCESS {
            return false;
        }

        let mut seen = Self::value_counts(present);
        for &source in &edge.sources {
            if let Some(count) = seen.get_mut(&source)
                && *count > 0
            {
                *count -= 1;
                continue;
            }
            if !self.can_emit_stack_phi_value(func, source) {
                return false;
            }
        }
        true
    }

    fn can_emit_stack_phi_value(&self, func: &Function, value: ValueId) -> bool {
        self.scheduler.can_emit_value(value, func)
            || self.scheduler.should_recompute_unstored_spill(value)
            || Self::is_always_rematerializable_value(func, value)
    }

    pub(in crate::backend::evm::codegen) fn can_prepare_stack_phi_branch(
        &self,
        func: &Function,
        condition: ValueId,
        branch: &StackPhiBranch,
    ) -> bool {
        !branch.union.is_empty()
            && branch.union.len() <= MAX_STACK_ACCESS
            && self.can_emit_stack_phi_value(func, condition)
            && self.can_prepare_stack_phi_branch_edge(func, &branch.then_edge)
            && self.can_prepare_stack_phi_branch_edge(func, &branch.else_edge)
    }

    fn can_prepare_stack_phi_branch_edge(&self, func: &Function, edge: &StackPhiEdge) -> bool {
        (edge.sources.is_empty() && edge.results.is_empty())
            || self.can_prepare_stack_phi_edge(func, edge)
    }

    fn emit_stack_phi_edge_layout(&mut self, edge: &StackPhiEdge) {
        self.pop_stack_values_not_needed_by(&edge.sources);
        let target: Vec<_> = edge.sources.iter().copied().map(TargetSlot::Value).collect();
        let shuffle = self
            .scheduler
            .shuffle_to_layout(&target)
            .expect("could not construct branch stack-phi edge layout");
        for op in shuffle.ops {
            self.asm.emit_stack_op(op);
        }
        self.set_stack_to_values(&edge.results);
    }

    pub(in crate::backend::evm::codegen) fn emit_stack_phi_branch(
        &mut self,
        func: &Function,
        condition: ValueId,
        then_block: BlockId,
        else_block: BlockId,
        branch: &StackPhiBranch,
        fallthrough: Option<BlockId>,
    ) {
        let mut needed = Vec::with_capacity(branch.union.len() + 1);
        needed.push(condition);
        needed.extend_from_slice(&branch.union);
        self.pop_stack_values_not_needed_by(&needed);
        for value in Self::missing_stack_phi_sources(&self.scheduler.stack, &needed) {
            debug_assert!(self.can_emit_stack_phi_value(func, value));
            self.emit_operand(func, value);
        }
        let target: Vec<_> = needed.iter().copied().map(TargetSlot::Value).collect();
        let shuffle = self
            .scheduler
            .shuffle_to_layout(&target)
            .expect("could not construct branch stack-phi layout");
        for op in shuffle.ops {
            self.asm.emit_stack_op(op);
        }

        let identity =
            |edge: &StackPhiEdge| edge.sources == branch.union && edge.results == edge.sources;
        let (laid_out, direct_block, laid_out_block, invert) = if identity(&branch.then_edge) {
            (&branch.else_edge, then_block, else_block, false)
        } else if identity(&branch.else_edge) {
            (&branch.then_edge, else_block, then_block, true)
        } else {
            let then_cleanup = self.asm.new_label();
            self.asm.emit_push_label(then_cleanup);
            self.asm.emit_op(op::JUMPI);
            self.scheduler.stack.pop();
            let union_stack = self.scheduler.stack.clone();

            self.emit_stack_phi_edge_layout(&branch.else_edge);
            self.emit_push_label(self.block_labels[&else_block]);
            self.asm.emit_op(op::JUMP);

            self.asm.define_label(then_cleanup);
            self.scheduler.stack = union_stack;
            self.emit_stack_phi_edge_layout(&branch.then_edge);
            self.emit_push_label(self.block_labels[&then_block]);
            self.asm.emit_op(op::JUMP);
            return;
        };
        if invert {
            self.asm.emit_op(op::ISZERO);
        }
        self.emit_push_label(self.block_labels[&direct_block]);
        self.asm.emit_op(op::JUMPI);
        self.scheduler.stack.pop();
        self.emit_stack_phi_edge_layout(laid_out);
        if fallthrough != Some(laid_out_block) {
            self.emit_push_label(self.block_labels[&laid_out_block]);
            self.asm.emit_op(op::JUMP);
        }
    }

    fn stack_phi_source_counts_after_trim(stack: &StackModel, sources: &[ValueId]) -> Vec<ValueId> {
        let mut remaining = Self::value_counts(sources.iter().copied());
        let mut kept = Vec::new();
        for value in stack.iter().flatten() {
            if let Some(count) = remaining.get_mut(&value)
                && *count > 0
            {
                *count -= 1;
                kept.push(value);
            }
        }
        kept
    }

    fn stack_contains_only_phi_sources(&self, sources: &[ValueId]) -> bool {
        let mut remaining = Self::value_counts(sources.iter().copied());
        for slot in self.scheduler.stack.iter() {
            let Some(value) = slot else {
                return false;
            };
            let Some(count) = remaining.get_mut(&value) else {
                return false;
            };
            if *count == 0 {
                return false;
            }
            *count -= 1;
        }
        true
    }

    pub(in crate::backend::evm::codegen) fn missing_stack_phi_sources(
        stack: &StackModel,
        sources: &[ValueId],
    ) -> Vec<ValueId> {
        let mut needed = Self::value_counts(sources.iter().copied());
        for value in stack.iter().flatten() {
            if let Some(count) = needed.get_mut(&value)
                && *count > 0
            {
                *count -= 1;
            }
        }

        let mut missing = Vec::new();
        for &source in sources {
            if let Some(count) = needed.get_mut(&source)
                && *count > 0
            {
                missing.push(source);
                *count -= 1;
            }
        }
        missing
    }

    pub(in crate::backend::evm::codegen) fn value_counts(
        values: impl IntoIterator<Item = ValueId>,
    ) -> FxHashMap<ValueId, usize> {
        let mut counts = FxHashMap::default();
        for value in values {
            *counts.entry(value).or_default() += 1;
        }
        counts
    }

    pub(in crate::backend::evm::codegen) fn can_preserve_stack_fallthrough(
        &self,
        func: &Function,
        block_id: BlockId,
        fallthrough: Option<BlockId>,
    ) -> bool {
        let Some(Terminator::Jump(target)) = func.blocks[block_id].terminator.as_ref() else {
            return false;
        };
        if Some(*target) != fallthrough {
            return false;
        }

        // This block is the target's only predecessor, so no non-fallthrough edge can observe or
        // depend on a JUMPDEST at the target label.
        func.blocks[*target].predecessors.as_slice() == [block_id]
    }

    pub(in crate::backend::evm::codegen) fn is_stack_phi_source(
        &self,
        block: BlockId,
        value: ValueId,
    ) -> bool {
        self.stack_phi_sources.get(&block).is_some_and(|sources| sources.contains(&value))
    }
}
