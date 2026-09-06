//! Cold classification for physical regions whose explicit paths end in revert.
//!
//! Revert exits seed a reverse-CFG worklist. A predecessor becomes proved only
//! after every edge in its nonempty successor list reaches a proved block. Each
//! block is queued once and each edge is consumed once, including repeated
//! conditional or indexed targets. Closed cycles and cycles with an exit remain
//! unproved: this least fixed point does not assume that a loop terminates.
//!
//! Existing cold hints survive, but do not seed the proof. In particular, a hinted
//! return, stop or computed jump cannot make its predecessors look like revert
//! paths. Interior raw control transfers and unknown opcodes also block proof,
//! since a structural terminator alone does not describe all their exits. Calls
//! may fail themselves; if they return, the proved continuation still reverts.
//!
//! The default pipeline annotates its final physical program after layout and
//! sharing, preserving executable placement. This pass neither inverts branches
//! nor rearranges or duplicates instructions. Explicit pipelines may consume the
//! hints earlier; their transforms must account for transfer and observation costs.
//! Work is linear in the allocated block domain, live instructions and CFG edges;
//! storage is linear in the block domain and edges, without stack-value analysis.

use super::{BlockId, EvmPass, InstKind, Module, TerminatorKind, verify::successors};
use crate::backend::evm::op;
use solar_data_structures::index::IndexVec;
use solar_sema::Gcx;
use std::collections::VecDeque;

pub(super) struct ColdBlocks;

impl EvmPass for ColdBlocks {
    fn name(&self) -> &'static str {
        "cold-blocks"
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        classify(module)
    }
}

fn classify(module: &mut Module) -> bool {
    let mut predecessors =
        IndexVec::<BlockId, Vec<BlockId>>::from_vec(vec![Vec::new(); module.blocks.len()]);
    let mut remaining = IndexVec::<BlockId, usize>::from_vec(vec![0; module.blocks.len()]);
    let mut pending = VecDeque::new();
    for id in module.block_ids() {
        let block = &module.blocks[id];
        if block.insts.iter().any(|inst| {
            matches!(inst.kind, InstKind::Op(code) if op::stack_io(code).is_none()
                || matches!(code, op::JUMP | op::JUMPI | op::STOP | op::RETURN
                    | op::REVERT | op::INVALID | op::SELFDESTRUCT))
        }) {
            continue;
        }
        if block.terminator.kind == TerminatorKind::Revert {
            pending.push_back(id);
        } else {
            for target in successors(&block.terminator.kind) {
                predecessors[target].push(id);
                remaining[id] += 1;
            }
        }
    }
    let mut changed = false;
    while let Some(id) = pending.pop_front() {
        changed |= !module.blocks[id].cold;
        // block -> block [cold]
        module.blocks[id].cold = true;
        for &source in &predecessors[id] {
            remaining[source] -= 1;
            if remaining[source] == 0 {
                pending.push_back(source);
            }
        }
    }
    changed
}
