//! Prefer multiply referenced entry paths when laying out shared terminal tails.
//!
//! Tail merging can give one path a fallthrough while several equivalent entry points jump to
//! the same suffix. After ordinary and loop layout, this pass moves a later predecessor next to
//! the suffix when it has more than twice the old predecessor's static incoming edges. Edge
//! counts are a frequency proxy, not a profile or a guarantee for every input. The displaced
//! predecessor gains a jump; the preferred predecessor loses one. No code is duplicated.
//!
//! Only hot, non-loop predecessors ending in an unconditional jump qualify. The moved block
//! must already follow a physical terminal boundary, and the suffix must terminate execution.
//! This preserves other fallthroughs and never rearranges instructions or debug metadata.
//! Placement-sensitive deferred values and data references are excluded. A conservative size
//! bound must keep the entire module below the one-byte label-address boundary: exchanging
//! jumps then cannot increase PUSH widths. Larger modules retain the established layout.
//!
//! For small fixed layouts, place a STOP-terminated trace last when both trace boundaries
//! already terminate control flow. This removes the final STOP byte without adding a jump.
//! Modules followed by data or code retain their explicit stop.
//!
//! Run after structural sharing and loop layout. Size mode only moves a final STOP trace;
//! exchanging which caller pays a jump has no static size benefit.

use super::{
    EvmPass,
    block_layout::{estimated_block_size, is_physical_terminal_boundary},
    utils::{is_split_point, remap_block_order},
};
use crate::backend::evm::{
    ir::{BlockId, Module, PushValue, TerminatorKind},
    op,
};
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec};
use solar_sema::Gcx;

pub(super) struct TerminalLayout;

impl EvmPass for TerminalLayout {
    fn name(&self) -> &'static str {
        "terminal-layout"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        if !has_small_fixed_layout(gcx, module) {
            return false;
        }
        let mut order = module.blocks.indices().collect::<Vec<_>>();
        if !gcx.sess.opts.optimization.is_gas() {
            if !place_stop_last(module, &mut order) {
                return false;
            }
            // terminal traces; stop trace
            remap_block_order(module, &order);
            return true;
        }
        let mut incoming = IndexVec::from_vec(vec![0usize; module.blocks.len()]);
        for block in &module.blocks {
            if let Some(term) = &block.terminator {
                term.kind.visit_targets(|target| incoming[target] += 1);
            }
            for pair in block.instructions.windows(2) {
                if pair[1].as_evm_opcode() == Some(op::JUMPI)
                    && let Some(target) = pair[0].pushed_block()
                {
                    incoming[target] += 1;
                }
            }
        }
        let mut moved = DenseBitSet::new_empty(module.blocks.len());
        for caller in module.blocks.indices() {
            let block = &module.blocks[caller];
            if block.metadata.in_loop
                || block.metadata.hotness.is_cold()
                || !is_split_point(&block.instructions, block.instructions.len())
            {
                continue;
            }
            let Some(TerminatorKind::Jump(tail)) = block.terminator.as_ref().map(|term| &term.kind)
            else {
                continue;
            };
            let suffix = &module.blocks[*tail];
            if *tail == BlockId::ENTRY
                || moved.contains(*tail)
                || suffix.metadata.in_loop
                || suffix.metadata.hotness.is_cold()
                || !matches!(suffix.terminator.as_ref().map(|term| &term.kind),
                    Some(TerminatorKind::Op(opcode)) if op::is_terminal(*opcode))
            {
                continue;
            }
            let t = order.iter().position(|id| id == tail).unwrap();
            let c = order.iter().position(|&id| id == caller).unwrap();
            if t == 0 || c <= t {
                continue;
            }
            let previous = order[t - 1];
            let old = &module.blocks[previous];
            if old.metadata.in_loop
                || old.metadata.hotness.is_cold()
                || incoming[caller] <= incoming[previous].max(1).saturating_mul(2)
                || !matches!(old.terminator.as_ref().map(|term| &term.kind),
                    Some(TerminatorKind::Jump(target)) if target == tail)
                || !is_split_point(&old.instructions, old.instructions.len())
                || !is_physical_terminal_boundary(&module.blocks[order[c - 1]], Some(caller))
            {
                continue;
            }
            // previous; tail ... boundary; caller; jump tail
            // -> previous; jump tail; caller; tail ... boundary
            order.remove(c);
            order.insert(t, caller);
            moved.insert(*tail);
        }
        let stop_moved = place_stop_last(module, &mut order);
        if moved.is_empty() && !stop_moved {
            return false;
        }
        remap_block_order(module, &order);
        true
    }
}

/// Let a final STOP fall off the bytecode without breaking a trace's fallthroughs.
fn place_stop_last(module: &Module, order: &mut [BlockId]) -> bool {
    if module.code_follows || order.is_empty() {
        return false;
    }
    let Some(stop) = order.iter().rposition(|&block| {
        matches!(
            module.blocks[block].terminator.as_ref().map(|term| &term.kind),
            Some(TerminatorKind::Op(op::STOP))
        )
    }) else {
        return false;
    };
    if stop + 1 == order.len() {
        return false;
    }
    let Some(last) = order.last().copied() else { return false };
    if !is_physical_terminal_boundary(&module.blocks[last], None) {
        return false;
    }
    let Some(start) = (1..=stop).rev().find(|&position| {
        is_physical_terminal_boundary(&module.blocks[order[position - 1]], Some(order[position]))
    }) else {
        return false;
    };
    // boundary; trace; stop; remaining terminal traces
    // -> boundary; remaining terminal traces; trace; stop
    order[start..].rotate_left(stop + 1 - start);
    true
}

/// Establishes a label-width bound without assembling or examining optional debug information.
fn has_small_fixed_layout(gcx: Gcx<'_>, module: &Module) -> bool {
    if !module.data.is_empty() {
        return false;
    }
    let mut upper_bound = 0usize;
    for block in &module.blocks {
        if block.instructions.iter().any(|inst| {
            inst.deferred_push().is_some()
                || inst.immutable_push().is_some()
                || matches!(inst.value, Some(PushValue::Data(_)))
                || matches!(inst.as_evm_opcode(), Some(op::PC | op::CODESIZE))
        }) {
            return false;
        }
        // Count a destination label at every block and keep all transfers explicit. If the
        // bound fits one-byte addresses even with two-byte label pushes, their least fixed
        // point fits too. Rearrangement cannot increase the number of transfers or labels.
        upper_bound += estimated_block_size(gcx, block, None, true, module.code_follows);
        // The layout estimator omits STOP for a final block; count it here because every
        // block may have a physical successor after reordering.
        if matches!(
            block.terminator.as_ref().map(|term| &term.kind),
            Some(TerminatorKind::Op(op::STOP))
        ) {
            upper_bound += crate::target::Target::new(gcx).opcode(op::STOP).bytes as usize;
        }
        if upper_bound > usize::from(u8::MAX) {
            return false;
        }
    }
    true
}
