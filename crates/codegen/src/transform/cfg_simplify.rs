//! CFG Simplification and Normalization passes.
//!
//! This module provides optimization passes to clean up the Control Flow Graph:
//!
//! ## Block Merging
//! If block A unconditionally jumps to B, and B has only A as predecessor,
//! merge A and B into a single block. This reduces jump instructions (8 gas each).
//!
//! ## Empty Block Elimination
//! Remove blocks that contain no instructions and only an unconditional jump,
//! redirecting predecessors to the target.
//!
//! ## Dead Function Elimination
//! Remove functions that are never called, starting from entry points
//! (public/external functions, constructor, fallback, receive).

use crate::{
    mir::{BlockId, Function, FunctionId, InstKind, Module, Terminator, Value, ValueId},
    pass::{FunctionPass, ModulePass},
};
use solar_data_structures::map::{FxHashMap, FxHashSet};
use std::collections::VecDeque;

/// Statistics from CFG simplification.
#[derive(Debug, Default, Clone)]
pub struct CfgSimplifyStats {
    /// Number of blocks merged.
    pub blocks_merged: usize,
    /// Number of empty blocks eliminated.
    pub empty_blocks_eliminated: usize,
    /// Number of degenerate terminators simplified.
    pub terminators_simplified: usize,
    /// Number of dead functions eliminated.
    pub dead_functions_eliminated: usize,
    /// Estimated gas saved (8 gas per eliminated jump).
    pub gas_saved: usize,
}

impl CfgSimplifyStats {
    /// Returns total optimizations performed.
    #[must_use]
    pub fn total(&self) -> usize {
        self.blocks_merged
            + self.empty_blocks_eliminated
            + self.terminators_simplified
            + self.dead_functions_eliminated
    }

    /// Combines stats from another run.
    pub fn combine(&mut self, other: &Self) {
        self.blocks_merged += other.blocks_merged;
        self.empty_blocks_eliminated += other.empty_blocks_eliminated;
        self.terminators_simplified += other.terminators_simplified;
        self.dead_functions_eliminated += other.dead_functions_eliminated;
        self.gas_saved += other.gas_saved;
    }
}

/// CFG simplification pass for a single function.
#[derive(Debug, Default)]
pub struct CfgSimplifier {
    /// Statistics from the last run.
    pub stats: CfgSimplifyStats,
}

/// Function pass for CFG simplification.
pub struct CfgSimplifyPass;

impl FunctionPass for CfgSimplifyPass {
    fn name(&self) -> &str {
        "cfg-simplify"
    }

    fn run_on_function(&mut self, func: &mut Function) -> bool {
        CfgSimplifier::new().run_to_fixpoint(func).total() != 0
    }
}

impl CfgSimplifier {
    /// Creates a new CFG simplifier.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs CFG simplification on a function.
    /// Returns the number of optimizations performed.
    pub fn run(&mut self, func: &mut Function) -> usize {
        self.stats = CfgSimplifyStats::default();

        self.simplify_degenerate_terminators(func);
        self.merge_blocks(func);
        self.eliminate_empty_blocks(func);

        self.stats.total()
    }

    fn simplify_degenerate_terminators(&mut self, func: &mut Function) {
        let block_ids: Vec<_> = func.blocks.indices().collect();
        let mut changed = false;
        for block_id in block_ids {
            let Some(Terminator::Branch { then_block, else_block, .. }) =
                func.blocks[block_id].terminator.as_ref()
            else {
                continue;
            };
            if then_block != else_block {
                continue;
            }

            let target = *then_block;
            func.blocks[block_id].terminator = Some(Terminator::Jump(target));
            self.stats.terminators_simplified += 1;
            self.stats.gas_saved += 10;
            changed = true;
        }

        if changed {
            repair_reachability_phis(func);
        }
    }

    /// Runs CFG simplification iteratively until no more changes.
    pub fn run_to_fixpoint(&mut self, func: &mut Function) -> CfgSimplifyStats {
        let mut total_stats = CfgSimplifyStats::default();
        loop {
            let changed = self.run(func);
            if changed == 0 {
                break;
            }
            total_stats.combine(&self.stats);
        }
        total_stats
    }

