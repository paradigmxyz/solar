//! Drops coverage counters whose hits another counter has.
//!
//! Lowering gives every statement, leaf condition, and path of the source a counter, so the map
//! stays exact wherever the code halts. Most of those counters sit together: a statement counter
//! and the counter of the condition after it, with nothing between them that can halt. They are
//! hit as often as the first of them, so the pass drops all but that counter and records each drop
//! as `[dropped, kept]`. Before it evaluates the map, a consumer sets the hits of the dropped
//! counter to those of the kept one, in that code object only, so the result stays exact.
//!
//! A probe starts a segment of a block. The segment ends at an instruction that can halt: a call, a
//! creation, a check that reverts, or a write to storage or a log, which stops the frame of a
//! static call; out-of-gas is ignored. The first counter of a segment is its leader, and the other
//! probes of the segment follow it. A counter `c` aliases the counter `k` when `k` leads the
//! segment of every probe of `c`, and the probes of `k` are in exactly the segments of the probes
//! of `c`, so both are hit exactly as often as their segments run. A counter that appears twice in
//! one segment keeps its probes. MC/DC probes keep theirs too: they update the state of a
//! decision, and halt nothing.
//!
//! The pass runs last, after the order of the instructions is final.

use crate::mir::{
    EffectKind, InstKind, Module,
    pass::{MirPass, ModuleAnalyses},
};
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_evmir::debug::DebugLocKind;
use solar_sema::Gcx;

/// Drops the counters that another counter always has the hits of.
pub(crate) struct CoverageAlias;

impl MirPass for CoverageAlias {
    fn name(&self) -> &'static str {
        "coverage-alias"
    }

    fn is_enabled(&self, gcx: Gcx<'_>, _module: &Module) -> bool {
        gcx.sess.opts.unstable.instrument_coverage
    }

    fn is_contract_wide(&self) -> bool {
        true
    }

    fn is_required(&self) -> bool {
        true
    }

    fn preserves_debugify_sites(&self) -> bool {
        true
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module, _analyses: &mut ModuleAnalyses) -> bool {
        let aliases = find_aliases(module);
        if aliases.is_empty() {
            return false;
        }
        let dropped = aliases.iter().map(|&[dropped, _]| dropped).collect::<FxHashSet<_>>();
        for func in &mut module.functions {
            for block in func.blocks.indices() {
                let mut instructions = std::mem::take(&mut func.blocks[block].instructions);
                instructions.retain(|&inst| {
                    let drop = matches!(func.inst(inst).kind, InstKind::Coverage(id) if dropped.contains(&id));
                    if drop {
                        func.inst_mut(inst).metadata.drop_debug_loc(DebugLocKind::Dropped);
                    }
                    !drop
                });
                func.blocks[block].instructions = instructions;
            }
        }
        module.coverage_aliases.extend(aliases);
        true
    }
}

/// Where a counter is hit: the segment of each probe and the leader of that segment.
#[derive(Default)]
struct Probes {
    segments: Vec<(u32, u32)>,
}

impl Probes {
    fn segment_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.segments.iter().map(|&(segment, _)| segment)
    }
}

fn find_aliases(module: &Module) -> Vec<[u32; 2]> {
    let mut probes = FxHashMap::<u32, Probes>::default();
    let mut segment = 0;
    for func in &module.functions {
        for block in func.blocks.iter() {
            segment += 1;
            let mut leader = None;
            for &inst in &block.instructions {
                let kind = &func.inst(inst).kind;
                match *kind {
                    InstKind::Coverage(id) if !is_mcdc(id) => {
                        let leader = *leader.get_or_insert(id);
                        probes.entry(id).or_default().segments.push((segment, leader));
                    }
                    _ if can_halt(kind) => {
                        segment += 1;
                        leader = None;
                    }
                    _ => {}
                }
            }
        }
    }
    let mut aliases = probes
        .iter()
        .filter_map(|(&counter, probe)| {
            let (_, leader) = *probe.segments.first()?;
            let same_leader = probe.segments.iter().all(|&(_, other)| other == leader);
            // The leader of every segment of the counter is a probe of its own, and the leader
            // has no probe in any other segment.
            (leader != counter
                && same_leader
                && probes[&leader].segment_ids().eq(probe.segment_ids()))
            .then_some([counter, leader])
        })
        .collect::<Vec<_>>();
    aliases.sort_unstable();
    aliases
}

/// Returns whether a probe is an MC/DC probe, which has bit 31 set.
fn is_mcdc(id: u32) -> bool {
    id & 1 << 31 != 0
}

/// Returns whether execution may stop at `kind`, as a callee that reverts or a failed check does.
/// A state write or a log stops a frame that a static call runs.
fn can_halt(kind: &InstKind) -> bool {
    kind.effects().control.any()
        || matches!(
            kind.effect_kind(),
            EffectKind::ExternalCall
                | EffectKind::ICall
                | EffectKind::Create
                | EffectKind::StorageWrite
                | EffectKind::TransientWrite
                | EffectKind::Log
        )
}
