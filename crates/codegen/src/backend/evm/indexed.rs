//! Bounded physical lowering of wide indexed jumps before compact assembly.
//!
//! Indexed jumps consume a zero-based, range-checked machine index. Small tables
//! are encoded by the assembler as packed label immediates. Larger tables are
//! split into balanced index ranges, with the upper edge subtracting its range
//! origin. Each leaf fits its selected address width in one EVM word. This covers all target
//! code-size limits without touching memory or introducing virtual values into EVM IR. All CFG
//! rewriting happens here, before the primitive compact stream is constructed.

use super::{ir, op};
use alloy_primitives::U256;
use std::borrow::Cow;

pub(crate) fn lower(module: &ir::Module, width: usize) -> Cow<'_, ir::Module> {
    let capacity = 32 / width;
    let pending: Vec<_> = module.block_ids().filter(|&id| matches!(&module.blocks[id].terminator.kind,ir::TerminatorKind::IndexedJump(targets) if targets.len()>capacity)).collect();
    if pending.is_empty() {
        return Cow::Borrowed(module);
    }
    let mut module = module.clone();
    let mut pending = pending;
    while let Some(id) = pending.pop() {
        let ir::TerminatorKind::IndexedJump(targets) = &module.blocks[id].terminator.kind else {
            continue;
        };
        if targets.len() <= capacity {
            continue;
        }
        let middle = targets.len() / 2;
        let left_targets = targets[..middle].to_vec();
        let right_targets = targets[middle..].to_vec();
        // left: indexed_jump <lower half>
        let left = module.blocks.push(ir::Block {
            terminator: ir::TerminatorKind::IndexedJump(left_targets).into(),
            ..Default::default()
        });
        // right: push <middle>; swap1; sub
        // indexed_jump <upper half>
        let right = module.blocks.push(ir::Block {
            insts: vec![
                ir::InstKind::Push(U256::from(middle)).into(),
                ir::InstKind::Swap(1).into(),
                ir::InstKind::Op(op::SUB).into(),
            ],
            terminator: ir::TerminatorKind::IndexedJump(right_targets).into(),
            ..Default::default()
        });
        // dup1; push <middle>; gt
        // jumpi <left>, <right>
        module.blocks[id].insts.extend([
            ir::InstKind::Dup(1).into(),
            ir::InstKind::Push(U256::from(middle)).into(),
            ir::InstKind::Op(op::GT).into(),
        ]);
        module.blocks[id].terminator = ir::TerminatorKind::JumpI(left, right).into();
        if let Some(layout) = &mut module.layout {
            layout.extend([left, right]);
        }
        pending.extend([left, right]);
    }
    Cow::Owned(module)
}