    /// Merges blocks where A unconditionally jumps to B and B has only A as predecessor.
    fn merge_blocks(&mut self, func: &mut Function) {
        let mut merged = true;
        while merged {
            merged = false;

            let block_ids: Vec<_> = func.blocks.indices().collect();
            for block_id in block_ids {
                if let Some(target) = self.can_merge(func, block_id) {
                    self.do_merge(func, block_id, target);
                    merged = true;
                    self.stats.blocks_merged += 1;
                    self.stats.gas_saved += 8;
                    break;
                }
            }
        }
    }

    /// Checks if block_id can be merged with its successor.
    /// Returns the target block if merge is possible.
    fn can_merge(&self, func: &Function, block_id: BlockId) -> Option<BlockId> {
        let block = &func.blocks[block_id];

        let Terminator::Jump(target) = block.terminator.as_ref()? else {
            return None;
        };

        if *target == block_id {
            return None;
        }

        if *target == func.entry_block {
            return None;
        }

        let target_block = &func.blocks[*target];
        if target_block.predecessors.len() != 1 {
            return None;
        }

        if target_block.predecessors[0] != block_id {
            return None;
        }

        for &inst_id in &target_block.instructions {
            let InstKind::Phi(incoming) = &func.instructions[inst_id].kind else {
                continue;
            };
            if !incoming.iter().any(|(pred, _)| *pred == block_id) {
                return None;
            }
        }

        Some(*target)
    }

    /// Merges block_id with target, appending target's instructions and terminator to block_id.
    fn do_merge(&self, func: &mut Function, block_id: BlockId, target: BlockId) {
        let phi_replacements = self.fold_target_phis_for_merge(func, block_id, target);
        let target_instructions: Vec<_> = func.blocks[target]
            .instructions
            .iter()
            .copied()
            .filter(|&inst_id| !matches!(func.instructions[inst_id].kind, InstKind::Phi(_)))
            .collect();
        let target_terminator = func.blocks[target].terminator.take();
        let target_successors =
            target_terminator.as_ref().map(Terminator::successors).unwrap_or_default();

        func.blocks[block_id].instructions.extend(target_instructions);
        func.blocks[block_id].terminator = target_terminator;

        for &succ in &target_successors {
            self.redirect_target_phi_incoming(func, target, succ, &[block_id]);

            let succ_block = &mut func.blocks[succ];
            for pred in &mut succ_block.predecessors {
                if *pred == target {
                    *pred = block_id;
                }
            }
        }

        func.blocks[target].instructions.clear();
        func.blocks[target].terminator = Some(Terminator::Invalid);
        func.blocks[target].predecessors.clear();

        Self::replace_uses(func, &phi_replacements);
    }

    fn fold_target_phis_for_merge(
        &self,
        func: &Function,
        pred: BlockId,
        target: BlockId,
    ) -> FxHashMap<ValueId, ValueId> {
        let mut replacements = FxHashMap::default();
        for &inst_id in &func.blocks[target].instructions {
            let InstKind::Phi(incoming) = &func.instructions[inst_id].kind else {
                continue;
            };
            let Some(phi_value) = Self::find_inst_result(func, inst_id) else {
                continue;
            };
            let Some((_, incoming_value)) =
                incoming.iter().find(|(incoming_pred, _)| *incoming_pred == pred)
            else {
                continue;
            };
            replacements.insert(phi_value, *incoming_value);
        }
        replacements
    }

    fn find_inst_result(func: &Function, inst_id: crate::mir::InstId) -> Option<ValueId> {
        func.values.iter_enumerated().find_map(|(value, kind)| {
            matches!(kind, Value::Inst(inst) if *inst == inst_id).then_some(value)
        })
    }

    fn replace_uses(func: &mut Function, replacements: &FxHashMap<ValueId, ValueId>) {
        if replacements.is_empty() {
            return;
        }
        for inst in func.instructions.iter_mut() {
            Self::replace_inst_operands(&mut inst.kind, replacements);
        }
        for value in func.values.iter_mut() {
            if let Value::Phi { incoming, .. } = value {
                for (_, value) in incoming {
                    if let Some(&replacement) = replacements.get(value) {
                        *value = replacement;
                    }
                }
            }
        }
        for block in func.blocks.iter_mut() {
            if let Some(term) = &mut block.terminator {
                Self::replace_terminator_operands(term, replacements);
            }
        }
    }

