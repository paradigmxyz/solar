//! Place each calldata read where the code that uses it runs: shared by a branch's paths, or
//! beside the uses a guard no longer shares.
//!
//! A word at a constant offset, such as an ABI head word, is read by the guard that checks it, by
//! the body that uses it, and in a dispatcher by every route. Two moves put those reads where the
//! backend can price them. Calldata reads cannot trap or change during a call, so both moves keep
//! every path's result.
//!
//! **Hoisting.** A word that every continuing path out of a branching block reads is read once in
//! that block, and each read the block dominates uses that value. A block that only reverts does
//! not count as a path, since a read there costs only the revert. A read common to the routes of a
//! dispatcher is then one read before the `switch` instead of one per route, which is the smaller
//! code, and the backend keeps it for the routes that use it. The target prices the hoist
//! ([`Target::hoisted_read_pays`]): one read, a `dup` per further reader, and the `dup` and `swap`
//! a word kept on the stack costs the words around it, against a read at each; too few readers do
//! not repay the longer live range, so their reads stay. A word already read in the block or above
//! it is left alone, and a block with a single continuing successor hoists nothing, which keeps a
//! guard's read apart from the body's.
//!
//! **Placement.** The guard's read and the body's are one value after CSE, and ABI lowering emits
//! them as one. That value is defined at the guard and lives to every later use, across calls and
//! branches. The backend prices keeping a word as one push and a cheap `dup` per use, and leaves
//! out what a long-lived word costs the words around it: the `dup` and `swap` that reach past it,
//! and the stack height every block between the guard and the last use carries. The price is right
//! only where the definition sits at the uses it serves. For each read in a block, the pass moves
//! every use outside that block to a copy of the read at the start of the nearest block outside
//! every loop that dominates those uses. A phi input counts as a use at the end of its
//! predecessor. The entry block keeps its own uses and the original, since the guard needs them,
//! so the body path reads the word a second time and the shorter live range repays it. Another
//! block's read moves entirely, and only when the block has no use of its own, so no path gains a
//! read. A word whose first use sits after a branch is then not read on the path that leaves at
//! the branch. The copy never lands inside a loop that does not contain the original.
//!
//! The pass runs once, after the last CSE and before the backend, so no later pass merges the
//! copies again.

use crate::{
    mir::{
        BlockId, Function, InstKind, Instruction, MirModuleTarget, Module, Value, ValueId,
        analysis::CfgInfo,
        pass::{MirPass, ModuleAnalyses, run_function_pass},
    },
    target::Target,
};
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::FxHashMap,
};
use solar_sema::Gcx;

pub(crate) struct PlaceReads;

impl MirPass for PlaceReads {
    fn name(&self) -> &'static str {
        "place-reads"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        let target = module.target(gcx);
        run_function_pass(module, analyses, |func, _| {
            let hoisted = hoist(func, &target);
            run(func) || hoisted
        })
    }
}

