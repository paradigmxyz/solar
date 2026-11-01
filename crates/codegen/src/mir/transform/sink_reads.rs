//! Sink calldata reads out of the entry block, to the block that dominates their other uses.
//!
//! This pass undoes CSE's merge of the guard load with the body loads. The entry block runs on
//! every path, so a calldata word at a constant offset read there, such as an ABI head word, is
//! read by the guard that checks it and again by the body. CSE merges the two into one value
//! defined in the entry block, which then lives from the guard to every later use, across calls
//! and branches. The backend prices keeping such a word as one push and a cheap `dup` per use, and
//! the cost of the extra `dup` and `swap` that a long-lived word forces on the words around it is
//! not part of that price: kept this way, aave-l2-encoder saves a little gas and grows 19% in
//! bytes. The pricing chooses well only when the word's definition sits at the uses it serves,
//! which is where the pass puts the copy. The stack copies of the merged word also differ from
//! function to function, where the guard-and-push code of two entries would otherwise repeat
//! exactly and merge.
//!
//! For each constant-offset `calldataload` in the entry block, the pass leaves the uses in the
//! entry block with it and moves every other use to a copy of the load at the start of the nearest
//! block outside every loop that dominates those uses, when that block is not the entry block. A
//! phi input counts as a use at the end of its predecessor. Nothing changes the calldata during
//! the call, so the copy reads the same word; the original keeps the entry block's uses, or
//! becomes dead and is removed by `dce`. Reads in other blocks stay as CSE left them: sinking
//! one that a branch separates from its other reads would only add a read.
//!
//! The pass runs once, after the last CSE and before the backend, so no later pass merges the
//! copies again. It never moves a read into a loop, and it adds no instruction when every use
//! already sits in the entry block.

use crate::mir::{
    BlockId, Function, InstKind, Instruction, Module, ValueId,
    analysis::CfgInfo,
    pass::{MirPass, ModuleAnalyses, run_function_pass},
};
use smallvec::SmallVec;
use solar_data_structures::map::FxHashMap;
use solar_sema::Gcx;

pub(crate) struct SinkReads;

impl MirPass for SinkReads {
    fn name(&self) -> &'static str {
        "sink-reads"
    }

    fn run_pass(&self, _: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        run_function_pass(module, analyses, |func, _| run(func))
    }
}

fn run(func: &mut Function) -> bool {
    let mut loads = Vec::new();
    for &inst in &func.blocks[BlockId::ENTRY].instructions {
        if let Some(result) = func.inst_result_value(inst)
            && func.is_constant_calldata_load(result)
        {
            loads.push((inst, result));
        }
    }
    if loads.is_empty() {
        return false;
    }
    // The blocks that read each load; a phi reads its input on the edge from its predecessor.
    let mut uses = FxHashMap::<ValueId, SmallVec<[BlockId; 2]>>::default();
    for &(_, result) in &loads {
        uses.insert(result, SmallVec::new());
    }
    for (block, data) in func.blocks.iter_enumerated() {
        let mut note = |value: ValueId, block: BlockId| {
            if let Some(blocks) = uses.get_mut(&value) {
                blocks.push(block);
            }
        };
        for &inst in &data.instructions {
            match &func.inst(inst).kind {
                InstKind::Phi(incoming) => {
                    for &(pred, operand) in incoming {
                        note(operand, pred);
                    }
                }
                kind => kind.visit_operands(|operand| note(operand, block)),
            }
        }
        if let Some(term) = &data.terminator {
            term.visit_operands(|operand| note(operand, block));
        }
    }
    let cfg = CfgInfo::new(func);
    let mut sunk = Vec::new();
    for &(inst, result) in &loads {
        let mut outside = uses[&result].iter().copied().filter(|&user| user != BlockId::ENTRY);
        let Some(first) = outside.next() else { continue };
        let mut target = outside.fold(first, |a, b| cfg.dominators().common_dominator(a, b));
        while cfg.cyclic_blocks().contains(target)
            && let Some(idom) = cfg.dominators().idom(target)
        {
            target = idom;
        }
        if target != BlockId::ENTRY && cfg.dominators().dominates(BlockId::ENTRY, target) {
            sunk.push((inst, result, target));
        }
    }
    if sunk.is_empty() {
        return false;
    }
    // original read => its copy at the start of the block that dominates the rest
    let mut copies = FxHashMap::<ValueId, ValueId>::default();
    for (inst, result, target) in sunk {
        let instruction = func.inst(inst);
        let mut copy = Instruction::new(instruction.kind.clone(), instruction.result_ty);
        copy.metadata.copy_debug_context(&instruction.metadata);
        let (copy_inst, copy_value) = func.alloc_value_inst(copy);
        let position = func.blocks[target]
            .instructions
            .iter()
            .position(|&other| !matches!(func.inst(other).kind, InstKind::Phi(_)))
            .unwrap_or(func.blocks[target].instructions.len());
        func.blocks[target].instructions.insert(position, copy_inst);
        copies.insert(result, copy_value);
    }
    // use of the read outside its block => use of the copy
    for block in func.blocks.indices() {
        let redirect = |value: &mut ValueId| {
            if block != BlockId::ENTRY
                && let Some(&copy) = copies.get(value)
            {
                *value = copy;
            }
        };
        for inst in func.blocks[block].instructions.clone() {
            match &mut func.inst_mut(inst).kind {
                InstKind::Phi(incoming) => {
                    for (pred, value) in incoming {
                        if *pred != BlockId::ENTRY
                            && let Some(&copy) = copies.get(value)
                        {
                            *value = copy;
                        }
                    }
                }
                kind => kind.visit_operands_mut(redirect),
            }
        }
        if let Some(term) = &mut func.blocks[block].terminator {
            term.visit_operands_mut(redirect);
        }
    }
    true
}