    fn replace_inst_operands(kind: &mut InstKind, replacements: &FxHashMap<ValueId, ValueId>) {
        kind.visit_operands_mut(|value| {
            if let Some(&replacement) = replacements.get(value) {
                *value = replacement;
            }
        });
    }

    fn replace_terminator_operands(
        term: &mut Terminator,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) {
        let replace = |value: &mut ValueId| {
            if let Some(&replacement) = replacements.get(value) {
                *value = replacement;
            }
        };

        match term {
            Terminator::Jump(_) | Terminator::Stop | Terminator::Invalid => {}
            Terminator::Branch { condition, .. } => replace(condition),
            Terminator::Switch { value, cases, .. } => {
                replace(value);
                for (case_value, _) in cases {
                    replace(case_value);
                }
            }
            Terminator::Return { values } => {
                for value in values {
                    replace(value);
                }
            }
            Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
                replace(offset);
                replace(size);
            }
            Terminator::SelfDestruct { recipient } => replace(recipient),
        }
    }

    /// Eliminates empty blocks that only contain an unconditional jump.
    fn eliminate_empty_blocks(&mut self, func: &mut Function) {
        let mut eliminated = true;
        while eliminated {
            eliminated = false;

            let block_ids: Vec<_> = func.blocks.indices().collect();
            for block_id in block_ids {
                if block_id == func.entry_block {
                    continue;
                }

                if self.is_empty_forwarder(func, block_id) {
                    self.eliminate_forwarder(func, block_id);
                    eliminated = true;
                    self.stats.empty_blocks_eliminated += 1;
                    self.stats.gas_saved += 8;
                    break;
                }
            }
        }
    }

    /// Checks if a block is an empty forwarder (no instructions, just a jump).
    fn is_empty_forwarder(&self, func: &Function, block_id: BlockId) -> bool {
        let block = &func.blocks[block_id];

        if !block.instructions.is_empty() {
            return false;
        }

        matches!(&block.terminator, Some(Terminator::Jump(target)) if *target != block_id)
    }

    /// Eliminates an empty forwarder block by redirecting its predecessors.
    fn eliminate_forwarder(&self, func: &mut Function, block_id: BlockId) {
        let target = match &func.blocks[block_id].terminator {
            Some(Terminator::Jump(t)) => *t,
            _ => return,
        };

        let predecessors: Vec<_> = func.blocks[block_id].predecessors.to_vec();
        self.redirect_target_phi_incoming(func, block_id, target, &predecessors);

        for pred_id in predecessors {
            self.redirect_terminator(func, pred_id, block_id, target);

            func.blocks[target].predecessors.push(pred_id);
        }

        func.blocks[target].predecessors.retain(|p| *p != block_id);

        func.blocks[block_id].instructions.clear();
        func.blocks[block_id].terminator = Some(Terminator::Invalid);
        func.blocks[block_id].predecessors.clear();
    }

    fn redirect_target_phi_incoming(
        &self,
        func: &mut Function,
        old_pred: BlockId,
        target: BlockId,
        new_preds: &[BlockId],
    ) {
        for &inst_id in &func.blocks[target].instructions {
            let InstKind::Phi(incoming) = &mut func.instructions[inst_id].kind else {
                continue;
            };

            let mut rewritten = Vec::with_capacity(incoming.len() + new_preds.len());
            for &(pred, value) in incoming.iter() {
                if pred == old_pred {
                    rewritten.extend(new_preds.iter().map(|&new_pred| (new_pred, value)));
                } else {
                    rewritten.push((pred, value));
                }
            }
            *incoming = rewritten;
        }
    }

    /// Redirects a terminator from old_target to new_target.
    fn redirect_terminator(
        &self,
        func: &mut Function,
        block_id: BlockId,
        old_target: BlockId,
        new_target: BlockId,
    ) {
        let block = &mut func.blocks[block_id];
        match &mut block.terminator {
            Some(Terminator::Jump(t)) if *t == old_target => {
                *t = new_target;
            }
            Some(Terminator::Branch { then_block, else_block, .. }) => {
                if *then_block == old_target {
                    *then_block = new_target;
                }
                if *else_block == old_target {
                    *else_block = new_target;
                }
            }
            Some(Terminator::Switch { default, cases, .. }) => {
                if *default == old_target {
                    *default = new_target;
                }
                for (_, target) in cases.iter_mut() {
                    if *target == old_target {
                        *target = new_target;
                    }
                }
            }
            _ => {}
        }
    }
}

