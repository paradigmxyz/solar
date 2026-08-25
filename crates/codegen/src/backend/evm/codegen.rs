//! EVM bytecode generation from MIR.
//!
//! This module generates EVM bytecode from MIR using:
//! - Liveness analysis to know when values die
//! - Phi elimination to convert SSA to parallel copies
//! - Stack scheduling to generate DUP/SWAP sequences
//! - EVM IR optimization, relocation, and byte encoding

use super::{
    EVM_WORD_BYTES,
    assembler::{
        ArtifactKind, Assembler, DeferredAlloc, DeferredConst, ImmutableRef, Label,
        PreparedAssembly,
    },
    ir,
    layout::{RelayoutAddress, preserves_push_width},
    op,
    stack::{
        MAX_STACK_ACCESS, MAX_STACK_DEPTH, OperandCostModel, OperandPlan, ScheduleCost,
        ScheduledOp, SpillSlot, StackModel, StackOp, StackScheduler, TargetSlot,
    },
};
use crate::{
    analysis::{
        CallGraphInfo, CfgInfo, CopyDest, CopySource, Liveness, Loop, LoopAnalyzer, ParallelCopy,
        PhiEliminator,
    },
    immutable::{
        immutable_push_type_size, immutable_staging_addr, immutable_staging_base,
        immutable_staging_end,
    },
    memory::EvmMemoryLayout,
    mir::{
        ArgIdx, BlockId, EffectKind, Function, FunctionId, ImmutableEncoding, ImmutableId, InstId,
        InstKind, MirPhase, Module, Terminator, ValueId,
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
use solar_interface::sym;
use solar_sema::Gcx;

mod switch;

use self::switch::MAX_GAS_CODE_GROWTH;

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

impl LazyStackArgPlan {
    fn values(&self) -> impl Iterator<Item = ValueId> + '_ {
        self.args.iter().map(|&(_, value)| value)
    }
}

/// A profitable static-call layout whose caller words stay below the
/// untracked return address until control returns.
#[derive(Clone, Debug)]
struct StaticCallStackPlan {
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
struct InternalCallStackEdge {
    caller: FunctionId,
    callee: FunctionId,
    preserved_words: usize,
    argument_words: usize,
}

#[derive(Clone, Debug, Default)]
struct StackPhiPlan {
    entries: FxHashMap<BlockId, Vec<ValueId>>,
    edges: FxHashMap<BlockId, StackPhiEdge>,
    branch_edges: FxHashMap<BlockId, StackPhiBranch>,
    edge_sources: FxHashMap<BlockId, Vec<ValueId>>,
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

fn union_values(first: &[ValueId], second: &[ValueId]) -> Vec<ValueId> {
    let mut union = first.to_vec();
    for &value in second {
        if !union.contains(&value) {
            union.push(value);
        }
    }
    union
}

#[derive(Clone, Copy, Debug)]
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
                    && (offset - 4) % 32 == 0
                    && let Ok(index) = u32::try_from((offset - 4) / 32)
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
        if values.is_empty() || values.len() > GLOBAL_STACK_LAYOUT_LIMIT {
            return None;
        }
        // Nested calls are eligible only when runtime emission can retain the live resident prefix
        // below their return address. Stack-phi edges compose their changing values above this
        // invariant prefix. The analysis remains deliberately all-or-nothing because resident
        // arguments cannot fall back to memory on just one edge.
        if func.blocks.iter().any(|block| {
            block.instructions.iter().any(|&inst_id| {
                !preserve_across_calls
                    && matches!(func.inst(inst_id).kind, InstKind::InternalCall { .. })
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
    fn analyze(func: &Function) -> Self {
        StackPhiPlanner::new(func).plan()
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
            self.edge_sources.insert(pred, edge.sources.clone());
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
            self.edge_sources.insert(pred, branch.union.clone());
        }
        true
    }
}

struct StackPhiPlanner<'a> {
    func: &'a Function,
    loops: Vec<Loop>,
    header_results: FxHashMap<BlockId, Vec<ValueId>>,
    definitions: IndexVec<ValueId, Option<BlockId>>,
}

impl<'a> StackPhiPlanner<'a> {
    fn new(func: &'a Function) -> Self {
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
        let mut planner = Self { func, loops, header_results: FxHashMap::default(), definitions };
        planner.collect_header_results();
        planner
    }

    fn plan(&self) -> StackPhiPlan {
        let mut plan = StackPhiPlan::default();
        for loop_info in &self.loops {
            self.plan_loop(loop_info, &mut plan);
        }
        self.plan_branch_phi_joins(&mut plan);
        for block in self.func.blocks.indices() {
            self.plan_join(block, &mut plan);
        }
        plan
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
            if then_block == else_block
                || loop_info.blocks.contains(*then_block) == loop_info.blocks.contains(*else_block)
                || plan.entries.contains_key(then_block)
                || plan.entries.contains_key(else_block)
            {
                continue;
            }
            if !self.branch_phi_shape_is_valid(loop_info, *then_block, *else_block) {
                continue;
            }

            let Some(then_results) = self.phi_results_for_only_block(*then_block) else {
                continue;
            };
            let Some(else_results) = self.phi_results_for_only_block(*else_block) else {
                continue;
            };
            if then_results.is_empty()
                || else_results.is_empty()
                || then_results.len() > STACK_PHI_LAYOUT_LIMIT
                || else_results.len() > STACK_PHI_LAYOUT_LIMIT
            {
                continue;
            }

            let mut predecessors = self.func.blocks[*then_block].predecessors.clone();
            for &pred in &self.func.blocks[*else_block].predecessors {
                if !predecessors.contains(&pred) {
                    predecessors.push(pred);
                }
            }
            if predecessors.is_empty()
                || predecessors.iter().any(|&pred| {
                    !loop_info.blocks.contains(pred)
                        || plan.edges.contains_key(&pred)
                        || plan.branch_edges.contains_key(&pred)
                        || !matches!(
                            self.func.blocks[pred].terminator,
                            Some(Terminator::Branch { then_block: t, else_block: e, .. })
                                if (t == *then_block && e == *else_block)
                                    || (t == *else_block && e == *then_block)
                        )
                })
            {
                continue;
            }

            let mut branch_edges = Vec::with_capacity(predecessors.len());
            let mut valid = true;
            for &pred in &predecessors {
                let Some(then_sources) = self.phi_sources_for_block_pred(*then_block, pred) else {
                    valid = false;
                    break;
                };
                let Some(else_sources) = self.phi_sources_for_block_pred(*else_block, pred) else {
                    valid = false;
                    break;
                };
                if then_sources.len() > MAX_STACK_ACCESS || else_sources.len() > MAX_STACK_ACCESS {
                    valid = false;
                    break;
                }
                branch_edges.push((
                    pred,
                    StackPhiBranch {
                        union: union_values(&then_sources, &else_sources),
                        then_edge: StackPhiEdge {
                            sources: then_sources,
                            results: then_results.clone(),
                        },
                        else_edge: StackPhiEdge {
                            sources: else_sources,
                            results: else_results.clone(),
                        },
                    },
                ));
            }
            if !valid {
                continue;
            }

            plan.entries.insert(*then_block, then_results);
            plan.entries.insert(*else_block, else_results);
            for (pred, branch) in branch_edges {
                plan.edge_sources.insert(pred, branch.union.clone());
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

    fn branch_phi_shape_is_valid(
        &self,
        loop_info: &Loop,
        then_block: BlockId,
        else_block: BlockId,
    ) -> bool {
        if then_block == else_block
            || loop_info.blocks.contains(then_block) == loop_info.blocks.contains(else_block)
        {
            return false;
        }
        let Some(then_results) = self.phi_results_for_only_block(then_block) else {
            return false;
        };
        let Some(else_results) = self.phi_results_for_only_block(else_block) else {
            return false;
        };
        if then_results.is_empty()
            || else_results.is_empty()
            || then_results.len() > STACK_PHI_LAYOUT_LIMIT
            || else_results.len() > STACK_PHI_LAYOUT_LIMIT
        {
            return false;
        }

        let mut predecessors = self.func.blocks[then_block].predecessors.clone();
        for &pred in &self.func.blocks[else_block].predecessors {
            if !predecessors.contains(&pred) {
                predecessors.push(pred);
            }
        }
        !predecessors.is_empty()
            && predecessors.iter().all(|&pred| {
                loop_info.blocks.contains(pred)
                    && matches!(
                        self.func.blocks[pred].terminator,
                        Some(Terminator::Branch { then_block: t, else_block: e, .. })
                            if (t == then_block && e == else_block)
                                || (t == else_block && e == then_block)
                    )
                    && self
                        .phi_sources_for_block_pred(then_block, pred)
                        .is_some_and(|sources| sources.len() <= MAX_STACK_ACCESS)
                    && self
                        .phi_sources_for_block_pred(else_block, pred)
                        .is_some_and(|sources| sources.len() <= MAX_STACK_ACCESS)
            })
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

    fn plan_loop(&self, loop_info: &Loop, plan: &mut StackPhiPlan) {
        let Some(preheader) = loop_info.preheader else {
            return;
        };
        let [latch] = loop_info.back_edges.as_slice() else {
            return;
        };
        if !matches!(self.func.blocks[preheader].terminator, Some(Terminator::Jump(target)) if target == loop_info.header)
            || !matches!(self.func.blocks[*latch].terminator, Some(Terminator::Jump(target)) if target == loop_info.header)
        {
            return;
        }
        if plan.edges.contains_key(&preheader) || plan.edges.contains_key(latch) {
            return;
        }
        let has_branching_body = loop_info.blocks.iter().any(|block_id| {
            block_id != loop_info.header
                && matches!(self.func.blocks[block_id].terminator, Some(Terminator::Branch { .. }))
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
        if !has_branching_body {
            self.extend_live_through_values(loop_info, &mut carry_through);
        }
        if carry_through.len() + results.len() > STACK_PHI_LAYOUT_LIMIT {
            return;
        }
        let mut entry = carry_through.clone();
        entry.extend(results.iter().copied());

        let predecessors = [preheader, *latch];
        let mut edges = Vec::with_capacity(predecessors.len());
        for pred in predecessors {
            let Some(phi_sources) = self.phi_sources_for_pred(&phi_insts, pred) else {
                return;
            };
            if pred == *latch
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
            plan.edge_sources.insert(pred, sources.clone());
            plan.edges.insert(pred, StackPhiEdge { sources, results: entry.clone() });
        }
    }

    fn can_plan_branching_loop(&self, loop_info: &Loop) -> bool {
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
        let function_safe = self.func.blocks.iter().all(|block| {
            block.instructions.iter().all(|&inst_id| {
                !matches!(
                    self.func.inst(inst_id).kind,
                    InstKind::CalldataCopy(_, _, _) | InstKind::SLoad(_) | InstKind::SStore(_, _)
                )
            })
        });
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
                    || self.branch_phi_shape_is_valid(loop_info, *then_block, *else_block)
            });
        function_safe
            && branch_shapes_safe
            && self.phi_insts(&self.func.blocks[loop_info.header]).len() >= 2
    }

    fn is_noreturn_block(&self, block_id: BlockId) -> bool {
        self.func.blocks[block_id].instructions.is_empty()
            && matches!(
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
            plan.edge_sources.insert(pred, sources.clone());
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
    /// Functions that are recursive or can reach recursion. A preserved
    /// prefix must not be carried into an unbounded descendant.
    recursion_reaching_functions: DenseBitSet<FunctionId>,
    /// High-water mark of the modeled stack above each function's inherited
    /// untracked prefix.
    function_stack_peaks: FxHashMap<FunctionId, usize>,
    /// Runtime internal-call edges and the caller words retained at each site.
    internal_call_stack_edges: Vec<InternalCallStackEdge>,
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
    /// Multi-return protocol instructions satisfied directly from adopted
    /// stack-return words; the emission loop skips them.
    elided_insts: FxHashSet<InstId>,
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
}

impl<'gcx> EvmCodegen<'gcx> {
    /// Creates a new EVM code generator.
    #[must_use]
    pub fn new(gcx: Gcx<'gcx>) -> Self {
        let switch_gas_code_growth_remaining = Self::switch_gas_code_growth_limit(gcx);
        Self {
            gcx,
            asm: Assembler::new(gcx),
            scheduler: StackScheduler::new(),
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
            recursion_reaching_functions: DenseBitSet::new_empty(0),
            function_stack_peaks: FxHashMap::default(),
            internal_call_stack_edges: Vec::new(),
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
            elided_insts: FxHashSet::default(),
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
        }
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
    /// This includes fallback shapes that ABI lowering did not recognize and
    /// logical slices whose aggregate use slice lowering could not fold.
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
                self.gcx.dcx().err(message).span(span).emit();
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
        self.asm.emit_op(op.opcode());
        match op {
            StackOp::Dup(n) => self.scheduler.stack.dup(n),
            StackOp::Swap(n) => self.scheduler.stack.swap(n),
            StackOp::Pop => {
                self.scheduler.stack.pop();
            }
        }
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
        if module.is_interface {
            return EvmArtifact::default();
        }
        if !module.functions.is_empty()
            && !module.functions.iter().any(|func| {
                Self::is_external_entry(func)
                    || func.attributes.is_constructor
                    || func.attributes.is_dispatch_entry
            })
        {
            if self.capture_mir {
                self.run_optimization_passes(module);
            }
            return EvmArtifact::default();
        }
        if let Some(func) = module.functions.iter().find(|func| func.blocks.is_empty()) {
            panic!("cannot codegen MIR function `{}` without an entry block", func.name);
        }
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
        self.cold_functions = if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            DenseBitSet::new_empty(module.functions.len())
        } else {
            Self::collect_cold_functions(module)
        };

        // First generate the runtime code
        let mut runtime_code = self.generate_runtime_code(module, &call_graph);
        if let Some(evm_ir) = &mut runtime_code.evm_ir {
            evm_ir.set_name(sym::runtime);
        }
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
        if let Some(evm_ir) = &mut deploy_code.evm_ir {
            evm_ir.set_name(sym::deployment);
        }

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
        }
    }

    fn runtime_copy_base(
        module: &Module,
        runtime_len: usize,
        immutable_refs: &[ImmutableRef],
    ) -> u64 {
        let patched_end = immutable_refs.iter().fold(runtime_len, |end, immutable_ref| {
            let patch_size = if immutable_ref.type_size.bytes() == 1 { 1 } else { EVM_WORD_BYTES };
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
        self.asm.emit_op(op::dup(1));
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

        if byte_width < 32 {
            let trailing_bits = usize::from(32 - byte_width) * 8;
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
        if byte_width == 32 {
            return;
        }
        match encoding {
            ImmutableEncoding::Unsigned(_) => {}
            ImmutableEncoding::Signed(_) => {
                self.asm.emit_push(U256::from(byte_width - 1));
                self.asm.emit_op(op::SIGNEXTEND);
            }
            ImmutableEncoding::LeftAligned(_) => {
                self.asm.emit_push(U256::from((32 - byte_width) * 8));
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
        let runtime_offset = self.asm.new_deferred_const();

        // Find constructor function if it exists
        let constructor =
            module.functions.iter_enumerated().find(|(_, f)| f.attributes.is_constructor);

        // An absent MIR constructor represents Solidity's implicit nonpayable
        // constructor. Explicit and synthetic constructors carry their guard
        // in MIR, where `lower-abi` can preserve payable constructors.
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
            self.restorable_internal_frames = DenseBitSet::new_empty(module.functions.len());
            self.static_frame_functions = DenseBitSet::new_empty(module.functions.len());
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
                self.asm.emit_op(op::dup(1));
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
                self.asm.emit_push_label(constructor_entry);
                self.asm.emit_op(op::JUMP);

                for (func_id, func) in module.functions.iter_enumerated() {
                    if !internal_targets.contains(func_id) {
                        continue;
                    }
                    let label = self.function_labels[&func_id];
                    self.asm.define_label(label);
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
            assembly: self.asm.prepare(self.capture_evm_ir),
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
        GeneratedCode { bytecode: result.bytecode, evm_ir: result.evm_ir }
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
        let mut preserve_caller_stack = self.gcx.sess.opts.optimization.is_gas();
        let mut runtime_stack_args = true;
        self.disabled_stack_only_functions = DenseBitSet::new_empty(module.functions.len());
        loop {
            let disabled_stack_only_functions = self.disabled_stack_only_functions.count();
            self.reset_runtime_codegen(module);
            self.preserve_caller_stack = preserve_caller_stack;
            self.runtime_stack_args = runtime_stack_args;

            if !module.functions.is_empty() {
                self.emit_runtime(module, call_graph);
            }

            if self.disabled_stack_only_functions.count() > disabled_stack_only_functions {
                continue;
            }
            if !self.internal_call_stack_edges.is_empty() && !self.caller_stack_prefixes_fit(module)
            {
                if preserve_caller_stack {
                    preserve_caller_stack = false;
                    continue;
                }
                if runtime_stack_args {
                    runtime_stack_args = false;
                    continue;
                }
                if self.stack_returns_enabled {
                    self.stack_returns_enabled = false;
                    continue;
                }
            }
            break;
        }

        let result = self.asm.assemble_with_evm_ir(self.capture_evm_ir);
        self.runtime_immutable_refs = result.immutable_refs;
        GeneratedCode { bytecode: result.bytecode, evm_ir: result.evm_ir }
    }

    fn reset_runtime_codegen(&mut self, module: &Module) {
        self.asm.clear();
        self.asm.set_artifact_kind(ArtifactKind::Runtime);
        self.block_labels.clear();
        self.function_labels.clear();
        self.empty_stop_functions = DenseBitSet::new_empty(module.functions.len());
        self.function_spill_sizes.clear();
        self.pending_frame_size_consts.clear();
        self.restorable_internal_frames = DenseBitSet::new_empty(module.functions.len());
        self.static_frame_functions = DenseBitSet::new_empty(module.functions.len());
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
        self.recursive_stack_functions = DenseBitSet::new_empty(module.functions.len());
        self.recursion_reaching_functions = DenseBitSet::new_empty(module.functions.len());
        self.function_stack_peaks.clear();
        self.internal_call_stack_edges.clear();
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
    fn caller_stack_prefixes_fit(&self, module: &Module) -> bool {
        if !self
            .internal_call_stack_edges
            .iter()
            .any(|edge| edge.preserved_words != 0 || edge.argument_words != 0)
        {
            return true;
        }
        let Some(entry_id) = module
            .functions
            .iter_enumerated()
            .find_map(|(func_id, func)| func.attributes.is_dispatch_entry.then_some(func_id))
        else {
            return true;
        };

        let mut incoming: IndexVec<FunctionId, Option<usize>> =
            index_vec![None; module.functions.len()];
        incoming[entry_id] = Some(0);
        for _ in 0..module.functions.len() {
            let mut changed = false;
            for edge in &self.internal_call_stack_edges {
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
                    > MAX_STACK_DEPTH
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
                    if candidate.saturating_add(1) > MAX_STACK_DEPTH {
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
                    <= MAX_STACK_DEPTH
            })
        })
    }

    /// Emits a runtime from final-phase MIR.
    ///
    /// Selector matching, receive/fallback routing, and callvalue checks all
    /// live in the MIR `entry`, whose `tail_call`s jump to the ABI wrappers.
    fn emit_runtime(&mut self, module: &Module, call_graph: &CallGraphInfo) {
        let Some((entry_id, _)) =
            module.functions.iter_enumerated().find(|(_, f)| f.attributes.is_dispatch_entry)
        else {
            assert!(
                !module.functions.iter().any(Self::is_external_entry),
                "evm-shaped module with a runtime interface must have a MIR `entry` function"
            );
            return;
        };

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
            // Non-recursive internal functions get compile-time-fixed frames.
            if func_id != entry_id
                && !Self::is_external_entry(func)
                && Self::is_runtime_function(func)
                && !call_graph.is_recursive(func_id)
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

    /// Generates the body of a function.
    fn generate_function_body(&mut self, func_id: FunctionId, func: &Function) {
        let liveness = self
            .emitting_entry
            .then(|| Liveness::compute_block_local_for_codegen(func))
            .flatten()
            .unwrap_or_else(|| Liveness::compute(func));
        let liveness = &liveness;

        // Eliminate phis.
        self.block_copies.clear();
        self.elided_insts.clear();
        let phi_result = PhiEliminator::analyze(func);
        let has_phis = !phi_result.phis_to_remove.is_empty();
        for (block_id, copies) in phi_result.block_copies {
            self.block_copies.insert(block_id, copies.copies);
        }
        // Stack-phi planning starts with loop analysis, but cannot produce a
        // plan without a phi. Avoid that analysis for the overwhelmingly
        // common phi-free function.
        let mut stack_phi_plan =
            if has_phis { StackPhiPlan::analyze(func) } else { StackPhiPlan::default() };
        let resident_stack_plan = self.resident_stack_plan(func_id).cloned();
        let mut global_stack_plan = resident_stack_plan
            .clone()
            .unwrap_or_else(|| GlobalStackPlan::analyze(func, liveness, &stack_phi_plan));
        let mut stack_phi_sources = stack_phi_plan.edge_sources.clone();
        if resident_stack_plan.is_some() {
            if !stack_phi_plan.merge_resident(func, &global_stack_plan) {
                // Selection preflights this exact composition. If a future transform invalidates
                // that proof, regenerate the runtime with the ordinary frame-backed convention
                // instead of emitting a partial stack ABI or panicking.
                self.disabled_stack_only_functions.insert(func_id);
                return;
            }
            stack_phi_sources = stack_phi_plan.edge_sources.clone();
        } else if global_stack_plan.is_empty()
            && let Some((values, plan)) =
                self.compute_cross_block_stack_layout(func, liveness, has_phis)
            // Phi layouts own their incoming stack on planned joins. Adopt the layout only when
            // that composition is proven, mirroring the resident arm.
            && stack_phi_plan.merge_resident(func, &plan)
        {
            global_stack_plan = plan;
            // An early spill store can be omitted only when every physical successor layout
            // carries the value; an edge-specific cleanup may otherwise discard the sole stack
            // copy before a later block reloads its reserved slot. The reserved spill slot
            // remains available if edge emission ever falls back to the memory convention.
            stack_phi_sources = stack_phi_plan.edge_sources.clone();
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
        self.stack_phi_sources = stack_phi_sources;
        self.global_stack_active = !global_stack_plan.is_empty();
        self.global_stack_aliases = global_stack_plan.aliases.clone();

        // Reset scheduler
        self.scheduler = StackScheduler::new();
        self.spill_addr_consts.clear();

        // Cross-block rematerialization is selected during spill preallocation. Record every
        // argument without a frame home before that analysis so an expression depending on one is
        // stored instead of later being rebuilt after its only physical copy was consumed.
        let initial_stack_only_values = self.stack_only_values(func_id, true);
        self.scheduler.set_stack_only_values(func.num_values(), initial_stack_only_values);

        self.preallocate_cross_block_spills(func, liveness);

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
        for (pos, &block_id) in block_order.iter().enumerate() {
            let block = &func.blocks[block_id];
            let fallthrough = block_order.get(pos + 1).copied();
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
                    self.invalidate_carried_phi_spills(func);
                    // Live-ins not on the carried stack still arrive in memory.
                    self.mark_live_in_spills(func, liveness, block_id);
                } else if let Some(entry) = stack_phi_plan.entries.get(&block_id) {
                    self.set_stack_to_values(entry);
                    self.invalidate_carried_phi_spills(func);
                    self.mark_live_in_spills(func, liveness, block_id);
                } else if let Some(entry) = global_stack_plan.entry(block_id) {
                    self.set_stack_to_values(entry);
                    self.invalidate_carried_phi_spills(func);
                    self.mark_live_in_spills(func, liveness, block_id);
                } else {
                    self.scheduler.clear_stack();
                    self.mark_live_in_spills(func, liveness, block_id);
                }
            }
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
            let stack_only_values = self.stack_only_values(func_id, block_id == BlockId::ENTRY);
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

            let terminator_growth =
                block.terminator.as_ref().map_or(0, Self::terminator_transient_growth);
            self.materialize_deep_stack_args(func_id, func, terminator_growth);

            let stack_phi_preserved = stack_phi_plan.edges.get(&block_id).is_some_and(|edge| {
                if !self.can_prepare_stack_phi_edge(func, edge) {
                    return false;
                }
                self.spill_live_out_values_except(func, liveness, block_id, &edge.sources);
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
            {
                self.spill_live_out_values_except(func, liveness, block_id, &branch.union);
            } else if !preserve_stack {
                self.spill_live_out_values(func, liveness, block_id);
            }

            // Generate terminator. An edge-specific resident branch owns its cleanup and jumps.
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
                self.emit_stack_phi_branch(func, *condition, *then_block, *else_block, branch);
            } else if let Some(term) = &block.terminator {
                self.generate_terminator(func, term, fallthrough, preserve_stack);
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
        }

        if let Some(value) = self.scheduler.spills.unstored_required() {
            panic!(
                "mandatory cross-block spill store for {value:?} was not emitted in `{}`",
                func.name
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

        for target in targets {
            if target == block_id
                || func.blocks[target].predecessors.as_slice() != [block_id]
                || block_pos.get(&target).copied() <= Some(pos)
                || func.blocks[target]
                    .instructions
                    .iter()
                    .any(|&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)))
            {
                return Vec::new();
            }
        }

        targets.into()
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
                            InstKind::InternalCall { function, .. } if cold.contains(function)
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
                InstKind::InternalCall { function, .. }
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
            self.asm.emit_op(op.opcode());
        }

        true
    }

    fn global_branch_union(then_layout: &[ValueId], else_layout: &[ValueId]) -> Vec<ValueId> {
        union_values(then_layout, else_layout)
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
            self.asm.emit_op(op.opcode());
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
            self.asm.emit_op(op.opcode());
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
            self.asm.emit_push_label(self.block_labels[&direct]);
            self.asm.emit_op(op::JUMPI);
            self.scheduler.stack.pop();
            self.emit_global_branch_cleanup(cleanup_layout);
            if Some(cleanup) != fallthrough {
                self.asm.emit_push_label(self.block_labels[&cleanup]);
                self.asm.emit_op(op::JUMP);
            }
            return;
        }

        // Neither target wants the complete incoming union. Route one edge through a local
        // cleanup label and clean the fallthrough edge inline.
        let then_cleanup = self.asm.new_label();
        self.asm.emit_push_label(then_cleanup);
        self.asm.emit_op(op::JUMPI);
        self.scheduler.stack.pop();
        let union_stack = self.scheduler.stack.clone();

        self.emit_global_branch_cleanup(else_layout);
        self.asm.emit_push_label(self.block_labels[&else_block]);
        self.asm.emit_op(op::JUMP);

        self.asm.define_label(then_cleanup);
        self.scheduler.stack = union_stack;
        self.emit_global_branch_cleanup(then_layout);
        if Some(then_block) != fallthrough {
            self.asm.emit_push_label(self.block_labels[&then_block]);
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
            self.asm.emit_push_label(actual);
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
            if !self.can_emit_stack_phi_value(func, source) {
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
            self.asm.emit_op(op.opcode());
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

    /// Stack-phi preparation emits through `emit_operand`, which can recompute an unstored spill.
    fn can_emit_stack_phi_value(&self, func: &Function, value: ValueId) -> bool {
        self.scheduler.can_emit_value(value, func)
            || self.scheduler.should_recompute_unstored_spill(value)
    }

    fn can_prepare_stack_phi_branch(
        &self,
        func: &Function,
        condition: ValueId,
        branch: &StackPhiBranch,
    ) -> bool {
        if branch.union.is_empty() || branch.union.len() > MAX_STACK_ACCESS {
            return false;
        }
        self.can_emit_stack_phi_value(func, condition)
            && self.can_prepare_stack_phi_edge(func, &branch.then_edge)
            && self.can_prepare_stack_phi_edge(func, &branch.else_edge)
    }

    fn emit_stack_phi_edge_layout(&mut self, edge: &StackPhiEdge) {
        self.pop_stack_values_not_needed_by(&edge.sources);
        let target: Vec<_> = edge.sources.iter().copied().map(TargetSlot::Value).collect();
        let shuffle = self
            .scheduler
            .shuffle_to_layout(&target)
            .unwrap_or_else(|| panic!("could not construct branch stack-phi edge layout"));
        assert_eq!(self.scheduler.depth(), edge.sources.len());
        assert!(self.scheduler.stack.iter().eq(edge.sources.iter().copied().map(Some)));
        for op in shuffle.ops {
            self.asm.emit_op(op.opcode());
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
    ) {
        let mut needed = Vec::with_capacity(branch.union.len() + 1);
        needed.push(condition);
        needed.extend_from_slice(&branch.union);
        self.pop_stack_values_not_needed_by(&needed);
        for value in Self::missing_stack_phi_sources(&self.scheduler.stack, &needed) {
            assert!(self.can_emit_stack_phi_value(func, value));
            self.emit_operand(func, value);
        }
        let target: Vec<_> = needed.iter().copied().map(TargetSlot::Value).collect();
        let shuffle = self
            .scheduler
            .shuffle_to_layout(&target)
            .unwrap_or_else(|| panic!("could not construct branch stack-phi layout"));
        for op in shuffle.ops {
            self.asm.emit_op(op.opcode());
        }
        assert_eq!(self.scheduler.stack.top(), Some(condition));

        let then_cleanup = self.asm.new_label();
        self.asm.emit_push_label(then_cleanup);
        self.asm.emit_op(op::JUMPI);
        self.scheduler.stack.pop();
        let union_stack = self.scheduler.stack.clone();

        self.emit_stack_phi_edge_layout(&branch.else_edge);
        self.asm.emit_push_label(self.block_labels[&else_block]);
        self.asm.emit_op(op::JUMP);

        self.asm.define_label(then_cleanup);
        self.scheduler.stack = union_stack;
        self.emit_stack_phi_edge_layout(&branch.then_edge);
        self.asm.emit_push_label(self.block_labels[&then_block]);
        self.asm.emit_op(op::JUMP);
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
    fn preallocate_cross_block_spills(&mut self, func: &Function, liveness: &Liveness) {
        // Coloring minimizes the local frame, which reduces memory expansion in gas mode. It is
        // deliberately disabled in size mode because renumbering spill addresses disturbed
        // downstream block sharing and regressed aggregate CI bytecode despite smaller frames.
        let mut values = if self.gcx.sess.opts.optimization.is_gas() {
            let (values, colorable) = Self::cross_block_spill_values_and_colorable(func, liveness);
            if !colorable.is_empty() {
                let ranges = Self::spill_live_ranges(func, liveness, &colorable);
                let mut interferences =
                    Self::parallel_phi_interferences(func, &colorable, &self.block_copies);
                Self::phi_edge_interferences(
                    func,
                    liveness,
                    &colorable,
                    &self.block_copies,
                    &mut interferences,
                );
                let mut colors = Vec::<SpillColor>::new();
                for value in &colorable {
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
            }

            for value in &values {
                if !colorable.contains(value) {
                    self.scheduler.spills.reserve(value);
                }
            }
            values
        } else {
            let values = Self::cross_block_spill_values(func, liveness);
            for value in &values {
                self.scheduler.spills.reserve(value);
            }
            values
        };
        let cfg = CfgInfo::new(func);
        let reachable = cfg.reachable();
        let mut reachable_values = DenseBitSet::new_empty(func.num_values());
        for block_id in reachable {
            for &inst_id in &func.blocks[block_id].instructions {
                if let Some(value) = func.inst_result_value(inst_id) {
                    reachable_values.insert(value);
                }
            }
        }
        for value in values.iter().collect::<Vec<_>>() {
            if !reachable_values.contains(value) {
                values.remove(value);
            }
        }
        self.preallocate_spill_metadata(func, &values);

        // A free-memory-pointer load cannot be recomputed after the pointer moves. Give loads
        // that cross a block or are directly reloaded a stable slot so a call operand can recover
        // the value even when block layout emits its use before the defining block.
        let reserve_all = matches!(self.gcx.sess.opts.optimization, OptimizationMode::Size);
        let reloaded = Self::cross_block_reload_values(func);
        for val in Self::fmp_load_values(func) {
            if !reachable_values.contains(val)
                || (!reserve_all && !values.contains(val) && !reloaded.contains(val))
            {
                continue;
            }
            self.scheduler.spills.reserve(val);
            self.scheduler.spills.require_store(val);
            self.scheduler.spills.mark_reloadable(val);
        }

        // A deferred allocation is materialized as a placeholder whose final form is chosen
        // after the whole function has been laid out. Reserve its result before block emission:
        // layout order may visit a use block before the defining block, so waiting until the
        // `alloc` instruction runs would leave that use without a reload route.
        for inst_id in func.instructions() {
            if func.inst(inst_id).metadata.deferred_alloc()
                && let Some(value) = func.inst_result_value(inst_id)
                && reachable_values.contains(value)
                && (reserve_all || values.contains(value) || reloaded.contains(value))
            {
                self.scheduler.spills.reserve(value);
                self.scheduler.spills.require_store(value);
                self.scheduler.spills.mark_reloadable(value);
            }
        }
    }

    fn preallocate_spill_metadata(&mut self, func: &Function, values: &DenseBitSet<ValueId>) {
        if values.iter().any(|value| Self::is_cross_block_recomputable_inst(func, value)) {
            let recomputable = Self::cross_block_recomputable_values_with(func, |value| {
                !self.scheduler.is_stack_only_value(value)
            });
            let reloaded = values
                .iter()
                .any(|value| {
                    !recomputable.contains(value)
                        && StackScheduler::is_cheap_recomputable_value(func, value)
                })
                .then(|| Self::cross_block_reload_values(func));
            for val in values {
                if recomputable.contains(val) {
                    self.scheduler.spills.mark_recomputable(val);
                } else if reloaded.as_ref().is_some_and(|values| values.contains(val))
                    && StackScheduler::is_cheap_recomputable_value(func, val)
                {
                    self.scheduler.spills.require_store(val);
                }
            }
        }
    }

    fn cross_block_spill_values_and_colorable(
        func: &Function,
        liveness: &Liveness,
    ) -> (DenseBitSet<ValueId>, DenseBitSet<ValueId>) {
        let mut values = DenseBitSet::new_empty(func.num_values());
        let mut colorable = DenseBitSet::new_empty(func.num_values());
        for block_id in func.blocks.indices() {
            for val in liveness.live_in(block_id).iter().chain(liveness.live_out(block_id).iter()) {
                if Self::can_own_spill_slot(func, val) {
                    values.insert(val);
                }
                if matches!(func.value(val), crate::mir::Value::Inst(_)) {
                    colorable.insert(val);
                }
            }
            for &inst_id in &func.blocks[block_id].instructions {
                if matches!(func.inst(inst_id).kind, InstKind::Phi(_))
                    && let Some(val) = func.inst_result_value(inst_id)
                {
                    values.insert(val);
                }
            }
        }
        (values, colorable)
    }

    fn spill_live_ranges(
        func: &Function,
        liveness: &Liveness,
        colorable: &DenseBitSet<ValueId>,
    ) -> IndexVec<ValueId, FxHashMap<BlockId, SpillLiveRange>> {
        let mut ranges = index_vec![FxHashMap::default(); func.num_values()];

        for block_id in func.blocks.indices() {
            for value in liveness.live_in(block_id) {
                Self::extend_spill_live_range(&mut ranges, colorable, value, block_id, 0);
            }
            let point = func.blocks[block_id].instructions.len() * 2 + 1;
            for value in liveness.live_out(block_id) {
                Self::extend_spill_live_range(&mut ranges, colorable, value, block_id, point);
            }
        }

        // Liveness already collected the final use in every block. Reuse that
        // map instead of walking and collecting every instruction operand again.
        for ((value, block_id), last_use) in liveness.last_uses() {
            let point =
                last_use.map_or(func.blocks[block_id].instructions.len() * 2, |index| index * 2);
            Self::extend_spill_live_range(&mut ranges, colorable, value, block_id, point);
        }

        for (block_id, block) in func.blocks.iter_enumerated() {
            for (inst_idx, &inst_id) in block.instructions.iter().enumerate() {
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
        }

        ranges
    }

    /// Records spill-slot conflicts introduced by simultaneous phi edge copies.
    ///
    /// The ordinary live ranges do not model the sequentialized copy schedule. Every destination
    /// must coexist at the successor, and a destination store must not alias a source that the
    /// schedule loads later. Sources already loaded before a store may safely share its slot.
    fn parallel_phi_interferences(
        func: &Function,
        colorable: &DenseBitSet<ValueId>,
        block_copies: &FxHashMap<BlockId, Vec<ParallelCopy>>,
    ) -> SpillInterferences {
        let mut interferences = FxHashMap::default();
        for (block_id, copies) in block_copies {
            for (index, copy) in copies.iter().enumerate() {
                let CopyDest::Value(destination) = &copy.dst else { continue };
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

    /// Keeps a phi destination disjoint from values that must survive the predecessor's
    /// terminator. Phi copies are emitted before that terminator, so ordinary value ranges begin
    /// too late to model the destination store overwriting a live branch operand or a value used
    /// on the other edge.
    fn phi_edge_interferences(
        func: &Function,
        liveness: &Liveness,
        colorable: &DenseBitSet<ValueId>,
        block_copies: &FxHashMap<BlockId, Vec<ParallelCopy>>,
        interferences: &mut SpillInterferences,
    ) {
        for (&block, copies) in block_copies {
            let mut protected = liveness.live_out(block).iter().collect::<Vec<_>>();
            if let Some(term) = &func.blocks[block].terminator {
                protected.extend(term.operands());
            }
            for copy in copies {
                let CopyDest::Value(destination) = &copy.dst else { continue };
                for &value in &protected {
                    Self::add_spill_interference(interferences, colorable, *destination, value);
                }
            }
        }
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

    fn cross_block_spill_values(func: &Function, liveness: &Liveness) -> DenseBitSet<ValueId> {
        let mut values = DenseBitSet::new_empty(func.num_values());
        for block_id in func.blocks.indices() {
            for val in liveness.live_in(block_id).iter().chain(liveness.live_out(block_id).iter()) {
                if Self::can_own_spill_slot(func, val) {
                    values.insert(val);
                }
            }
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

    /// Computes cross-block rematerialization while excluding leaves unavailable under the active
    /// calling convention. A reverse-use worklist handles long expression chains linearly.
    /// Stack-only arguments have no frame home, so an expression depending on one must be stored
    /// at its definition instead of being rebuilt after the argument is gone.
    fn cross_block_recomputable_values_with(
        func: &Function,
        leaf_is_available: impl Fn(ValueId) -> bool,
    ) -> DenseBitSet<ValueId> {
        let mut users =
            IndexVec::<ValueId, SmallVec<[ValueId; 2]>>::with_capacity(func.num_values());
        let mut remaining = IndexVec::<ValueId, usize>::with_capacity(func.num_values());
        for _ in 0..func.num_values() {
            users.push(SmallVec::new());
            remaining.push(usize::MAX);
        }

        let mut recomputable = DenseBitSet::new_empty(func.num_values());
        let mut worklist = Vec::new();
        for value in func.live_values() {
            if Self::is_rematerializable_value(func, value)
                && leaf_is_available(value)
                && recomputable.insert(value)
            {
                worklist.push(value);
            }
        }
        for inst_id in func.instructions() {
            let Some(result) = func.inst_result_value(inst_id) else { continue };
            if !Self::is_cross_block_recomputable_inst(func, result) {
                continue;
            }
            let operands = func.inst(inst_id).kind.operands();
            remaining[result] = operands.len();
            if operands.is_empty() && recomputable.insert(result) {
                worklist.push(result);
            }
            for operand in operands {
                users[operand].push(result);
            }
        }

        while let Some(value) = worklist.pop() {
            for &user in &users[value] {
                remaining[user] -= 1;
                if remaining[user] == 0 && recomputable.insert(user) {
                    worklist.push(user);
                }
            }
        }
        recomputable
    }

    fn is_cross_block_recomputable_inst(func: &Function, value: ValueId) -> bool {
        if StackScheduler::is_cheap_recomputable_value(func, value) {
            return true;
        }
        let crate::mir::Value::Inst(inst_id) = func.value(value) else { return false };
        matches!(
            func.inst(*inst_id).kind,
            InstKind::CallValue
                | InstKind::Caller
                | InstKind::Origin
                | InstKind::CalldataSize
                | InstKind::CalldataLoad(_)
                | InstKind::InternalFrameAddr(_)
                | InstKind::Timestamp
                | InstKind::BlockNumber
        )
    }

    /// Spills all live-out values that are currently on the stack to memory.
    /// This ensures values that need to be accessed in successor blocks can be reloaded.
    fn spill_live_out_values(&mut self, func: &Function, liveness: &Liveness, block_id: BlockId) {
        let live_out = liveness.live_out(block_id);

        for val in live_out {
            self.spill_value_if_needed(func, val);
        }
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
        for val in liveness.live_out(block_id) {
            if !exempt_values.contains(val) {
                self.spill_value_if_needed(func, val);
            }
        }
    }

    fn pop_stack_values_not_needed_by(&mut self, needed: &[ValueId]) {
        while let Some(depth) = self.first_stack_value_not_needed_by(needed) {
            if depth > 0 {
                assert!(depth <= MAX_STACK_ACCESS, "resident stack discard exceeded SWAP16 reach");
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

    /// Invalidates the spill bookkeeping of every phi result on a stack
    /// restored from a carried edge. A loop-carried phi is redefined on every
    /// re-entry without a store, so a slot stored during an earlier iteration
    /// holds a stale definition: an exit-path use must spill the carried copy
    /// again before anything reloads the slot. Other carried values are
    /// immutable SSA definitions whose stored slots stay current, and
    /// invalidating those would force later paths to recompute
    /// memory-dependent definitions whose operands may have changed.
    fn invalidate_carried_phi_spills(&mut self, func: &Function) {
        let carried: Vec<ValueId> = self.scheduler.stack.iter().flatten().collect();
        for value in carried {
            if let crate::mir::Value::Inst(inst_id) = func.value(value)
                && matches!(func.inst(*inst_id).kind, InstKind::Phi(_))
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
        for &operand in operands {
            self.spill_value_if_needed(func, operand);
        }
    }

    /// Duplicates a stack-only operand before earlier fresh operands would bury it past DUP16.
    /// `operands` are ordered deepest-first, exactly as the following emission sequence pushes
    /// them.
    fn stage_stack_only_fresh_operands(&mut self, operands: &[ValueId]) {
        if !self.scheduler.has_stack_only_values() {
            return;
        }

        loop {
            let mut stack = self.scheduler.stack.clone();
            let mut inaccessible = None;
            for &operand in operands {
                if self.scheduler.is_stack_only_value(operand) {
                    match stack.find(operand) {
                        Some(depth) if depth < MAX_STACK_ACCESS => {
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
            let depth = self.scheduler.stack.find(operand).unwrap_or_else(|| {
                panic!("stack-only CALL operand {operand:?} was lost before its use")
            });
            assert!(depth < MAX_STACK_ACCESS, "stack-only CALL operand exceeded DUP16 reach");
            self.emit_stack_op(StackOp::Dup((depth + 1) as u8));
        }
    }

    /// Spills an instruction result if it is on the stack and not already stored.
    fn spill_value_if_needed(&mut self, func: &Function, val: ValueId) {
        if !Self::can_own_spill_slot(func, val) {
            return;
        }

        if self.scheduler.spills.is_stored(val) {
            return;
        }

        if let Some(depth) = self.scheduler.stack.find(val) {
            let slot = self.scheduler.spills.allocate(val);
            if depth >= MAX_STACK_ACCESS {
                self.spill_deep_stack_value(func, val, slot, depth);
                return;
            }

            self.spill_accessible_stack_value(func, val, slot, depth);
        }
    }

    fn spill_value_to_reserved_slot(&mut self, func: &Function, val: ValueId) -> bool {
        if Self::is_rematerializable_value(func, val) || self.scheduler.spills.get(val).is_none() {
            return false;
        }

        let Some(depth) = self.scheduler.stack.find(val) else {
            return false;
        };
        let slot = self.scheduler.spills.allocate(val);
        if depth >= MAX_STACK_ACCESS {
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
        debug_assert!(depth < MAX_STACK_ACCESS);

        // DUP the value to top of stack for storing.
        // We need to DUP (not just use ensure_on_top) because:
        // 1. If value is on top, ensure_on_top does nothing but we need a copy
        // 2. MSTORE will consume the value, and we want to preserve the original
        let dup_n = (depth + 1) as u8;
        self.asm.emit_op(op::dup(dup_n));
        self.scheduler.stack.dup(dup_n);

        self.store_stack_top_to_spill(func, val, slot);
    }

    fn spill_deep_stack_value(
        &mut self,
        func: &Function,
        val: ValueId,
        slot: SpillSlot,
        depth: usize,
    ) {
        debug_assert!(depth >= MAX_STACK_ACCESS);

        let mut saved_above = Vec::with_capacity(depth + 1 - MAX_STACK_ACCESS);
        for _ in 0..(depth + 1 - MAX_STACK_ACCESS) {
            let Some(top) = self.scheduler.stack.top() else {
                panic!("cannot spill deep stack value {val:?}: untracked stack entry above it");
            };
            let top_slot = self.scheduler.spills.allocate(top);
            if self.scheduler.reloadable_spill(top).is_some() {
                self.emit_stack_op(StackOp::Pop);
            } else {
                self.store_stack_top_to_spill(func, top, top_slot);
            }
            saved_above.push((top, top_slot));
        }

        let Some(accessible_depth) = self.scheduler.stack.find(val) else {
            panic!("cannot spill deep stack value {val:?}: value disappeared while exposing it");
        };
        self.spill_accessible_stack_value(func, val, slot, accessible_depth);

        for (saved, saved_slot) in saved_above.into_iter().rev() {
            self.emit_spill_slot_addr(func, saved_slot);
            self.asm.emit_op(op::MLOAD);
            self.scheduler.stack.push(saved);
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
    /// DUP16 reach: [`Self::emit_value_impl`] gives that otherwise stranded
    /// word a spill slot. Do not make ordinary frame-backed arguments own
    /// slots without redesigning argument spilling.
    fn is_rematerializable_value(func: &Function, value: ValueId) -> bool {
        matches!(func.value(value), crate::mir::Value::Immediate(_) | crate::mir::Value::Arg(_))
    }

    fn can_own_spill_slot(func: &Function, value: ValueId) -> bool {
        matches!(func.value(value), crate::mir::Value::Inst(_))
    }

    /// Returns true when `value` needs no spill before the instruction that
    /// is about to consume it: it owns no reserved cross-block slot, it is
    /// not live out of the block, and more stack copies exist at this point
    /// than the instruction will consume net of the emissions still to come
    /// (`consumed`). Later in-block uses DUP the survivor, or deep-spill it
    /// on demand if it sinks past `MAX_STACK_ACCESS`, so skipping the store
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
        if Self::is_rematerializable_value(func, value) {
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
        if has_reserved_cross_block_slot {
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
                    panic!("resident stack argument {operand:?} was lost before its final use")
                });
                assert!(depth < MAX_STACK_ACCESS, "resident stack argument exceeded DUP16 reach");
                self.emit_stack_op(StackOp::Dup((depth + 1) as u8));
            }
        }
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
            // Binary arithmetic operations
            InstKind::Add(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::ADD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Sub(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::SUB,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Mul(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::MUL,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Div(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::DIV,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::SDiv(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::SDIV,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Mod(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::MOD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::SMod(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::SMOD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Exp(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::EXP,
                result_value,
                liveness,
                block,
                inst_idx,
            ),

            // Bitwise operations
            InstKind::And(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::AND,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Or(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::OR,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Xor(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::XOR,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Not(a) => self.emit_unary_op_with_result(
                func,
                *a,
                op::NOT,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Clz(a) => self.emit_unary_op_with_result(
                func,
                *a,
                op::CLZ,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Shl(shift, val) => self.emit_binary_op_with_result(
                func,
                *shift,
                *val,
                op::SHL,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Shr(shift, val) => self.emit_binary_op_with_result(
                func,
                *shift,
                *val,
                op::SHR,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Sar(shift, val) => self.emit_binary_op_with_result(
                func,
                *shift,
                *val,
                op::SAR,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Byte(i, x) => self.emit_binary_op_with_result(
                func,
                *i,
                *x,
                op::BYTE,
                result_value,
                liveness,
                block,
                inst_idx,
            ),

            // Comparison operations - track results for branch conditions and Select
            InstKind::Lt(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::LT,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Gt(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::GT,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::SLt(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::SLT,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::SGt(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::SGT,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Eq(a, b) => self.emit_binary_op_with_result(
                func,
                *a,
                *b,
                op::EQ,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::IsZero(a) => self.emit_unary_op_with_result(
                func,
                *a,
                op::ISZERO,
                result_value,
                liveness,
                block,
                inst_idx,
            ),

            // Memory operations
            // Track MLOAD results so they can be used as operands in subsequent instructions.
            // This is essential for nested external calls where the return value from one call
            // becomes an argument to another call.
            InstKind::MLoad(addr) => self.emit_unary_op_with_result(
                func,
                *addr,
                op::MLOAD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::MStore(addr, val) => self.emit_store_op_live_aware(
                func,
                *addr,
                *val,
                op::MSTORE,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::MStore8(addr, val) => self.emit_store_op_live_aware(
                func,
                *addr,
                *val,
                op::MSTORE8,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::MSize => {
                self.asm.emit_op(op::MSIZE);
                self.scheduler.instruction_executed(0, result_value);
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

            // Storage operations
            InstKind::SLoad(slot) => self.emit_unary_op_with_result(
                func,
                *slot,
                op::SLOAD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::SStore(slot, val) => self.emit_store_op_live_aware(
                func,
                *slot,
                *val,
                op::SSTORE,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::TLoad(slot) => self.emit_unary_op_with_result(
                func,
                *slot,
                op::TLOAD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::TStore(slot, val) => self.emit_store_op_live_aware(
                func,
                *slot,
                *val,
                op::TSTORE,
                liveness,
                block,
                inst_idx,
            ),

            // Calldata operations
            InstKind::CalldataLoad(off) => self.emit_unary_op_with_result(
                func,
                *off,
                op::CALLDATALOAD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::CalldataSize => {
                self.asm.emit_op(op::CALLDATASIZE);
                self.scheduler.instruction_executed(0, result_value);
            }

            // Hash operations
            InstKind::Keccak256(off, len) => self.emit_binary_op_with_result(
                func,
                *off,
                *len,
                op::KECCAK256,
                result_value,
                liveness,
                block,
                inst_idx,
            ),

            // Environment operations
            InstKind::Caller => {
                self.asm.emit_op(op::CALLER);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::CallValue => {
                self.asm.emit_op(op::CALLVALUE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Address => {
                self.asm.emit_op(op::ADDRESS);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Origin => {
                self.asm.emit_op(op::ORIGIN);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::GasPrice => {
                self.asm.emit_op(op::GASPRICE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Gas => {
                self.asm.emit_op(op::GAS);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Timestamp => {
                self.asm.emit_op(op::TIMESTAMP);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::BlockNumber => {
                self.asm.emit_op(op::NUMBER);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Coinbase => {
                self.asm.emit_op(op::COINBASE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::ChainId => {
                self.asm.emit_op(op::CHAINID);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::SelfBalance => {
                self.asm.emit_op(op::SELFBALANCE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::BaseFee => {
                self.asm.emit_op(op::BASEFEE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::BlobBaseFee => {
                self.asm.emit_op(op::BLOBBASEFEE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::GasLimit => {
                self.asm.emit_op(op::GASLIMIT);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::PrevRandao => {
                self.asm.emit_op(op::PREVRANDAO);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Balance(addr) => self.emit_unary_op_with_result(
                func,
                *addr,
                op::BALANCE,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::BlockHash(num) => self.emit_unary_op_with_result(
                func,
                *num,
                op::BLOCKHASH,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::BlobHash(idx) => self.emit_unary_op_with_result(
                func,
                *idx,
                op::BLOBHASH,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::ExtCodeSize(addr) => self.emit_unary_op_with_result(
                func,
                *addr,
                op::EXTCODESIZE,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::ExtCodeHash(addr) => self.emit_unary_op_with_result(
                func,
                *addr,
                op::EXTCODEHASH,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::CodeSize => {
                self.asm.emit_op(op::CODESIZE);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::StoreImmutable(..) => {
                unreachable!("immutable stores must be lowered before EVM codegen")
            }
            InstKind::LoadImmutable(id) => {
                self.emit_load_immutable(*id);
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::ReturnDataSize => {
                self.asm.emit_op(op::RETURNDATASIZE);
                self.scheduler.instruction_executed(0, result_value);
            }
            // Ternary operations
            InstKind::AddMod(a, b, n) => self.emit_nary_op(
                func,
                &[*n, *b, *a],
                op::ADDMOD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::MulMod(a, b, n) => self.emit_nary_op(
                func,
                &[*n, *b, *a],
                op::MULMOD,
                result_value,
                liveness,
                block,
                inst_idx,
            ),

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
                // Step 1: DUP3 to get f -> [f, t, cond, f]
                self.emit_stack_op(StackOp::Dup(3));
                // Step 2: DUP3 to get t (now at depth 2) -> [f, t, cond, f, t]
                self.emit_stack_op(StackOp::Dup(3));
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

            // Sign extend
            InstKind::SignExtend(b, x) => self.emit_binary_op_with_result(
                func,
                *b,
                *x,
                op::SIGNEXTEND,
                result_value,
                liveness,
                block,
                inst_idx,
            ),

            // Phi nodes are skipped (handled by copies)
            InstKind::Phi(_) => {}

            // Contract creation
            InstKind::Create(value, offset, size) => self.emit_nary_op(
                func,
                &[*size, *offset, *value],
                op::CREATE,
                result_value,
                liveness,
                block,
                inst_idx,
            ),
            InstKind::Create2(value, offset, size, salt) => self.emit_nary_op(
                func,
                &[*salt, *size, *offset, *value],
                op::CREATE2,
                result_value,
                liveness,
                block,
                inst_idx,
            ),

            // External calls
            //
            // These use emit_value_fresh to guarantee correct values regardless of scheduler state.
            // The stack-aware emit_op_with_effect ensures proper tracking after emission.
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
                self.emit_value_fresh(func, *gas);

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
                self.emit_value_fresh(func, *gas);

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
                self.emit_value_fresh(func, *gas);
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
                self.emit_value_fresh(func, *gas);
                // DELEGATECALL consumes 6 values and produces 1 (success bool)
                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(
                    op::DELEGATECALL,
                    StackEffect { pops: 6, pushes: 1 },
                    push,
                );
            }

            InstKind::ExtCall { addr, args_offset, args_size, value } => {
                self.prepare_fresh_operands(func, &[*addr, *args_offset, *args_size, *value]);
                self.emit_value_fresh(func, *value);
                self.emit_value_fresh(func, *args_size);
                self.emit_value_fresh(func, *args_offset);
                self.emit_value_fresh(func, *addr);
                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(op::EXTCALL, StackEffect { pops: 4, pushes: 1 }, push);
            }

            InstKind::ExtDelegateCall { addr, args_offset, args_size } => {
                self.prepare_fresh_operands(func, &[*addr, *args_offset, *args_size]);
                self.emit_value_fresh(func, *args_size);
                self.emit_value_fresh(func, *args_offset);
                self.emit_value_fresh(func, *addr);
                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(
                    op::EXTDELEGATECALL,
                    StackEffect { pops: 3, pushes: 1 },
                    push,
                );
            }

            InstKind::ExtStaticCall { addr, args_offset, args_size } => {
                self.prepare_fresh_operands(func, &[*addr, *args_offset, *args_size]);
                self.emit_value_fresh(func, *args_size);
                self.emit_value_fresh(func, *args_offset);
                self.emit_value_fresh(func, *addr);
                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(
                    op::EXTSTATICCALL,
                    StackEffect { pops: 3, pushes: 1 },
                    push,
                );
            }

            InstKind::InternalCall { function, args, returns } => {
                self.preserve_stack_only_operands(args, liveness, block, inst_idx);
                self.emit_internal_call(
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
                // LOG2(offset, size, topic1, topic2) - stack order: offset, size, topic1, topic2
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
            | InstKind::MappingSlotCalldata(_, _)
            | InstKind::StorageArrayDataSlot(_)
            | InstKind::StorageArrayElementSlot { .. } => {
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
            | InstKind::MemoryObjectLoadField { .. }
            | InstKind::MemoryObjectStoreField { .. }
            | InstKind::MemoryObjectLoadElement { .. }
            | InstKind::MemoryObjectLoadByte { .. }
            | InstKind::MemoryObjectStoreElement { .. }
            | InstKind::MemoryObjectStoreByte { .. }
            | InstKind::MemoryObjectStoreWord { .. }
            | InstKind::MemorySliceLoadWord { .. }
            | InstKind::CalldataSliceLoadWord { .. }
            | InstKind::MemoryObjectCopyFromSlice { .. }
            | InstKind::MemoryObjectCopyFromSliceAt { .. }
            | InstKind::MemoryObjectCopy { .. }
            | InstKind::FrameLoad { .. }
            | InstKind::FrameStore { .. }
            | InstKind::Keccak256Bytes(_) => {
                unreachable!("semantic memory instructions must be lowered before EVM codegen")
            }

            InstKind::MemoryZero(_, _) => {
                unreachable!("memory-zero instructions must be lowered before EVM codegen")
            }

            InstKind::AbiEncode { .. } => {
                unreachable!("ABI encoding must be lowered before EVM codegen")
            }

            InstKind::AbiDecode { .. } => {
                unreachable!("ABI decoding must be lowered before EVM codegen")
            }

            InstKind::StorageToMemory { .. }
            | InstKind::MemoryToStorage { .. }
            | InstKind::ClearStorage { .. } => {
                unreachable!("aggregate operations must be lowered before EVM codegen")
            }
        }

        if let Some(result) = result_value
            && ((liveness.live_out(block).contains(result)
                && !self.is_stack_phi_source(block, result))
                || (self.scheduler.spills.requires_store(result)
                    && !self.scheduler.spills.is_stored(result)))
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
            self.asm.emit_op(op.opcode());
        }
        #[cfg(debug_assertions)]
        {
            debug_assert!(self.scheduler.depth() <= 1024);
        }
    }

    /// Bounds how many words an instruction can place above a stack-only operand before reaching
    /// it. Ordinary operations consume one operand while arranging the rest, but an internal call
    /// also pushes its return label before emitting stack-passed arguments.
    fn instruction_transient_growth(kind: &InstKind, operands: usize) -> usize {
        if matches!(kind, InstKind::InternalCall { .. }) {
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
        self.emit_current_internal_frame_addr(offset);
    }

    /// Removes the unused dynamic-frame header and stack-return words from a static frame.
    fn compact_static_frame_offset(&self, func_id: FunctionId, offset: u64) -> u64 {
        // The stack-return compaction below must stay active on the frame-backed
        // fallback (`runtime_stack_args == false`): `emitted_frame_size` subtracts
        // the returned words in both modes, so skipping the shift here would place
        // locals and spills beyond the reserved frame.
        let mut compact = if self.runtime_stack_args {
            offset
                .checked_sub(EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE)
                .expect("static frame header is still referenced")
        } else {
            offset
        };
        if let Some(plan) = self.stack_return_plan(func_id) {
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
    /// the callee are also excluded defensively. This convention is runtime-gas-only: size mode
    /// retains shared frame slots rather than adding stack shuffles at every return.
    fn compute_stack_return_plans(&mut self, module: &Module) {
        for abi in self.static_call_abis.values_mut() {
            abi.returns = None;
        }
        if !self.stack_returns_enabled || !self.gcx.sess.opts.optimization.is_gas() {
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
                && !self.disabled_stack_only_functions.contains(func_id)
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
                if let InstKind::InternalCall { function, returns, .. } = &func.inst(inst_id).kind
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
    /// the drain for immediates and position-independently reloadable caller
    /// arguments, or through a freshness-validated spill reload for computed
    /// values. The per-argument choice is scored across all sites — raw and
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
                        InstKind::InternalCall { function, .. }
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
                    let InstKind::InternalCall { function, args, .. } = &func.inst(inst_id).kind
                    else {
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
        if !self.gcx.sess.opts.optimization.is_gas() {
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
        func: &Function,
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
            phi_plan: has_phis.then(|| StackPhiPlan::analyze(func)),
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
            }) || phi_plan.branch_edges.iter().any(
                |(&pred, branch)| {
                    let term = func.blocks[pred]
                        .terminator
                        .as_ref()
                        .expect("stack-phi predecessor has no terminator");
                    plan.branch_layouts(term)
                        .or_else(|| plan.edge_layout(func, term).map(|layout| (layout, layout)))
                        .is_some_and(|(then, else_)| {
                            [(&branch.then_edge, then), (&branch.else_edge, else_)].into_iter().any(
                                |(edge, layout)| {
                                    layout.iter().any(|value| edge.sources.contains(value))
                                },
                            )
                        })
                },
            );
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
        let padded_entry =
            ScheduleCost::stack_op(StackOp::Swap(1)).plus(ScheduleCost::stack_op(StackOp::Pop));
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
        let resident_access = ScheduleCost::stack_op(StackOp::Dup(1));
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
        let context = Self::resident_search_context(func, values, has_phis);
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
        has_phis: bool,
    ) -> Option<(Vec<ValueId>, GlobalStackPlan)> {
        if !self.gcx.sess.opts.optimization.is_gas()
            || !Self::is_external_entry(func)
            || func.blocks.len() < 3
        {
            return None;
        }

        let inst_blocks = func.inst_blocks();
        let cross_block = Self::cross_block_live_values(func, liveness);
        let mut uses =
            FxHashMap::<ValueId, (BlockId, usize, FxHashSet<BlockId>, bool, bool, bool)>::default();
        for value in &cross_block {
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

        let spill_store = ScheduleCost::spill_store(OperandCostModel::DIRECT);
        let spill_load = ScheduleCost::memory_load(OperandCostModel::DIRECT);
        let resident_access = ScheduleCost::stack_op(StackOp::Dup(1));
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
        let context = Self::resident_search_context(func, values, has_phis);
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
        if !self.gcx.sess.opts.optimization.is_gas() {
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
                    let InstKind::InternalCall { function, args, .. } = &caller.inst(inst_id).kind
                    else {
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
                    && block.instructions.iter().any(|&inst_id| {
                        matches!(func.inst(inst_id).kind, InstKind::InternalCall { .. })
                    })
            }) {
                continue;
            }
            let has_phis =
                func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::Phi(_)));
            let liveness = (func.blocks.len() != 1 || has_phis).then(|| Liveness::compute(func));
            let plan = if let Some(liveness) = &liveness {
                let context = Self::resident_search_context(func, &values, has_phis);
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
        if !self.gcx.sess.opts.optimization.is_gas() {
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
                    let is_call = matches!(kind, InstKind::InternalCall { .. });
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
        if !self.gcx.sess.opts.optimization.is_gas() {
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
                    .any(|&inst| matches!(func.inst(inst).kind, InstKind::InternalCall { .. }))
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
        if !self.gcx.sess.opts.optimization.is_gas() {
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
    /// its stack drain: an immediate, or a caller argument whose reload is
    /// position independent.
    fn raw_arg_emittable(func: &Function, raw_leaves_ok: bool, val: ValueId) -> bool {
        match func.value(val) {
            crate::mir::Value::Immediate(imm) => imm.as_u256().is_some(),
            crate::mir::Value::Arg(_) => raw_leaves_ok,
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
        if let Some(depth) = caller_stack.and_then(|stack| stack.find(val)) {
            let dup = depth + words_above + 1;
            assert!(
                dup <= MAX_STACK_ACCESS,
                "resident caller argument exceeded DUP16 reach at an internal call"
            );
            self.asm.emit_op(op::dup(dup as u8));
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
                    self.asm.emit_push(U256::from(4 + (index.index() as u64) * 32));
                    self.asm.emit_op(op::CALLDATALOAD);
                }
            }
            crate::mir::Value::Inst(_) => {
                let slot = spill_slot.expect("computed stack argument has a validated spill slot");
                self.emit_spill_slot_addr(func, slot);
                self.asm.emit_op(op::MLOAD);
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
                    self.asm.emit_op(op::swap(depth as u8));
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
            let mut scheduler = StackScheduler::new();
            scheduler.stack = incoming;
            let shuffle = scheduler.shuffle_to_layout(&target).unwrap_or_else(|| {
                panic!("could not construct selective resident entry layout for `{}`", func.name)
            });
            for op in shuffle.ops {
                self.asm.emit_op(op.opcode());
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
            let value = self.scheduler.stack.iter().enumerate().find_map(|(depth, value)| {
                value.filter(|&value| {
                    depth >= materialize_depth && self.scheduler.is_stack_only_value(value)
                })
            });
            let Some(value) = value else { break };
            let crate::mir::Value::Arg(index) = func.value(value) else {
                unreachable!("only arguments may use the stack-only calling convention")
            };
            self.materialize_stack_arg(func_id, *index, value);
            disabled_residency = true;
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
        if let Some(plan) = self.stack_return_plan(func_id) {
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
    /// caller contributes its frame size, a dynamic caller (recursive, or an
    /// external entry whose locals live below the region) only forwards its
    /// own depth, so a static function reached THROUGH a dynamic one is still
    /// placed above its static ancestors. Static functions are acyclic by
    /// construction, so every cycle in the graph is weight-zero and the
    /// relaxation converges. Functions that can never be simultaneously live
    /// end up sharing addresses; that is the point of the overlay.
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
                        InstKind::InternalCall { function, .. }
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
        let entry_bases: FxHashMap<FunctionId, u64> = runtime_entries
            .iter()
            .copied()
            .map(|func_id| {
                (
                    func_id,
                    Self::external_spill_base(
                        &module.functions[func_id],
                        uses_dynamic_internal_frames,
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
                if let InstKind::InternalCall { function, .. } = func.inst(inst_id).kind {
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
                if self.static_frame_functions.contains(caller) {
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
        let mut static_span = 0;
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
            let relative = depth.get(&func_id).copied().unwrap_or(0);
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
                        depth.get(&func_id).copied().unwrap_or(0)
                            + self.emitted_frame_size(module, func_id)
                    })
                    .max()
                    .unwrap_or(0);
                (entry, span)
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
                        let relative = depth.get(&static_func).copied().unwrap_or(0) + offset;
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
                                let offset = rank as u64 * 32;
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
                Self::external_spill_base(&module.functions[func_id], uses_dynamic_internal_frames)
                    + static_alloc_sizes.get(&func_id).copied().unwrap_or(0);
            for (rank, (id, _)) in spills.into_iter().enumerate() {
                self.asm.set_deferred_const(id, U256::from(base + rank as u64 * 32));
            }
        }

        let max_entry_end = entry_ends.values().copied().max().unwrap_or(0);
        let (region_start, _) = layout(max_entry_end);
        for (&(func_id, offset), &(id, _)) in &self.static_frame_addr_consts {
            let relative = depth.get(&func_id).copied().unwrap_or(0) + offset;
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

    fn external_spill_base(func: &Function, dynamic_frames_enabled: bool) -> u64 {
        let low_memory_start = if dynamic_frames_enabled && Self::uses_internal_frame_slot(func) {
            EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT + EvmMemoryLayout::WORD_SIZE
        } else {
            EvmMemoryLayout::HEAP_START
        };
        low_memory_start + func.internal_frame_size.max(func.external_static_return_size)
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
        func.instructions()
            .any(|inst_id| matches!(func.inst(inst_id).kind, InstKind::InternalCall { .. }))
    }

    fn emit_entry_free_memory_start(
        &mut self,
        module: &Module,
        call_graph: &CallGraphInfo,
        entry: FunctionId,
    ) {
        let mut reachable = call_graph.reachable_callees_from([entry]);
        reachable.insert(entry);
        let needs_free_memory = reachable.iter().any(|func_id| {
            call_graph.is_recursive(func_id)
                || Self::function_may_observe_free_memory_slot(&module.functions[func_id])
                || module.functions[func_id].instructions().any(|inst_id| {
                    matches!(
                        module.functions[func_id].inst(inst_id).kind,
                        InstKind::InternalCall { function, returns, .. }
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
        self.runtime_entry_reachability.insert(entry, reachable);
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
            let spill_base = EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                + (func.params.len() as u64) * EvmMemoryLayout::WORD_SIZE
                + (func.returns.len() as u64) * EvmMemoryLayout::WORD_SIZE;
            self.emit_own_frame_addr(
                spill_base + func.internal_frame_size + u64::from(slot.offset) * 32,
            );
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
        self.emit_own_frame_addr(
            EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                + (index.index() as u64) * EvmMemoryLayout::WORD_SIZE,
        );
        self.asm.emit_op(op::MLOAD);
    }

    /// Returns the first internal-call result only when it is consumed. The call itself remains
    /// effectful, and additional returns are staged separately in the multi-return buffer.
    fn live_internal_call_result(
        result: Option<ValueId>,
        returns: usize,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) -> Option<ValueId> {
        result.filter(|&result| returns > 0 && !liveness.is_dead_after(result, block, inst_idx))
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_internal_call(
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
            let (preserved_words, argument_words) = self.emit_internal_call_static(
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
            self.internal_call_stack_edges.push(InternalCallStackEdge {
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
        self.spill_live_stack_values(func, liveness, block, inst_idx);

        self.emit_new_internal_frame_base_tracked();

        // frame[32] = previous frame pointer
        self.asm.emit_push(U256::from(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT));
        self.asm.emit_op(op::MLOAD);
        self.scheduler.stack.push_unknown();
        self.emit_internal_frame_store_from_top_preserving_base(32);

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
                self.asm.emit_op(op.opcode());
            }
            Some(self.scheduler.stack.clone())
        };
        let preserved_words = caller_stack.as_ref().map_or(0, StackModel::depth);
        self.internal_call_stack_edges.push(InternalCallStackEdge {
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
        self.asm.emit_push_label(return_label);

        self.asm.emit_push_label(callee_label);
        self.asm.emit_op(op::JUMP);

        self.asm.define_label(return_label);
        if let Some(caller_stack) = caller_stack {
            self.scheduler.stack = caller_stack;
        } else {
            self.scheduler.clear_stack();
        }

        if let Some(result) =
            Self::live_internal_call_result(result, returns, liveness, block, inst_idx)
        {
            self.emit_current_internal_frame_addr(
                EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                    + (args.len() as u64) * EvmMemoryLayout::WORD_SIZE,
            );
            self.asm.emit_op(op::MLOAD);
            self.scheduler.stack.push(result);
            // Store the result to its reserved slot now, while it is on top.
            // Other value-producing instructions do this; internal calls did
            // not, so a reserved result (e.g. a recompute leaf of a live-out
            // cheap value) was never stored. No-op unless reserved and live.
            self.spill_top_value_if_live(func, liveness, block, inst_idx, result);
        }

        // Copy returns 2..N to an ephemeral buffer at the current free-memory
        // pointer. Keep the base below the loop and publish it through the
        // dedicated scratch word afterwards; the first return stays on the
        // stack. This happens before restoring the frame pointer while the
        // callee frame remains addressable.
        if returns > 1 {
            self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
            self.asm.emit_op(op::MLOAD);
            self.asm.emit_push(U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT));
            self.asm.emit_op(op::MSTORE);
            for i in 1..returns {
                self.emit_current_internal_frame_addr(
                    EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                        + (args.len() as u64) * EvmMemoryLayout::WORD_SIZE
                        + (i as u64) * EvmMemoryLayout::WORD_SIZE,
                );
                self.asm.emit_op(op::MLOAD);
                self.asm.emit_push(U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT));
                self.asm.emit_op(op::MLOAD);
                self.asm.emit_push(U256::from((i as u64) * 32));
                self.asm.emit_op(op::ADD);
                self.asm.emit_op(op::MSTORE);
            }
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
        self.emit_current_internal_frame_addr(32);
        self.asm.emit_op(op::MLOAD);
        self.asm.emit_push(U256::from(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT));
        self.asm.emit_op(op::MSTORE);
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

        let &next_inst = func.blocks[block].instructions.get(inst_idx + 1)?;
        let orders = Self::static_call_operand_orders(&func.inst(next_inst).kind);
        let needed = orders.first()?;
        if self.first_stack_value_not_needed_by(needed).is_some() {
            return None;
        }

        let live_result =
            Self::live_internal_call_result(result, returns, liveness, block, inst_idx);
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
                drain_cost = drain_cost.plus(ScheduleCost::spill_store(cost_model));
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
                self.gcx.sess.opts.evm_version,
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
                self.gcx.sess.opts.evm_version,
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
            .map(|_| StaticCallStackPlan { caller_stack: self.scheduler.stack.clone() })
    }

    fn static_call_operand_orders(kind: &InstKind) -> SmallVec<[SmallVec<[ValueId; 3]>; 2]> {
        let mut orders = SmallVec::new();
        let binary = match kind {
            InstKind::Add(a, b)
            | InstKind::Mul(a, b)
            | InstKind::And(a, b)
            | InstKind::Or(a, b)
            | InstKind::Xor(a, b)
            | InstKind::Eq(a, b)
            | InstKind::Lt(a, b)
            | InstKind::Gt(a, b)
            | InstKind::SLt(a, b)
            | InstKind::SGt(a, b) => Some((*a, *b, true)),
            InstKind::Sub(a, b)
            | InstKind::Div(a, b)
            | InstKind::SDiv(a, b)
            | InstKind::Mod(a, b)
            | InstKind::SMod(a, b)
            | InstKind::Exp(a, b)
            | InstKind::Shl(a, b)
            | InstKind::Shr(a, b)
            | InstKind::Sar(a, b)
            | InstKind::Byte(a, b)
            | InstKind::Keccak256(a, b)
            | InstKind::SignExtend(a, b) => Some((*a, *b, false)),
            _ => None,
        };
        if let Some((a, b, swappable)) = binary {
            orders.push(smallvec::smallvec![b, a]);
            if swappable && a != b {
                orders.push(smallvec::smallvec![a, b]);
            }
            return orders;
        }

        match kind {
            InstKind::Not(a)
            | InstKind::Clz(a)
            | InstKind::IsZero(a)
            | InstKind::MLoad(a)
            | InstKind::SLoad(a)
            | InstKind::TLoad(a)
            | InstKind::CalldataLoad(a)
            | InstKind::Balance(a)
            | InstKind::BlockHash(a)
            | InstKind::BlobHash(a)
            | InstKind::ExtCodeSize(a)
            | InstKind::ExtCodeHash(a) => orders.push(smallvec::smallvec![*a]),
            InstKind::AddMod(a, b, n) | InstKind::MulMod(a, b, n) => {
                orders.push(smallvec::smallvec![*n, *b, *a]);
            }
            InstKind::Select(condition, if_true, if_false) => {
                orders.push(smallvec::smallvec![*if_false, *if_true, *condition]);
            }
            _ => {}
        }
        orders
    }

    /// Call to a static-frame callee: arguments are stored at absolute
    /// addresses, the return address rides the EVM stack (same invariants as
    /// the dynamic path), and there is no frame-pointer save/update/restore
    /// and no free-pointer traffic — the callee's frame is a fixed region
    /// below the heap that its single live activation owns.
    #[allow(clippy::too_many_arguments)]
    fn emit_internal_call_static(
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
        let mut resident_call_values = Vec::new();
        if self.preserve_caller_stack
            && let Some(resident) = self.resident_stack_args(func_id)
        {
            resident_call_values.extend(resident.iter().copied().filter(|&value| {
                self.scheduler.is_stack_only_value(value)
                    && (liveness.is_used_at_or_after(value, block, inst_idx + 1)
                        || stack_mask.as_ref().is_some_and(|mask| {
                            args.iter()
                                .enumerate()
                                .any(|(index, &arg)| mask.contains(index) && arg == value)
                        }))
            }));
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
        if caller_stack_plan.is_none() {
            // The fallback drains the caller stack, so park every value needed after the call
            // before consuming arguments.
            self.spill_live_stack_values(func, liveness, block, inst_idx);
        }

        for (i, &arg) in args.iter().enumerate() {
            if stack_mask.as_ref().is_some_and(|mask| mask.contains(i)) {
                continue;
            }
            self.emit_operand(func, arg);
            let addr = self.static_frame_addr(
                callee,
                EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                    + (i as u64) * EvmMemoryLayout::WORD_SIZE,
            );
            self.asm.emit_push_deferred(addr);
            self.scheduler.stack.push_unknown();
            self.asm.emit_op(op::MSTORE);
            self.scheduler.instruction_executed(2, None);
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
                    && matches!(func.value(arg), crate::mir::Value::Inst(_))
                {
                    let slot = if let Some(slot) = self.scheduler.reloadable_spill(arg) {
                        slot
                    } else {
                        self.emit_value(func, arg);
                        self.spill_value_if_needed(func, arg);
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
                self.asm.emit_op(op.opcode());
            }
            Some(self.scheduler.stack.clone())
        } else {
            caller_stack_plan.map(|mut plan| {
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

        self.asm.emit_push_label(return_label);
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
                self.asm.emit_op(op.opcode());
            }
        }
        self.asm.emit_push_label(callee_label);
        self.asm.emit_op(op::JUMP);

        self.asm.define_label(return_label);
        if let Some(caller_stack) = caller_stack {
            self.scheduler.stack = caller_stack;
        } else {
            self.scheduler.clear_stack();
        }

        if let Some(plan) = self.stack_return_plan(callee) {
            self.adopt_stack_call_results(func, plan, returns, result, liveness, block, inst_idx);
            return (preserved_words, argument_words);
        }

        if let Some(result) =
            Self::live_internal_call_result(result, returns, liveness, block, inst_idx)
        {
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

        // Copy return values 2..N into the same ephemeral buffer as the
        // dynamic-frame path.
        if returns > 1 {
            self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
            self.asm.emit_op(op::MLOAD);
            self.asm.emit_push(U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT));
            self.asm.emit_op(op::MSTORE);
            for i in 1..returns {
                let addr = self.static_frame_addr(
                    callee,
                    EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                        + ((args.len() + i) as u64) * EvmMemoryLayout::WORD_SIZE,
                );
                self.asm.emit_push_deferred(addr);
                self.asm.emit_op(op::MLOAD);
                self.asm.emit_push(U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT));
                self.asm.emit_op(op::MLOAD);
                self.asm.emit_push(U256::from((i as u64) * 32));
                self.asm.emit_op(op::ADD);
                self.asm.emit_op(op::MSTORE);
            }
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
                Self::live_internal_call_result(result, returns, liveness, block, inst_idx)
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

            self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
            self.asm.emit_op(op::MLOAD);
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

        if let Some(result) =
            Self::live_internal_call_result(result, returns, liveness, block, inst_idx)
        {
            self.scheduler.stack.push(result);
            self.spill_top_value_if_live(func, liveness, block, inst_idx, result);
        } else {
            self.asm.emit_op(op::POP);
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
        if Self::is_rematerializable_value(func, value) {
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
        func: &Function,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        let stack_values: Vec<_> = self.scheduler.stack.iter().flatten().collect();
        for value in stack_values {
            if !liveness.is_dead_after(value, block, inst_idx) {
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
            self.gcx.sess.opts.evm_version,
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
            let alias_is_live = self
                .global_stack_aliases
                .get(&value)
                .is_some_and(|&alias| !liveness.is_dead_after(alias, block, inst_idx));
            let carried_arg_is_live = self.global_stack_active
                && matches!(func.value(value), crate::mir::Value::Arg(_))
                && !liveness.is_dead_after(value, block, inst_idx);
            if !preserved.contains(&value)
                && (!liveness.is_dead_after(value, block, inst_idx) || alias_is_live)
                && (!Self::is_rematerializable_value(func, value) || carried_arg_is_live)
                && (scheduler.reloadable_spill(value).is_none() || scheduler.stack.contains(value))
            {
                preserved.push(value);
            }
        }
        preserved
    }

    fn emit_operand_plan(&mut self, func: &Function, plan: OperandPlan) {
        let ops = self.scheduler.apply_operand_plan(plan);
        self.emit_scheduled_ops(func, ops);
    }

    fn emit_scheduled_ops(&mut self, func: &Function, ops: Vec<ScheduledOp>) {
        for op in ops {
            match op {
                ScheduledOp::Stack(stack_op) => {
                    self.asm.emit_op(stack_op.opcode());
                }
                ScheduledOp::PushImmediate(imm) => {
                    self.asm.emit_push(imm);
                }
                ScheduledOp::LoadSpill(slot) => {
                    // PUSH slot_offset, MLOAD
                    self.emit_spill_slot_addr(func, slot);
                    self.asm.emit_op(op::MLOAD);
                }
                ScheduledOp::LoadArg(index) => {
                    if self.in_internal_function {
                        self.emit_internal_arg_load(index);
                    } else if self.in_constructor {
                        self.emit_constructor_arg_load(index);
                    } else {
                        // Runtime function: load from calldata
                        // ABI encoding: selector (4 bytes) + args (32 bytes each)
                        // Offset = 4 + index * 32
                        let offset = 4 + (index.index() as u64) * 32;
                        self.asm.emit_push(U256::from(offset));
                        self.asm.emit_op(op::CALLDATALOAD);
                    }
                }
            }
        }
    }

    fn emit_value_impl(&mut self, func: &Function, val: ValueId, claim_top: bool) {
        if let Some(depth) = self.scheduler.stack.find(val)
            && depth >= MAX_STACK_ACCESS
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

        let ops = if claim_top {
            self.scheduler.ensure_on_top(val, func)
        } else {
            self.scheduler.ensure_operand_on_top(val, func)
        }
        .to_vec();
        self.emit_scheduled_ops(func, ops);
    }

    /// Emits a value fresh, without trying to DUP from the stack.
    /// This is used for CALL operands where we need to guarantee correct values
    /// regardless of scheduler stack tracking state.
    fn emit_value_fresh(&mut self, func: &Function, val: ValueId) {
        match func.value(val) {
            crate::mir::Value::Immediate(imm) => {
                if let Some(u256) = imm.as_u256() {
                    self.asm.emit_push(u256);
                    self.scheduler.stack.push(val);
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
                    assert!(depth < MAX_STACK_ACCESS, "stack-only argument exceeded DUP16 reach");
                    self.emit_stack_op(StackOp::Dup(depth as u8 + 1));
                    return;
                }
                if let Some(depth) = self.scheduler.stack.find(val)
                    && depth < MAX_STACK_ACCESS
                {
                    self.emit_stack_op(StackOp::Dup(depth as u8 + 1));
                    return;
                }
                if self.in_internal_function {
                    self.emit_internal_arg_load(*index);
                } else if self.in_constructor {
                    self.emit_constructor_arg_load(*index);
                } else {
                    let offset = 4 + (index.index() as u64) * 32;
                    self.asm.emit_push(U256::from(offset));
                    self.asm.emit_op(op::CALLDATALOAD);
                }
                self.scheduler.stack.push(val);
            }
            crate::mir::Value::Inst(inst_id) => {
                // A value carried on the live stack is the current definition;
                // duplicate it instead of reloading or recomputing. A preserved
                // edge can carry a value that was never spilled, and
                // recomputing a definition such as an FMP load would observe
                // memory that changed since the definition executed.
                if let Some(depth) = self.scheduler.stack.find(val)
                    && depth < MAX_STACK_ACCESS
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
                    self.emit_spill_slot_addr(func, slot);
                    self.asm.emit_op(op::MLOAD);
                    self.scheduler.stack.push(val);
                } else {
                    // Check if the instruction is one that we can "re-execute" to get a fresh value
                    // This handles GAS (which is always fresh) and MLOAD (which re-reads from
                    // memory)
                    let inst_kind = &func.inst(*inst_id).kind;
                    match inst_kind {
                        crate::mir::InstKind::Gas => {
                            self.asm.emit_op(op::GAS);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::LoadImmutable(id) if !self.in_constructor => {
                            self.emit_load_immutable(*id);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::CallValue => {
                            self.asm.emit_op(op::CALLVALUE);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::Caller => {
                            self.asm.emit_op(op::CALLER);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::Origin => {
                            self.asm.emit_op(op::ORIGIN);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::CalldataSize => {
                            self.asm.emit_op(op::CALLDATASIZE);
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
                        crate::mir::InstKind::Timestamp => {
                            self.asm.emit_op(op::TIMESTAMP);
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::BlockNumber => {
                            self.asm.emit_op(op::NUMBER);
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
                                    self.emit_spill_slot_addr(func, slot);
                                    self.asm.emit_op(op::MLOAD);
                                    self.scheduler.stack.push(val);
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
                        crate::mir::InstKind::Keccak256(offset, size) => {
                            // Re-emit KECCAK256 - memory content should still be valid.
                            // KECCAK256 reads s[0] = offset, s[1] = size, so emit the
                            // offset last so it ends up on top.
                            self.emit_value_fresh(func, *size);
                            self.emit_value_fresh(func, *offset);
                            self.asm.emit_op(op::KECCAK256);
                            // Pop offset and size, push result
                            self.scheduler.stack.pop();
                            self.scheduler.stack.pop();
                            self.scheduler.stack.push(val);
                        }
                        crate::mir::InstKind::Not(value) => {
                            self.emit_fresh_unary(func, val, *value, op::NOT);
                        }
                        crate::mir::InstKind::Clz(value) => {
                            self.emit_fresh_unary(func, val, *value, op::CLZ);
                        }
                        crate::mir::InstKind::IsZero(value) => {
                            self.emit_fresh_unary(func, val, *value, op::ISZERO);
                        }
                        crate::mir::InstKind::Byte(index, value) => {
                            self.emit_fresh_binary(func, val, *index, *value, op::BYTE, false);
                        }
                        crate::mir::InstKind::SignExtend(index, value) => {
                            self.emit_fresh_binary(
                                func,
                                val,
                                *index,
                                *value,
                                op::SIGNEXTEND,
                                false,
                            );
                        }
                        crate::mir::InstKind::Add(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::ADD, true);
                        }
                        crate::mir::InstKind::Sub(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::SUB, false);
                        }
                        crate::mir::InstKind::Mul(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::MUL, true);
                        }
                        crate::mir::InstKind::And(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::AND, true);
                        }
                        crate::mir::InstKind::Or(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::OR, true);
                        }
                        crate::mir::InstKind::Xor(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::XOR, true);
                        }
                        crate::mir::InstKind::Shl(shift, value) => {
                            self.emit_fresh_binary(func, val, *shift, *value, op::SHL, false);
                        }
                        crate::mir::InstKind::Shr(shift, value) => {
                            self.emit_fresh_binary(func, val, *shift, *value, op::SHR, false);
                        }
                        crate::mir::InstKind::Div(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::DIV, false);
                        }
                        crate::mir::InstKind::SDiv(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::SDIV, false);
                        }
                        crate::mir::InstKind::Mod(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::MOD, false);
                        }
                        crate::mir::InstKind::SMod(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::SMOD, false);
                        }
                        crate::mir::InstKind::Lt(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::LT, false);
                        }
                        crate::mir::InstKind::Gt(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::GT, false);
                        }
                        crate::mir::InstKind::SLt(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::SLT, false);
                        }
                        crate::mir::InstKind::SGt(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::SGT, false);
                        }
                        crate::mir::InstKind::Eq(a, b) => {
                            self.emit_fresh_binary(func, val, *a, *b, op::EQ, true);
                        }
                        crate::mir::InstKind::Sar(shift, value) => {
                            self.emit_fresh_binary(func, val, *shift, *value, op::SAR, false);
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
                                if depth < 16 {
                                    self.asm.emit_op(op::DUP1 + depth as u8);
                                    self.scheduler.stack.push(val);
                                } else {
                                    let slot = self.scheduler.spills.allocate(val);
                                    self.spill_deep_stack_value(func, val, slot, depth);
                                    self.emit_spill_slot_addr(func, slot);
                                    self.asm.emit_op(op::MLOAD);
                                    self.scheduler.stack.push(val);
                                }
                            } else if let Some(slot) = self.scheduler.reloadable_spill(val) {
                                // A defining block emitted later still stores
                                // this slot before the load executes at runtime.
                                self.emit_spill_slot_addr(func, slot);
                                self.asm.emit_op(op::MLOAD);
                                self.scheduler.stack.push(val);
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

    fn emit_fresh_unary(&mut self, func: &Function, result: ValueId, value: ValueId, opcode: u8) {
        self.emit_value_fresh(func, value);
        self.asm.emit_op(opcode);
        self.scheduler.stack.pop();
        self.scheduler.stack.push(result);
    }

    fn swapped_binary_opcode(opcode: u8) -> Option<u8> {
        Some(match opcode {
            op::ADD | op::MUL | op::AND | op::OR | op::XOR | op::EQ => opcode,
            op::LT => op::GT,
            op::GT => op::LT,
            op::SLT => op::SGT,
            op::SGT => op::SLT,
            _ => return None,
        })
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
            && let Some(swapped_opcode) = Self::swapped_binary_opcode(opcode)
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
            // DUP for the second operand
            self.asm.emit_op(op::DUP1);
            self.scheduler.stack.dup(1);
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
            self.asm.emit_op(op::SWAP1);
            self.scheduler.stack_swapped();
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
            self.asm.emit_op(op::SWAP1);
            self.scheduler.stack_swapped();
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
            self.asm.emit_op(op::SWAP1);
            self.scheduler.stack_swapped();
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
                self.asm.emit_op(op::SWAP1);
                self.scheduler.stack_swapped();
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
                    // DUP the temp value to top of stack
                    if let Some(depth) = self.scheduler.stack.find(temp_val) {
                        let dup_n = (depth + 1) as u8;
                        self.asm.emit_op(op::dup(dup_n));
                        self.scheduler.stack.dup(dup_n);
                    }
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
            self.asm.emit_op(op::POP);
            self.scheduler.stack.pop();
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
                self.asm.emit_op(op.opcode());
            }

            // Rotate the untracked return address from below the result tuple to the top without
            // disturbing result order: SWAP1, SWAP2, ..., SWAPN maps
            // [return, r0, ..., rN] to [r0, ..., rN, return].
            for depth in 1..=plan.arity {
                self.asm.emit_op(op::swap(depth as u8));
            }
            self.asm.emit_op(op::JUMP);
            self.scheduler.clear_stack();
            return;
        }

        let return_base = EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
            + (func.params.len() as u64) * EvmMemoryLayout::WORD_SIZE;
        for (i, &value) in values.iter().enumerate() {
            self.emit_operand(func, value);
            self.emit_own_frame_addr(return_base + (i as u64) * 32);
            self.asm.emit_op(op::MSTORE);
            self.scheduler.stack.pop();
        }

        self.pop_all_stack_values();
        // The caller's return address is the untracked value at the bottom of
        // the stack; after popping every tracked value it is on top.
        self.asm.emit_op(op::JUMP);
    }

    fn emit_external_stop(&mut self) {
        if let Some(exit) = self.constructor_exit {
            self.asm.emit_push_label(exit);
            self.asm.emit_op(op::JUMP);
        } else {
            self.asm.emit_op(op::STOP);
        }
    }

    /// Emits the backend fallback for a semantic returndata-bubbling revert.
    ///
    /// The canonical pipeline lowers this terminator in `lower-abi`; keeping
    /// the emission here makes ad-hoc MIR pipelines fail closed instead of
    /// reaching an unreachable arm in the backend.
    fn emit_revert_returndata(&mut self) {
        if self.gcx.sess.opts.evm_version.supports_returndata() {
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
                    for (i, &arg) in args.iter().enumerate() {
                        if stack_mask.as_ref().is_some_and(|mask| mask.contains(i)) {
                            continue;
                        }
                        self.emit_operand(func, arg);
                        let addr = self.static_frame_addr(
                            *function,
                            EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                                + (i as u64) * EvmMemoryLayout::WORD_SIZE,
                        );
                        self.asm.emit_push_deferred(addr);
                        self.scheduler.stack.push_unknown();
                        self.asm.emit_op(op::MSTORE);
                        self.scheduler.instruction_executed(2, None);
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
                            self.asm.emit_op(op.opcode());
                        }
                    }
                }
                let label = self.function_labels[function];
                self.asm.emit_push_label(label);
                self.asm.emit_op(op::JUMP);
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
                self.asm.emit_push_label(self.block_labels[target]);
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
                        self.asm.emit_push_label(self.block_labels[then_block]);
                        self.asm.emit_op(op::JUMPI);
                        self.scheduler.stack.pop(); // condition consumed by JUMPI
                    }
                    Some(next) if *then_block == next => {
                        // Invert the condition so true falls through to `then_block`.
                        self.asm.emit_op(op::ISZERO);
                        self.scheduler.instruction_executed_untracked(1);
                        self.asm.emit_push_label(self.block_labels[else_block]);
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
                            self.asm.emit_push_label(self.block_labels[else_block]);
                            self.asm.emit_op(op::JUMPI);
                            self.scheduler.stack.pop(); // inverted condition consumed by JUMPI

                            self.asm.emit_push_label(self.block_labels[then_block]);
                            self.asm.emit_op(op::JUMP);
                        } else {
                            // JUMPI consumes the condition
                            self.asm.emit_push_label(self.block_labels[then_block]);
                            self.asm.emit_op(op::JUMPI);
                            self.scheduler.stack.pop(); // condition consumed by JUMPI

                            self.asm.emit_push_label(self.block_labels[else_block]);
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
                self.emit_external_stop();
            }

            Terminator::Revert { offset, size } => {
                self.emit_value(func, *size);
                self.emit_operand(func, *offset);
                self.asm.emit_op(op::REVERT);
            }

            Terminator::RevertReturndata => {
                self.emit_revert_returndata();
            }

            Terminator::ReturnData { offset, size } => {
                // Valid in internal functions too: a fused external body called
                // through an ABI wrapper returns straight to the external
                // caller, abandoning the internal frame.
                self.emit_value(func, *size);
                self.emit_operand(func, *offset);
                self.asm.emit_op(op::RETURN);
            }

            Terminator::Stop => {
                if self.in_internal_function {
                    self.emit_internal_return(func, &[]);
                } else {
                    self.emit_external_stop();
                }
            }

            Terminator::SelfDestruct { recipient } => {
                self.emit_value(func, *recipient);
                self.asm.emit_op(op::SELFDESTRUCT);
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
    use crate::mir::{FunctionBuilder, Immediate, Instruction, MirType, TypeSize, Value};
    use solar_config::CompileOpts;
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
    fn caller_stack_prefix_validation_rejects_overflow() {
        with_codegen(CompileOpts::default(), |mut codegen| {
            let mut module = Module::new(Ident::DUMMY);
            let mut entry = Function::new(Ident::with_dummy_span(sym::entry));
            entry.attributes.is_dispatch_entry = true;
            let entry = module.add_function(entry);
            let callee = module.add_function(Function::new(Ident::with_dummy_span(sym::Test)));

            codegen.recursive_stack_functions = DenseBitSet::new_empty(module.functions.len());
            codegen.function_stack_peaks.insert(entry, 1);
            codegen.function_stack_peaks.insert(callee, MAX_STACK_DEPTH - 1);
            codegen.internal_call_stack_edges.push(InternalCallStackEdge {
                caller: entry,
                callee,
                preserved_words: 1,
                argument_words: 0,
            });
            assert!(!codegen.caller_stack_prefixes_fit(&module));

            codegen.function_stack_peaks.insert(callee, MAX_STACK_DEPTH - 2);
            assert!(codegen.caller_stack_prefixes_fit(&module));

            // The transient argument tuple and target label must be budgeted even
            // when the preserved prefix and callee peak fit on their own.
            codegen.internal_call_stack_edges[0].preserved_words = MAX_STACK_DEPTH - 3;
            codegen.internal_call_stack_edges[0].argument_words = 2;
            codegen.function_stack_peaks.insert(callee, 2);
            assert!(!codegen.caller_stack_prefixes_fit(&module));

            codegen.internal_call_stack_edges[0].preserved_words = 0;
            codegen.internal_call_stack_edges[0].argument_words = MAX_STACK_DEPTH;
            codegen.function_stack_peaks.insert(callee, 0);
            assert!(!codegen.caller_stack_prefixes_fit(&module));
        });
    }

    #[test]
    fn dynamic_frame_stack_args_require_reloadable_values() {
        let mut function = Function::new(Ident::DUMMY);
        let argument = function.alloc_param(MirType::uint256());
        let immediate = function.alloc_value(Value::Immediate(Immediate::uint256(U256::from(1))));
        let (_, computed) = function.alloc_value_inst(Instruction::new(
            InstKind::Add(argument, immediate),
            Some(MirType::uint256()),
        ));

        assert!(EvmCodegen::stack_arg_site_eligible(&function, false, immediate));
        assert!(!EvmCodegen::stack_arg_site_eligible(&function, false, argument));
        assert!(EvmCodegen::stack_arg_site_eligible(&function, false, computed));
        assert!(EvmCodegen::stack_arg_site_eligible(&function, true, argument));
        assert!(EvmCodegen::stack_arg_site_eligible(&function, true, computed));
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
    fn internal_call_headroom_includes_return_label() {
        let value = ValueId::from_usize(0);
        let call = InstKind::InternalCall {
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
            let one = builder.imm_u64(1);
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
                let one = builder.imm_u64(1);
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
    fn unreachable_phi_copies_do_not_leak_between_functions() {
        with_codegen(CompileOpts::default(), |mut codegen| {
            let mut first = Function::new(Ident::with_dummy_span(sym::Test));
            let mut builder = FunctionBuilder::new(&mut first);
            let unreachable_pred = builder.create_block();
            let unreachable_merge = builder.create_block();
            builder.stop();
            builder.switch_to_block(unreachable_pred);
            let value = builder.imm_u64(1);
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
    fn cross_block_recomputation_requires_stable_leaves() {
        let mut function = Function::new(Ident::DUMMY);
        let argument = function.alloc_param(MirType::uint256());
        let immediate = function.alloc_value(Value::Immediate(Immediate::uint256(U256::from(1))));
        let (safe_inst, safe) = function.alloc_value_inst(Instruction::new(
            InstKind::Add(argument, immediate),
            Some(MirType::uint256()),
        ));
        let (nested_safe_inst, nested_safe) = function.alloc_value_inst(Instruction::new(
            InstKind::Mul(safe, argument),
            Some(MirType::uint256()),
        ));
        let (calldata_inst, calldata) = function.alloc_value_inst(Instruction::new(
            InstKind::CalldataLoad(safe),
            Some(MirType::uint256()),
        ));
        let (calldata_safe_inst, calldata_safe) = function.alloc_value_inst(Instruction::new(
            InstKind::Add(calldata, immediate),
            Some(MirType::uint256()),
        ));
        let (context_inst, context) = function
            .alloc_value_inst(Instruction::new(InstKind::CallValue, Some(MirType::uint256())));
        let (immutable_inst, immutable) = function.alloc_value_inst(Instruction::new(
            InstKind::LoadImmutable(ImmutableId::from_usize(0)),
            Some(MirType::uint256()),
        ));
        let (mutable_inst, mutable) = function.alloc_value_inst(Instruction::new(
            InstKind::SLoad(immediate),
            Some(MirType::uint256()),
        ));
        let (unsafe_inst, unsafe_value) = function.alloc_value_inst(Instruction::new(
            InstKind::Add(mutable, immediate),
            Some(MirType::uint256()),
        ));
        function.blocks[BlockId::ENTRY].instructions.extend([
            safe_inst,
            nested_safe_inst,
            calldata_inst,
            calldata_safe_inst,
            context_inst,
            immutable_inst,
            mutable_inst,
            unsafe_inst,
        ]);
        let recomputable = EvmCodegen::cross_block_recomputable_values_with(&function, |_| true);
        let without_argument =
            EvmCodegen::cross_block_recomputable_values_with(&function, |value| value != argument);

        assert!(recomputable.contains(safe));
        assert!(recomputable.contains(nested_safe));
        assert!(recomputable.contains(calldata));
        assert!(recomputable.contains(calldata_safe));
        assert!(recomputable.contains(context));
        assert!(!recomputable.contains(immutable));
        assert!(!recomputable.contains(mutable));
        assert!(!recomputable.contains(unsafe_value));
        assert!(!without_argument.contains(safe));
        assert!(!without_argument.contains(nested_safe));
        assert!(!without_argument.contains(calldata));
        assert!(!without_argument.contains(calldata_safe));
        assert!(without_argument.contains(context));
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
        let interferences =
            EvmCodegen::parallel_phi_interferences(&function, &colorable, &block_copies);
        assert!(interferences[&destination0].contains(&destination1));
        assert!(interferences[&destination0].contains(&source1));
        assert!(!interferences[&destination0].contains(&source0));
        assert!(!interferences[&destination1].contains(&source0));
    }
}
