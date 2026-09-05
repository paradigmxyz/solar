//! EVM bytecode generation from MIR.
//!
//! This module generates EVM bytecode from MIR using:
//! - Liveness analysis to know when values die
//! - Phi elimination to convert SSA to parallel copies
//! - Stack scheduling to generate DUP/SWAP sequences
//! - EVM IR optimization, relocation, and byte encoding

use super::{
    DebugFunction, DebugFunctionExit, DebugInstruction,
    assembler::{
        ArtifactKind, Assembler, DeferredAlloc, DeferredConst, ImmutableRef, Label,
        PreparedAssembly,
    },
    ir,
    layout::{RelayoutAddress, preserves_push_width},
    op::{self, WORD_BYTES},
    stack::{
        MAX_STACK_ACCESS, MAX_STACK_DEPTH, OperandCostModel, OperandPlan, ScheduleCost,
        ScheduledOp, SpillSlot, StackModel, StackOp, StackScheduler, TargetSlot,
        cross_block_values, is_cross_block_recomputable_kind, is_rematerializable_leaf,
        rematerializable_nullary_opcode, rematerializable_nullary_value,
    },
};
use crate::{
    analysis::{
        AliasAnalysis, CallGraphInfo, CfgInfo, CopyDest, CopySource, Liveness, Loop, LoopAnalyzer,
        MemoryBase, ParallelCopy, PhiEliminator,
    },
    immutable::{
        immutable_push_type_size, immutable_staging_addr, immutable_staging_base,
        immutable_staging_end,
    },
    memory::EvmMemoryLayout,
    mir::{
        ArgIdx, BlockId, EffectKind, Function, FunctionId, ImmutableEncoding, ImmutableId, InstId,
        InstKind, MemoryRegion, MirPhase, MirType, Module, Terminator, Value, ValueId,
    },
    pass::run_pipeline,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_config::OptimizationMode;
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    index::{IndexVec, index_vec},
    map::{FxHashMap, FxHashSet},
};
use solar_sema::Gcx;
use std::{cell::OnceCell, collections::hash_map::Entry as StdEntry};

mod switch;

use self::switch::MAX_GAS_CODE_GROWTH;

/// A dynamic-length write to a low absolute base below this bound above
/// `HEAP_START` is treated as possibly reaching the spill area.
const SPILL_HAZARD_BOUND: u64 = 0x2000;

const STACK_PHI_LAYOUT_LIMIT: usize = 8;
const GLOBAL_STACK_LAYOUT_LIMIT: usize = 8;
const GLOBAL_STACK_MAX_ARGS: usize = 3;
const GLOBAL_STACK_MIN_BLOCKS: usize = 8;
const GLOBAL_STACK_MIN_ARG_USES: usize = 6;
const GLOBAL_STACK_DENSE_AMORTIZATION_BLOCKS: usize = 16;
const STACK_ARG_ROTATION_LIMIT: usize = 16;

#[derive(Default)]
struct GeneratedCode {
    bytecode: Vec<u8>,
    evm_ir: Option<ir::Module>,
    debug_info: Option<Vec<DebugInstruction>>,
}

struct PreparedDeploymentPrefix {
    assembly: PreparedAssembly,
    constructor_arg_offset: Option<DeferredConst>,
    runtime_offset: DeferredConst,
}

/// Describes the stack effect of an EVM instruction.
/// This is used to keep the scheduler's stack model in sync with the actual EVM stack.
#[derive(Clone, Copy, Debug)]
struct StackEffect {
    /// Number of values popped from the stack.
    pops: usize,
    /// Number of values pushed to the stack.
    pushes: usize,
}

/// What value to track for a pushed stack entry.
#[derive(Clone, Copy, Debug)]
enum StackPush {
    /// No value is pushed (pushes == 0).
    #[allow(dead_code)]
    None,
    /// Push a tracked ValueId (pushes == 1).
    Tracked(ValueId),
    /// Push an unknown/untracked value (pushes == 1).
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StaticCallStackWord {
    ReturnAddress,
    Argument(usize),
}

#[derive(Clone, Debug)]
struct StackArgRetentionPlan {
    retained: DenseBitSet<usize>,
    drain_ops: Vec<StackOp>,
    shuffle_ops: Vec<StackOp>,
}

/// Stack arguments whose static-frame stores are delayed until their first instruction use.
///
/// `args` follows physical stack order, highest argument index first. Values in `frame_values` are
/// used again and therefore receive a store immediately before that use; the others die on the
/// stack without ever occupying their declared frame slot.
#[derive(Clone, Debug)]
struct LazyStackArgPlan {
    args: Vec<(ArgIdx, ValueId)>,
    frame_values: DenseBitSet<ValueId>,
}

type CanonicalArgValues = IndexVec<ArgIdx, Option<ValueId>>;

struct StackArgUseInfo {
    use_counts: FxHashMap<ValueId, usize>,
    non_entry_uses: DenseBitSet<ValueId>,
    call_uses: DenseBitSet<ValueId>,
    entry_first_uses: FxHashMap<ValueId, usize>,
    first_entry_call: Option<usize>,
}

#[derive(Clone)]
struct SpillStore {
    value: ValueId,
    slot: SpillSlot,
    block: ir::BlockId,
    range: std::ops::Range<usize>,
}

/// A single-use call gas operand rebuilt at the call site.
struct LateGasOperand {
    subtracted: Option<U256>,
}

impl LazyStackArgPlan {
    fn values(&self) -> impl Iterator<Item = ValueId> + '_ {
        self.args.iter().map(|&(_, value)| value)
    }
}

/// A profitable static-call layout whose caller words stay below the
/// untracked return address until control returns.
#[derive(Clone, Debug)]
struct StaticCallStackPlan {
    prepare_ops: Vec<StackOp>,
    caller_stack: StackModel,
}

/// Stack-native exit signature for a non-recursive static callee.
#[derive(Clone, Copy, Debug)]
struct StackReturnPlan {
    /// Number of result words left on the physical stack.
    arity: usize,
    /// First local/spill byte in the original MIR frame layout.
    local_base: u64,
}

/// Caller-side binding of stack-returned tuple words to their multi-return
/// protocol loads.
struct StackResultProjection {
    /// The buffer-pointer read, its offset additions, and the extra-return
    /// loads; all skipped during emission.
    elided: Vec<InstId>,
    /// The adopted load result for each extra return index `1..arity`.
    extras: Vec<ValueId>,
}

/// Subset-invariant analyses shared by one resident-layout subset search.
struct ResidentSearchContext {
    /// Planned stack-phi edges, present when the function has phis.
    phi_plan: Option<StackPhiPlan>,
    /// CFG facts whose memoized dominators persist across candidates.
    cfg: CfgInfo,
    /// Operand occurrences per candidate value across the whole function.
    value_uses: FxHashMap<ValueId, usize>,
}

/// Complete stack calling convention selected for one non-recursive static callee.
#[derive(Clone, Debug)]
struct StaticCallAbi {
    /// Argument positions delivered above the return address. Arguments not selected here keep
    /// their static-frame homes, which is the conservative per-word spill fallback.
    stack_args: DenseBitSet<usize>,
    /// How the callee adopts the incoming argument tuple.
    entry: StaticCallEntry,
    /// Complete tuple returned above the preserved caller prefix, when profitable.
    returns: Option<StackReturnPlan>,
}

impl StaticCallAbi {
    fn new(arg_count: usize) -> Self {
        Self {
            stack_args: DenseBitSet::new_empty(arg_count),
            entry: StaticCallEntry::Stored,
            returns: None,
        }
    }
}

/// Callee-side realization of a [`StaticCallAbi`] entry signature.
#[derive(Clone, Debug, Default)]
enum StaticCallEntry {
    /// Store incoming stack arguments into their ordinary static-frame slots.
    #[default]
    Stored,
    /// Consume every incoming stack argument directly in the entry block.
    Direct(Vec<ValueId>),
    /// Keep a profitable subset resident through the complete callee CFG.
    Resident { values: Vec<ValueId>, layout: GlobalStackPlan },
    /// Consume the first use directly and materialize only values used again.
    Lazy(LazyStackArgPlan),
}

#[derive(Clone, Copy, Debug)]
struct ICallStackEdge {
    caller: FunctionId,
    callee: FunctionId,
    preserved_words: usize,
    argument_words: usize,
}

#[derive(Default)]
struct StackPhiPlan {
    entries: FxHashMap<BlockId, Vec<ValueId>>,
    edges: FxHashMap<BlockId, StackPhiEdge>,
    branch_edges: FxHashMap<BlockId, StackPhiBranch>,
    phi_edge_sources: FxHashMap<BlockId, Vec<ValueId>>,
}

#[derive(Clone, Debug)]
struct StackPhiEdge {
    sources: Vec<ValueId>,
    results: Vec<ValueId>,
}

#[derive(Clone, Debug)]
struct StackPhiBranch {
    then_edge: StackPhiEdge,
    else_edge: StackPhiEdge,
    union: Vec<ValueId>,
}

/// Returns true when a planned entry layout hands `value` to `block` on the stack, so the block
/// reads it from there and it needs no spill slot.
fn planned_entry_carries(
    stack_phi_plan: &StackPhiPlan,
    global_stack_plan: &GlobalStackPlan,
    block: BlockId,
    value: ValueId,
) -> bool {
    stack_phi_plan.entries.get(&block).is_some_and(|entry| entry.contains(&value))
        || global_stack_plan.entry(block).is_some_and(|entry| entry.contains(&value))
}

struct BranchPhiShape {
    then_results: Vec<ValueId>,
    else_results: Vec<ValueId>,
    edges: Vec<(BlockId, StackPhiEdge, StackPhiEdge)>,
}

fn union_values(first: &[ValueId], second: &[ValueId]) -> Vec<ValueId> {
    let mut union = first.to_vec();
    let mut available = FxHashMap::default();
    for &value in first {
        *available.entry(value).or_insert(0usize) += 1;
    }
    for &value in second {
        let count = available.entry(value).or_default();
        if *count == 0 {
            union.push(value);
        } else {
            *count -= 1;
        }
    }
    union
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SpillLiveRange {
    start: usize,
    end: usize,
}

struct SpillColor {
    values: DenseBitSet<ValueId>,
    ranges: FxHashMap<BlockId, SmallVec<[SpillLiveRange; 4]>>,
}

type SpillInterferences = FxHashMap<ValueId, SmallVec<[ValueId; 4]>>;

impl SpillColor {
    fn new(value_count: usize) -> Self {
        Self { values: DenseBitSet::new_empty(value_count), ranges: FxHashMap::default() }
    }

    fn accepts(
        &self,
        value: ValueId,
        ranges: &FxHashMap<BlockId, SpillLiveRange>,
        interferences: &SpillInterferences,
    ) -> bool {
        interferences
            .get(&value)
            .is_none_or(|conflicts| !conflicts.iter().any(|&other| self.values.contains(other)))
            && ranges.iter().all(|(block, candidate)| {
                self.ranges.get(block).is_none_or(|assigned| {
                    assigned
                        .iter()
                        .all(|range| candidate.end < range.start || range.end < candidate.start)
                })
            })
    }

    fn insert(&mut self, value: ValueId, ranges: &FxHashMap<BlockId, SpillLiveRange>) {
        self.values.insert(value);
        for (&block, &range) in ranges {
            self.ranges.entry(block).or_default().push(range);
        }
    }
}

/// Canonical argument layouts carried between MIR basic blocks.
///
/// A block-local scheduler normally discards its model at every join. Function
/// arguments are special: they have one identity on every incoming edge and can
/// always be rematerialized as a safe fallback. Agreeing on one layout for all
/// predecessors lets the first load remain stack-resident through diamonds and
/// loops instead of repeating `CALLDATALOAD` or frame `MLOAD` in every block.
#[derive(Clone, Debug, Default)]
struct GlobalStackPlan {
    entries: FxHashMap<BlockId, Vec<ValueId>>,
    aliases: FxHashMap<ValueId, ValueId>,
    /// Whether terminal successors need an explicit, exact entry layout.
    /// External arguments can reload in revert blocks; resident internal
    /// arguments have no memory fallback and therefore cannot ignore them.
    terminal_sensitive: bool,
}

impl GlobalStackPlan {
    fn analyze(func: &Function, liveness: &Liveness, stack_phi_plan: &StackPhiPlan) -> Self {
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
        // reject dense layout plans unless a long CFG can amortize them.
        let arg_use_count = arg_uses.iter().map(Vec::len).sum::<usize>();
        if arg_use_count < GLOBAL_STACK_MIN_ARG_USES
            || (entries.len() * 2 > cfg.reachable().count()
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
    fn analyze_resident_args(
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

    fn set_entry(
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

    fn entry(&self, block: BlockId) -> Option<&[ValueId]> {
        self.entries.get(&block).map(Vec::as_slice)
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn edge_layout(&self, func: &Function, term: &Terminator) -> Option<&[ValueId]> {
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

    fn branch_layouts(&self, term: &Terminator) -> Option<(&[ValueId], &[ValueId])> {
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

    fn switch_layouts(&self, term: &Terminator) -> Option<Vec<(BlockId, &[ValueId])>> {
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
    fn uniformly_carried_values(&self, func: &Function, term: &Terminator) -> Vec<ValueId> {
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

    fn is_terminal_block(func: &Function, block: BlockId) -> bool {
        matches!(
            func.blocks[block].terminator,
            Some(Terminator::Revert { .. } | Terminator::RevertReturndata | Terminator::Invalid)
        )
    }
}

impl StackPhiPlan {
    fn analyze(
        func: &Function,
        liveness: &Liveness,
        cold_functions: &DenseBitSet<FunctionId>,
    ) -> Self {
        StackPhiPlanner::new(func, cold_functions).plan(liveness)
    }

    fn edge_fits(edge: &StackPhiEdge, values: &[ValueId]) -> bool {
        let source_additions = values.iter().filter(|value| !edge.sources.contains(value)).count();
        let result_additions = values.iter().filter(|value| !edge.results.contains(value)).count();
        edge.sources.len().saturating_add(source_additions) <= MAX_STACK_ACCESS
            && edge.results.len().saturating_add(result_additions) <= MAX_STACK_ACCESS
    }

    fn merge_edge(edge: &mut StackPhiEdge, values: &[ValueId]) {
        let source_additions: Vec<_> =
            values.iter().copied().filter(|value| !edge.sources.contains(value)).collect();
        let result_additions: Vec<_> =
            values.iter().copied().filter(|value| !edge.results.contains(value)).collect();
        edge.sources.extend(source_additions);
        edge.results.extend(result_additions);
    }

    fn edge_sources(&self) -> FxHashMap<BlockId, Vec<ValueId>> {
        let mut sources: FxHashMap<BlockId, Vec<ValueId>> = self
            .edges
            .iter()
            .map(|(&block, edge)| (block, edge.sources.clone()))
            .chain(self.branch_edges.iter().map(|(&block, branch)| (block, branch.union.clone())))
            .collect();
        for (&block, phi_sources) in &self.phi_edge_sources {
            sources.insert(block, phi_sources.clone());
        }
        sources
    }

    /// Extends planned phi edges with the resident argument prefix required by
    /// the same target. Phi values stay nearest the top, preserving the
    /// existing loop/join schedule, while invariant arguments ride below them.
    fn merge_resident(&mut self, func: &Function, resident: &GlobalStackPlan) -> bool {
        for (&block, entry) in &self.entries {
            if let Some(values) = resident.entry(block) {
                let additions = values.iter().filter(|value| !entry.contains(value)).count();
                if entry.len().saturating_add(additions) > MAX_STACK_ACCESS {
                    return false;
                }
            }
        }
        for (&pred, edge) in &self.edges {
            let Some(term) = func.blocks[pred].terminator.as_ref() else { return false };
            if let Some(values) = resident.edge_layout(func, term)
                && !Self::edge_fits(edge, values)
            {
                return false;
            }
        }
        for (&pred, branch) in &self.branch_edges {
            let Some(term) = func.blocks[pred].terminator.as_ref() else { return false };
            let (then_values, else_values) =
                if let Some((then, else_)) = resident.branch_layouts(term) {
                    (then, else_)
                } else if let Some(values) = resident.edge_layout(func, term) {
                    (values, values)
                } else {
                    continue;
                };
            for (edge, values) in
                [(&branch.then_edge, then_values), (&branch.else_edge, else_values)]
            {
                if !Self::edge_fits(edge, values) {
                    return false;
                }
            }
        }

        for (&block, entry) in &mut self.entries {
            if let Some(values) = resident.entry(block) {
                let additions: Vec<_> =
                    values.iter().copied().filter(|value| !entry.contains(value)).collect();
                entry.extend(additions);
            }
        }

        for (&pred, edge) in &mut self.edges {
            let Some(term) = func.blocks[pred].terminator.as_ref() else { return false };
            if let Some(values) = resident.edge_layout(func, term) {
                Self::merge_edge(edge, values);
            }
        }
        for (&pred, branch) in &mut self.branch_edges {
            let Some(term) = func.blocks[pred].terminator.as_ref() else { return false };
            let (then_values, else_values) =
                if let Some((then, else_)) = resident.branch_layouts(term) {
                    (then, else_)
                } else if let Some(values) = resident.edge_layout(func, term) {
                    (values, values)
                } else {
                    continue;
                };
            for (edge, values) in
                [(&mut branch.then_edge, then_values), (&mut branch.else_edge, else_values)]
            {
                Self::merge_edge(edge, values);
            }
            branch.union = union_values(&branch.then_edge.sources, &branch.else_edge.sources);
        }
        true
    }
}

struct StackPhiPlanner<'a> {
    func: &'a Function,
    loops: Vec<Loop>,
    header_results: FxHashMap<BlockId, Vec<ValueId>>,
    definitions: IndexVec<ValueId, Option<BlockId>>,
    /// Functions whose every exit aborts; a tail call into one never returns to the words a
    /// carried stack leaves beneath it.
    cold_functions: &'a DenseBitSet<FunctionId>,
}

/// Longest entry layout `plan_live_joins` carries into a block.
const LIVE_JOIN_LAYOUT_LIMIT: usize = 12;

/// Most forward-and-backward rounds `plan_live_joins` spends converging its layouts.
const LIVE_JOIN_ROUNDS: usize = 64;

/// What one `plan_live_joins` run knows about the function before any layout exists: the
/// blocks it plans and the per-block facts every round reads and none changes.
struct LiveJoinFacts {
    /// The joins being planned.
    is_join: FxHashSet<BlockId>,
    /// Sibling arms of planned branches, with the branch and the join it enters.
    arms: FxHashMap<BlockId, (BlockId, BlockId)>,
    /// Branches that enter a planned join.
    planned_branches: DenseBitSet<BlockId>,
    /// The latches of every loop header among the joins.
    back_edges: FxHashMap<BlockId, Vec<BlockId>>,
    /// The non-phi operands a block reads that are live into it.
    own_uses: IndexVec<BlockId, Vec<ValueId>>,
    /// The values live both into and out of a block: what it may carry onward.
    live_through: IndexVec<BlockId, DenseBitSet<ValueId>>,
    /// The carriable results a block defines after its last internal call and keeps live at
    /// its exit, top of the stack first.
    defs: IndexVec<BlockId, Vec<ValueId>>,
    /// Blocks with an internal call, which drains whatever they entered with.
    has_call: DenseBitSet<BlockId>,
    /// The headers of the loops containing each block.
    loop_headers_of: IndexVec<BlockId, SmallVec<[BlockId; 2]>>,
    /// Blocks a branch carries its stack into: a single-predecessor block without phis or a
    /// junk-tolerant terminal.
    carries_arm: DenseBitSet<BlockId>,
    /// Every value a planned join's own instructions read.
    join_uses: FxHashMap<BlockId, FxHashSet<ValueId>>,
    /// A planned join's phi results, in instruction order.
    join_phis: FxHashMap<BlockId, Vec<ValueId>>,
}

/// The converging state of one `plan_live_joins` run: the layouts so far, what every block
/// carries out under them, and what every block wants carried in.
struct LiveJoinState {
    layouts: FxHashMap<BlockId, Vec<ValueId>>,
    resident_out: FxHashMap<BlockId, Vec<ValueId>>,
    wanted: IndexVec<BlockId, DenseBitSet<ValueId>>,
    /// The next `wanted` set under construction, swapped in when it differs.
    scratch: DenseBitSet<ValueId>,
    /// A successor's wants masked to what the block carries through.
    mask: DenseBitSet<ValueId>,
}

impl LiveJoinState {
    fn new(num_blocks: usize, num_values: usize) -> Self {
        Self {
            layouts: FxHashMap::default(),
            resident_out: FxHashMap::default(),
            wanted: IndexVec::from_vec(vec![DenseBitSet::new_empty(num_values); num_blocks]),
            scratch: DenseBitSet::new_empty(num_values),
            mask: DenseBitSet::new_empty(num_values),
        }
    }
}

impl<'a> StackPhiPlanner<'a> {
    fn new(func: &'a Function, cold_functions: &'a DenseBitSet<FunctionId>) -> Self {
        let mut loop_analyzer = LoopAnalyzer::new();
        let loop_info = loop_analyzer.analyze(func);
        let loops = loop_info.all_loops().cloned().collect();

        let mut definitions = index_vec![None; func.num_values()];
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                if let Some(value) = func.inst_result_value(inst_id) {
                    definitions[value] = Some(block_id);
                }
            }
        }
        let mut planner =
            Self { func, loops, header_results: FxHashMap::default(), definitions, cold_functions };
        planner.collect_header_results();
        planner
    }

    fn plan(&self, liveness: &Liveness) -> StackPhiPlan {
        let mut plan = StackPhiPlan::default();
        self.plan_live_joins(liveness, &mut plan);
        for loop_info in &self.loops {
            self.plan_loop(loop_info, liveness, &mut plan);
        }
        self.plan_branch_phi_joins(&mut plan);
        for block in self.func.blocks.indices() {
            self.plan_join(block, &mut plan);
        }
        plan
    }

    /// Plans entry layouts for the acyclic joins the phi planners left alone. A join keeps its
    /// phi results and the live-in values that are already on the stack at the exit of every
    /// predecessor, so a value defined before a diamond crosses it without a spill store and a
    /// reload on the far side, and no edge has to load anything it did not have. Residency is a
    /// static fixpoint over the planned layouts: a carried value stays resident through
    /// single-predecessor chains and planned joins until a call drains the stack. The sibling
    /// arm of a planned branch gets a layout of its own; a sibling that only aborts is entered
    /// with the carried words beneath it, and any other sibling starts from an empty stack.
    fn plan_live_joins(&self, liveness: &Liveness, plan: &mut StackPhiPlan) {
        let func = self.func;
        let mut loop_headers = DenseBitSet::new_empty(func.blocks.len());
        for loop_info in &self.loops {
            loop_headers.insert(loop_info.header);
        }
        let mut joins = Vec::new();
        for (block_id, block) in func.blocks.iter_enumerated() {
            if block.predecessors.len() < 2
                || plan.entries.contains_key(&block_id)
                || GlobalStackPlan::is_terminal_block(func, block_id)
            {
                continue;
            }
            let preds_ok = block.predecessors.iter().all(|&pred| {
                !plan.edges.contains_key(&pred)
                    && !plan.branch_edges.contains_key(&pred)
                    && match func.blocks[pred].terminator.as_ref() {
                        Some(Terminator::Jump(_)) => true,
                        Some(Terminator::Branch { then_block, else_block, .. }) => {
                            then_block != else_block
                        }
                        _ => false,
                    }
            });
            let phis = self.phi_insts(block);
            let phi_source_is_live_in = block.predecessors.iter().any(|&pred| {
                self.phi_sources_for_pred(&phis, pred).is_some_and(|sources| {
                    sources.iter().any(|&source| liveness.live_in(block_id).contains(source))
                })
            });
            if preds_ok
                && !phi_source_is_live_in
                && phis.len() <= LIVE_JOIN_LAYOUT_LIMIT
                && self.phi_result_values(&phis).is_some()
            {
                joins.push(block_id);
            }
        }
        if joins.is_empty() {
            return;
        }
        let is_join = joins.iter().copied().collect::<FxHashSet<_>>();

        // Branches into a planned join carry into their sibling arm too when only that branch
        // enters it.
        // A sibling arm receives exactly the words its branch sends to the join, so the branch
        // reaches it straight from the `JUMPI` with no cleanup of its own.
        let mut arms = FxHashMap::default();
        let mut planned_branches = DenseBitSet::new_empty(func.blocks.len());
        for &join in &joins {
            for &pred in func.blocks[join].predecessors.iter() {
                let Some(Terminator::Branch { then_block, else_block, .. }) =
                    func.blocks[pred].terminator.as_ref()
                else {
                    continue;
                };
                planned_branches.insert(pred);
                for arm in [*then_block, *else_block] {
                    if arm != join
                        && !is_join.contains(&arm)
                        && !loop_headers.contains(arm)
                        && !plan.entries.contains_key(&arm)
                        && func.blocks[arm].predecessors.as_slice() == [pred]
                        && self.phi_insts(&func.blocks[arm]).is_empty()
                        && !self.junk_tolerant_terminal(liveness, arm)
                    {
                        arms.insert(arm, (pred, join));
                    }
                }
            }
        }

        // A loop invariant is resident at the latch only once the header carries it, so a
        // header's layout is seeded from its forward predecessors alone; a word that then
        // fails to reach a latch is banned and the fixpoint reruns.
        let mut back_edges: FxHashMap<BlockId, Vec<BlockId>> = FxHashMap::default();
        for loop_info in &self.loops {
            back_edges
                .entry(loop_info.header)
                .or_default()
                .extend(loop_info.back_edges.iter().copied());
        }
        // Two phases: an optimistic one where a successor asks for everything it wants, so a
        // word can enter a chain of layouts that each depend on the next, then a precise one
        // where a successor asks only for its converged layout, pruning what nothing keeps.
        // A round sweeps the blocks forward, refreshing what each carries out and the layouts
        // that residency feeds, then backward, refreshing what each wants and the layouts
        // those wants prune. Every refresh reads the newest neighbors, so a word crosses a
        // whole chain of joins in one round rather than one join per round.
        let facts =
            self.live_join_facts(liveness, &joins, is_join, arms, planned_branches, back_edges);
        let order = func.blocks.indices().collect::<Vec<_>>();
        let mut banned: FxHashMap<BlockId, FxHashSet<ValueId>> = FxHashMap::default();
        let mut state = LiveJoinState::new(func.blocks.len(), func.num_values());
        let mut precise = false;
        for _ in 0..LIVE_JOIN_ROUNDS {
            let mut changed = false;
            for &block_id in &order {
                changed |= self.refresh_live_join_layout(
                    liveness, block_id, &banned, precise, &facts, &mut state,
                );
                changed |= self.refresh_resident_out(liveness, plan, block_id, &facts, &mut state);
            }
            for &block_id in order.iter().rev() {
                changed |= self.refresh_wanted(plan, block_id, precise, &facts, &mut state);
                changed |= self.refresh_live_join_layout(
                    liveness, block_id, &banned, precise, &facts, &mut state,
                );
            }
            if changed {
                continue;
            }
            // Under the converged layouts, every latch must deliver the header's words.
            for (&header, latches) in &facts.back_edges {
                let Some(layout) = state.layouts.get(&header) else { continue };
                let phis = facts.join_phis.get(&header).map(Vec::as_slice).unwrap_or_default();
                for &value in layout {
                    if !phis.contains(&value)
                        && !latches.iter().all(|latch| {
                            state.resident_out.get(latch).is_some_and(|list| list.contains(&value))
                        })
                        && banned.entry(header).or_default().insert(value)
                    {
                        changed = true;
                    }
                }
            }
            if changed {
                continue;
            }
            if precise {
                break;
            }
            precise = true;
        }
        let mut layouts = state.layouts;
        layouts.retain(|_, layout| !layout.is_empty());
        let mut join_layouts = layouts.clone();
        join_layouts.retain(|block, _| facts.is_join.contains(block));
        let mut arm_layouts = layouts;
        arm_layouts.retain(|block, _| facts.arms.contains_key(block));
        if join_layouts.is_empty() {
            return;
        }

        // A predecessor that cannot produce a join's layout drops that join; dropping only
        // shrinks the plan, so this converges.
        loop {
            let mut edges = FxHashMap::default();
            let mut branches = FxHashMap::default();
            let mut dropped = Vec::new();
            let mut planned = join_layouts.keys().copied().collect::<Vec<_>>();
            planned.sort_unstable_by_key(|block| block.index());
            'joins: for join in planned {
                let layout = &join_layouts[&join];
                for &pred in func.blocks[join].predecessors.iter() {
                    match func.blocks[pred].terminator.as_ref() {
                        Some(Terminator::Jump(_)) => {
                            let Some(sources) = self.layout_sources(join, layout, pred) else {
                                dropped.push(join);
                                continue 'joins;
                            };
                            edges.insert(pred, StackPhiEdge { sources, results: layout.clone() });
                        }
                        Some(Terminator::Branch { then_block, else_block, .. }) => {
                            if branches.contains_key(&pred) {
                                continue;
                            }
                            let mut planned_arms = [None, None];
                            for (slot, &arm) in
                                planned_arms.iter_mut().zip(&[*then_block, *else_block])
                            {
                                let Some(edge) =
                                    self.arm_edge(liveness, pred, arm, &join_layouts, &arm_layouts)
                                else {
                                    dropped.push(join);
                                    continue 'joins;
                                };
                                *slot = edge;
                            }
                            let [then_edge, else_edge] = planned_arms;
                            let junk = |other: &Option<StackPhiEdge>| {
                                let sources = other
                                    .as_ref()
                                    .map(|edge| edge.sources.clone())
                                    .unwrap_or_default();
                                StackPhiEdge { results: sources.clone(), sources }
                            };
                            let then_edge = then_edge.unwrap_or_else(|| junk(&else_edge));
                            let else_edge =
                                else_edge.unwrap_or_else(|| junk(&Some(then_edge.clone())));
                            // An arm holding every word of the other is the identity edge; its
                            // order is the one the branch shuffles to, so keep it verbatim.
                            let covers = |outer: &[ValueId], inner: &[ValueId]| {
                                inner.iter().all(|value| outer.contains(value))
                            };
                            let union = if covers(&else_edge.sources, &then_edge.sources) {
                                else_edge.sources.clone()
                            } else if covers(&then_edge.sources, &else_edge.sources) {
                                then_edge.sources.clone()
                            } else {
                                union_values(&then_edge.sources, &else_edge.sources)
                            };
                            if union.is_empty() || union.len() > MAX_STACK_ACCESS {
                                dropped.push(join);
                                continue 'joins;
                            }
                            branches.insert(pred, StackPhiBranch { then_edge, else_edge, union });
                        }
                        _ => {
                            dropped.push(join);
                            continue 'joins;
                        }
                    }
                }
            }
            if !dropped.is_empty() {
                for block in dropped {
                    join_layouts.remove(&block);
                }
                if join_layouts.is_empty() {
                    return;
                }
                continue;
            }

            for (join, layout) in join_layouts {
                plan.entries.insert(join, layout);
            }
            for &pred in branches.keys() {
                let Some(Terminator::Branch { then_block, else_block, .. }) =
                    func.blocks[pred].terminator.as_ref()
                else {
                    continue;
                };
                for arm in [*then_block, *else_block] {
                    if let Some(layout) = arm_layouts.get(&arm) {
                        plan.entries.insert(arm, layout.clone());
                    }
                }
            }
            for (pred, edge) in edges {
                plan.phi_edge_sources.insert(pred, Self::phi_sources_of(&edge));
                plan.edges.insert(pred, edge);
            }
            for (pred, branch) in branches {
                let mut sources = Self::phi_sources_of(&branch.then_edge);
                for value in Self::phi_sources_of(&branch.else_edge) {
                    if !sources.contains(&value) {
                        sources.push(value);
                    }
                }
                plan.phi_edge_sources.insert(pred, sources);
                plan.branch_edges.insert(pred, branch);
            }
            return;
        }
    }

    /// Gathers what every round of `plan_live_joins` reads and none changes.
    fn live_join_facts(
        &self,
        liveness: &Liveness,
        joins: &[BlockId],
        is_join: FxHashSet<BlockId>,
        arms: FxHashMap<BlockId, (BlockId, BlockId)>,
        planned_branches: DenseBitSet<BlockId>,
        back_edges: FxHashMap<BlockId, Vec<BlockId>>,
    ) -> LiveJoinFacts {
        let func = self.func;
        let count = func.blocks.len();
        let num_values = func.num_values();
        let mut own_uses = IndexVec::with_capacity(count);
        let mut live_through = IndexVec::with_capacity(count);
        let mut defs = IndexVec::with_capacity(count);
        let mut has_call = DenseBitSet::new_empty(count);
        let mut carries_arm = DenseBitSet::new_empty(count);
        for (block_id, block) in func.blocks.iter_enumerated() {
            let live_in = liveness.live_in(block_id);
            let live_out = liveness.live_out(block_id);
            let mut uses = Vec::new();
            let mut kept = Vec::new();
            for &inst in &block.instructions {
                let kind = &func.inst(inst).kind;
                if matches!(kind, InstKind::ICall { .. }) {
                    has_call.insert(block_id);
                    kept.clear();
                }
                if !matches!(kind, InstKind::Phi(_)) {
                    uses.extend(
                        kind.operands().into_iter().filter(|value| live_in.contains(*value)),
                    );
                }
                if let Some(result) = func.inst_result_value(inst)
                    && self.carriable(result)
                    && live_out.contains(result)
                {
                    kept.push(result);
                }
            }
            if let Some(term) = &block.terminator {
                uses.extend(term.operands().into_iter().filter(|value| live_in.contains(*value)));
            }
            uses.sort_unstable_by_key(|value| value.index());
            uses.dedup();
            let mut through = DenseBitSet::new_empty(num_values);
            for value in live_in.iter().filter(|&value| live_out.contains(value)) {
                through.insert(value);
            }
            live_through.push(through);
            // Layouts list the top of the stack first; a new definition lands on top.
            kept.reverse();
            own_uses.push(uses);
            defs.push(kept);
            if block.predecessors.len() == 1 && self.phi_insts(block).is_empty()
                || self.junk_tolerant_terminal(liveness, block_id)
            {
                carries_arm.insert(block_id);
            }
        }
        let mut loop_headers_of = IndexVec::from_vec(vec![SmallVec::new(); count]);
        for loop_info in &self.loops {
            for block in loop_info.blocks.iter() {
                loop_headers_of[block].push(loop_info.header);
            }
        }
        let join_uses = joins.iter().map(|&join| (join, self.block_uses(join))).collect();
        let join_phis = joins
            .iter()
            .map(|&join| {
                let phis = self.phi_insts(&func.blocks[join]);
                (join, self.phi_result_values(&phis).unwrap_or_default())
            })
            .collect();
        LiveJoinFacts {
            is_join,
            arms,
            planned_branches,
            back_edges,
            own_uses,
            live_through,
            defs,
            has_call,
            loop_headers_of,
            carries_arm,
            join_uses,
            join_phis,
        }
    }

    /// Refreshes the layout of a planned join or sibling arm from the newest residency and
    /// wants; returns whether it changed.
    fn refresh_live_join_layout(
        &self,
        liveness: &Liveness,
        block_id: BlockId,
        banned: &FxHashMap<BlockId, FxHashSet<ValueId>>,
        precise: bool,
        facts: &LiveJoinFacts,
        state: &mut LiveJoinState,
    ) -> bool {
        let func = self.func;
        let layout = if facts.is_join.contains(&block_id) {
            let join = block_id;
            let block = &func.blocks[join];
            let live_in = liveness.live_in(join);
            let latches = facts.back_edges.get(&join).map(Vec::as_slice).unwrap_or(&[]);
            let forward = block.predecessors.iter().copied().filter(|pred| !latches.contains(pred));
            // The first forward predecessor's stack order is the layout order, so that edge
            // needs no shuffle and the others usually little.
            let Some(first) = forward.clone().next() else { return false };
            // A wide join shuffles every predecessor into one order; a word the join only
            // passes on rarely pays that there. A loop header is different: its latches
            // return with the header's own order, so a word riding around the loop
            // shuffles nowhere.
            let wide = block.predecessors.len() > 2 && !facts.back_edges.contains_key(&join);
            let used_here = &facts.join_uses[&join];
            let wanted = &state.wanted[join];
            let mut carried = state
                .resident_out
                .get(&first)
                .into_iter()
                .flatten()
                .copied()
                .filter(|&value| {
                    live_in.contains(value)
                        && self.carriable(value)
                        && !banned.get(&join).is_some_and(|set| set.contains(&value))
                        && wanted.contains(value)
                        && (!precise || !wide || used_here.contains(&value))
                        && forward.clone().all(|pred| {
                            state.resident_out.get(&pred).is_some_and(|list| list.contains(&value))
                        })
                })
                .collect::<Vec<_>>();
            // Phi sources are the newest words of a predecessor, so the results ride on top.
            let mut phis = facts.join_phis[&join].clone();
            carried.truncate(LIVE_JOIN_LAYOUT_LIMIT - phis.len());
            phis.extend(carried);
            phis
        } else if let Some(&(pred, join)) = facts.arms.get(&block_id) {
            let arm = block_id;
            // The join's words plus whatever else the arm reads that the branch already
            // holds, in the order the predecessor is expected to hold them: the arm edge is
            // the branch's identity edge, so this order is what the branch shuffles to, and
            // matching the resident order keeps that shuffle empty on every execution. The
            // join edge reorders on its own path only.
            let sources = state
                .layouts
                .get(&join)
                .and_then(|layout| self.layout_sources(join, layout, pred))
                .unwrap_or_default();
            let live_in = liveness.live_in(arm);
            let resident = state.resident_out.get(&pred).map(Vec::as_slice).unwrap_or_default();
            let wanted = &state.wanted[arm];
            let mut carried = resident
                .iter()
                .copied()
                .filter(|&value| {
                    sources.contains(&value)
                        || (live_in.contains(value)
                            && self.carriable(value)
                            && wanted.contains(value))
                })
                .collect::<Vec<_>>();
            // Join words the predecessor does not hold are materialized for the branch.
            for &value in &sources {
                if !carried.contains(&value) {
                    carried.push(value);
                }
            }
            carried.truncate(LIVE_JOIN_LAYOUT_LIMIT.max(sources.len()));
            carried
        } else {
            return false;
        };
        if state.layouts.get(&block_id) == Some(&layout) {
            return false;
        }
        state.layouts.insert(block_id, layout);
        true
    }

    /// Refreshes the values on the stack at a block's exit under the newest layouts: a block
    /// starts from its planned entry, from what a single predecessor carries across a jump or
    /// a fully preserved branch, or from nothing; keeps what it defines; and drains everything
    /// but its later definitions at an internal call. Returns whether the set changed.
    fn refresh_resident_out(
        &self,
        liveness: &Liveness,
        plan: &StackPhiPlan,
        block_id: BlockId,
        facts: &LiveJoinFacts,
        state: &mut LiveJoinState,
    ) -> bool {
        let func = self.func;
        let block = &func.blocks[block_id];
        let incoming: &[ValueId] = if let Some(layout) = state.layouts.get(&block_id) {
            layout
        } else if let Some(entry) = plan.entries.get(&block_id) {
            entry
        } else if let [pred] = block.predecessors.as_slice()
            && let Some(term) = func.blocks[*pred].terminator.as_ref()
        {
            let carried = match term {
                Terminator::Jump(_) => true,
                Terminator::Branch { then_block, else_block, .. } => {
                    !facts.planned_branches.contains(*pred)
                        && facts.carries_arm.contains(*then_block)
                        && facts.carries_arm.contains(*else_block)
                }
                _ => false,
            };
            if carried {
                state.resident_out.get(pred).map(Vec::as_slice).unwrap_or_default()
            } else {
                &[]
            }
        } else {
            &[]
        };
        let defs = &facts.defs[block_id];
        let mut resident = Vec::with_capacity(defs.len() + incoming.len());
        if facts.has_call.contains(block_id) {
            resident.extend_from_slice(defs);
        } else {
            let live_out = liveness.live_out(block_id);
            resident.extend(defs.iter().copied().filter(|def| !incoming.contains(def)));
            resident.extend(incoming.iter().copied().filter(|value| live_out.contains(*value)));
        }
        if state.resident_out.get(&block_id) == Some(&resident) {
            return false;
        }
        state.resident_out.insert(block_id, resident);
        true
    }

    /// Refreshes the live-in values a block reads itself or carries on to a successor that
    /// reads them, so a layout never pays a shuffle for a word nothing downstream consumes on
    /// the stack. Returns whether the set changed.
    fn refresh_wanted(
        &self,
        plan: &StackPhiPlan,
        block_id: BlockId,
        precise: bool,
        facts: &LiveJoinFacts,
        state: &mut LiveJoinState,
    ) -> bool {
        let func = self.func;
        let block = &func.blocks[block_id];
        let live_through = &facts.live_through[block_id];
        let LiveJoinState { layouts, wanted, scratch, mask, .. } = state;
        scratch.clear();
        for &value in &facts.own_uses[block_id] {
            scratch.insert(value);
        }
        let want = |scratch: &mut DenseBitSet<ValueId>, layout: &[ValueId]| {
            for &value in layout {
                if live_through.contains(value) {
                    scratch.insert(value);
                }
            }
        };
        if let Some(term) = &block.terminator {
            let carried_succs: SmallVec<[BlockId; 2]> = match term {
                Terminator::Jump(target) => {
                    let target_block = &func.blocks[*target];
                    (target_block.predecessors.len() == 1
                        || layouts.contains_key(target)
                        || plan.entries.contains_key(target))
                    .then_some(*target)
                    .into_iter()
                    .collect()
                }
                Terminator::Branch { then_block, else_block, .. } => {
                    let arms = [*then_block, *else_block];
                    if facts.planned_branches.contains(block_id) {
                        arms.into_iter()
                            .filter(|arm| {
                                layouts.contains_key(arm) || plan.entries.contains_key(arm)
                            })
                            .collect()
                    } else if arms.iter().all(|&arm| facts.carries_arm.contains(arm)) {
                        arms.into_iter().collect()
                    } else {
                        SmallVec::new()
                    }
                }
                _ => SmallVec::new(),
            };
            for succ in carried_succs {
                // A planned join carries exactly its layout; a chained block carries
                // whatever it wants. The optimistic phase asks for wants everywhere, a
                // loop header included: its layout can only hold what the preheader
                // and the latches deliver, and those only carry what the header asks
                // for, so asking with the layout alone never bootstraps a loop-carried
                // word. The latch check and the precise phase prune what it costs.
                if let Some(layout) = layouts.get(&succ).filter(|_| precise) {
                    want(scratch, layout);
                } else if let Some(entry) = plan.entries.get(&succ) {
                    want(scratch, entry);
                } else {
                    mask.clone_from(&wanted[succ]);
                    mask.intersect(live_through);
                    scratch.union(mask);
                }
            }
        }
        // A word the enclosing loop carries around is wanted everywhere inside it:
        // the joins on the way to a latch must carry it, or the latch cannot deliver
        // it back to the header and the header drops it.
        for header in &facts.loop_headers_of[block_id] {
            if let Some(layout) = layouts.get(header) {
                want(scratch, layout);
            }
        }
        if wanted[block_id] == *scratch {
            return false;
        }
        std::mem::swap(&mut wanted[block_id], scratch);
        true
    }

    /// The words of an edge that feed phis rather than ride through unchanged. Only these skip
    /// their definition-time store: a carried live-in keeps its store, so a later block that
    /// finds it dropped can still reload it.
    fn phi_sources_of(edge: &StackPhiEdge) -> Vec<ValueId> {
        edge.sources
            .iter()
            .zip(&edge.results)
            .filter(|(source, result)| source != result)
            .map(|(&source, _)| source)
            .collect()
    }

    /// The edge a planned branch `pred` uses for one of its arms: the arm's join layout, the
    /// arm's own layout when only `pred` enters it, `Some(None)` for an aborting arm that
    /// tolerates the carried words, and an empty edge for anything else. A planned branch owns
    /// the phi copies of both arms, so an arm with phis must be a planned join whose layout
    /// this predecessor can produce; otherwise the branch cannot be planned.
    fn arm_edge(
        &self,
        liveness: &Liveness,
        pred: BlockId,
        arm: BlockId,
        join_layouts: &FxHashMap<BlockId, Vec<ValueId>>,
        arm_layouts: &FxHashMap<BlockId, Vec<ValueId>>,
    ) -> Option<Option<StackPhiEdge>> {
        if let Some(layout) = join_layouts.get(&arm) {
            let sources = self.layout_sources(arm, layout, pred)?;
            return Some(Some(StackPhiEdge { sources, results: layout.clone() }));
        }
        if let Some(layout) = arm_layouts.get(&arm) {
            return Some(Some(StackPhiEdge { sources: layout.clone(), results: layout.clone() }));
        }
        if self.junk_tolerant_terminal(liveness, arm) {
            return Some(None);
        }
        if !self.phi_insts(&self.func.blocks[arm]).is_empty() {
            return None;
        }
        Some(Some(StackPhiEdge { sources: Vec::new(), results: Vec::new() }))
    }

    /// Whether a block may be entered with arbitrary words beneath the stack it expects: it
    /// reads no live-in value and aborts, directly or through a cold tail call.
    fn junk_tolerant_terminal(&self, liveness: &Liveness, block: BlockId) -> bool {
        liveness
            .live_in(block)
            .iter()
            .all(|value| matches!(self.func.value(value), crate::mir::Value::Immediate(_)))
            && match &self.func.blocks[block].terminator {
                Some(
                    Terminator::Revert { .. } | Terminator::RevertReturndata | Terminator::Invalid,
                ) => true,
                Some(Terminator::TailCall { function, .. }) => {
                    self.cold_functions.contains(*function)
                }
                _ => false,
            }
    }

    /// The values a block's own instructions and terminator read.
    fn block_uses(&self, block_id: BlockId) -> FxHashSet<ValueId> {
        let block = &self.func.blocks[block_id];
        let mut uses = FxHashSet::default();
        for &inst in &block.instructions {
            let kind = &self.func.inst(inst).kind;
            if !matches!(kind, InstKind::Phi(_)) {
                uses.extend(kind.operands());
            }
        }
        if let Some(term) = &block.terminator {
            uses.extend(term.operands());
        }
        uses
    }

    /// Whether a layout may carry `value`: an instruction result that is cheaper to keep than
    /// to recompute.
    fn carriable(&self, value: ValueId) -> bool {
        matches!(
            self.func.value(value),
            crate::mir::Value::Inst(inst)
                if rematerializable_nullary_opcode(&self.func.inst(*inst).kind).is_none()
        )
    }

    /// The words `pred` places for `block`'s layout: a phi result comes from its incoming
    /// value for `pred`, every other value is itself.
    fn layout_sources(
        &self,
        block_id: BlockId,
        layout: &[ValueId],
        pred: BlockId,
    ) -> Option<Vec<ValueId>> {
        let block = &self.func.blocks[block_id];
        let phi_insts = self.phi_insts(block);
        let results = self.phi_result_values(&phi_insts)?;
        let incoming = self.phi_sources_for_pred(&phi_insts, pred)?;
        layout
            .iter()
            .map(|&value| match results.iter().position(|&result| result == value) {
                Some(index) => Some(incoming[index]),
                None => Some(value),
            })
            .collect()
    }

    /// Plans branch edges whose two destinations are phi-only blocks in one loop-shaped CFG.
    /// Other conditional joins keep the conservative spill path.
    fn plan_branch_phi_joins(&self, plan: &mut StackPhiPlan) {
        for branch_id in self.func.blocks.indices() {
            let Some(loop_info) = self.loops.iter().find(|loop_info| {
                loop_info.blocks.contains(branch_id)
                    && loop_info.header != branch_id
                    && loop_info.blocks.iter().any(|block| {
                        matches!(
                            self.func.blocks[block].terminator,
                            Some(Terminator::Branch { .. })
                        )
                    })
            }) else {
                continue;
            };
            let Some(Terminator::Branch { then_block, else_block, .. }) =
                self.func.blocks[branch_id].terminator.as_ref()
            else {
                continue;
            };
            if plan.entries.contains_key(then_block) || plan.entries.contains_key(else_block) {
                continue;
            }
            let Some(shape) = self.branch_phi_shape(loop_info, *then_block, *else_block) else {
                continue;
            };
            if shape.edges.iter().any(|(pred, _, _)| {
                plan.edges.contains_key(pred) || plan.branch_edges.contains_key(pred)
            }) {
                continue;
            }

            plan.entries.insert(*then_block, shape.then_results.clone());
            plan.entries.insert(*else_block, shape.else_results.clone());
            for (pred, then_edge, else_edge) in shape.edges {
                let branch = StackPhiBranch {
                    union: union_values(&then_edge.sources, &else_edge.sources),
                    then_edge,
                    else_edge,
                };
                plan.branch_edges.insert(pred, branch);
            }
        }
    }

    fn phi_results_for_only_block(&self, block_id: BlockId) -> Option<Vec<ValueId>> {
        let block = &self.func.blocks[block_id];
        let phi_insts = self.phi_insts(block);
        if phi_insts.is_empty() || phi_insts.len() != block.instructions.len() {
            return None;
        }
        self.phi_result_values(&phi_insts)
    }

    fn phi_sources_for_block_pred(&self, block_id: BlockId, pred: BlockId) -> Option<Vec<ValueId>> {
        let phi_insts = self.phi_insts(&self.func.blocks[block_id]);
        self.phi_sources_for_pred(&phi_insts, pred)
    }

    fn branch_phi_shape(
        &self,
        loop_info: &Loop,
        then_block: BlockId,
        else_block: BlockId,
    ) -> Option<BranchPhiShape> {
        if then_block == else_block
            || loop_info.blocks.contains(then_block) == loop_info.blocks.contains(else_block)
        {
            return None;
        }
        let then_results = self.phi_results_for_only_block(then_block)?;
        let else_results = self.phi_results_for_only_block(else_block)?;
        if then_results.is_empty()
            || else_results.is_empty()
            || then_results.len() > STACK_PHI_LAYOUT_LIMIT
            || else_results.len() > STACK_PHI_LAYOUT_LIMIT
        {
            return None;
        }

        let mut predecessors = self.func.blocks[then_block].predecessors.clone();
        for &pred in &self.func.blocks[else_block].predecessors {
            if !predecessors.contains(&pred) {
                predecessors.push(pred);
            }
        }
        if predecessors.is_empty() {
            return None;
        }
        let mut edges = Vec::with_capacity(predecessors.len());
        for pred in predecessors {
            let Some(Terminator::Branch { then_block: pred_then, else_block: pred_else, .. }) =
                self.func.blocks[pred].terminator.as_ref()
            else {
                return None;
            };
            if !loop_info.blocks.contains(pred)
                || !((*pred_then == then_block && *pred_else == else_block)
                    || (*pred_then == else_block && *pred_else == then_block))
            {
                return None;
            }
            // Emission applies `then_edge` to the predecessor's own `then_block`, so a
            // predecessor whose arms are reversed relative to the first branch carries its
            // layouts in its own orientation.
            let (pred_then_results, pred_else_results) = if *pred_then == then_block {
                (then_results.clone(), else_results.clone())
            } else {
                (else_results.clone(), then_results.clone())
            };
            let then_sources = self.phi_sources_for_block_pred(*pred_then, pred)?;
            let else_sources = self.phi_sources_for_block_pred(*pred_else, pred)?;
            if then_sources.len() > MAX_STACK_ACCESS || else_sources.len() > MAX_STACK_ACCESS {
                return None;
            }
            edges.push((
                pred,
                StackPhiEdge { sources: then_sources, results: pred_then_results },
                StackPhiEdge { sources: else_sources, results: pred_else_results },
            ));
        }
        Some(BranchPhiShape { then_results, else_results, edges })
    }

    fn collect_header_results(&mut self) {
        for loop_info in &self.loops {
            let block = &self.func.blocks[loop_info.header];
            let phi_insts = self.phi_insts(block);
            if let Some(results) = self.phi_result_values(&phi_insts) {
                self.header_results.insert(loop_info.header, results);
            }
        }
    }

    fn plan_loop(&self, loop_info: &Loop, liveness: &Liveness, plan: &mut StackPhiPlan) {
        let Some(preheader) = loop_info.preheader else {
            return;
        };
        if loop_info.back_edges.is_empty() {
            return;
        }
        if !matches!(self.func.blocks[preheader].terminator, Some(Terminator::Jump(target)) if target == loop_info.header)
        {
            return;
        }
        if let [latch] = loop_info.back_edges.as_slice()
            && *latch == loop_info.header
            && self.plan_conditional_self_loop(loop_info, preheader, liveness, plan)
        {
            return;
        }
        if loop_info.back_edges.iter().any(|&latch| {
            !matches!(self.func.blocks[latch].terminator, Some(Terminator::Jump(target)) if target == loop_info.header)
        }) {
            return;
        }
        if plan.edges.contains_key(&preheader)
            || loop_info.back_edges.iter().any(|latch| plan.edges.contains_key(latch))
        {
            return;
        }
        let has_branching_body = loop_info.blocks.iter().any(|block_id| {
            block_id != loop_info.header
                && matches!(self.func.blocks[block_id].terminator, Some(Terminator::Branch { .. }))
        });
        let has_nested_loop = self.loops.iter().any(|other| {
            other.header != loop_info.header && loop_info.blocks.contains(other.header)
        });
        if has_branching_body && !self.can_plan_branching_loop(loop_info) {
            return;
        }
        let block = &self.func.blocks[loop_info.header];
        let phi_insts = self.phi_insts(block);
        if phi_insts.is_empty() || phi_insts.len() > STACK_PHI_LAYOUT_LIMIT {
            return;
        }

        let Some(results) = self.phi_result_values(&phi_insts) else {
            return;
        };
        if results.len() > STACK_PHI_LAYOUT_LIMIT {
            return;
        }

        let mut carry_through = self.carry_through_values(loop_info);
        if has_branching_body {
            if has_nested_loop {
                carry_through.clear();
            } else {
                self.extend_live_across_exits(loop_info, liveness, &mut carry_through);
            }
        } else {
            self.extend_live_through_values(loop_info, &mut carry_through);
        }
        if carry_through.len() + results.len() > STACK_PHI_LAYOUT_LIMIT {
            return;
        }
        let mut entry = carry_through.clone();
        entry.extend(results.iter().copied());

        let mut edges = Vec::with_capacity(loop_info.back_edges.len() + 1);
        for pred in std::iter::once(preheader).chain(loop_info.back_edges.iter().copied()) {
            let Some(phi_sources) = self.phi_sources_for_pred(&phi_insts, pred) else {
                return;
            };
            if pred != preheader
                && !has_branching_body
                && phi_sources.iter().any(|&source| {
                    self.is_phi_value(source)
                        && !results.contains(&source)
                        && !self.is_loop_header_phi(source)
                })
            {
                return;
            }
            let mut sources = carry_through.clone();
            sources.extend(phi_sources);
            debug_assert_eq!(sources.len(), entry.len());
            edges.push((pred, sources));
        }

        plan.entries.insert(loop_info.header, entry.clone());
        for (pred, sources) in edges {
            plan.edges.insert(pred, StackPhiEdge { sources, results: entry.clone() });
        }
    }

    fn plan_conditional_self_loop(
        &self,
        loop_info: &Loop,
        preheader: BlockId,
        liveness: &Liveness,
        plan: &mut StackPhiPlan,
    ) -> bool {
        let header = loop_info.header;
        if loop_info.blocks.iter().any(|block| block != header)
            || plan.entries.contains_key(&header)
            || plan.edges.contains_key(&preheader)
            || plan.edges.contains_key(&header)
            || plan.branch_edges.contains_key(&header)
            || !self.loop_instructions_are_stack_safe(loop_info)
        {
            return false;
        }
        let Some(Terminator::Branch { then_block, else_block, .. }) =
            self.func.blocks[header].terminator.as_ref()
        else {
            return false;
        };
        let (self_is_then, exit) = match (*then_block == header, *else_block == header) {
            (true, false) => (true, *else_block),
            (false, true) => (false, *then_block),
            _ => return false,
        };
        if loop_info.blocks.contains(exit)
            || self.func.blocks[exit].predecessors.as_slice() != [header]
            || !self.phi_insts(&self.func.blocks[exit]).is_empty()
            || plan.entries.contains_key(&exit)
        {
            return false;
        }

        let phi_insts = self.phi_insts(&self.func.blocks[header]);
        if phi_insts.is_empty() || phi_insts.len() > STACK_PHI_LAYOUT_LIMIT {
            return false;
        }
        let Some(results) = self.phi_result_values(&phi_insts) else { return false };
        let mut carry_through = self.carry_through_values(loop_info);
        if carry_through.is_empty() {
            // Keep enclosing-loop phis resident; other live-outs can still spill.
            self.extend_live_across_exits(loop_info, liveness, &mut carry_through);
        }
        let mut entry = carry_through.clone();
        entry.extend(results.iter().copied());
        if entry.len() > STACK_PHI_LAYOUT_LIMIT {
            return false;
        }

        let Some(initial_phi_sources) = self.phi_sources_for_pred(&phi_insts, preheader) else {
            return false;
        };
        let Some(backedge_phi_sources) = self.phi_sources_for_pred(&phi_insts, header) else {
            return false;
        };
        let mut initial_sources = carry_through.clone();
        initial_sources.extend(initial_phi_sources);
        let mut backedge_sources = carry_through;
        backedge_sources.extend(backedge_phi_sources);
        if initial_sources.len() != entry.len()
            || backedge_sources.len() != entry.len()
            || initial_sources.len() > MAX_STACK_ACCESS
            || backedge_sources.len() > MAX_STACK_ACCESS
        {
            return false;
        }

        let exit_values = entry
            .iter()
            .copied()
            .filter(|value| liveness.live_in(exit).contains(*value))
            .collect::<Vec<_>>();
        let backedge = StackPhiEdge { sources: backedge_sources, results: entry.clone() };
        let exit_edge = StackPhiEdge { sources: exit_values.clone(), results: exit_values.clone() };
        let (then_edge, else_edge) =
            if self_is_then { (backedge, exit_edge) } else { (exit_edge, backedge) };
        let union = union_values(&then_edge.sources, &else_edge.sources);
        if union.is_empty() || union.len() > MAX_STACK_ACCESS {
            return false;
        }

        plan.entries.insert(header, entry.clone());
        if !exit_values.is_empty() {
            plan.entries.insert(exit, exit_values);
        }
        plan.edges.insert(preheader, StackPhiEdge { sources: initial_sources, results: entry });
        plan.branch_edges.insert(header, StackPhiBranch { then_edge, else_edge, union });
        true
    }

    fn can_plan_branching_loop(&self, loop_info: &Loop) -> bool {
        let mut nesting_depth = 0;
        for other in &self.loops {
            if other.header == loop_info.header {
                continue;
            }
            nesting_depth += usize::from(other.blocks.contains(loop_info.header));
        }
        if nesting_depth != 0 && nesting_depth != 2 {
            return false;
        }
        if !self.loop_instructions_are_stack_safe(loop_info) {
            return false;
        }
        let branch_shapes_safe = loop_info
            .blocks
            .iter()
            .filter(|&block_id| block_id != loop_info.header)
            .all(|block_id| {
                let Some(Terminator::Branch { then_block, else_block, .. }) =
                    self.func.blocks[block_id].terminator.as_ref()
                else {
                    return true;
                };
                (loop_info.blocks.contains(*then_block) == loop_info.blocks.contains(*else_block))
                    || (!loop_info.blocks.contains(*then_block)
                        && self.is_noreturn_block(*then_block))
                    || (!loop_info.blocks.contains(*else_block)
                        && self.is_noreturn_block(*else_block))
                    || self.branch_phi_shape(loop_info, *then_block, *else_block).is_some()
            });
        branch_shapes_safe && self.phi_insts(&self.func.blocks[loop_info.header]).len() >= 2
    }

    fn loop_instructions_are_stack_safe(&self, loop_info: &Loop) -> bool {
        for block_id in loop_info.blocks.iter() {
            for &inst_id in &self.func.blocks[block_id].instructions {
                let kind = &self.func.inst(inst_id).kind;
                if !matches!(
                    kind,
                    InstKind::Add(_, _)
                        | InstKind::Sub(_, _)
                        | InstKind::Mul(_, _)
                        | InstKind::Div(_, _)
                        | InstKind::SDiv(_, _)
                        | InstKind::Mod(_, _)
                        | InstKind::SMod(_, _)
                        | InstKind::Exp(_, _)
                        | InstKind::AddMod(_, _, _)
                        | InstKind::MulMod(_, _, _)
                        | InstKind::And(_, _)
                        | InstKind::Or(_, _)
                        | InstKind::Xor(_, _)
                        | InstKind::Not(_)
                        | InstKind::Clz(_)
                        | InstKind::Shl(_, _)
                        | InstKind::Shr(_, _)
                        | InstKind::Sar(_, _)
                        | InstKind::Byte(_, _)
                        | InstKind::Lt(_, _)
                        | InstKind::Gt(_, _)
                        | InstKind::SLt(_, _)
                        | InstKind::SGt(_, _)
                        | InstKind::Eq(_, _)
                        | InstKind::IsZero(_)
                        | InstKind::MLoad(_)
                        | InstKind::MStore(_, _)
                        | InstKind::MStore8(_, _)
                        | InstKind::CalldataLoad(_)
                        | InstKind::CalldataSize
                        | InstKind::CalldataCopy(_, _, _)
                        | InstKind::MSize
                        | InstKind::Fmp
                        | InstKind::Keccak256(_, _)
                        | InstKind::Phi(_)
                        | InstKind::Select(_, _, _)
                        | InstKind::SignExtend(_, _)
                ) {
                    return false;
                }
            }
        }
        true
    }

    fn is_noreturn_block(&self, block_id: BlockId) -> bool {
        GlobalStackPlan::is_terminal_block(self.func, block_id)
            || matches!(
                self.func.blocks[block_id].terminator.as_ref(),
                Some(Terminator::TailCall { args, .. }) if args.is_empty()
            )
    }

    fn plan_join(&self, block_id: BlockId, plan: &mut StackPhiPlan) {
        let block = &self.func.blocks[block_id];
        if plan.entries.contains_key(&block_id)
            || self.loops.iter().any(|loop_info| loop_info.header == block_id)
            || block.predecessors.len() < 2
        {
            return;
        }

        let phi_insts = self.phi_insts(block);
        if phi_insts.is_empty() || phi_insts.len() > STACK_PHI_LAYOUT_LIMIT {
            return;
        }
        let Some(results) = self.phi_result_values(&phi_insts) else {
            return;
        };
        if block.predecessors.iter().any(|pred| {
            plan.edges.contains_key(pred)
                || !matches!(
                    self.func.blocks[*pred].terminator,
                    Some(Terminator::Jump(target)) if target == block_id
                )
        }) {
            return;
        }

        let mut edges = Vec::with_capacity(block.predecessors.len());
        for &pred in &block.predecessors {
            let Some(sources) = self.phi_sources_for_pred(&phi_insts, pred) else {
                return;
            };
            edges.push((pred, sources));
        }

        plan.entries.insert(block_id, results.clone());
        for (pred, sources) in edges {
            plan.edges.insert(pred, StackPhiEdge { sources, results: results.clone() });
        }
    }

    fn phi_insts(&self, block: &crate::mir::BasicBlock) -> Vec<InstId> {
        block
            .instructions
            .iter()
            .copied()
            .filter(|&inst| matches!(self.func.inst(inst).kind, InstKind::Phi(_)))
            .collect()
    }

    fn carry_through_values(&self, loop_info: &Loop) -> Vec<ValueId> {
        let mut carry_through = Vec::new();
        for outer in &self.loops {
            if outer.header == loop_info.header || !outer.blocks.contains(loop_info.header) {
                continue;
            }
            let Some(results) = self.header_results.get(&outer.header) else {
                continue;
            };
            for &value in results {
                if carry_through.contains(&value)
                    || !self.value_used_in_blocks(&loop_info.blocks, value)
                {
                    continue;
                }
                carry_through.push(value);
            }
        }
        carry_through
    }

    fn value_used_in_blocks(&self, blocks: &DenseBitSet<BlockId>, value: ValueId) -> bool {
        for block_id in blocks {
            let block = &self.func.blocks[block_id];
            for &inst_id in &block.instructions {
                if matches!(self.func.inst(inst_id).kind, InstKind::Phi(_)) {
                    continue;
                }
                if self.func.inst(inst_id).kind.operands().contains(&value) {
                    return true;
                }
            }
            if block.terminator.as_ref().is_some_and(|term| term.operands().contains(&value)) {
                return true;
            }
        }
        false
    }

    fn extend_live_through_values(&self, loop_info: &Loop, values: &mut Vec<ValueId>) {
        for block_id in &loop_info.blocks {
            let block = &self.func.blocks[block_id];
            for &inst_id in &block.instructions {
                let inst = self.func.inst(inst_id);
                if matches!(inst.kind, InstKind::Phi(_)) {
                    continue;
                }
                for value in inst.kind.operands() {
                    self.push_live_through_value(loop_info, value, values);
                }
            }
            if let Some(term) = &block.terminator {
                for value in term.operands() {
                    self.push_live_through_value(loop_info, value, values);
                }
            }
        }
    }

    fn extend_live_across_exits(
        &self,
        loop_info: &Loop,
        liveness: &Liveness,
        values: &mut Vec<ValueId>,
    ) {
        for block_id in &loop_info.blocks {
            let Some(terminator) = &self.func.blocks[block_id].terminator else { continue };
            for successor in terminator
                .successors()
                .into_iter()
                .filter(|successor| !loop_info.blocks.contains(*successor))
            {
                for value in liveness.live_in(successor) {
                    self.push_live_through_value(loop_info, value, values);
                }
            }
        }
    }

    fn push_live_through_value(&self, loop_info: &Loop, value: ValueId, values: &mut Vec<ValueId>) {
        let crate::mir::Value::Inst(_) = self.func.value(value) else { return };
        let Some(definition) = self.definitions[value] else { return };
        if !loop_info.blocks.contains(definition) && !values.contains(&value) {
            values.push(value);
        }
    }

    fn phi_result_values(&self, phi_insts: &[InstId]) -> Option<Vec<ValueId>> {
        phi_insts.iter().map(|&inst| self.func.inst_result_value(inst)).collect()
    }

    fn phi_sources_for_pred(&self, phi_insts: &[InstId], pred: BlockId) -> Option<Vec<ValueId>> {
        phi_insts
            .iter()
            .map(|&inst| {
                let InstKind::Phi(incoming) = &self.func.inst(inst).kind else {
                    return None;
                };
                incoming.iter().find_map(|&(block, value)| (block == pred).then_some(value))
            })
            .collect()
    }

    fn is_phi_value(&self, value: ValueId) -> bool {
        matches!(self.func.value(value), crate::mir::Value::Inst(inst) if matches!(self.func.inst(*inst).kind, InstKind::Phi(_)))
    }

    fn is_loop_header_phi(&self, value: ValueId) -> bool {
        let crate::mir::Value::Inst(inst) = self.func.value(value) else {
            return false;
        };
        self.loops
            .iter()
            .any(|loop_info| self.func.blocks[loop_info.header].instructions.contains(inst))
    }
}

/// EVM code generator.
pub struct EvmCodegen<'gcx> {
    gcx: Gcx<'gcx>,
    /// The assembler for bytecode generation.
    asm: Assembler<'gcx>,
    /// Stack scheduler.
    scheduler: StackScheduler,
    /// Block labels.
    block_labels: FxHashMap<BlockId, Label>,
    /// Function labels for direct internal calls.
    function_labels: FxHashMap<FunctionId, Label>,
    /// Functions whose reachable exits all abort. Calls to these functions
    /// make their containing block cold as well.
    cold_functions: DenseBitSet<FunctionId>,
    /// Functions consisting only of an empty block terminated by `stop`.
    empty_stop_functions: DenseBitSet<FunctionId>,
    /// Cold blocks in the function currently being emitted, including blocks
    /// that only forward control to other cold blocks.
    cold_blocks: DenseBitSet<BlockId>,
    /// Exact per-function spill area sizes, in bytes, recorded after emission.
    function_spill_sizes: FxHashMap<FunctionId, u64>,
    /// Internal-call frame-size constants waiting for exact callee spill sizes.
    pending_frame_size_consts: Vec<(DeferredConst, FunctionId)>,
    /// Per-function entry/exit stack signatures for non-recursive static calls. An absent plan, or
    /// an argument not selected by a plan, uses the existing static-memory convention.
    static_call_abis: FxHashMap<FunctionId, StaticCallAbi>,
    /// Functions whose stack-only argument convention had to materialize a frame fallback during
    /// emission. They stay on the ordinary stack-argument convention on the regenerated runtime.
    disabled_stack_only_functions: DenseBitSet<FunctionId>,
    /// Whether stack-native return tuples may be selected. Cleared when the
    /// whole-program stack proof fails even without preserved prefixes or
    /// stack arguments, falling back to the frame-backed return convention.
    stack_returns_enabled: bool,
    /// Enables the optional caller-prefix convention for this emission. If
    /// post-emission stack validation rejects it, runtime codegen reruns once
    /// with this disabled.
    preserve_caller_stack: bool,
    /// Functions reached from a recursive activation. Their incoming physical
    /// prefix is unbounded, so preserving another caller prefix would change
    /// the recursion limit.
    recursive_stack_functions: DenseBitSet<FunctionId>,
    /// Functions that are themselves members of a recursive call cycle. A
    /// nested activation reuses their static scratch frame only after the
    /// suspended activation's live words have moved to the EVM stack.
    recursive_frame_functions: DenseBitSet<FunctionId>,
    /// Call edges within a recursive static-frame component. The caller state
    /// must survive the entire callee activation because it can re-enter and
    /// overwrite the caller's fixed frame.
    recursive_frame_edges: FxHashSet<(FunctionId, FunctionId)>,
    /// Functions that are recursive or can reach recursion. A preserved
    /// prefix must not be carried into an unbounded descendant.
    recursion_reaching_functions: DenseBitSet<FunctionId>,
    /// High-water mark of the modeled stack above each function's inherited
    /// untracked prefix.
    function_stack_peaks: FxHashMap<FunctionId, usize>,
    /// Runtime internal-call edges and the caller words retained at each site.
    icall_stack_edges: Vec<ICallStackEdge>,
    /// Whether the current assembly is the runtime (stack-passed arguments
    /// apply). The constructor assembly emits its own copies of internal
    /// functions with the plain frame-store convention.
    runtime_stack_args: bool,
    /// Deferred spill-slot address pushes of the external body being emitted,
    /// keyed by the slot's allocation offset, with their reference counts.
    /// Ranked hottest-first at body end so the most reloaded slots take the
    /// shortest addresses; final addresses wait for global layout.
    spill_addr_consts: FxHashMap<u64, (DeferredConst, usize)>,
    /// Ranked external spill pushes retained until static-allocation layout is
    /// finalized, keyed by entry function.
    external_spill_addr_consts: FxHashMap<FunctionId, Vec<(DeferredConst, usize)>>,
    /// Callees whose internal-call frame can be deallocated after return.
    restorable_internal_frames: DenseBitSet<FunctionId>,
    /// Functions whose frame lives at a compile-time-fixed address (static
    /// frames): internal-convention, non-recursive functions in the runtime
    /// passes. Their arg/local/spill accesses are absolute pushes and their
    /// call sites skip all frame-pointer and free-pointer bookkeeping.
    static_frame_functions: DenseBitSet<FunctionId>,
    /// Interned deferred constants for absolute static-frame addresses, keyed
    /// by (function, byte offset within its frame). Resolved at the end of
    /// the pass, once every body's exact spill size is known.
    static_frame_addr_consts: FxHashMap<(FunctionId, u64), (DeferredConst, usize)>,
    /// Deferred allocations emitted by each external entry.
    pending_static_allocs: FxHashMap<FunctionId, Vec<(DeferredAlloc, u64)>>,
    /// Per-external-entry free-memory-pointer constants, resolved after static-frame placement.
    /// Entries that never use dynamic memory omit the initialization entirely.
    runtime_free_memory_consts: FxHashMap<FunctionId, DeferredConst>,
    /// Internal functions reachable from each entry that initializes the free-memory pointer.
    runtime_entry_reachability: FxHashMap<FunctionId, DenseBitSet<FunctionId>>,
    /// Every external body emitted this pass, for sizing the heap floor.
    runtime_entry_funcs: Vec<FunctionId>,
    /// The internal-convention function currently being emitted.
    current_internal_function: Option<FunctionId>,
    /// Copies to insert at block exits (from phi elimination).
    block_copies: FxHashMap<BlockId, Vec<ParallelCopy>>,
    /// Values carried by planned stack-resident edges, keyed by predecessor block.
    stack_phi_sources: FxHashMap<BlockId, Vec<ValueId>>,
    /// Spill stores available on the current block's path at the current
    /// emission point (`None` outside block emission or when no emitted
    /// forward predecessor constrains it). Stores and clobbers in the block
    /// update the set before it propagates to successors.
    spill_available: Option<FxHashSet<ValueId>>,
    /// Multi-return protocol instructions satisfied directly from adopted
    /// stack-return words; the emission loop skips them.
    elided_insts: FxHashSet<InstId>,
    late_gas_operands: FxHashMap<ValueId, LateGasOperand>,
    spill_stores: Vec<SpillStore>,
    spill_loads: Vec<(SpillSlot, ir::BlockId, usize)>,
    early_spill_removals: Vec<(ir::BlockId, std::ops::Range<usize>)>,
    function_ir_block_start: usize,
    /// Whole-calldata-forwarding clobbers (`calldatacopy(0, 0, calldatasize())`
    /// in a proxy) whose write reaches the compiler spill area. Values live
    /// across one are kept stack-resident instead of reloaded from the
    /// overwritten slot. Empty for every function without such a forward.
    spill_hazard_insts: FxHashSet<InstId>,
    /// Leaf helpers whose sole returned word is derived from the free-memory pointer.
    /// Their callers may safely use the result as a dynamic forwarding-buffer base.
    heap_pointer_return_functions: DenseBitSet<FunctionId>,
    /// Whether the current function has canonical cross-block argument layouts.
    global_stack_active: bool,
    /// Calldata words physically identical to arguments in the active global
    /// layout, adopted after their final validation use.
    global_stack_aliases: FxHashMap<ValueId, ValueId>,
    /// Immutable `PUSH<N>` placeholders in the last assembled runtime code.
    runtime_immutable_refs: Vec<ImmutableRef>,
    /// Backend encodings derived from the current module's immutable declarations.
    immutable_encodings: IndexVec<ImmutableId, ImmutableEncoding>,
    /// First constructor-memory word reserved for immutable staging.
    immutable_staging_base: u64,
    /// Deferred absolute base of the copied constructor ABI argument blob.
    constructor_args_base_const: Option<DeferredConst>,
    /// Deferred code offset of the copied constructor ABI argument blob.
    constructor_args_offset_const: Option<DeferredConst>,
    /// Whether we're currently generating constructor code.
    /// When true, arguments load from the copied deployment ABI blob.
    in_constructor: bool,
    /// Shared constructor completion reached by ordinary `stop` terminators.
    constructor_exit: Option<Label>,
    /// Number of constructor parameters (used for CODECOPY offset calculation).
    constructor_param_count: u32,
    /// Whether we're emitting an internal function body.
    in_internal_function: bool,
    /// Whether we're emitting the MIR `entry` function. Its switch
    /// keeps the selector on the physical stack through the case chain and
    /// leaves it inert below the taken arm. This is only sound for `entry`: it
    /// runs once and every arm terminates externally, so the leftover word can
    /// neither accumulate nor disturb an internal return.
    emitting_entry: bool,
    /// Gas-mode switch growth still available in the current deployment artifact.
    switch_gas_code_growth_remaining: usize,
    capture_mir: bool,
    capture_evm_ir: bool,
    capture_debug_info: bool,
}

impl<'gcx> EvmCodegen<'gcx> {
    /// Creates a new EVM code generator.
    #[must_use]
    pub fn new(gcx: Gcx<'gcx>) -> Self {
        let switch_gas_code_growth_remaining = Self::switch_gas_code_growth_limit(gcx);
        Self {
            gcx,
            asm: Assembler::new(gcx),
            scheduler: StackScheduler::for_evm_version(gcx.sess.opts.evm_version),
            block_labels: FxHashMap::default(),
            function_labels: FxHashMap::default(),
            cold_functions: DenseBitSet::new_empty(0),
            empty_stop_functions: DenseBitSet::new_empty(0),
            cold_blocks: DenseBitSet::new_empty(0),
            function_spill_sizes: FxHashMap::default(),
            pending_frame_size_consts: Vec::new(),
            static_call_abis: FxHashMap::default(),
            disabled_stack_only_functions: DenseBitSet::new_empty(0),
            stack_returns_enabled: true,
            preserve_caller_stack: false,
            recursive_stack_functions: DenseBitSet::new_empty(0),
            recursive_frame_functions: DenseBitSet::new_empty(0),
            recursive_frame_edges: FxHashSet::default(),
            recursion_reaching_functions: DenseBitSet::new_empty(0),
            function_stack_peaks: FxHashMap::default(),
            icall_stack_edges: Vec::new(),
            runtime_stack_args: false,
            spill_addr_consts: FxHashMap::default(),
            external_spill_addr_consts: FxHashMap::default(),
            restorable_internal_frames: DenseBitSet::new_empty(0),
            static_frame_functions: DenseBitSet::new_empty(0),
            static_frame_addr_consts: FxHashMap::default(),
            pending_static_allocs: FxHashMap::default(),
            runtime_free_memory_consts: FxHashMap::default(),
            runtime_entry_reachability: FxHashMap::default(),
            runtime_entry_funcs: Vec::new(),
            current_internal_function: None,
            block_copies: FxHashMap::default(),
            stack_phi_sources: FxHashMap::default(),
            spill_available: None,
            elided_insts: FxHashSet::default(),
            late_gas_operands: FxHashMap::default(),
            spill_stores: Vec::new(),
            spill_loads: Vec::new(),
            early_spill_removals: Vec::new(),
            function_ir_block_start: 0,
            spill_hazard_insts: FxHashSet::default(),
            heap_pointer_return_functions: DenseBitSet::new_empty(0),
            global_stack_active: false,
            global_stack_aliases: FxHashMap::default(),
            runtime_immutable_refs: Vec::new(),
            immutable_encodings: IndexVec::new(),
            immutable_staging_base: EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT
                + EvmMemoryLayout::WORD_SIZE,
            constructor_args_base_const: None,
            constructor_args_offset_const: None,
            in_constructor: false,
            constructor_exit: None,
            constructor_param_count: 0,
            in_internal_function: false,
            emitting_entry: false,
            switch_gas_code_growth_remaining,
            capture_mir: false,
            capture_evm_ir: false,
            capture_debug_info: false,
        }
    }

    /// Clears state that belongs to one lowered MIR module.
    fn reset_for_module(&mut self, module: &Module) {
        self.asm.clear();
        self.scheduler.reset();
        self.block_labels.clear();
        self.function_labels.clear();
        self.cold_functions.clear_to(module.functions.len());
        self.empty_stop_functions.clear_to(module.functions.len());
        self.cold_blocks.clear_to(0);
        self.function_spill_sizes.clear();
        self.pending_frame_size_consts.clear();
        self.static_call_abis.clear();
        self.disabled_stack_only_functions.clear_to(module.functions.len());
        self.stack_returns_enabled = true;
        self.preserve_caller_stack = false;
        self.recursive_stack_functions.clear_to(module.functions.len());
        self.recursive_frame_functions.clear_to(module.functions.len());
        self.recursive_frame_edges.clear();
        self.recursion_reaching_functions.clear_to(module.functions.len());
        self.function_stack_peaks.clear();
        self.icall_stack_edges.clear();
        self.runtime_stack_args = false;
        self.spill_addr_consts.clear();
        self.external_spill_addr_consts.clear();
        self.restorable_internal_frames.clear_to(module.functions.len());
        self.static_frame_functions.clear_to(module.functions.len());
        self.static_frame_addr_consts.clear();
        self.pending_static_allocs.clear();
        self.runtime_free_memory_consts.clear();
        self.runtime_entry_reachability.clear();
        self.runtime_entry_funcs.clear();
        self.current_internal_function = None;
        self.block_copies.clear();
        self.stack_phi_sources.clear();
        self.spill_available = None;
        self.elided_insts.clear();
        self.late_gas_operands.clear();
        self.spill_hazard_insts.clear();
        self.heap_pointer_return_functions.clear_to(module.functions.len());
        self.global_stack_active = false;
        self.global_stack_aliases.clear();
        self.runtime_immutable_refs.clear();
        self.immutable_encodings.clear();
        self.immutable_staging_base =
            EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT + EvmMemoryLayout::WORD_SIZE;
        self.constructor_args_base_const = None;
        self.constructor_args_offset_const = None;
        self.in_constructor = false;
        self.constructor_exit = None;
        self.constructor_param_count = 0;
        self.in_internal_function = false;
        self.emitting_entry = false;
        self.reset_switch_gas_code_growth();
    }

    fn static_call_abi_mut(&mut self, func_id: FunctionId, arg_count: usize) -> &mut StaticCallAbi {
        self.static_call_abis.entry(func_id).or_insert_with(|| StaticCallAbi::new(arg_count))
    }

    fn stack_arg_mask(&self, func_id: FunctionId) -> Option<&DenseBitSet<usize>> {
        self.static_call_abis
            .get(&func_id)
            .map(|abi| &abi.stack_args)
            .filter(|mask| !mask.is_empty())
    }

    fn direct_stack_args(&self, func_id: FunctionId) -> Option<&[ValueId]> {
        match &self.static_call_abis.get(&func_id)?.entry {
            StaticCallEntry::Direct(values) => Some(values),
            _ => None,
        }
    }

    fn resident_stack_args(&self, func_id: FunctionId) -> Option<&[ValueId]> {
        match &self.static_call_abis.get(&func_id)?.entry {
            StaticCallEntry::Resident { values, .. } => Some(values),
            _ => None,
        }
    }

    fn resident_stack_plan(&self, func_id: FunctionId) -> Option<&GlobalStackPlan> {
        match &self.static_call_abis.get(&func_id)?.entry {
            StaticCallEntry::Resident { layout, .. } => Some(layout),
            _ => None,
        }
    }

    fn lazy_stack_args(&self, func_id: FunctionId) -> Option<&LazyStackArgPlan> {
        match &self.static_call_abis.get(&func_id)?.entry {
            StaticCallEntry::Lazy(plan) => Some(plan),
            _ => None,
        }
    }

    /// Returns arguments without a valid frame home at this point in the callee.
    fn stack_only_values(&self, func_id: FunctionId, entry: bool) -> Vec<ValueId> {
        self.resident_stack_args(func_id)
            .into_iter()
            .flatten()
            .copied()
            .chain(
                entry
                    .then(|| self.direct_stack_args(func_id))
                    .flatten()
                    .into_iter()
                    .flatten()
                    .copied(),
            )
            .chain(
                entry
                    .then(|| self.lazy_stack_args(func_id))
                    .flatten()
                    .into_iter()
                    .flat_map(LazyStackArgPlan::values),
            )
            .collect()
    }

    fn stack_return_plan(&self, func_id: FunctionId) -> Option<StackReturnPlan> {
        self.static_call_abis.get(&func_id)?.returns
    }

    fn reset_switch_gas_code_growth(&mut self) {
        self.switch_gas_code_growth_remaining = Self::switch_gas_code_growth_limit(self.gcx);
    }

    fn switch_gas_code_growth_limit(gcx: Gcx<'_>) -> usize {
        gcx.sess.opts.unstable.switch_max_gas_code_growth.unwrap_or(MAX_GAS_CODE_GROWTH)
    }

    /// Reports MIR constructs the backend cannot emit yet.
    ///
    /// This includes argument-taking fallbacks and logical slices whose
    /// aggregate use slice lowering could not fold.
    ///
    /// Only live instructions — those still in a block — are checked, since the
    /// instruction arena retains folded-away slices the backend never emits.
    #[must_use]
    fn emit_unsupported(&self, module: &Module) -> bool {
        if module
            .functions
            .iter()
            .any(|func| func.attributes.is_fallback && !func.params.is_empty())
        {
            self.gcx
                .dcx()
                .err("codegen does not support `fallback(bytes) returns (bytes)` yet")
                .span(module.name.span)
                .emit();
            return true;
        }

        let mut emitted = false;
        'func: for func in module.functions.iter() {
            for inst_id in func.instructions() {
                let inst = func.inst(inst_id);
                let message = match inst.kind {
                    InstKind::MakeSlice { .. } | InstKind::SlicePtr(_) | InstKind::SliceLen(_) => {
                        "codegen does not support this calldata-slice usage yet"
                    }
                    InstKind::StoreImmutable(..) => {
                        "immutable assignments must be lowered before EVM codegen"
                    }
                    _ => continue,
                };
                let span = inst.metadata.source_span().unwrap_or(module.name.span);
                self.gcx
                    .dcx()
                    .err(message)
                    .span(span)
                    .note(format!("remaining MIR slice is in function `{}`", func.name))
                    .emit();
                emitted = true;
                // One diagnostic per function is enough to explain the bail.
                continue 'func;
            }
        }
        emitted
    }

    /// Controls whether generated artifacts include final EVM IR.
    pub fn set_capture_evm_ir(&mut self, capture: bool) {
        self.capture_evm_ir = capture;
    }

    /// Controls whether generated artifacts include final instruction locations.
    pub fn set_capture_debug_info(&mut self, capture: bool) {
        self.capture_debug_info = capture;
    }

    /// Controls whether modules without an external entry still run the MIR pipeline.
    pub(crate) fn set_capture_mir(&mut self, capture: bool) {
        self.capture_mir = capture;
    }

    // ==================== Stack-Aware Emitter API ====================
    //
    // These helpers ensure that all EVM stack mutations are tracked by the scheduler.
    // Any opcode that changes the EVM stack must be emitted through these methods
    // to keep the scheduler's StackModel in sync with the actual EVM stack.

    /// Emits a stack manipulation operation (DUP, SWAP, POP) and updates the scheduler.
    fn emit_stack_op(&mut self, op: StackOp) {
        self.asm.emit_stack_op(op);
        self.scheduler.stack.apply(op);
    }

    /// Emits an opcode with known stack effects and updates the scheduler.
    ///
    /// This is the core method for stack-aware emission. After emitting the opcode:
    /// - `effect.pops` values are removed from the scheduler's stack model
    /// - Values are pushed according to `push`:
    ///   - `StackPush::None`: no value pushed (effect.pushes must be 0)
    ///   - `StackPush::Tracked(v)`: push a tracked ValueId (effect.pushes must be 1)
    ///   - `StackPush::Unknown`: push an untracked value (effect.pushes must be 1)
    fn emit_op_with_effect(&mut self, opcode: u8, effect: StackEffect, push: StackPush) {
        #[cfg(debug_assertions)]
        let before = self.scheduler.depth();

        self.asm.emit_op(opcode);

        // Pop consumed values
        for _ in 0..effect.pops {
            self.scheduler.stack.pop();
        }

        // Push produced values
        match (effect.pushes, push) {
            (0, StackPush::None) => {}
            (1, StackPush::Tracked(v)) => self.scheduler.stack.push(v),
            (1, StackPush::Unknown) => self.scheduler.stack.push_unknown(),
            (n, _) if n > 1 => {
                // Multi-push: push unknown values
                for _ in 0..n {
                    self.scheduler.stack.push_unknown();
                }
            }
            _ => {}
        }

        #[cfg(debug_assertions)]
        {
            let expected = before + effect.pushes - effect.pops;
            debug_assert_eq!(
                self.scheduler.depth(),
                expected,
                "Stack model drift after opcode 0x{:02x}: expected depth {}, got {}",
                opcode,
                expected,
                self.scheduler.depth()
            );
        }
    }

    /// Generates deployment bytecode for a module.
    /// Returns (deployment_bytecode, runtime_bytecode).
    /// Returns empty bytecodes for interfaces (they have no implementation).
    ///
    /// This runs optimization passes (including DCE) on the module before codegen unless disabled.
    pub fn generate_deployment_bytecode(&mut self, module: &mut Module) -> (Vec<u8>, Vec<u8>) {
        let artifact = self.generate_deployment_artifact(module);
        (artifact.deployment, artifact.runtime)
    }

    #[tracing::instrument(
        name = "evm_codegen",
        level = "debug",
        skip_all,
        fields(module = %module.name),
    )]
    fn generate_deployment_artifact(&mut self, module: &mut Module) -> EvmArtifact {
        // Interfaces have no code. An internal-only library keeps its rejecting
        // dispatch stub, like `solc`.
        if module.is_interface {
            return EvmArtifact::default();
        }
        if let Some(func) = module.functions.iter().find(|func| func.blocks.is_empty()) {
            panic!("cannot codegen MIR function `{}` without an entry block", func.name);
        }
        self.reset_for_module(module);
        self.run_optimization_passes(module);
        if self.emit_unsupported(module) {
            return EvmArtifact::default();
        }
        if module.phase != MirPhase::EvmShaped {
            self.gcx
                .dcx()
                .err(format!(
                    "EVM codegen requires MIR in the `evm-shaped` phase, stopped at `{}`",
                    module.phase.name()
                ))
                .span(module.name.span)
                .emit();
            return EvmArtifact::default();
        }
        self.immutable_staging_base = immutable_staging_base(module);
        self.immutable_encodings.clear();
        for (id, immutable) in module.iter_immutables() {
            let encoding =
                immutable.ty.immutable_encoding().expect("validated immutable declaration");
            let allocated = self.immutable_encodings.push(encoding);
            debug_assert_eq!(allocated, id);
        }
        // Phi elimination places a predecessor's parallel copies before its
        // terminator, which executes them on every outgoing edge. Late CFG
        // passes can leave critical edges whose copies would clobber values
        // still live on a sibling edge, so give each such edge its own block.
        for func in &mut module.functions {
            Self::split_phi_critical_edges(func);
        }
        if !matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            for func in &mut module.functions {
                func.canonicalize_argument_uses();
                if matches!(self.gcx.sess.opts.optimization, OptimizationMode::Size) {
                    func.canonicalize_immediate_uses();
                }
            }
        }
        // Runtime and constructor emission inspect the same final MIR. Compute module-wide facts
        // once instead of rebuilding them for each artifact and caller-stack retry.
        let call_graph = CallGraphInfo::new(module);
        self.heap_pointer_return_functions = Self::collect_heap_pointer_return_functions(module);
        self.cold_functions = if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            DenseBitSet::new_empty(module.functions.len())
        } else {
            Self::collect_cold_functions(module)
        };

        // First generate the runtime code
        let runtime_code = self.generate_runtime_code(module, &call_graph);
        let runtime_len = runtime_code.bytecode.len();
        let immutable_refs = std::mem::take(&mut self.runtime_immutable_refs);

        // The constructor copies the runtime code to memory and patches the
        // immutable placeholders with the staged words before
        // returning. Copy to offset 0 unless that would overwrite the immutable
        // staging area before the patch loop reads it.
        let copy_base = Self::runtime_copy_base(module, runtime_len, &immutable_refs);

        // Generate constructor initialization and the deployment postlude as
        // one control-flow graph and optimize it once. Constructor arguments
        // are appended after the generated deployment prefix, so their offset
        // and the runtime-code offset depend on its final push widths. Only
        // repeat final assembly while both offsets stabilize.
        let prepared_deploy_code = self.prepare_deployment_prefix(
            module,
            &call_graph,
            runtime_len,
            copy_base,
            &immutable_refs,
        );
        let mut deploy_code_len = 0usize;
        let mut constructor_arg_offset = runtime_len;
        let mut deploy_code = self.assemble_deployment_prefix(
            &prepared_deploy_code,
            constructor_arg_offset,
            deploy_code_len,
        );
        for _ in 0..8 {
            let next_deploy_code_len = deploy_code.bytecode.len();
            let next_arg_offset = next_deploy_code_len + runtime_len;
            if next_deploy_code_len == deploy_code_len && next_arg_offset == constructor_arg_offset
            {
                break;
            }
            deploy_code_len = next_deploy_code_len;
            constructor_arg_offset = next_arg_offset;
            deploy_code = self.assemble_deployment_prefix(
                &prepared_deploy_code,
                constructor_arg_offset,
                deploy_code_len,
            );
        }

        // Deploy code structure:
        // [constructor_code]    ; run constructor (SSTOREs + immutable staging)
        // PUSH<n> runtime_len   ; size to copy from creation code
        // DUP1                  ; duplicate for the final RETURN size
        // PUSH<n> offset        ; where runtime starts
        // PUSH<n> copy_base     ; memory destination
        // CODECOPY              ; copy runtime to memory
        // [immutable patches]   ; patch staged words into the PUSH<N> placeholders
        // PUSH<n> copy_base     ; memory offset
        // RETURN                ; return the runtime code
        let mut deploy_bytecode = deploy_code.bytecode;
        deploy_bytecode.extend_from_slice(&runtime_code.bytecode);

        // The returned runtime artifact keeps the zero placeholders, like
        // solc's `deployedBytecode` for contracts with immutables.
        EvmArtifact {
            deployment: deploy_bytecode,
            runtime: runtime_code.bytecode,
            immutable_references: immutable_refs,
            deployment_evm_ir: deploy_code.evm_ir,
            runtime_evm_ir: runtime_code.evm_ir,
            deployment_debug_info: deploy_code.debug_info,
            runtime_debug_info: runtime_code.debug_info,
        }
    }

    fn runtime_copy_base(
        module: &Module,
        runtime_len: usize,
        immutable_refs: &[ImmutableRef],
    ) -> u64 {
        let patched_end = immutable_refs.iter().fold(runtime_len, |end, immutable_ref| {
            let patch_size = if immutable_ref.type_size.bytes() == 1 { 1 } else { WORD_BYTES };
            end.max(
                immutable_ref
                    .code_offset
                    .checked_add(1 + patch_size)
                    .expect("immutable patch offset overflow"),
            )
        });
        let staging_base = immutable_staging_base(module);
        if !immutable_refs.is_empty() && patched_end as u64 > staging_base {
            immutable_staging_end(staging_base, module.immutable_count())
        } else {
            0
        }
    }

    fn emit_deployment_postlude(
        &mut self,
        module: &Module,
        runtime_offset: DeferredConst,
        runtime_len: usize,
        copy_base: u64,
        immutable_refs: &[ImmutableRef],
    ) {
        // Copy runtime code from creation code to memory at `copy_base`.
        self.asm.emit_push(U256::from(runtime_len as u64));
        self.asm.emit_stack_op(StackOp::Dup(1));
        self.asm.emit_push_deferred(runtime_offset);
        self.asm.emit_push(U256::from(copy_base));
        self.asm.emit_op(op::CODECOPY);

        // Patch each `PUSH<N>` placeholder with its staged immutable value.
        for r in immutable_refs {
            let encoding = module
                .immutable_type(r.id)
                .immutable_encoding()
                .expect("validated immutable declaration");
            debug_assert_eq!(
                immutable_push_type_size(
                    encoding,
                    self.gcx.sess.opts.optimization,
                    self.gcx.sess.opts.evm_version.has_bitwise_shifting(),
                ),
                r.type_size
            );
            self.emit_immutable_patch(copy_base, *r, encoding);
        }

        // Return the patched runtime code; the DUP'd length is still on the stack.
        self.asm.emit_push(U256::from(copy_base));
        self.asm.emit_op(op::RETURN);
    }

    fn emit_immutable_patch(
        &mut self,
        copy_base: u64,
        immutable_ref: ImmutableRef,
        encoding: ImmutableEncoding,
    ) {
        let byte_width = immutable_ref.type_size.bytes();
        let destination = copy_base + immutable_ref.code_offset as u64 + 1;

        self.asm.emit_push(U256::from(immutable_staging_addr(
            self.immutable_staging_base,
            immutable_ref.id,
        )));
        self.asm.emit_op(op::MLOAD);

        if byte_width == 1 {
            if matches!(encoding, ImmutableEncoding::LeftAligned(_)) {
                self.asm.emit_push(U256::ZERO);
                self.asm.emit_op(op::BYTE);
            }
            self.asm.emit_push(U256::from(destination));
            self.asm.emit_op(op::MSTORE8);
            return;
        }

        if byte_width < WORD_BYTES as u8 {
            let trailing_bits = usize::from(WORD_BYTES as u8 - byte_width) * 8;
            match encoding {
                ImmutableEncoding::LeftAligned(_) => {
                    self.asm.emit_push(U256::MAX << trailing_bits);
                    self.asm.emit_op(op::AND);
                }
                ImmutableEncoding::Unsigned(_) | ImmutableEncoding::Signed(_) => {
                    self.asm.emit_push(U256::from(trailing_bits));
                    self.asm.emit_op(op::SHL);
                }
            }

            // Preserve the runtime bytes following the short placeholder. An
            // unaligned MLOAD/MSTORE pair works even across word boundaries.
            self.asm.emit_push(U256::from(destination));
            self.asm.emit_op(op::MLOAD);
            self.asm.emit_push(U256::MAX >> (usize::from(byte_width) * 8));
            self.asm.emit_op(op::AND);
            self.asm.emit_op(op::OR);
        }

        self.asm.emit_push(U256::from(destination));
        self.asm.emit_op(op::MSTORE);
    }

    fn emit_load_immutable(&mut self, id: ImmutableId) {
        if self.in_constructor {
            // The running constructor's own placeholders are never patched.
            self.asm.emit_push(U256::from(immutable_staging_addr(self.immutable_staging_base, id)));
            self.asm.emit_op(op::MLOAD);
            return;
        }

        let encoding = self.immutable_encodings[id];
        let type_size = immutable_push_type_size(
            encoding,
            self.gcx.sess.opts.optimization,
            self.gcx.sess.opts.evm_version.has_bitwise_shifting(),
        );
        let byte_width = type_size.bytes();
        self.asm.emit_push_immutable(id, type_size);
        if byte_width == WORD_BYTES as u8 {
            return;
        }
        match encoding {
            ImmutableEncoding::Unsigned(_) => {}
            ImmutableEncoding::Signed(_) => {
                self.asm.emit_push(U256::from(byte_width - 1));
                self.asm.emit_op(op::SIGNEXTEND);
            }
            ImmutableEncoding::LeftAligned(_) => {
                self.asm.emit_push(U256::from((WORD_BYTES as u8 - byte_width) * 8));
                self.asm.emit_op(op::SHL);
            }
        }
    }

    /// Generates constructor code that runs during deployment.
    /// This includes state variable initializers.
    ///
    /// Constructor arguments are read from the end of the initcode using CODECOPY.
    /// The args are ABI-encoded and appended after the deployment bytecode.
    fn prepare_deployment_prefix(
        &mut self,
        module: &Module,
        call_graph: &CallGraphInfo,
        runtime_len: usize,
        copy_base: u64,
        immutable_refs: &[ImmutableRef],
    ) -> PreparedDeploymentPrefix {
        self.asm.clear();
        self.asm.set_artifact_kind(ArtifactKind::Constructor);
        self.asm.set_evm_ir_name(module.name.name);
        self.asm.load_data(module);
        let runtime_offset = self.asm.new_deferred_const();

        // Find constructor function if it exists
        let constructor =
            module.functions.iter_enumerated().find(|(_, f)| f.attributes.is_constructor);

        let implicit_constructor_revert = constructor.is_none().then(|| self.asm.new_label());
        if let Some(revert) = implicit_constructor_revert {
            self.asm.emit_op(op::CALLVALUE);
            self.asm.emit_push_label(revert);
            self.asm.emit_op(op::JUMPI);
        }

        let constructor_arg_offset = if let Some((ctor_id, ctor)) = constructor {
            // Generate constructor bytecode
            // Clear state and generate function body
            self.block_labels.clear();
            self.block_copies.clear();
            self.function_labels.clear();
            self.function_spill_sizes.clear();
            self.pending_frame_size_consts.clear();
            self.restorable_internal_frames.clear_to(module.functions.len());
            self.static_frame_functions.clear_to(module.functions.len());
            self.static_call_abis.clear();
            self.runtime_stack_args = false;
            // Constructor code has a separate call graph and is not part of
            // the runtime prefix validation below.
            self.preserve_caller_stack = false;
            self.static_frame_addr_consts.clear();
            self.external_spill_addr_consts.clear();
            self.pending_static_allocs.clear();
            self.runtime_free_memory_consts.clear();
            self.runtime_entry_reachability.clear();
            self.runtime_entry_funcs.clear();
            self.current_internal_function = None;
            self.stack_phi_sources.clear();
            self.function_stack_peaks.clear();
            self.icall_stack_edges.clear();

            for (func_id, func) in module.functions.iter_enumerated() {
                if !func.attributes.may_return_memory
                    && !func.params.iter().chain(&func.returns).any(|ty| ty.is_memory_reference())
                {
                    self.restorable_internal_frames.insert(func_id);
                }
            }

            let internal_targets = call_graph.reachable_callees_from(std::iter::once(ctor_id));
            for func_id in &internal_targets {
                let label = self.new_function_label(func_id);
                self.function_labels.insert(func_id, label);
            }

            // Constructor locals, immutable staging, and spills occupy fixed
            // compiler-owned regions. The ABI blob starts after their exact
            // post-emission end, and dynamic allocations start after the blob.
            let constructor_fixed_memory_end = self.asm.new_deferred_const();
            let constructor_arg_offset =
                (!ctor.params.is_empty()).then(|| self.asm.new_deferred_const());

            // Set constructor context for LoadArg handling
            self.in_constructor = true;
            self.constructor_param_count = ctor.params.len() as u32;

            // Constructor args are appended after generated deployment bytecode.
            // Copy the complete blob above every fixed compiler-owned region,
            // then place the free-memory pointer after its word-aligned end.
            if let Some(arg_offset) = constructor_arg_offset {
                self.constructor_args_base_const = Some(constructor_fixed_memory_end);
                self.constructor_args_offset_const = Some(arg_offset);
                self.asm.emit_push_deferred(arg_offset);
                self.asm.emit_op(op::CODESIZE);
                self.asm.emit_op(op::SUB); // size = CODESIZE - arg_offset
                self.asm.emit_stack_op(StackOp::Dup(1));
                self.asm.emit_push_deferred(arg_offset); // code offset
                self.asm.emit_push_deferred(constructor_fixed_memory_end);
                self.asm.emit_op(op::CODECOPY);

                self.asm.emit_push_deferred(constructor_fixed_memory_end);
                self.asm.emit_op(op::ADD);
                self.asm.emit_push(U256::from(EvmMemoryLayout::WORD_SIZE - 1));
                self.asm.emit_op(op::ADD);
                self.asm.emit_push(U256::MAX - U256::from(EvmMemoryLayout::WORD_SIZE - 1));
                self.asm.emit_op(op::AND);
                self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
                self.asm.emit_op(op::MSTORE);
            } else {
                self.asm.emit_push_deferred(constructor_fixed_memory_end);
                self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
                self.asm.emit_op(op::MSTORE);
            }

            if !internal_targets.is_empty() {
                let constructor_entry = self.asm.new_label();
                self.emit_push_label(constructor_entry);
                self.asm.emit_op(op::JUMP);

                for (func_id, func) in module.functions.iter_enumerated() {
                    if !internal_targets.contains(func_id) {
                        continue;
                    }
                    let label = self.function_labels[&func_id];
                    self.asm.define_label(label);
                    self.mark_debug_function_invoke(func);
                    self.in_internal_function = true;
                    self.generate_function_body(func_id, func);
                    self.in_internal_function = false;
                    self.record_function_spill_size(func_id);
                }

                self.asm.define_label(constructor_entry);
            }

            // Generate the constructor body (which includes SSTORE for
            // initializers). Every ordinary completion jumps to one label so
            // branch layout cannot strand the deployment postlude behind a
            // non-final STOP.
            let constructor_exit = self.asm.new_label();
            self.constructor_exit = Some(constructor_exit);
            self.mark_debug_function_invoke(ctor);
            self.generate_function_body(ctor_id, ctor);
            let constructor_spill_size = self.record_function_spill_size(ctor_id);
            self.asm.set_deferred_const(
                constructor_fixed_memory_end,
                U256::from(self.constructor_fixed_memory_end(
                    module.immutable_count(),
                    constructor_spill_size,
                )),
            );

            self.resolve_pending_frame_size_consts(module);

            if !self.stack_prefixes_fit_from(module, ctor_id, MAX_STACK_DEPTH) {
                self.report_stack_limit_error();
            }

            // Reset constructor context
            self.in_constructor = false;
            self.constructor_args_base_const = None;
            self.constructor_args_offset_const = None;
            self.constructor_exit = None;
            self.constructor_param_count = 0;

            self.asm.define_label(constructor_exit);
            constructor_arg_offset
        } else {
            None
        };

        self.emit_deployment_postlude(
            module,
            runtime_offset,
            runtime_len,
            copy_base,
            immutable_refs,
        );
        if let Some(revert) = implicit_constructor_revert {
            self.asm.define_label(revert);
            self.asm.emit_push(U256::ZERO);
            self.asm.emit_push(U256::ZERO);
            self.asm.emit_op(op::REVERT);
        }
        PreparedDeploymentPrefix {
            assembly: self.asm.prepare(self.capture_evm_ir, self.capture_debug_info),
            constructor_arg_offset,
            runtime_offset,
        }
    }

    fn assemble_deployment_prefix(
        &mut self,
        prepared: &PreparedDeploymentPrefix,
        constructor_arg_offset: usize,
        runtime_offset: usize,
    ) -> GeneratedCode {
        let mut deferred_values = Vec::with_capacity(2);
        if let Some(id) = prepared.constructor_arg_offset {
            deferred_values.push((id, U256::from(constructor_arg_offset)));
        }
        deferred_values.push((prepared.runtime_offset, U256::from(runtime_offset)));
        let result = self.asm.assemble_prepared(&prepared.assembly, &deferred_values);
        GeneratedCode {
            bytecode: result.bytecode,
            evm_ir: result.evm_ir,
            debug_info: result.debug_info,
        }
    }

    /// Runs the canonical MIR optimization pipeline on the module.
    fn run_optimization_passes(&mut self, module: &mut Module) {
        let _changed = run_pipeline(self.gcx, module, None);
    }

    /// Generates runtime bytecode for a module.
    fn generate_runtime_code(
        &mut self,
        module: &Module,
        call_graph: &CallGraphInfo,
    ) -> GeneratedCode {
        assert_eq!(
            module.phase,
            MirPhase::EvmShaped,
            "EVM codegen requires MIR in the final phase"
        );
        let runtime_code_size_limit = self.gcx.sess.opts.evm_version.runtime_code_size_limit();
        let may_need_code_size_rescue = self.gcx.sess.opts.optimization.is_gas();
        let mut code_size_rescue = false;
        let mut gas_first_result = None;
        loop {
            let mut preserve_caller_stack =
                !matches!(self.gcx.sess.opts.optimization, OptimizationMode::None);
            let mut runtime_stack_args = true;
            let mut stack_returns_enabled = true;
            self.disabled_stack_only_functions.clear_to(module.functions.len());
            loop {
                let disabled_stack_only_functions = self.disabled_stack_only_functions.count();
                self.reset_runtime_codegen(module);
                self.preserve_caller_stack = preserve_caller_stack;
                self.runtime_stack_args = runtime_stack_args;
                self.stack_returns_enabled = stack_returns_enabled;

                if !module.functions.is_empty() {
                    self.emit_runtime(module, call_graph);
                }

                if self.disabled_stack_only_functions.count() > disabled_stack_only_functions {
                    continue;
                }
                let stack_fits = self.caller_stack_prefixes_fit(module, MAX_STACK_DEPTH);
                if !stack_fits && !self.icall_stack_edges.is_empty() {
                    if preserve_caller_stack {
                        preserve_caller_stack = false;
                        continue;
                    }
                    if runtime_stack_args {
                        runtime_stack_args = false;
                        continue;
                    }
                    if stack_returns_enabled {
                        stack_returns_enabled = false;
                        continue;
                    }
                }
                if !stack_fits {
                    self.report_stack_limit_error();
                }
                break;
            }

            self.asm.set_enable_size_outlining(code_size_rescue);

            let result =
                self.asm.assemble_with_captures(self.capture_evm_ir, self.capture_debug_info);
            if may_need_code_size_rescue
                && !code_size_rescue
                && let Some(limit) = runtime_code_size_limit
                && result.bytecode.len() > limit
                && result.bytecode.len() <= limit * 2
            {
                gas_first_result = Some(result);
                code_size_rescue = true;
                continue;
            }
            let result = if code_size_rescue
                && result.bytecode.len()
                    > runtime_code_size_limit.expect("code-size rescue requires a size limit")
            {
                gas_first_result.take().expect("code-size rescue must retain the gas-first runtime")
            } else {
                result
            };
            self.runtime_immutable_refs = result.immutable_refs;
            return GeneratedCode {
                bytecode: result.bytecode,
                evm_ir: result.evm_ir,
                debug_info: result.debug_info,
            };
        }
    }

    fn reset_runtime_codegen(&mut self, module: &Module) {
        self.asm.clear();
        self.asm.set_artifact_kind(ArtifactKind::Runtime);
        self.asm.set_evm_ir_name(module.name.name);
        self.asm.load_data(module);
        self.block_labels.clear();
        self.function_labels.clear();
        self.empty_stop_functions.clear_to(module.functions.len());
        self.function_spill_sizes.clear();
        self.pending_frame_size_consts.clear();
        self.restorable_internal_frames.clear_to(module.functions.len());
        self.static_frame_functions.clear_to(module.functions.len());
        self.static_frame_addr_consts.clear();
        self.external_spill_addr_consts.clear();
        self.pending_static_allocs.clear();
        self.runtime_free_memory_consts.clear();
        self.runtime_entry_reachability.clear();
        self.runtime_entry_funcs.clear();
        self.current_internal_function = None;
        self.block_copies.clear();
        self.stack_phi_sources.clear();
        self.static_call_abis.clear();
        self.recursive_stack_functions.clear_to(module.functions.len());
        self.recursive_frame_functions.clear_to(module.functions.len());
        self.recursive_frame_edges.clear();
        self.recursion_reaching_functions.clear_to(module.functions.len());
        self.function_stack_peaks.clear();
        self.icall_stack_edges.clear();
        self.runtime_stack_args = true;
        self.stack_returns_enabled = true;
        self.emitting_entry = false;
        self.reset_switch_gas_code_growth();
    }

    /// Validates the complete physical stack, including words intentionally
    /// hidden below each function's scheduler model. The local high-water
    /// marks are exact for the emitted bodies; call-edge propagation is
    /// conservative for tail calls, which may carry any locally observed
    /// stack into their target. Recursive regions are excluded from the
    /// optimization before emission because their incoming prefix is
    /// intentionally unbounded.
    fn caller_stack_prefixes_fit(&self, module: &Module, max_stack_depth: usize) -> bool {
        let Some(entry_id) = module.dispatch_entry() else {
            return self.function_stack_peaks.values().all(|&peak| peak <= max_stack_depth);
        };

        self.stack_prefixes_fit_from(module, entry_id, max_stack_depth)
    }

    fn report_stack_limit_error(&self) {
        self.gcx
            .dcx()
            .err(format!(
                "codegen cannot keep the generated EVM stack within {MAX_STACK_DEPTH} words"
            ))
            .emit();
    }

    fn stack_prefixes_fit_from(
        &self,
        module: &Module,
        entry_id: FunctionId,
        max_stack_depth: usize,
    ) -> bool {
        if self.function_stack_peaks.values().any(|&peak| peak > max_stack_depth) {
            return false;
        }

        let mut incoming: IndexVec<FunctionId, Option<usize>> =
            index_vec![None; module.functions.len()];
        incoming[entry_id] = Some(0);
        for _ in 0..module.functions.len() {
            let mut changed = false;
            for edge in &self.icall_stack_edges {
                if self.recursive_stack_functions.contains(edge.caller)
                    || self.recursive_stack_functions.contains(edge.callee)
                    || !self.function_stack_peaks.contains_key(&edge.callee)
                {
                    continue;
                }
                let Some(base) = incoming[edge.caller] else { continue };
                let candidate = base.saturating_add(edge.preserved_words).saturating_add(1);
                // Before JUMP consumes its destination, the caller briefly holds the preserved
                // prefix, return address, complete argument tuple, and target label. Arguments
                // become part of the callee's modeled stack (or are consumed by its prologue), so
                // only the preserved prefix and return address propagate as its hidden prefix.
                let entry_transient = edge.argument_words.saturating_add(1);
                // After the callee returns, multiword stack-return adoption stages the
                // buffer pointer and one address above the returned tuple, peaking at
                // `base + preserved + arity + 2` = `candidate + arity + 1`.
                let adoption_transient = self
                    .stack_return_plan(edge.callee)
                    .map_or(0, |plan| if plan.arity > 1 { plan.arity + 1 } else { 0 });
                if candidate.saturating_add(entry_transient.max(adoption_transient))
                    > max_stack_depth
                {
                    return false;
                }
                if incoming[edge.callee].is_none_or(|current| candidate > current) {
                    incoming[edge.callee] = Some(candidate);
                    changed = true;
                }
            }

            for (caller, func) in module.functions.iter_enumerated() {
                if self.recursive_stack_functions.contains(caller) {
                    continue;
                }
                let Some(base) = incoming[caller] else { continue };
                let carried = self.function_stack_peaks.get(&caller).copied().unwrap_or(0);
                for block in &func.blocks {
                    let Some(Terminator::TailCall { function: callee, .. }) = &block.terminator
                    else {
                        continue;
                    };
                    if self.recursive_stack_functions.contains(*callee)
                        || !self.function_stack_peaks.contains_key(callee)
                    {
                        continue;
                    }
                    let candidate = base.saturating_add(carried);
                    // Tail calls carry the caller stack and briefly push only
                    // the target label; they do not add a return address.
                    if candidate.saturating_add(1) > max_stack_depth {
                        return false;
                    }
                    if incoming[*callee].is_none_or(|current| candidate > current) {
                        incoming[*callee] = Some(candidate);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }

        incoming.iter_enumerated().all(|(func_id, incoming)| {
            incoming.is_none_or(|incoming| {
                incoming
                    .saturating_add(self.function_stack_peaks.get(&func_id).copied().unwrap_or(0))
                    <= max_stack_depth
            })
        })
    }

    /// Emits a runtime from final-phase MIR.
    ///
    /// Selector matching, receive/fallback routing, and callvalue checks all
    /// live in the MIR `entry`, whose `tail_call`s jump to the ABI wrappers.
    fn emit_runtime(&mut self, module: &Module, call_graph: &CallGraphInfo) {
        let Some(entry_id) = module.dispatch_entry() else {
            assert!(
                !module.functions.iter().any(Self::is_external_entry),
                "evm-shaped module with a runtime interface must have a MIR `entry` function"
            );
            return;
        };

        let mut classified_recursive_frames = DenseBitSet::new_empty(module.functions.len());
        for (root, _) in module.functions.iter_enumerated() {
            if !call_graph.is_recursive(root) || classified_recursive_frames.contains(root) {
                continue;
            }
            let component = call_graph.recursive_component(root);
            classified_recursive_frames.union(&component);
            let supported = if component.count() == 1 {
                Self::uses_reentrant_static_frame(root, &module.functions[root])
            } else {
                // The validated mutual-recursion shape is JSON-style Yul:
                // each helper returns a two-word tuple and recursive edges go
                // through another component member. A direct multi-return
                // self edge could overwrite child results while restoring the
                // suspended copy of that same frame, so keep it dynamic.
                component.iter().all(|func_id| {
                    let func = &module.functions[func_id];
                    func.attributes.is_yul
                        && func.returns.len() == 2
                        && Self::static_frame_offsets_are_local(func)
                        && !Self::has_direct_self_call(func_id, func)
                })
            };
            if supported {
                self.recursive_frame_functions.union(&component);
                for caller in component.iter() {
                    for callee in component.iter() {
                        self.recursive_frame_edges.insert((caller, callee));
                    }
                }
            }
        }

        for (func_id, func) in module.functions.iter_enumerated() {
            if func.blocks.len() == 1
                && func.blocks[BlockId::ENTRY].instructions.is_empty()
                && matches!(func.blocks[BlockId::ENTRY].terminator, Some(Terminator::Stop))
            {
                self.empty_stop_functions.insert(func_id);
            }
            if call_graph.is_recursive(func_id) {
                self.recursive_stack_functions.insert(func_id);
                self.recursive_stack_functions.union(&call_graph.reachable_callees_from([func_id]));
            }
            if call_graph.is_recursive(func_id)
                || call_graph
                    .reachable_callees_from([func_id])
                    .iter()
                    .any(|callee| call_graph.is_recursive(callee))
            {
                self.recursion_reaching_functions.insert(func_id);
            }
        }
        let internal_targets = call_graph.reachable_callees_from(
            module.functions.iter_enumerated().filter_map(|(func_id, func)| {
                (func_id == entry_id || Self::is_external_entry(func)).then_some(func_id)
            }),
        );

        for (func_id, func) in module.functions.iter_enumerated() {
            if !func.attributes.may_return_memory
                && !func.params.iter().chain(&func.returns).any(|ty| ty.is_memory_reference())
            {
                self.restorable_internal_frames.insert(func_id);
            }
            // Internal functions get compile-time-fixed frames. Recursive
            // activations reuse their function's scratch frame after carrying
            // the suspended activation's live state on the EVM stack.
            if func_id != entry_id
                && !Self::is_external_entry(func)
                && Self::is_runtime_function(func)
                && (!call_graph.is_recursive(func_id)
                    || self.recursive_frame_functions.contains(func_id))
                && Self::static_frame_offsets_are_local(func)
            {
                self.static_frame_functions.insert(func_id);
            }
        }
        if self.runtime_stack_args {
            self.compute_stack_arg_masks(module);
            let stack_arg_values = self.collect_canonical_stack_arg_values(module);
            self.compute_resident_stack_args(module, &stack_arg_values);
            let stack_arg_uses = self.collect_stack_arg_uses(module);
            self.compute_lazy_stack_args(module, &stack_arg_values, &stack_arg_uses);
            self.compute_direct_stack_args(module, &stack_arg_values, &stack_arg_uses);
        }
        self.compute_stack_return_plans(module);
        // Labels for every tail-call and internal-call target.
        for (func_id, func) in module.functions.iter_enumerated() {
            if func_id == entry_id {
                continue;
            }
            let needs_body = Self::is_external_entry(func)
                || (Self::is_runtime_function(func) && internal_targets.contains(func_id));
            if needs_body {
                let label = self.new_function_label(func_id);
                self.function_labels.insert(func_id, label);
            }
        }

        // The MIR entry only dispatches. External wrappers initialize the free-memory pointer on
        // demand with a floor sized for their own reachable static frames.
        self.in_internal_function = false;
        self.emitting_entry = true;
        self.generate_function_body(entry_id, &module.functions[entry_id]);
        self.emitting_entry = false;
        self.record_function_spill_size(entry_id);
        self.runtime_entry_funcs.push(entry_id);

        // External entries, reached only through `tail_call` jumps.
        for (func_id, func) in module.functions.iter_enumerated() {
            if func_id == entry_id || !Self::is_external_entry(func) {
                continue;
            }
            let Some(&label) = self.function_labels.get(&func_id) else { continue };
            self.asm.define_label(label);
            self.mark_debug_function_invoke(func);
            self.in_internal_function = false;
            self.emit_entry_free_memory_start(module, call_graph, func_id);
            self.generate_function_body(func_id, func);
            self.record_function_spill_size(func_id);
            self.runtime_entry_funcs.push(func_id);
        }

        // Internal-call targets.
        for (func_id, func) in module.functions.iter_enumerated() {
            if func_id == entry_id
                || Self::is_external_entry(func)
                || !Self::is_runtime_function(func)
            {
                continue;
            }
            let Some(&label) = self.function_labels.get(&func_id) else { continue };
            self.asm.define_label(label);
            self.mark_debug_function_invoke(func);
            self.emit_stack_arg_prologue(func_id, func);
            self.in_internal_function = true;
            self.current_internal_function = Some(func_id);
            self.generate_function_body(func_id, func);
            self.in_internal_function = false;
            self.current_internal_function = None;
            self.record_function_spill_size(func_id);
        }

        self.resolve_pending_frame_size_consts(module);
        self.resolve_static_frames(module);
    }

    /// Records the exact spill area size of the function body that just emitted.
    fn record_function_spill_size(&mut self, func_id: FunctionId) -> u64 {
        let spill_size = u64::from(self.scheduler.spills.spill_area_size());
        self.function_spill_sizes.insert(func_id, spill_size);
        spill_size
    }

    fn mark_debug_function_invoke(&mut self, func: &Function) {
        if self.capture_debug_info
            && !func.declaration_span.is_dummy()
            && let Some(identifier) = func.debug_identifier
        {
            self.asm.mark_function_invoke(DebugFunction {
                identifier,
                declaration: func.declaration_span,
            });
        }
    }

    fn mark_debug_function_exit(&mut self, func: &Function, exit: DebugFunctionExit) {
        if self.capture_debug_info
            && !func.declaration_span.is_dummy()
            && func.debug_identifier.is_some()
        {
            self.asm.mark_function_exit(exit);
        }
    }

    /// Returns the exact spill area recorded for `func_id` after emission.
    fn function_spill_size(&self, func_id: FunctionId) -> u64 {
        self.function_spill_sizes.get(&func_id).copied().unwrap_or_else(|| {
            panic!("spill size for emitted function {func_id:?} was not recorded")
        })
    }

    /// Resolves all pending internal-call frame-size constants.
    ///
    /// Every pending constant belongs to a labeled callee. Runtime and
    /// constructor emission record all labeled bodies before reaching this
    /// resolution point.
    fn resolve_pending_frame_size_consts(&mut self, module: &Module) {
        for (id, callee) in std::mem::take(&mut self.pending_frame_size_consts) {
            self.asm.set_deferred_const(id, U256::from(self.emitted_frame_size(module, callee)));
        }
    }

    /// Whether a directly self-recursive Yul helper can reuse one static
    /// scratch frame while suspended activations carry their live state on the
    /// EVM stack.
    fn uses_reentrant_static_frame(func_id: FunctionId, func: &Function) -> bool {
        func.attributes.is_yul
            && func.returns.len() == 1
            && Self::has_direct_self_call(func_id, func)
    }

    fn has_direct_self_call(func_id: FunctionId, func: &Function) -> bool {
        func.instructions().any(|inst_id| {
            matches!(func.inst(inst_id).kind, InstKind::ICall { function, .. }
                if function == func_id)
        }) || func.blocks.iter().any(|block| {
            matches!(block.terminator, Some(Terminator::TailCall { function, .. })
                if function == func_id)
        })
    }

    fn is_external_entry(func: &Function) -> bool {
        Self::is_runtime_function(func)
            && (func.selector.is_some()
                || func.attributes.is_receive
                || func.attributes.is_fallback)
    }

    fn is_runtime_function(func: &Function) -> bool {
        !func.attributes.is_constructor
    }

    /// Returns whether every explicit frame address belongs to the local region above the dynamic
    /// header and signature slots.
    ///
    /// Static frames omit the header, while stack-only arguments and returns may omit signature
    /// slots. Parsed MIR can address those regions directly, without identifying the aliased
    /// component, so such a function must keep the ordinary dynamic-frame convention.
    fn static_frame_offsets_are_local(func: &Function) -> bool {
        let Some(signature_slots) = func.params.len().checked_add(func.returns.len()) else {
            return false;
        };
        let Some(signature_size) = u64::try_from(signature_slots)
            .ok()
            .and_then(|slots| slots.checked_mul(EvmMemoryLayout::WORD_SIZE))
        else {
            return false;
        };
        let Some(local_start) =
            EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE.checked_add(signature_size)
        else {
            return false;
        };

        let Some(local_end) = local_start.checked_add(func.internal_frame_size) else {
            return false;
        };

        func.instructions().all(|inst_id| match func.inst(inst_id).kind {
            InstKind::InternalFrameAddr(offset) => offset >= local_start && offset < local_end,
            _ => true,
        })
    }

    /// Splits phi-carrying edges out of multi-successor predecessors when a
    /// phi destination is still read before or on a sibling path.
    ///
    /// A phi's parallel copies are emitted in the predecessor before its
    /// terminator, so a conditional predecessor runs them on the edge that
    /// does not reach the phi as well. That is harmless for ordinary join
    /// phis, whose destinations are dead before the terminator and on the
    /// sibling path, but a loop header may test its old phi result in the
    /// latch terminator or keep it live on the exit. Writing the backedge copy
    /// early then makes the branch observe the next iteration's value.
    /// Rerouting such edges through a fresh jump-only block gives the copies
    /// an unconditional home after the predecessor's branch.
    fn split_phi_critical_edges(func: &mut Function) {
        let has_phis = func.blocks.iter().any(|block| {
            block
                .instructions
                .iter()
                .any(|&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
        });
        if !has_phis {
            return;
        }
        let liveness = Liveness::compute(func);

        let mut splits: Vec<(BlockId, BlockId)> = Vec::new();
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                let InstKind::Phi(incoming) = &func.inst(inst_id).kind else { continue };
                let Some(dst) = func.inst_result_value(inst_id) else { continue };
                for &(pred, src) in incoming {
                    if src == dst || splits.contains(&(pred, block_id)) {
                        continue;
                    }
                    let Some(terminator) = func.blocks[pred].terminator.as_ref() else { continue };
                    let successors = terminator.successors();
                    if terminator.operands().contains(&dst)
                        || successors
                            .iter()
                            .any(|&succ| succ != block_id && liveness.live_in(succ).contains(dst))
                    {
                        splits.push((pred, block_id));
                    }
                }
            }
        }

        for (pred, succ) in splits {
            let edge = func.alloc_block();
            func.blocks[edge].terminator = Some(Terminator::Jump(succ));
            func.blocks[edge].predecessors.push(pred);
            match func.blocks[pred].terminator.as_mut() {
                Some(Terminator::Branch { then_block, else_block, .. }) => {
                    if *then_block == succ {
                        *then_block = edge;
                    }
                    if *else_block == succ {
                        *else_block = edge;
                    }
                }
                Some(Terminator::Switch { default, cases, .. }) => {
                    if *default == succ {
                        *default = edge;
                    }
                    for (_, target) in cases {
                        if *target == succ {
                            *target = edge;
                        }
                    }
                }
                _ => continue,
            }
            for pred_entry in &mut func.blocks[succ].predecessors {
                if *pred_entry == pred {
                    *pred_entry = edge;
                }
            }
            let phi_insts: Vec<InstId> = func.blocks[succ]
                .instructions
                .iter()
                .copied()
                .filter(|&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
                .collect();
            for inst_id in phi_insts {
                if let InstKind::Phi(incoming) = &mut func.inst_mut(inst_id).kind {
                    for (incoming_pred, _) in incoming {
                        if *incoming_pred == pred {
                            *incoming_pred = edge;
                        }
                    }
                }
            }
        }
    }

    /// Generates the body of a function.
    fn generate_function_body(&mut self, func_id: FunctionId, func: &Function) {
        let stack_only_disabled_at_entry = self.stack_only_function_disabled(func_id);
        let report_missing_spill_home = self.gcx.sess.opts.unstable.assert_planned_edge_spill_home;
        let liveness = self
            .emitting_entry
            .then(|| Liveness::compute_block_local_for_codegen(func))
            .flatten()
            .unwrap_or_else(|| Liveness::compute(func));
        let liveness = &liveness;
        let cross_block_live = OnceCell::new();

        self.spill_hazard_insts = self.compute_spill_hazard_insts(func);

        // Eliminate phis.
        self.block_copies.clear();
        self.elided_insts.clear();
        self.collect_late_gas_operands(func);
        let phi_result = PhiEliminator::analyze(func);
        let has_phis = !phi_result.block_copies.is_empty();
        for (block_id, copies) in phi_result.block_copies {
            self.block_copies.insert(block_id, copies.copies);
        }
        // Stack-phi planning starts with loop analysis, but cannot produce a
        // plan without a phi. Avoid that analysis for the overwhelmingly
        // common phi-free function.
        let mut stack_phi_plan = if has_phis {
            StackPhiPlan::analyze(func, liveness, &self.cold_functions)
        } else {
            StackPhiPlan::default()
        };
        let resident_stack_plan = self.resident_stack_plan(func_id).cloned();
        let existing_stack_only_values = self.stack_only_values(func_id, true);
        let hazard_recomputable =
            cross_block_values(func, |value| !existing_stack_only_values.contains(&value));
        let hazard_cross_block_values = self.spill_hazard_cross_block_values(
            func,
            liveness,
            &cross_block_live,
            &hazard_recomputable,
        );
        let resident_carries_hazards = resident_stack_plan.as_ref().is_some_and(|plan| {
            self.stack_plan_carries_spill_hazards(func, liveness, plan, &hazard_cross_block_values)
        });
        let mut protected_stack_values =
            self.resident_stack_args(func_id).map_or_else(Vec::new, |values| values.to_vec());
        for &value in &hazard_cross_block_values {
            if !protected_stack_values.contains(&value) {
                protected_stack_values.push(value);
            }
        }
        let hazard_stack_layout = (!hazard_cross_block_values.is_empty()
            && !resident_carries_hazards)
            .then(|| {
                self.compute_spill_hazard_stack_layout(
                    func,
                    liveness,
                    &stack_phi_plan,
                    &protected_stack_values,
                )
            })
            .flatten();
        if !hazard_cross_block_values.is_empty()
            && hazard_stack_layout.is_none()
            && !resident_carries_hazards
        {
            self.gcx
                .dcx()
                .err(format!(
                    "codegen cannot preserve values across a low-memory forwarding buffer in `{}`",
                    func.name
                ))
                .emit();
            return;
        }
        let hazard_stack_values = hazard_stack_layout.as_ref().map(|(values, _)| values.as_slice());
        let hazard_stack_plan =
            hazard_stack_layout.as_ref().map(|(_, plan)| plan.clone()).or_else(|| {
                if resident_carries_hazards { resident_stack_plan.clone() } else { None }
            });
        let has_hazard_stack_plan = hazard_stack_plan.is_some();
        let required_stack_plan = resident_stack_plan.is_some() || hazard_stack_plan.is_some();
        let mut global_stack_plan = hazard_stack_plan
            .clone()
            .or(resident_stack_plan)
            .unwrap_or_else(|| GlobalStackPlan::analyze(func, liveness, &stack_phi_plan));
        let mut stack_phi_sources = stack_phi_plan.edge_sources();
        if required_stack_plan {
            if !stack_phi_plan.merge_resident(func, &global_stack_plan) {
                // Selection preflights this exact composition. If a future transform invalidates
                // that proof, regenerate the runtime with the ordinary frame-backed convention
                // instead of emitting a partial stack ABI or panicking.
                self.disabled_stack_only_functions.insert(func_id);
                return;
            }
            stack_phi_sources = stack_phi_plan.edge_sources();
        } else if global_stack_plan.is_empty()
            && let Some((values, plan)) =
                self.compute_cross_block_stack_layout(
                    func,
                    liveness,
                    &cross_block_live,
                    has_phis,
                )
            // Phi layouts own their incoming stack on planned joins. Adopt the layout only when
            // that composition is proven, mirroring the resident arm.
            && stack_phi_plan.merge_resident(func, &plan)
        {
            global_stack_plan = plan;
            // An early spill store can be omitted only when every physical successor layout
            // carries the value; an edge-specific cleanup may otherwise discard the sole stack
            // copy before a later block reloads its reserved slot. The reserved spill slot
            // remains available if edge emission ever falls back to the memory convention.
            stack_phi_sources = stack_phi_plan.edge_sources();
            for (block_id, block) in func.blocks.iter_enumerated() {
                let Some(term) = block.terminator.as_ref() else { continue };
                let carried = global_stack_plan.uniformly_carried_values(func, term);
                let sources = stack_phi_sources.entry(block_id).or_default();
                for &value in &values {
                    if carried.contains(&value) && !sources.contains(&value) {
                        sources.push(value);
                    }
                }
            }
        }
        // A planned edge consumes its stack sources, so the emitter skips
        // their spill stores. A source with uses beyond the edge's phis —
        // live past the successor or read by one of its own instructions —
        // loses its only stack copy to the edge, and a later block reloads
        // its reserved slot: the store must stay unless a planned successor
        // layout keeps the value stack-resident. A materialization loop's
        // base pointer feeding a second copy loop reloaded uninitialized
        // memory this way. Phi operands count as uses in the merge block, so
        // `live_in` alone cannot separate edge-only sources from these.
        //
        // A successor's entry layout only proves one hop: the value must be
        // carried by the planned entry of EVERY block where it stays live, or
        // an edge cleanup on some uncovered path drops the sole stack copy
        // and a later block reloads its reserved slot uninitialized (a
        // diamond over a multi-return value followed by an internal call read
        // stale decode scratch this way).
        let carried_at = |block: BlockId, value: ValueId| {
            stack_phi_plan.entries.get(&block).is_some_and(|entry| entry.contains(&value))
                || global_stack_plan.entries.get(&block).is_some_and(|entry| entry.contains(&value))
        };
        let mut carried_everywhere = FxHashMap::<ValueId, bool>::default();
        for (block_id, sources) in &mut stack_phi_sources {
            let Some(term) = func.blocks[*block_id].terminator.as_ref() else { continue };
            let successors = term.successors();
            sources.retain(|&value| {
                successors.iter().all(|&succ| {
                    if carried_at(succ, value)
                        && *carried_everywhere.entry(value).or_insert_with(|| {
                            func.blocks.indices().all(|block| {
                                !liveness.live_in(block).contains(value) || carried_at(block, value)
                            })
                        })
                    {
                        return true;
                    }
                    let block = &func.blocks[succ];
                    let used_past_phis = liveness.live_out(succ).contains(value)
                        || block.instructions.iter().any(|&inst_id| {
                            let inst = func.inst(inst_id);
                            !matches!(inst.kind, InstKind::Phi(_))
                                && inst.kind.operands().contains(&value)
                        })
                        || block
                            .terminator
                            .as_ref()
                            .is_some_and(|term| term.operands().contains(&value));
                    !used_past_phis
                })
            });
        }
        if has_hazard_stack_plan {
            for (block_id, block) in func.blocks.iter_enumerated() {
                let Some(term) = block.terminator.as_ref() else { continue };
                let sources = stack_phi_sources.entry(block_id).or_default();
                for successor in term.successors() {
                    for &value in global_stack_plan.entry(successor).into_iter().flatten() {
                        if !sources.contains(&value) {
                            sources.push(value);
                        }
                    }
                }
            }
        }
        self.stack_phi_sources = stack_phi_sources;
        self.global_stack_active = !global_stack_plan.is_empty();
        self.global_stack_aliases = global_stack_plan.aliases.clone();

        // Reset scheduler
        self.scheduler.reset();
        self.spill_addr_consts.clear();
        self.spill_stores.clear();
        self.spill_loads.clear();
        self.early_spill_removals.clear();
        self.function_ir_block_start = self.asm.block_count();

        // Cross-block rematerialization is selected during spill preallocation. Record every
        // argument without a frame home before that analysis so an expression depending on one is
        // stored instead of later being rebuilt after its only physical copy was consumed.
        let mut initial_stack_only_values = self.stack_only_values(func_id, true);
        initial_stack_only_values.extend(hazard_stack_values.into_iter().flatten().copied());
        self.scheduler.set_stack_only_values(func.num_values(), initial_stack_only_values);

        self.preallocate_cross_block_spills(func, liveness, &cross_block_live);

        self.cold_blocks = self.collect_cold_blocks(func);

        // Create labels for each block
        self.block_labels.clear();
        for block_id in func.blocks.indices() {
            let label = self.asm.new_label();
            if self.block_is_cold(block_id) {
                self.asm.mark_label_cold(label);
            }
            self.block_labels.insert(block_id, label);
        }

        // Generate each block.
        let block_order = self.block_layout_order(func);
        let block_pos: FxHashMap<BlockId, usize> =
            block_order.iter().enumerate().map(|(pos, &b)| (b, pos)).collect();
        // Stack layout a block must start with when it is reached by a stack-
        // preserving jump from its single predecessor (recorded by that
        // predecessor, restored here).
        let mut block_entry_stacks: FxHashMap<BlockId, StackModel> = FxHashMap::default();
        let mut preserved_fallthrough: Option<BlockId> = None;
        // A spill store is a path fact, not a function fact: a store emitted
        // in one branch arm must not satisfy reloads on paths that bypass it.
        // Track store availability per emitted block — a slot is trustworthy
        // at a block only when every forward predecessor makes it available —
        // and drop the scheduler's stored guarantee where a live value is not
        // available, so that path stores again before any reload.
        let store_cfg = CfgInfo::new(func);
        let mut spill_avail_out: FxHashMap<BlockId, FxHashSet<ValueId>> = FxHashMap::default();
        for (pos, &block_id) in block_order.iter().enumerate() {
            let block = &func.blocks[block_id];
            let fallthrough = block_order.get(pos + 1).copied();
            if self.capture_debug_info {
                let modifier_depth = block
                    .instructions
                    .first()
                    .map(|&inst_id| func.inst(inst_id).metadata.modifier_depth())
                    .unwrap_or(0);
                self.asm.set_modifier_depth(modifier_depth);
            }
            let entered_by_preserved_fallthrough = preserved_fallthrough == Some(block_id);
            preserved_fallthrough = None;
            let label = self.block_labels[&block_id];
            if !entered_by_preserved_fallthrough && !block.predecessors.is_empty() {
                self.asm.define_label(label);
            }

            // Reset stack at block entry unless the block is reached with a
            // known live stack: a physical fallthrough carries the scheduler's
            // stack directly, and a stack-preserving jump from a single
            // predecessor restores the recorded layout. All other cross-block
            // values live in spill slots.
            if !entered_by_preserved_fallthrough {
                if let Some(entry_stack) = block_entry_stacks.remove(&block_id) {
                    let max_depth = self.scheduler.stack.max_depth();
                    self.scheduler.stack = entry_stack;
                    self.scheduler.stack.inherit_max_depth(max_depth);
                    // Live-ins not on the carried stack still arrive in memory.
                    self.mark_live_in_spills(func, liveness, block_id);
                } else if let Some(entry) = stack_phi_plan.entries.get(&block_id) {
                    self.set_stack_to_values(entry);
                    self.mark_live_in_spills(func, liveness, block_id);
                } else if let Some(entry) = global_stack_plan.entry(block_id) {
                    self.set_stack_to_values(entry);
                    self.mark_live_in_spills(func, liveness, block_id);
                } else {
                    self.scheduler.clear_stack();
                    self.mark_live_in_spills(func, liveness, block_id);
                }
            }
            // A spill store is a path fact, not a function fact: a store
            // emitted in a sibling branch arm sets the global `stored` flag,
            // which must not suppress this path's own store. Intersect store
            // availability over emitted forward predecessors (loop back edges
            // are exempt: a value redefined around a loop is handled by the
            // carried-phi invalidation, and its pre-loop store stays valid; a
            // predecessor emitted later stores every value live across the edge
            // before jumping here, including the ones a preserved stack edge
            // would otherwise leave to this block).
            let mut avail_in: Option<FxHashSet<ValueId>> = None;
            for &pred in func.blocks[block_id].predecessors.iter() {
                if store_cfg.dominators().dominates(block_id, pred) {
                    continue;
                }
                let pred_avail = spill_avail_out.get(&pred);
                match (&mut avail_in, pred_avail) {
                    (None, Some(pred_avail)) => avail_in = Some(pred_avail.clone()),
                    (Some(set), Some(pred_avail)) => {
                        set.retain(|value| pred_avail.contains(value));
                    }
                    (_, None) => {}
                }
            }
            self.spill_available = avail_in;
            // A store emitted by a sibling branch arm also marked its value
            // reloadable, so a copy carried in on the stack could be dropped
            // in favor of a slot this path never wrote. Forget stores that are
            // not available on every emitted forward predecessor.
            if let Some(available) = &self.spill_available {
                let stale: Vec<ValueId> = self
                    .scheduler
                    .spills
                    .reloadable_values()
                    .filter(|&value| {
                        !available.contains(&value) && self.scheduler.stack.contains(value)
                    })
                    .collect();
                for value in stale {
                    self.scheduler.spills.invalidate_stored(value);
                }
            }
            self.invalidate_carried_phi_spills(func, block_id);
            if block_id == BlockId::ENTRY
                && let Some(values) =
                    self.resident_stack_args(func_id).map(|values| values.to_vec())
            {
                debug_assert_eq!(self.scheduler.stack.depth(), 0);
                self.set_stack_to_values(&values);
            } else if block_id == BlockId::ENTRY
                && let Some(values) = self.direct_stack_args(func_id).map(|values| values.to_vec())
            {
                debug_assert_eq!(self.scheduler.stack.depth(), 0);
                self.set_stack_to_values(&values);
            } else if block_id == BlockId::ENTRY
                && let Some(plan) = self.lazy_stack_args(func_id).cloned()
            {
                debug_assert_eq!(self.scheduler.stack.depth(), 0);
                self.set_stack_to_values(&plan.values().collect::<Vec<_>>());
            }
            // Resident and direct arguments have no frame fallback.
            let mut stack_only_values = self.stack_only_values(func_id, block_id == BlockId::ENTRY);
            stack_only_values.extend(hazard_stack_values.into_iter().flatten().copied());
            self.scheduler.set_stack_only_values(func.num_values(), stack_only_values);
            if block_id != BlockId::ENTRY
                && self.resident_stack_args(func_id).is_some()
                && !stack_phi_plan.entries.contains_key(&block_id)
            {
                let live_in = liveness.live_in(block_id);
                let needed: Vec<_> = self
                    .scheduler
                    .stack
                    .iter()
                    .flatten()
                    .filter(|value| live_in.contains(*value))
                    .collect();
                self.pop_stack_values_not_needed_by(&needed);
            }

            // Generate instructions
            let mut pinned_hazard_values = FxHashSet::<ValueId>::default();
            for (inst_idx, &inst_id) in block.instructions.iter().enumerate() {
                let inst = func.inst(inst_id);

                // Skip phi instructions (they're handled by copies)
                if matches!(inst.kind, InstKind::Phi(_)) {
                    continue;
                }

                // Skip multi-return protocol instructions already satisfied
                // from adopted stack-return words.
                if self.elided_insts.remove(&inst_id) {
                    continue;
                }

                if self.capture_debug_info {
                    self.asm.set_source_span(inst.metadata.source_span());
                    self.asm.set_modifier_depth(inst.metadata.modifier_depth());
                }

                // A whole-calldata-forwarding clobber overwrites the low memory
                // the spill area lives in. Reload every value live across it
                // onto the stack while the slot is still valid, and drop the
                // stored flag so nothing reloads the clobbered slot. The slot is
                // NOT re-stored here: the clobbered range is the very memory the
                // following forward reads, so writing it back would corrupt the
                // forwarded input. The value rides the stack, which the write
                // never touches; live-out values re-store once at block end.
                // Only values still needed past the clobber are pinned: a phi
                // source or an operand the copy itself consumes has its last
                // recorded use in this block at or before it, and reloading it
                // would only deepen the stack with a dead word.
                if self.spill_hazard_insts.contains(&inst_id) {
                    let at_risk: Vec<ValueId> = self
                        .scheduler
                        .spills
                        .reloadable_values()
                        .filter(|&value| {
                            liveness.is_used_at_or_after(value, block_id, inst_idx + 1)
                        })
                        .collect();
                    for value in at_risk {
                        let recomputable = self.scheduler.spills.is_recomputable(value);
                        if !recomputable {
                            if !self.scheduler.stack.contains(value) {
                                self.emit_value(func, value);
                            }
                            pinned_hazard_values.insert(value);
                        }
                        self.scheduler.spills.invalidate_stored(value);
                        if let Some(available) = &mut self.spill_available {
                            available.remove(&value);
                        }
                    }
                }

                // Find the value ID that corresponds to this instruction (if any)
                let result_value = func.inst_result_value(inst_id);

                // Generate the instruction
                self.generate_inst(
                    func_id,
                    inst_id,
                    func,
                    &inst.kind,
                    liveness,
                    block_id,
                    inst_idx,
                    result_value,
                );
                if !stack_only_disabled_at_entry && self.stack_only_function_disabled(func_id) {
                    return;
                }
                if let Some(result) = result_value {
                    self.spill_reserved_result_if_live(func, liveness, block_id, inst_idx, result);
                    // A free-memory-pointer load cannot be rematerialized once
                    // the pointer moves. Park every FMP load at its
                    // definition so later uses reload the original value —
                    // whether the definition crosses a block on a preserved
                    // edge or is re-materialized between two allocations in
                    // its own block.
                    if matches!(
                        inst.kind,
                        InstKind::MLoad(addr)
                            if func.value_u64(addr) == Some(EvmMemoryLayout::FMP_SLOT)
                    ) {
                        self.spill_value_if_needed(func, result);
                    }
                }
            }
            if self.capture_debug_info {
                self.asm.set_source_span(None);
            }

            // Every clobber in this block has now been emitted, so its spill
            // slots are safe to write again. Re-store a pinned value only if a
            // successor reloads it; values consumed within this block stay
            // stack-resident and need no memory home, so drop their obligation.
            if !pinned_hazard_values.is_empty() {
                let live_out = liveness.live_out(block_id);
                let hazard_carried = block.terminator.as_ref().map_or_else(Vec::new, |term| {
                    hazard_stack_plan
                        .as_ref()
                        .map_or_else(Vec::new, |plan| plan.uniformly_carried_values(func, term))
                });
                for value in std::mem::take(&mut pinned_hazard_values) {
                    if live_out.contains(value) && !hazard_carried.contains(&value) {
                        self.spill_value_if_needed(func, value);
                    }
                }
            }

            // A preserved stack edge hands its carried values to the successor,
            // which stores the ones it drops. An already-emitted successor
            // chose those stores without seeing this path, so it cannot take
            // the obligation over: store everything live across the edge here,
            // which is what the store-availability intersection assumes a
            // later-emitted predecessor does. Back edges are exempt for the
            // same reason that intersection skips them.
            //
            // Only a planned edge can preserve into an already-emitted block:
            // its successor rebuilds the layout from the plan at its own entry,
            // whichever order the two were emitted in. Fallthrough, jump, and
            // branch preservation all require a single-predecessor target that
            // is still ahead, other than an arm whose live-ins are immediates,
            // and an unpreserved terminator spills every live-out value at
            // block end regardless, so storing here would only move that store
            // earlier at the cost of a deeper `dup`.
            //
            //   dup depth(value)
            //   push slot(value)
            //   mstore
            let planned_stack_edge = stack_phi_plan.edges.contains_key(&block_id)
                || stack_phi_plan.branch_edges.contains_key(&block_id)
                || block.terminator.as_ref().is_some_and(|term| {
                    global_stack_plan.edge_layout(func, term).is_some()
                        || global_stack_plan.branch_layouts(term).is_some()
                        || global_stack_plan.switch_layouts(term).is_some()
                });
            let emitted_successors = block
                .terminator
                .as_ref()
                .filter(|_| planned_stack_edge)
                .map(Terminator::successors)
                .unwrap_or_default();
            for successor in emitted_successors {
                if block_pos.get(&successor).is_some_and(|&target| target < pos)
                    && !store_cfg.dominators().dominates(successor, block_id)
                {
                    let live_in = liveness.live_in(successor);
                    for value in liveness.live_out(block_id) {
                        if live_in.contains(value) {
                            // `spill_value_if_needed` stores nothing for a value that is
                            // neither on the stack nor validly stored, and re-emitting one
                            // here is not an option: the pushed word would deepen the
                            // stack the preserved layout is built from. It never has to.
                            // Such a value is one the plan carries into the successor,
                            // which rebuilds it from its recorded entry layout and wants no
                            // memory home; anything that has to travel in memory is still
                            // on the stack, already stored, or holds its slot on every
                            // emitted path into this block at this point.
                            //
                            // A value that arrives only from a forward predecessor further
                            // down the stream is the exception: that predecessor stores its
                            // live-out values when it is emitted, later in the stream but
                            // earlier at runtime, so the home is owed rather than missing
                            // and there is nothing to check here yet.
                            //
                            // This is a description of the common cases and not an
                            // invariant the scheduler maintains, so it is logged rather
                            // than asserted. A value that is on this block's stack is
                            // stored right below, and one that is not cannot get a home
                            // here whatever the reason is: pushing it again would deepen
                            // the stack the preserved layout is built from. The store
                            // obligation is then not this block's, and stating whose it is
                            // needs the availability record of the paths this emission
                            // order has not walked yet.
                            //
                            // The check itself walks predecessors with dominance queries,
                            // so it only runs when something can observe its result.
                            if (report_missing_spill_home
                                || tracing::enabled!(tracing::Level::DEBUG))
                                && !self.has_spill_home(func, value)
                                && !planned_entry_carries(
                                    &stack_phi_plan,
                                    &global_stack_plan,
                                    successor,
                                    value,
                                )
                                && Self::forward_predecessors_emitted(
                                    func, &store_cfg, &block_pos, block_id, pos,
                                )
                            {
                                assert!(
                                    !report_missing_spill_home,
                                    "{value:?} lives across the edge from {block_id:?} to \
                                     the already-emitted {successor:?} with no home"
                                );
                                tracing::debug!(
                                    ?value,
                                    ?block_id,
                                    ?successor,
                                    "value lives across the edge to an already-emitted \
                                     block with no home"
                                );
                            }
                            self.spill_value_if_needed(func, value);
                        }
                    }
                }
            }

            let terminator_growth =
                block.terminator.as_ref().map_or(0, Self::terminator_transient_growth);
            self.materialize_deep_stack_args(func_id, func, terminator_growth);
            if !stack_only_disabled_at_entry && self.stack_only_function_disabled(func_id) {
                return;
            }

            let stack_phi_preserved = stack_phi_plan.edges.get(&block_id).is_some_and(|edge| {
                if !self.can_prepare_stack_phi_edge(func, edge) {
                    return false;
                }
                self.spill_live_out_values_except(func, liveness, block_id, &edge.results);
                self.pop_stack_values_not_needed_by(&edge.sources);
                self.try_emit_stack_phi_edge(func, edge)
            });
            let stack_phi_branch_preserved = block
                .terminator
                .as_ref()
                .and_then(|term| {
                    let Terminator::Branch { condition, .. } = term else { return None };
                    stack_phi_plan.branch_edges.get(&block_id).map(|branch| (*condition, branch))
                })
                .is_some_and(|(condition, branch)| {
                    self.can_prepare_stack_phi_branch(func, condition, branch)
                });

            // Insert phi copies before terminator. If the edge was materialized
            // as a stack-resident phi layout, the copies for this unconditional
            // predecessor are represented by the edge stack itself.
            if stack_phi_preserved || stack_phi_branch_preserved {
                self.block_copies.remove(&block_id);
            } else if let Some(copies) = self.block_copies.remove(&block_id) {
                let mut temps = FxHashMap::default();
                for copy in &copies {
                    self.generate_copy(func, copy, &mut temps);
                }
            }

            let global_branch_layouts = block.terminator.as_ref().and_then(|term| {
                global_stack_plan.branch_layouts(term).and_then(|(then_layout, else_layout)| {
                    (then_layout != else_layout)
                        .then(|| (then_layout.to_vec(), else_layout.to_vec()))
                })
            });
            let global_switch_layouts = block.terminator.as_ref().and_then(|term| {
                global_stack_plan.switch_layouts(term).and_then(|layouts| {
                    let first = layouts.first()?.1;
                    layouts.iter().any(|(_, layout)| *layout != first).then(|| {
                        layouts
                            .into_iter()
                            .map(|(target, layout)| (target, layout.to_vec()))
                            .collect::<Vec<_>>()
                    })
                })
            });
            let has_edge_specific_global =
                global_branch_layouts.is_some() || global_switch_layouts.is_some();

            let preserve_stack_to_fallthrough = !has_edge_specific_global
                && self.can_preserve_stack_fallthrough(func, block_id, fallthrough);

            // A jump to a single-predecessor target that is emitted later can
            // keep its live stack instead of spilling: the target has exactly
            // one entry stack (this block's exit), so it can be restored there.
            let preserve_jump_target = (!has_edge_specific_global
                && !preserve_stack_to_fallthrough)
                .then(|| self.single_pred_jump_target(func, block_id, fallthrough))
                .flatten()
                .filter(|target| block_pos.get(target).copied() > Some(pos));

            // A conditional branch whose other arm is a cold revert can carry
            // its single freshly-computed live-out on the stack into the hot
            // arm, which restores it as its recorded entry layout.
            let preserve_branch_targets = if !has_edge_specific_global
                && !preserve_stack_to_fallthrough
                && preserve_jump_target.is_none()
                && !stack_phi_branch_preserved
            {
                self.branch_preserve_targets(func, liveness, block_id, pos, &block_pos)
            } else {
                Vec::new()
            };
            if !preserve_branch_targets.is_empty()
                && let Some(Terminator::Branch { condition, .. }) = block.terminator.as_ref()
                && liveness.live_out(block_id).contains(*condition)
            {
                // JUMPI consumes the condition while the preserved successor layout omits it.
                // Save an instruction result that remains live so either successor can reload
                // the same definition instead of observing an unwritten reserved spill slot.
                self.spill_value_if_needed(func, *condition);
            }
            if !preserve_branch_targets.is_empty() {
                self.remove_dead_carried_spill_stores(
                    func,
                    liveness,
                    block_id,
                    &preserve_branch_targets,
                );
            }

            let global_branch_preserved = if !stack_phi_preserved
                && !stack_phi_branch_preserved
                && let Some((then_layout, else_layout)) = &global_branch_layouts
                && let Some(Terminator::Branch { condition, .. }) = block.terminator.as_ref()
            {
                let union = Self::global_branch_union(then_layout, else_layout);
                self.spill_live_out_values_except(func, liveness, block_id, &union);
                self.try_emit_global_stack_branch(func, *condition, then_layout, else_layout)
            } else {
                None
            };

            let global_switch_preserved = if !stack_phi_preserved
                && !stack_phi_branch_preserved
                && global_branch_preserved.is_none()
                && let Some(layouts) = &global_switch_layouts
                && let Some(term @ Terminator::Switch { .. }) = block.terminator.as_ref()
            {
                let union = Self::global_switch_union(layouts);
                self.spill_live_out_values_except(func, liveness, block_id, &union);
                self.try_emit_global_stack_edge(func, term, &union).then_some(union)
            } else {
                None
            };

            let global_stack_preserved = if global_branch_preserved.is_none()
                && global_switch_preserved.is_none()
                && !preserve_stack_to_fallthrough
                && preserve_jump_target.is_none()
                && preserve_branch_targets.is_empty()
                && !stack_phi_preserved
                && !stack_phi_branch_preserved
                && let Some(term) = block.terminator.as_ref()
                && let Some(layout) = global_stack_plan.edge_layout(func, term)
            {
                self.spill_live_out_values_except(func, liveness, block_id, layout);
                self.try_emit_global_stack_edge(func, term, layout)
            } else {
                false
            };

            let preserve_stack = preserve_stack_to_fallthrough
                || preserve_jump_target.is_some()
                || !preserve_branch_targets.is_empty()
                || stack_phi_preserved
                || stack_phi_branch_preserved
                || global_branch_preserved.is_some()
                || global_switch_preserved.is_some()
                || global_stack_preserved;

            // Spill all live-out values before the terminator so they can be reloaded in successor
            // blocks. For a preserved edge, keep stack values live instead.
            if stack_phi_branch_preserved
                && let Some(branch) = stack_phi_plan.branch_edges.get(&block_id)
                && let Some(Terminator::Branch { then_block, else_block, .. }) =
                    block.terminator.as_ref()
            {
                let arms = [(*then_block, &branch.then_edge), (*else_block, &branch.else_edge)];
                let exempt = branch
                    .union
                    .iter()
                    .copied()
                    .filter(|value| {
                        arms.iter().all(|(arm, edge)| {
                            !liveness.live_in(*arm).contains(*value) || edge.results.contains(value)
                        })
                    })
                    .collect::<Vec<_>>();
                self.spill_live_out_values_except(func, liveness, block_id, &exempt);
            } else if !preserve_stack {
                self.spill_live_out_values(func, liveness, block_id);
            }

            // Generate terminator. An edge-specific resident branch owns its cleanup and jumps.
            if self.capture_debug_info {
                let metadata =
                    block.instructions.last().map(|&inst_id| &func.inst(inst_id).metadata);
                self.asm.set_source_span(metadata.and_then(|metadata| metadata.source_span()));
                self.asm
                    .set_modifier_depth(metadata.map_or(0, |metadata| metadata.modifier_depth()));
            }
            if let (
                Some(union),
                Some((then_layout, else_layout)),
                Some(Terminator::Branch { condition, then_block, else_block }),
            ) = (&global_branch_preserved, &global_branch_layouts, &block.terminator)
            {
                self.emit_global_stack_branch(
                    func,
                    *condition,
                    *then_block,
                    *else_block,
                    then_layout,
                    else_layout,
                    union,
                    fallthrough,
                );
            } else if let (
                Some(union),
                Some(layouts),
                Some(Terminator::Switch { value, default, cases }),
            ) = (&global_switch_preserved, &global_switch_layouts, &block.terminator)
            {
                self.emit_global_stack_switch(func, *value, *default, cases, layouts, union);
            } else if stack_phi_branch_preserved
                && let Some(Terminator::Branch { condition, then_block, else_block }) =
                    block.terminator.as_ref()
                && let Some(branch) = stack_phi_plan.branch_edges.get(&block_id)
            {
                self.emit_stack_phi_branch(
                    func,
                    *condition,
                    *then_block,
                    *else_block,
                    branch,
                    fallthrough,
                );
            } else if let Some(term) = &block.terminator {
                self.generate_terminator(func, term, fallthrough, preserve_stack);
            }
            if self.capture_debug_info {
                self.asm.set_source_span(None);
                self.asm.set_modifier_depth(0);
            }
            self.scheduler.spills.release_block_locals();
            if preserve_stack_to_fallthrough {
                preserved_fallthrough = fallthrough;
            } else if let Some(target) = preserve_jump_target {
                block_entry_stacks.insert(target, self.scheduler.stack.clone());
            }
            for target in preserve_branch_targets {
                let mut entry_stack = self.scheduler.stack.clone();
                // The branch has one physical exit stack, but a cold successor need not retain
                // identities used only by its hot sibling. Keep hot layouts exact: anonymizing
                // their dead slots can turn a loop-carried stack hit into a reload every iteration.
                if self.block_is_cold(target) {
                    let live_in = liveness.live_in(target);
                    entry_stack.forget_values_not_matching(|value| live_in.contains(value));
                }
                block_entry_stacks.insert(target, entry_stack);
            }

            spill_avail_out.insert(
                block_id,
                self.spill_available
                    .clone()
                    .unwrap_or_else(|| self.scheduler.spills.stored_values().collect()),
            );
        }

        let mut peak = self.scheduler.stack.max_depth();
        if self.direct_stack_args(func_id).is_none()
            && self.resident_stack_args(func_id).is_none()
            && self.lazy_stack_args(func_id).is_none()
            && let Some(mask) = self.stack_arg_mask(func_id)
        {
            peak = peak.max(mask.count());
        }
        self.function_stack_peaks.insert(func_id, peak);
        self.remove_dead_spill_stores();
        self.assign_ranked_spill_addrs(func_id);
    }

    /// Returns the target of a stack-preservable jump: the block ends in
    /// `Jump(T)` to a non-fallthrough, single-predecessor block with no phis
    /// (whose copies would otherwise interfere with the carried layout).
    fn single_pred_jump_target(
        &self,
        func: &Function,
        block_id: BlockId,
        fallthrough: Option<BlockId>,
    ) -> Option<BlockId> {
        let Some(Terminator::Jump(target)) = func.blocks[block_id].terminator.as_ref() else {
            return None;
        };
        if Some(*target) == fallthrough
            || func.blocks[*target].predecessors.as_slice() != [block_id]
        {
            return None;
        }
        let has_phi = func.blocks[*target]
            .instructions
            .iter()
            .any(|&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)));
        (!has_phi).then_some(*target)
    }

    /// Returns branch successors that can receive the current stack layout.
    ///
    /// This handles loop headers after stack-resident phi planning: the header
    /// computes the branch condition while the carried phi values remain below
    /// it. If both successors are private, later blocks, we can leave those
    /// values on the stack for both edges instead of spilling them before every
    /// loop condition.
    fn branch_preserve_targets(
        &self,
        func: &Function,
        liveness: &Liveness,
        block_id: BlockId,
        pos: usize,
        block_pos: &FxHashMap<BlockId, usize>,
    ) -> Vec<BlockId> {
        let Some(Terminator::Branch { condition, then_block, else_block }) =
            func.blocks[block_id].terminator.as_ref()
        else {
            return Vec::new();
        };

        if self.scheduler.stack.depth() <= 1 || self.scheduler.stack.top() != Some(*condition) {
            return Vec::new();
        }

        let Some(carried) = self
            .scheduler
            .stack
            .iter()
            .skip(1)
            .map(|slot| {
                let value = slot?;
                liveness.live_out(block_id).contains(value).then_some(value)
            })
            .collect::<Option<Vec<_>>>()
        else {
            return Vec::new();
        };
        if carried.len() > STACK_PHI_LAYOUT_LIMIT {
            return Vec::new();
        }

        // Consuming the branch condition removes the top stack word. A resident argument has no
        // frame fallback, so every stack-only live-out must already have another carried copy
        // below the condition. Otherwise let the global stack-edge planner duplicate and arrange
        // the condition together with the successor layout.
        if liveness
            .live_out(block_id)
            .iter()
            .any(|value| self.scheduler.is_stack_only_value(value) && !carried.contains(&value))
        {
            return Vec::new();
        }

        let targets = [*then_block, *else_block];
        let mut live_in_any_target = DenseBitSet::new_empty(func.num_values());
        for target in targets {
            for value in liveness.live_in(target) {
                live_in_any_target.insert(value);
            }
        }
        if carried.iter().any(|&value| !live_in_any_target.contains(value)) {
            return Vec::new();
        }

        let mut preserved = Vec::with_capacity(2);
        for target in targets {
            if target == block_id {
                return Vec::new();
            }
            let has_phi = func.blocks[target]
                .instructions
                .iter()
                .any(|&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)));
            if func.blocks[target].predecessors.as_slice() == [block_id]
                && block_pos.get(&target).copied() > Some(pos)
                && !has_phi
            {
                preserved.push(target);
                continue;
            }
            if !has_phi && self.is_junk_tolerant_terminal(func, liveness, target) {
                continue;
            }
            return Vec::new();
        }

        preserved
    }

    fn is_junk_tolerant_terminal(
        &self,
        func: &Function,
        liveness: &Liveness,
        block: BlockId,
    ) -> bool {
        liveness.live_in(block).iter().all(|value| matches!(func.value(value), Value::Immediate(_)))
            && match func.blocks[block].terminator {
                Some(
                    Terminator::Revert { .. } | Terminator::RevertReturndata | Terminator::Invalid,
                ) => true,
                Some(Terminator::TailCall { function, .. }) => {
                    self.cold_functions.contains(function)
                }
                _ => false,
            }
    }

    /// Finds functions whose reachable exits all abort, including chains of
    /// calls to other cold functions.
    fn collect_cold_functions(module: &Module) -> DenseBitSet<FunctionId> {
        let mut cold = DenseBitSet::new_empty(module.functions.len());
        let mut worklist = Vec::new();
        let mut visited = GrowableBitSet::new_empty();
        loop {
            let mut changed = false;
            for (function_id, func) in module.functions.iter_enumerated() {
                if cold.contains(function_id) {
                    continue;
                }
                worklist.clear();
                worklist.push(BlockId::ENTRY);
                visited.clear();
                let mut saw_exit = false;
                let mut all_exits_cold = true;
                while let Some(block_id) = worklist.pop()
                    && all_exits_cold
                {
                    if !visited.insert(block_id) {
                        continue;
                    }
                    let block = &func.blocks[block_id];
                    if block.instructions.iter().any(|&inst_id| {
                        matches!(
                            func.inst(inst_id).kind,
                            InstKind::ICall { function, .. } if cold.contains(function)
                        )
                    }) {
                        saw_exit = true;
                        continue;
                    }
                    let Some(term) = block.terminator.as_ref() else {
                        all_exits_cold = false;
                        continue;
                    };
                    match term {
                        Terminator::Revert { .. }
                        | Terminator::RevertReturndata
                        | Terminator::Invalid => {
                            saw_exit = true;
                        }
                        Terminator::TailCall { function, .. } if cold.contains(*function) => {
                            saw_exit = true;
                        }
                        _ => {
                            let successors = term.successors();
                            if successors.is_empty() {
                                all_exits_cold = false;
                            } else {
                                worklist.extend(successors);
                            }
                        }
                    }
                }
                if saw_exit && all_exits_cold {
                    cold.insert(function_id);
                    changed = true;
                }
            }
            if !changed {
                return cold;
            }
        }
    }

    /// Finds blocks that abort directly or can only reach other cold blocks.
    fn collect_cold_blocks(&self, func: &Function) -> DenseBitSet<BlockId> {
        let mut cold = DenseBitSet::new_empty(func.blocks.len());
        let mut worklist = Vec::new();
        for block_id in func.blocks.indices() {
            if self.block_aborts(func, block_id) {
                cold.insert(block_id);
                worklist.push(block_id);
            }
        }
        if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            return cold;
        }

        while let Some(block_id) = worklist.pop() {
            for &predecessor in &func.blocks[block_id].predecessors {
                if cold.contains(predecessor) {
                    continue;
                }
                let Some(term) = func.blocks[predecessor].terminator.as_ref() else {
                    continue;
                };
                let successors = term.successors();
                if !successors.is_empty()
                    && successors.iter().all(|&successor| cold.contains(successor))
                {
                    cold.insert(predecessor);
                    worklist.push(predecessor);
                }
            }
        }
        cold
    }

    /// Returns true when a block aborts directly or calls a function whose
    /// reachable exits all abort.
    fn block_aborts(&self, func: &Function, block_id: BlockId) -> bool {
        let block = &func.blocks[block_id];
        matches!(
            block.terminator,
            Some(Terminator::Revert { .. } | Terminator::RevertReturndata | Terminator::Invalid)
        ) || matches!(
            block.terminator,
            Some(Terminator::TailCall { function, .. })
                if self.cold_functions.contains(function)
        ) || block.instructions.iter().any(|&inst_id| {
            matches!(
                func.inst(inst_id).kind,
                InstKind::ICall { function, .. }
                    if self.cold_functions.contains(function)
            )
        })
    }

    fn block_is_cold(&self, block_id: BlockId) -> bool {
        self.cold_blocks.contains(block_id)
    }

    fn new_function_label(&mut self, function: FunctionId) -> Label {
        let label = self.asm.new_label();
        if self.cold_functions.contains(function) {
            self.asm.mark_label_cold(label);
        }
        label
    }

    fn block_layout_order(&self, func: &Function) -> Vec<BlockId> {
        // Layout only initializes reachability; RPO, dominators, and
        // transitive reachability remain unevaluated.
        let cfg = CfgInfo::new(func);
        let reachable = cfg.reachable();
        let mut order = Vec::with_capacity(func.blocks.len());
        let mut placed = DenseBitSet::new_empty(func.blocks.len());

        self.append_layout_chain(func, BlockId::ENTRY, reachable, &mut placed, &mut order);
        for block_id in func.blocks.indices() {
            if reachable.contains(block_id) {
                self.append_layout_chain(func, block_id, reachable, &mut placed, &mut order);
            }
        }

        order
    }

    fn append_layout_chain(
        &self,
        func: &Function,
        mut block_id: BlockId,
        reachable: &DenseBitSet<BlockId>,
        placed: &mut DenseBitSet<BlockId>,
        order: &mut Vec<BlockId>,
    ) {
        loop {
            if !reachable.contains(block_id) || !placed.insert(block_id) {
                return;
            }
            order.push(block_id);

            let target = match func.blocks[block_id].terminator.as_ref() {
                Some(Terminator::Jump(target))
                    if func.blocks[*target].predecessors.as_slice() == [block_id] =>
                {
                    *target
                }
                Some(Terminator::Branch { then_block, else_block, .. })
                    if !matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) =>
                {
                    match (self.block_is_cold(*then_block), self.block_is_cold(*else_block)) {
                        (true, false) => *else_block,
                        (false, true) => *then_block,
                        _ => return,
                    }
                }
                _ => return,
            };
            if placed.contains(target) {
                return;
            }

            block_id = target;
        }
    }

    fn set_stack_to_values(&mut self, values: &[ValueId]) {
        self.scheduler.stack.clear();
        for &value in values.iter().rev() {
            self.scheduler.stack.push(value);
        }
    }

    fn try_emit_global_stack_edge(
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

    fn global_branch_union(then_layout: &[ValueId], else_layout: &[ValueId]) -> Vec<ValueId> {
        let mut union = then_layout.to_vec();
        for &value in else_layout {
            if !union.contains(&value) {
                union.push(value);
            }
        }
        union
    }

    fn try_emit_global_stack_branch(
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

    fn global_switch_union(layouts: &[(BlockId, Vec<ValueId>)]) -> Vec<ValueId> {
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
    fn emit_global_stack_branch(
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

    fn emit_global_stack_switch(
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

    fn try_emit_stack_phi_edge(&mut self, func: &Function, edge: &StackPhiEdge) -> bool {
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

    fn can_prepare_stack_phi_edge(&self, func: &Function, edge: &StackPhiEdge) -> bool {
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

    fn can_prepare_stack_phi_branch(
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

    fn emit_stack_phi_branch(
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

    fn missing_stack_phi_sources(stack: &StackModel, sources: &[ValueId]) -> Vec<ValueId> {
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

    fn value_counts(values: impl IntoIterator<Item = ValueId>) -> FxHashMap<ValueId, usize> {
        let mut counts = FxHashMap::default();
        for value in values {
            *counts.entry(value).or_default() += 1;
        }
        counts
    }

    fn can_preserve_stack_fallthrough(
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

    fn is_stack_phi_source(&self, block: BlockId, value: ValueId) -> bool {
        self.stack_phi_sources.get(&block).is_some_and(|sources| sources.contains(&value))
    }

    /// Preallocates stable spill slots for values that may cross block boundaries.
    ///
    /// Blocks are emitted in layout order, not necessarily dominance order, so a block can be
    /// emitted before the predecessor that stores one of its live-in values. Reserving the slot up
    /// front lets the later load use a stable memory location; stores still happen only when the
    /// value is actually available on the stack.
    fn preallocate_cross_block_spills(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        cross_block_live: &OnceCell<DenseBitSet<ValueId>>,
    ) {
        let cross_block_live =
            cross_block_live.get_or_init(|| Self::cross_block_live_values(func, liveness));
        let values = Self::cross_block_spill_values(func, cross_block_live);

        // Coloring minimizes the local frame, which reduces memory expansion in gas mode. It is
        // deliberately disabled in size mode because renumbering spill addresses disturbed
        // downstream block sharing and regressed aggregate CI bytecode despite smaller frames.
        if self.gcx.sess.opts.optimization.is_gas() {
            let colorable = cross_block_live;
            let recomputable =
                cross_block_values(func, |value| !self.scheduler.is_stack_only_value(value));
            let ranges = Self::spill_live_ranges(func, liveness, colorable, &recomputable);
            let interferences =
                Self::parallel_phi_interferences(func, liveness, colorable, &self.block_copies);

            let mut colors = Vec::<SpillColor>::new();
            for value in colorable {
                let value_ranges = &ranges[value];
                let color = colors
                    .iter()
                    .position(|color| color.accepts(value, value_ranges, &interferences))
                    .unwrap_or_else(|| {
                        colors.push(SpillColor::new(func.num_values()));
                        colors.len() - 1
                    });
                colors[color].insert(value, value_ranges);
                self.scheduler.spills.reserve_at(value, color as u32);
            }

            for value in &values {
                if !colorable.contains(value) {
                    self.scheduler.spills.reserve(value);
                }
            }
        } else {
            for value in &values {
                self.scheduler.spills.reserve(value);
            }
        }

        self.preallocate_spill_metadata(func, &values);

        // A free-memory-pointer load cannot be recomputed after the pointer moves. Reserve stable
        // slots for cross-block values, including direct uses that liveness does not carry. Size
        // mode keeps every FMP slot stable because block-local reuse can increase output size.
        let reserve_all = matches!(self.gcx.sess.opts.optimization, OptimizationMode::Size);
        let reloaded = Self::cross_block_reload_values(func);
        for val in Self::fmp_load_values(func) {
            if reserve_all || values.contains(val) || reloaded.contains(val) {
                self.scheduler.spills.reserve(val);
                self.scheduler.spills.mark_reloadable(val);
            }
        }
    }

    fn preallocate_spill_metadata(&mut self, func: &Function, values: &DenseBitSet<ValueId>) {
        let recomputable =
            cross_block_values(func, |value| !self.scheduler.is_stack_only_value(value));
        for val in values {
            if recomputable.contains(val) {
                self.scheduler.spills.mark_recomputable(val);
            }
        }
    }

    fn cross_block_live_values(func: &Function, liveness: &Liveness) -> DenseBitSet<ValueId> {
        let mut values = DenseBitSet::new_empty(func.num_values());
        for block in func.blocks.indices() {
            for value in liveness.live_in(block).iter().chain(liveness.live_out(block).iter()) {
                if matches!(func.value(value), crate::mir::Value::Inst(_)) {
                    values.insert(value);
                }
            }
        }
        values
    }

    /// Returns the per-block interval of each colorable value's slot, keyed by block.
    ///
    /// The interval spans the points where the slot has to hold the value: its live-in and
    /// live-out ends, the instructions that define and consume it, and the range of every value
    /// the scheduler may rebuild from it. A rebuild materializes its operands where it happens,
    /// which for an operand with a slot is a load from that slot, so an operand's slot has to
    /// survive as long as the rebuilt value's, not only until liveness drops the operand.
    fn spill_live_ranges(
        func: &Function,
        liveness: &Liveness,
        colorable: &DenseBitSet<ValueId>,
        recomputable: &DenseBitSet<ValueId>,
    ) -> IndexVec<ValueId, FxHashMap<BlockId, SpillLiveRange>> {
        let mut ranges = index_vec![FxHashMap::default(); func.num_values()];
        let mut operands = SmallVec::<[ValueId; 8]>::new();

        for (block_id, block) in func.blocks.iter_enumerated() {
            for value in liveness.live_in(block_id) {
                Self::extend_spill_live_range(&mut ranges, colorable, value, block_id, 0);
            }
            for (inst_idx, &inst_id) in block.instructions.iter().enumerate() {
                operands.clear();
                func.inst(inst_id).kind.collect_operands(&mut operands);
                for &value in &operands {
                    Self::extend_spill_live_range(
                        &mut ranges,
                        colorable,
                        value,
                        block_id,
                        inst_idx * 2,
                    );
                }
                if let Some(value) = func.inst_result_value(inst_id) {
                    Self::extend_spill_live_range(
                        &mut ranges,
                        colorable,
                        value,
                        block_id,
                        inst_idx * 2 + 1,
                    );
                }
            }
            if let Some(terminator) = &block.terminator {
                let point = block.instructions.len() * 2;
                for value in terminator.operands() {
                    Self::extend_spill_live_range(&mut ranges, colorable, value, block_id, point);
                }
            }
            let point = block.instructions.len() * 2 + 1;
            for value in liveness.live_out(block_id) {
                Self::extend_spill_live_range(&mut ranges, colorable, value, block_id, point);
            }
        }

        Self::extend_recomputed_operand_ranges(func, colorable, recomputable, &mut ranges);
        ranges
    }

    /// Widens every operand's range over the range of the values rebuilt from it.
    ///
    /// A rebuild is only chosen where the rebuilt value is needed, so the rebuilt value's own
    /// range covers every point an operand can be read at. Rebuilding is transitive and passes
    /// through values that never own a slot themselves, so the requirement propagates over the
    /// whole recomputable operand graph and only lands on the colorable values at the end.
    fn extend_recomputed_operand_ranges(
        func: &Function,
        colorable: &DenseBitSet<ValueId>,
        recomputable: &DenseBitSet<ValueId>,
        ranges: &mut IndexVec<ValueId, FxHashMap<BlockId, SpillLiveRange>>,
    ) {
        let mut required = ranges.clone();
        let mut operands = SmallVec::<[ValueId; 8]>::new();
        let mut worklist: Vec<ValueId> =
            recomputable.iter().filter(|&value| !required[value].is_empty()).collect();
        while let Some(value) = worklist.pop() {
            let crate::mir::Value::Inst(inst_id) = func.value(value) else { continue };
            operands.clear();
            func.inst(*inst_id).kind.collect_operands(&mut operands);
            let value_required = required[value].clone();
            for &operand in &operands {
                if !recomputable.contains(operand) && !colorable.contains(operand) {
                    continue;
                }
                let mut grew = false;
                for (&block, &range) in &value_required {
                    grew |= Self::merge_spill_live_range(&mut required[operand], block, range);
                }
                if grew && recomputable.contains(operand) {
                    worklist.push(operand);
                }
            }
        }

        for value in colorable.iter() {
            for (&block, &range) in &required[value] {
                Self::merge_spill_live_range(&mut ranges[value], block, range);
            }
        }
    }

    /// Unions `range` into a value's interval for `block`, reporting whether it grew.
    fn merge_spill_live_range(
        ranges: &mut FxHashMap<BlockId, SpillLiveRange>,
        block: BlockId,
        range: SpillLiveRange,
    ) -> bool {
        match ranges.entry(block) {
            StdEntry::Occupied(mut entry) => {
                let merged = SpillLiveRange {
                    start: entry.get().start.min(range.start),
                    end: entry.get().end.max(range.end),
                };
                let grew = merged != *entry.get();
                entry.insert(merged);
                grew
            }
            StdEntry::Vacant(entry) => {
                entry.insert(range);
                true
            }
        }
    }

    /// Records spill-slot conflicts introduced by simultaneous phi edge copies.
    ///
    /// The ordinary live ranges do not model the sequentialized copy schedule. Every destination
    /// must coexist at the successor, and a destination store must not alias a source that the
    /// schedule loads later. Sources already loaded before a store may safely share its slot.
    fn parallel_phi_interferences(
        func: &Function,
        liveness: &Liveness,
        colorable: &DenseBitSet<ValueId>,
        block_copies: &FxHashMap<BlockId, Vec<ParallelCopy>>,
    ) -> SpillInterferences {
        let mut interferences = FxHashMap::default();
        for (block_id, copies) in block_copies {
            // Copies of a multi-successor predecessor execute before the
            // branch, on every outgoing edge. Splitting keeps phi results
            // read on sibling paths out of this position, but a destination
            // could still reuse the spill slot of an unrelated value that is
            // live only on a sibling edge; keep those apart.
            let sibling_live = func.blocks[*block_id].terminator.as_ref().and_then(|term| {
                let successors = term.successors();
                (successors.len() > 1).then_some(successors)
            });
            for (index, copy) in copies.iter().enumerate() {
                let CopyDest::Value(destination) = &copy.dst else { continue };
                if let Some(successors) = &sibling_live {
                    for &successor in successors {
                        for value in liveness.live_in(successor).iter() {
                            Self::add_spill_interference(
                                &mut interferences,
                                colorable,
                                *destination,
                                value,
                            );
                        }
                    }
                }
                for other in copies {
                    let CopyDest::Value(other_destination) = &other.dst else { continue };
                    Self::add_spill_interference(
                        &mut interferences,
                        colorable,
                        *destination,
                        *other_destination,
                    );
                }
                for later in &copies[index + 1..] {
                    let CopySource::Value(source) = &later.src else { continue };
                    Self::add_spill_interference(
                        &mut interferences,
                        colorable,
                        *destination,
                        *source,
                    );
                }
                // Interference is modeled from the sequentialized copy schedule
                // plus the terminator's operands below. Extending destinations
                // through the whole predecessor live-out set is provably safe
                // but was measured to cost 12% runtime gas on the LibString hot
                // workload by defeating slot reuse; the residual (a destination
                // sharing with a non-source value live only on a sibling edge)
                // has never been reproduced and is accepted deliberately.
                let own_source = match &copy.src {
                    CopySource::Value(source) => Some(*source),
                    _ => None,
                };
                // A value consumed only by this predecessor's terminator is not
                // live-out, but the copy stores execute before the terminator: a
                // destination sharing the condition's slot would clobber a
                // pending reload and take the wrong branch.
                if let Some(term) =
                    func.blocks.get(*block_id).and_then(|block| block.terminator.as_ref())
                {
                    for operand in term.operands() {
                        if Some(operand) != own_source {
                            Self::add_spill_interference(
                                &mut interferences,
                                colorable,
                                *destination,
                                operand,
                            );
                        }
                    }
                }
            }
        }
        interferences
    }

    fn add_spill_interference(
        interferences: &mut SpillInterferences,
        colorable: &DenseBitSet<ValueId>,
        lhs: ValueId,
        rhs: ValueId,
    ) {
        if lhs == rhs || !colorable.contains(lhs) || !colorable.contains(rhs) {
            return;
        }
        for (value, conflict) in [(lhs, rhs), (rhs, lhs)] {
            let conflicts = interferences.entry(value).or_default();
            if !conflicts.contains(&conflict) {
                conflicts.push(conflict);
            }
        }
    }

    fn extend_spill_live_range(
        ranges: &mut IndexVec<ValueId, FxHashMap<BlockId, SpillLiveRange>>,
        colorable: &DenseBitSet<ValueId>,
        value: ValueId,
        block: BlockId,
        point: usize,
    ) {
        if !colorable.contains(value) {
            return;
        }
        ranges[value]
            .entry(block)
            .and_modify(|range| {
                range.start = range.start.min(point);
                range.end = range.end.max(point);
            })
            .or_insert(SpillLiveRange { start: point, end: point });
    }

    /// Returns values directly consumed outside their defining block. Phi inputs are edge uses:
    /// codegen consumes them in the predecessor or carries them on the edge, so they do not need a
    /// reload route under the source value's identity.
    fn cross_block_reload_values(func: &Function) -> DenseBitSet<ValueId> {
        let mut definitions =
            IndexVec::<ValueId, Option<BlockId>>::from_vec(vec![None; func.num_values()]);
        for block_id in func.blocks.indices() {
            for &inst_id in &func.blocks[block_id].instructions {
                if let Some(result) = func.inst_result_value(inst_id) {
                    definitions[result] = Some(block_id);
                }
            }
        }

        let mut reloaded = DenseBitSet::new_empty(func.num_values());
        for block_id in func.blocks.indices() {
            for &inst_id in &func.blocks[block_id].instructions {
                if matches!(func.inst(inst_id).kind, InstKind::Phi(_)) {
                    continue;
                }
                for operand in func.inst(inst_id).kind.operands() {
                    if definitions[operand].is_some_and(|definition| definition != block_id) {
                        reloaded.insert(operand);
                    }
                }
            }
            if let Some(terminator) = &func.blocks[block_id].terminator {
                for operand in terminator.operands() {
                    if definitions[operand].is_some_and(|definition| definition != block_id) {
                        reloaded.insert(operand);
                    }
                }
            }
        }
        reloaded
    }

    /// Every live free-memory-pointer load result in the function.
    fn fmp_load_values(func: &Function) -> Vec<ValueId> {
        let mut values = Vec::new();
        for inst_id in func.instructions() {
            if matches!(
                func.inst(inst_id).kind,
                InstKind::MLoad(addr)
                    if func.value_u64(addr) == Some(EvmMemoryLayout::FMP_SLOT)
            ) && let Some(val) = func.inst_result_value(inst_id)
            {
                values.push(val);
            }
        }
        values
    }

    fn cross_block_spill_values(
        func: &Function,
        cross_block_live: &DenseBitSet<ValueId>,
    ) -> DenseBitSet<ValueId> {
        let mut values = DenseBitSet::new_empty(func.num_values());
        for value in cross_block_live {
            if Self::can_own_spill_slot(func, value)
                || Self::is_always_rematerializable_value(func, value)
            {
                values.insert(value);
            }
        }
        for block_id in func.blocks.indices() {
            for &inst_id in &func.blocks[block_id].instructions {
                if matches!(func.inst(inst_id).kind, InstKind::Phi(_))
                    && let Some(val) = func.inst_result_value(inst_id)
                {
                    values.insert(val);
                }
            }
        }
        values
    }

    fn is_cross_block_recomputable_inst(func: &Function, value: ValueId) -> bool {
        let Value::Inst(inst_id) = func.value(value) else { return false };
        is_cross_block_recomputable_kind(&func.inst(*inst_id).kind)
    }

    /// Spills stack-resident values a successor reads under their own identity.
    /// Phi inputs are consumed by their predecessor edge copies.
    fn spill_live_out_values(&mut self, func: &Function, liveness: &Liveness, block_id: BlockId) {
        self.spill_live_out_values_except(func, liveness, block_id, &[]);
    }

    fn spill_live_out_values_except(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block_id: BlockId,
        exempt: &[ValueId],
    ) {
        let mut exempt_values = DenseBitSet::new_empty(func.num_values());
        for &value in exempt {
            exempt_values.insert(value);
        }
        let successors = func.blocks[block_id]
            .terminator
            .as_ref()
            .map(Terminator::successors)
            .unwrap_or_default();
        for val in liveness.live_out(block_id) {
            if !exempt_values.contains(val)
                && successors.iter().any(|&succ| liveness.live_in(succ).contains(val))
            {
                self.spill_value_if_needed(func, val);
            }
        }
    }

    fn pop_stack_values_not_needed_by(&mut self, needed: &[ValueId]) {
        while let Some(depth) = self.first_stack_value_not_needed_by(needed) {
            if depth > 0 {
                assert!(
                    depth <= self.stack_access_limit(),
                    "resident stack discard exceeded SWAP reach"
                );
                self.emit_stack_op(StackOp::Swap(depth as u8));
            }
            self.emit_stack_op(StackOp::Pop);
        }
    }

    fn first_stack_value_not_needed_by(&self, needed: &[ValueId]) -> Option<usize> {
        let mut remaining = Self::value_counts(needed.iter().copied());
        for (depth, slot) in self.scheduler.stack.iter().enumerate() {
            let Some(value) = slot else {
                return Some(depth);
            };
            let Some(count) = remaining.get_mut(&value) else {
                return Some(depth);
            };
            if *count == 0 {
                return Some(depth);
            }
            *count -= 1;
        }
        None
    }

    /// A phi defined in this block is a new loop iteration's value. A phi
    /// defined elsewhere retains its spill only when every incoming path has
    /// already established it.
    fn invalidate_carried_phi_spills(&mut self, func: &Function, block_id: BlockId) {
        let carried: Vec<ValueId> = self.scheduler.stack.iter().flatten().collect();
        for value in carried {
            if let crate::mir::Value::Inst(inst_id) = func.value(value)
                && matches!(func.inst(*inst_id).kind, InstKind::Phi(_))
                && (func.blocks[block_id].instructions.contains(inst_id)
                    || self
                        .spill_available
                        .as_ref()
                        .is_none_or(|available| !available.contains(&value)))
            {
                self.scheduler.spills.invalidate_stored(value);
            }
        }
    }

    fn mark_live_in_spills(&mut self, func: &Function, liveness: &Liveness, block_id: BlockId) {
        // Values already on the stack (carried in from a preserved predecessor
        // edge) are read directly; marking them reloadable would point at a
        // spill slot that may never have been stored.
        for val in liveness.live_in(block_id) {
            if !self.scheduler.stack.contains(val) && self.scheduler.spills.get(val).is_some() {
                self.scheduler.spills.mark_reloadable(val);
            }
        }
        for &inst_id in &func.blocks[block_id].instructions {
            if matches!(func.inst(inst_id).kind, InstKind::Phi(_))
                && let Some(val) = func.inst_result_value(inst_id)
                && !self.scheduler.stack.contains(val)
                && self.scheduler.spills.get(val).is_some()
            {
                self.scheduler.spills.mark_reloadable(val);
            }
        }
    }

    fn spill_values_before_stack_clear(&mut self, func: &Function, values: &[ValueId]) {
        for &value in values {
            self.spill_value_if_needed(func, value);
        }
    }

    /// Parks stack-resident operands in their spill slots before an
    /// `emit_value_fresh` sequence. The sequence re-materializes each value,
    /// and definitions such as free-memory-pointer loads cannot be recomputed
    /// once memory has moved on: reaching them through a reload keeps the
    /// original definition.
    fn prepare_fresh_operands(&mut self, func: &Function, operands: &[ValueId]) {
        // The spill here is only a burial fallback: `emit_value_fresh` DUPs an
        // on-stack operand and recomputes a cheap one, reaching this spill copy
        // only if the operand sinks past DUP16 during argument emission. In a
        // forwarding proxy the call reads the low memory the spill area lives in
        // (a `delegatecall` whose args are `[0, calldatasize())`), so writing
        // the backup there corrupts the call's own input. Such a function keeps
        // its few operands stack-resident instead — a simple forwarder never
        // buries them.
        if !self.spill_hazard_insts.is_empty() {
            return;
        }
        for &operand in operands {
            self.spill_value_if_needed(func, operand);
        }
    }

    /// Duplicates a stack-only operand before earlier fresh operands bury it past `DUP` reach.
    /// `operands` are ordered deepest-first, exactly as the following emission sequence pushes
    /// them.
    fn stage_stack_only_fresh_operands(&mut self, operands: &[ValueId]) {
        if !self.scheduler.has_stack_only_values() {
            return;
        }
        let stack_access_limit = self.stack_access_limit();

        loop {
            let mut stack = self.scheduler.stack.clone();
            let mut inaccessible = None;
            for &operand in operands {
                if self.scheduler.is_stack_only_value(operand) {
                    match stack.find(operand) {
                        Some(depth) if depth < stack_access_limit => {
                            stack.dup((depth + 1) as u8);
                        }
                        _ => {
                            inaccessible = Some(operand);
                            break;
                        }
                    }
                } else {
                    stack.push_unknown();
                }
            }
            let Some(operand) = inaccessible else { break };
            let Some(depth) = self.scheduler.stack.find(operand) else {
                if self.recover_lost_internal_stack_value(operand) {
                    return;
                }
                panic!("stack-only CALL operand {operand:?} was lost before its use");
            };
            assert!(depth < stack_access_limit, "stack-only CALL operand exceeded DUP reach");
            self.emit_stack_op(StackOp::Dup((depth + 1) as u8));
        }
    }

    fn stack_access_limit(&self) -> usize {
        self.gcx.sess.opts.evm_version.reachable_stack_depth()
    }

    /// Returns true when `val` is reachable from a successor block: it is on the stack, it has a
    /// valid store, its slot is available on every emitted path into this block, or
    /// [`Self::spill_value_if_needed`] gives it no slot in the first place because it is
    /// stack-only, rematerializable, or reloadable from its argument address.
    fn has_spill_home(&self, func: &Function, val: ValueId) -> bool {
        self.scheduler.stack.contains(val)
            || self.scheduler.spills.is_stored(val)
            || self.spill_store_available(val)
            || self.scheduler.is_stack_only_value(val)
            || !Self::can_own_spill_slot(func, val)
            || Self::is_reloadable_argument_address(func, val)
    }

    /// Returns true when `val`'s slot holds it on every emitted forward path
    /// into the current block.
    ///
    /// The scheduler's stored flag is one function-wide bit, so it is the
    /// weaker record of the two. A block that carries a value in on the stack
    /// while the slot is not available there clears the bit, which is right for
    /// that block but also forgets the store for the blocks whose predecessors
    /// all did write the slot. The store-availability intersection is the
    /// per-path record and still names the value there, so a cleared bit alone
    /// does not mean the value lost its memory home.
    fn spill_store_available(&self, val: ValueId) -> bool {
        self.scheduler.spills.get(val).is_some()
            && self.spill_available.as_ref().is_some_and(|available| available.contains(&val))
    }

    /// Returns whether every forward predecessor of `block`, which sits at `pos` in the emission
    /// order, was already emitted. Only then does a value live into `block` have to own a home
    /// already: a predecessor emitted later stores its live-out values when its own turn comes,
    /// which is later in the stream but earlier at runtime.
    fn forward_predecessors_emitted(
        func: &Function,
        store_cfg: &CfgInfo,
        block_pos: &FxHashMap<BlockId, usize>,
        block: BlockId,
        pos: usize,
    ) -> bool {
        func.blocks[block].predecessors.iter().all(|&pred| {
            store_cfg.dominators().dominates(block, pred)
                || block_pos.get(&pred).is_some_and(|&pred_pos| pred_pos < pos)
        })
    }

    /// Spills an instruction result if it is on the stack and not already stored.
    fn spill_value_if_needed(&mut self, func: &Function, val: ValueId) {
        if self.scheduler.is_stack_only_value(val) || !Self::can_own_spill_slot(func, val) {
            return;
        }
        if self.scheduler.should_recompute_unstored_spill(val)
            && Self::is_reloadable_argument_address(func, val)
        {
            return;
        }

        // `stored` is a global emission flag; a store emitted by a sibling
        // branch arm sets it without covering this path. Trust it only when
        // the store is available on every emitted path into this block. The
        // current availability set is updated whenever this block stores.
        if self.scheduler.spills.is_stored(val)
            && self.spill_available.as_ref().is_none_or(|avail| avail.contains(&val))
        {
            return;
        }

        if let Some(depth) = self.scheduler.stack.find(val) {
            let slot = self.scheduler.spills.allocate(val);
            if depth >= self.stack_access_limit() {
                self.spill_deep_stack_value(func, val, slot, depth);
                return;
            }

            self.spill_accessible_stack_value(func, val, slot, depth);
        }
    }

    fn is_reloadable_argument_address(func: &Function, value: ValueId) -> bool {
        let Value::Inst(inst_id) = func.value(value) else { return false };
        let InstKind::Add(left, right) = func.inst(*inst_id).kind else { return false };
        if !matches!(func.value(left), Value::Arg(_)) && !matches!(func.value(right), Value::Arg(_))
        {
            return false;
        }

        let mut store = false;
        let mut load = false;
        for inst_id in func.instructions() {
            match func.inst(inst_id).kind {
                InstKind::MStore(address, _) if address == value => store = true,
                InstKind::MLoad(address) if address == value => load = true,
                _ => {}
            }
        }
        store && load
    }

    fn spill_value_to_reserved_slot(&mut self, func: &Function, val: ValueId) -> bool {
        if self.scheduler.is_stack_only_value(val)
            || Self::is_rematerializable_value(func, val)
            || Self::is_reloadable_argument_address(func, val)
            || self.scheduler.spills.get(val).is_none()
        {
            return false;
        }

        let Some(depth) = self.scheduler.stack.find(val) else {
            return false;
        };
        let slot = self.scheduler.spills.allocate(val);
        if depth >= self.stack_access_limit() {
            self.spill_deep_stack_value(func, val, slot, depth);
        } else {
            self.spill_accessible_stack_value(func, val, slot, depth);
        }
        true
    }

    fn spill_reserved_result_if_live(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
        value: ValueId,
    ) {
        // This is not the normal first-store path; `generate_inst` handles live-out results.
        // It repairs physical emission orders where a successor block emitted first has already
        // marked this reserved cross-block slot as stored/reloadable before the defining block
        // materializes the value.
        if self.scheduler.spills.get(value).is_none()
            || !self.scheduler.spills.is_stored(value)
            || liveness.is_dead_after(value, block, inst_idx)
        {
            return;
        }

        self.spill_value_to_reserved_slot(func, value);
    }

    fn spill_accessible_stack_value(
        &mut self,
        func: &Function,
        val: ValueId,
        slot: SpillSlot,
        depth: usize,
    ) {
        debug_assert!(depth < self.stack_access_limit());

        // DUP the value to top of stack for storing.
        // We need to DUP (not just use ensure_on_top) because:
        // 1. If value is on top, ensure_on_top does nothing but we need a copy
        // 2. MSTORE will consume the value, and we want to preserve the original
        let (block, start) = self.asm.next_instruction_position();
        let dup_n = (depth + 1) as u8;
        self.emit_stack_op(StackOp::Dup(dup_n));

        self.store_stack_top_to_spill(func, val, slot);
        let (end_block, end) = self.asm.next_instruction_position();
        if end_block == block {
            self.spill_stores.push(SpillStore { value: val, slot, block, range: start..end });
        }
    }

    /// Drops stores of values that remain on the stack on every live branch
    /// arm. A later block stores the value again if it needs a memory home.
    fn remove_dead_carried_spill_stores(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block_id: BlockId,
        preserved: &[BlockId],
    ) {
        let Some(Terminator::Branch { condition, then_block, else_block }) =
            func.blocks[block_id].terminator.as_ref()
        else {
            return;
        };
        let successors = [*then_block, *else_block];
        let current_block = self.asm.next_instruction_position().0;
        let mut removals = Vec::new();
        self.spill_stores.retain(|store| {
            let defined_here = matches!(func.value(store.value), Value::Inst(inst)
                if func.blocks[block_id].instructions.contains(inst));
            let reloaded_here = self
                .spill_loads
                .iter()
                .any(|&(slot, block, _)| block == store.block && slot == store.slot);
            let remove = store.block == current_block
                && store.value != *condition
                && defined_here
                && !reloaded_here
                && self.scheduler.stack.contains(store.value)
                && successors.iter().all(|&successor| {
                    preserved.contains(&successor)
                        || !liveness.live_in(successor).contains(store.value)
                });
            if remove {
                removals.push(store.clone());
            }
            !remove
        });
        for store in &removals {
            if let Some((_, references)) =
                self.spill_addr_consts.get_mut(&u64::from(store.slot.offset))
            {
                *references = references.saturating_sub(1);
            }
            self.scheduler.spills.invalidate_stored(store.value);
            if let Some(available) = &mut self.spill_available {
                available.remove(&store.value);
            }
        }
        self.early_spill_removals
            .extend(removals.into_iter().map(|store| (store.block, store.range)));
    }

    fn remove_dead_spill_stores(&mut self) {
        enum Event {
            Store(usize),
            Load(SpillSlot),
        }

        if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            return;
        }

        // A spill store is dead when every path either overwrites its slot before a reload or
        // leaves the function. The scheduler keeps these stores while forming blocks, then drops
        // them after their final control flow is known.
        let stores = std::mem::take(&mut self.spill_stores);
        let loads = std::mem::take(&mut self.spill_loads);
        if stores.is_empty() {
            return;
        }

        let mut events = FxHashMap::<ir::BlockId, Vec<(usize, Event)>>::default();
        for (index, store) in stores.iter().enumerate() {
            events.entry(store.block).or_default().push((store.range.start, Event::Store(index)));
        }
        for (slot, block, index) in loads {
            events.entry(block).or_default().push((index, Event::Load(slot)));
        }
        for events in events.values_mut() {
            events.sort_unstable_by_key(|&(index, _)| index);
        }

        let range = self.function_ir_block_start..self.asm.block_count();
        let mut successors = FxHashMap::<ir::BlockId, Vec<ir::BlockId>>::default();
        for (source, target) in self.asm.dataflow_edges(range.clone()) {
            successors.entry(source).or_default().push(target);
        }
        let blocks = range.map(ir::BlockId::from_usize).collect::<Vec<_>>();
        let mut live_in = FxHashMap::<ir::BlockId, FxHashSet<SpillSlot>>::default();
        loop {
            let mut changed = false;
            for &block in blocks.iter().rev() {
                let mut live = successors
                    .get(&block)
                    .into_iter()
                    .flatten()
                    .filter_map(|successor| live_in.get(successor))
                    .flatten()
                    .copied()
                    .collect::<FxHashSet<_>>();
                for (_, event) in events.get(&block).into_iter().flatten().rev() {
                    match event {
                        Event::Store(index) => {
                            live.remove(&stores[*index].slot);
                        }
                        Event::Load(slot) => {
                            live.insert(*slot);
                        }
                    }
                }
                if live_in.get(&block) != Some(&live) {
                    live_in.insert(block, live);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let mut dead = FxHashSet::default();
        for &block in &blocks {
            let mut live = successors
                .get(&block)
                .into_iter()
                .flatten()
                .filter_map(|successor| live_in.get(successor))
                .flatten()
                .copied()
                .collect::<FxHashSet<_>>();
            for (_, event) in events.get(&block).into_iter().flatten().rev() {
                match event {
                    Event::Store(index) if !live.remove(&stores[*index].slot) => {
                        dead.insert(*index);
                    }
                    Event::Store(_) => {}
                    Event::Load(slot) => {
                        live.insert(*slot);
                    }
                }
            }
        }
        let mut removals = dead
            .into_iter()
            .map(|index| &stores[index])
            .map(|store| {
                if let Some((_, references)) =
                    self.spill_addr_consts.get_mut(&u64::from(store.slot.offset))
                {
                    *references = references.saturating_sub(1);
                }
                (store.block, store.range.clone())
            })
            .collect::<Vec<_>>();
        removals.extend(std::mem::take(&mut self.early_spill_removals));
        self.asm.remove_instructions(&mut removals);
    }

    fn spill_deep_stack_value(
        &mut self,
        func: &Function,
        val: ValueId,
        slot: SpillSlot,
        depth: usize,
    ) {
        let stack_access_limit = self.stack_access_limit();
        debug_assert!(depth >= stack_access_limit);

        let mut saved_above = Vec::with_capacity(depth + 1 - stack_access_limit);
        for _ in 0..(depth + 1 - stack_access_limit) {
            let Some(top) = self.scheduler.stack.top() else {
                panic!("cannot spill deep stack value {val:?}: untracked stack entry above it");
            };
            let restore = if let Some(op) = Self::always_rematerializable_op(func, top) {
                self.emit_stack_op(StackOp::Pop);
                ScheduledOp::RematerializeNullary(op)
            } else {
                let top_slot = self.scheduler.spills.allocate(top);
                if self.scheduler.reloadable_spill(top).is_some() {
                    self.emit_stack_op(StackOp::Pop);
                } else {
                    self.store_stack_top_to_spill(func, top, top_slot);
                }
                ScheduledOp::LoadSpill(top_slot)
            };
            saved_above.push((top, restore));
        }

        let Some(accessible_depth) = self.scheduler.stack.find(val) else {
            panic!("cannot spill deep stack value {val:?}: value disappeared while exposing it");
        };
        self.spill_accessible_stack_value(func, val, slot, accessible_depth);

        for (saved, restore) in saved_above.into_iter().rev() {
            let stack_depth = self.scheduler.depth();
            self.record_scheduled_ops_peak(stack_depth, std::slice::from_ref(&restore));
            self.emit_scheduled_ops(func, [restore]);
            self.scheduler.stack.push(saved);
        }
    }

    /// Establishes reload routes for dynamic-call arguments before the anonymous frame base adds
    /// one word above them. This is a fallback, not a stack-depth limit: accessible arguments keep
    /// their ordinary stack convention and arbitrarily deep layouts spill through memory.
    fn materialize_deep_dynamic_call_args(&mut self, func: &Function, args: &[ValueId]) {
        let stack_access_limit = self.stack_access_limit();
        for &arg in args {
            let Some(depth) = self.scheduler.stack.find(arg) else { continue };
            if depth + 1 < stack_access_limit
                || self.scheduler.reloadable_spill(arg).is_some()
                || Self::is_rematerializable_value(func, arg)
            {
                continue;
            }

            let slot = self.scheduler.spills.allocate(arg);
            if depth >= stack_access_limit {
                self.spill_deep_stack_value(func, arg, slot, depth);
            } else {
                self.spill_accessible_stack_value(func, arg, slot, depth);
            }
            self.scheduler.materialize_stack_only_value(arg);
        }
    }

    fn store_stack_top_to_spill(&mut self, func: &Function, value: ValueId, slot: SpillSlot) {
        // Store to spill slot: PUSH offset, MSTORE.
        // The PUSH creates an untracked stack entry, so we track it as unknown.
        self.emit_spill_slot_addr(func, slot);
        self.scheduler.stack.push_unknown();

        self.asm.emit_op(op::MSTORE);
        // MSTORE consumes 2 values: the untracked offset and the value being spilled.
        self.scheduler.stack.pop();
        self.scheduler.stack.pop();
        self.scheduler.spills.mark_stored(value);
        if let Some(available) = &mut self.spill_available {
            available.insert(value);
        }
    }

    /// Spills operands that are live-out before an instruction consumes them.
    /// This ensures cross-block values are preserved in memory.
    fn spill_live_out_operands(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block_id: BlockId,
        operands: &[ValueId],
    ) {
        let live_out = liveness.live_out(block_id);

        for &op in operands {
            if live_out.contains(op) && !self.is_stack_phi_source(block_id, op) {
                self.spill_value_if_needed(func, op);
            }
        }
    }

    /// Values that are always re-emitted at each use instead of being kept on
    /// the stack or spilled.
    ///
    /// `Arg` MUST stay in this set. With static frames an argument reload is a
    /// 3-4 byte `PUSH addr; MLOAD`/`CALLDATALOAD`, cheaper than the spill
    /// traffic that tracking would create — and the spill machinery assumes
    /// arguments never own slots: making `Arg` non-rematerializable was
    /// measured to REGRESS every bench contract's size (erc20 +61 B, maple
    /// +72 B, fractional +127 B) and to break 4 of 8 bench harnesses at
    /// runtime. The one exception is a stack-only argument that sinks below
    /// the target's DUP reach: [`Self::emit_value_impl`] gives that otherwise stranded
    /// word a spill slot. Do not make ordinary frame-backed arguments own
    /// slots without redesigning argument spilling.
    fn is_rematerializable_value(func: &Function, value: ValueId) -> bool {
        is_rematerializable_leaf(func.value(value))
    }

    fn is_always_rematerializable_value(func: &Function, value: ValueId) -> bool {
        Self::always_rematerializable_op(func, value).is_some()
    }

    fn always_rematerializable_op(func: &Function, value: ValueId) -> Option<u8> {
        rematerializable_nullary_value(func, value)
    }

    fn can_own_spill_slot(func: &Function, value: ValueId) -> bool {
        matches!(func.value(value), crate::mir::Value::Inst(_))
            && !Self::is_always_rematerializable_value(func, value)
    }

    /// Returns true when `value` needs no spill before the instruction that
    /// is about to consume it: it owns no reserved cross-block slot, it is
    /// not live out of the block, and more stack copies exist at this point
    /// than the instruction will consume net of the emissions still to come
    /// (`consumed`). Later in-block uses DUP the survivor, or deep-spill it
    /// on demand if it sinks past the target's `DUP` reach, so skipping the store
    /// cannot strand the value and adds no stack depth.
    fn block_local_copy_survives(
        &self,
        liveness: &Liveness,
        block: BlockId,
        value: ValueId,
        consumed: usize,
    ) -> bool {
        self.scheduler.spills.get(value).is_none()
            && !liveness.live_out(block).contains(value)
            && self.scheduler.stack.iter().flatten().filter(|&v| v == value).count() > consumed
    }

    fn spill_top_value_if_live(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
        value: ValueId,
    ) {
        if self.scheduler.is_stack_only_value(value) || Self::is_rematerializable_value(func, value)
        {
            return;
        }

        let has_reserved_cross_block_slot = self.scheduler.spills.get(value).is_some();
        if liveness.is_dead_after(value, block, inst_idx) && !has_reserved_cross_block_slot {
            return;
        }

        debug_assert_eq!(self.scheduler.stack.top(), Some(value));
        if !self.spill_value_to_reserved_slot(func, value) {
            self.spill_value_if_needed(func, value);
        }
        if has_reserved_cross_block_slot && !Self::is_reloadable_argument_address(func, value) {
            assert!(
                self.scheduler.reloadable_spill(value).is_some(),
                "reserved operand {value:?} was not stored before consumption in `{}`",
                func.name
            );
        }
    }

    /// Keeps stack-only operands alive when an instruction is emitted without an operand plan.
    /// Planned operations preserve these values as part of the plan itself, so doing this before
    /// every instruction duplicates both liveness queries and stack scans on the hot path.
    fn preserve_stack_only_operands(
        &mut self,
        operands: &[ValueId],
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        if !self.scheduler.has_stack_only_values() {
            return;
        }

        let mut uses = SmallVec::<[(ValueId, usize); 4]>::new();
        for &operand in operands {
            if self.scheduler.is_stack_only_value(operand) {
                if let Some((_, count)) = uses.iter_mut().find(|(value, _)| *value == operand) {
                    *count += 1;
                } else {
                    uses.push((operand, 1));
                }
            }
        }
        for (operand, consumed) in uses {
            if liveness.is_dead_after(operand, block, inst_idx) {
                continue;
            }
            while self.scheduler.stack.iter().filter(|slot| *slot == Some(operand)).count()
                <= consumed
            {
                let depth = self.scheduler.stack.find(operand).unwrap_or_else(|| {
                    if self.recover_lost_internal_stack_value(operand) {
                        return 0;
                    }
                    panic!("resident stack argument {operand:?} was lost before its final use")
                });
                assert!(depth < MAX_STACK_ACCESS, "resident stack argument exceeded DUP16 reach");
                self.emit_stack_op(StackOp::Dup((depth + 1) as u8));
            }
        }
    }

    /// Abandons a speculative internal stack ABI after one of its values was lost.
    ///
    /// The emitted placeholder belongs to an attempt that the outer codegen loop discards. The
    /// next attempt excludes this function from stack-only argument and return plans, so every
    /// value has a frame-backed reload route.
    fn recover_lost_internal_stack_value(&mut self, value: ValueId) -> bool {
        let Some(func_id) = self.current_internal_function else { return false };
        self.disabled_stack_only_functions.insert(func_id);
        self.asm.emit_push(U256::ZERO);
        self.scheduler.stack.push(value);
        true
    }

    fn stack_only_function_disabled(&self, func_id: FunctionId) -> bool {
        func_id.index() < self.disabled_stack_only_functions.domain_size()
            && self.disabled_stack_only_functions.contains(func_id)
    }

    /// Generates bytecode for an instruction.
    #[allow(clippy::too_many_arguments)]
    fn generate_inst(
        &mut self,
        func_id: FunctionId,
        inst_id: InstId,
        func: &Function,
        kind: &InstKind,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
        result_value: Option<ValueId>,
    ) {
        // A resident stack ABI may carry this value by MIR identity, so it needs one tracked
        // physical definition. Without one, retain the cheaper emit-at-use behavior.
        if self.resident_stack_args(func_id).is_none()
            && result_value.is_some_and(|value| Self::is_always_rematerializable_value(func, value))
        {
            return;
        }

        let operands = kind.operands();
        self.materialize_lazy_stack_args(func_id, kind, block, inst_idx);
        let transient_growth = Self::instruction_transient_growth(kind, operands.len());
        self.materialize_deep_stack_args(func_id, func, transient_growth);

        // Calldata-backed global layouts can rematerialize a missing argument;
        // keep the old lazy-copy behavior for those non-stack-only values.
        for &operand in &operands {
            if self.global_stack_active
                && matches!(func.value(operand), crate::mir::Value::Arg(_))
                && !self.scheduler.is_stack_only_value(operand)
                && !self.scheduler.stack.contains(operand)
                && !liveness.is_dead_after(operand, block, inst_idx)
            {
                self.emit_value(func, operand);
            }
        }

        // Spill any operands that are live-out before they get consumed.
        // This ensures cross-block values are preserved in memory.
        self.spill_live_out_operands(func, liveness, block, &operands);

        match kind {
            kind if let Some(opcode) = kind.evm_opcode() => {
                self.emit_evm_opcode(
                    func,
                    &operands,
                    opcode,
                    result_value,
                    liveness,
                    block,
                    inst_idx,
                );
            }
            InstKind::Alloc { size, .. } => {
                debug_assert!(func.inst(inst_id).metadata.deferred_alloc());
                let size =
                    func.value_u64(*size).expect("deferred allocation must have a constant size");
                let alloc = self.asm.emit_deferred_alloc();
                self.pending_static_allocs.entry(func_id).or_default().push((alloc, size));
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Fmp | InstKind::SetFmp(_) => {
                unreachable!("abstract allocation instruction reached EVM emission")
            }

            InstKind::StoreImmutable(..) => {
                unreachable!("immutable stores must be lowered before EVM codegen")
            }
            InstKind::LoadImmutable(id) => {
                self.emit_load_immutable(*id);
                self.scheduler.instruction_executed(0, result_value);
            }

            // Select is like a ternary conditional
            InstKind::Select(cond, true_val, false_val) => {
                // select(cond, t, f) = f + cond * (t - f)
                //
                // We emit all three values to the stack, then do inline computation.
                // Stack notation: rightmost = top (depth 0).
                // Stack after emit_value calls: [f, t, cond] with cond on top.

                if let Some(plan) = self.plan_operands(
                    func,
                    &[*false_val, *true_val, *cond],
                    liveness,
                    block,
                    inst_idx,
                ) {
                    self.emit_operand_plan(func, plan);
                } else {
                    self.preserve_stack_only_operands(
                        &[*false_val, *true_val, *cond],
                        liveness,
                        block,
                        inst_idx,
                    );
                    self.emit_value(func, *false_val); // Stack: [f]
                    self.emit_operand(func, *true_val); // Stack: [f, t]
                    self.emit_operand(func, *cond); // Stack: [f, t, cond]
                }

                // Now compute: f + cond * (t - f)
                // Stack is [f, t, cond] with cond on top (depth 0), t at depth 1, f at depth 2
                //
                // Step 1: get f -> [f, t, cond, f]
                self.emit_operand(func, *false_val);
                // Step 2: get t -> [f, t, cond, f, t]
                self.emit_operand(func, *true_val);
                // Step 3: SUB (top - second = t - f) -> [f, t, cond, t-f]
                self.emit_op_with_effect(
                    op::SUB,
                    StackEffect { pops: 2, pushes: 1 },
                    StackPush::Unknown,
                );
                // Step 4: MUL (cond * (t-f)) -> [f, t, cond*(t-f)]
                self.emit_op_with_effect(
                    op::MUL,
                    StackEffect { pops: 2, pushes: 1 },
                    StackPush::Unknown,
                );
                // Step 5: SWAP1 -> [f, cond*(t-f), t]
                self.emit_stack_op(StackOp::Swap(1));
                // Step 6: POP (remove t) -> [f, cond*(t-f)]
                self.emit_stack_op(StackOp::Pop);
                // Step 7: ADD (cond*(t-f) + f = f + cond*(t-f)) -> [result]
                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(op::ADD, StackEffect { pops: 2, pushes: 1 }, push);
            }

            // Phi nodes are skipped (handled by copies)
            InstKind::Phi(_) => {}

            // External calls
            //
            // These use emit_value_fresh to guarantee correct values regardless of scheduler
            // state. The stack-aware emit_op_with_effect ensures proper
            // tracking after emission.
            InstKind::Call { gas, addr, value, args_offset, args_size, ret_offset, ret_size } => {
                // CALL(gas, addr, value, argsOffset, argsSize, retOffset, retSize)
                // EVM pops in order: gas (TOS), addr, value, argsOffset, argsSize, retOffset,
                // retSize So we push in reverse order: retSize first (deepest), gas
                // last (TOS)
                let operands =
                    [*gas, *addr, *value, *args_offset, *args_size, *ret_offset, *ret_size];
                self.preserve_stack_only_operands(&operands, liveness, block, inst_idx);
                self.prepare_fresh_operands(func, &operands);
                self.stage_stack_only_fresh_operands(&[
                    *ret_size,
                    *ret_offset,
                    *args_size,
                    *args_offset,
                    *value,
                    *addr,
                    *gas,
                ]);
                self.emit_value_fresh(func, *ret_size);
                self.emit_value_fresh(func, *ret_offset);
                self.emit_value_fresh(func, *args_size);
                self.emit_value_fresh(func, *args_offset);
                self.emit_value_fresh(func, *value);
                self.emit_value_fresh(func, *addr);
                self.emit_gas_operand(func, *gas);

                // CALL consumes 7 values and produces 1 (success bool)
                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(op::CALL, StackEffect { pops: 7, pushes: 1 }, push);
            }

            InstKind::CallCode {
                gas,
                addr,
                value,
                args_offset,
                args_size,
                ret_offset,
                ret_size,
            } => {
                let operands =
                    [*gas, *addr, *value, *args_offset, *args_size, *ret_offset, *ret_size];
                self.preserve_stack_only_operands(&operands, liveness, block, inst_idx);
                self.prepare_fresh_operands(func, &operands);
                self.stage_stack_only_fresh_operands(&[
                    *ret_size,
                    *ret_offset,
                    *args_size,
                    *args_offset,
                    *value,
                    *addr,
                    *gas,
                ]);
                self.emit_value_fresh(func, *ret_size);
                self.emit_value_fresh(func, *ret_offset);
                self.emit_value_fresh(func, *args_size);
                self.emit_value_fresh(func, *args_offset);
                self.emit_value_fresh(func, *value);
                self.emit_value_fresh(func, *addr);
                self.emit_gas_operand(func, *gas);

                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(op::CALLCODE, StackEffect { pops: 7, pushes: 1 }, push);
            }

            InstKind::StaticCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                // STATICCALL(gas, addr, argsOffset, argsSize, retOffset, retSize)
                let operands = [*gas, *addr, *args_offset, *args_size, *ret_offset, *ret_size];
                self.preserve_stack_only_operands(&operands, liveness, block, inst_idx);
                self.prepare_fresh_operands(func, &operands);
                self.stage_stack_only_fresh_operands(&[
                    *ret_size,
                    *ret_offset,
                    *args_size,
                    *args_offset,
                    *addr,
                    *gas,
                ]);
                self.emit_value_fresh(func, *ret_size);
                self.emit_value_fresh(func, *ret_offset);
                self.emit_value_fresh(func, *args_size);
                self.emit_value_fresh(func, *args_offset);
                self.emit_value_fresh(func, *addr);
                self.emit_gas_operand(func, *gas);
                // STATICCALL consumes 6 values and produces 1 (success bool)
                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(op::STATICCALL, StackEffect { pops: 6, pushes: 1 }, push);
            }

            InstKind::DelegateCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                let operands = [*gas, *addr, *args_offset, *args_size, *ret_offset, *ret_size];
                self.preserve_stack_only_operands(&operands, liveness, block, inst_idx);
                self.prepare_fresh_operands(func, &operands);
                self.stage_stack_only_fresh_operands(&[
                    *ret_size,
                    *ret_offset,
                    *args_size,
                    *args_offset,
                    *addr,
                    *gas,
                ]);
                // DELEGATECALL(gas, addr, argsOffset, argsSize, retOffset, retSize)
                self.emit_value_fresh(func, *ret_size);
                self.emit_value_fresh(func, *ret_offset);
                self.emit_value_fresh(func, *args_size);
                self.emit_value_fresh(func, *args_offset);
                self.emit_value_fresh(func, *addr);
                self.emit_gas_operand(func, *gas);
                // DELEGATECALL consumes 6 values and produces 1 (success bool)
                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(
                    op::DELEGATECALL,
                    StackEffect { pops: 6, pushes: 1 },
                    push,
                );
            }

            InstKind::ICall { function, args, returns } => {
                self.preserve_stack_only_operands(args, liveness, block, inst_idx);
                self.emit_icall(
                    func_id,
                    func,
                    *function,
                    args,
                    *returns as usize,
                    result_value,
                    liveness,
                    block,
                    inst_idx,
                );
            }

            InstKind::InternalFrameAddr(offset) => {
                self.emit_own_frame_addr(*offset);
                if let Some(result) = result_value {
                    self.scheduler.stack.push(result);
                }
            }
            InstKind::ConstructorArgsBase => {
                self.emit_constructor_args_base();
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::ConstructorArgsEnd => {
                self.emit_constructor_args_end();
                self.scheduler.instruction_executed(0, result_value);
            }

            // Log operations
            InstKind::Log0(offset, size) => {
                // LOG0(offset, size) - stack order: offset on top, then size
                self.emit_log(func, op::LOG0, &[*size, *offset], liveness, block, inst_idx);
            }
            InstKind::Log1(offset, size, topic1) => {
                // LOG1(offset, size, topic1) - stack order: offset, size, topic1
                self.emit_log(
                    func,
                    op::LOG1,
                    &[*topic1, *size, *offset],
                    liveness,
                    block,
                    inst_idx,
                );
            }
            InstKind::Log2(offset, size, topic1, topic2) => {
                // LOG2(offset, size, topic1, topic2) - stack order: offset, size, topic1,
                // topic2
                self.emit_log(
                    func,
                    op::LOG2,
                    &[*topic2, *topic1, *size, *offset],
                    liveness,
                    block,
                    inst_idx,
                );
            }
            InstKind::Log3(offset, size, topic1, topic2, topic3) => {
                // LOG3(offset, size, topic1, topic2, topic3)
                self.emit_log(
                    func,
                    op::LOG3,
                    &[*topic3, *topic2, *topic1, *size, *offset],
                    liveness,
                    block,
                    inst_idx,
                );
            }
            InstKind::Log4(offset, size, topic1, topic2, topic3, topic4) => {
                // LOG4(offset, size, topic1, topic2, topic3, topic4)
                self.emit_log(
                    func,
                    op::LOG4,
                    &[*topic4, *topic3, *topic2, *topic1, *size, *offset],
                    liveness,
                    block,
                    inst_idx,
                );
            }

            // Memory copy operations
            InstKind::CalldataCopy(dest, offset, size) => {
                // CALLDATACOPY(destOffset, offset, size)
                self.emit_copy_op_live_aware(
                    func,
                    &[*size, *offset, *dest],
                    op::CALLDATACOPY,
                    liveness,
                    block,
                    inst_idx,
                );
            }

            InstKind::DataCopy(data, dest, size) => {
                self.emit_data_copy(func, *data, *dest, *size, liveness, block, inst_idx);
            }

            InstKind::CodeCopy(dest, offset, size) => {
                // CODECOPY(destOffset, offset, size)
                self.emit_copy_op_live_aware(
                    func,
                    &[*size, *offset, *dest],
                    op::CODECOPY,
                    liveness,
                    block,
                    inst_idx,
                );
            }

            InstKind::ReturnDataCopy(dest, offset, size) => {
                // RETURNDATACOPY(destOffset, offset, size)
                self.emit_copy_op_live_aware(
                    func,
                    &[*size, *offset, *dest],
                    op::RETURNDATACOPY,
                    liveness,
                    block,
                    inst_idx,
                );
            }

            InstKind::MCopy(dest, src, size) => {
                // MCOPY(destOffset, srcOffset, size)
                self.emit_copy_op_live_aware(
                    func,
                    &[*size, *src, *dest],
                    op::MCOPY,
                    liveness,
                    block,
                    inst_idx,
                );
            }

            InstKind::ExtCodeCopy(addr, dest, offset, size) => {
                // EXTCODECOPY(address, destOffset, offset, size)
                self.emit_copy_op_live_aware(
                    func,
                    &[*size, *offset, *dest, *addr],
                    op::EXTCODECOPY,
                    liveness,
                    block,
                    inst_idx,
                );
            }

            InstKind::MappingSlot(_, _)
            | InstKind::MappingSlotMemory(_, _)
            | InstKind::MappingSlotCalldata(_, _) => {
                unreachable!("mapping-slot builtins must be lowered before EVM codegen")
            }

            InstKind::MakeSlice { .. } | InstKind::SlicePtr(_) | InstKind::SliceLen(_) => {
                unreachable!(
                    "slice instructions must be lowered before EVM codegen: {kind:?} in `{}`",
                    func.name
                )
            }

            InstKind::MemoryObjectLen(_, _)
            | InstKind::SetMemoryObjectLen(_, _, _)
            | InstKind::MemoryObjectData(_, _)
            | InstKind::MemoryObjectFieldAddr { .. }
            | InstKind::MemoryObjectElementAddr { .. }
            | InstKind::Keccak256Bytes(_) => {
                unreachable!("memory-object instructions must be lowered before EVM codegen")
            }

            InstKind::MemoryZero(_, _) => {
                unreachable!("memory-zero instructions must be lowered before EVM codegen")
            }

            InstKind::AbiEncode { .. } => {
                unreachable!("ABI encoding must be lowered before EVM codegen")
            }

            InstKind::StorageToMemory { .. }
            | InstKind::MemoryToStorage { .. }
            | InstKind::ClearStorage { .. } => {
                unreachable!("aggregate operations must be lowered before EVM codegen")
            }
            _ => unreachable!("MIR instruction was not handled: {kind:?}"),
        }

        if let Some(result) = result_value
            && liveness.live_out(block).contains(result)
            && !self.is_stack_phi_source(block, result)
        {
            self.spill_value_if_needed(func, result);
        }

        // A constant-offset calldata load is the same physical word as the
        // corresponding external argument. Once its instruction result dies,
        // adopt a surviving stack copy as the argument instead of loading that
        // word again on the first planned edge.
        for operand in operands {
            if liveness.is_dead_after(operand, block, inst_idx)
                && let Some(&arg) = self.global_stack_aliases.get(&operand)
                && !liveness.is_dead_after(arg, block, inst_idx)
                && !self.scheduler.stack.contains(arg)
            {
                self.scheduler.stack.rename(operand, arg);
            }
        }

        // Drop dead values after the instruction
        let dead_ops = self.scheduler.drop_dead_values(liveness, block, inst_idx);
        for op in dead_ops {
            self.asm.emit_stack_op(op);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_evm_opcode(
        &mut self,
        func: &Function,
        operands: &[ValueId],
        opcode: u8,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        let (inputs, outputs) = op::stack_io(opcode).expect("MIR opcode has no stack effect");
        assert_eq!(usize::from(inputs), operands.len(), "MIR opcode operand count mismatch");

        match (inputs, outputs) {
            (0, 1) => {
                self.asm.emit_op(opcode);
                self.scheduler.instruction_executed(0, result);
            }
            (1, 1) => self.emit_unary_op_with_result(
                func,
                operands[0],
                opcode,
                result,
                liveness,
                block,
                inst_idx,
            ),
            (2, 1) => self.emit_binary_op_with_result(
                func,
                operands[0],
                operands[1],
                opcode,
                result,
                liveness,
                block,
                inst_idx,
            ),
            (2, 0) => self.emit_store_op_live_aware(
                func,
                operands[0],
                operands[1],
                opcode,
                liveness,
                block,
                inst_idx,
            ),
            (_, 1) => {
                let mut stack_order = SmallVec::<[ValueId; 8]>::from_slice(operands);
                stack_order.reverse();
                self.emit_nary_op(func, &stack_order, opcode, result, liveness, block, inst_idx);
            }
            _ => unreachable!("unsupported MIR opcode stack effect {inputs}->{outputs}"),
        }
    }

    /// Bounds how many words an instruction can place above a stack-only operand before reaching
    /// it. Ordinary operations consume one operand while arranging the rest, but an internal call
    /// also pushes its return label before emitting stack-passed arguments.
    fn instruction_transient_growth(kind: &InstKind, operands: usize) -> usize {
        if matches!(kind, InstKind::ICall { .. }) {
            operands.max(1)
        } else {
            operands.saturating_sub(1).max(1)
        }
    }

    /// Bounds how many words a terminator can place above a stack-only operand before reaching it.
    /// Even a one-operand terminator needs the baseline check: an operand already below `DUP16`
    /// cannot be emitted without first materializing its frame fallback.
    fn terminator_transient_growth(term: &Terminator) -> usize {
        let operands = term.operands().len();
        if operands == 0 { 0 } else { operands.saturating_sub(1).max(1) }
    }

    fn emit_new_internal_frame_base_tracked(&mut self) {
        self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
        self.asm.emit_op(op::MLOAD);
        self.scheduler.stack.push_unknown();
    }

    fn emit_internal_frame_store_from_top_preserving_base(&mut self, offset: u64) {
        self.emit_stack_op(StackOp::Dup(2));
        if offset != 0 {
            self.asm.emit_push(U256::from(offset));
            self.scheduler.stack.push_unknown();
            self.emit_op_with_effect(
                op::ADD,
                StackEffect { pops: 2, pushes: 1 },
                StackPush::Unknown,
            );
        }
        self.asm.emit_op(op::MSTORE);
        self.scheduler.instruction_executed(2, None);
    }

    fn emit_store_frame_base_to_current_frame_slot(&mut self) {
        self.emit_stack_op(StackOp::Dup(1));
        self.asm.emit_push(U256::from(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT));
        self.scheduler.stack.push_unknown();
        self.asm.emit_op(op::MSTORE);
        self.scheduler.instruction_executed(2, None);
    }

    fn emit_store_new_free_pointer_from_frame_base(&mut self, frame_size: DeferredConst) {
        self.asm.emit_push_deferred(frame_size);
        self.scheduler.stack.push_unknown();
        self.emit_op_with_effect(op::ADD, StackEffect { pops: 2, pushes: 1 }, StackPush::Unknown);
        self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
        self.scheduler.stack.push_unknown();
        self.asm.emit_op(op::MSTORE);
        self.scheduler.instruction_executed(2, None);
    }

    /// Address of `offset` within whatever frame the frame-pointer slot
    /// currently holds. Dynamic call sites use this to reach the callee frame
    /// right after a call (before the pointer is restored); dynamic functions
    /// use it for their own frame. For accesses that are statically about the
    /// CURRENT function's own frame, use [`Self::emit_own_frame_addr`], which
    /// resolves to an absolute address when the function has a static frame.
    fn emit_current_internal_frame_addr(&mut self, offset: u64) {
        let growth = if offset == 0 { 1 } else { 2 };
        self.scheduler.stack.observe_peak(self.scheduler.depth().saturating_add(growth));
        self.emit_current_internal_frame_addr_untracked(offset);
    }

    fn emit_current_internal_frame_addr_untracked(&mut self, offset: u64) {
        self.asm.emit_push(U256::from(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT));
        self.asm.emit_op(op::MLOAD);
        if offset != 0 {
            self.asm.emit_push(U256::from(offset));
            self.asm.emit_op(op::ADD);
        }
    }

    fn emit_constructor_args_base(&mut self) {
        let id = self
            .constructor_args_base_const
            .expect("constructor argument base used outside constructor codegen");
        self.asm.emit_push_deferred(id);
    }

    fn emit_constructor_args_end(&mut self) {
        let offset = self
            .constructor_args_offset_const
            .expect("constructor argument end used outside constructor codegen");
        // base = constructor_args_base
        // end = base + (codesize - constructor_args_offset)
        self.emit_constructor_args_base();
        self.asm.emit_push_deferred(offset);
        self.asm.emit_op(op::CODESIZE);
        self.asm.emit_op(op::SUB);
        self.asm.emit_op(op::ADD);
    }

    fn emit_constructor_arg_load(&mut self, index: ArgIdx) {
        self.emit_constructor_args_base();
        let offset = index.index() as u64 * EvmMemoryLayout::WORD_SIZE;
        if offset != 0 {
            self.asm.emit_push(U256::from(offset));
            self.asm.emit_op(op::ADD);
        }
        self.asm.emit_op(op::MLOAD);
    }

    /// Address of `offset` within the current function's own frame: a single
    /// absolute push for static-frame functions, the frame-pointer indirection
    /// otherwise.
    fn emit_own_frame_addr(&mut self, offset: u64) {
        if self.own_frame_addr_is_dynamic() {
            let growth = if offset == 0 { 1 } else { 2 };
            self.scheduler.stack.observe_peak(self.scheduler.depth().saturating_add(growth));
        }
        self.emit_own_frame_addr_untracked(offset);
    }

    fn emit_own_frame_addr_untracked(&mut self, offset: u64) {
        if let Some(func_id) = self.current_internal_function
            && self.static_frame_functions.contains(func_id)
        {
            let addr = self.static_frame_addr(func_id, offset);
            self.asm.emit_push_deferred(addr);
            return;
        }
        if !self.in_internal_function && !self.in_constructor {
            self.asm.emit_push(U256::from(EvmMemoryLayout::HEAP_START + offset));
            return;
        }
        self.emit_current_internal_frame_addr_untracked(offset);
    }

    fn own_frame_addr_is_dynamic(&self) -> bool {
        self.current_internal_function
            .is_none_or(|func_id| !self.static_frame_functions.contains(func_id))
            && (self.in_internal_function || self.in_constructor)
    }

    /// Removes the unused dynamic-frame header and single stack-return word from a static frame.
    fn compact_static_frame_offset(&self, func_id: FunctionId, offset: u64) -> u64 {
        // Single-word stack returns remove their backing slot even on the
        // frame-backed fallback. Multiword returns retain their ordinary area
        // so a failed bounded projection has compiler-owned staging memory.
        let mut compact = if self.runtime_stack_args {
            offset
                .checked_sub(EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE)
                .expect("static frame header is still referenced")
        } else {
            offset
        };
        if let Some(plan) = self.stack_return_plan(func_id)
            && plan.arity == 1
        {
            let return_size = plan.arity as u64 * EvmMemoryLayout::WORD_SIZE;
            let return_base = plan.local_base - return_size;
            debug_assert!(
                !(return_base..plan.local_base).contains(&offset),
                "removed stack-return slot is still referenced: func={func_id:?}"
            );
            if offset >= plan.local_base {
                compact -= return_size;
            }
        }
        compact
    }

    /// Selects static-frame helpers that can return their complete tuple on the EVM stack.
    ///
    /// Tail-call edges keep the memory convention because an external dispatch path does not
    /// necessarily carry an internal return address. Calls whose MIR return arity disagrees with
    /// the callee are also excluded defensively. Every optimized mode uses the convention: the
    /// bounded tuple shuffle replaces callee stores and caller loads, and any function that cannot
    /// realize its plan is regenerated with its ordinary frame-backed return area.
    fn compute_stack_return_plans(&mut self, module: &Module) {
        for abi in self.static_call_abis.values_mut() {
            abi.returns = None;
        }
        if !self.stack_returns_enabled {
            return;
        }

        for (func_id, func) in module.functions.iter_enumerated() {
            let arity = func.returns.len();
            let mut has_return = false;
            let has_consistent_returns = func.blocks.iter().all(|block| match &block.terminator {
                Some(Terminator::Return { values }) => {
                    has_return = true;
                    values.len() == arity
                }
                // The backend treats `stop` in an internal function as a void return, which is
                // incompatible with a non-empty stack-return convention.
                Some(Terminator::Stop) => false,
                _ => true,
            });
            if self.static_frame_functions.contains(func_id)
                && !matches!(self.gcx.sess.opts.optimization, OptimizationMode::None)
                && !self.disabled_stack_only_functions.contains(func_id)
                && !self.recursive_frame_functions.contains(func_id)
                && (1..=MAX_STACK_ACCESS).contains(&arity)
                && has_return
                && has_consistent_returns
            {
                let local_base = EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                    + ((func.params.len() + arity) as u64) * EvmMemoryLayout::WORD_SIZE;
                self.static_call_abi_mut(func_id, func.params.len()).returns =
                    Some(StackReturnPlan { arity, local_base });
            }
        }

        for (caller, func) in module.functions.iter_enumerated() {
            for inst_id in func.instructions() {
                if let InstKind::ICall { function, returns, .. } = &func.inst(inst_id).kind
                    && self
                        .stack_return_plan(*function)
                        .is_some_and(|plan| *returns as usize != plan.arity)
                    && let Some(abi) = self.static_call_abis.get_mut(function)
                {
                    abi.returns = None;
                }
            }
            for block in &func.blocks {
                if let Some(Terminator::TailCall { function, .. }) = &block.terminator {
                    if !self.cold_functions.contains(*function)
                        && let Some(abi) = self.static_call_abis.get_mut(&caller)
                    {
                        abi.returns = None;
                    }
                    if let Some(abi) = self.static_call_abis.get_mut(function) {
                        abi.returns = None;
                    }
                }
            }
        }
    }

    /// Computes which arguments of each static-frame callee pass on the
    /// stack. A site can deliver a stack argument through raw re-emission after
    /// the drain for immediates, position-independently reloadable caller
    /// arguments, and always-rematerializable reads, or through a
    /// freshness-validated spill reload for other computed values. The
    /// per-argument choice is scored across all sites — raw and
    /// already-stored (cross-block) values save the four-byte frame store,
    /// while a fresh block-local value must first pay its own spill — and an
    /// argument passes on the stack when the sites' savings outweigh the
    /// callee's one-time prologue store. Tail calls use the same entry tuple without pushing a new
    /// return label; an internal caller reuses its inherited label, while fused external bodies do
    /// not return through one.
    fn compute_stack_arg_masks(&mut self, module: &Module) {
        self.static_call_abis.clear();
        if self.static_frame_functions.is_empty() {
            return;
        }

        let mut scores = FxHashMap::<FunctionId, IndexVec<ArgIdx, Option<i64>>>::default();
        let mut excluded = DenseBitSet::new_empty(module.functions.len());
        for (caller_id, func) in module.functions.iter_enumerated() {
            let mut has_candidate_call = false;
            for block in func.blocks.iter() {
                has_candidate_call |= block.instructions.iter().any(|&inst_id| {
                    matches!(
                        &func.inst(inst_id).kind,
                        InstKind::ICall { function, .. }
                            if self.static_frame_functions.contains(*function)
                    )
                });
                has_candidate_call |= matches!(
                    &block.terminator,
                    Some(Terminator::TailCall { function, .. })
                        if self.static_frame_functions.contains(*function)
                );
            }
            if !has_candidate_call {
                continue;
            }

            let caller_is_entry = Self::is_external_entry(func);
            let caller_static = self.static_frame_functions.contains(caller_id);
            let raw_leaves_ok = caller_is_entry || caller_static;
            // Where each instruction result is defined, to spot cross-block
            // arguments (already stored at their definition).
            let mut inst_block = index_vec![None; func.num_insts()];
            let mut use_counts: FxHashMap<ValueId, usize> = FxHashMap::default();
            for (block_idx, block) in func.blocks.iter().enumerate() {
                for &inst_id in &block.instructions {
                    inst_block[inst_id] = Some(block_idx);
                    for operand in func.inst(inst_id).kind.operands() {
                        *use_counts.entry(operand).or_default() += 1;
                    }
                }
                if let Some(term) = &block.terminator {
                    for operand in term.operands() {
                        *use_counts.entry(operand).or_default() += 1;
                    }
                }
            }
            for (block_idx, block) in func.blocks.iter().enumerate() {
                for &inst_id in &block.instructions {
                    let InstKind::ICall { function, args, .. } = &func.inst(inst_id).kind else {
                        continue;
                    };
                    if !self.static_frame_functions.contains(*function) {
                        continue;
                    }
                    let score = scores
                        .entry(*function)
                        .or_insert_with(|| IndexVec::from_vec(vec![Some(0); args.len()]));
                    if score.len() != args.len() {
                        excluded.insert(*function);
                        continue;
                    }
                    for (i, &arg) in args.iter().enumerate() {
                        let index = ArgIdx::new(i);
                        let Some(current) = score[index] else { continue };
                        let benefit = if Self::raw_arg_emittable(func, raw_leaves_ok, arg) {
                            // The frame store disappears outright.
                            Some(4)
                        } else if !Self::stack_arg_site_eligible(func, raw_leaves_ok, arg) {
                            // This site can neither emit the argument raw nor
                            // reload it through the computed-value spill path,
                            // so the argument must stay frame-passed everywhere.
                            None
                        } else {
                            Some(match func.value(arg) {
                                crate::mir::Value::Inst(def)
                                    if inst_block[*def] != Some(block_idx) =>
                                {
                                    // Cross-block values are stored at their
                                    // definition; the site keeps only the
                                    // slot reload it would have paid anyway.
                                    4
                                }
                                crate::mir::Value::Inst(_)
                                    if use_counts.get(&arg).copied().unwrap_or(0) > 1 =>
                                {
                                    // Multi-use block-local values usually
                                    // have a stack copy; the extra spill is
                                    // partially amortized.
                                    1
                                }
                                // A fresh single-use value pays a spill it
                                // did not need before.
                                _ => -5,
                            })
                        };
                        score[index] = benefit.map(|benefit| current.saturating_add(benefit));
                    }
                }
                if let Some(Terminator::TailCall { function, args }) = &block.terminator {
                    if !self.static_frame_functions.contains(*function) {
                        continue;
                    }
                    let score = scores
                        .entry(*function)
                        .or_insert_with(|| IndexVec::from_vec(vec![Some(0); args.len()]));
                    if score.len() != args.len() {
                        excluded.insert(*function);
                        continue;
                    }
                    for (i, &arg) in args.iter().enumerate() {
                        let index = ArgIdx::new(i);
                        let Some(current) = score[index] else { continue };
                        let benefit = if Self::raw_arg_emittable(func, raw_leaves_ok, arg) {
                            Some(4)
                        } else if !Self::stack_arg_site_eligible(func, raw_leaves_ok, arg) {
                            None
                        } else {
                            Some(match func.value(arg) {
                                crate::mir::Value::Inst(def)
                                    if inst_block[*def] != Some(block_idx) =>
                                {
                                    4
                                }
                                crate::mir::Value::Inst(_)
                                    if use_counts.get(&arg).copied().unwrap_or(0) > 1 =>
                                {
                                    1
                                }
                                _ => -5,
                            })
                        };
                        score[index] = benefit.map(|benefit| current.saturating_add(benefit));
                    }
                }
            }
        }
        scores.retain(|func_id, _| {
            self.static_frame_functions.contains(*func_id)
                && !self.recursive_frame_functions.contains(*func_id)
                && !excluded.contains(*func_id)
                && !self.disabled_stack_only_functions.contains(*func_id)
        });
        let mut masks = FxHashMap::default();
        for (func_id, score) in scores {
            // The callee prologue pays one store per stack argument.
            let mut mask = DenseBitSet::new_empty(score.len());
            for (index, benefit) in score.iter_enumerated() {
                if benefit.is_some_and(|benefit| benefit > 4) {
                    mask.insert(index.index());
                }
            }
            // The tail-call emitter shuffles the selected tuple into an exact
            // entry layout, so a mask beyond DUP16/SWAP16 reach could never be
            // constructed.
            if !mask.is_empty() && mask.count() <= MAX_STACK_ACCESS {
                masks.insert(func_id, mask);
            }
        }
        for (func_id, stack_args) in masks {
            self.static_call_abis.insert(
                func_id,
                StaticCallAbi { stack_args, entry: StaticCallEntry::Stored, returns: None },
            );
        }
    }

    /// Collects the canonical identity of each used static-callee argument once for the stack
    /// argument analyses below. Gas codegen canonicalizes argument operands before runtime
    /// planning, so every active occurrence of one argument must use the same value identity.
    fn collect_canonical_stack_arg_values(
        &self,
        module: &Module,
    ) -> FxHashMap<FunctionId, CanonicalArgValues> {
        let mut all_values = FxHashMap::default();
        if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            return all_values;
        }

        for func_id in self.static_frame_functions.iter() {
            let func = &module.functions[func_id];
            if func.params.is_empty() {
                continue;
            }
            let mut values = CanonicalArgValues::from_vec(vec![None; func.params.len()]);
            for value in func.live_values() {
                let crate::mir::Value::Arg(index) = func.value(value) else { continue };
                let canonical = &mut values[*index];
                debug_assert!(canonical.is_none_or(|existing| existing == value));
                *canonical = Some(value);
            }
            if values.iter().any(Option::is_some) {
                all_values.insert(func_id, values);
            }
        }
        all_values
    }

    /// Builds the subset-invariant analyses shared by one resident-layout
    /// search. The exhaustive subset loop evaluates up to `2^8` candidates;
    /// recomputing the CFG, its dominators, the phi plan, or operand counts
    /// per candidate made the search quadratic on large functions.
    fn resident_search_context(
        &self,
        func: &Function,
        liveness: &Liveness,
        values: &[ValueId],
        has_phis: bool,
    ) -> ResidentSearchContext {
        let mut value_uses = FxHashMap::default();
        for block in &func.blocks {
            for operand in block
                .instructions
                .iter()
                .flat_map(|&inst| func.inst(inst).kind.operands())
                .chain(block.terminator.iter().flat_map(Terminator::operands))
            {
                if values.contains(&operand) {
                    *value_uses.entry(operand).or_insert(0usize) += 1;
                }
            }
        }
        ResidentSearchContext {
            phi_plan: has_phis.then(|| StackPhiPlan::analyze(func, liveness, &self.cold_functions)),
            cfg: CfgInfo::new(func),
            value_uses,
        }
    }

    fn analyze_resident_subset(
        &self,
        func: &Function,
        liveness: &Liveness,
        values: &[ValueId],
        preserve_across_calls: bool,
        context: &ResidentSearchContext,
    ) -> Option<(GlobalStackPlan, ScheduleCost)> {
        let plan =
            GlobalStackPlan::analyze_resident_args(func, liveness, values, preserve_across_calls)?;
        if let Some(phi_plan) = &context.phi_plan {
            // One physical word cannot be both a phi input and an invariant resident prefix word.
            // `merge_resident` would otherwise extend only the result side of that edge, leaving a
            // non-square layout and a phantom word at the successor entry. Reject the complete or
            // candidate subset here and retain the frame home for those arguments.
            let resident_is_phi_source = phi_plan.edges.iter().any(|(&pred, edge)| {
                let term = func.blocks[pred]
                    .terminator
                    .as_ref()
                    .expect("stack-phi predecessor has no terminator");
                plan.edge_layout(func, term)
                    .is_some_and(|layout| layout.iter().any(|value| edge.sources.contains(value)))
            });
            if resident_is_phi_source {
                return None;
            }
            // A resident prefix on a planned backedge pays its shuffle on every loop iteration
            // and the composed emission is not yet correct for loop-carried prefixes: lifting
            // this gate miscompiled the nitro cold-prover paths (deep nested-loop deserialize
            // callees) and cost gas even where output stayed correct. Keep loop phis on the
            // established layout until the planner has execution-frequency-aware costing and
            // the loop composition is fixed; acyclic join edges compose without that
            // multiplier.
            let carries_planned_backedge = phi_plan.edges.keys().any(|&pred| {
                let Some(Terminator::Jump(target)) = func.blocks[pred].terminator.as_ref() else {
                    return false;
                };
                plan.entry(*target).is_some() && context.cfg.dominators().dominates(*target, pred)
            });
            if carries_planned_backedge {
                return None;
            }
        }

        let padding = plan
            .entries
            .iter()
            .map(|(block, entry)| {
                if GlobalStackPlan::is_terminal_block(func, *block)
                    || matches!(
                        func.blocks[*block].terminator,
                        Some(Terminator::TailCall { function, .. })
                            if self.cold_functions.contains(function)
                    )
                {
                    return 0;
                }
                entry.iter().filter(|&&value| !liveness.live_in(*block).contains(value)).count()
            })
            .sum::<usize>();
        // Without execution frequencies, the exact opcode cost below still cannot account for
        // padding paid repeatedly on hot edges. Require enough static uses to amortize every
        // padded word before comparing otherwise realizable layouts.
        let uses = values
            .iter()
            .map(|value| context.value_uses.get(value).copied().unwrap_or_default())
            .sum::<usize>();
        if uses < padding * 2 {
            return None;
        }
        let evm_version = self.gcx.sess.opts.evm_version;
        let padded_entry = ScheduleCost::stack_op(StackOp::Swap(1), evm_version)
            .plus(ScheduleCost::stack_op(StackOp::Pop, evm_version));
        let mut overhead = padded_entry.times(padding);
        for block in &func.blocks {
            match block.terminator.as_ref() {
                Some(term @ Terminator::Branch { then_block, else_block, .. }) => {
                    let Some((then_layout, else_layout)) = plan.branch_layouts(term) else {
                        continue;
                    };
                    let union = Self::global_branch_union(then_layout, else_layout);
                    let terminal_cleanup = (then_layout.is_empty()
                        && else_layout == union
                        && GlobalStackPlan::is_terminal_block(func, *then_block))
                        || (else_layout.is_empty()
                            && then_layout == union
                            && GlobalStackPlan::is_terminal_block(func, *else_block));
                    if then_layout != else_layout && !terminal_cleanup {
                        overhead = overhead.plus(ScheduleCost::control_flow_jump());
                    }
                }
                Some(term @ Terminator::Switch { .. }) => {
                    let Some(layouts) = plan.switch_layouts(term) else { continue };
                    let union = layouts.iter().fold(Vec::new(), |mut union, (_, layout)| {
                        for &value in *layout {
                            if !union.contains(&value) {
                                union.push(value);
                            }
                        }
                        union
                    });
                    let trampolines = layouts.iter().filter(|(_, layout)| *layout != union).count();
                    overhead = overhead.plus(
                        ScheduleCost::control_flow_jump()
                            .plus(ScheduleCost::jumpdest())
                            .times(trampolines),
                    );
                }
                _ => {}
            }
        }
        Some((plan, overhead))
    }

    /// Finds the least-cost realizable resident layout. This is deliberately exhaustive: the
    /// static ABI is capped at eight values, so all subsets are cheap to evaluate and a difficult
    /// argument need not force every independent word back into memory.
    fn select_resident_layout(
        &self,
        func: &Function,
        liveness: &Liveness,
        values: &[ValueId],
        preserve_across_calls: bool,
        has_phis: bool,
    ) -> Option<(Vec<ValueId>, GlobalStackPlan)> {
        debug_assert!(values.len() <= GLOBAL_STACK_LAYOUT_LIMIT);
        let mut use_counts = FxHashMap::default();
        let mut use_blocks = FxHashMap::default();
        for block in &func.blocks {
            let mut used_here = FxHashSet::default();
            for operand in block
                .instructions
                .iter()
                .flat_map(|&inst| func.inst(inst).kind.operands())
                .chain(block.terminator.iter().flat_map(Terminator::operands))
            {
                if values.contains(&operand) {
                    *use_counts.entry(operand).or_insert(0usize) += 1;
                    used_here.insert(operand);
                }
            }
            for operand in used_here {
                *use_blocks.entry(operand).or_insert(0usize) += 1;
            }
        }

        let frame_store = ScheduleCost::memory_store(OperandCostModel::DIRECT);
        let frame_load = ScheduleCost::memory_load(OperandCostModel::DIRECT);
        let resident_access =
            ScheduleCost::stack_op(StackOp::Dup(1), self.gcx.sess.opts.evm_version);
        let memory_cost = |value: ValueId| {
            let uses = use_counts.get(&value).copied().unwrap_or_default();
            let blocks = use_blocks.get(&value).copied().unwrap_or_default();
            frame_store
                .plus(frame_load.times(blocks))
                .plus(resident_access.times(uses.saturating_sub(blocks)))
        };
        let baseline = values
            .iter()
            .fold(ScheduleCost::default(), |cost, &value| cost.plus(memory_cost(value)));
        let optimization = self.gcx.sess.opts.optimization;
        let expected_executions = self.gcx.sess.opts.optimizer_runs.unwrap_or(200);
        let context = self.resident_search_context(func, liveness, values, has_phis);
        let mut best = Option::<(ScheduleCost, Vec<ValueId>, GlobalStackPlan)>::None;
        for bits in 1usize..(1usize << values.len()) {
            let subset = values
                .iter()
                .enumerate()
                .filter_map(|(index, &value)| ((bits >> index) & 1 != 0).then_some(value))
                .collect::<Vec<_>>();
            let Some((plan, mut candidate)) = self.analyze_resident_subset(
                func,
                liveness,
                &subset,
                preserve_across_calls,
                &context,
            ) else {
                continue;
            };

            // A resident word is accessed with a DUP/SWAP-class stack operation instead of a
            // direct-address load. Charge every use rather than assuming the last one is free;
            // this is conservative when a return shuffle consumes the final copy. A padded entry
            // pays both the exposure swap and final pop that codegen may need on its dead arm.
            for &value in values {
                candidate = candidate.plus(if subset.contains(&value) {
                    resident_access.times(
                        use_counts.get(&value).copied().unwrap_or_default().saturating_sub(1),
                    )
                } else {
                    memory_cost(value)
                });
            }
            if !candidate.cmp_lifetime_for(baseline, optimization, expected_executions).is_lt() {
                continue;
            }
            if best.as_ref().is_none_or(|(best_cost, best_values, _)| {
                candidate.cmp_lifetime_for(*best_cost, optimization, expected_executions).is_lt()
                    || (candidate == *best_cost && subset.len() > best_values.len())
            }) {
                best = Some((candidate, subset, plan));
            }
        }
        best.map(|(_, values, plan)| (values, plan))
    }

    /// Retains profitable computed words across acyclic joins while reserving ordinary spill slots
    /// as an edge-time fallback. The current global argument planner owns external entry layouts
    /// when it applies, so this incremental planner runs only when that plan is empty and limits
    /// its search to eight cross-block definitions.
    fn compute_cross_block_stack_layout(
        &self,
        func: &Function,
        liveness: &Liveness,
        cross_block_live: &OnceCell<DenseBitSet<ValueId>>,
        has_phis: bool,
    ) -> Option<(Vec<ValueId>, GlobalStackPlan)> {
        if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None)
            || !Self::is_external_entry(func)
            || func.blocks.len() < 3
        {
            return None;
        }

        let inst_blocks = func.inst_blocks();
        let cross_block =
            cross_block_live.get_or_init(|| Self::cross_block_live_values(func, liveness));
        let mut uses =
            FxHashMap::<ValueId, (BlockId, usize, FxHashSet<BlockId>, bool, bool, bool)>::default();
        for value in cross_block {
            let crate::mir::Value::Inst(inst_id) = func.value(value) else { continue };
            if matches!(func.inst(*inst_id).kind, InstKind::Phi(_))
                || Self::is_cross_block_recomputable_inst(func, value)
            {
                continue;
            }
            let Some(&definition) = inst_blocks.get(inst_id) else { continue };
            uses.insert(value, (definition, 0, FxHashSet::default(), false, false, false));
        }
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &user in &block.instructions {
                let phi = matches!(func.inst(user).kind, InstKind::Phi(_));
                for operand in func.inst(user).kind.operands() {
                    if let Some((definition, count, blocks, used_in_definition, used_by_phi, _)) =
                        uses.get_mut(&operand)
                    {
                        *used_in_definition |= block_id == *definition;
                        *used_by_phi |= phi;
                        *count += 1;
                        blocks.insert(block_id);
                    }
                }
            }
            for operand in block.terminator.iter().flat_map(Terminator::operands) {
                if let Some((definition, count, blocks, used_in_definition, _, _)) =
                    uses.get_mut(&operand)
                {
                    *used_in_definition |= block_id == *definition;
                    *count += 1;
                    blocks.insert(block_id);
                }
            }
            if block.predecessors.len() > 1 {
                for value in liveness.live_in(block_id) {
                    if let Some((_, _, _, _, _, crosses_join)) = uses.get_mut(&value) {
                        *crosses_join = true;
                    }
                }
            }
        }

        let mut ranked = Vec::new();
        for (value, (_, use_count, use_blocks, used_in_definition, used_by_phi, crosses_join)) in
            uses
        {
            // Definition-block and phi-edge uses need position-sensitive accounting. Leave those
            // to the existing spill/phi planners until this plan models individual program points.
            if used_in_definition || used_by_phi || !crosses_join || use_count < 2 {
                continue;
            }
            ranked.push((value, use_blocks.len(), use_count));
        }
        ranked.sort_unstable_by_key(|&(value, blocks, uses)| {
            (std::cmp::Reverse(blocks), std::cmp::Reverse(uses), value.index())
        });
        let values = ranked
            .into_iter()
            .take(GLOBAL_STACK_LAYOUT_LIMIT)
            .map(|(value, _, _)| value)
            .collect::<Vec<_>>();
        self.select_cross_block_stack_layout(func, liveness, &values, has_phis)
    }

    /// Keeps values that survive a low-memory calldata copy in a canonical
    /// stack layout until their uses are complete. The copied range is
    /// dynamic, so no fixed spill address is safe: a decompressor can grow its
    /// output through every compiler-owned low-memory slot before forwarding
    /// that output to a call.
    fn compute_spill_hazard_stack_layout(
        &self,
        func: &Function,
        liveness: &Liveness,
        stack_phi_plan: &StackPhiPlan,
        values: &[ValueId],
    ) -> Option<(Vec<ValueId>, GlobalStackPlan)> {
        if self.spill_hazard_insts.is_empty() {
            return None;
        }

        if values.is_empty() {
            return None;
        }

        let mut plan = GlobalStackPlan::analyze_resident_args(
            func,
            liveness,
            values,
            self.preserve_caller_stack,
        )?;
        // Phi operands are edge uses, not unchanged target live-ins. Full
        // liveness conservatively includes them at the header; remove those
        // incoming identities from the resident prefix so the phi edge can
        // replace each source with its result instead of trying to carry both.
        for (&pred, edge) in &stack_phi_plan.edges {
            let Some(Terminator::Jump(target)) = func.blocks[pred].terminator.as_ref() else {
                continue;
            };
            if let Some(entry) = plan.entries.get_mut(target) {
                entry.retain(|value| !edge.sources.contains(value));
            }
        }
        plan.entries.retain(|_, entry| !entry.is_empty());
        Some((values.to_vec(), plan))
    }

    /// Values that need a successor after a forwarding-buffer clobber.
    fn spill_hazard_cross_block_values(
        &self,
        func: &Function,
        liveness: &Liveness,
        cross_block_live: &OnceCell<DenseBitSet<ValueId>>,
        recomputable: &DenseBitSet<ValueId>,
    ) -> Vec<ValueId> {
        if self.spill_hazard_is_repeated_low_phi(func) {
            return cross_block_live
                .get_or_init(|| Self::cross_block_live_values(func, liveness))
                .iter()
                .filter(|&value| {
                    Self::can_own_spill_slot(func, value) && !recomputable.contains(value)
                })
                .collect();
        }

        let inst_blocks = func.inst_blocks();
        let mut values = DenseBitSet::new_empty(func.num_values());
        for inst in &self.spill_hazard_insts {
            let Some(&block) = inst_blocks.get(inst) else { continue };
            for value in liveness.live_out(block) {
                if Self::can_own_spill_slot(func, value) && !recomputable.contains(value) {
                    values.insert(value);
                }
            }
        }
        values.iter().collect()
    }

    /// Whether a low forwarding destination is a loop-carried pointer. Every
    /// cross-block value participates in the loop's canonical stack shape;
    /// selecting only the values live out of the copy can omit phi companions
    /// needed to preserve that shape on the backedge.
    fn spill_hazard_is_repeated_low_phi(&self, func: &Function) -> bool {
        let inst_blocks = func.inst_blocks();
        let mut loop_analyzer = LoopAnalyzer::new();
        let loop_info = loop_analyzer.analyze(func);
        loop_info.all_loops().any(|loop_info| {
            self.spill_hazard_insts.iter().any(|inst| {
                inst_blocks.get(inst).is_some_and(|block| loop_info.blocks.contains(*block))
                    && Self::dynamic_spill_write_dest(func, *inst).is_some_and(|dest| {
                        matches!(func.value(dest), Value::Inst(definition)
                        if matches!(&func.inst(*definition).kind, InstKind::Phi(incoming)
                            if incoming.iter().any(|&(_, value)| {
                                func.value_u64(value).is_some_and(|address| {
                                    address < EvmMemoryLayout::HEAP_START
                                })
                            })))
                    })
            })
        })
    }

    /// Whether an existing resident plan carries every at-risk live-out.
    fn stack_plan_carries_spill_hazards(
        &self,
        func: &Function,
        liveness: &Liveness,
        plan: &GlobalStackPlan,
        hazard_values: &[ValueId],
    ) -> bool {
        let inst_blocks = func.inst_blocks();
        self.spill_hazard_insts.iter().all(|inst| {
            let Some(&block_id) = inst_blocks.get(inst) else { return false };
            let Some(term) = func.blocks[block_id].terminator.as_ref() else { return false };
            let carried = plan.uniformly_carried_values(func, term);
            hazard_values
                .iter()
                .filter(|&&value| liveness.live_out(block_id).contains(value))
                .all(|value| carried.contains(value))
        })
    }

    fn select_cross_block_stack_layout(
        &self,
        func: &Function,
        liveness: &Liveness,
        values: &[ValueId],
        has_phis: bool,
    ) -> Option<(Vec<ValueId>, GlobalStackPlan)> {
        if values.is_empty() {
            return None;
        }
        debug_assert!(values.len() <= GLOBAL_STACK_LAYOUT_LIMIT);

        let mut use_counts = FxHashMap::default();
        let mut use_blocks = FxHashMap::<ValueId, FxHashSet<BlockId>>::default();
        for (block_id, block) in func.blocks.iter_enumerated() {
            for operand in block
                .instructions
                .iter()
                .flat_map(|&inst| func.inst(inst).kind.operands())
                .chain(block.terminator.iter().flat_map(Terminator::operands))
            {
                if values.contains(&operand) {
                    *use_counts.entry(operand).or_insert(0usize) += 1;
                    use_blocks.entry(operand).or_default().insert(block_id);
                }
            }
        }

        let spill_store = ScheduleCost::memory_store(OperandCostModel::DIRECT);
        let spill_load = ScheduleCost::memory_load(OperandCostModel::DIRECT);
        let resident_access =
            ScheduleCost::stack_op(StackOp::Dup(1), self.gcx.sess.opts.evm_version);
        let memory_cost = |value: ValueId| {
            let uses = use_counts.get(&value).copied().unwrap_or_default();
            let blocks = use_blocks.get(&value).map_or(0, FxHashSet::len);
            spill_store
                .plus(spill_load.times(blocks))
                .plus(resident_access.times(uses.saturating_sub(blocks)))
        };
        let baseline = values
            .iter()
            .fold(ScheduleCost::default(), |cost, &value| cost.plus(memory_cost(value)));
        let optimization = self.gcx.sess.opts.optimization;
        let expected_executions = self.gcx.sess.opts.optimizer_runs.unwrap_or(200);
        let context = self.resident_search_context(func, liveness, values, has_phis);
        let mut best = Option::<(ScheduleCost, Vec<ValueId>, GlobalStackPlan)>::None;
        for bits in 1usize..(1usize << values.len()) {
            let subset = values
                .iter()
                .enumerate()
                .filter_map(|(index, &value)| ((bits >> index) & 1 != 0).then_some(value))
                .collect::<Vec<_>>();
            let Some((plan, mut candidate)) = self.analyze_resident_subset(
                func,
                liveness,
                &subset,
                self.preserve_caller_stack,
                &context,
            ) else {
                continue;
            };
            if plan.entries.iter().any(|(&block, _)| {
                func.blocks[block]
                    .predecessors
                    .iter()
                    .any(|&pred| context.cfg.dominators().dominates(block, pred))
            }) {
                continue;
            }

            for &value in values {
                candidate = candidate.plus(if subset.contains(&value) {
                    let uses = use_counts.get(&value).copied().unwrap_or_default();
                    // A carried SSA copy can be consumed on its final use; all earlier uses retain
                    // it with a stack operation. Its spill store is emitted only if an edge falls
                    // back to memory, so it does not belong to the selected layout's hot cost.
                    resident_access.times(uses.saturating_sub(1))
                } else {
                    memory_cost(value)
                });
            }
            if !candidate.cmp_lifetime_for(baseline, optimization, expected_executions).is_lt() {
                continue;
            }
            if best.as_ref().is_none_or(|(best_cost, best_values, _)| {
                candidate.cmp_lifetime_for(*best_cost, optimization, expected_executions).is_lt()
                    || (candidate == *best_cost && subset.len() > best_values.len())
            }) {
                best = Some((candidate, subset, plan));
            }
        }
        best.map(|(_, values, plan)| (values, plan))
    }

    /// Promotes a profitable subset of stack arguments to a callee-wide physical layout. The
    /// ordinary stack-argument convention pays one
    /// prologue `MSTORE` and subsequent frame `MLOAD`s. A proven resident
    /// layout removes both, including across joins and loops.
    fn compute_resident_stack_args(
        &mut self,
        module: &Module,
        arg_values: &FxHashMap<FunctionId, CanonicalArgValues>,
    ) {
        for abi in self.static_call_abis.values_mut() {
            if matches!(abi.entry, StaticCallEntry::Resident { .. }) {
                abi.entry = StaticCallEntry::Stored;
            }
        }
        if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            return;
        }

        // Start with every used canonical argument of an eligible function.
        // A computed actual can use the same validated spill/reload fallback
        // as ordinary stack arguments. Residency removes the callee prologue
        // store and every later frame load, so it can amortize that caller-side
        // copy even when the actual is not directly rematerializable.
        let mut candidates = FxHashMap::default();
        for (&func_id, values) in arg_values {
            if self.disabled_stack_only_functions.contains(func_id)
                || !self.static_frame_functions.contains(func_id)
                || self.recursive_stack_functions.contains(func_id)
                || self.recursion_reaching_functions.contains(func_id)
            {
                continue;
            }
            let func = &module.functions[func_id];
            let mut mask = DenseBitSet::new_empty(func.params.len());
            for index in 0..func.params.len() {
                if values[ArgIdx::new(index)].is_some() {
                    mask.insert(index);
                }
            }
            if !mask.is_empty() {
                candidates.insert(func_id, mask);
            }
        }

        let mut seen = DenseBitSet::new_empty(module.functions.len());
        let mut excluded = DenseBitSet::new_empty(module.functions.len());
        for (caller_id, caller) in module.functions.iter_enumerated() {
            let raw_leaves_ok =
                Self::is_external_entry(caller) || self.static_frame_functions.contains(caller_id);
            for block in &caller.blocks {
                for &inst_id in &block.instructions {
                    let InstKind::ICall { function, args, .. } = &caller.inst(inst_id).kind else {
                        continue;
                    };
                    let Some(mask) = candidates.get_mut(function) else { continue };
                    seen.insert(*function);
                    if args.len() != mask.domain_size() {
                        excluded.insert(*function);
                        continue;
                    }
                    for (index, &arg) in args.iter().enumerate() {
                        if !Self::stack_arg_site_eligible(caller, raw_leaves_ok, arg) {
                            mask.remove(index);
                        }
                    }
                }
                if let Some(Terminator::TailCall { function, args }) = &block.terminator
                    && let Some(mask) = candidates.get_mut(function)
                {
                    seen.insert(*function);
                    if args.len() != mask.domain_size() {
                        excluded.insert(*function);
                        continue;
                    }
                    for (index, &arg) in args.iter().enumerate() {
                        if !Self::raw_arg_emittable(caller, raw_leaves_ok, arg)
                            && !matches!(caller.value(arg), crate::mir::Value::Inst(_))
                        {
                            mask.remove(index);
                        }
                    }
                }
            }
        }

        for (func_id, mut mask) in candidates {
            if !seen.contains(func_id) || excluded.contains(func_id) || mask.is_empty() {
                continue;
            }
            if mask.count() > GLOBAL_STACK_LAYOUT_LIMIT {
                let retained: Vec<_> = mask.iter().take(GLOBAL_STACK_LAYOUT_LIMIT).collect();
                mask.clear();
                for index in retained {
                    mask.insert(index);
                }
            }

            let func = &module.functions[func_id];
            let arg_values = &arg_values[&func_id];
            let mut values = Vec::with_capacity(mask.count());
            let mut eligible = true;
            for index in mask.iter() {
                let Some(value) = arg_values[ArgIdx::new(index)] else {
                    eligible = false;
                    break;
                };
                values.push(value);
            }
            if !eligible {
                continue;
            }
            // The layout keeps arguments in descending index order.
            values.reverse();

            // Reject shapes the resident-layout analysis cannot represent before paying for
            // whole-function liveness. Single-block leaves have no inter-block layout to solve.
            if func.blocks.iter().any(|block| {
                !self.preserve_caller_stack
                    && block
                        .instructions
                        .iter()
                        .any(|&inst_id| matches!(func.inst(inst_id).kind, InstKind::ICall { .. }))
            }) {
                continue;
            }
            let has_phis =
                func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::Phi(_)));
            let liveness = (func.blocks.len() != 1 || has_phis).then(|| Liveness::compute(func));
            let plan = if let Some(liveness) = &liveness {
                let context = self.resident_search_context(func, liveness, &values, has_phis);
                if let Some((plan, _)) = self.analyze_resident_subset(
                    func,
                    liveness,
                    &values,
                    self.preserve_caller_stack,
                    &context,
                ) {
                    // Preserve the established full-tuple layout when it passes the structural
                    // amortization guard. Costed subset selection is a fallback for tuples where
                    // one difficult value would otherwise disable every independent resident
                    // argument.
                    plan
                } else if let Some((subset, plan)) = self.select_resident_layout(
                    func,
                    liveness,
                    &values,
                    self.preserve_caller_stack,
                    has_phis,
                ) {
                    values = subset;
                    mask.clear();
                    for &value in &values {
                        let crate::mir::Value::Arg(index) = func.value(value) else {
                            unreachable!("resident candidates are canonical arguments")
                        };
                        mask.insert(index.index());
                    }
                    plan
                } else {
                    continue;
                }
            } else {
                GlobalStackPlan {
                    entries: FxHashMap::default(),
                    aliases: FxHashMap::default(),
                    terminal_sensitive: true,
                }
            };
            let abi = self.static_call_abi_mut(func_id, func.params.len());
            let mut stack_args = DenseBitSet::new_empty(func.params.len());
            for index in abi.stack_args.iter().chain(mask.iter()) {
                if arg_values[ArgIdx::new(index)].is_some() {
                    stack_args.insert(index);
                }
            }
            abi.stack_args = stack_args;
            abi.entry = StaticCallEntry::Resident { values, layout: plan };
        }
    }

    /// Collects operand-use facts shared by lazy and direct stack-argument selection.
    fn collect_stack_arg_uses(&self, module: &Module) -> FxHashMap<FunctionId, StackArgUseInfo> {
        let mut all_uses = FxHashMap::default();
        if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            return all_uses;
        }

        for (&func_id, abi) in &self.static_call_abis {
            if matches!(abi.entry, StaticCallEntry::Resident { .. }) || abi.stack_args.is_empty() {
                continue;
            }
            let func = &module.functions[func_id];
            let mut info = StackArgUseInfo {
                use_counts: FxHashMap::default(),
                non_entry_uses: DenseBitSet::new_empty(func.num_values()),
                call_uses: DenseBitSet::new_empty(func.num_values()),
                entry_first_uses: FxHashMap::default(),
                first_entry_call: None,
            };
            for (block_id, block) in func.blocks.iter_enumerated() {
                for (inst_idx, &inst_id) in block.instructions.iter().enumerate() {
                    let kind = &func.inst(inst_id).kind;
                    let is_call = matches!(kind, InstKind::ICall { .. });
                    if block_id == BlockId::ENTRY && info.first_entry_call.is_none() && is_call {
                        info.first_entry_call = Some(inst_idx);
                    }
                    for operand in kind.operands() {
                        *info.use_counts.entry(operand).or_insert(0) += 1;
                        if block_id == BlockId::ENTRY {
                            info.entry_first_uses.entry(operand).or_insert(inst_idx);
                        } else {
                            info.non_entry_uses.insert(operand);
                        }
                        if is_call {
                            info.call_uses.insert(operand);
                        }
                    }
                }
                if let Some(term) = &block.terminator {
                    let is_call = matches!(term, Terminator::TailCall { .. });
                    for operand in term.operands() {
                        *info.use_counts.entry(operand).or_insert(0) += 1;
                        if block_id != BlockId::ENTRY {
                            info.non_entry_uses.insert(operand);
                        }
                        if is_call {
                            info.call_uses.insert(operand);
                        }
                    }
                }
            }
            all_uses.insert(func_id, info);
        }
        all_uses
    }

    /// Selects stack-passed arguments that the callee can consume directly.
    ///
    /// Every selected argument must have one canonical value identity and all of its uses must stay
    /// in the entry block. The local scheduler can then retain or duplicate that physical word for
    /// as many operations as need it without giving it a memory home. Arguments consumed by a
    /// nested call remain frame-passed until call layouts can carry stack-only values through the
    /// nested edge. Requiring the entire stack-argument mask to qualify lets the prologue omit
    /// every store without shuffling around partially materialized words.
    fn compute_direct_stack_args(
        &mut self,
        module: &Module,
        arg_values: &FxHashMap<FunctionId, CanonicalArgValues>,
        use_info: &FxHashMap<FunctionId, StackArgUseInfo>,
    ) {
        for abi in self.static_call_abis.values_mut() {
            if matches!(abi.entry, StaticCallEntry::Direct(_)) {
                abi.entry = StaticCallEntry::Stored;
            }
        }
        if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            return;
        }

        let candidates: Vec<_> = self
            .static_call_abis
            .iter()
            .filter(|(_, abi)| matches!(abi.entry, StaticCallEntry::Stored))
            .map(|(&func_id, abi)| (func_id, abi.stack_args.clone()))
            .collect();
        for (func_id, mask) in candidates {
            if self.disabled_stack_only_functions.contains(func_id) {
                continue;
            }
            let func = &module.functions[func_id];
            if mask.domain_size() != func.params.len() {
                continue;
            }
            if mask.count() > 4 {
                continue;
            }
            // A direct stack argument has no frame home and cannot own a spill slot, so a
            // stack-draining internal or tail call between the entry stack and a later use would
            // drop it and reload it from its never-written frame slot. `use_info` only rejects
            // arguments consumed *by* a call, not ones merely live across one, so exclude any
            // callee that makes a call at all. Resident and lazy selection already
            // cover the callee shapes that survive a drain.
            if func.blocks.iter().any(|block| {
                block
                    .instructions
                    .iter()
                    .any(|&inst| matches!(func.inst(inst).kind, InstKind::ICall { .. }))
                    || matches!(
                        block.terminator,
                        Some(Terminator::TailCall { .. } | Terminator::Switch { .. })
                    )
            }) {
                continue;
            }
            let Some(arg_values) = arg_values.get(&func_id) else { continue };
            let Some(info) = use_info.get(&func_id) else { continue };

            let mut values = Vec::with_capacity(mask.count());
            let mut eligible = true;
            for index in mask.iter() {
                let Some(value) = arg_values[ArgIdx::new(index)] else {
                    eligible = false;
                    break;
                };
                if info.use_counts.get(&value).copied().unwrap_or(0) == 0
                    || info.non_entry_uses.contains(value)
                    || info.call_uses.contains(value)
                {
                    eligible = false;
                    break;
                }
                values.push(value);
            }
            // The entry layout keeps arguments in descending index order.
            values.reverse();
            if eligible && !values.is_empty() {
                self.static_call_abi_mut(func_id, func.params.len()).entry =
                    StaticCallEntry::Direct(values);
            }
        }
    }

    /// Selects stack arguments whose first memory materialization can move past their first use.
    ///
    /// The whole mask must qualify because the incoming words are contiguous above the return
    /// address. Each selected argument needs an identity used by the entry block's first
    /// instruction. A repeated argument gets a frame home immediately before that instruction; a
    /// single-use argument is consumed directly from the incoming stack. This restriction keeps
    /// the rewrite local and prevents it from changing later stack scheduling or CFG layout.
    fn compute_lazy_stack_args(
        &mut self,
        module: &Module,
        arg_values: &FxHashMap<FunctionId, CanonicalArgValues>,
        use_info: &FxHashMap<FunctionId, StackArgUseInfo>,
    ) {
        for abi in self.static_call_abis.values_mut() {
            if matches!(abi.entry, StaticCallEntry::Lazy(_)) {
                abi.entry = StaticCallEntry::Stored;
            }
        }
        if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            return;
        }

        let candidates: Vec<_> = self
            .static_call_abis
            .iter()
            .filter(|(_, abi)| matches!(abi.entry, StaticCallEntry::Stored))
            .map(|(&func_id, abi)| (func_id, abi.stack_args.clone()))
            .collect();
        for (func_id, mask) in candidates {
            if self.disabled_stack_only_functions.contains(func_id) {
                continue;
            }
            if mask.count() > MAX_STACK_ACCESS {
                continue;
            }
            let func = &module.functions[func_id];
            if mask.domain_size() != func.params.len() {
                continue;
            }

            let Some(arg_values) = arg_values.get(&func_id) else { continue };
            let Some(info) = use_info.get(&func_id) else { continue };
            let mut args = Vec::with_capacity(mask.count());
            let mut frame_values = DenseBitSet::new_empty(func.num_values());
            let mut eligible = true;
            for index in mask.iter() {
                let Some(value) = arg_values[ArgIdx::new(index)] else {
                    eligible = false;
                    break;
                };
                let Some(&first_use) = info.entry_first_uses.get(&value) else {
                    eligible = false;
                    break;
                };
                if info.first_entry_call.is_some_and(|call| first_use >= call) {
                    eligible = false;
                    break;
                }
                if first_use != 0 {
                    eligible = false;
                    break;
                }
                args.push((ArgIdx::new(index), value));
                let total_uses = info.use_counts.get(&value).copied().unwrap_or(0);
                if total_uses > 1 {
                    frame_values.insert(value);
                }
            }
            // Materialization emits in descending index order.
            args.reverse();
            if eligible && !args.is_empty() {
                self.static_call_abi_mut(func_id, func.params.len()).entry =
                    StaticCallEntry::Lazy(LazyStackArgPlan { args, frame_values });
            }
        }
    }

    /// Returns true when the caller can re-emit `val` raw (untracked) after
    /// its stack drain.
    fn raw_arg_emittable(func: &Function, raw_leaves_ok: bool, val: ValueId) -> bool {
        match func.value(val) {
            crate::mir::Value::Immediate(imm) => imm.as_u256().is_some(),
            crate::mir::Value::Arg(_) => raw_leaves_ok,
            crate::mir::Value::Inst(_) => rematerializable_nullary_value(func, val).is_some(),
            _ => false,
        }
    }

    /// Returns whether one call site can participate in a stack-argument convention. Computed
    /// values reload from a validated spill after draining the modeled stack; this remains valid
    /// for a dynamic-frame caller because a static call does not replace its frame pointer and the
    /// reload is emitted before control transfers to the callee. Caller arguments do not own spill
    /// slots, so they still require the position-independent raw path. Both ordinary and resident
    /// selection use this predicate to keep their call-site eligibility invariant identical.
    fn stack_arg_site_eligible(func: &Function, raw_leaves_ok: bool, val: ValueId) -> bool {
        Self::raw_arg_emittable(func, raw_leaves_ok, val)
            || matches!(func.value(val), crate::mir::Value::Inst(_))
    }

    /// Emits a mask-qualified argument without touching the scheduler model:
    /// the value lands on the physical stack for the callee prologue, below
    /// everything the caller's model describes.
    fn emit_raw_stack_arg(
        &mut self,
        func: &Function,
        val: ValueId,
        spill_slot: Option<SpillSlot>,
        caller_stack: Option<&StackModel>,
        words_above: usize,
    ) {
        if let Some(op) = Self::always_rematerializable_op(func, val) {
            self.asm.emit_op(op);
            return;
        }

        if let crate::mir::Value::Immediate(imm) = func.value(val)
            && imm.as_u256() == Some(U256::ZERO)
            && self.gcx.sess.opts.evm_version.has_push0()
        {
            self.asm.emit_push(U256::ZERO);
            return;
        }

        if let Some(depth) = caller_stack.and_then(|stack| stack.find(val)) {
            let dup = depth + words_above + 1;
            assert!(
                dup <= MAX_STACK_ACCESS,
                "resident caller argument exceeded DUP16 reach at an internal call"
            );
            self.asm.emit_stack_op(StackOp::Dup(dup as u8));
            return;
        }

        match func.value(val) {
            crate::mir::Value::Immediate(imm) => {
                self.asm.emit_push(imm.as_u256().expect("mask requires a word immediate"));
            }
            crate::mir::Value::Arg(index) => {
                if self.in_internal_function {
                    let func_id = self
                        .current_internal_function
                        .expect("internal caller has a current function");
                    let addr = self.static_frame_addr(
                        func_id,
                        EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                            + (index.index() as u64) * EvmMemoryLayout::WORD_SIZE,
                    );
                    self.asm.emit_push_deferred(addr);
                    self.asm.emit_op(op::MLOAD);
                } else {
                    self.asm.emit_push(U256::from(4 + (index.index() as u64) * WORD_BYTES as u64));
                    self.asm.emit_op(op::CALLDATALOAD);
                }
            }
            crate::mir::Value::Inst(_) => {
                let slot = spill_slot.expect("computed stack argument has a validated spill slot");
                self.emit_spill_load(func, slot);
            }
            other => unreachable!("stack-arg mask admitted an unsupported value: {other:?}"),
        }
    }

    /// Stores the stack-passed arguments of `func_id` into their frame slots.
    /// Arguments were pushed in index order, so the highest index is on top;
    /// after the last store only the return address remains above the
    /// caller's drained stack.
    fn emit_stack_arg_prologue(&mut self, func_id: FunctionId, func: &Function) {
        if !self.runtime_stack_args {
            return;
        }
        if self.direct_stack_args(func_id).is_some() || self.lazy_stack_args(func_id).is_some() {
            return;
        }
        let Some(mask) = self.stack_arg_mask(func_id).cloned() else { return };
        if mask.domain_size() != func.params.len() {
            return;
        }
        // A selective resident ABI may leave only part of the incoming tuple on-stack. Store the
        // other words even when resident words sit above them, then leave exactly the layout that
        // `generate_function_body` adopts. The hidden return address remains immediately below it.
        let stack_indices = mask.iter().collect::<Vec<_>>();
        if let Some(resident) = self.resident_stack_args(func_id).map(|values| values.to_vec()) {
            let mut args = CanonicalArgValues::from_vec(vec![None; func.params.len()]);
            for value in func.live_values() {
                if let crate::mir::Value::Arg(index) = func.value(value) {
                    args[*index] = Some(value);
                }
            }
            let mut incoming = StackModel::new();
            for &index in &stack_indices {
                incoming.push(args[ArgIdx::new(index)].expect("stack argument has no identity"));
            }
            for &index in stack_indices.iter().rev() {
                let value = args[ArgIdx::new(index)].expect("stack argument has no identity");
                if resident.contains(&value) {
                    continue;
                }
                let depth = incoming
                    .find(value)
                    .expect("non-resident stack argument disappeared in the callee prologue");
                if depth != 0 {
                    assert!(depth <= MAX_STACK_ACCESS, "stack argument exceeded SWAP16 reach");
                    self.asm.emit_stack_op(StackOp::Swap(depth as u8));
                    incoming.swap(depth as u8);
                }
                let addr = self.static_frame_addr(
                    func_id,
                    EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                        + index as u64 * EvmMemoryLayout::WORD_SIZE,
                );
                self.asm.emit_push_deferred(addr);
                self.asm.emit_op(op::MSTORE);
                incoming.pop();
            }
            let target: Vec<_> = resident.iter().copied().map(TargetSlot::Value).collect();
            let mut scheduler = StackScheduler::for_evm_version(self.gcx.sess.opts.evm_version);
            scheduler.stack = incoming;
            let shuffle = scheduler.shuffle_to_layout(&target).unwrap_or_else(|| {
                panic!("could not construct selective resident entry layout for `{}`", func.name)
            });
            for op in shuffle.ops {
                self.asm.emit_stack_op(op);
            }
            debug_assert_eq!(
                scheduler.stack.as_slice(),
                resident.iter().copied().map(Some).collect::<Vec<_>>().as_slice(),
                "selective resident prologue produced the wrong entry layout"
            );
            return;
        }

        for i in stack_indices.into_iter().rev() {
            let addr = self.static_frame_addr(
                func_id,
                EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE + i as u64 * EvmMemoryLayout::WORD_SIZE,
            );
            self.asm.emit_push_deferred(addr);
            self.asm.emit_op(op::MSTORE);
        }
    }

    /// Gives a stack-passed argument a valid frame home while retaining its stack copy.
    fn materialize_stack_arg(&mut self, func_id: FunctionId, index: ArgIdx, value: ValueId) {
        if !self.scheduler.is_stack_only_value(value) {
            return;
        }
        let depth = self.scheduler.stack.find(value).unwrap_or_else(|| {
            panic!("stack argument {value:?} was lost before frame materialization")
        });
        assert!(depth < MAX_STACK_ACCESS, "stack argument exceeded DUP16 reach");
        self.emit_stack_op(StackOp::Dup((depth + 1) as u8));

        let addr = self.static_frame_addr(
            func_id,
            EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                + (index.index() as u64) * EvmMemoryLayout::WORD_SIZE,
        );
        self.asm.emit_push_deferred(addr);
        self.scheduler.stack.push_unknown();
        self.asm.emit_op(op::MSTORE);
        self.scheduler.instruction_executed(2, None);
        self.scheduler.materialize_stack_only_value(value);
    }

    /// Gives a stack-only value a memory home before a fallback drains the physical stack.
    fn materialize_stack_only_home(
        &mut self,
        func_id: FunctionId,
        func: &Function,
        value: ValueId,
    ) {
        if !self.scheduler.is_stack_only_value(value) {
            return;
        }
        match func.value(value) {
            crate::mir::Value::Arg(index) => self.materialize_stack_arg(func_id, *index, value),
            crate::mir::Value::Inst(_) => {
                let depth = self.scheduler.stack.find(value).unwrap_or_else(|| {
                    panic!("stack-only value {value:?} was lost before memory materialization")
                });
                let slot = self.scheduler.spills.allocate(value);
                if depth >= self.stack_access_limit() {
                    self.spill_deep_stack_value(func, value, slot, depth);
                } else {
                    self.spill_accessible_stack_value(func, value, slot, depth);
                }
                self.scheduler.materialize_stack_only_value(value);
            }
            crate::mir::Value::Immediate(_)
            | crate::mir::Value::Undef(_)
            | crate::mir::Value::Error(_) => unreachable!("unsupported stack-only value"),
        }
    }

    /// Gives resident arguments a frame fallback before an emission stage can bury their last
    /// stack copy beyond `DUP16` reach. `transient_growth` bounds the words pushed before the stage
    /// reaches a resident operand, or the one result left by an ordinary MIR instruction.
    fn materialize_deep_stack_args(
        &mut self,
        func_id: FunctionId,
        func: &Function,
        transient_growth: usize,
    ) {
        if transient_growth == 0 {
            return;
        }
        let materialize_depth = MAX_STACK_ACCESS.saturating_sub(transient_growth);
        let mut disabled_residency = false;
        loop {
            let entry = self.scheduler.stack.iter().enumerate().find_map(|(depth, value)| {
                value
                    .filter(|&value| {
                        depth >= materialize_depth && self.scheduler.is_stack_only_value(value)
                    })
                    .map(|value| (depth, value))
            });
            let Some((_, value)) = entry else { break };
            disabled_residency |= matches!(func.value(value), crate::mir::Value::Arg(_));
            self.materialize_stack_only_home(func_id, func, value);
        }
        if disabled_residency {
            self.disabled_stack_only_functions.insert(func_id);
        }
    }

    /// Materializes repeated arguments immediately before the entry block's first instruction.
    fn materialize_lazy_stack_args(
        &mut self,
        func_id: FunctionId,
        kind: &InstKind,
        block: BlockId,
        inst_idx: usize,
    ) {
        if block != BlockId::ENTRY || inst_idx != 0 {
            return;
        }
        let Some(plan) = self.lazy_stack_args(func_id).cloned() else { return };
        let operands = kind.operands();
        for (index, value) in plan.args {
            debug_assert!(operands.contains(&value));
            if plan.frame_values.contains(value) {
                self.materialize_stack_arg(func_id, index, value);
            }
        }
    }

    /// Plans a bounded rotation that keeps computed arguments on the physical
    /// stack while the rest of the caller stack is drained. The resulting
    /// layout matches the existing stack-argument convention: selected
    /// arguments in descending index order above the return address.
    fn plan_retained_stack_args(
        &self,
        func: &Function,
        args: &[ValueId],
        mask: &DenseBitSet<usize>,
    ) -> Option<StackArgRetentionPlan> {
        let selected = mask.count();
        if mask.domain_size() != args.len()
            || selected == 0
            || selected > STACK_ARG_ROTATION_LIMIT
            || self.scheduler.stack.depth() > STACK_ARG_ROTATION_LIMIT + 1
        {
            return None;
        }

        // One physical word cannot fill two argument positions. Repeated
        // values keep the spill-reload path, which materializes each
        // occurrence independently.
        let mut selected_value_counts = FxHashMap::default();
        for (i, &arg) in args.iter().enumerate() {
            if mask.contains(i) && matches!(func.value(arg), crate::mir::Value::Inst(_)) {
                *selected_value_counts.entry(arg).or_insert(0usize) += 1;
            }
        }
        let candidates: Vec<_> = args
            .iter()
            .enumerate()
            .filter_map(|(i, &arg)| {
                (mask.contains(i)
                    && selected_value_counts.get(&arg) == Some(&1)
                    && self.scheduler.stack.contains(arg))
                .then_some(i)
            })
            .collect();
        if candidates.is_empty() {
            return None;
        }
        self.build_stack_arg_retention_plan(args, mask, &candidates)
    }

    fn build_stack_arg_retention_plan(
        &self,
        args: &[ValueId],
        mask: &DenseBitSet<usize>,
        retained_indices: &[usize],
    ) -> Option<StackArgRetentionPlan> {
        let mut keep = FxHashMap::default();
        for &index in retained_indices {
            keep.insert(args[index], index);
        }

        let mut stack = self.scheduler.stack.as_slice().to_vec();
        let mut drain_ops = Vec::new();
        while stack.len() > keep.len() {
            let depth = stack.iter().position(|word| match word {
                Some(value) if keep.contains_key(value) => {
                    stack.iter().filter(|other| **other == *word).count() > 1
                }
                _ => true,
            })?;
            if depth > STACK_ARG_ROTATION_LIMIT {
                return None;
            }
            if depth != 0 {
                drain_ops.push(StackOp::Swap(depth as u8));
                stack.swap(0, depth);
            }
            drain_ops.push(StackOp::Pop);
            stack.remove(0);
        }

        let mut layout = Vec::with_capacity(mask.count() + 1);
        for word in stack {
            layout.push(StaticCallStackWord::Argument(*keep.get(&word?)?));
        }
        layout.insert(0, StaticCallStackWord::ReturnAddress);
        for i in mask.iter() {
            if !retained_indices.contains(&i) {
                layout.insert(0, StaticCallStackWord::Argument(i));
            }
        }

        let mut target: Vec<_> = mask.iter().map(StaticCallStackWord::Argument).collect();
        target.reverse();
        target.push(StaticCallStackWord::ReturnAddress);
        if layout.len() != target.len() || layout.len() > STACK_ARG_ROTATION_LIMIT + 1 {
            return None;
        }

        let mut shuffle_ops = Vec::new();
        for target_depth in (1..layout.len()).rev() {
            if layout[target_depth] == target[target_depth] {
                continue;
            }
            let source_depth =
                layout[..=target_depth].iter().position(|&word| word == target[target_depth])?;
            if source_depth != 0 {
                shuffle_ops.push(StackOp::Swap(source_depth as u8));
                layout.swap(0, source_depth);
            }
            shuffle_ops.push(StackOp::Swap(target_depth as u8));
            layout.swap(0, target_depth);
        }
        debug_assert_eq!(layout, target);

        // Baseline drains every tracked word and reloads each computed stack
        // argument through at least PUSH1+MLOAD. A value without a stored slot
        // also pays at least DUP+PUSH1+MSTORE. Deferred addresses can only make
        // that baseline larger, so this is a conservative byte gate.
        let fresh = retained_indices
            .iter()
            .filter(|&&index| !self.scheduler.spills.is_stored(args[index]))
            .count();
        let baseline_cost = self.scheduler.stack.depth() + retained_indices.len() * 3 + fresh * 4;
        let planned_cost = drain_ops.len() + shuffle_ops.len();
        if planned_cost >= baseline_cost {
            return None;
        }

        let mut retained = DenseBitSet::new_empty(args.len());
        for &index in retained_indices {
            retained.insert(index);
        }
        Some(StackArgRetentionPlan { retained, drain_ops, shuffle_ops })
    }

    fn static_frame_addr(&mut self, func_id: FunctionId, offset: u64) -> DeferredConst {
        let offset = self.compact_static_frame_offset(func_id, offset);
        if let Some((id, references)) = self.static_frame_addr_consts.get_mut(&(func_id, offset)) {
            *references += 1;
            return *id;
        }
        let id = self.asm.new_deferred_const();
        self.static_frame_addr_consts.insert((func_id, offset), (id, 1));
        id
    }

    /// Total emitted frame size of `func_id`, including its exact spill area.
    fn emitted_frame_size(&self, module: &Module, func_id: FunctionId) -> u64 {
        let func = &module.functions[func_id];
        let header = if self.runtime_stack_args && self.static_frame_functions.contains(func_id) {
            0
        } else {
            EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
        };
        let size = header
            + ((func.params.len() + func.returns.len()) as u64) * EvmMemoryLayout::WORD_SIZE
            + func.internal_frame_size
            + self.function_spill_size(func_id);
        if let Some(plan) = self.stack_return_plan(func_id)
            && plan.arity == 1
        {
            size - plan.arity as u64 * EvmMemoryLayout::WORD_SIZE
        } else {
            size
        }
    }

    /// Places every referenced static frame and resolves the address and
    /// free-memory-pointer constants recorded during this pass.
    ///
    /// Placement is an overlay: `base(f) = region_start + depth(f)`, where
    /// `depth(f)` is the longest chain of static frames that can be live below
    /// an activation of `f`. Depth propagates along every call edge — a static
    /// caller contributes its frame size, while an external entry whose locals
    /// live below the region only forwards its depth. Supported recursive Yul
    /// components occupy a disjoint prefix with one frame per function; their
    /// call edges are weight-zero because a nested activation reuses the same
    /// function frame after carrying its suspended state on the EVM stack.
    /// Every remaining cycle is therefore weight-zero and the relaxation
    /// converges. Functions that can never be simultaneously live end up
    /// sharing addresses; that is the point of the overlay.
    ///
    /// The heap floor moves up to `region_end`: each entry's free-pointer
    /// constant accounts for its exact spill area and every accepted static
    /// allocation, plus the overlaid helper region when one is referenced.
    fn resolve_static_frames(&mut self, module: &Module) {
        let uses_dynamic_internal_frames = !self.runtime_stack_args
            || module.functions.iter().any(|func| {
                func.instructions().any(|inst_id| {
                    matches!(
                        func.inst(inst_id).kind,
                        InstKind::ICall { function, .. }
                            if !self.static_frame_functions.contains(function)
                    )
                }) || func.blocks.iter().any(|block| {
                    // Dispatch and external-fusion tail calls never touch internal
                    // frames; only a selector-less callee outside the static set
                    // could imply dynamic frames (a shape `lower-evm-shaped` does
                    // not currently form).
                    matches!(
                        &block.terminator,
                        Some(Terminator::TailCall { function, .. })
                            if module.functions[*function].selector.is_none()
                                && !self.static_frame_functions.contains(*function)
                    )
                })
            });
        let low_memory_end = if uses_dynamic_internal_frames {
            EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT + EvmMemoryLayout::WORD_SIZE
        } else {
            EvmMemoryLayout::HEAP_START
        };
        let runtime_entries = std::mem::take(&mut self.runtime_entry_funcs);
        let reachable_memory_marks = runtime_entries
            .iter()
            .copied()
            .map(|entry| {
                let mark = self
                    .runtime_entry_reachability
                    .get(&entry)
                    .into_iter()
                    .flat_map(|reachable| reachable.iter())
                    .map(|func_id| {
                        Self::constant_memory_high_water_mark(&module.functions[func_id])
                    })
                    .max()
                    .unwrap_or_else(|| {
                        Self::constant_memory_high_water_mark(&module.functions[entry])
                    });
                (entry, mark)
            })
            .collect::<FxHashMap<_, _>>();
        let entry_bases: FxHashMap<FunctionId, u64> = runtime_entries
            .iter()
            .copied()
            .map(|func_id| {
                (
                    func_id,
                    Self::external_spill_base(
                        &module.functions[func_id],
                        uses_dynamic_internal_frames,
                        reachable_memory_marks[&func_id],
                    ),
                )
            })
            .collect();
        let mut entry_ends: FxHashMap<FunctionId, u64> = runtime_entries
            .iter()
            .copied()
            .map(|func_id| (func_id, entry_bases[&func_id] + self.function_spill_size(func_id)))
            .collect();

        // Longest live-chain depth below each function, over all call edges.
        // Only emitted callers count: an unemitted function (an internal
        // `.body` clone nobody calls, unreachable dead code) stacks no real
        // frame below its callees.
        let mut edges = Vec::new();
        for (func_id, func) in module.functions.iter_enumerated() {
            if !self.function_labels.contains_key(&func_id) {
                continue;
            }
            for inst_id in func.instructions() {
                if let InstKind::ICall { function, .. } = func.inst(inst_id).kind {
                    edges.push((func_id, function));
                }
            }
            for block in func.blocks.iter() {
                if let Some(Terminator::TailCall { function, .. }) = &block.terminator {
                    edges.push((func_id, *function));
                }
            }
        }
        let mut depth: FxHashMap<FunctionId, u64> = FxHashMap::default();
        for _ in 0..=module.functions.len() {
            let mut changed = false;
            for &(caller, callee) in &edges {
                let mut contribution = depth.get(&caller).copied().unwrap_or(0);
                if self.static_frame_functions.contains(caller)
                    && !self.recursive_frame_functions.contains(caller)
                {
                    contribution += self.emitted_frame_size(module, caller);
                }
                if contribution > depth.get(&callee).copied().unwrap_or(0) {
                    depth.insert(callee, contribution);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let placed: FxHashSet<FunctionId> =
            self.static_frame_addr_consts.keys().map(|&(func_id, _)| func_id).collect();
        let mut recursive_placed: Vec<_> = placed
            .iter()
            .copied()
            .filter(|&func_id| self.recursive_frame_functions.contains(func_id))
            .collect();
        recursive_placed.sort_unstable();
        let mut frame_relative = FxHashMap::default();
        let mut recursive_span = 0;
        for func_id in recursive_placed {
            frame_relative.insert(func_id, recursive_span);
            recursive_span += self.emitted_frame_size(module, func_id);
        }

        let mut static_span = recursive_span;
        for &func_id in &placed {
            let frame_size = self.emitted_frame_size(module, func_id);
            assert!(
                self.static_frame_addr_consts
                    .keys()
                    .filter(|&&(referenced, _)| referenced == func_id)
                    .all(|&(_, offset)| offset
                        .checked_add(EvmMemoryLayout::WORD_SIZE)
                        .is_some_and(|end| end <= frame_size)),
                "static frame reference exceeds emitted frame size for `{}`",
                module.functions[func_id].name
            );
            let relative = *frame_relative
                .entry(func_id)
                .or_insert_with(|| recursive_span + depth.get(&func_id).copied().unwrap_or(0));
            static_span = static_span.max(relative + frame_size);
        }

        let layout = |max_entry_end: u64| {
            if placed.is_empty() {
                (max_entry_end, max_entry_end)
            } else {
                let start = max_entry_end.max(low_memory_end);
                (start, start + static_span)
            }
        };
        let reachable_static_spans: FxHashMap<FunctionId, u64> = self
            .runtime_entry_reachability
            .iter()
            .map(|(&entry, reachable)| {
                let span = placed
                    .iter()
                    .copied()
                    .filter(|&func_id| reachable.contains(func_id))
                    .map(|func_id| {
                        frame_relative[&func_id] + self.emitted_frame_size(module, func_id)
                    })
                    .max()
                    .unwrap_or(0);
                (entry, span)
            })
            .collect();
        let heap_prefix_returns = Self::heap_prefix_return_offsets(module);
        let reachable_heap_prefix_guards: FxHashMap<FunctionId, u64> = self
            .runtime_entry_reachability
            .iter()
            .map(|(&entry, reachable)| {
                let guard = reachable
                    .iter()
                    .map(|func_id| {
                        Self::heap_prefix_guard(&module.functions[func_id], &heap_prefix_returns)
                    })
                    .max()
                    .unwrap_or(0);
                (entry, guard)
            })
            .collect();
        let free_memory_floor =
            |entry: FunctionId, entry_ends: &FxHashMap<FunctionId, u64>, region_start: u64| {
                let mut floor = entry_ends.get(&entry).copied().unwrap_or(low_memory_end);
                if let Some(&span) = reachable_static_spans.get(&entry)
                    && span != 0
                {
                    floor = floor.max(region_start + span);
                }
                if let Some(&guard) = reachable_heap_prefix_guards.get(&entry) {
                    floor = floor.checked_add(guard).expect("runtime heap prefix overflow");
                }
                floor.max(low_memory_end)
            };

        // Prefer eligible allocations before each entry's exact spill area,
        // then fall back to appending them after spills when only spill pushes
        // prevent the lower placement.
        // Entries overlay because only one runtime entry executes per call.
        // Reject any proposal that widens a shared heap/static-frame or
        // ranked-spill push.
        let mut static_alloc_sizes: FxHashMap<FunctionId, u64> = FxHashMap::default();
        let mut post_spill_entries = FxHashSet::default();
        for func_id in runtime_entries {
            let Some(allocations) = self.pending_static_allocs.remove(&func_id) else { continue };
            for (alloc, size) in allocations {
                let current_static_size = static_alloc_sizes.get(&func_id).copied().unwrap_or(0);
                let proposed_static_size = current_static_size + size;
                let current_end = entry_ends[&func_id];
                let proposed_end = current_end + size;
                let before_max = entry_ends.values().copied().max().unwrap_or(0);
                let after_max = entry_ends
                    .iter()
                    .map(|(&entry, &end)| if entry == func_id { proposed_end } else { end })
                    .max()
                    .unwrap_or(proposed_end);
                let (before_start, _) = layout(before_max);
                let (after_start, _) = layout(after_max);

                let mut addresses = Vec::with_capacity(self.static_frame_addr_consts.len() + 1);
                for &entry in self.runtime_free_memory_consts.keys() {
                    addresses.push(RelayoutAddress {
                        before: free_memory_floor(entry, &entry_ends, before_start),
                        after: free_memory_floor(entry, &entry_ends, after_start),
                        references: 1,
                    });
                }
                addresses.extend(self.static_frame_addr_consts.iter().map(
                    |(&(static_func, offset), &(_, references))| {
                        let relative = frame_relative[&static_func] + offset;
                        RelayoutAddress {
                            before: before_start + relative,
                            after: after_start + relative,
                            references,
                        }
                    },
                ));
                let global_width_neutral = preserves_push_width(addresses.iter().copied());
                let spills_width_neutral =
                    self.external_spill_addr_consts.get(&func_id).is_none_or(|spills| {
                        let base = entry_bases[&func_id];
                        preserves_push_width(spills.iter().enumerate().map(
                            |(rank, &(_, references))| {
                                let offset = rank as u64 * WORD_BYTES as u64;
                                RelayoutAddress {
                                    before: base + current_static_size + offset,
                                    after: base + proposed_static_size + offset,
                                    references,
                                }
                            },
                        ))
                    });

                if global_width_neutral
                    && spills_width_neutral
                    && !post_spill_entries.contains(&func_id)
                {
                    let static_address = entry_bases[&func_id] + current_static_size;
                    self.asm.set_deferred_alloc_static(alloc, U256::from(static_address));
                    entry_ends.insert(func_id, proposed_end);
                    static_alloc_sizes.insert(func_id, proposed_static_size);
                } else if global_width_neutral {
                    // If inserting before spills would widen one of their
                    // pushes, append after the exact spill area instead. Once
                    // an entry uses this suffix, later allocations must stay
                    // there so already-emitted static addresses never move.
                    self.asm.set_deferred_alloc_static(alloc, U256::from(current_end));
                    entry_ends.insert(func_id, proposed_end);
                    post_spill_entries.insert(func_id);
                } else {
                    self.asm.set_deferred_alloc_dynamic(alloc, U256::from(size));
                }
            }
        }

        // A retained candidate should always belong to an emitted external
        // entry. Lower defensively to the dynamic form if an unusual pipeline
        // shape leaves one behind.
        for (_, allocations) in self.pending_static_allocs.drain() {
            for (alloc, size) in allocations {
                self.asm.set_deferred_alloc_dynamic(alloc, U256::from(size));
            }
        }

        for (func_id, spills) in self.external_spill_addr_consts.drain() {
            let base =
                entry_bases[&func_id] + static_alloc_sizes.get(&func_id).copied().unwrap_or(0);
            for (rank, (id, _)) in spills.into_iter().enumerate() {
                self.asm.set_deferred_const(id, U256::from(base + rank as u64 * WORD_BYTES as u64));
            }
        }

        let max_entry_end = entry_ends.values().copied().max().unwrap_or(0);
        let (region_start, _) = layout(max_entry_end);
        for (&(func_id, offset), &(id, _)) in &self.static_frame_addr_consts {
            let relative = frame_relative[&func_id] + offset;
            self.asm.set_deferred_const(id, U256::from(region_start + relative));
        }
        let free_memory_floors: FxHashMap<FunctionId, u64> = self
            .runtime_free_memory_consts
            .keys()
            .copied()
            .map(|entry| (entry, free_memory_floor(entry, &entry_ends, region_start)))
            .collect();
        for (entry, id) in self.runtime_free_memory_consts.drain() {
            let floor = free_memory_floors[&entry];
            self.asm.set_deferred_const(id, U256::from(floor));
        }
        self.runtime_entry_reachability.clear();
    }

    fn external_spill_base(
        func: &Function,
        dynamic_frames_enabled: bool,
        reachable_memory_mark: u64,
    ) -> u64 {
        let low_memory_start = if dynamic_frames_enabled && Self::uses_internal_frame_slot(func) {
            EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT + EvmMemoryLayout::WORD_SIZE
        } else {
            EvmMemoryLayout::HEAP_START
        };
        let base =
            low_memory_start + func.internal_frame_size.max(func.external_static_return_size);
        // Hand-written assembly may own low memory above the compiler's own
        // frame through constant addresses; spill only above everything it
        // names, so a reload never reads a byte of the user's image and a
        // store never lands inside it.
        let mark = Self::constant_memory_high_water_mark(func).max(reachable_memory_mark);
        base.max(mark.next_multiple_of(EvmMemoryLayout::WORD_SIZE))
    }

    /// Returns the working-memory prefix a hand-written heap image needs.
    ///
    /// Static frames end where the runtime heap begins. Creation-code builders
    /// such as CWIA intentionally save, write, and restore words immediately
    /// before a `bytes` object, then consume that prefix with `create2` or
    /// `keccak256`. Reserve the largest constant backward offset for entries
    /// that reach such a builder so its temporary image cannot overlap the
    /// highest static-frame spill slots.
    fn heap_prefix_guard(func: &Function, returned_offsets: &FxHashMap<FunctionId, u64>) -> u64 {
        func.instructions()
            .filter_map(|inst_id| {
                let offset = match func.inst(inst_id).kind {
                    InstKind::Keccak256(offset, _)
                    | InstKind::Create(_, offset, _)
                    | InstKind::Create2(_, offset, _, _)
                    | InstKind::Call { args_offset: offset, .. }
                    | InstKind::CallCode { args_offset: offset, .. }
                    | InstKind::StaticCall { args_offset: offset, .. }
                    | InstKind::DelegateCall { args_offset: offset, .. } => Some(offset),
                    _ => None,
                }?;
                let mut visiting = DenseBitSet::new_empty(func.num_values());
                let mut memo = FxHashMap::default();
                Self::heap_prefix_offset(func, offset, returned_offsets, &mut visiting, &mut memo)
            })
            .max()
            .unwrap_or(0)
            .next_multiple_of(EvmMemoryLayout::WORD_SIZE)
    }

    /// Computes the largest backward heap offset returned by each helper.
    fn heap_prefix_return_offsets(module: &Module) -> FxHashMap<FunctionId, u64> {
        let mut offsets = FxHashMap::default();
        for _ in 0..module.functions.len() {
            let mut changed = false;
            for (func_id, func) in module.functions.iter_enumerated() {
                let mut offset = offsets.get(&func_id).copied().unwrap_or(0);
                for block in &func.blocks {
                    let Some(Terminator::Return { values }) = &block.terminator else { continue };
                    for &value in values {
                        let mut visiting = DenseBitSet::new_empty(func.num_values());
                        let mut memo = FxHashMap::default();
                        if let Some(returned) = Self::heap_prefix_offset(
                            func,
                            value,
                            &offsets,
                            &mut visiting,
                            &mut memo,
                        ) {
                            offset = offset.max(returned);
                        }
                    }
                }
                if offset > offsets.get(&func_id).copied().unwrap_or(0) {
                    offsets.insert(func_id, offset);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        offsets
    }

    /// Returns how far `value` can point before its underlying heap object.
    fn heap_prefix_offset(
        func: &Function,
        value: ValueId,
        returned_offsets: &FxHashMap<FunctionId, u64>,
        visiting: &mut DenseBitSet<ValueId>,
        memo: &mut FxHashMap<ValueId, u64>,
    ) -> Option<u64> {
        if let Some(&offset) = memo.get(&value) {
            return Some(offset);
        }
        if !visiting.insert(value) {
            return None;
        }
        let derive = |value, visiting: &mut DenseBitSet<ValueId>, memo: &mut FxHashMap<_, _>| {
            Self::heap_prefix_offset(func, value, returned_offsets, visiting, memo)
        };
        let offset = match func.value(value) {
            Value::Arg(_) if func.value_ty(value).is_some_and(MirType::is_memory_reference) => {
                Some(0)
            }
            Value::Inst(inst_id) => match &func.inst(*inst_id).kind {
                InstKind::Fmp | InstKind::Alloc { .. } => Some(0),
                InstKind::MLoad(address)
                    if func.value_u64(*address) == Some(EvmMemoryLayout::FMP_SLOT) =>
                {
                    Some(0)
                }
                InstKind::ICall { function, returns: 1, .. } => {
                    returned_offsets.get(function).copied()
                }
                InstKind::Sub(base, amount) => {
                    let base = derive(*base, visiting, memo).or_else(|| {
                        func.value_ty(*base).is_some_and(MirType::is_memory_reference).then_some(0)
                    })?;
                    base.checked_add(func.value_u64(*amount)?)
                }
                InstKind::Phi(incoming) => incoming
                    .iter()
                    .map(|&(_, incoming)| derive(incoming, visiting, memo))
                    .collect::<Option<Vec<_>>>()?
                    .into_iter()
                    .max(),
                InstKind::Select(_, then_value, else_value) => Some(
                    derive(*then_value, visiting, memo)?.max(derive(*else_value, visiting, memo)?),
                ),
                _ if func.value_ty(value).is_some_and(MirType::is_memory_reference) => Some(0),
                _ => None,
            },
            _ => None,
        };
        visiting.remove(value);
        if let Some(offset) = offset {
            memo.insert(value, offset);
        }
        offset
    }

    /// Returns the highest end address of any memory access in `func` whose
    /// offset and size are both compile-time constants, or zero without one.
    ///
    /// A routine that assembles an image at fixed low addresses legally uses
    /// the memory the spill area would otherwise occupy: the ERC-6551 registry
    /// lays its proxy initcode out at `[0x55, 0x10c)` with
    /// `calldatacopy(0x8c, 0x24, 0x80)` and reads it back through
    /// `create2(0, 0x55, 0xb7, salt)`, so a spill slot at `0xa0` ends up in
    /// the deployed footer. Reads count as well as writes. The compiler's own
    /// absolute accesses (the external return buffer, frame locals) never
    /// exceed the base they are placed under, so they never raise it. Ranges
    /// starting at or above `SPILL_HAZARD_BOUND` above `HEAP_START` are not low
    /// memory and are ignored. A range that starts below the bound still owns
    /// its complete extent, even when its end lies above the bound.
    fn constant_memory_high_water_mark(func: &Function) -> u64 {
        let bound = EvmMemoryLayout::HEAP_START + SPILL_HAZARD_BOUND;
        let end_of = |offset: ValueId, size: u64| -> Option<u64> {
            let start = func.value_u64(offset)?;
            let end = start.checked_add(size)?;
            (start < bound).then_some(end)
        };
        let sized_end = |offset: ValueId, size: ValueId| end_of(offset, func.value_u64(size)?);
        let mut mark = 0;
        for inst_id in func.instructions() {
            let end = match func.inst(inst_id).kind {
                InstKind::MLoad(addr) | InstKind::MStore(addr, _) => {
                    end_of(addr, EvmMemoryLayout::WORD_SIZE)
                }
                InstKind::MStore8(addr, _) => end_of(addr, 1),
                InstKind::MCopy(dest, src, size) => sized_end(dest, size).max(sized_end(src, size)),
                InstKind::CalldataCopy(dest, _, size)
                | InstKind::DataCopy(_, dest, size)
                | InstKind::CodeCopy(dest, _, size)
                | InstKind::ReturnDataCopy(dest, _, size)
                | InstKind::ExtCodeCopy(_, dest, _, size)
                | InstKind::Keccak256(dest, size)
                | InstKind::Log0(dest, size)
                | InstKind::Log1(dest, size, _)
                | InstKind::Log2(dest, size, _, _)
                | InstKind::Log3(dest, size, _, _, _)
                | InstKind::Log4(dest, size, _, _, _, _)
                | InstKind::Create(_, dest, size)
                | InstKind::Create2(_, dest, size, _) => sized_end(dest, size),
                InstKind::Call { args_offset, args_size, ret_offset, ret_size, .. }
                | InstKind::CallCode { args_offset, args_size, ret_offset, ret_size, .. }
                | InstKind::StaticCall { args_offset, args_size, ret_offset, ret_size, .. }
                | InstKind::DelegateCall { args_offset, args_size, ret_offset, ret_size, .. } => {
                    sized_end(args_offset, args_size).max(sized_end(ret_offset, ret_size))
                }
                _ => None,
            };
            mark = mark.max(end.unwrap_or(0));
        }
        for block in func.blocks.iter() {
            if let Some(
                Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size },
            ) = &block.terminator
            {
                mark = mark.max(sized_end(*offset, *size).unwrap_or(0));
            }
        }
        mark
    }

    fn constructor_spill_base(&self, immutable_count: usize) -> u64 {
        immutable_staging_end(self.immutable_staging_base, immutable_count)
    }

    fn constructor_fixed_memory_end(&self, immutable_count: usize, spill_size: u64) -> u64 {
        self.constructor_spill_base(immutable_count)
            .checked_add(spill_size)
            .expect("constructor spill area overflow")
    }

    fn uses_internal_frame_slot(func: &Function) -> bool {
        func.instructions().any(|inst_id| matches!(func.inst(inst_id).kind, InstKind::ICall { .. }))
    }

    fn emit_entry_free_memory_start(
        &mut self,
        module: &Module,
        call_graph: &CallGraphInfo,
        entry: FunctionId,
    ) {
        let mut reachable = call_graph.reachable_callees_from([entry]);
        reachable.insert(entry);
        self.runtime_entry_reachability.insert(entry, reachable.clone());
        let needs_free_memory = reachable.iter().any(|func_id| {
            call_graph.is_recursive(func_id)
                || Self::function_may_observe_free_memory_slot(&module.functions[func_id])
                || module.functions[func_id].instructions().any(|inst_id| {
                    matches!(
                        module.functions[func_id].inst(inst_id).kind,
                        InstKind::ICall { function, returns, .. }
                            if returns > 1 || !self.static_frame_functions.contains(function)
                    )
                })
        });
        if !needs_free_memory {
            return;
        }

        let id = self.asm.new_deferred_const();
        self.asm.emit_push_deferred(id);
        self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
        self.asm.emit_op(op::MSTORE);
        self.runtime_free_memory_consts.insert(entry, id);
    }

    /// Returns the destination of a symbolic memory write that can cover a
    /// compiler spill slot. Constant ranges are handled by the static memory
    /// high-water mark instead.
    fn dynamic_spill_write_dest(func: &Function, inst_id: InstId) -> Option<ValueId> {
        if matches!(
            func.inst(inst_id).metadata.memory_region(),
            Some(MemoryRegion::AbiReturn | MemoryRegion::Heap | MemoryRegion::InternalFrame)
        ) {
            return None;
        }
        let dynamic_range = |dest, size| {
            // Fixed-width copies have the same explicit-memory contract as
            // `mstore`: arbitrary destinations in hand-written assembly may
            // alias compiler memory. The forwarding-buffer protocol is for
            // variable-length writes that can sweep over every spill slot.
            if func.value_u64(size).is_some() {
                return None;
            }
            let below_spills = func.value_u64(dest).is_some_and(|dest| {
                let mut visiting = DenseBitSet::new_empty(func.num_values());
                Self::value_u64_upper_bound(func, size, &mut visiting)
                    .and_then(|size| dest.checked_add(size))
                    .is_some_and(|end| end <= EvmMemoryLayout::HEAP_START)
            });
            (!below_spills).then_some(dest)
        };
        match func.inst(inst_id).kind {
            InstKind::CalldataCopy(dest, _, size)
            | InstKind::DataCopy(_, dest, size)
            | InstKind::CodeCopy(dest, _, size)
            | InstKind::ExtCodeCopy(_, dest, _, size)
            | InstKind::MCopy(dest, _, size) => dynamic_range(dest, size),
            InstKind::ReturnDataCopy(dest, offset, size) => {
                // `returndatacopy(_, returndatasize(), n)` is an OOG guard:
                // `n == 0` writes nothing, while every nonzero length is out
                // of bounds and traps before memory is modified.
                let starts_at_end = matches!(
                    func.value(offset),
                    Value::Inst(inst) if matches!(func.inst(*inst).kind, InstKind::ReturnDataSize)
                );
                if starts_at_end { None } else { dynamic_range(dest, size) }
            }
            InstKind::Call { ret_offset: dest, ret_size: size, .. }
            | InstKind::CallCode { ret_offset: dest, ret_size: size, .. }
            | InstKind::StaticCall { ret_offset: dest, ret_size: size, .. }
            | InstKind::DelegateCall { ret_offset: dest, ret_size: size, .. }
                if func.value_u64(size) != Some(0) =>
            {
                dynamic_range(dest, size)
            }
            _ => None,
        }
    }

    /// Returns a conservative upper bound for a small integer expression.
    fn value_u64_upper_bound(
        func: &Function,
        value: ValueId,
        visiting: &mut DenseBitSet<ValueId>,
    ) -> Option<u64> {
        if let Some(value) = func.value_u64(value) {
            return Some(value);
        }
        if !visiting.insert(value) {
            return None;
        }
        let bound = match func.value(value) {
            Value::Inst(inst_id) => match func.inst(*inst_id).kind {
                InstKind::Select(_, if_true, if_false) => Some(
                    Self::value_u64_upper_bound(func, if_true, visiting)?
                        .max(Self::value_u64_upper_bound(func, if_false, visiting)?),
                ),
                _ => None,
            },
            Value::Arg(_) | Value::Immediate(_) | Value::Undef(_) | Value::Error(_) => None,
        };
        visiting.remove(value);
        bound
    }

    /// Collects symbolic low-memory clobbers that can overwrite the spill
    /// area. This includes variable-length copy opcodes and call return
    /// buffers. Alias analysis excludes destinations rooted at the free-memory
    /// pointer, allocations, or internal frames. Single-word stores do not
    /// need the forwarding-buffer protocol: their exact runtime address does
    /// not create an unbounded clobber range.
    fn compute_spill_hazard_insts(&self, func: &Function) -> FxHashSet<InstId> {
        let mut hazards = FxHashSet::default();
        let candidates = func
            .instructions()
            .filter_map(|inst_id| {
                Self::dynamic_spill_write_dest(func, inst_id).map(|dest| (inst_id, dest))
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return hazards;
        }
        let aa = AliasAnalysis::new(func);
        for (inst_id, dest) in candidates {
            if self.write_dest_may_reach_spills(func, &aa, dest) {
                hazards.insert(inst_id);
            }
        }
        hazards
    }

    /// Whether a dynamic-length write's destination may overlap the spill area.
    /// A free-memory-pointer, allocation, or internal-frame destination stays
    /// in compiler-owned high memory; a symbolic low base (raw
    /// `returndatasize()` addressing) or a low absolute base can reach the
    /// fixed low-memory spill slots.
    fn write_dest_may_reach_spills(
        &self,
        func: &Function,
        aa: &AliasAnalysis,
        dest: ValueId,
    ) -> bool {
        let Some(address) = aa.memory_address(func, dest) else {
            return true;
        };
        if matches!(address.region, MemoryRegion::Heap | MemoryRegion::InternalFrame) {
            return false;
        }
        match address.base {
            MemoryBase::Allocation(_)
            | MemoryBase::DynamicAllocation(_)
            | MemoryBase::InternalFrame => false,
            MemoryBase::Absolute => {
                address.offset < EvmMemoryLayout::HEAP_START.saturating_add(SPILL_HAZARD_BOUND)
            }
            MemoryBase::Value(value) => {
                let mut visiting = DenseBitSet::new_empty(func.num_values());
                let mut memo = FxHashMap::default();
                self.heap_pointer_provenance(func, aa, value, &mut visiting, &mut memo)
                    != Some(true)
            }
        }
    }

    /// Finds leaf helpers that return a pointer rooted at the free-memory pointer.
    /// Calls through these helpers lose alias provenance in MIR, so remember the
    /// narrow interprocedural fact needed by forwarding-buffer hazard analysis.
    fn collect_heap_pointer_return_functions(module: &Module) -> DenseBitSet<FunctionId> {
        let mut functions = DenseBitSet::new_empty(module.functions.len());
        let no_helpers = DenseBitSet::new_empty(module.functions.len());
        for (func_id, func) in module.functions.iter_enumerated() {
            if func.instructions().any(|inst_id| {
                matches!(func.inst(inst_id).kind, InstKind::ICall { .. } | InstKind::SetFmp(_))
                    || matches!(
                        func.inst(inst_id).kind,
                        InstKind::MStore(address, _)
                            if func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT)
                    )
            }) {
                continue;
            }

            let aa = AliasAnalysis::new(func);
            let mut saw_return = false;
            let mut valid = true;
            for block in &func.blocks {
                let Some(Terminator::Return { values }) = &block.terminator else { continue };
                saw_return = true;
                if values.len() != 1 {
                    valid = false;
                    break;
                }
                let mut visiting = DenseBitSet::new_empty(func.num_values());
                let mut memo = FxHashMap::default();
                if Self::heap_pointer_provenance_with_helpers(
                    func,
                    &aa,
                    values[0],
                    &no_helpers,
                    &mut visiting,
                    &mut memo,
                ) != Some(true)
                {
                    valid = false;
                    break;
                }
            }
            if saw_return && valid {
                functions.insert(func_id);
            }
        }
        functions
    }

    /// Returns `Some(grounded)` for a heap-pointer derivation. Recursive phi
    /// edges are provisionally valid but ungrounded; every accepted cycle must
    /// also contain a concrete FMP, allocation, or qualified-helper origin.
    fn heap_pointer_provenance(
        &self,
        func: &Function,
        aa: &AliasAnalysis,
        value: ValueId,
        visiting: &mut DenseBitSet<ValueId>,
        memo: &mut FxHashMap<ValueId, bool>,
    ) -> Option<bool> {
        Self::heap_pointer_provenance_with_helpers(
            func,
            aa,
            value,
            &self.heap_pointer_return_functions,
            visiting,
            memo,
        )
    }

    fn heap_pointer_provenance_with_helpers(
        func: &Function,
        aa: &AliasAnalysis,
        value: ValueId,
        helper_returns: &DenseBitSet<FunctionId>,
        visiting: &mut DenseBitSet<ValueId>,
        memo: &mut FxHashMap<ValueId, bool>,
    ) -> Option<bool> {
        if let Some(&grounded) = memo.get(&value) {
            return Some(grounded);
        }
        if !visiting.insert(value) {
            return Some(false);
        }

        let aligned_mask = |value: ValueId| {
            func.value_u256(value).is_some_and(|mask| {
                mask == U256::MAX - U256::from(31)
                    || mask == U256::from(u64::MAX.saturating_sub(31))
            })
        };
        let derive = |value, visiting: &mut DenseBitSet<ValueId>, memo: &mut FxHashMap<_, _>| {
            Self::heap_pointer_provenance_with_helpers(
                func,
                aa,
                value,
                helper_returns,
                visiting,
                memo,
            )
        };

        let provenance = aa
            .memory_address(func, value)
            .and_then(|address| matches!(address.region, MemoryRegion::Heap).then_some(true))
            .or_else(|| {
                if matches!(func.value(value), Value::Arg(_))
                    && func.value_ty(value).is_some_and(MirType::is_memory_reference)
                {
                    return Some(true);
                }
                let Value::Inst(inst_id) = func.value(value) else { return None };
                match &func.inst(*inst_id).kind {
                    InstKind::Fmp | InstKind::Alloc { .. } => Some(true),
                    InstKind::MLoad(address)
                        if func.value_u64(*address) == Some(EvmMemoryLayout::FMP_SLOT) =>
                    {
                        Some(true)
                    }
                    InstKind::ICall { function, returns: 1, .. }
                        if helper_returns.contains(*function) =>
                    {
                        Some(true)
                    }
                    InstKind::Add(first, second) => {
                        derive(*first, visiting, memo).or_else(|| derive(*second, visiting, memo))
                    }
                    InstKind::Sub(base, _) => derive(*base, visiting, memo),
                    InstKind::And(first, second) if aligned_mask(*second) => {
                        derive(*first, visiting, memo)
                    }
                    InstKind::And(first, second) if aligned_mask(*first) => {
                        derive(*second, visiting, memo)
                    }
                    InstKind::MemoryObjectData(object, _)
                    | InstKind::MemoryObjectFieldAddr { object, .. }
                    | InstKind::MemoryObjectElementAddr { object, .. } => {
                        derive(*object, visiting, memo)
                    }
                    InstKind::Phi(incoming) => {
                        let mut grounded = false;
                        for &(_, incoming) in incoming {
                            grounded |= derive(incoming, visiting, memo)?;
                        }
                        Some(grounded)
                    }
                    InstKind::Select(_, then_value, else_value) => Some(
                        derive(*then_value, visiting, memo)? | derive(*else_value, visiting, memo)?,
                    ),
                    _ => None,
                }
            });
        visiting.remove(value);
        if let Some(grounded) = provenance {
            memo.insert(value, grounded);
        }
        provenance
    }

    /// Returns whether a function can read, write, or observe the reserved free-memory-pointer
    /// word. Unknown offsets and lengths are conservatively overlapping; constant ranges proven
    /// disjoint from `[0x40, 0x60)` keep the lazy entry initialization optimization.
    fn function_may_observe_free_memory_slot(func: &Function) -> bool {
        let overlaps = |offset, size| {
            Self::constant_memory_range_may_overlap_fmp(
                func.value_u64(offset),
                func.value_u64(size),
            )
        };
        let overlaps_const = |offset, size| {
            Self::constant_memory_range_may_overlap_fmp(func.value_u64(offset), Some(size))
        };
        if func.instructions().any(|inst_id| match &func.inst(inst_id).kind {
            InstKind::MLoad(offset) | InstKind::MStore(offset, _) => {
                overlaps_const(*offset, EvmMemoryLayout::WORD_SIZE)
            }
            InstKind::MStore8(offset, _) => overlaps_const(*offset, 1),
            InstKind::MemoryZero(offset, size)
            | InstKind::Keccak256(offset, size)
            | InstKind::CalldataCopy(offset, _, size)
            | InstKind::DataCopy(_, offset, size)
            | InstKind::CodeCopy(offset, _, size)
            | InstKind::ReturnDataCopy(offset, _, size)
            | InstKind::ExtCodeCopy(_, offset, _, size) => overlaps(*offset, *size),
            InstKind::MCopy(dest, src, size) => overlaps(*dest, *size) || overlaps(*src, *size),
            InstKind::Call { args_offset, args_size, ret_offset, ret_size, .. }
            | InstKind::CallCode { args_offset, args_size, ret_offset, ret_size, .. }
            | InstKind::StaticCall { args_offset, args_size, ret_offset, ret_size, .. }
            | InstKind::DelegateCall { args_offset, args_size, ret_offset, ret_size, .. } => {
                overlaps(*args_offset, *args_size) || overlaps(*ret_offset, *ret_size)
            }
            InstKind::Create(_, offset, size) | InstKind::Create2(_, offset, size, _) => {
                overlaps(*offset, *size)
            }
            InstKind::Log0(offset, size)
            | InstKind::Log1(offset, size, _)
            | InstKind::Log2(offset, size, _, _)
            | InstKind::Log3(offset, size, _, _, _)
            | InstKind::Log4(offset, size, _, _, _, _) => overlaps(*offset, *size),
            InstKind::MSize | InstKind::Fmp | InstKind::SetFmp(_) | InstKind::Alloc { .. } => true,
            // These semantic memory operations are normally gone by the `evm-shaped` phase. If
            // one remains, its complete accessed range is not represented as physical operands
            // here, so retain the Solidity memory invariant conservatively.
            InstKind::MemoryObjectLen(_, _)
            | InstKind::SetMemoryObjectLen(_, _, _)
            | InstKind::MemoryObjectData(_, _)
            | InstKind::MemoryObjectFieldAddr { .. }
            | InstKind::MemoryObjectElementAddr { .. }
            | InstKind::AbiEncode { .. }
            | InstKind::StorageToMemory { .. }
            | InstKind::MemoryToStorage { .. }
            | InstKind::Keccak256Bytes(_)
            | InstKind::MappingSlotMemory(_, _) => true,
            _ => false,
        }) {
            return true;
        }

        func.blocks.iter().any(|block| match block.terminator.as_ref() {
            Some(Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size }) => {
                overlaps(*offset, *size)
            }
            _ => false,
        })
    }

    fn constant_memory_range_may_overlap_fmp(offset: Option<u64>, size: Option<u64>) -> bool {
        if size == Some(0) {
            return false;
        }
        let Some(offset) = offset else { return true };
        let start = EvmMemoryLayout::FMP_SLOT;
        let end = start + EvmMemoryLayout::WORD_SIZE;
        if offset >= end {
            return false;
        }
        let Some(size) = size else { return true };
        offset.checked_add(size).is_none_or(|range_end| range_end > start)
    }

    fn emit_spill_slot_addr(&mut self, func: &Function, slot: SpillSlot) {
        if self.in_internal_function {
            self.emit_own_frame_addr(self.internal_spill_slot_offset(func, slot));
        } else {
            self.emit_spill_slot_addr_untracked(func, slot);
        }
    }

    fn emit_spill_slot_addr_untracked(&mut self, func: &Function, slot: SpillSlot) {
        if self.in_internal_function {
            self.emit_own_frame_addr_untracked(self.internal_spill_slot_offset(func, slot));
        } else if self.in_constructor {
            let spill_addr = self.constructor_spill_base(self.immutable_encodings.len())
                + u64::from(slot.offset) * EvmMemoryLayout::WORD_SIZE;
            self.asm.emit_push(U256::from(spill_addr));
        } else {
            // Route the address through a deferred constant and count the
            // reference; `assign_ranked_spill_addrs` renumbers the body's
            // slots hottest-first when it completes.
            let key = u64::from(slot.offset);
            let id = if let Some(entry) = self.spill_addr_consts.get_mut(&key) {
                entry.1 += 1;
                entry.0
            } else {
                let id = self.asm.new_deferred_const();
                self.spill_addr_consts.insert(key, (id, 1));
                id
            };
            self.asm.emit_push_deferred(id);
        }
    }

    fn emit_spill_load(&mut self, func: &Function, slot: SpillSlot) {
        let (block, index) = self.asm.next_instruction_position();
        self.spill_loads.push((slot, block, index));
        self.emit_spill_slot_addr_untracked(func, slot);
        self.asm.emit_op(op::MLOAD);
    }

    fn internal_spill_slot_offset(&self, func: &Function, slot: SpillSlot) -> u64 {
        EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
            + (func.params.len() as u64) * EvmMemoryLayout::WORD_SIZE
            + (func.returns.len() as u64) * EvmMemoryLayout::WORD_SIZE
            + func.internal_frame_size
            + u64::from(slot.offset) * EvmMemoryLayout::WORD_SIZE
    }

    /// Ranks the external body's spill slots by reference count, hottest
    /// first, so the most reloaded slots receive the shortest addresses after
    /// final layout. The ranking is a bijection over the same slot area —
    /// every site of a slot goes through one deferred constant — so sizes and
    /// disjointness are unchanged.
    fn assign_ranked_spill_addrs(&mut self, func_id: FunctionId) {
        if self.spill_addr_consts.is_empty() {
            return;
        }
        let mut slots: Vec<(u64, (DeferredConst, usize))> =
            self.spill_addr_consts.drain().collect();
        slots.sort_unstable_by(|a, b| b.1.1.cmp(&a.1.1).then(a.0.cmp(&b.0)));
        self.external_spill_addr_consts
            .insert(func_id, slots.into_iter().map(|(_, deferred)| deferred).collect());
    }

    fn emit_internal_arg_load(&mut self, index: ArgIdx) {
        self.emit_own_frame_addr_untracked(
            EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                + (index.index() as u64) * EvmMemoryLayout::WORD_SIZE,
        );
        self.asm.emit_op(op::MLOAD);
    }

    /// Returns the first internal-call result only when it is consumed. The call itself remains
    /// effectful, and additional returns are staged separately in the multi-return buffer.
    fn live_icall_result(
        result: Option<ValueId>,
        returns: usize,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) -> Option<ValueId> {
        result.filter(|&result| returns > 0 && !liveness.is_dead_after(result, block, inst_idx))
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_icall(
        &mut self,
        func_id: FunctionId,
        func: &Function,
        callee: FunctionId,
        args: &[ValueId],
        returns: usize,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        let Some(&callee_label) = self.function_labels.get(&callee) else {
            return;
        };
        let return_label = self.asm.new_label();

        // A static-frame callee needs none of the frame-pointer or
        // free-pointer bookkeeping below: its addresses are compile-time
        // constants.
        if self.static_frame_functions.contains(callee) {
            let (preserved_words, argument_words) = self.emit_icall_static(
                func_id,
                func,
                callee,
                callee_label,
                return_label,
                args,
                returns,
                result,
                liveness,
                block,
                inst_idx,
            );
            self.icall_stack_edges.push(ICallStackEdge {
                caller: func_id,
                callee,
                preserved_words,
                argument_words,
            });
            return;
        }

        let resident_call_values: Vec<_> = if self.preserve_caller_stack {
            self.resident_stack_args(func_id)
                .into_iter()
                .flatten()
                .copied()
                .filter(|&value| {
                    self.scheduler.is_stack_only_value(value)
                        && liveness.is_used_at_or_after(value, block, inst_idx + 1)
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        // Frame layout: [reserved][saved frame ptr][args][returns][locals][spills].
        // The first slot is reserved (the return address used to live there;
        // it now travels on the EVM stack) so downstream offsets stay stable.
        // The spill suffix is only known after the callee body has emitted.
        let frame_size = self.asm.new_deferred_const();
        self.pending_frame_size_consts.push((frame_size, callee));

        // Spill values that are live after this call BEFORE consuming the
        // arguments. An argument that is also used later (e.g. a flag passed to
        // a helper and then stored, as in `tryAdd`) would otherwise be popped by
        // the arg-store loop below and then lost when the stack is cleared for
        // the call, leaving it unavailable at its later use.
        self.spill_live_stack_values(func_id, func, liveness, block, inst_idx);

        // The dynamic-frame base is an anonymous word kept on the physical stack while arguments
        // are stored. Give any argument that this extra word would bury beyond `DUP16` a memory
        // route first. Deep-spill recovery can move named MIR values out of the way, but it cannot
        // save an anonymous frame-base word after that word has already been pushed.
        self.materialize_deep_dynamic_call_args(func, args);

        self.emit_new_internal_frame_base_tracked();

        // The second frame word stores the previous frame pointer.
        self.asm.emit_push(U256::from(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT));
        self.asm.emit_op(op::MLOAD);
        self.scheduler.stack.push_unknown();
        self.emit_internal_frame_store_from_top_preserving_base(WORD_BYTES as u64);

        for (i, &arg) in args.iter().enumerate() {
            self.emit_operand(func, arg);
            self.emit_internal_frame_store_from_top_preserving_base(
                EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                    + (i as u64) * EvmMemoryLayout::WORD_SIZE,
            );
        }

        // current_frame = frame
        self.emit_store_frame_base_to_current_frame_slot();

        // free_ptr += frame_size
        self.emit_store_new_free_pointer_from_frame_base(frame_size);

        // Resident arguments have no frame home to reload after the nested
        // call. Keep their canonical prefix below the return address just as
        // the static-frame call path does. The callee's scheduler models only
        // words above this hidden prefix, and the whole-program stack-depth
        // validation accounts for the preserved words after emission.
        let caller_stack = if resident_call_values.is_empty() {
            None
        } else {
            self.pop_stack_values_not_needed_by(&resident_call_values);
            let target =
                resident_call_values.iter().copied().map(TargetSlot::Value).collect::<Vec<_>>();
            let shuffle = self.scheduler.shuffle_to_layout(&target).unwrap_or_else(|| {
                panic!(
                    "could not preserve resident arguments across a dynamic internal call in \
                     `{}`: stack={:?}, target={target:?}",
                    func.name, self.scheduler.stack
                )
            });
            for op in shuffle.ops {
                self.asm.emit_stack_op(op);
            }
            Some(self.scheduler.stack.clone())
        };
        let preserved_words = caller_stack.as_ref().map_or(0, StackModel::depth);
        self.icall_stack_edges.push(ICallStackEdge {
            caller: func_id,
            callee,
            preserved_words,
            argument_words: 0,
        });
        if caller_stack.is_none() {
            self.pop_all_stack_values();
        }
        self.scheduler.clear_stack();

        // The return address travels on the EVM stack, not in the frame: it is
        // pushed after the caller's stack is fully drained, so it is the only
        // physical value below the callee's execution. It is deliberately not
        // tracked by the scheduler — the model only describes the region above
        // it and every emitted DUP/SWAP/POP is model-relative, so nothing in
        // the callee can reach it. The callee's return consumes it with a bare
        // JUMP, and a tail call within the callee forwards it untouched.
        self.emit_push_label(return_label);

        self.emit_push_label(callee_label);
        self.asm.emit_op(op::JUMP);

        self.asm.define_label(return_label);
        if let Some(caller_stack) = caller_stack {
            self.scheduler.stack = caller_stack;
        } else {
            self.scheduler.clear_stack();
        }

        let live_result = Self::live_icall_result(result, returns, liveness, block, inst_idx);
        if let Some(result) = live_result {
            self.emit_current_internal_frame_addr(
                EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                    + (args.len() as u64) * EvmMemoryLayout::WORD_SIZE,
            );
            self.asm.emit_op(op::MLOAD);
            self.scheduler.stack.push(result);
            if returns <= 1 {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, result);
            }
        }

        // Publish the callee's return area directly as the multi-return buffer.
        // MIR consumes every tail result immediately after the call, while the
        // callee frame is still intact. Copying those words to the unbumped
        // free-memory pointer can overwrite user assembly that is constructing
        // an object there.
        if returns > 1 {
            self.emit_current_internal_frame_addr(
                EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                    + (args.len() as u64) * EvmMemoryLayout::WORD_SIZE,
            );
            self.asm.emit_push(U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT));
            self.asm.emit_op(op::MSTORE);
        }

        // Deallocate the callee frame in strict LIFO order by restoring the
        // free memory pointer to the callee frame base. This must happen before
        // restoring the caller frame pointer because `emit_current_internal_frame_addr`
        // reads the internal-frame pointer slot. Do this only when the callee's declared
        // params/returns contain no memory pointer: memory pointer returns may
        // reference the callee's frame/heap region, and a memory pointer param lets
        // the callee install a fresh pointer into caller-visible memory. Solidity
        // allocation lowering zero-initializes new arrays/bytes/structs, so reclaimed
        // frame bytes need not be wiped.
        if self.restorable_internal_frames.contains(callee) {
            self.emit_current_internal_frame_addr(0);
            self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
            self.asm.emit_op(op::MSTORE);
        }

        // Restore the caller frame pointer. If a result is on the stack, this leaves it there.
        self.emit_current_internal_frame_addr(WORD_BYTES as u64);
        self.asm.emit_op(op::MLOAD);
        self.asm.emit_push(U256::from(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT));
        self.asm.emit_op(op::MSTORE);

        // Store a multi-return call's first result only after restoring the
        // caller frame pointer. The result is a caller value, so spilling it
        // while the callee frame is active can overwrite another return word.
        if returns > 1
            && let Some(result) = live_result
        {
            self.spill_top_value_if_live(func, liveness, block, inst_idx, result);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn plan_static_call_stack(
        &self,
        func_id: FunctionId,
        func: &Function,
        callee: FunctionId,
        stack_mask: Option<&DenseBitSet<usize>>,
        args: &[ValueId],
        returns: usize,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) -> Option<StaticCallStackPlan> {
        let depth = self.scheduler.stack.depth();
        if !self.preserve_caller_stack
            || !(1..MAX_STACK_ACCESS).contains(&depth)
            || self.recursive_stack_functions.contains(func_id)
            || self.recursion_reaching_functions.contains(callee)
        {
            return None;
        }

        // Stack arguments are inserted above the hidden return label. A value duplicated from the
        // preserved caller prefix must remain addressable after the label and earlier arguments
        // have been pushed.
        if let Some(mask) = stack_mask {
            let mut words_above = 1;
            for (index, &arg) in args.iter().enumerate() {
                if !mask.contains(index) {
                    continue;
                }
                if self
                    .scheduler
                    .stack
                    .find(arg)
                    .is_some_and(|depth| depth + words_above + 1 > MAX_STACK_ACCESS)
                {
                    return None;
                }
                words_above += 1;
            }
        }

        // A call cannot observe words below its hidden return address. Keep an entirely live,
        // uniquely identified caller stack there without requiring the first post-call
        // instruction to consume the whole layout. The callee scheduler remains relative to its
        // own stack, and whole-program prefix validation retains the conservative spill fallback
        // when nested calls would exceed the physical EVM stack.
        let mut seen = FxHashSet::default();
        let opaque_prefix = self.scheduler.stack.iter().all(|word| {
            word.is_some_and(|value| {
                seen.insert(value) && liveness.is_used_at_or_after(value, block, inst_idx + 1)
            })
        });
        if opaque_prefix {
            return Some(StaticCallStackPlan {
                prepare_ops: Vec::new(),
                caller_stack: self.scheduler.stack.clone(),
            });
        }

        let mut retained = Vec::new();
        for value in self.scheduler.stack.iter().flatten() {
            if !retained.contains(&value)
                && liveness.is_used_at_or_after(value, block, inst_idx + 1)
                && (self.scheduler.is_stack_only_value(value)
                    || (matches!(func.value(value), crate::mir::Value::Inst(_))
                        && Self::can_own_spill_slot(func, value)))
            {
                retained.push(value);
            }
        }
        if !retained.is_empty() {
            let is_stack_arg = |value: ValueId| {
                stack_mask.is_some_and(|mask| {
                    args.iter()
                        .enumerate()
                        .any(|(index, &arg)| mask.contains(index) && arg == value)
                })
            };
            for value in self.scheduler.stack.iter().flatten() {
                if !retained.contains(&value)
                    && is_stack_arg(value)
                    && matches!(func.value(value), Value::Inst(_))
                    && Self::can_own_spill_slot(func, value)
                {
                    retained.push(value);
                }
            }
        }
        if !retained.is_empty() {
            let mut caller_stack = self.scheduler.stack.clone();
            let mut prepare_ops = Vec::new();
            while let Some(depth) = {
                let mut remaining = Self::value_counts(retained.iter().copied());
                caller_stack.iter().enumerate().find_map(|(depth, word)| {
                    if let Some(value) = word
                        && let Some(count) = remaining.get_mut(&value)
                        && *count != 0
                    {
                        *count -= 1;
                        return None;
                    }
                    Some(depth)
                })
            } {
                if depth > MAX_STACK_ACCESS {
                    break;
                }
                if depth != 0 {
                    prepare_ops.push(StackOp::Swap(depth as u8));
                    caller_stack.swap(depth as u8);
                }
                prepare_ops.push(StackOp::Pop);
                caller_stack.pop();
            }
            if caller_stack.depth() == retained.len() {
                let stack_args_are_stable = stack_mask.is_none_or(|mask| {
                    args.iter().enumerate().all(|(index, &arg)| {
                        !mask.contains(index)
                            || !matches!(func.value(arg), crate::mir::Value::Inst(_))
                            || caller_stack.contains(arg)
                            || self.scheduler.reloadable_spill(arg).is_some()
                            || !self.scheduler.stack.contains(arg)
                    })
                });
                let fresh = retained
                    .iter()
                    .filter(|&&value| !self.scheduler.spills.is_stored(value))
                    .count();
                let spill_fallback_cost = depth + fresh * 3 + retained.len() * 2;
                if stack_args_are_stable && prepare_ops.len() < spill_fallback_cost {
                    return Some(StaticCallStackPlan { prepare_ops, caller_stack });
                }
            }
        }

        let &next_inst = func.blocks[block].instructions.get(inst_idx + 1)?;
        let orders = Self::static_call_operand_orders(&func.inst(next_inst).kind);
        let needed = orders.first()?;
        if self.first_stack_value_not_needed_by(needed).is_some() {
            return None;
        }

        let live_result = Self::live_icall_result(result, returns, liveness, block, inst_idx);
        let mut post_call = self.scheduler.clone();
        if let Some(result) = live_result {
            post_call.stack.push(result);
        }

        let cost_model = self.operand_cost_model();
        let mut drained = self.scheduler.clone();
        let mut drain_cost = ScheduleCost::stack_drain_lower_bound(depth);
        let mut stored = FxHashSet::default();
        for value in self.scheduler.stack.iter().flatten() {
            if !liveness.is_dead_after(value, block, inst_idx)
                && Self::can_own_spill_slot(func, value)
                && !drained.spills.is_stored(value)
                && stored.insert(value)
            {
                drained.spills.allocate(value);
                drained.spills.mark_stored(value);
                drain_cost = drain_cost.plus(ScheduleCost::memory_store(cost_model));
            }
        }
        drained.clear_stack();
        if let Some(result) = live_result {
            drained.stack.push(result);
        }

        let next_idx = inst_idx + 1;
        let mut preserve_cost = None;
        let mut drained_next_cost = None;
        for operands in &orders {
            let preserved =
                self.preserved_operands_for(&post_call, func, operands, liveness, block, next_idx);
            let Some(plan) = post_call.plan_operands(
                operands,
                &preserved,
                func,
                self.gcx.sess.opts.optimization,
                cost_model,
            ) else {
                continue;
            };
            let cost = plan.cost();
            if preserve_cost.is_none_or(|best: ScheduleCost| {
                cost.cmp_for(best, self.gcx.sess.opts.optimization).is_lt()
            }) {
                preserve_cost = Some(cost);
            }

            let preserved =
                self.preserved_operands_for(&drained, func, operands, liveness, block, next_idx);
            if let Some(plan) = drained.plan_operands(
                operands,
                &preserved,
                func,
                self.gcx.sess.opts.optimization,
                cost_model,
            ) {
                let cost = plan.cost();
                if drained_next_cost.is_none_or(|best: ScheduleCost| {
                    cost.cmp_for(best, self.gcx.sess.opts.optimization).is_lt()
                }) {
                    drained_next_cost = Some(cost);
                }
            }
        }

        let drain_cost = drain_cost.plus(drained_next_cost?);
        preserve_cost
            .filter(|cost| cost.cmp_for(drain_cost, self.gcx.sess.opts.optimization).is_lt())
            .map(|_| StaticCallStackPlan {
                prepare_ops: Vec::new(),
                caller_stack: self.scheduler.stack.clone(),
            })
    }

    fn static_call_operand_orders(kind: &InstKind) -> SmallVec<[SmallVec<[ValueId; 3]>; 2]> {
        let mut orders = SmallVec::new();
        if let Some(opcode) = kind.evm_opcode() {
            let operands = kind.operands();
            if !operands.is_empty()
                && op::stack_io(opcode).is_some_and(|(inputs, outputs)| {
                    outputs == 1 && usize::from(inputs) == operands.len()
                })
            {
                let mut stack_order = SmallVec::<[ValueId; 3]>::from_iter(operands.iter().copied());
                stack_order.reverse();
                orders.push(stack_order);
                if operands.len() == 2
                    && operands[0] != operands[1]
                    && op::swapped_binary_opcode(opcode).is_some()
                {
                    orders.push(SmallVec::from_iter(operands));
                }
            }
            return orders;
        }

        if let InstKind::Select(condition, if_true, if_false) = kind {
            orders.push(smallvec::smallvec![*if_false, *if_true, *condition]);
        }
        orders
    }

    /// Stores the top stack word into one static-frame argument slot.
    fn emit_static_frame_arg_store(&mut self, callee: FunctionId, index: usize) {
        let addr = self.static_frame_addr(
            callee,
            EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                + (index as u64) * EvmMemoryLayout::WORD_SIZE,
        );
        self.asm.emit_push_deferred(addr);
        self.scheduler.stack.push_unknown();
        self.asm.emit_op(op::MSTORE);
        self.scheduler.instruction_executed(2, None);
    }

    /// Call to a static-frame callee: arguments are stored at absolute
    /// addresses, the return address rides the EVM stack (same invariants as
    /// the dynamic path), and there is no frame-pointer save/update/restore
    /// and no free-pointer traffic — the callee's frame is a fixed region
    /// below the heap. Supported self-recursive Yul callees temporarily carry
    /// the suspended activation's live state on the EVM stack before reusing
    /// that region.
    #[allow(clippy::too_many_arguments)]
    fn emit_icall_static(
        &mut self,
        func_id: FunctionId,
        func: &Function,
        callee: FunctionId,
        callee_label: Label,
        return_label: Label,
        args: &[ValueId],
        returns: usize,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) -> (usize, usize) {
        let stack_mask =
            if self.runtime_stack_args { self.stack_arg_mask(callee).cloned() } else { None };
        let argument_words = stack_mask.as_ref().map_or(0, DenseBitSet::count);
        let recursive_reentry = self.recursive_frame_edges.contains(&(func_id, callee));
        let mut recursive_call_values = Vec::new();
        if recursive_reentry {
            // The callee is about to reuse a scratch frame that may belong to
            // an older activation in the same recursive component. Recover
            // every caller word needed after the call before argument stores
            // overwrite that frame, then keep those words below the hidden
            // return address for the duration of the nested activation.
            let mut seen = FxHashSet::default();
            for value in self
                .scheduler
                .stack
                .iter()
                .flatten()
                .chain(self.scheduler.spills.reloadable_values())
            {
                if Some(value) != result
                    && liveness.is_used_at_or_after(value, block, inst_idx + 1)
                    && seen.insert(value)
                {
                    recursive_call_values.push(value);
                }
            }
            for value in func.live_values() {
                if Some(value) != result
                    && matches!(func.value(value), crate::mir::Value::Arg(_))
                    && liveness.is_used_at_or_after(value, block, inst_idx + 1)
                    && seen.insert(value)
                {
                    recursive_call_values.push(value);
                }
            }
            for &value in &recursive_call_values {
                if !self.scheduler.stack.contains(value) {
                    self.emit_value(func, value);
                }
                self.scheduler.spills.invalidate_stored(value);
                if let Some(available) = &mut self.spill_available {
                    available.remove(&value);
                }
            }
        }
        let mut resident_call_values = recursive_call_values.clone();
        if recursive_reentry && let Some(mask) = &stack_mask {
            // Stack-passed actuals are installed after the memory arguments.
            // Snapshot them before any store can overwrite their source frame.
            for (index, &arg) in args.iter().enumerate() {
                if mask.contains(index) && !resident_call_values.contains(&arg) {
                    if !self.scheduler.stack.contains(arg) {
                        self.emit_value(func, arg);
                    }
                    resident_call_values.push(arg);
                }
            }
        }
        if self.preserve_caller_stack
            && let Some(resident) = self.resident_stack_args(func_id)
        {
            for &value in resident {
                if self.scheduler.is_stack_only_value(value)
                    && (liveness.is_used_at_or_after(value, block, inst_idx + 1)
                        || stack_mask.as_ref().is_some_and(|mask| {
                            args.iter()
                                .enumerate()
                                .any(|(index, &arg)| mask.contains(index) && arg == value)
                        }))
                    && !resident_call_values.contains(&value)
                {
                    resident_call_values.push(value);
                }
            }
        }
        let carries_resident_stack = !resident_call_values.is_empty();
        let caller_stack_plan = (!carries_resident_stack).then(|| {
            self.plan_static_call_stack(
                func_id,
                func,
                callee,
                stack_mask.as_ref(),
                args,
                returns,
                result,
                liveness,
                block,
                inst_idx,
            )
        });
        let caller_stack_plan = caller_stack_plan.flatten();
        if carries_resident_stack {
            for &value in &resident_call_values {
                let consumed = args
                    .iter()
                    .enumerate()
                    .filter(|&(index, &arg)| {
                        arg == value && stack_mask.as_ref().is_none_or(|mask| !mask.contains(index))
                    })
                    .count();
                while self.scheduler.stack.iter().filter(|slot| *slot == Some(value)).count()
                    <= consumed
                {
                    let depth = self.scheduler.stack.find(value).unwrap_or_else(|| {
                        if self.recover_lost_internal_stack_value(value) {
                            return 0;
                        }
                        panic!(
                            "resident argument {value:?} was lost before an internal call in `{}` \
                             at {block:?}:{inst_idx}; args={args:?}, mask={stack_mask:?}, \
                             resident={resident_call_values:?}, stack={:?}",
                            func.name, self.scheduler.stack
                        )
                    });
                    assert!(depth < MAX_STACK_ACCESS, "resident argument exceeded DUP16 reach");
                    self.emit_stack_op(StackOp::Dup((depth + 1) as u8));
                }
            }
        }
        if !recursive_reentry && caller_stack_plan.is_none() {
            // The fallback drains the caller stack, so park every value needed after the call
            // before consuming arguments.
            self.spill_live_stack_values(func_id, func, liveness, block, inst_idx);
        }

        let memory_args = args
            .iter()
            .enumerate()
            .filter(|&(index, _)| stack_mask.as_ref().is_none_or(|mask| !mask.contains(index)))
            .map(|(index, &arg)| (index, arg))
            .collect::<Vec<_>>();
        if recursive_reentry {
            // The callee reuses this component's static frame. Materialize the
            // complete memory-passed tuple before the first destination write,
            // then consume the snapshots in reverse order. This is a parallel
            // copy even when arguments permute the caller's frame slots.
            for &(_, arg) in &memory_args {
                self.emit_operand(func, arg);
            }
        }
        if recursive_reentry {
            for (index, _) in memory_args.into_iter().rev() {
                self.emit_static_frame_arg_store(callee, index);
            }
        } else {
            for (index, arg) in memory_args {
                self.emit_operand(func, arg);
                self.emit_static_frame_arg_store(callee, index);
            }
        }

        let mut retention_plan = (!carries_resident_stack && caller_stack_plan.is_none())
            .then(|| {
                stack_mask.as_ref().and_then(|mask| self.plan_retained_stack_args(func, args, mask))
            })
            .flatten();

        // A retention plan is tied to the exact modeled stack it was built from. If any computed,
        // non-retained argument still needs materialization, reject retention before that
        // materialization can mutate the stack and use the conservative spill/drain/reload path.
        // This makes the ordering invariant structural instead of relying on
        // `spill_live_stack_values` having happened to store every such value already.
        if retention_plan.as_ref().is_some_and(|plan| {
            stack_mask.as_ref().is_some_and(|mask| {
                args.iter().enumerate().any(|(i, &arg)| {
                    mask.contains(i)
                        && !plan.retained.contains(i)
                        && matches!(func.value(arg), crate::mir::Value::Inst(_))
                        && self.scheduler.reloadable_spill(arg).is_none()
                })
            })
        }) {
            retention_plan = None;
        }

        // A computed argument not retained physically survives the drain in
        // its spill slot and is reloaded raw after it. Validate and retain the
        // exact slot before clearing the stack; failure is an invariant error
        // in every build instead of an unchecked MLOAD in release builds.
        let mut raw_spill_slots = vec![None; args.len()];
        if let Some(mask) = &stack_mask {
            for (i, &arg) in args.iter().enumerate() {
                if mask.contains(i)
                    && !retention_plan.as_ref().is_some_and(|plan| plan.retained.contains(i))
                    && !caller_stack_plan
                        .as_ref()
                        .is_some_and(|plan| plan.caller_stack.contains(arg))
                    && matches!(func.value(arg), crate::mir::Value::Inst(_))
                    && !Self::is_always_rematerializable_value(func, arg)
                {
                    let slot = if let Some(slot) = self.scheduler.reloadable_spill(arg) {
                        slot
                    } else {
                        self.emit_value(func, arg);
                        self.spill_value_if_needed(func, arg);
                        if self.scheduler.stack.top() == Some(arg) {
                            self.emit_stack_op(StackOp::Pop);
                        }
                        self.scheduler.reloadable_spill(arg).unwrap_or_else(|| {
                            panic!(
                                "computed stack argument {arg:?} is neither resident nor \
                                 runtime-reloadable in `{}`",
                                func.name
                            )
                        })
                    };
                    raw_spill_slots[i] = Some(slot);
                }
            }
        }

        let caller_stack = if carries_resident_stack {
            self.pop_stack_values_not_needed_by(&resident_call_values);
            let target =
                resident_call_values.iter().copied().map(TargetSlot::Value).collect::<Vec<_>>();
            let shuffle = self.scheduler.shuffle_to_layout(&target).unwrap_or_else(|| {
                panic!(
                    "could not preserve resident arguments across an internal call in `{}`: \
                     stack={:?}, target={target:?}",
                    func.name, self.scheduler.stack
                )
            });
            for op in shuffle.ops {
                self.asm.emit_stack_op(op);
            }
            Some(self.scheduler.stack.clone())
        } else {
            caller_stack_plan.map(|mut plan| {
                for op in plan.prepare_ops {
                    self.emit_stack_op(op);
                }
                debug_assert_eq!(plan.caller_stack.as_slice(), self.scheduler.stack.as_slice());
                plan.caller_stack.inherit_max_depth(self.scheduler.stack.max_depth());
                plan.caller_stack
            })
        };
        let preserved_words = caller_stack.as_ref().map_or(0, StackModel::depth);
        if let Some(plan) = &retention_plan {
            for &op in &plan.drain_ops {
                self.emit_stack_op(op);
            }
            debug_assert_eq!(self.scheduler.stack.depth(), plan.retained.count());
        } else if caller_stack.is_none() {
            self.pop_all_stack_values();
        }
        self.scheduler.clear_stack();

        self.emit_push_label(return_label);
        // Stack-passed arguments ride above the return address, untracked by
        // the model like the return address itself; the callee prologue
        // stores them into its frame before its body runs.
        if let Some(mask) = &stack_mask {
            let mut pushed_args = 0;
            for (i, &arg) in args.iter().enumerate() {
                if mask.contains(i)
                    && !retention_plan.as_ref().is_some_and(|plan| plan.retained.contains(i))
                {
                    self.emit_raw_stack_arg(
                        func,
                        arg,
                        raw_spill_slots[i],
                        caller_stack.as_ref(),
                        1 + pushed_args,
                    );
                    pushed_args += 1;
                }
            }
        }
        if let Some(plan) = &retention_plan {
            for &op in &plan.shuffle_ops {
                debug_assert!(matches!(op, StackOp::Swap(_)));
                self.asm.emit_stack_op(op);
            }
        }
        self.emit_push_label(callee_label);
        self.asm.emit_op(op::JUMP);

        self.asm.define_label(return_label);
        if let Some(caller_stack) = caller_stack {
            self.scheduler.stack = caller_stack;
        } else {
            self.scheduler.clear_stack();
        }

        // The nested activation is finished, so rebuild the caller's frame
        // homes from the words retained below its return address. This makes
        // later block entries and another recursive call see the caller's
        // state rather than the child activation's last stores.
        for &value in &recursive_call_values {
            if let crate::mir::Value::Arg(index) = func.value(value) {
                let depth = self.scheduler.stack.find(value).unwrap_or_else(|| {
                    panic!("recursive caller argument {value:?} was not preserved")
                });
                assert!(depth < MAX_STACK_ACCESS, "recursive caller argument exceeded DUP16 reach");
                self.emit_stack_op(StackOp::Dup((depth + 1) as u8));
                let addr = self.static_frame_addr(
                    func_id,
                    EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                        + index.index() as u64 * EvmMemoryLayout::WORD_SIZE,
                );
                self.asm.emit_push_deferred(addr);
                self.scheduler.stack.push_unknown();
                self.asm.emit_op(op::MSTORE);
                self.scheduler.instruction_executed(2, None);
            } else {
                self.spill_value_if_needed(func, value);
            }
        }

        if let Some(plan) = self.stack_return_plan(callee) {
            self.adopt_stack_call_results(
                func, callee, plan, returns, result, liveness, block, inst_idx,
            );
            return (preserved_words, argument_words);
        }

        if let Some(result) = Self::live_icall_result(result, returns, liveness, block, inst_idx) {
            let addr = self.static_frame_addr(
                callee,
                EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                    + (args.len() as u64) * EvmMemoryLayout::WORD_SIZE,
            );
            self.asm.emit_push_deferred(addr);
            self.asm.emit_op(op::MLOAD);
            self.scheduler.stack.push(result);
            self.spill_top_value_if_live(func, liveness, block, inst_idx, result);
        }

        // Publish the static callee's return area directly. Tail projections
        // are consumed before another call can reuse the overlaid frame.
        if returns > 1 {
            let addr = self.static_frame_addr(
                callee,
                EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                    + (args.len() as u64) * EvmMemoryLayout::WORD_SIZE,
            );
            self.asm.emit_push_deferred(addr);
            self.asm.emit_push(U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT));
            self.asm.emit_op(op::MSTORE);
        }
        (preserved_words, argument_words)
    }

    /// Adopts a static callee's stack-native return tuple.
    ///
    /// MIR names only the first result; later results are consumed through the multi-return
    /// buffer. When the caller's protocol reads project cleanly, every returned word binds
    /// directly to its consumer and the buffer never materializes. Otherwise the callee leaves
    /// result `N - 1` on top, so stage those anonymous tail words in reverse order and then
    /// attach result zero to the caller's scheduler model.
    #[allow(clippy::too_many_arguments)]
    fn adopt_stack_call_results(
        &mut self,
        func: &Function,
        callee: FunctionId,
        plan: StackReturnPlan,
        returns: usize,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        assert_eq!(returns, plan.arity, "stack-return call arity changed after ABI planning");

        if plan.arity > 1 {
            if let Some(result) =
                Self::live_icall_result(result, returns, liveness, block, inst_idx)
                && let Some(projection) =
                    Self::plan_stack_result_projection(func, block, inst_idx, plan.arity)
            {
                // The words already sit in tuple order with result `N - 1` on
                // top; bind each to its protocol load and skip the republish
                // and the loads entirely.
                self.scheduler.stack.push(result);
                for &extra in &projection.extras {
                    self.scheduler.stack.push(extra);
                }
                self.elided_insts.extend(projection.elided);
                self.spill_adopted_call_result(func, liveness, block, inst_idx, result);
                for &extra in &projection.extras {
                    self.spill_adopted_call_result(func, liveness, block, inst_idx, extra);
                }
                return;
            }

            // Keep the ordinary return area for multiword stack callees as a compiler-owned
            // fallback buffer. A callee may legally leave slot `0x40` clobbered, so deriving this
            // address from the post-call free-memory pointer would turn a valid return into an
            // arbitrary write or OOG. The common direct-projection path never touches the buffer.
            let return_base = plan.local_base - plan.arity as u64 * EvmMemoryLayout::WORD_SIZE;
            let buffer = self.static_frame_addr(callee, return_base);
            self.asm.emit_push_deferred(buffer);
            self.asm.emit_push(U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT));
            self.asm.emit_op(op::MSTORE);

            for index in (1..plan.arity).rev() {
                self.asm.emit_push(U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT));
                self.asm.emit_op(op::MLOAD);
                self.asm.emit_push(U256::from(index as u64 * EvmMemoryLayout::WORD_SIZE));
                self.asm.emit_op(op::ADD);
                // The address is on top of the anonymous return word.
                self.asm.emit_op(op::MSTORE);
            }
        }

        if let Some(result) = Self::live_icall_result(result, returns, liveness, block, inst_idx) {
            self.scheduler.stack.push(result);
            self.spill_top_value_if_live(func, liveness, block, inst_idx, result);
        } else {
            self.asm.emit_stack_op(StackOp::Pop);
        }
    }

    /// Plans direct adoption of a stack-returned tuple's anonymous tail words.
    ///
    /// MIR consumes returns `1..N` through the ephemeral buffer published at
    /// the scratch pointer slot. When the complete protocol — the pointer read
    /// and one offset load per extra return — follows the call with only pure
    /// instructions between, each load observes exactly the word the callee
    /// left on the stack, so the loads' results can adopt those words and the
    /// buffer never needs to exist. Any other consumer of the pointer or its
    /// offset addresses keeps the memory protocol.
    fn plan_stack_result_projection(
        func: &Function,
        block: BlockId,
        call_idx: usize,
        arity: usize,
    ) -> Option<StackResultProjection> {
        let tail = func.blocks[block].instructions.get(call_idx + 1..)?;

        // The first effectful instruction after the call must be the buffer
        // pointer read; nothing may intervene that could publish or clobber.
        let mut base = None;
        for (offset, &inst_id) in tail.iter().enumerate() {
            let inst = func.inst(inst_id);
            if let InstKind::MLoad(addr) = inst.kind
                && func.value_u64(addr) == Some(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT)
            {
                base = Some((offset, inst_id));
                break;
            }
            if inst.kind.effect_kind() != EffectKind::Pure {
                return None;
            }
        }
        let (base_offset, base_inst) = base?;
        let base_value = func.inst_result_value(base_inst)?;

        let mut elided = vec![base_inst];
        let mut addresses = FxHashMap::default();
        let mut extras = vec![None; arity - 1];
        for &inst_id in &tail[base_offset + 1..] {
            let inst = func.inst(inst_id);
            match &inst.kind {
                InstKind::Add(a, b) if *a == base_value || *b == base_value => {
                    let imm = if *a == base_value { *b } else { *a };
                    let index = func
                        .value_u64(imm)
                        .filter(|offset| offset % EvmMemoryLayout::WORD_SIZE == 0)
                        .map(|offset| (offset / EvmMemoryLayout::WORD_SIZE) as usize)?;
                    let address = func.inst_result_value(inst_id)?;
                    if !(1..arity).contains(&index) || addresses.insert(address, index).is_some() {
                        return None;
                    }
                    elided.push(inst_id);
                }
                InstKind::MLoad(addr) if addresses.contains_key(addr) => {
                    let result = func.inst_result_value(inst_id)?;
                    if extras[addresses[addr] - 1].replace(result).is_some() {
                        return None;
                    }
                    elided.push(inst_id);
                    if extras.iter().all(Option::is_some) {
                        break;
                    }
                }
                kind if kind.effect_kind() == EffectKind::Pure => {}
                _ => return None,
            }
        }
        let extras = extras.into_iter().collect::<Option<Vec<_>>>()?;

        // The pointer and its offset addresses must have no consumers beyond
        // the elided protocol; anything else still expects the buffer.
        let tracked = addresses.keys().copied().chain([base_value]).collect::<FxHashSet<_>>();
        let elided_set = elided.iter().copied().collect::<FxHashSet<_>>();
        for check_block in func.blocks.iter() {
            for &inst_id in &check_block.instructions {
                if !elided_set.contains(&inst_id)
                    && func.inst(inst_id).kind.operands().iter().any(|op| tracked.contains(op))
                {
                    return None;
                }
            }
            if let Some(terminator) = &check_block.terminator
                && terminator.operands().iter().any(|op| tracked.contains(op))
            {
                return None;
            }
        }

        Some(StackResultProjection { elided, extras })
    }

    /// Applies the eager-spill contract to a call result adopted mid-stack.
    ///
    /// Mirrors [`Self::spill_top_value_if_live`] without requiring the value
    /// on top: adopted tuple words sit in return order, so earlier results
    /// spill from beneath the later ones.
    fn spill_adopted_call_result(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
        value: ValueId,
    ) {
        if self.scheduler.is_stack_only_value(value) || Self::is_rematerializable_value(func, value)
        {
            return;
        }
        let has_reserved_cross_block_slot = self.scheduler.spills.get(value).is_some();
        if liveness.is_dead_after(value, block, inst_idx) && !has_reserved_cross_block_slot {
            return;
        }
        if !self.spill_value_to_reserved_slot(func, value) {
            self.spill_value_if_needed(func, value);
        }
        if has_reserved_cross_block_slot {
            assert!(
                self.scheduler.reloadable_spill(value).is_some(),
                "reserved operand {value:?} was not stored before consumption in `{}`",
                func.name
            );
        }
    }

    fn spill_live_stack_values(
        &mut self,
        func_id: FunctionId,
        func: &Function,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        let stack_values: Vec<_> = self.scheduler.stack.iter().flatten().collect();
        for value in stack_values {
            if !liveness.is_dead_after(value, block, inst_idx) {
                self.materialize_stack_only_home(func_id, func, value);
                self.spill_value_if_needed(func, value);
            }
        }
    }

    /// Emits a value to the stack.
    fn emit_value(&mut self, func: &Function, val: ValueId) {
        self.emit_value_impl(func, val, true);
    }

    /// Emits a consuming operand occurrence to the stack.
    fn emit_operand(&mut self, func: &Function, val: ValueId) {
        self.emit_value_impl(func, val, false);
    }

    /// Returns materialization costs for the active argument and spill addressing convention.
    fn operand_cost_model(&self) -> OperandCostModel {
        if self.in_internal_function
            && self
                .current_internal_function
                .is_none_or(|func_id| !self.static_frame_functions.contains(func_id))
        {
            OperandCostModel::DYNAMIC_FRAME
        } else if self.in_constructor {
            OperandCostModel::CONSTRUCTOR
        } else {
            OperandCostModel::DIRECT
        }
    }

    /// Plans operand preparation for operations whose inputs remain valid while
    /// they are rearranged. Memory-mutating stores/copies and calls keep their
    /// freshness-aware emitters until the stack model represents value epochs.
    fn plan_operands(
        &self,
        func: &Function,
        operands: &[ValueId],
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) -> Option<OperandPlan> {
        let preserved =
            self.preserved_operands_for(&self.scheduler, func, operands, liveness, block, inst_idx);
        self.scheduler.plan_operands(
            operands,
            &preserved,
            func,
            self.gcx.sess.opts.optimization,
            self.operand_cost_model(),
        )
    }

    fn preserved_operands_for(
        &self,
        scheduler: &StackScheduler,
        func: &Function,
        operands: &[ValueId],
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) -> SmallVec<[ValueId; 8]> {
        let mut preserved = SmallVec::<[ValueId; 8]>::new();
        for value in scheduler.stack.iter().flatten() {
            if scheduler.is_stack_only_value(value)
                && liveness.is_used_at_or_after(value, block, inst_idx + 1)
                && !preserved.contains(&value)
            {
                preserved.push(value);
            }
        }
        for &value in operands {
            // A shallow reload can serve the next instruction through one DUP. Keeping a deeper
            // reload can cost more in shuffles than its later memory load.
            let used_by_next_instruction = scheduler.stack.depth() <= 1
                && func.blocks[block].instructions[inst_idx + 1..]
                    .first()
                    .is_some_and(|&inst| func.inst(inst).kind.operands().contains(&value));
            let alias_is_live = self
                .global_stack_aliases
                .get(&value)
                .is_some_and(|&alias| !liveness.is_dead_after(alias, block, inst_idx));
            let carried_arg_is_live = self.global_stack_active
                && matches!(func.value(value), crate::mir::Value::Arg(_))
                && !liveness.is_dead_after(value, block, inst_idx);
            let rematerializable = Self::is_rematerializable_value(func, value)
                || Self::is_always_rematerializable_value(func, value);
            if !preserved.contains(&value)
                && (!liveness.is_dead_after(value, block, inst_idx) || alias_is_live)
                && (!rematerializable || carried_arg_is_live)
                && (scheduler.reloadable_spill(value).is_none()
                    || scheduler.stack.contains(value)
                    || used_by_next_instruction)
            {
                preserved.push(value);
            }
        }
        preserved
    }

    fn emit_operand_plan(&mut self, func: &Function, plan: OperandPlan) {
        let stack_depth = self.scheduler.depth();
        let ops = self.scheduler.apply_operand_plan(plan);
        self.record_scheduled_ops_peak(stack_depth, &ops);
        self.emit_scheduled_ops(func, ops);
    }

    fn record_scheduled_ops_peak(&mut self, stack_depth: usize, ops: &[ScheduledOp]) {
        self.scheduler.observe_scheduled_ops_peak(stack_depth, ops, self.operand_cost_model());
    }

    fn emit_scheduled_ops(&mut self, func: &Function, ops: impl IntoIterator<Item = ScheduledOp>) {
        for op in ops {
            match op {
                ScheduledOp::Stack(stack_op) => {
                    self.asm.emit_stack_op(stack_op);
                }
                ScheduledOp::PushImmediate(imm) => {
                    self.asm.emit_push(imm);
                }
                ScheduledOp::RematerializeNullary(opcode) => {
                    self.asm.emit_op(opcode);
                }
                ScheduledOp::LoadSpill(slot) => {
                    // PUSH slot_offset, MLOAD
                    self.emit_spill_load(func, slot);
                }
                ScheduledOp::LoadArg(index) => {
                    if self.in_internal_function {
                        self.emit_internal_arg_load(index);
                    } else if self.in_constructor {
                        self.emit_constructor_arg_load(index);
                    } else {
                        // Runtime function: load from calldata
                        // ABI encoding stores the selector in the first four bytes.
                        let offset = 4 + (index.index() as u64) * WORD_BYTES as u64;
                        self.asm.emit_push(U256::from(offset));
                        self.asm.emit_op(op::CALLDATALOAD);
                    }
                }
            }
        }
    }

    fn emit_fresh_scheduled_value(&mut self, func: &Function, value: ValueId, op: ScheduledOp) {
        self.record_scheduled_ops_peak(self.scheduler.depth(), &[op]);
        self.emit_scheduled_ops(func, [op]);
        self.scheduler.stack.push(value);
    }

    fn emit_value_impl(&mut self, func: &Function, val: ValueId, claim_top: bool) {
        // Prefer the tracked definition while it is resident. Re-emitting beside that copy gives
        // one MIR identity two physical stack positions and invalidates resident layout plans.
        if Self::is_always_rematerializable_value(func, val)
            && self.scheduler.stack.find(val).is_none()
        {
            self.emit_value_fresh(func, val);
            return;
        }

        if self.scheduler.is_stack_only_value(val)
            && self.scheduler.stack.find(val).is_none()
            && self.scheduler.reloadable_spill(val).is_none()
            && self.recover_lost_internal_stack_value(val)
        {
            return;
        }
        if let Some(depth) = self.scheduler.stack.find(val)
            && depth >= self.stack_access_limit()
            && self.scheduler.reloadable_spill(val).is_none()
            && (self.scheduler.is_stack_only_value(val)
                || !matches!(
                    func.value(val),
                    crate::mir::Value::Immediate(_) | crate::mir::Value::Arg(_)
                ))
        {
            let slot = self.scheduler.spills.allocate(val);
            self.spill_deep_stack_value(func, val, slot, depth);
        }

        if self.scheduler.stack.find(val).is_none()
            && self.scheduler.should_recompute_unstored_spill(val)
        {
            self.emit_value_fresh(func, val);
            return;
        }

        let stack_depth = self.scheduler.depth();
        let ops = if claim_top {
            self.scheduler.ensure_on_top(val, func)
        } else {
            self.scheduler.ensure_operand_on_top(val, func)
        }
        .to_vec();
        self.record_scheduled_ops_peak(stack_depth, &ops);
        self.emit_scheduled_ops(func, ops);
    }

    /// Emits a value fresh, without trying to DUP from the stack.
    /// This is used for CALL operands where we need to guarantee correct values
    /// regardless of scheduler stack tracking state.
    fn collect_late_gas_operands(&mut self, func: &Function) {
        self.late_gas_operands.clear();

        let mut use_counts = index_vec![0u32; func.num_values()];
        for block in &func.blocks {
            for &inst_id in &block.instructions {
                for operand in func.inst(inst_id).kind.operands() {
                    use_counts[operand] += 1;
                }
            }
            if let Some(terminator) = &block.terminator {
                for operand in terminator.operands() {
                    use_counts[operand] += 1;
                }
            }
        }

        for block in &func.blocks {
            for &inst_id in &block.instructions {
                let gas = match func.inst(inst_id).kind {
                    InstKind::Call { gas, .. }
                    | InstKind::CallCode { gas, .. }
                    | InstKind::StaticCall { gas, .. }
                    | InstKind::DelegateCall { gas, .. } => gas,
                    _ => continue,
                };
                if use_counts[gas] != 1 {
                    continue;
                }
                let Value::Inst(operand) = func.value(gas) else { continue };
                let (reading, subtracted) = match func.inst(*operand).kind {
                    InstKind::Gas => (*operand, None),
                    InstKind::Sub(lhs, rhs) => {
                        let Value::Inst(reading) = func.value(lhs) else { continue };
                        let Value::Immediate(imm) = func.value(rhs) else { continue };
                        let Some(subtracted) = imm.as_u256() else { continue };
                        if !matches!(func.inst(*reading).kind, InstKind::Gas)
                            || use_counts[lhs] != 1
                        {
                            continue;
                        }
                        (*reading, Some(subtracted))
                    }
                    _ => continue,
                };
                if !block.instructions.contains(&reading) || !block.instructions.contains(operand) {
                    continue;
                }
                self.elided_insts.insert(reading);
                self.elided_insts.insert(*operand);
                self.late_gas_operands.insert(gas, LateGasOperand { subtracted });
            }
        }
    }

    fn emit_gas_operand(&mut self, func: &Function, gas: ValueId) {
        let Some(late) = self.late_gas_operands.get(&gas) else {
            self.emit_value_fresh(func, gas);
            return;
        };

        if let Some(subtracted) = late.subtracted {
            // push <reserve>
            // gas !metadata(keep_with_next)
            // sub !metadata(keep_with_next)
            //
            // Before EIP-150 a call asking for more gas than is left throws, so the reserve only
            // keeps solc's 10-gas margin while nothing but the `SUB` runs between the `GAS` and
            // the call. Keeping both with the next instruction stops every backend transform from
            // making that boundary a block boundary, and with it from inserting a jump.
            let keep_with_call = !self.gcx.sess.opts.evm_version.can_overcharge_gas_for_call();
            self.asm.emit_push(subtracted);
            self.asm.emit_op(op::GAS);
            if keep_with_call {
                self.asm.keep_last_with_next();
            }
            self.asm.emit_op(op::SUB);
            if keep_with_call {
                self.asm.keep_last_with_next();
            }
        } else {
            self.asm.emit_op(op::GAS);
        }
        self.scheduler.stack.push(gas);
    }

    fn emit_value_fresh(&mut self, func: &Function, val: ValueId) {
        if let Some(op) = Self::always_rematerializable_op(func, val) {
            self.emit_fresh_scheduled_value(func, val, ScheduledOp::RematerializeNullary(op));
            return;
        }

        if self.scheduler.is_stack_only_value(val)
            && self.scheduler.stack.find(val).is_none()
            && self.scheduler.reloadable_spill(val).is_none()
            && self.recover_lost_internal_stack_value(val)
        {
            return;
        }
        match func.value(val) {
            crate::mir::Value::Immediate(imm) => {
                if let Some(u256) = imm.as_u256() {
                    self.emit_fresh_scheduled_value(func, val, ScheduledOp::PushImmediate(u256));
                }
            }
            crate::mir::Value::Arg(index) => {
                if self.scheduler.is_stack_only_value(val) {
                    let depth = self.scheduler.stack.find(val).unwrap_or_else(|| {
                        panic!(
                            "stack-only argument {val:?} was lost before fresh emission in `{}`",
                            func.name
                        )
                    });
                    assert!(
                        depth < self.stack_access_limit(),
                        "stack-only argument exceeded DUP reach"
                    );
                    self.emit_stack_op(StackOp::Dup(depth as u8 + 1));
                    return;
                }
                if let Some(depth) = self.scheduler.stack.find(val)
                    && depth < self.stack_access_limit()
                {
                    self.emit_stack_op(StackOp::Dup(depth as u8 + 1));
                    return;
                }
                self.emit_fresh_scheduled_value(func, val, ScheduledOp::LoadArg(*index));
            }
            crate::mir::Value::Inst(inst_id) => {
                // A value carried on the live stack is the current definition;
                // duplicate it instead of reloading or recomputing. A preserved
                // edge can carry a value that was never spilled, and
                // recomputing a definition such as an FMP load would observe
                // memory that changed since the definition executed.
                if let Some(depth) = self.scheduler.stack.find(val)
                    && depth < self.stack_access_limit()
                {
                    self.emit_stack_op(StackOp::Dup(depth as u8 + 1));
                    return;
                }
                // For instruction results, we need to check if they're spilled
                // or if they're instruction results that produce fresh values (like GAS, MLOAD)
                if let Some(slot) = self.scheduler.reloadable_spill(val) {
                    // Load from spill slot. Reloadable covers slots whose
                    // defining block is emitted later: the definition still
                    // executes before any use at runtime.
                    self.emit_fresh_scheduled_value(func, val, ScheduledOp::LoadSpill(slot));
                } else {
                    // Check if the instruction is one that we can "re-execute" to get a fresh value
                    // This handles GAS (which is always fresh) and MLOAD (which re-reads from
                    // memory)
                    let inst_kind = &func.inst(*inst_id).kind;
                    if let Some(opcode) = rematerializable_nullary_opcode(inst_kind).or_else(|| {
                        inst_kind.evm_opcode().filter(|_| matches!(inst_kind, InstKind::Gas))
                    }) {
                        self.emit_fresh_scheduled_value(
                            func,
                            val,
                            ScheduledOp::RematerializeNullary(opcode),
                        );
                    } else {
                        match inst_kind {
                            crate::mir::InstKind::LoadImmutable(id) if !self.in_constructor => {
                                self.emit_load_immutable(*id);
                                self.scheduler.stack.push(val);
                            }
                            crate::mir::InstKind::InternalFrameAddr(offset) => {
                                self.emit_own_frame_addr(*offset);
                                self.scheduler.stack.push(val);
                            }
                            crate::mir::InstKind::ConstructorArgsBase => {
                                self.emit_constructor_args_base();
                                self.scheduler.stack.push(val);
                            }
                            crate::mir::InstKind::ConstructorArgsEnd => {
                                self.emit_constructor_args_end();
                                self.scheduler.stack.push(val);
                            }
                            crate::mir::InstKind::MLoad(offset) => {
                                // Re-reading a constant scratch location is safe, but the
                                // free-memory-pointer word moves: a pointer defined as
                                // `mload(0x40)` must reach this point through its spill
                                // slot. A slot that is reloadable but not yet stored
                                // belongs to a defining block emitted after this point
                                // that still executes first at runtime.
                                if func.value_u64(*offset) == Some(EvmMemoryLayout::FMP_SLOT) {
                                    if let Some(slot) = self.scheduler.reloadable_spill(val) {
                                        self.emit_fresh_scheduled_value(
                                            func,
                                            val,
                                            ScheduledOp::LoadSpill(slot),
                                        );
                                        return;
                                    }
                                    panic!(
                                        "emit_value_fresh: rematerializing a stale \
                                     free-memory-pointer load: {val:?} in `{}`",
                                        func.name
                                    );
                                }
                                self.emit_value_fresh(func, *offset);
                                self.asm.emit_op(op::MLOAD);
                                // Pop offset, push result
                                self.scheduler.stack.pop();
                                self.scheduler.stack.push(val);
                            }
                            crate::mir::InstKind::CalldataLoad(offset) => {
                                // Calldata is immutable, so re-reading it is
                                // always safe once the address rematerializes.
                                self.emit_value_fresh(func, *offset);
                                self.asm.emit_op(op::CALLDATALOAD);
                                // Pop offset, push result
                                self.scheduler.stack.pop();
                                self.scheduler.stack.push(val);
                            }
                            kind if kind.evm_opcode().is_some_and(|opcode| {
                                matches!(
                                    opcode,
                                    op::KECCAK256
                                        | op::ADD
                                        | op::SUB
                                        | op::MUL
                                        | op::AND
                                        | op::OR
                                        | op::XOR
                                        | op::SHL
                                        | op::SHR
                                        | op::DIV
                                        | op::SDIV
                                        | op::MOD
                                        | op::SMOD
                                        | op::LT
                                        | op::GT
                                        | op::SLT
                                        | op::SGT
                                        | op::EQ
                                        | op::SAR
                                )
                            }) =>
                            {
                                let opcode = kind.evm_opcode().unwrap();
                                let operands = kind.operands();
                                debug_assert_eq!(operands.len(), 2);
                                self.emit_fresh_binary(
                                    func,
                                    val,
                                    operands[0],
                                    operands[1],
                                    opcode,
                                    op::is_commutative(opcode),
                                );
                            }
                            crate::mir::InstKind::SLoad(slot) => {
                                // Re-emit SLOAD. CALL operands are materialized in a
                                // tight sequence with no intervening store, so the
                                // storage slot reads the same value as the original
                                // load (same recompute contract as MLOAD above).
                                self.emit_value_fresh(func, *slot);
                                self.asm.emit_op(op::SLOAD);
                                self.scheduler.stack.pop();
                                self.scheduler.stack.push(val);
                            }
                            _ => {
                                // A value that cannot be re-executed (e.g. an
                                // internal-call result used to compute a CALL
                                // operand) is live on the stack: duplicate it rather
                                // than re-running it. If it is buried too deep to
                                // `DUP`, spill it to a reserved slot and reload.
                                if let Some(depth) = self.scheduler.stack.find(val) {
                                    if depth < self.stack_access_limit() {
                                        self.emit_stack_op(StackOp::Dup(depth as u8 + 1));
                                    } else {
                                        let slot = self.scheduler.spills.allocate(val);
                                        self.spill_deep_stack_value(func, val, slot, depth);
                                        self.emit_fresh_scheduled_value(
                                            func,
                                            val,
                                            ScheduledOp::LoadSpill(slot),
                                        );
                                    }
                                } else if let Some(slot) = self.scheduler.reloadable_spill(val) {
                                    // A defining block emitted later still stores
                                    // this slot before the load executes at runtime.
                                    self.emit_fresh_scheduled_value(
                                        func,
                                        val,
                                        ScheduledOp::LoadSpill(slot),
                                    );
                                } else {
                                    panic!(
                                        "emit_value_fresh: value {val:?} ({:?}) is neither on the \
                                     stack, spilled, nor re-executable",
                                        func.inst(*inst_id).kind
                                    );
                                }
                            }
                        }
                    }
                }
            }
            crate::mir::Value::Undef(_) => {
                // Undef values shouldn't appear in CALL operands
                panic!(
                    "emit_value_fresh: unexpected undef value {val:?}. \
                     CALL operands should be concrete values."
                );
            }
            crate::mir::Value::Error(_) => {
                // A lowering error fails compilation before codegen runs.
                panic!("emit_value_fresh: error sentinel {val:?} reached the backend");
            }
        }
    }

    fn emit_fresh_binary(
        &mut self,
        func: &Function,
        result: ValueId,
        a: ValueId,
        b: ValueId,
        opcode: u8,
        commutative: bool,
    ) {
        if commutative {
            self.emit_value_fresh(func, a);
            self.emit_value_fresh(func, b);
        } else {
            // EVM binary opcodes consume `a` from the top of stack and `b`
            // from the word below, matching the normal binary emitter.
            self.emit_value_fresh(func, b);
            self.emit_value_fresh(func, a);
        }
        self.asm.emit_op(opcode);
        self.scheduler.stack.pop();
        self.scheduler.stack.pop();
        self.scheduler.stack.push(result);
    }

    /// Emits a binary operation with result tracking and liveness awareness.
    /// If an operand is still live after this instruction, we DUP it before it gets consumed.
    #[allow(clippy::too_many_arguments)]
    fn emit_binary_op_with_result(
        &mut self,
        func: &Function,
        a: ValueId,
        b: ValueId,
        opcode: u8,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        let mut selected =
            self.plan_operands(func, &[b, a], liveness, block, inst_idx).map(|plan| (opcode, plan));
        if a != b
            && selected.as_ref().is_none_or(|(_, plan)| !plan.is_free())
            && let Some(swapped_opcode) = op::swapped_binary_opcode(opcode)
            && let Some(swapped) = self.plan_operands(func, &[a, b], liveness, block, inst_idx)
            && selected.as_ref().is_none_or(|(_, current)| {
                swapped.cost().cmp_for(current.cost(), self.gcx.sess.opts.optimization).is_lt()
            })
        {
            selected = Some((swapped_opcode, swapped));
        }
        if let Some((opcode, plan)) = selected {
            self.emit_operand_plan(func, plan);
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(2, result);
            return;
        }

        self.preserve_stack_only_operands(&[a, b], liveness, block, inst_idx);

        // Check if operands are still live after this instruction.
        let a_is_live = !liveness.is_dead_after(a, block, inst_idx);

        // Special case: same operand used twice (e.g., a + a, a - a)
        if a == b {
            self.emit_value(func, a);
            if !self.block_local_copy_survives(liveness, block, a, 1) {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, a);
            }
            self.emit_operand(func, a);
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(2, result);
            return;
        }

        // Operands that already sit on top of the tracked stack are consumed
        // in place when they are dead afterwards and own no reserved spill
        // slot, instead of being re-emitted and the stale copy nipped later
        // (`DUP2 <op> ... SWAP1 POP` becomes `<op>`).
        let a_dead_free =
            liveness.is_dead_after(a, block, inst_idx) && self.scheduler.spills.get(a).is_none();
        let b_dead_free =
            liveness.is_dead_after(b, block, inst_idx) && self.scheduler.spills.get(b).is_none();
        if self.scheduler.stack.top() == Some(a)
            && self.scheduler.stack.peek(1) == Some(b)
            && a_dead_free
            && b_dead_free
        {
            // The stack is already [b, a].
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(2, result);
            return;
        }
        if self.scheduler.stack.top() == Some(b)
            && b_dead_free
            && self.scheduler.can_emit_value(a, func)
        {
            // b is in place below; put a above it.
            self.emit_value(func, a);
            if a_is_live
                && !Self::is_rematerializable_value(func, a)
                && !self.block_local_copy_survives(liveness, block, a, 1)
            {
                self.spill_value_if_needed(func, a);
            }
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(2, result);
            return;
        }
        if self.scheduler.stack.top() == Some(a)
            && a_dead_free
            && self.scheduler.can_emit_value(b, func)
        {
            // a is in place; emit b above it and swap into [b, a].
            self.emit_value(func, b);
            if !self.block_local_copy_survives(liveness, block, b, 1) {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, b);
            }
            self.emit_stack_op(StackOp::Swap(1));
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(2, result);
            return;
        }

        // Check if either operand is already on stack as an untracked value
        let a_can_emit = self.scheduler.can_emit_value(a, func);
        let b_can_emit = self.scheduler.can_emit_value(b, func);
        let has_untracked = self.scheduler.has_untracked_on_top();
        let has_untracked_at_1 = self.scheduler.has_untracked_at_depth(1);

        if !a_can_emit && b_can_emit && has_untracked {
            // a is an untracked value on top of stack, emit b, then SWAP
            self.emit_value(func, b);
            if !self.block_local_copy_survives(liveness, block, b, 1) {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, b);
            }
            self.emit_stack_op(StackOp::Swap(1));
        } else if a_can_emit && !b_can_emit && has_untracked {
            // b is an untracked value on top of stack, emit a on top
            self.emit_value(func, a);
            // Spill a if live-after (it's now at depth 0).
            if a_is_live
                && !Self::is_rematerializable_value(func, a)
                && !self.block_local_copy_survives(liveness, block, a, 1)
            {
                self.spill_value_if_needed(func, a);
            }
        } else if !a_can_emit && b_can_emit && has_untracked_at_1 {
            // a is an untracked value at depth 1, b is tracked on top
            // Stack is [b, a_untracked], need [a, b]
            self.emit_stack_op(StackOp::Swap(1));
        } else {
            // Normal case: emit b first (bottom), then a (top)
            self.emit_value(func, b);
            if !self.block_local_copy_survives(liveness, block, b, 1) {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, b);
            }
            self.emit_value(func, a);
            // Spill a if live-after (it's now at depth 0).
            if a_is_live
                && !Self::is_rematerializable_value(func, a)
                && !self.block_local_copy_survives(liveness, block, a, 1)
            {
                self.spill_value_if_needed(func, a);
            }
        }

        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(2, result);
    }

    /// Emits a unary operation with result tracking and liveness awareness.
    /// If the operand is still live after this instruction, we spill it after emitting.
    #[allow(clippy::too_many_arguments)]
    fn emit_unary_op_with_result(
        &mut self,
        func: &Function,
        a: ValueId,
        opcode: u8,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        if let Some(plan) = self.plan_operands(func, &[a], liveness, block, inst_idx) {
            self.emit_operand_plan(func, plan);
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(1, result);
            return;
        }

        self.preserve_stack_only_operands(&[a], liveness, block, inst_idx);

        self.emit_value(func, a);
        if !self.block_local_copy_survives(liveness, block, a, 1) {
            self.spill_top_value_if_live(func, liveness, block, inst_idx, a);
        }

        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(1, result);
    }

    /// Emits a `LOG0`..=`LOG4` instruction. `operands` are given in stack order
    /// (deepest first, top last) and pushed in that order; the `LOG` then
    /// consumes all of them. Each operand still live after this instruction is
    /// spilled once it reaches the top, so a later use in the same block can
    /// reload it — the same operand-liveness handling as the arithmetic, store
    /// and copy paths. Without it, a topic value consumed by the `LOG` and used
    /// again later (e.g. an event that also stores its data word) would be lost.
    fn emit_log(
        &mut self,
        func: &Function,
        opcode: u8,
        operands: &[ValueId],
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        if let Some(plan) = self.plan_operands(func, operands, liveness, block, inst_idx) {
            self.emit_operand_plan(func, plan);
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(operands.len(), None);
            return;
        }

        self.preserve_stack_only_operands(operands, liveness, block, inst_idx);

        for (i, &operand) in operands.iter().enumerate() {
            if i == 0 {
                self.emit_value(func, operand);
            } else {
                // Repeated operands (e.g. duplicate topics) need their own stack item.
                self.emit_operand(func, operand);
            }
            // Occurrences of `operand` emitted so far, this one included: the
            // instruction consumes that many copies net of the occurrences
            // still to be pushed.
            let seen = operands[..=i].iter().filter(|&&op| op == operand).count();
            if !self.block_local_copy_survives(liveness, block, operand, seen) {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, operand);
            }
        }
        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(operands.len(), None);
    }

    /// Emits a store operation with liveness awareness.
    /// If the value operand is still live after this instruction, we spill it after emitting
    /// to preserve it for later use.
    #[allow(clippy::too_many_arguments)]
    fn emit_store_op_live_aware(
        &mut self,
        func: &Function,
        addr: ValueId,
        val: ValueId,
        opcode: u8,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        self.preserve_stack_only_operands(&[addr, val], liveness, block, inst_idx);

        // Check if addr is still live after this instruction.
        let addr_is_live = !liveness.is_dead_after(addr, block, inst_idx);

        // Operands already sitting on top of the tracked stack are consumed
        // in place when they are dead afterwards and own no reserved spill
        // slot, instead of being re-emitted and the stale copies popped later
        // (`DUP2 DUP2 MSTORE ... POP POP` becomes `MSTORE`). Mirrors the
        // binary-op fast paths.
        let addr_dead_free = !addr_is_live && self.scheduler.spills.get(addr).is_none();
        let val_dead_free = liveness.is_dead_after(val, block, inst_idx)
            && self.scheduler.spills.get(val).is_none();
        if addr_dead_free && val_dead_free && self.scheduler.stack.depth() >= 2 {
            if self.scheduler.stack.top() == Some(addr) && self.scheduler.stack.peek(1) == Some(val)
            {
                // The stack is already [addr, val].
                self.asm.emit_op(opcode);
                self.scheduler.instruction_executed(2, None);
                return;
            }
            if self.scheduler.stack.top() == Some(val) && self.scheduler.stack.peek(1) == Some(addr)
            {
                self.emit_stack_op(StackOp::Swap(1));
                self.asm.emit_op(opcode);
                self.scheduler.instruction_executed(2, None);
                return;
            }
        }

        // Emit val
        self.emit_value(func, val);
        if !self.block_local_copy_survives(liveness, block, val, 1) {
            self.spill_top_value_if_live(func, liveness, block, inst_idx, val);
        }

        // Emit addr
        self.emit_operand(func, addr);
        // Spill addr if live-after (it's now at depth 0).
        let addr_consumed = if addr == val { 2 } else { 1 };
        if addr_is_live
            && !Self::is_rematerializable_value(func, addr)
            && !self.block_local_copy_survives(liveness, block, addr, addr_consumed)
        {
            self.spill_value_if_needed(func, addr);
        }

        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(2, None);
    }

    /// Emits a copy-style instruction (no result) with liveness awareness.
    /// `operands` are pushed in order, so the last one ends up on top of the
    /// stack; any operand still live after this instruction is spilled before
    /// the instruction consumes it, preserving it for later uses.
    fn emit_copy_op_live_aware(
        &mut self,
        func: &Function,
        operands: &[ValueId],
        opcode: u8,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        self.preserve_stack_only_operands(operands, liveness, block, inst_idx);

        for (i, &op) in operands.iter().enumerate() {
            if i == 0 {
                self.emit_value(func, op);
            } else {
                // Repeated operands need their own stack item each.
                self.emit_operand(func, op);
            }
            // See `emit_log`: copies consumed net of occurrences still to come.
            let seen = operands[..=i].iter().filter(|&&o| o == op).count();
            if !self.block_local_copy_survives(liveness, block, op, seen) {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, op);
            }
        }

        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(operands.len(), None);
    }

    /// Emits a copy from relocatable module data to memory.
    #[allow(clippy::too_many_arguments)]
    fn emit_data_copy(
        &mut self,
        func: &Function,
        data: crate::mir::DataRef,
        dest: ValueId,
        size: ValueId,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        let operands = [size, dest];
        self.preserve_stack_only_operands(&operands, liveness, block, inst_idx);

        self.emit_value(func, size);
        if !self.block_local_copy_survives(liveness, block, size, 1) {
            self.spill_top_value_if_live(func, liveness, block, inst_idx, size);
        }

        // Keep `dest` within DUP16 reach before the anonymous relocation push.
        self.emit_operand(func, dest);
        let dest_consumed = if dest == size { 2 } else { 1 };
        if !self.block_local_copy_survives(liveness, block, dest, dest_consumed) {
            self.spill_top_value_if_live(func, liveness, block, inst_idx, dest);
        }

        self.asm.emit_push_data(data);
        self.scheduler.stack.push_unknown();
        self.emit_stack_op(StackOp::Swap(1));

        self.asm.emit_op(op::CODECOPY);
        self.scheduler.instruction_executed(3, None);
    }

    /// Emits an operation with liveness awareness.
    #[allow(clippy::too_many_arguments)]
    fn emit_nary_op(
        &mut self,
        func: &Function,
        operands: &[ValueId],
        opcode: u8,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        if let Some(plan) = self.plan_operands(func, operands, liveness, block, inst_idx) {
            self.emit_operand_plan(func, plan);
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(operands.len(), result);
            return;
        }

        self.preserve_stack_only_operands(operands, liveness, block, inst_idx);

        for (i, &operand) in operands.iter().enumerate() {
            if i == 0 {
                self.emit_value(func, operand);
            } else {
                self.emit_operand(func, operand);
            }
            let seen = operands[..=i].iter().filter(|&&op| op == operand).count();
            if !self.block_local_copy_survives(liveness, block, operand, seen) {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, operand);
            }
        }
        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(operands.len(), result);
    }

    /// Generates a parallel copy.
    ///
    /// Phi copies move values from source to destination. The destination is typically
    /// a phi result that needs to be available in the successor block. We handle this
    /// by spilling the source value to the destination's spill slot.
    fn generate_copy(
        &mut self,
        func: &Function,
        copy: &ParallelCopy,
        temps: &mut FxHashMap<u32, ValueId>,
    ) {
        // Handle source: either a MIR value or a temporary
        match &copy.src {
            CopySource::Value(val) => {
                self.emit_operand(func, *val);
            }
            CopySource::Temp(temp_id) => {
                // Temporaries are tracked in our temps map with their ValueId
                if let Some(&temp_val) = temps.get(temp_id) {
                    self.emit_operand(func, temp_val);
                }
            }
        }

        // Handle destination: either a MIR value or a temporary
        match &copy.dst {
            CopyDest::Value(dst_val) => {
                // Spill the value on top of stack to the destination's spill slot
                // This allows the successor block to reload it
                let slot = self.scheduler.spills.reserve(*dst_val);
                self.emit_spill_slot_addr(func, slot);
                self.scheduler.stack.push_unknown();
                self.asm.emit_op(op::MSTORE);
                self.scheduler.stack.pop(); // pop the untracked offset
                self.scheduler.stack.pop(); // pop the value
                self.scheduler.spills.mark_stored(*dst_val);
                if let Some(available) = &mut self.spill_available {
                    available.insert(*dst_val);
                }
            }
            CopyDest::Temp(temp_id) => {
                // Mark this temporary as defined - it's now on the stack
                // Get the ValueId of the value currently on top
                if let Some(val_on_top) = self.scheduler.stack.top() {
                    temps.insert(*temp_id, val_on_top);
                }
            }
        }
    }

    /// Pops all remaining values from the stack.
    /// This ensures the stack is empty before control flow transfer to another block.
    fn pop_all_stack_values(&mut self) {
        while self.scheduler.stack_depth() > 0 {
            self.emit_stack_op(StackOp::Pop);
        }
    }

    fn emit_internal_return(&mut self, func: &Function, values: &[ValueId]) {
        if let Some(plan) =
            self.current_internal_function.and_then(|func_id| self.stack_return_plan(func_id))
        {
            assert_eq!(
                values.len(),
                plan.arity,
                "stack-return function `{}` changed return arity after ABI planning",
                func.name
            );
            self.pop_stack_values_not_needed_by(values);
            for value in Self::missing_stack_phi_sources(&self.scheduler.stack, values) {
                self.emit_operand(func, value);
            }
            // StackModel and the shuffler use top-to-bottom order. The physical ABI leaves the
            // last result on top so the caller can stage anonymous results N-1..1 before adopting
            // the MIR-visible first result.
            let target: Vec<_> = values.iter().rev().copied().map(TargetSlot::Value).collect();
            let Some(shuffle) = self.scheduler.shuffle_to_layout(&target) else {
                // A forwarded multi-result call leaves adopted copies of the
                // returned values on the stack; re-emitting them for the
                // return doubles every word and the bounded shuffler cannot
                // always drop the surplus mid-stack. Regenerate the runtime
                // with this function on the frame-backed return convention
                // instead of panicking.
                let func_id = self
                    .current_internal_function
                    .expect("stack-return plans only cover internal functions");
                self.disabled_stack_only_functions.insert(func_id);
                self.scheduler.clear_stack();
                return;
            };
            for op in shuffle.ops {
                self.asm.emit_stack_op(op);
            }

            // Rotate the untracked return address from below the result tuple to the top without
            // disturbing result order: SWAP1, SWAP2, ..., SWAPN maps
            // [return, r0, ..., rN] to [r0, ..., rN, return].
            for depth in 1..=plan.arity {
                self.asm.emit_stack_op(StackOp::Swap(depth as u8));
            }
            self.asm.emit_op(op::JUMP);
            self.mark_debug_function_exit(func, DebugFunctionExit::Return);
            self.scheduler.clear_stack();
            return;
        }

        let return_base = EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
            + (func.params.len() as u64) * EvmMemoryLayout::WORD_SIZE;
        for (i, &value) in values.iter().enumerate() {
            self.emit_operand(func, value);
            self.emit_own_frame_addr(return_base + (i as u64) * WORD_BYTES as u64);
            self.asm.emit_op(op::MSTORE);
            self.scheduler.stack.pop();
        }

        self.pop_all_stack_values();
        // The caller's return address is the untracked value at the bottom of
        // the stack; after popping every tracked value it is on top.
        self.asm.emit_op(op::JUMP);
        self.mark_debug_function_exit(func, DebugFunctionExit::Return);
    }

    fn emit_external_stop(&mut self, func: &Function) {
        if let Some(exit) = self.constructor_exit {
            self.emit_push_label(exit);
            self.asm.emit_op(op::JUMP);
        } else {
            self.asm.emit_op(op::STOP);
        }
        self.mark_debug_function_exit(func, DebugFunctionExit::Return);
    }

    fn emit_revert_returndata(&mut self) {
        if self.gcx.sess.opts.evm_version.supports_returndata() {
            // size = returndatasize
            // returndatacopy 0, 0, size
            // revert 0, returndatasize
            self.asm.emit_push(U256::ZERO);
            self.scheduler.stack.push_unknown();
            self.asm.emit_push(U256::ZERO);
            self.scheduler.stack.push_unknown();
            self.emit_op_with_effect(
                op::RETURNDATASIZE,
                StackEffect { pops: 0, pushes: 1 },
                StackPush::Unknown,
            );
            self.emit_op_with_effect(
                op::RETURNDATACOPY,
                StackEffect { pops: 3, pushes: 0 },
                StackPush::None,
            );
            self.emit_op_with_effect(
                op::RETURNDATASIZE,
                StackEffect { pops: 0, pushes: 1 },
                StackPush::Unknown,
            );
            self.asm.emit_push(U256::ZERO);
            self.scheduler.stack.push_unknown();
            self.emit_op_with_effect(
                op::REVERT,
                StackEffect { pops: 2, pushes: 0 },
                StackPush::None,
            );
        } else {
            // revert 0, 0
            self.asm.emit_push(U256::ZERO);
            self.scheduler.stack.push_unknown();
            self.asm.emit_push(U256::ZERO);
            self.scheduler.stack.push_unknown();
            self.emit_op_with_effect(
                op::REVERT,
                StackEffect { pops: 2, pushes: 0 },
                StackPush::None,
            );
        }
    }

    fn emit_push_label(&mut self, label: Label) {
        self.scheduler.stack.observe_peak(self.scheduler.depth().saturating_add(1));
        self.asm.emit_push_label(label);
    }

    fn generate_terminator(
        &mut self,
        func: &Function,
        term: &Terminator,
        fallthrough: Option<BlockId>,
        preserve_stack: bool,
    ) {
        match term {
            Terminator::TailCall { function, args } => {
                // Control transfers to the target and never returns. A stack ABI reuses the
                // caller's inherited return label (if any) and places selected arguments above
                // it; otherwise arguments retain their compile-time frame homes. Fused external
                // bodies terminate directly and therefore need no hidden label.
                if !args.is_empty() {
                    // `lower-evm-shaped` only forms argument-carrying tail
                    // calls to callees the backend statically frames.
                    assert!(
                        self.static_frame_functions.contains(*function),
                        "argument-carrying tail call to a non-static-frame callee"
                    );
                    let stack_mask = self
                        .runtime_stack_args
                        .then(|| self.stack_arg_mask(*function).cloned())
                        .flatten();
                    let stack_args: Vec<_> = stack_mask
                        .as_ref()
                        .into_iter()
                        .flat_map(|mask| {
                            (0..args.len())
                                .rev()
                                .filter_map(|index| mask.contains(index).then_some(args[index]))
                        })
                        .collect();
                    let recursive_reentry = self.current_internal_function.is_some_and(|caller| {
                        self.recursive_frame_edges.contains(&(caller, *function))
                    });
                    let memory_args = args
                        .iter()
                        .enumerate()
                        .filter(|&(index, _)| {
                            stack_mask.as_ref().is_none_or(|mask| !mask.contains(index))
                        })
                        .map(|(index, &arg)| (index, arg))
                        .collect::<Vec<_>>();
                    if recursive_reentry {
                        // Keep stack-passed actuals above the caller frame, and
                        // stage all memory-passed actuals before overwriting any
                        // reused destination slot.
                        for &arg in &stack_args {
                            self.emit_operand(func, arg);
                        }
                        for &(_, arg) in &memory_args {
                            self.emit_operand(func, arg);
                        }
                        for (index, _) in memory_args.into_iter().rev() {
                            self.emit_static_frame_arg_store(*function, index);
                        }
                    } else {
                        for (index, arg) in memory_args {
                            self.emit_operand(func, arg);
                            self.emit_static_frame_arg_store(*function, index);
                        }
                    }
                    if !stack_args.is_empty() {
                        self.pop_stack_values_not_needed_by(&stack_args);
                        for value in
                            Self::missing_stack_phi_sources(&self.scheduler.stack, &stack_args)
                        {
                            self.emit_operand(func, value);
                        }
                        let target: Vec<_> =
                            stack_args.iter().copied().map(TargetSlot::Value).collect();
                        let Some(shuffle) = self.scheduler.shuffle_to_layout(&target) else {
                            // An unconstructible entry layout regenerates the
                            // runtime with the callee on the frame convention;
                            // the partially emitted attempt is discarded.
                            self.disabled_stack_only_functions.insert(*function);
                            return;
                        };
                        for op in shuffle.ops {
                            self.asm.emit_stack_op(op);
                        }
                    }
                }
                let label = self.function_labels[function];
                self.emit_push_label(label);
                self.asm.emit_op(op::JUMP);
                self.mark_debug_function_exit(func, DebugFunctionExit::Return);
            }
            Terminator::Jump(target) => {
                // Pop any remaining values from the stack before jumping.
                // Each block normally starts with an empty stack, so we must
                // clean the stack before jumping — unless this edge preserves
                // its live stack into a single-predecessor target.
                if Some(*target) == fallthrough {
                    if !preserve_stack {
                        self.pop_all_stack_values();
                    }
                    return;
                }
                if !preserve_stack {
                    self.pop_all_stack_values();
                }
                self.emit_push_label(self.block_labels[target]);
                self.asm.emit_op(op::JUMP);
            }

            Terminator::Branch { condition, then_block, else_block } => {
                if preserve_stack {
                    self.emit_value(func, *condition);
                } else {
                    // Retain a resident condition while draining the rest. Materializing it first
                    // can duplicate an accessible copy only to swap and pop the original.
                    self.pop_stack_values_not_needed_by(&[*condition]);
                    self.emit_value(func, *condition);
                }

                match fallthrough {
                    Some(next) if *else_block == next => {
                        // JUMPI consumes the condition; false falls through to `else_block`.
                        self.emit_push_label(self.block_labels[then_block]);
                        self.asm.emit_op(op::JUMPI);
                        self.scheduler.stack.pop(); // condition consumed by JUMPI
                    }
                    Some(next) if *then_block == next => {
                        // Invert the condition so true falls through to `then_block`.
                        self.asm.emit_op(op::ISZERO);
                        self.scheduler.instruction_executed_untracked(1);
                        self.emit_push_label(self.block_labels[else_block]);
                        self.asm.emit_op(op::JUMPI);
                        self.scheduler.stack.pop(); // inverted condition consumed by JUMPI
                    }
                    _ => {
                        // Neither target falls through. Route the likely-hot
                        // edge through JUMPI (16 gas) and leave the cold
                        // revert path on the trailing unconditional jump,
                        // instead of paying JUMPI + JUMP (24 gas) on the hot
                        // path.
                        if self.block_is_cold(*then_block) && !self.block_is_cold(*else_block) {
                            self.asm.emit_op(op::ISZERO);
                            self.scheduler.instruction_executed_untracked(1);
                            self.emit_push_label(self.block_labels[else_block]);
                            self.asm.emit_op(op::JUMPI);
                            self.scheduler.stack.pop(); // inverted condition consumed by JUMPI

                            self.emit_push_label(self.block_labels[then_block]);
                            self.asm.emit_op(op::JUMP);
                        } else {
                            // JUMPI consumes the condition
                            self.emit_push_label(self.block_labels[then_block]);
                            self.asm.emit_op(op::JUMPI);
                            self.scheduler.stack.pop(); // condition consumed by JUMPI

                            self.emit_push_label(self.block_labels[else_block]);
                            self.asm.emit_op(op::JUMP);
                        }
                    }
                }
            }

            Terminator::Switch { value, default, cases } => {
                self.emit_switch_terminator(
                    func,
                    *value,
                    *default,
                    cases,
                    fallthrough,
                    preserve_stack,
                );
            }

            Terminator::Return { values } => {
                if self.in_internal_function {
                    self.emit_internal_return(func, values);
                    return;
                }

                assert!(values.is_empty(), "external ABI returns with values must use ReturnData");
                self.emit_external_stop(func);
            }

            Terminator::Revert { offset, size } => {
                self.emit_value(func, *size);
                self.emit_operand(func, *offset);
                self.asm.emit_op(op::REVERT);
                self.mark_debug_function_exit(func, DebugFunctionExit::Revert);
            }

            Terminator::RevertReturndata => self.emit_revert_returndata(),

            Terminator::ReturnData { offset, size } => {
                // Valid in internal functions too: a fused external body called
                // through an ABI wrapper returns straight to the external
                // caller, abandoning the internal frame.
                self.emit_value(func, *size);
                self.emit_operand(func, *offset);
                self.asm.emit_op(op::RETURN);
                self.mark_debug_function_exit(func, DebugFunctionExit::Return);
            }

            Terminator::Stop => {
                if self.in_internal_function {
                    self.emit_internal_return(func, &[]);
                } else {
                    self.emit_external_stop(func);
                }
            }

            Terminator::SelfDestruct { recipient } => {
                self.emit_value(func, *recipient);
                self.asm.emit_op(op::SELFDESTRUCT);
                self.mark_debug_function_exit(func, DebugFunctionExit::Return);
            }

            Terminator::Invalid => {
                self.asm.emit_op(op::INVALID);
            }
        }
    }
}

/// The artifact produced by the EVM backend.
#[derive(Clone, Debug, Default)]
pub struct EvmArtifact {
    /// Deployment (init) bytecode that, when run, returns the runtime code.
    pub deployment: Vec<u8>,
    /// Runtime bytecode, i.e. the code stored on-chain.
    pub runtime: Vec<u8>,
    /// Immutable placeholders in the runtime bytecode.
    pub(crate) immutable_references: Vec<ImmutableRef>,
    /// Final deployment-prefix EVM IR immediately before byte emission.
    pub deployment_evm_ir: Option<ir::Module>,
    /// Final runtime EVM IR immediately before byte emission.
    pub runtime_evm_ir: Option<ir::Module>,
    /// Final deployment-prefix instruction locations.
    pub deployment_debug_info: Option<Vec<DebugInstruction>>,
    /// Final runtime instruction locations.
    pub runtime_debug_info: Option<Vec<DebugInstruction>>,
}

impl crate::backend::Backend for EvmCodegen<'_> {
    type Output = EvmArtifact;

    fn lower_module(&mut self, module: &mut Module) -> EvmArtifact {
        self.generate_deployment_artifact(module)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{
        DataRef, FunctionBuilder, Immediate, Instruction, MirType, TypeSize, Value,
        utils as mir_utils,
    };
    use solar_config::{CompileOpts, EvmVersion};
    use solar_interface::{Ident, Session, sym};
    use solar_sema::{Compiler, hir::Visibility};

    #[test]
    fn constructor_memory_regions_do_not_overlap() {
        let mut module = Module::new(Ident::with_dummy_span(sym::Test));
        let mut constructor = Function::new(Ident::with_dummy_span(sym::Test));
        constructor.attributes.is_constructor = true;
        constructor.internal_frame_size = 0x3000;
        module.add_function(constructor);
        let id = module.add_immutable(
            Ident::with_dummy_span(sym::x),
            MirType::UInt(TypeSize::new_int_bits(8)),
            None,
        );
        let staging_base = immutable_staging_base(&module);
        assert_eq!(staging_base, 0x3080);
        let runtime_len = staging_base as usize;

        let full_word = ImmutableRef {
            id,
            code_offset: runtime_len - 33,
            type_size: TypeSize::new_int_bits(256),
        };
        assert_eq!(EvmCodegen::runtime_copy_base(&module, runtime_len, &[full_word]), 0);

        let short =
            ImmutableRef { id, code_offset: runtime_len - 2, type_size: TypeSize::new_int_bits(8) };
        assert_eq!(EvmCodegen::runtime_copy_base(&module, runtime_len, &[short]), 0);

        let short = ImmutableRef {
            id,
            code_offset: runtime_len - 3,
            type_size: TypeSize::new_int_bits(16),
        };
        assert_eq!(
            EvmCodegen::runtime_copy_base(&module, runtime_len, &[short]),
            immutable_staging_end(staging_base, 1)
        );

        with_codegen(CompileOpts::default(), |mut codegen| {
            codegen.immutable_staging_base = staging_base;
            assert_eq!(codegen.constructor_spill_base(0), staging_base);
            assert_eq!(codegen.constructor_spill_base(1), immutable_staging_end(staging_base, 1));
            assert_eq!(
                codegen.constructor_fixed_memory_end(257, 0),
                immutable_staging_end(staging_base, 257)
            );
            assert_eq!(
                codegen.constructor_fixed_memory_end(1, 0x2000),
                immutable_staging_end(staging_base, 1) + 0x2000
            );
        });
    }

    fn with_codegen<T: Send>(opts: CompileOpts, f: impl FnOnce(EvmCodegen<'_>) -> T + Send) -> T {
        let compiler = Compiler::new(Session::builder().opts(opts).build());
        compiler.enter(|c| f(EvmCodegen::new(c.gcx())))
    }

    #[test]
    fn codegen_reuses_module_state() {
        with_codegen(CompileOpts::default(), |mut codegen| {
            let mut module = Module::new(Ident::DUMMY);
            let mut entry = Function::new(Ident::DUMMY);
            FunctionBuilder::new(&mut entry).stop();
            let entry = module.add_function(entry);
            module.set_dispatch_entry(entry);
            module.advance_phase(MirPhase::EvmShaped);

            let mut first_module = module.clone();
            let first = codegen.generate_deployment_bytecode(&mut first_module);
            let mut second_module = module.clone();
            let second = codegen.generate_deployment_bytecode(&mut second_module);

            assert_eq!(second, first);
        });
    }

    #[test]
    fn static_frames_reject_explicit_signature_addresses() {
        let make_function = |offset| {
            let mut function = Function::new(Ident::DUMMY);
            function.alloc_param(MirType::uint256());
            function.internal_frame_size = EvmMemoryLayout::WORD_SIZE;
            let (inst, _) = function.alloc_value_inst(Instruction::new(
                InstKind::InternalFrameAddr(offset),
                Some(MirType::MemPtr),
            ));
            function.blocks[BlockId::ENTRY].instructions.push(inst);
            function
        };

        assert!(!EvmCodegen::static_frame_offsets_are_local(&make_function(0)));
        assert!(!EvmCodegen::static_frame_offsets_are_local(&make_function(
            EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
        )));
        assert!(EvmCodegen::static_frame_offsets_are_local(&make_function(
            EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE + EvmMemoryLayout::WORD_SIZE
        )));
        assert!(!EvmCodegen::static_frame_offsets_are_local(&make_function(u64::MAX)));
    }

    #[test]
    fn data_copy_reaches_destination_before_relocation_push() {
        with_codegen(CompileOpts::default(), |mut codegen| {
            let mut module = Module::new(Ident::DUMMY);
            module.phase = MirPhase::EvmShaped;
            let data = module.add_data(vec![0; WORD_BYTES].into(), None);

            let mut function = Function::new(Ident::DUMMY);
            let mut builder = FunctionBuilder::new(&mut function);
            let one = builder.imm(1);
            let dest = builder.add(one, one);
            let size = builder.imm(WORD_BYTES as u64);
            builder.data_copy(DataRef::new(data, 0), dest, size);
            builder.stop();
            let function = module.add_function(function);

            codegen.asm.load_data(&module);
            let function = &module.functions[function];
            let liveness = Liveness::compute(function);
            codegen.scheduler.stack.push(dest);
            for _ in 0..MAX_STACK_ACCESS - 2 {
                codegen.scheduler.stack.push_unknown();
            }
            assert_eq!(codegen.scheduler.stack.find(dest), Some(MAX_STACK_ACCESS - 2));

            codegen.emit_data_copy(
                function,
                DataRef::new(data, 0),
                dest,
                size,
                &liveness,
                BlockId::ENTRY,
                1,
            );

            assert_eq!(codegen.scheduler.stack.find(dest), Some(MAX_STACK_ACCESS - 2));
        });
    }

    #[test]
    fn data_copy_participates_in_memory_analysis() {
        let data = DataRef::new(crate::mir::DataId::from_usize(0), 0);

        let mut constant = Function::new(Ident::DUMMY);
        let dest = constant.alloc_value(Value::Immediate(Immediate::uint256(U256::from(0x40))));
        let size = constant.alloc_value(Value::Immediate(Immediate::uint256(U256::from(0x20))));
        let inst =
            constant.alloc_inst(Instruction::new(InstKind::DataCopy(data, dest, size), None));
        constant.blocks[BlockId::ENTRY].instructions.push(inst);
        assert_eq!(EvmCodegen::constant_memory_high_water_mark(&constant), 0x60);
        assert!(EvmCodegen::function_may_observe_free_memory_slot(&constant));
        assert!(mir_utils::is_memory_inst(&constant.inst(inst).kind));

        let mut dynamic = Function::new(Ident::DUMMY);
        let dest = dynamic.alloc_param(MirType::MemPtr);
        let size = dynamic.alloc_param(MirType::uint256());
        let inst = dynamic.alloc_inst(Instruction::new(InstKind::DataCopy(data, dest, size), None));
        dynamic.blocks[BlockId::ENTRY].instructions.push(inst);
        assert_eq!(EvmCodegen::dynamic_spill_write_dest(&dynamic, inst), Some(dest));
    }

    #[test]
    fn caller_stack_prefix_validation_rejects_overflow() {
        with_codegen(CompileOpts::default(), |mut codegen| {
            let mut module = Module::new(Ident::DUMMY);
            let entry = module.add_function(Function::new(Ident::with_dummy_span(sym::entry)));
            module.set_dispatch_entry(entry);
            let callee = module.add_function(Function::new(Ident::with_dummy_span(sym::Test)));
            let mut constructor = Function::new(Ident::DUMMY);
            constructor.attributes.is_constructor = true;
            let constructor = module.add_function(constructor);

            codegen.recursive_stack_functions = DenseBitSet::new_empty(module.functions.len());
            codegen.function_stack_peaks.insert(entry, 1);
            codegen.function_stack_peaks.insert(callee, MAX_STACK_DEPTH - 1);
            codegen.icall_stack_edges.push(ICallStackEdge {
                caller: entry,
                callee,
                preserved_words: 1,
                argument_words: 0,
            });
            assert!(!codegen.caller_stack_prefixes_fit(&module, MAX_STACK_DEPTH));

            codegen.function_stack_peaks.insert(callee, MAX_STACK_DEPTH - 2);
            assert!(codegen.caller_stack_prefixes_fit(&module, MAX_STACK_DEPTH));

            // The transient argument tuple and target label must be budgeted even
            // when the preserved prefix and callee peak fit on their own.
            codegen.icall_stack_edges[0].preserved_words = MAX_STACK_DEPTH - 3;
            codegen.icall_stack_edges[0].argument_words = 2;
            codegen.function_stack_peaks.insert(callee, 2);
            assert!(!codegen.caller_stack_prefixes_fit(&module, MAX_STACK_DEPTH));

            codegen.icall_stack_edges[0].preserved_words = 0;
            codegen.icall_stack_edges[0].argument_words = MAX_STACK_DEPTH;
            codegen.function_stack_peaks.insert(callee, 0);
            assert!(!codegen.caller_stack_prefixes_fit(&module, MAX_STACK_DEPTH));

            codegen.icall_stack_edges[0] = ICallStackEdge {
                caller: constructor,
                callee,
                preserved_words: 1,
                argument_words: 0,
            };
            codegen.function_stack_peaks.insert(callee, MAX_STACK_DEPTH - 1);
            assert!(!codegen.stack_prefixes_fit_from(&module, constructor, MAX_STACK_DEPTH));
        });
    }

    #[test]
    fn label_push_extends_the_scheduler_peak() {
        with_codegen(CompileOpts::default(), |mut codegen| {
            for _ in 0..MAX_STACK_DEPTH {
                codegen.scheduler.stack.push_unknown();
            }

            let label = codegen.asm.new_label();
            codegen.emit_push_label(label);

            assert_eq!(codegen.scheduler.stack.max_depth(), MAX_STACK_DEPTH + 1);
        });
    }

    #[test]
    fn removing_instructions_keeps_label_relocations() {
        with_codegen(CompileOpts::default(), |mut codegen| {
            let label = codegen.asm.new_label();
            let (block, start) = codegen.asm.next_instruction_position();
            codegen.asm.emit_op(op::ADD);
            codegen.emit_push_label(label);
            codegen.asm.remove_instructions(&mut [(block, start..start + 1)]);
            codegen.asm.define_label(label);

            let (module, _) = codegen.asm.finish_evm_ir().unwrap();
            assert_eq!(
                module.blocks[ir::BlockId::ENTRY].instructions[0].pushed_block(),
                Some(ir::BlockId::from_usize(1))
            );
        });
    }

    #[test]
    fn irreducible_runtime_stack_overflow_terminates() {
        with_codegen(CompileOpts::default(), |mut codegen| {
            let mut module = Module::new(Ident::DUMMY);
            for index in 0..=MAX_STACK_DEPTH {
                let mut function = Function::new(Ident::DUMMY);
                let mut builder = FunctionBuilder::new(&mut function);
                if index < MAX_STACK_DEPTH {
                    builder.icall_void(FunctionId::from_usize(index + 1), Vec::new(), 0);
                }
                builder.stop();
                let function = module.add_function(function);
                if index == 0 {
                    module.set_dispatch_entry(function);
                }
            }
            module.advance_phase(MirPhase::EvmShaped);
            let call_graph = CallGraphInfo::new(&module);
            codegen.cold_functions = DenseBitSet::new_empty(module.functions.len());

            let _ = codegen.generate_runtime_code(&module, &call_graph);

            assert!(!codegen.stack_returns_enabled);
            assert!(codegen.gcx.dcx().has_errors().is_err());
        });
    }

    #[test]
    fn dynamic_frame_stack_args_allow_raw_values() {
        let mut function = Function::new(Ident::DUMMY);
        let argument = function.alloc_param(MirType::uint256());
        let immediate = function.alloc_value(Value::Immediate(Immediate::uint256(U256::from(1))));
        let (_, computed) = function.alloc_value_inst(Instruction::new(
            InstKind::Add(argument, immediate),
            Some(MirType::uint256()),
        ));
        let (_, calldata_size) = function
            .alloc_value_inst(Instruction::new(InstKind::CalldataSize, Some(MirType::uint256())));

        assert!(EvmCodegen::stack_arg_site_eligible(&function, false, immediate));
        assert!(!EvmCodegen::stack_arg_site_eligible(&function, false, argument));
        assert!(EvmCodegen::stack_arg_site_eligible(&function, false, computed));
        assert!(EvmCodegen::raw_arg_emittable(&function, false, calldata_size));
        assert!(EvmCodegen::stack_arg_site_eligible(&function, true, argument));
        assert!(EvmCodegen::stack_arg_site_eligible(&function, true, computed));

        with_codegen(CompileOpts::default(), |mut codegen| {
            codegen.emit_raw_stack_arg(&function, calldata_size, None, None, 0);
            assert_eq!(codegen.asm.assemble().bytecode, [op::CALLDATASIZE]);
        });
    }

    #[test]
    fn spill_elision_requires_uniform_successor_residency() {
        let mut function = Function::new(Ident::DUMMY);
        let condition = function.alloc_value(Value::Immediate(Immediate::bool(true)));
        let first = function.alloc_value(Value::Immediate(Immediate::uint256(U256::from(1))));
        let second = function.alloc_value(Value::Immediate(Immediate::uint256(U256::from(2))));
        let then_block = function.alloc_block();
        let else_block = function.alloc_block();
        let term = Terminator::Branch { condition, then_block, else_block };
        let mut plan = GlobalStackPlan {
            entries: FxHashMap::from_iter([
                (then_block, vec![first, second]),
                (else_block, vec![first]),
            ]),
            aliases: FxHashMap::default(),
            terminal_sensitive: true,
        };

        assert_eq!(plan.uniformly_carried_values(&function, &term), [first]);
        plan.entries.insert(else_block, vec![first, second]);
        assert_eq!(plan.uniformly_carried_values(&function, &term), [first, second]);

        // Switch layouts intersect across the default and every case target.
        let case_block = function.alloc_block();
        let switch = Terminator::Switch {
            value: condition,
            default: else_block,
            cases: vec![(condition, case_block)],
        };
        plan.entries.insert(case_block, vec![second]);
        assert_eq!(plan.uniformly_carried_values(&function, &switch), [second]);
        plan.entries.insert(case_block, vec![first, second]);
        assert_eq!(plan.uniformly_carried_values(&function, &switch), [first, second]);
    }

    #[test]
    fn icall_headroom_includes_return_label() {
        let value = ValueId::from_usize(0);
        let call = InstKind::ICall {
            function: FunctionId::from_usize(0),
            args: vec![value; MAX_STACK_ACCESS].into(),
            returns: 0,
        };
        assert_eq!(
            EvmCodegen::instruction_transient_growth(&call, MAX_STACK_ACCESS),
            MAX_STACK_ACCESS
        );

        let add = InstKind::Add(value, value);
        assert_eq!(EvmCodegen::instruction_transient_growth(&add, 2), 1);
    }

    #[test]
    fn one_operand_terminators_check_stack_arg_reach() {
        let value = ValueId::from_usize(0);
        let branch = Terminator::Branch {
            condition: value,
            then_block: BlockId::from_usize(1),
            else_block: BlockId::from_usize(2),
        };
        assert_eq!(EvmCodegen::terminator_transient_growth(&branch), 1);

        let return_value = Terminator::Return { values: smallvec::smallvec![value] };
        assert_eq!(EvmCodegen::terminator_transient_growth(&return_value), 1);
        assert_eq!(EvmCodegen::terminator_transient_growth(&Terminator::Stop), 0);
    }

    #[test]
    fn resident_phi_merge_rejects_inaccessible_layout() {
        let mut function = Function::new(Ident::DUMMY);
        let join = function.alloc_block();
        let mut phi = StackPhiPlan::default();
        phi.entries.insert(join, (0..MAX_STACK_ACCESS).map(ValueId::from_usize).collect());
        let resident = GlobalStackPlan {
            entries: FxHashMap::from_iter([(join, vec![ValueId::from_usize(MAX_STACK_ACCESS)])]),
            aliases: FxHashMap::default(),
            terminal_sensitive: true,
        };

        assert!(!phi.merge_resident(&function, &resident));
        assert_eq!(phi.entries[&join].len(), MAX_STACK_ACCESS);
    }

    #[test]
    fn materialized_stack_only_args_use_frame_on_retry() {
        let opts = CompileOpts { optimization: OptimizationMode::Gas, ..Default::default() };
        with_codegen(opts, |mut codegen| {
            let mut module = Module::new(Ident::DUMMY);
            let mut function = Function::new(Ident::DUMMY);
            let argument = function.alloc_param(MirType::uint256());
            let function = module.add_function(function);

            codegen.static_call_abi_mut(function, 1).stack_args.insert(0);
            codegen.static_frame_functions = DenseBitSet::new_empty(module.functions.len());
            codegen.static_frame_functions.insert(function);
            codegen.disabled_stack_only_functions = DenseBitSet::new_empty(module.functions.len());
            codegen.disabled_stack_only_functions.insert(function);

            let arg_values = FxHashMap::from_iter([(
                function,
                CanonicalArgValues::from_vec(vec![Some(argument)]),
            )]);
            let use_info = FxHashMap::from_iter([(
                function,
                StackArgUseInfo {
                    use_counts: FxHashMap::from_iter([(argument, 1)]),
                    non_entry_uses: DenseBitSet::new_empty(1),
                    call_uses: DenseBitSet::new_empty(1),
                    entry_first_uses: FxHashMap::from_iter([(argument, 0)]),
                    first_entry_call: None,
                },
            )]);

            codegen.compute_lazy_stack_args(&module, &arg_values, &use_info);
            codegen.compute_direct_stack_args(&module, &arg_values, &use_info);
            assert!(codegen.lazy_stack_args(function).is_none());
            assert!(codegen.direct_stack_args(function).is_none());
        });
    }

    #[test]
    fn stack_return_compacts_offsets_after_stack_arg_fallback() {
        let opts = CompileOpts { optimization: OptimizationMode::Gas, ..Default::default() };
        with_codegen(opts, |mut codegen| {
            let mut module = Module::new(Ident::DUMMY);
            let mut function = Function::new(Ident::with_dummy_span(sym::Test));
            function.internal_frame_size = EvmMemoryLayout::WORD_SIZE;
            let mut builder = FunctionBuilder::new(&mut function);
            let argument = builder.add_param(MirType::uint256());
            builder.add_return(MirType::uint256());
            builder.ret([argument]);
            let function = module.add_function(function);

            codegen.static_frame_functions = DenseBitSet::new_empty(module.functions.len());
            codegen.static_frame_functions.insert(function);
            codegen.disabled_stack_only_functions = DenseBitSet::new_empty(module.functions.len());
            codegen.recursive_frame_functions = DenseBitSet::new_empty(module.functions.len());
            codegen.function_spill_sizes.insert(function, 0);
            codegen.runtime_stack_args = false;
            codegen.compute_stack_return_plans(&module);

            let local =
                EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE + 2 * EvmMemoryLayout::WORD_SIZE;
            assert!(codegen.stack_return_plan(function).is_some());
            assert_eq!(
                codegen.compact_static_frame_offset(function, local),
                local - EvmMemoryLayout::WORD_SIZE
            );
            assert_eq!(codegen.emitted_frame_size(&module, function), local);
        });
    }

    #[test]
    fn direct_stack_args_reject_switch_terminators() {
        let opts = CompileOpts { optimization: OptimizationMode::Gas, ..Default::default() };
        with_codegen(opts, |mut codegen| {
            let mut module = Module::new(Ident::DUMMY);
            let mut function = Function::new(Ident::with_dummy_span(sym::Test));
            let mut builder = FunctionBuilder::new(&mut function);
            let argument = builder.add_param(MirType::uint256());
            let one = builder.imm(1);
            let _unrelated = builder.add(one, one);
            let _use = builder.add(argument, one);
            let default = builder.create_block();
            let case = builder.create_block();
            builder.switch(argument, default, vec![(one, case)]);
            builder.switch_to_block(default);
            builder.stop();
            builder.switch_to_block(case);
            builder.stop();
            let function = module.add_function(function);

            codegen.static_call_abi_mut(function, 1).stack_args.insert(0);
            codegen.disabled_stack_only_functions = DenseBitSet::new_empty(module.functions.len());
            let arg_values = codegen.collect_canonical_stack_arg_values(&module);
            let use_info = codegen.collect_stack_arg_uses(&module);
            codegen.compute_direct_stack_args(&module, &arg_values, &use_info);

            assert!(codegen.direct_stack_args(function).is_none());
        });
    }

    #[test]
    fn resident_layout_selection_is_pinned_across_runs() {
        // `select_resident_layout` weighs runtime gas against deploy bytes
        // through `optimizer_runs`. No current cost shape flips the choice
        // (an eligible stack-riding value dominates the frame convention in
        // both dimensions), so this pins the selection at both extremes:
        // a cost-model change that silently alters layout choices, or makes
        // them run-count-unstable, must show up here as an intentional edit.
        let select = |runs: u64| {
            let opts = CompileOpts {
                optimization: OptimizationMode::Gas,
                optimizer_runs: Some(runs),
                ..Default::default()
            };
            with_codegen(opts, |codegen| {
                let mut function = Function::new(Ident::DUMMY);
                let argument = function.alloc_param(MirType::uint256());
                let mut builder = FunctionBuilder::new(&mut function);
                let one = builder.imm(1);
                let blocks: Vec<_> = (0..5).map(|_| builder.create_block()).collect();
                builder.jump(blocks[0]);
                for (index, &block) in blocks.iter().enumerate() {
                    builder.switch_to_block(block);
                    if let Some(&next) = blocks.get(index + 1) {
                        builder.jump(next);
                    }
                }
                let acc = builder.add(argument, one);
                builder.ret([acc]);
                let liveness = Liveness::compute(&function);
                codegen
                    .select_resident_layout(&function, &liveness, &[argument], false, false)
                    .map(|(values, _)| values)
            })
        };

        let deploy_dominated = select(1);
        let runtime_dominated = select(200_000);
        assert_eq!(deploy_dominated, select(1));
        assert_eq!(runtime_dominated, select(200_000));
        assert_eq!(deploy_dominated.as_deref(), Some(&[ValueId::from_usize(0)][..]));
        assert_eq!(runtime_dominated.as_deref(), Some(&[ValueId::from_usize(0)][..]));
    }

    #[test]
    fn free_memory_slot_overlap_is_conservative() {
        let overlaps = EvmCodegen::constant_memory_range_may_overlap_fmp;

        assert!(!overlaps(Some(0x20), Some(0x20)));
        assert!(!overlaps(Some(0x3f), Some(1)));
        assert!(overlaps(Some(0x3f), Some(2)));
        assert!(overlaps(Some(0x40), Some(0x20)));
        assert!(overlaps(Some(0x5f), Some(1)));
        assert!(!overlaps(Some(0x60), None));
        assert!(!overlaps(None, Some(0)));
        assert!(overlaps(None, Some(1)));
        assert!(overlaps(Some(0), None));
        assert!(overlaps(Some(0x20), Some(u64::MAX)));
    }

    #[test]
    fn empty_external_return_falls_off_end() {
        with_codegen(CompileOpts::default(), |mut codegen| {
            let mut function = Function::new(Ident::with_dummy_span(sym::Test));
            function.attributes.visibility = Visibility::External;
            FunctionBuilder::new(&mut function).ret(Vec::new());
            codegen.generate_function_body(FunctionId::from_usize(0), &function);

            assert!(codegen.asm.assemble().bytecode.is_empty());
        });
    }

    #[test]
    fn nullary_reads_have_expected_rematerialization_opcodes() {
        let mut function = Function::new(Ident::with_dummy_span(sym::Test));
        for (kind, expected_op) in [
            (InstKind::CalldataSize, op::CALLDATASIZE),
            (InstKind::CodeSize, op::CODESIZE),
            (InstKind::Caller, op::CALLER),
            (InstKind::CallValue, op::CALLVALUE),
            (InstKind::Address, op::ADDRESS),
            (InstKind::Origin, op::ORIGIN),
            (InstKind::GasPrice, op::GASPRICE),
            (InstKind::Coinbase, op::COINBASE),
            (InstKind::Timestamp, op::TIMESTAMP),
            (InstKind::BlockNumber, op::NUMBER),
            (InstKind::PrevRandao, op::PREVRANDAO),
            (InstKind::GasLimit, op::GASLIMIT),
            (InstKind::SlotNum, op::SLOTNUM),
            (InstKind::ChainId, op::CHAINID),
            (InstKind::BaseFee, op::BASEFEE),
            (InstKind::BlobBaseFee, op::BLOBBASEFEE),
        ] {
            let (_, value) =
                function.alloc_value_inst(Instruction::new(kind, Some(MirType::uint256())));
            assert_eq!(EvmCodegen::always_rematerializable_op(&function, value), Some(expected_op));
            assert!(!EvmCodegen::can_own_spill_slot(&function, value));
        }

        for kind in
            [InstKind::MSize, InstKind::ReturnDataSize, InstKind::SelfBalance, InstKind::Gas]
        {
            let (_, value) =
                function.alloc_value_inst(Instruction::new(kind, Some(MirType::uint256())));
            assert_eq!(EvmCodegen::always_rematerializable_op(&function, value), None);
            assert!(EvmCodegen::can_own_spill_slot(&function, value));
        }
    }

    #[test]
    fn deep_spill_exposes_one_word_at_each_target_limit() {
        for evm_version in [EvmVersion::Osaka, EvmVersion::Amsterdam] {
            let opts = CompileOpts { evm_version, ..Default::default() };
            with_codegen(opts, |mut codegen| {
                let mut function = Function::new(Ident::with_dummy_span(sym::Test));
                let lhs = function.alloc_value(Value::Immediate(Immediate::uint256(U256::ZERO)));
                let rhs = function.alloc_value(Value::Immediate(Immediate::uint256(U256::ONE)));
                let (_, target) = function.alloc_value_inst(Instruction::new(
                    InstKind::Add(lhs, rhs),
                    Some(MirType::uint256()),
                ));
                codegen.scheduler.stack.push(target);
                for _ in 0..evm_version.reachable_stack_depth() {
                    let (_, filler) = function.alloc_value_inst(Instruction::new(
                        InstKind::Add(lhs, rhs),
                        Some(MirType::uint256()),
                    ));
                    codegen.scheduler.stack.push(filler);
                }
                let before = codegen.scheduler.stack.as_slice().to_vec();

                codegen.spill_value_if_needed(&function, target);

                assert_eq!(codegen.scheduler.stack.as_slice(), before);
                assert!(codegen.scheduler.spills.is_stored(target));
                assert_eq!(codegen.scheduler.spills.spill_area_size(), 64);
            });
        }
    }

    #[test]
    fn unreachable_phi_copies_do_not_leak_between_functions() {
        with_codegen(CompileOpts::default(), |mut codegen| {
            let mut first = Function::new(Ident::with_dummy_span(sym::Test));
            let mut builder = FunctionBuilder::new(&mut first);
            let unreachable_pred = builder.create_block();
            let unreachable_merge = builder.create_block();
            builder.stop();
            builder.switch_to_block(unreachable_pred);
            let value = builder.imm(1);
            builder.jump(unreachable_merge);
            builder.switch_to_block(unreachable_merge);
            let value = builder.phi(vec![(unreachable_pred, value)]);
            builder.ret([value]);

            codegen.generate_function_body(FunctionId::from_usize(0), &first);
            assert!(codegen.block_copies.contains_key(&unreachable_pred));

            let mut second = Function::new(Ident::with_dummy_span(sym::Test));
            FunctionBuilder::new(&mut second).stop();
            codegen.generate_function_body(FunctionId::from_usize(1), &second);

            assert!(codegen.block_copies.is_empty());
        });
    }

    #[test]
    fn cross_block_reload_excludes_phi_edge_uses() {
        let mut function = Function::new(Ident::DUMMY);
        let immediate = function.alloc_value(Value::Immediate(Immediate::uint256(U256::from(1))));
        let (edge_inst, edge_value) = function.alloc_value_inst(Instruction::new(
            InstKind::Add(immediate, immediate),
            Some(MirType::uint256()),
        ));
        let (direct_inst, direct_value) = function.alloc_value_inst(Instruction::new(
            InstKind::Mul(immediate, immediate),
            Some(MirType::uint256()),
        ));
        function.blocks[BlockId::ENTRY].instructions.extend([edge_inst, direct_inst]);

        let phi_block = function.alloc_block();
        let (phi_inst, _) = function.alloc_value_inst(Instruction::new(
            InstKind::Phi(vec![(BlockId::ENTRY, edge_value)]),
            Some(MirType::uint256()),
        ));
        function.blocks[phi_block].instructions.push(phi_inst);

        let direct_block = function.alloc_block();
        let (use_inst, _) = function.alloc_value_inst(Instruction::new(
            InstKind::Add(direct_value, immediate),
            Some(MirType::uint256()),
        ));
        function.blocks[direct_block].instructions.push(use_inst);

        let reloaded = EvmCodegen::cross_block_reload_values(&function);
        assert!(!reloaded.contains(edge_value));
        assert!(reloaded.contains(direct_value));
    }

    #[test]
    fn spill_color_accepts_only_disjoint_ranges() {
        let block0 = BlockId::from_usize(0);
        let block1 = BlockId::from_usize(1);
        let value0 = ValueId::from_usize(0);
        let value1 = ValueId::from_usize(1);
        let interferences = FxHashMap::default();
        let mut color = SpillColor::new(2);
        color
            .insert(value0, &FxHashMap::from_iter([(block0, SpillLiveRange { start: 2, end: 4 })]));

        assert!(color.accepts(
            value1,
            &FxHashMap::from_iter([(block0, SpillLiveRange { start: 5, end: 7 })]),
            &interferences,
        ));
        assert!(!color.accepts(
            value1,
            &FxHashMap::from_iter([(block0, SpillLiveRange { start: 4, end: 7 })]),
            &interferences,
        ));
        assert!(color.accepts(
            value1,
            &FxHashMap::from_iter([(block1, SpillLiveRange { start: 2, end: 4 })]),
            &interferences,
        ));
    }

    #[test]
    fn parallel_phi_interference_follows_copy_order() {
        let source0 = ValueId::from_usize(0);
        let destination0 = ValueId::from_usize(1);
        let source1 = ValueId::from_usize(2);
        let destination1 = ValueId::from_usize(3);
        let mut colorable = DenseBitSet::new_empty(4);
        colorable.insert_all();
        let block_copies = FxHashMap::from_iter([(
            BlockId::ENTRY,
            vec![
                ParallelCopy {
                    src: CopySource::Value(source0),
                    dst: CopyDest::Value(destination0),
                    ty: MirType::uint256(),
                },
                ParallelCopy {
                    src: CopySource::Value(source1),
                    dst: CopyDest::Value(destination1),
                    ty: MirType::uint256(),
                },
            ],
        )]);

        let function = Function::new(Ident::DUMMY);
        let liveness = Liveness::compute(&function);
        let interferences =
            EvmCodegen::parallel_phi_interferences(&function, &liveness, &colorable, &block_copies);
        assert!(interferences[&destination0].contains(&destination1));
        assert!(interferences[&destination0].contains(&source1));
        assert!(!interferences[&destination0].contains(&source0));
        assert!(!interferences[&destination1].contains(&source0));
    }
}