/// Dead function elimination pass for a module.
#[derive(Debug, Default)]
pub struct DeadFunctionEliminator {
    /// Statistics from the last run.
    pub stats: CfgSimplifyStats,
}

/// Module pass for dead internal function elimination.
pub struct FunctionDcePass;

impl ModulePass for FunctionDcePass {
    fn name(&self) -> &str {
        "function-dce"
    }

    fn run(&mut self, module: &mut Module) -> bool {
        DeadFunctionEliminator::new().run(module) != 0
    }
}

impl DeadFunctionEliminator {
    /// Creates a new dead function eliminator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs dead function elimination on a module.
    /// Returns the number of functions eliminated.
    pub fn run(&mut self, module: &mut Module) -> usize {
        self.stats = CfgSimplifyStats::default();

        let reachable = self.find_reachable_functions(module);
        if reachable.is_empty() {
            return 0;
        }

        let dead_functions: Vec<FunctionId> =
            module.functions.indices().filter(|id| !reachable.contains(id)).collect();

        self.stats.dead_functions_eliminated = dead_functions.len();

        for func_id in &dead_functions {
            let func = &mut module.functions[*func_id];
            func.blocks.clear();
            func.instructions.clear();
            func.values.clear();
        }

        self.stats.dead_functions_eliminated
    }

    /// Finds all functions reachable from entry points.
    fn find_reachable_functions(&self, module: &Module) -> FxHashSet<FunctionId> {
        let mut reachable = FxHashSet::default();
        let mut worklist = VecDeque::new();

        for (func_id, func) in module.functions.iter_enumerated() {
            if self.is_entry_point(func) {
                reachable.insert(func_id);
                worklist.push_back(func_id);
            }
        }

        let call_graph = self.build_call_graph(module);

        while let Some(func_id) = worklist.pop_front() {
            if let Some(callees) = call_graph.get(&func_id) {
                for &callee in callees {
                    if reachable.insert(callee) {
                        worklist.push_back(callee);
                    }
                }
            }
        }

        reachable
    }

    /// Checks if a function is an entry point.
    fn is_entry_point(&self, func: &Function) -> bool {
        func.selector.is_some()
            || func.attributes.is_constructor
            || func.attributes.is_fallback
            || func.attributes.is_receive
    }

    /// Builds a call graph by analyzing internal calls in reachable MIR blocks.
    fn build_call_graph(&self, module: &Module) -> FxHashMap<FunctionId, FxHashSet<FunctionId>> {
        module
            .functions
            .iter_enumerated()
            .filter_map(|(func_id, func)| {
                let callees = find_internal_callees(func);
                (!callees.is_empty()).then_some((func_id, callees))
            })
            .collect()
    }
}

/// Call graph analysis for detecting recursive functions.
#[derive(Debug, Default)]
pub struct CallGraphAnalyzer {
    /// Functions that are recursive (directly or indirectly).
    pub recursive_functions: FxHashSet<FunctionId>,
    /// Call graph: caller -> set of callees.
    pub call_graph: FxHashMap<FunctionId, FxHashSet<FunctionId>>,
}

impl CallGraphAnalyzer {
    /// Creates a new call graph analyzer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Analyzes a module and detects recursive functions.
    pub fn analyze(&mut self, module: &Module) {
        self.build_call_graph(module);
        self.detect_recursion();
    }

    /// Builds the call graph from the module.
    fn build_call_graph(&mut self, module: &Module) {
        for (func_id, func) in module.functions.iter_enumerated() {
            let callees = self.find_callees(func, module);
            if !callees.is_empty() {
                self.call_graph.insert(func_id, callees);
            }
        }
    }

