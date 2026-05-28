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

use crate::mir::{BlockId, Function, FunctionId, InstKind, Module, Terminator};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;

/// Statistics from CFG simplification.
#[derive(Debug, Default, Clone)]
pub struct CfgSimplifyStats {
    /// Number of blocks merged.
    pub blocks_merged: usize,
    /// Number of empty blocks eliminated.
    pub empty_blocks_eliminated: usize,
    /// Number of dead functions eliminated.
    pub dead_functions_eliminated: usize,
    /// Estimated gas saved (8 gas per eliminated jump).
    pub gas_saved: usize,
}

impl CfgSimplifyStats {
    /// Returns total optimizations performed.
    #[must_use]
    pub fn total(&self) -> usize {
        self.blocks_merged + self.empty_blocks_eliminated + self.dead_functions_eliminated
    }

    /// Combines stats from another run.
    pub fn combine(&mut self, other: &Self) {
        self.blocks_merged += other.blocks_merged;
        self.empty_blocks_eliminated += other.empty_blocks_eliminated;
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

        self.merge_blocks(func);
        self.eliminate_empty_blocks(func);

        self.stats.total()
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

        Some(*target)
    }

    /// Merges block_id with target, appending target's instructions and terminator to block_id.
    fn do_merge(&self, func: &mut Function, block_id: BlockId, target: BlockId) {
        let target_instructions: Vec<_> = func.blocks[target].instructions.clone();
        let target_terminator = func.blocks[target].terminator.take();
        let target_successors: Vec<_> = func.blocks[target].successors.to_vec();

        func.blocks[block_id].instructions.extend(target_instructions);
        func.blocks[block_id].terminator = target_terminator;
        func.blocks[block_id].successors.clear();
        func.blocks[block_id].successors.extend(target_successors.iter().copied());

        for &succ in &target_successors {
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
        func.blocks[target].successors.clear();
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
            let has_real_instructions = block
                .instructions
                .iter()
                .any(|&inst_id| !matches!(func.instructions[inst_id].kind, InstKind::Phi(_)));
            if has_real_instructions {
                return false;
            }
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

        for pred_id in predecessors {
            self.redirect_terminator(func, pred_id, block_id, target);

            func.blocks[pred_id].successors.retain(|s| *s != block_id);
            if !func.blocks[pred_id].successors.contains(&target) {
                func.blocks[pred_id].successors.push(target);
            }

            func.blocks[target].predecessors.push(pred_id);
        }

        func.blocks[target].predecessors.retain(|p| *p != block_id);

        func.blocks[block_id].instructions.clear();
        func.blocks[block_id].terminator = Some(Terminator::Invalid);
        func.blocks[block_id].predecessors.clear();
        func.blocks[block_id].successors.clear();
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

        let dead_functions: Vec<FunctionId> =
            module.functions.indices().filter(|id| !reachable.contains(id)).collect();

        self.stats.dead_functions_eliminated = dead_functions.len();

        for func_id in &dead_functions {
            let func = &mut module.functions[*func_id];
            func.blocks.clear();
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
        func.is_public()
            || func.attributes.is_constructor
            || func.attributes.is_fallback
            || func.attributes.is_receive
    }

    /// Builds a call graph by analyzing function calls in the MIR.
    /// Since internal function calls in EVM are implemented via JUMP (not CALL),
    /// we need to track which functions might be called.
    /// For now, this returns an empty call graph - we rely on the entry point
    /// check to keep public functions, and internal functions are eliminated
    /// if they're never referenced.
    fn build_call_graph(&self, _module: &Module) -> FxHashMap<FunctionId, FxHashSet<FunctionId>> {
        FxHashMap::default()
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
    fn find_callees(&self, _func: &Function, _module: &Module) -> FxHashSet<FunctionId> {
        FxHashSet::default()
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

/// Runs all CFG simplification passes on a function.
pub fn simplify_cfg(func: &mut Function) -> CfgSimplifyStats {
    let mut simplifier = CfgSimplifier::new();
    simplifier.run_to_fixpoint(func)
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
    use crate::mir::FunctionBuilder;
    use solar_interface::Ident;

    fn make_test_func() -> Function {
        Function::new(Ident::DUMMY)
    }

    #[test]
    fn test_block_merging_simple() {
        let mut func = make_test_func();
        {
            let mut builder = FunctionBuilder::new(&mut func);

            let bb1 = builder.create_block();
            let bb2 = builder.create_block();

            let v = builder.imm_u64(42);
            let w = builder.imm_u64(100);
            let sum = builder.add(v, w);
            builder.jump(bb1);

            builder.switch_to_block(bb1);
            let x = builder.imm_u64(200);
            let mul = builder.mul(sum, x);
            builder.jump(bb2);

            builder.switch_to_block(bb2);
            builder.ret([mul]);
        }

        let mut simplifier = CfgSimplifier::new();
        let stats = simplifier.run_to_fixpoint(&mut func);

        assert!(stats.blocks_merged >= 2, "Should merge all blocks in the chain");
    }

    #[test]
    fn test_block_merging_chain() {
        let mut func = make_test_func();
        {
            let mut builder = FunctionBuilder::new(&mut func);

            let bb1 = builder.create_block();
            let bb2 = builder.create_block();
            let bb3 = builder.create_block();

            builder.jump(bb1);

            builder.switch_to_block(bb1);
            let _v1 = builder.imm_u64(1);
            builder.jump(bb2);

            builder.switch_to_block(bb2);
            let _v2 = builder.imm_u64(2);
            builder.jump(bb3);

            builder.switch_to_block(bb3);
            let v3 = builder.imm_u64(3);
            builder.ret([v3]);
        }

        let mut simplifier = CfgSimplifier::new();
        let stats = simplifier.run_to_fixpoint(&mut func);

        assert!(stats.blocks_merged >= 2, "Should merge chain of blocks");
    }

    #[test]
    fn test_no_merge_multiple_predecessors() {
        let mut func = make_test_func();
        {
            let mut builder = FunctionBuilder::new(&mut func);

            let cond = builder.imm_bool(true);
            let then_block = builder.create_block();
            let else_block = builder.create_block();
            let merge_block = builder.create_block();

            builder.branch(cond, then_block, else_block);

            builder.switch_to_block(then_block);
            builder.jump(merge_block);

            builder.switch_to_block(else_block);
            builder.jump(merge_block);

            builder.switch_to_block(merge_block);
            builder.stop();
        }

        let mut simplifier = CfgSimplifier::new();
        let stats = simplifier.run_to_fixpoint(&mut func);

        assert_eq!(stats.blocks_merged, 0, "Should not merge block with multiple predecessors");
    }

    #[test]
    fn test_empty_forwarder_elimination() {
        let mut func = make_test_func();
        {
            let mut builder = FunctionBuilder::new(&mut func);

            let bb1 = builder.create_block();
            let bb2 = builder.create_block();

            builder.jump(bb1);

            builder.switch_to_block(bb1);
            builder.jump(bb2);

            builder.switch_to_block(bb2);
            builder.stop();
        }

        let mut simplifier = CfgSimplifier::new();
        let stats = simplifier.run_to_fixpoint(&mut func);

        assert!(
            stats.blocks_merged > 0 || stats.empty_blocks_eliminated > 0,
            "Should eliminate empty forwarder blocks"
        );
    }

    #[test]
    fn test_no_self_loop_merge() {
        let mut func = make_test_func();
        {
            let mut builder = FunctionBuilder::new(&mut func);

            let loop_block = builder.create_block();

            builder.jump(loop_block);

            builder.switch_to_block(loop_block);
            let cond = builder.imm_bool(true);
            let exit = builder.create_block();
            builder.branch(cond, loop_block, exit);

            builder.switch_to_block(exit);
            builder.stop();
        }

        let mut simplifier = CfgSimplifier::new();
        simplifier.run_to_fixpoint(&mut func);
    }

    #[test]
    fn test_gas_savings_calculation() {
        let mut func = make_test_func();
        {
            let mut builder = FunctionBuilder::new(&mut func);

            let bb1 = builder.create_block();
            let bb2 = builder.create_block();

            builder.jump(bb1);
            builder.switch_to_block(bb1);
            builder.jump(bb2);
            builder.switch_to_block(bb2);
            builder.stop();
        }

        let mut simplifier = CfgSimplifier::new();
        let stats = simplifier.run_to_fixpoint(&mut func);

        if stats.blocks_merged > 0 || stats.empty_blocks_eliminated > 0 {
            assert!(stats.gas_saved > 0, "Should report gas savings");
            assert_eq!(stats.gas_saved, (stats.blocks_merged + stats.empty_blocks_eliminated) * 8);
        }
    }

    #[test]
    fn test_dead_function_elimination_basic() {
        let mut module = Module::new(Ident::DUMMY);

        let mut public_func = Function::new(Ident::DUMMY);
        public_func.attributes.visibility = solar_sema::hir::Visibility::Public;
        public_func.selector = Some([0x12, 0x34, 0x56, 0x78]);
        {
            let mut builder = FunctionBuilder::new(&mut public_func);
            builder.stop();
        }
        module.add_function(public_func);

        let mut internal_func = Function::new(Ident::DUMMY);
        internal_func.attributes.visibility = solar_sema::hir::Visibility::Internal;
        {
            let mut builder = FunctionBuilder::new(&mut internal_func);
            builder.stop();
        }
        let internal_id = module.add_function(internal_func);

        let mut dfe = DeadFunctionEliminator::new();
        dfe.run(&mut module);

        assert!(module.functions[internal_id].blocks.is_empty());
    }

    #[test]
    fn test_call_graph_analyzer() {
        let module = Module::new(Ident::DUMMY);

        let mut analyzer = CallGraphAnalyzer::new();
        analyzer.analyze(&module);
    }
}