/// Reads each word that every path out of a branching block must read, once, in that block.
fn hoist(func: &mut Function, target: &Target) -> bool {
    // The constant offsets read anywhere, and the offsets each block reads.
    let mut offsets = Vec::new();
    let mut loads = Vec::new();
    for (block, data) in func.blocks.iter_enumerated() {
        for &inst in &data.instructions {
            if let Some(result) = func.inst_result_value(inst)
                && func.is_constant_calldata_load(result)
                && let InstKind::Load(address, _) = func.inst(inst).kind
                && let Some(offset) = func.value_u64(address)
            {
                offsets.push(offset);
                loads.push((block, offset, result));
            }
        }
    }
    offsets.sort_unstable();
    offsets.dedup();
    if offsets.is_empty() {
        return false;
    }
    let index = |offset: u64| offsets.binary_search(&offset).expect("offset was collected");
    let cfg = CfgInfo::new(func);
    // A block that only reverts reads nothing, and a read on its path costs only the revert.
    let continuing = func
        .blocks
        .iter()
        .map(|data| {
            let mut next = data.terminator.as_ref().map_or(Vec::new(), |t| t.successors().to_vec());
            next.sort_unstable();
            next.dedup();
            next.retain(|&next| {
                !func.blocks[next].terminator.as_ref().is_some_and(|t| t.reverts())
            });
            next
        })
        .collect::<IndexVec<BlockId, _>>();
    let mut local = index_vec![DenseBitSet::<usize>::new_empty(offsets.len()); func.blocks.len()];
    for &(block, offset, _) in &loads {
        local[block].insert(index(offset));
    }
    // The offsets read on every path from the start of each block, a greatest fixed point.
    let mut anticipated =
        index_vec![DenseBitSet::<usize>::new_filled(offsets.len()); func.blocks.len()];
    let mut changed = true;
    while changed {
        changed = false;
        for &block in cfg.rpo().iter().rev() {
            let mut next = continuing[block].iter().copied();
            let mut reads = DenseBitSet::new_empty(offsets.len());
            if let Some(first) = next.next() {
                reads.union(&anticipated[first]);
                for other in next {
                    reads.intersect(&anticipated[other]);
                }
            }
            reads.union(&local[block]);
            if reads != anticipated[block] {
                anticipated[block] = reads;
                changed = true;
            }
        }
    }
    let mut hoisted = false;
    for &block in cfg.rpo() {
        let next = &continuing[block];
        if next.len() < 2 {
            continue;
        }
        let mut reads = anticipated[next[0]].clone();
        for &other in &next[1..] {
            reads.intersect(&anticipated[other]);
        }
        for offset in reads.iter() {
            let offset = offsets[offset];
            // An offset read here or above already has a value.
            if loads
                .iter()
                .any(|&(home, read, _)| read == offset && cfg.dominators().dominates(home, block))
            {
                continue;
            }
            // The reads below the branch that the hoisted read replaces.
            let below = loads
                .iter()
                .filter(|&&(home, read, _)| {
                    read == offset && cfg.dominators().dominates(block, home)
                })
                .filter_map(|&(_, _, result)| match *func.value(result) {
                    Value::Inst(inst) => Some(inst),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if !target.hoisted_read_pays(offset, below.len() as u32) {
                continue;
            }
            // v = calldataload offset, before the branch
            let instruction = func.inst(below[0]);
            let mut copy = Instruction::new(instruction.kind.clone(), instruction.result_ty);
            copy.metadata.copy_debug_context(&instruction.metadata);
            for &other in &below[1..] {
                copy.metadata.merge_debug_context(&func.inst(other).metadata);
            }
            let (inst, value) = func.alloc_value_inst(copy);
            func.blocks[block].instructions.push(inst);
            // read dominated by the branch => the hoisted read
            let mut replacements = FxHashMap::default();
            for (home, read, result) in &mut loads {
                if *read == offset && cfg.dominators().dominates(block, *home) {
                    replacements.insert(*result, value);
                    *home = block;
                    *result = value;
                }
            }
            func.replace_uses(&replacements);
            hoisted = true;
        }
    }
    hoisted
}

fn run(func: &mut Function) -> bool {
    let mut loads = Vec::new();
    for (block, data) in func.blocks.iter_enumerated() {
        for &inst in &data.instructions {
            if let Some(result) = func.inst_result_value(inst)
                && func.is_constant_calldata_load(result)
            {
                loads.push((block, inst, result));
            }
        }
    }
    if loads.is_empty() {
        return false;
    }
    // The blocks that read each load; a phi reads its input on the edge from its predecessor.
    let mut uses = FxHashMap::<ValueId, SmallVec<[BlockId; 2]>>::default();
    for &(_, _, result) in &loads {
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
    for &(home, inst, result) in &loads {
        // Another block keeps its own read only while the entry block's guard still needs it.
        if home != BlockId::ENTRY && uses[&result].contains(&home) {
            continue;
        }
        let mut outside = uses[&result].iter().copied().filter(|&user| user != home);
        let Some(first) = outside.next() else { continue };
        let mut target = outside.fold(first, |a, b| cfg.dominators().common_dominator(a, b));
        while cfg.cyclic_blocks().contains(target)
            && let Some(idom) = cfg.dominators().idom(target)
            && idom != target
        {
            target = idom;
        }
        if target != home && cfg.dominators().dominates(home, target) {
            sunk.push((home, inst, result, target));
        }
    }
    if sunk.is_empty() {
        return false;
    }
    // original read => its block and its copy at the start of the block that dominates the rest
    let mut copies = FxHashMap::<ValueId, (BlockId, ValueId)>::default();
    for (home, inst, result, target) in sunk {
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
        copies.insert(result, (home, copy_value));
    }
    // use of the read outside its block => use of the copy
    for block in func.blocks.indices() {
        let redirect = |value: &mut ValueId| {
            if let Some(&(home, copy)) = copies.get(value)
                && home != block
            {
                *value = copy;
            }
        };
        for inst in func.blocks[block].instructions.clone() {
            match &mut func.inst_mut(inst).kind {
                InstKind::Phi(incoming) => {
                    for (pred, value) in incoming {
                        if let Some(&(home, copy)) = copies.get(value)
                            && home != *pred
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