    /// Finds all functions called by this function.
    fn find_callees(&self, func: &Function, _module: &Module) -> FxHashSet<FunctionId> {
        find_internal_callees(func)
    }

    /// Detects recursive functions using DFS.
    fn detect_recursion(&mut self) {
        let func_ids: Vec<_> = self.call_graph.keys().copied().collect();

        for func_id in func_ids {
            if self.is_recursive(func_id, &mut FxHashSet::default()) {
                self.recursive_functions.insert(func_id);
            }
        }
    }

    /// Checks if a function is recursive (directly or indirectly).
    fn is_recursive(&self, func_id: FunctionId, visited: &mut FxHashSet<FunctionId>) -> bool {
        if !visited.insert(func_id) {
            return true;
        }

        if let Some(callees) = self.call_graph.get(&func_id) {
            for &callee in callees {
                if self.is_recursive(callee, visited) {
                    return true;
                }
            }
        }

        visited.remove(&func_id);
        false
    }

    /// Returns true if the function is recursive.
    #[must_use]
    pub fn is_function_recursive(&self, func_id: FunctionId) -> bool {
        self.recursive_functions.contains(&func_id)
    }
}

fn find_internal_callees(func: &Function) -> FxHashSet<FunctionId> {
    let mut callees = FxHashSet::default();
    for block in func.blocks.iter() {
        for &inst_id in &block.instructions {
            if let InstKind::InternalCall { function, .. } = func.instructions[inst_id].kind {
                callees.insert(function);
            }
        }
    }
    callees
}

/// Runs all CFG simplification passes on a function.
pub fn simplify_cfg(func: &mut Function) -> CfgSimplifyStats {
    let mut simplifier = CfgSimplifier::new();
    simplifier.run_to_fixpoint(func)
}

/// Rebuilds CFG edge lists from terminators and drops phi inputs from blocks
/// that are no longer predecessors.
pub fn repair_reachability_phis(func: &mut Function) {
    let mut edges = Vec::new();
    for (block, bb) in func.blocks.iter_enumerated() {
        if let Some(term) = &bb.terminator {
            edges.push((block, term.successors()));
        }
    }

    for block in func.blocks.iter_mut() {
        block.predecessors.clear();
    }

    for (block, successors) in edges {
        for succ in successors {
            func.blocks[succ].predecessors.push(block);
        }
    }

    for block_id in func.blocks.indices() {
        let predecessors = func.blocks[block_id].predecessors.clone();
        for &inst_id in &func.blocks[block_id].instructions {
            if let InstKind::Phi(incoming) = &mut func.instructions[inst_id].kind {
                incoming.retain(|(pred, _)| predecessors.contains(pred));
            }
        }
    }
}

/// Runs all CFG simplification passes on a module.
pub fn simplify_module_cfg(module: &mut Module) -> CfgSimplifyStats {
    let mut total_stats = CfgSimplifyStats::default();

    for func_id in module.functions.indices() {
        let func = &mut module.functions[func_id];
        let stats = simplify_cfg(func);
        total_stats.combine(&stats);
    }

    let mut dfe = DeadFunctionEliminator::new();
    dfe.run(module);
    total_stats.combine(&dfe.stats);

    total_stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{FunctionBuilder, Instruction, MirType, Value};
    use solar_interface::Ident;
    use solar_sema::hir::Visibility;

    #[test]
    fn dead_function_elimination_keeps_internal_call_targets() {
        let mut module = Module::new(Ident::DUMMY);

        let live_helper = module.add_function(Function::new(Ident::DUMMY));
        let dead_helper = module.add_function(Function::new(Ident::DUMMY));

        let mut entry = Function::new(Ident::DUMMY);
        entry.selector = Some([0, 0, 0, 1]);
        entry.attributes.visibility = Visibility::Public;
        {
            let mut builder = FunctionBuilder::new(&mut entry);
            let value = builder.internal_call(live_helper, Vec::new(), Some(MirType::uint256()), 1);
            builder.ret([value]);
        }
        let entry = module.add_function(entry);

        {
            let mut builder = FunctionBuilder::new(module.function_mut(live_helper));
            let value = builder.imm_u64(1);
            builder.ret([value]);
        }
        {
            let mut builder = FunctionBuilder::new(module.function_mut(dead_helper));
            let value = builder.imm_u64(2);
            builder.ret([value]);
        }

        let mut dfe = DeadFunctionEliminator::new();
        assert_eq!(dfe.run(&mut module), 1);

        assert!(!module.function(entry).blocks.is_empty());
        assert!(!module.function(live_helper).blocks.is_empty());
        assert!(module.function(dead_helper).blocks.is_empty());
        assert!(module.function(dead_helper).instructions.is_empty());
        assert!(module.function(dead_helper).values.is_empty());
    }

    #[test]
    fn empty_forwarder_rewrites_target_phi_incoming() {
        let mut func = Function::new(Ident::DUMMY);
        let forwarder;
        let direct;
        let target;
        let value;
        let other;
        {
            let mut builder = FunctionBuilder::new(&mut func);
            forwarder = builder.create_block();
            direct = builder.create_block();
            target = builder.create_block();

            value = builder.imm_u64(42);
            let cond = builder.imm_bool(true);
            builder.branch(cond, forwarder, direct);

            builder.switch_to_block(direct);
            let seven = builder.imm_u64(7);
            other = builder.add(seven, value);
            builder.jump(target);

            builder.switch_to_block(forwarder);
            builder.jump(target);
        }

        let phi_inst = func.alloc_inst(Instruction::new(
            InstKind::Phi(vec![(forwarder, value), (direct, other)]),
            Some(MirType::uint256()),
        ));
        let phi_value = func.alloc_value(Value::Inst(phi_inst));
        func.blocks[target].instructions.push(phi_inst);
        func.blocks[target].terminator =
            Some(Terminator::Return { values: vec![phi_value].into() });

        let mut simplifier = CfgSimplifier::new();
        simplifier.run_to_fixpoint(&mut func);

        assert!(matches!(func.blocks[forwarder].terminator, Some(Terminator::Invalid)));
        let phi_inst = func.blocks[target].instructions[0];
        let InstKind::Phi(incoming) = &func.instructions[phi_inst].kind else {
            panic!("expected phi");
        };
        assert_eq!(incoming.as_slice(), &[(func.entry_block, value), (direct, other)]);
    }

    #[test]
    fn block_merge_rewrites_successor_phi_incoming() {
        let mut func = Function::new(Ident::DUMMY);
        let source;
        let middle;
        let other;
        let exit;
        let result;
        let other_value;
        {
            let mut builder = FunctionBuilder::new(&mut func);
            source = builder.create_block();
            middle = builder.create_block();
            other = builder.create_block();
            exit = builder.create_block();

            let cond = builder.imm_bool(true);
            builder.branch(cond, source, other);

            builder.switch_to_block(source);
            builder.jump(middle);

            builder.switch_to_block(middle);
            let one = builder.imm_u64(1);
            let two = builder.imm_u64(2);
            result = builder.add(one, two);
            builder.jump(exit);

            builder.switch_to_block(other);
            let three = builder.imm_u64(3);
            let four = builder.imm_u64(4);
            other_value = builder.add(three, four);
            builder.jump(exit);
        }

        let phi_inst = func.alloc_inst(Instruction::new(
            InstKind::Phi(vec![(middle, result), (other, other_value)]),
            Some(MirType::uint256()),
        ));
        let phi_value = func.alloc_value(Value::Inst(phi_inst));
        func.blocks[exit].instructions.push(phi_inst);
        func.blocks[exit].terminator = Some(Terminator::Return { values: vec![phi_value].into() });

        let mut simplifier = CfgSimplifier::new();
        simplifier.run_to_fixpoint(&mut func);

        assert!(matches!(func.blocks[middle].terminator, Some(Terminator::Invalid)));
        let phi_inst = func.blocks[exit].instructions[0];
        let InstKind::Phi(incoming) = &func.instructions[phi_inst].kind else {
            panic!("expected phi");
        };
        assert_eq!(incoming.as_slice(), &[(source, result), (other, other_value)]);
    }
}
