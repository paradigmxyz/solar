//! Function inlining optimization pass.
//!
//! This module inlines profitable MIR internal calls to remove their call
//! protocol and expose further optimization opportunities. The dedicated single-use pass only
//! consumes one-call-site, frameless scalar helpers without reference returns. The
//! original body disappears through function DCE, so this avoids duplicating shared bodies.
//! Recursive calls, explicit no-inline functions, large helpers, and aggregate allocation
//! semantics stay with the existing call convention. It runs before late scalar cleanup.
//! For gas-oriented lifetime decisions, statically counted loops weight call-protocol
//! savings by their iteration count. Only blocks that dominate every latch in a loop
//! with a header guard and no other exit receive that weight. Conditional calls and
//! unknown loop bounds retain the ordinary per-invocation estimate; size mode keeps
//! its existing growth policy. These are profitability estimates, never legality facts.
//! Tiny forwarding wrappers may return one call result or forward a void call.
//! Both forms contain only that call and its internal return, so inlining exposes
//! the original call exactly once without cloning the callee body. Shared void
//! forwarders that add arguments stay shared: cloning their extra setup can
//! outweigh the removed wrapper and increase stack pressure at every caller.
//! Frameless one-word literal initializers also inline when each emitted artifact
//! contains at most one call site. Creation and runtime reachability are counted
//! separately, including tail calls and conservative roots for unrooted MIR.
//! Cloning retains each allocation and its initialization at the original call
//! site; arbitrary reference-returning helpers remain excluded.
//! Slice-only helpers are also transparent: constructing and projecting a slice
//! copies its pointer, length and address space without reading the payload or
//! allocating memory. Inlining exposes these components before slice lowering
//! expands a returned slice into the internal multi-word return convention.
//! The existing leaf-size and lifetime-cost limits still apply.
//! A targeted late adapter revisits argument-free immutable word helpers after
//! range checks and CFG cleanup make them straight-line leaves. It admits only
//! immutable loads and pure single-opcode computations, retaining the ordinary
//! tiny-leaf size and lifetime-cost limits. Inlining stays at the original call
//! site, including constructor calls; no runtime immutable bounds are assumed.
//! Small scalar helpers with phis may also inline at their sole call site.
//! Backward liveness estimates the callee's peak live words and the caller values
//! surviving the call. Their sum must fit twelve words, leaving stack-addressing
//! headroom for operand staging. The gas-only hot-leaf pass reuses that estimate
//! for bounded acyclic scalar helpers called from inside loops, with a ten-word
//! budget because the loop's carried words stay resident through every join a
//! clone adds: each such site is cloned when the call protocol it removes, weighed
//! over the loop's trip count (ten iterations when none is computable) and the
//! expected executions, repays the deposited copy; sites outside loops and
//! callees shared by more than eight sites keep the call. Read-only loops are eligible after
//! loop-idiom lowering when their bounded MIR shape replaces the original scalar loop;
//! writes and shared phi helpers remain excluded. This is a bounded profitability
//! estimate, not a promise that the scheduler will emit no spills.
//! A separate gas-only late adapter accepts frameless wrappers with one returning
//! call followed by at most five physical address/load/store operations. It clones
//! the call and subsequent memory operations in order, without moving accesses
//! across the call or assuming alias freedom. Both caller live words and wrapper
//! peak words must fit the same twelve-word budget, and lifetime gas must pay for
//! code growth. General allocators, branches, and larger memory helpers stay shared.
//! After inlining a multi-word return, immediate reads of the published return
//! buffer can use the returned SSA words directly. Only scalar word loads at
//! constant offsets before the next memory/effect barrier qualify. Publication
//! stores remain intact so later observers of the buffer retain their behavior;
//! ordinary memory DSE decides whether those stores are removable.

use crate::{
    backend::evm::{op, select},
    mir::{
        AbiLayout, AbiType, AllocationSemantics, BlockId, EffectKind, FrameMode, FrameSlotKind,
        Function, FunctionBuilder, FunctionId as MirFunctionId, Immediate, ImmutableEncoding,
        InstId, InstKind, Instruction, MemoryObjectKind, MirPhase, MirType, Module, Terminator,
        Value, ValueId,
        analysis::{CallGraphInfo, Liveness, LoopAnalyzer},
        immutable::immutable_push_type_size,
        memory::{EvmMemoryLayout, MemoryLayoutPolicy},
        pass::MirPass,
        utils::{replace_terminator_uses_canonicalized, resolve_replacement},
    },
    target::{Cost, Target},
};
use smallvec::SmallVec;
use solar_ast::StateMutability;
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    index::IndexVec,
    map::FxHashMap,
};
use solar_sema::Gcx;

/// Module pass for metadata-backed MIR inlining.
pub(crate) struct Inline;

impl MirPass for Inline {
    fn name(&self) -> &'static str {
        "inline"
    }

    fn run_pass(
        &self,
        gcx: Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let mut inliner = if gcx.sess.opts.optimization == solar_config::OptimizationMode::Size {
            MirInliner::for_size()
        } else {
            MirInliner::default()
        };
        inliner.run(gcx, module).inlined != 0
    }
}

/// Module pass for inlining only trivial leaf helpers in gas mode.
pub(crate) struct InlineTinyLeaves;

impl MirPass for InlineTinyLeaves {
    fn name(&self) -> &'static str {
        "inline-tiny-leaves"
    }

    fn run_pass(
        &self,
        gcx: Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let mut inliner = MirInliner::for_tiny_leaves();
        inliner.run(gcx, module).inlined != 0
    }
}

/// Revisits small immutable computations exposed by late check elimination.
pub(crate) struct InlineImmutableLeaves;

impl MirPass for InlineImmutableLeaves {
    fn name(&self) -> &'static str {
        "inline-immutable-leaves"
    }

    fn run_pass(
        &self,
        gcx: Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        if !module.functions.iter().any(is_immutable_word_leaf) {
            return false;
        }
        MirInliner { immutable_leaves_only: true, ..MirInliner::for_tiny_leaves() }
            .run(gcx, module)
            .inlined
            != 0
    }
}

/// Inlines small post-call memory wrappers after physical memory lowering.
pub(crate) struct InlineMemoryWrappers;

impl MirPass for InlineMemoryWrappers {
    fn name(&self) -> &'static str {
        "inline-memory-wrappers"
    }

    fn run_pass(
        &self,
        gcx: Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        if !gcx.sess.opts.optimization.is_gas() || !module.functions.iter().any(is_memory_wrapper) {
            return false;
        }
        MirInliner {
            memory_wrappers_only: true,
            max_instructions: 6,
            max_single_call_sanity_instructions: 6,
            ..MirInliner::for_tiny_leaves()
        }
        .run(gcx, module)
        .inlined
            != 0
    }
}

/// Clones bounded scalar helpers into the loops that call them in gas mode.
pub(crate) struct InlineHotLeaves;

impl MirPass for InlineHotLeaves {
    fn name(&self) -> &'static str {
        "inline-hot-leaves"
    }

    fn run_pass(
        &self,
        gcx: Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        if !gcx.sess.opts.optimization.is_gas() {
            return false;
        }
        MirInliner {
            mode: InlineMode::HotLeaves,
            max_shared_callee_blocks: 16,
            max_caller_inlined_instructions: 128,
            ..MirInliner::default()
        }
        .run(gcx, module)
        .inlined
            != 0
    }
}

/// Module pass for consuming a single-use helper without duplicating its body.
pub(crate) struct InlineSingleUse;

impl MirPass for InlineSingleUse {
    fn name(&self) -> &'static str {
        "inline-single-use"
    }

    fn run_pass(
        &self,
        gcx: Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let stats = MirInliner {
            mode: InlineMode::SingleUse,
            max_single_call_sanity_instructions: 256,
            frame_staging_allowed: module.phase < MirPhase::MemoryLowered,
            ..MirInliner::default()
        }
        .run(gcx, module);
        // The consumed bodies are dead now. Remove exactly those instead of a
        // module-wide dead-function sweep, which would also delete uncalled
        // functions that were never reachable, such as the subjects of
        // pipeline tests.
        if !stats.consumed.is_empty() {
            super::cfg_simplify::remove_unreferenced_functions(module, &stats.consumed);
        }
        stats.inlined != 0
    }
}

/// Module pass for specializing one constant-argument call to a shared pure leaf.
pub(crate) struct InlineConstantLeaves;

impl MirPass for InlineConstantLeaves {
    fn name(&self) -> &'static str {
        "inline-constant-leaves"
    }

    fn run_pass(
        &self,
        gcx: Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let mut inliner = MirInliner::for_constant_leaves();
        inliner.run(gcx, module).inlined != 0
    }
}

/// Module pass for specializing calls through constant internal function pointers.
pub(crate) struct SpecializeFunctionPointers;

impl MirPass for SpecializeFunctionPointers {
    fn name(&self) -> &'static str {
        "specialize-function-pointers"
    }

    fn run_pass(
        &self,
        _gcx: Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        specialize_function_pointers(module) != 0
    }
}

/// Module-level MIR internal-call inliner.
///
/// This pass clones small internal/private callees into their callers. Each
/// inline expansion gets a fresh internal-frame range so copied local slots do
/// not overlap caller locals.
struct MirInliner {
    /// Maximum instruction count for ordinary inline candidates.
    max_instructions: usize,
    /// Hard sanity limit for single-call-site callees. These bypass the normal
    /// size and block caps because function DCE removes their original body.
    max_single_call_sanity_instructions: usize,
    /// Maximum number of blocks to clone from one multi-use callee.
    max_blocks: usize,
    /// Maximum block count for a shared callee to inline at every profitable
    /// call site. Larger shared callees inline only at their best call site.
    max_shared_callee_blocks: usize,
    /// Whether a single call site may use the larger threshold.
    inline_single_call: bool,
    /// Maximum number of instructions a single caller may gain from inlining
    /// multi-use callees, bounding total code growth per function.
    max_caller_inlined_instructions: usize,
    /// Expected executions per deployment. This is supplied by Standard JSON
    /// optimizer runs and defaults to solc's 200-run convention.
    expected_executions_per_deployment: u64,
    /// The cost model pricing call protocol against deposited bytes.
    target: Target,
    /// Optional hard ceiling for the module size estimator. Normal gas-mode
    /// profitability is governed by lifetime cost instead of this ceiling;
    /// zero remains the explicit off switch used by size mode.
    max_module_code_size: usize,
    /// Restricts late expansion to argument-free immutable word computations.
    immutable_leaves_only: bool,
    /// Restricts late expansion to small post-call memory wrappers.
    memory_wrappers_only: bool,
    /// Whether multi-value returns may stage through semantic frame slots. Once
    /// frame slots are lowered to physical memory, a late run must leave such
    /// callees alone: the staging instructions would survive the phase boundary.
    frame_staging_allowed: bool,
    mode: InlineMode,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InlineMode {
    Normal,
    TinyLeaves,
    ConstantLeaves,
    SingleUse,
    HotLeaves,
}

/// Which callees get a stack-peak estimate when the module is summarized.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PeakAnalysis {
    /// No estimate; phi-carrying callees stay shared.
    None,
    /// Bounded scalar helpers with phis, for their sole call site.
    Phis,
    /// Every bounded scalar helper, for loop call sites.
    Scalars,
}

impl Default for MirInliner {
    fn default() -> Self {
        Self {
            max_instructions: 96,
            max_single_call_sanity_instructions: 4096,
            max_blocks: 16,
            max_shared_callee_blocks: 10,
            inline_single_call: true,
            max_caller_inlined_instructions: 64,
            expected_executions_per_deployment: Target::DEFAULT_EXPECTED_EXECUTIONS,
            target: Target::with(
                solar_config::EvmVersion::default(),
                solar_config::OptimizationMode::Gas,
                Target::DEFAULT_EXPECTED_EXECUTIONS,
            ),
            max_module_code_size: usize::MAX,
            immutable_leaves_only: false,
            memory_wrappers_only: false,
            frame_staging_allowed: true,
            mode: InlineMode::Normal,
        }
    }
}

impl MirInliner {
    /// How many times a loop without a computable trip count is assumed to
    /// run per invocation when a hot leaf is weighed: GCC's estimate for such
    /// loops. Counted loops use their real trip count instead.
    const UNCOUNTED_LOOP_EXECUTIONS: u64 = 10;
    /// A hot leaf shared by more call sites than this stays a call: every
    /// clone deposits the whole body again.
    const MAX_HOT_LEAF_CALL_SITES: usize = 8;
    /// Live words a caller may hold across an inlined body plus the body's own
    /// peak, leaving stack-addressing headroom for operand staging.
    const STACK_BUDGET: usize = 12;
    /// The tighter budget for hot leaves: a clone inside a loop body also keeps
    /// the loop's carried words resident through every join it adds, and the
    /// Base64 encoder lost half its gas to spills when the two budgets matched.
    const HOT_LEAF_STACK_BUDGET: usize = 10;

    /// The live-word budget for inlining at the current mode's sites.
    const fn stack_budget(&self) -> usize {
        match self.mode {
            InlineMode::HotLeaves => Self::HOT_LEAF_STACK_BUDGET,
            InlineMode::Normal
            | InlineMode::TinyLeaves
            | InlineMode::ConstantLeaves
            | InlineMode::SingleUse => Self::STACK_BUDGET,
        }
    }

    /// Creates the `-O size` inliner: a module budget of zero disables all MIR
    /// inlining, which only ever grows emitted code on real contracts (both
    /// multi-use duplication and the cascades that single-call inlining sets
    /// off were measured to increase size). Lowering-time inlining is disabled
    /// independently; this zero budget also lets the MIR inliner skip analysis.
    #[must_use]
    fn for_size() -> Self {
        Self { max_module_code_size: 0, ..Self::default() }
    }

    /// Creates the gas-focused leaf inliner. Shared helpers stay limited to four instructions;
    /// a modestly larger single-use leaf disappears after function DCE and can also shed both
    /// sides of the internal-call protocol without duplicating its body.
    #[must_use]
    fn for_tiny_leaves() -> Self {
        Self {
            max_instructions: 4,
            max_single_call_sanity_instructions: 12,
            max_blocks: 1,
            max_shared_callee_blocks: 1,
            mode: InlineMode::TinyLeaves,
            ..Self::default()
        }
    }

    #[must_use]
    fn for_constant_leaves() -> Self {
        Self {
            max_instructions: 64,
            inline_single_call: false,
            mode: InlineMode::ConstantLeaves,
            ..Self::default()
        }
    }
}

/// Statistics for MIR-level inlining.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct MirInlineStats {
    /// Number of internal call sites considered.
    call_sites: usize,
    /// Number of call sites inlined.
    inlined: usize,
    /// Number of call sites skipped because the callee was not inlineable.
    skipped: usize,
    /// Callees whose only call site was inlined; nothing calls them afterwards.
    consumed: Vec<MirFunctionId>,
}

#[derive(Clone, Copy, Debug, Default)]
struct MirInlineSummary {
    instruction_count: usize,
    block_count: usize,
    return_count: usize,
    /// Number of values one return delivers; two or more stage through a frame buffer.
    return_values: usize,
    param_count: usize,
    estimated_code_size: usize,
    internal_frame_size: u64,
    has_icall: bool,
    has_phi: bool,
    phi_stack_peak: Option<usize>,
    has_external_call: bool,
    has_storage_write: bool,
    has_immutable_write: bool,
    has_log: bool,
    has_control_flow: bool,
    /// Whether the body contains a back edge; a loop cloned into a loop nests
    /// its carried words inside the caller's.
    has_loop: bool,
    has_unsupported_terminator: bool,
    has_reference_return: bool,
    /// A one-block helper that forwards an argument, slice components, or one internal call.
    /// Such wrappers are safe to inline even when the value is memory-backed.
    is_transparent_forwarder: bool,
    /// Whether a void forwarder adds arguments whose setup benefits from sharing.
    void_forwarder_adds_args: bool,
    /// A frameless initializer returning at most one word of literal bytes.
    is_small_literal_return: bool,
    is_entry_point: bool,
    is_constructor: bool,
    is_function_pointer_dispatcher: bool,
    has_function_selector: bool,
    is_pure: bool,
}

impl MirInliner {
    /// Runs the inliner over the whole module.
    fn run(&mut self, gcx: Gcx<'_>, module: &mut Module) -> MirInlineStats {
        let mut stats = MirInlineStats::default();
        self.target = Target::new(gcx);
        self.expected_executions_per_deployment = self.target.expected_executions();

        // A zero budget is an explicit off switch (used by `-O size`). Avoid
        // summarizing the module or building its call graph when no call site
        // can be accepted.
        if self.max_module_code_size == 0 {
            return stats;
        }

        let mut call_counts = self.call_counts(module);
        // Keep the initial candidate set stable as inlining removes call sites.
        let memory_wrappers = if self.memory_wrappers_only {
            module
                .functions
                .iter_enumerated()
                .filter(|(id, func)| {
                    call_counts.get(id).copied().unwrap_or(0) > 0 && is_memory_wrapper(func)
                })
                .map(|(id, func)| (id, scalar_stack_peak(func)))
                .collect::<FxHashMap<_, _>>()
        } else {
            FxHashMap::default()
        };

        if self.memory_wrappers_only && memory_wrappers.is_empty() {
            return stats;
        }

        let mut summaries = self.summarize_module(gcx, module);

        // Track the estimator for explicit hard ceilings. Gas mode leaves the
        // ceiling unlimited and decides from lifetime execution/deposit cost.
        let mut module_code_size: usize = summaries.values().map(|s| s.estimated_code_size).sum();
        if module_code_size >= self.max_module_code_size {
            return stats;
        }

        let call_graph = CallGraphInfo::new(module);
        let mut artifact_calls = (self.mode == InlineMode::TinyLeaves
            && summaries.values().any(|summary| summary.is_small_literal_return))
        .then(|| ArtifactCallCounts::new(module, &call_graph));
        let preferred_large_call_sites = self.preferred_large_call_sites(module, &summaries);

        // Specialize dispatcher calls before helper-local inlining introduces phis.
        let mut caller_ids = module.functions.indices().collect::<Vec<_>>();
        caller_ids.sort_by_key(|caller| {
            summaries.get(caller).is_some_and(|summary| {
                summary.is_function_pointer_dispatcher && summary.has_function_selector
            })
        });
        for caller_id in caller_ids {
            // Leaf bodies cannot contain an inline candidate. Keep their summary for
            // callers, but avoid rebuilding loop analysis for every inlining mode.
            if !summaries.get(&caller_id).is_some_and(|summary| summary.has_icall) {
                continue;
            }
            let mut loop_costs = block_loop_costs(module.function(caller_id));
            // Bound how much each caller may grow from inlining so a function
            // calling many internal helpers (e.g. a large verifier) cannot
            // balloon past the deployable code-size limit.
            let base_instructions =
                summaries.get(&caller_id).map(|s| s.instruction_count).unwrap_or_default();
            let mut cursor = (0, 0);
            let mut caller_liveness = None;
            while let Some(site) =
                self.find_next_call(module.function(caller_id), cursor, &loop_costs)
            {
                stats.call_sites += 1;
                cursor = (site.block.index(), site.inst_index + 1);

                let Some(summary) = summaries.get(&site.callee).copied() else {
                    stats.skipped += 1;
                    continue;
                };
                let call_count = if summary.is_small_literal_return {
                    artifact_calls
                        .as_ref()
                        .and_then(|calls| calls.counts.get(&site.callee))
                        .map(|counts| counts[0].max(counts[1]))
                } else {
                    None
                }
                .unwrap_or_else(|| call_counts.get(&site.callee).copied().unwrap_or_default());
                let grew_too_much = summaries.get(&caller_id).is_some_and(|s| {
                    s.instruction_count.saturating_sub(base_instructions)
                        > self.max_caller_inlined_instructions
                });
                let framed_constructor_call = summaries.get(&caller_id).is_some_and(|caller| {
                    caller.is_constructor && summary.internal_frame_size != 0
                });
                if module_code_size >= self.max_module_code_size
                    || grew_too_much
                    || framed_constructor_call
                    || call_graph.is_recursive(site.callee)
                    || (self.immutable_leaves_only
                        && !is_immutable_word_leaf(module.function(site.callee)))
                    || (self.memory_wrappers_only && !memory_wrappers.contains_key(&site.callee))
                    || (self.mode == InlineMode::SingleUse
                        && module.function(site.callee).attributes.no_inline)
                    || !self.is_inlineable(
                        caller_id,
                        site,
                        summary,
                        call_count,
                        preferred_large_call_sites.get(&site.callee).copied(),
                    )
                {
                    stats.skipped += 1;
                    continue;
                }

                if let Some(peak) =
                    summary.phi_stack_peak.or_else(|| memory_wrappers.get(&site.callee).copied())
                {
                    let caller = module.function(caller_id);
                    let liveness = caller_liveness.get_or_insert_with(|| Liveness::compute(caller));
                    if surviving_call_words(caller, liveness, site).saturating_add(peak)
                        > self.stack_budget()
                    {
                        stats.skipped += 1;
                        continue;
                    }
                }

                let callee = module.function(site.callee).clone();
                let old_size =
                    summaries.get(&caller_id).map(|s| s.estimated_code_size).unwrap_or_default();
                let caller = module.function_mut(caller_id);
                if inline_call(caller, site.block, site.inst_index, &callee) {
                    stats.inlined += 1;
                    if self.mode == InlineMode::SingleUse && call_count == 1 {
                        stats.consumed.push(site.callee);
                    }
                    caller_liveness = None;
                    // The clone split the call block, so the loop membership
                    // of the calls that followed it must be recomputed before
                    // they are weighed.
                    if self.mode == InlineMode::HotLeaves {
                        loop_costs = block_loop_costs(module.function(caller_id));
                    }
                    let new_summary = summarize_function(
                        gcx,
                        module,
                        module.function(caller_id),
                        self.peak_analysis(),
                    );
                    module_code_size = module_code_size
                        .saturating_sub(old_size)
                        .saturating_add(new_summary.estimated_code_size);
                    summaries.insert(caller_id, new_summary);
                    if self.mode == InlineMode::TinyLeaves {
                        // Remove this call site and count the forwarded calls cloned into its
                        // caller. The original callee remains until function DCE runs.
                        if let Some(count) = call_counts.get_mut(&site.callee) {
                            *count = count.saturating_sub(1);
                        }
                        for inst in callee.instructions() {
                            if let InstKind::ICall { function, .. } = callee.inst(inst).kind {
                                *call_counts.entry(function).or_default() += 1;
                            }
                        }
                        if let Some(calls) = &mut artifact_calls {
                            calls.inline(caller_id, site.callee, &callee);
                        }
                    } else {
                        call_counts = self.call_counts(module);
                    }
                    cursor = (site.block.index(), 0);
                } else {
                    stats.skipped += 1;
                }
            }
        }

        stats
    }

    fn summarize_module(
        &self,
        gcx: Gcx<'_>,
        module: &Module,
    ) -> FxHashMap<MirFunctionId, MirInlineSummary> {
        module
            .functions
            .iter_enumerated()
            .map(|(id, func)| (id, summarize_function(gcx, module, func, self.peak_analysis())))
            .collect()
    }

    /// The stack-peak estimate the current mode needs from callee summaries.
    fn peak_analysis(&self) -> PeakAnalysis {
        match self.mode {
            InlineMode::SingleUse => PeakAnalysis::Phis,
            InlineMode::HotLeaves => PeakAnalysis::Scalars,
            InlineMode::Normal | InlineMode::TinyLeaves | InlineMode::ConstantLeaves => {
                PeakAnalysis::None
            }
        }
    }

    fn call_counts(&self, module: &Module) -> FxHashMap<MirFunctionId, usize> {
        let mut counts = FxHashMap::default();
        for func in module.functions.iter() {
            for inst_id in func.instructions() {
                if let InstKind::ICall { function, .. } = func.inst(inst_id).kind {
                    *counts.entry(function).or_default() += 1;
                }
            }
            for block in &func.blocks {
                if let Some(Terminator::TailCall { function, .. }) = &block.terminator {
                    *counts.entry(*function).or_default() += 1;
                }
            }
        }
        counts
    }

    /// Picks one call site for each large shared callee.
    ///
    /// Prefer a call nearest the end of its block, where inlining is most
    /// likely to expose a terminal path, then the smallest caller.
    fn preferred_large_call_sites(
        &self,
        module: &Module,
        summaries: &FxHashMap<MirFunctionId, MirInlineSummary>,
    ) -> FxHashMap<MirFunctionId, (MirFunctionId, InstId)> {
        let mut preferred = FxHashMap::<
            MirFunctionId,
            ((bool, usize, usize, usize, usize), (MirFunctionId, InstId)),
        >::default();
        for (caller, func) in module.functions.iter_enumerated() {
            let caller_size =
                summaries.get(&caller).map(|summary| summary.instruction_count).unwrap_or_default();
            for (block_index, block) in func.blocks.iter().enumerate() {
                for (inst_index, &inst_id) in block.instructions.iter().enumerate() {
                    let InstKind::ICall { function, ref args, .. } = func.inst(inst_id).kind else {
                        continue;
                    };
                    if !summaries.get(&function).is_some_and(|summary| {
                        summary.block_count > self.max_shared_callee_blocks
                            || (self.mode == InlineMode::ConstantLeaves
                                && summary.instruction_count > 4)
                    }) {
                        continue;
                    }
                    let instructions_after = block.instructions.len() - inst_index - 1;
                    let has_constant_argument =
                        args.iter().any(|&arg| func.value(arg).as_immediate().is_some());
                    let score = (
                        self.mode == InlineMode::ConstantLeaves && !has_constant_argument,
                        instructions_after,
                        caller_size,
                        caller.index(),
                        block_index,
                    );
                    preferred
                        .entry(function)
                        .and_modify(|current| {
                            if score < current.0 {
                                *current = (score, (caller, inst_id));
                            }
                        })
                        .or_insert((score, (caller, inst_id)));
                }
            }
        }
        preferred.into_iter().map(|(callee, (_, site))| (callee, site)).collect()
    }

    fn find_next_call(
        &self,
        func: &Function,
        start: (usize, usize),
        loop_costs: &FxHashMap<BlockId, LoopCost>,
    ) -> Option<CallSite> {
        for (block, bb) in func.blocks.iter_enumerated().skip(start.0) {
            let start_inst = if block.index() == start.0 { start.1 } else { 0 };
            for (inst_index, &inst_id) in bb.instructions.iter().enumerate().skip(start_inst) {
                if let InstKind::ICall { function, ref args, returns } = func.inst(inst_id).kind {
                    return Some(CallSite {
                        block,
                        inst_index,
                        inst: inst_id,
                        callee: function,
                        args_len: args.len(),
                        returns: returns as usize,
                        loop_depth: loop_costs.get(&block).map_or(0, |cost| cost.depth),
                        loop_executions: loop_costs.get(&block).map_or(1, |cost| cost.executions),
                        loop_counted: loop_costs.get(&block).is_none_or(|cost| cost.counted),
                        has_constant_function_selector: args
                            .first()
                            .is_some_and(|&arg| func.value(arg).as_immediate().is_some()),
                        has_constant_argument: args
                            .iter()
                            .any(|&arg| func.value(arg).as_immediate().is_some()),
                    });
                }
            }
        }
        None
    }

    fn is_inlineable(
        &self,
        caller: MirFunctionId,
        site: CallSite,
        summary: MirInlineSummary,
        call_count: usize,
        preferred_large_call_site: Option<(MirFunctionId, InstId)>,
    ) -> bool {
        let single_call = self.inline_single_call && call_count == 1;
        let bounded_phi = summary.phi_stack_peak.is_some()
            && match self.mode {
                InlineMode::SingleUse => single_call,
                InlineMode::HotLeaves => true,
                InlineMode::Normal | InlineMode::TinyLeaves | InlineMode::ConstantLeaves => false,
            };
        if self.mode == InlineMode::SingleUse
            && (!single_call
                || summary.internal_frame_size != 0
                || summary.has_reference_return
                || (summary.has_phi && !bounded_phi)
                || (!self.frame_staging_allowed && summary.return_values > 1))
        {
            return false;
        }

        // A hot leaf is a bounded scalar helper called from inside a loop. Its
        // other call sites keep the shared body; the lifetime check below
        // weighs each clone against the call protocol it removes per iteration.
        if self.mode == InlineMode::HotLeaves
            && (site.loop_depth == 0
                || summary.phi_stack_peak.is_none()
                || summary.has_loop
                || summary.has_icall
                || summary.internal_frame_size != 0
                || summary.has_reference_return
                || call_count > Self::MAX_HOT_LEAF_CALL_SITES)
        {
            return false;
        }

        // Keep shared helpers intact unless a constant function selector lets
        // later passes discard all but one dispatcher arm.
        let can_specialize_dispatcher = summary.is_function_pointer_dispatcher
            && summary.has_function_selector
            && site.has_constant_function_selector;
        if caller == site.callee
            || (summary.is_function_pointer_dispatcher
                && !single_call
                && !can_specialize_dispatcher)
            || summary.is_entry_point
            || summary.is_constructor
            || (summary.has_phi && !bounded_phi)
            || summary.has_unsupported_terminator
            || summary.return_count == 0
        {
            return false;
        }

        if self.mode == InlineMode::TinyLeaves
            && (summary.block_count != 1
                || summary.instruction_count
                    > if single_call {
                        self.max_single_call_sanity_instructions
                    } else {
                        self.max_instructions
                    }
                || summary.return_count != 1
                || (summary.has_reference_return
                    && !summary.is_transparent_forwarder
                    && !self.memory_wrappers_only
                    && !(single_call && summary.is_small_literal_return))
                || (summary.has_icall
                    && !summary.is_transparent_forwarder
                    && !self.memory_wrappers_only)
                || (!single_call && summary.void_forwarder_adds_args)
                || summary.has_control_flow)
        {
            return false;
        }

        if self.mode == InlineMode::ConstantLeaves {
            return call_count > 1
                && preferred_large_call_site == Some((caller, site.inst))
                && site.has_constant_argument
                && summary.is_pure
                && summary.block_count == 1
                && summary.instruction_count <= self.max_instructions
                && !summary.has_icall
                && !summary.has_reference_return
                && !summary.has_control_flow;
        }

        if !single_call
            && summary.block_count > self.max_shared_callee_blocks
            && preferred_large_call_site != Some((caller, site.inst))
        {
            return false;
        }

        if can_specialize_dispatcher {
            return summary.instruction_count <= self.max_single_call_sanity_instructions;
        }

        if single_call {
            if summary.instruction_count > self.max_single_call_sanity_instructions {
                return false;
            }
        } else if summary.block_count > self.max_blocks
            || summary.instruction_count > self.max_instructions
        {
            return false;
        }

        // Multi-use stateful callees are usually not worth cloning unless the
        // call is hot or the body is no larger than the internal-call protocol
        // it replaces. Single-call callees disappear from emitted runtime
        // bytecode after inlining, so they are allowed through the normal
        // code-growth check below.
        if !single_call
            && site.loop_depth == 0
            && (summary.has_storage_write
                || summary.has_immutable_write
                || summary.has_external_call
                || summary.has_log)
            && summary.estimated_code_size
                > estimated_icall_code_size(self.target, site)
                    + estimated_internal_return_code_size(self.target, summary, site)
        {
            return false;
        }

        self.inline_lifetime_cost_improves(summary, site, single_call)
    }

    /// Applies the same economic model as solc's assembly inliner: compare
    /// runtime protocol savings over the expected contract lifetime with the
    /// bytecode deposit cost of cloning the body. The body itself executes in
    /// both alternatives and therefore contributes only to deposited bytes.
    fn inline_lifetime_cost_improves(
        &self,
        summary: MirInlineSummary,
        site: CallSite,
        single_call: bool,
    ) -> bool {
        const CODE_DEPOSIT_GAS_PER_BYTE: u128 = Target::CODE_DEPOSIT_GAS_PER_BYTE as u128;

        let inlined_bytes = summary.estimated_code_size;
        let mut removed_bytes = estimated_icall_code_size(self.target, site);
        if single_call {
            removed_bytes = removed_bytes.saturating_add(
                summary.estimated_code_size
                    + estimated_internal_return_code_size(self.target, summary, site),
            );
        }
        if inlined_bytes <= removed_bytes {
            return true;
        }

        let added_deposit_cost =
            (inlined_bytes - removed_bytes) as u128 * CODE_DEPOSIT_GAS_PER_BYTE;
        let loop_executions = if !self.target.optimization().is_gas() {
            1
        } else if self.mode == InlineMode::HotLeaves && site.loop_depth > 0 && !site.loop_counted {
            Self::UNCOUNTED_LOOP_EXECUTIONS
        } else {
            site.loop_executions
        };
        let execution_savings = u128::from(estimated_icall_savings(self.target, site, summary))
            .saturating_mul(u128::from(self.expected_executions_per_deployment))
            .saturating_mul(u128::from(loop_executions));
        execution_savings > added_deposit_cost
    }
}

#[derive(Clone, Copy)]
struct CallSite {
    block: BlockId,
    inst_index: usize,
    inst: InstId,
    callee: MirFunctionId,
    args_len: usize,
    returns: usize,
    loop_depth: usize,
    loop_executions: u64,
    /// Whether every enclosing loop has a computed trip count.
    loop_counted: bool,
    has_constant_function_selector: bool,
    has_constant_argument: bool,
}

/// Counts physical call sites separately in creation and runtime code.
/// Reachability is an upper bound throughout inlining: cloning a callee into a
/// caller cannot make either artifact reach a previously unreachable function.
/// Functions retained until DCE may therefore overcount, but never undercount.
struct ArtifactCallCounts {
    creation: DenseBitSet<MirFunctionId>,
    runtime: DenseBitSet<MirFunctionId>,
    counts: FxHashMap<MirFunctionId, [usize; 2]>,
}

impl ArtifactCallCounts {
    fn new(module: &Module, graph: &CallGraphInfo) -> Self {
        let roots = |creation| {
            module.functions.iter_enumerated().filter_map(move |(id, func)| {
                let selected = if creation {
                    func.attributes.is_constructor
                } else {
                    func.selector.is_some()
                        || func.attributes.is_fallback
                        || func.attributes.is_receive
                        || module.dispatch_entry() == Some(id)
                };
                selected.then_some(id)
            })
        };
        let mut creation = graph.reachable_callees_from(roots(true));
        let mut runtime = graph.reachable_callees_from(roots(false));
        for root in roots(true) {
            creation.insert(root);
        }
        for root in roots(false) {
            runtime.insert(root);
        }
        // Unrooted ad-hoc MIR and address-exposed functions get the conservative
        // shared classification, rather than zero incoming artifact counts.
        let unknown = module
            .functions
            .indices()
            .filter(|&id| !creation.contains(id) && !runtime.contains(id))
            .collect::<Vec<_>>();
        let unknown_callees = graph.reachable_callees_from(unknown.iter().copied());
        for id in unknown.into_iter().chain(unknown_callees.iter()) {
            creation.insert(id);
            runtime.insert(id);
        }
        let mut result = Self { creation, runtime, counts: FxHashMap::default() };
        for (caller, func) in module.functions.iter_enumerated() {
            for inst in func.instructions() {
                if let InstKind::ICall { function, .. } = func.inst(inst).kind {
                    result.add(caller, function);
                }
            }
            for block in &func.blocks {
                if let Some(Terminator::TailCall { function, .. }) = block.terminator {
                    result.add(caller, function);
                }
            }
        }
        result
    }

    fn add(&mut self, caller: MirFunctionId, callee: MirFunctionId) {
        let counts = self.counts.entry(callee).or_default();
        counts[0] += usize::from(self.creation.contains(caller));
        counts[1] += usize::from(self.runtime.contains(caller));
    }

    fn inline(&mut self, caller: MirFunctionId, callee_id: MirFunctionId, callee: &Function) {
        if let Some(counts) = self.counts.get_mut(&callee_id) {
            counts[0] -= usize::from(self.creation.contains(caller));
            counts[1] -= usize::from(self.runtime.contains(caller));
        }
        for inst in callee.instructions() {
            if let InstKind::ICall { function, .. } = callee.inst(inst).kind {
                self.add(caller, function);
            }
        }
    }
}

/// Recognizes a complete constant bytes initializer without relaxing the
/// general reference-return guard. Inlining preserves the allocation and every
/// store at the call site; separate calls still produce separate objects.
fn is_small_literal_return(func: &Function) -> bool {
    if func.attributes.no_inline
        || func.internal_frame_size != 0
        || func.blocks.len() != 1
        || func.returns.as_slice() != [MirType::MemoryObject(MemoryObjectKind::Bytes)]
    {
        return false;
    }
    let block = &func.blocks[BlockId::ENTRY];
    let [alloc, len, rest @ ..] = block.instructions.as_slice() else { return false };
    if func.inst(*alloc).metadata.preserves_fmp() {
        return false;
    }
    let InstKind::Alloc { size, semantics: AllocationSemantics::INTERNAL, .. } =
        func.inst(*alloc).kind
    else {
        return false;
    };
    let Some(object) = func.inst_result_value(*alloc) else { return false };
    let InstKind::SetMemoryObjectLen(value, length, MemoryObjectKind::Bytes) = func.inst(*len).kind
    else {
        return false;
    };
    if value != object
        || !matches!(&block.terminator, Some(Terminator::Return { values }) if values.as_slice() == [object])
    {
        return false;
    }
    let Some(length) = func.value_u64(length) else { return false };
    if length > 32 || func.value_u64(size) != Some(32 + length.next_multiple_of(32)) {
        return false;
    }
    match rest {
        [] => length == 0,
        [store] => matches!(func.inst(*store).kind,
            InstKind::MemoryObjectStoreWord { object: value, offset, value: word }
            if length != 0 && value == object && func.value_u64(offset) == Some(0)
                && func.value_u256(word).is_some()),
        [data, store] => matches!((&func.inst(*data).kind, &func.inst(*store).kind),
            (InstKind::MemoryObjectData(value, MemoryObjectKind::Bytes), InstKind::MStore(ptr, word))
            if length != 0 && *value == object && func.inst_result_value(*data) == Some(*ptr)
                && func.value_u256(*word).is_some()),
        _ => false,
    }
}

fn summarize_function(
    gcx: Gcx<'_>,
    module: &Module,
    func: &Function,
    peak: PeakAnalysis,
) -> MirInlineSummary {
    let target = Target::new(gcx);
    let mut summary = MirInlineSummary {
        block_count: func.blocks.len(),
        return_values: func.returns.len(),
        param_count: func.params.len(),
        internal_frame_size: func.internal_frame_size,
        is_entry_point: func.attributes.is_fallback
            || func.attributes.is_receive
            || func.selector.is_some(),
        is_constructor: func.attributes.is_constructor,
        has_reference_return: func.returns.iter().any(|ty| {
            matches!(
                ty,
                MirType::MemPtr
                    | MirType::MemoryObject(_)
                    | MirType::StoragePtr
                    | MirType::CalldataPtr
                    | MirType::Slice(_)
            )
        }),
        is_transparent_forwarder: is_transparent_forwarder(func),
        is_small_literal_return: is_small_literal_return(func),
        is_function_pointer_dispatcher: func.attributes.is_function_pointer_dispatcher,
        has_function_selector: func.params.first() == Some(&MirType::Function),
        is_pure: func.attributes.state_mutability == StateMutability::Pure,
        has_loop: has_back_edge(func),
        ..MirInlineSummary::default()
    };

    for block in func.blocks.iter() {
        for &inst_id in &block.instructions {
            let kind = &func.inst(inst_id).kind;
            let (inst_cost, instructions) = estimate_inst_cost(gcx, module, kind);
            summary.instruction_count += instructions;
            summary.estimated_code_size += inst_cost.bytes as usize;
            match kind {
                InstKind::ICall { args, .. } => {
                    summary.has_icall = true;
                    if summary.is_transparent_forwarder && func.returns.is_empty() {
                        summary.void_forwarder_adds_args = args.len() > func.params.len();
                    }
                }
                InstKind::Phi(_) => summary.has_phi = true,
                // ABI decoding validates its input through branches, and dynamic encoding
                // emits copy loops and padding branches, so neither operation is a tiny leaf.
                InstKind::AbiDecode { .. } => summary.has_control_flow = true,
                InstKind::AbiEncode { layout, .. } if abi_layout_has_loops(layout) => {
                    summary.has_control_flow = true;
                }
                InstKind::Call { .. }
                | InstKind::CallCode { .. }
                | InstKind::StaticCall { .. }
                | InstKind::DelegateCall { .. }
                | InstKind::ExtCall { .. }
                | InstKind::ExtDelegateCall { .. }
                | InstKind::ExtStaticCall { .. }
                | InstKind::Create(..)
                | InstKind::Create2(..) => {
                    summary.has_external_call = true;
                }
                InstKind::SStore(..) | InstKind::TStore(..) => summary.has_storage_write = true,
                InstKind::StoreImmutable(..) => summary.has_immutable_write = true,
                InstKind::Log0(..)
                | InstKind::Log1(..)
                | InstKind::Log2(..)
                | InstKind::Log3(..)
                | InstKind::Log4(..) => summary.has_log = true,
                _ => {}
            }
        }
        match block.terminator.as_ref() {
            Some(term @ Terminator::Return { .. }) => {
                summary.return_count += 1;
                summary.estimated_code_size +=
                    estimate_terminator_cost(target, term).bytes as usize;
            }
            Some(term @ Terminator::Revert { .. }) => {
                summary.estimated_code_size +=
                    estimate_terminator_cost(target, term).bytes as usize;
            }
            Some(term @ Terminator::RevertReturndata) => {
                summary.estimated_code_size +=
                    estimate_terminator_cost(target, term).bytes as usize;
            }
            // A void internal function returns via `Stop` (the backend lowers it
            // to an internal return). Treat it as a return point so void callees
            // can be inlined.
            Some(Terminator::Stop) if func.returns.is_empty() => {
                summary.return_count += 1;
            }
            Some(Terminator::Jump(_))
            | Some(Terminator::Branch { .. })
            | Some(Terminator::Switch { .. }) => {
                summary.has_control_flow = true;
                summary.estimated_code_size +=
                    estimate_terminator_cost(target, block.terminator.as_ref().unwrap()).bytes
                        as usize;
            }
            Some(Terminator::ReturnData { .. })
            | Some(Terminator::Stop)
            | Some(Terminator::SelfDestruct { .. })
            | Some(Terminator::TailCall { .. })
            | None => summary.has_unsupported_terminator = true,
            Some(Terminator::Invalid) => {}
        }
    }

    if (peak == PeakAnalysis::Scalars || (peak == PeakAnalysis::Phis && summary.has_phi))
        && summary.block_count <= 16
        && summary.instruction_count <= 112
        && summary.param_count <= 4
        && !func.params.iter().any(|ty| matches!(ty, MirType::Slice(_)))
        && summary.return_count <= 3
        && summary.return_count != 0
        && summary.internal_frame_size == 0
        && !summary.has_reference_return
        && !summary.has_icall
        && func.instructions().all(|inst| {
            matches!(
                func.inst(inst).kind.effect_kind(),
                EffectKind::Pure | EffectKind::MemoryRead | EffectKind::EnvironmentRead
            )
        })
    {
        summary.phi_stack_peak = Some(scalar_stack_peak(func));
    }
    summary
}

/// Whether the control-flow graph has a back edge, found by a depth-first walk
/// from the entry block that tracks the blocks on the current path.
fn has_back_edge(func: &Function) -> bool {
    let mut on_path = DenseBitSet::new_empty(func.blocks.len());
    let mut finished = DenseBitSet::new_empty(func.blocks.len());
    let mut stack = vec![(BlockId::ENTRY, 0)];
    on_path.insert(BlockId::ENTRY);
    while let Some(top) = stack.last_mut() {
        let (block, next) = *top;
        let successors = func.blocks[block]
            .terminator
            .as_ref()
            .map(|term| term.successors())
            .unwrap_or_default();
        let Some(&successor) = successors.get(next) else {
            on_path.remove(block);
            stack.pop();
            continue;
        };
        top.1 += 1;
        if on_path.contains(successor) {
            return true;
        }
        if finished.insert(successor) {
            on_path.insert(successor);
            stack.push((successor, 0));
        }
    }
    false
}

/// Peak SSA live words in a small scalar helper; immediates are rematerialized.
fn scalar_stack_peak(func: &Function) -> usize {
    let liveness = Liveness::compute(func);
    let mut peak = 0;
    for (block, body) in func.blocks.iter_enumerated() {
        let mut live = liveness.live_out(block).clone();
        if let Some(term) = &body.terminator {
            term.for_each_operand(|value| {
                live.insert(value);
            });
        }
        peak = peak.max(live_word_count(func, &live));
        for &inst in body.instructions.iter().rev() {
            if let Some(result) = func.inst_result_value(inst) {
                live.remove(result);
            }
            if !matches!(func.inst(inst).kind, InstKind::Phi(_)) {
                for value in func.inst(inst).operands() {
                    live.insert(value);
                }
            }
            peak = peak.max(live_word_count(func, &live));
        }
    }
    peak
}

/// Caller words that survive the internal call and overlap an inline expansion.
fn surviving_call_words(func: &Function, liveness: &Liveness, site: CallSite) -> usize {
    let body = &func.blocks[site.block];
    let mut live = liveness.live_out(site.block).clone();
    if let Some(term) = &body.terminator {
        term.for_each_operand(|value| {
            live.insert(value);
        });
    }
    for &inst in body.instructions[site.inst_index + 1..].iter().rev() {
        if let Some(result) = func.inst_result_value(inst) {
            live.remove(result);
        }
        for value in func.inst(inst).operands() {
            live.insert(value);
        }
    }
    if let Some(result) = func.inst_result_value(site.inst) {
        live.remove(result);
    }
    live_word_count(func, &live)
}

fn live_word_count(func: &Function, live: &GrowableBitSet<ValueId>) -> usize {
    live.iter().filter(|&value| matches!(func.value(value), Value::Arg(_) | Value::Inst(_))).count()
}

/// A call followed by bounded physical word operations, with no allocation or frame locals.
fn is_memory_wrapper(func: &Function) -> bool {
    if func.attributes.no_inline
        || func.internal_frame_size != 0
        || func.blocks.len() != 1
        || func.params.len() > 2
        || func.returns.as_slice() != [MirType::MemPtr]
    {
        return false;
    }
    let block = &func.blocks[BlockId::ENTRY];
    let [call, rest @ ..] = block.instructions.as_slice() else { return false };
    rest.len() <= 5
        && matches!(func.inst(*call).kind, InstKind::ICall { returns: 1, .. })
        && rest.iter().any(|&inst| {
            matches!(func.inst(inst).kind, InstKind::MStore(..) | InstKind::MStore8(..))
        })
        && rest.iter().all(|&inst| {
            matches!(
                func.inst(inst).kind,
                InstKind::Add(..)
                    | InstKind::Sub(..)
                    | InstKind::MLoad(..)
                    | InstKind::MStore(..)
                    | InstKind::MStore8(..)
            )
        })
        && matches!(&block.terminator, Some(Terminator::Return { values }) if values.len() == 1)
}

/// Recognizes immutable loads combined without calls, memory access, or control flow.
fn is_immutable_word_leaf(func: &Function) -> bool {
    if func.attributes.no_inline
        || !func.params.is_empty()
        || func.internal_frame_size != 0
        || func.blocks.len() != 1
        || func.returns.len() != 1
        || func.blocks[BlockId::ENTRY].instructions.len() > 12
        || !matches!(func.blocks[BlockId::ENTRY].terminator.as_ref(),
            Some(Terminator::Return { values }) if values.len() == 1)
    {
        return false;
    }
    let mut has_immutable = false;
    for inst in func.instructions() {
        let kind = &func.inst(inst).kind;
        if matches!(kind, InstKind::LoadImmutable(_)) {
            has_immutable = true;
        } else if kind.effect_kind() != EffectKind::Pure || kind.evm_opcode().is_none() {
            return false;
        }
    }
    has_immutable
}

fn is_transparent_forwarder(func: &Function) -> bool {
    if func.attributes.no_inline
        || func.selector.is_some()
        || func.attributes.is_constructor
        || func.attributes.is_fallback
        || func.attributes.is_receive
        || func.blocks.len() != 1
        || func.internal_frame_size != 0
        || func.returns.len() > 1
    {
        return false;
    }

    if is_identity_function(func) {
        return true;
    }

    if matches!(func.returns.as_slice(), [MirType::Slice(_)])
        && matches!(func.blocks[BlockId::ENTRY].terminator.as_ref(),
            Some(Terminator::Return { values }) if values.len() == 1)
        && func.instructions().all(|inst| {
            matches!(
                func.inst(inst).kind,
                InstKind::MakeSlice { .. } | InstKind::SlicePtr(_) | InstKind::SliceLen(_)
            )
        })
    {
        return true;
    }

    let [call] = func.blocks[BlockId::ENTRY].instructions.as_slice() else { return false };
    let InstKind::ICall { returns, .. } = func.inst(*call).kind else { return false };
    match (returns, func.blocks[BlockId::ENTRY].terminator.as_ref()) {
        (0, Some(Terminator::Stop)) => func.returns.is_empty(),
        (0, Some(Terminator::Return { values })) => func.returns.is_empty() && values.is_empty(),
        (1, Some(Terminator::Return { values })) => {
            func.returns.len() == 1
                && func.inst_result_value(*call).is_some_and(|result| values.as_slice() == [result])
        }
        _ => false,
    }
}

fn is_identity_function(func: &Function) -> bool {
    let [param] = func.params.raw.as_slice() else { return false };
    let [return_ty] = func.returns.as_slice() else { return false };
    if param != return_ty || func.blocks.len() != 1 {
        return false;
    }

    let block = &func.blocks[BlockId::ENTRY];
    let Some(Terminator::Return { values }) = &block.terminator else { return false };
    let [value] = values.as_slice() else { return false };
    block.instructions.is_empty()
        && matches!(func.value(*value), Value::Arg(index) if index.index() == 0)
        && func.value_ty(*value) == Some(*param)
}

fn is_transparent_function_pointer_cast(func: &Function) -> bool {
    func.params == [MirType::Function]
        && func.returns == [MirType::Function]
        && is_identity_function(func)
}

/// Estimates the instructions an `abi_encode` of `ty` expands into after `lower-abi-encode`.
///
/// The placeholder is one MIR instruction, but words are loaded, cleaned, and stored one by
/// one, dynamic values become copy loops, and aggregates encode field by field, so inlining
/// decisions must see the expanded shape rather than the placeholder.
fn abi_type_expansion(ty: &AbiType) -> usize {
    match ty {
        AbiType::Word(None) => 2,
        AbiType::Word(Some(_)) | AbiType::Function => 3,
        AbiType::Bytes(_) => 14,
        AbiType::DynamicArray { element, .. } => 12 + 2 * abi_type_expansion(element),
        AbiType::FixedArray { element, len } => {
            1 + abi_type_expansion(element) * usize::try_from(*len).unwrap_or(usize::MAX).min(8)
        }
        AbiType::Tuple(fields) => 1 + fields.iter().map(abi_type_expansion).sum::<usize>(),
    }
}

fn abi_layout_expansion(layout: &AbiLayout) -> usize {
    layout.types.iter().map(abi_type_expansion).sum()
}

/// Whether encoding the layout emits loops or branches: every dynamic value does.
fn abi_layout_has_loops(layout: &AbiLayout) -> bool {
    layout.types.iter().any(AbiType::is_dynamic)
}

/// Estimated code of an operation the later lowerings expand, as the opcode sequence they
/// emit priced by the target, with the number of MIR instructions the expansion produces.
/// An operation that lowers to one opcode is priced by the target directly; immediates are
/// left out throughout, as they are for those.
fn estimate_inst_cost(gcx: Gcx<'_>, module: &Module, kind: &InstKind) -> (Cost, usize) {
    let target = Target::new(gcx);
    let seq =
        |codes: &[u8]| codes.iter().map(|&code| target.opcode(code)).fold(Cost::ZERO, Cost::plus);
    if select::opcode_lowering(&kind.op()).is_some()
        && !matches!(kind, InstKind::ICall { .. } | InstKind::LoadImmutable(_))
    {
        return (target.op(&kind.op(), |_| None), 1);
    }
    let code = match kind {
        InstKind::MakeSlice { .. } | InstKind::SlicePtr(_) | InstKind::SliceLen(_) => Cost::ZERO,
        InstKind::MemoryObjectData(_, kind) => {
            if EvmMemoryLayout::object_data_offset(*kind) == 0 {
                Cost::ZERO
            } else {
                seq(&[op::ADD])
            }
        }
        InstKind::MemoryObjectFieldAddr { layout, field, .. } => {
            if EvmMemoryLayout::field_offset(*layout, *field) == Some(0) {
                Cost::ZERO
            } else {
                seq(&[op::ADD])
            }
        }
        InstKind::MemoryObjectElementAddr { layout, .. } => {
            let base = EvmMemoryLayout::object_data_offset(layout.kind()) != 0;
            seq(&[op::MUL, op::ADD]).plus(if base { seq(&[op::ADD]) } else { Cost::ZERO })
        }
        InstKind::MemoryObjectLoadField { layout, field, .. } => {
            if EvmMemoryLayout::field_offset(*layout, *field) == Some(0) {
                seq(&[op::MLOAD])
            } else {
                seq(&[op::ADD, op::MLOAD])
            }
        }
        InstKind::MemoryObjectStoreField { layout, field, .. } => {
            if EvmMemoryLayout::field_offset(*layout, *field) == Some(0) {
                seq(&[op::MSTORE])
            } else {
                seq(&[op::ADD, op::MSTORE])
            }
        }
        InstKind::MemoryObjectLoadElement { layout, .. } => {
            let base = EvmMemoryLayout::object_data_offset(layout.kind()) != 0;
            seq(&[op::MUL, op::ADD, op::MLOAD]).plus(if base {
                seq(&[op::ADD])
            } else {
                Cost::ZERO
            })
        }
        InstKind::MemoryObjectStoreElement { layout, .. } => {
            let base = EvmMemoryLayout::object_data_offset(layout.kind()) != 0;
            seq(&[op::MUL, op::ADD, op::MSTORE]).plus(if base {
                seq(&[op::ADD])
            } else {
                Cost::ZERO
            })
        }
        InstKind::MemoryObjectLoadByte { .. } => seq(&[op::MLOAD, op::BYTE]),
        InstKind::MemoryObjectStoreByte { .. } => seq(&[op::ADD, op::MSTORE8]),
        InstKind::MemoryObjectStoreWord { .. } => seq(&[op::ADD, op::MSTORE]),
        InstKind::MemorySliceLoadWord { .. } => seq(&[op::MLOAD]),
        InstKind::CalldataSliceLoadWord { .. } => seq(&[op::CALLDATALOAD]),
        InstKind::MemoryObjectCopyFromSlice { .. }
        | InstKind::MemoryObjectCopyFromSliceAt { .. }
        | InstKind::MemoryObjectCopy { .. } => seq(&[op::MCOPY]),
        InstKind::MemoryObjectLen(_, _) | InstKind::Fmp | InstKind::FrameLoad { .. } => {
            seq(&[op::MLOAD])
        }
        InstKind::SetMemoryObjectLen(_, _, _)
        | InstKind::SetFmp(_)
        | InstKind::FrameStore { .. } => seq(&[op::MSTORE]),
        // Bump the free-memory pointer past the allocation.
        InstKind::Alloc { .. } => seq(&[op::MLOAD, op::ADD, op::MSTORE]),
        // Reserve the buffer, store the selector, produce the slice, store every argument's
        // head, and expand every dynamic value.
        InstKind::AbiEncode { args, layout, .. } => seq(&[
            op::MLOAD,
            op::ADD,
            op::MSTORE,
            op::MSTORE,
            op::DUP1,
            op::DUP2,
            op::SWAP1,
            op::SWAP2,
        ])
        .plus(seq(&[op::DUP1, op::ADD, op::MSTORE]).times(args.len() as u32))
        .plus(seq(&[op::DUP1]).times(abi_layout_expansion(layout) as u32)),
        // Check the calldata bound, then load and validate every head word.
        InstKind::AbiDecode { layout, .. } => seq(&[
            op::CALLDATASIZE,
            op::SUB,
            op::LT,
            op::JUMPI,
            op::CALLDATALOAD,
            op::ADD,
            op::MSTORE,
            op::SWAP1,
        ])
        .plus(seq(&[op::CALLDATALOAD, op::DUP1, op::MSTORE]).times(layout.types.len() as u32)),
        InstKind::StorageToMemory { layout, .. } => seq(&[op::SLOAD, op::MSTORE])
            .times(u32::try_from(layout.storage_slots()).unwrap_or(u32::MAX)),
        InstKind::MemoryToStorage { layout, .. } | InstKind::ClearStorage { layout, .. } => {
            seq(&[op::MLOAD, op::SSTORE])
                .times(u32::try_from(layout.storage_slots()).unwrap_or(u32::MAX))
        }
        // A pushed offset into the immutables area and the store.
        InstKind::StoreImmutable(..) => seq(&[op::PUSH2, op::MSTORE]),
        InstKind::DataCopy(..) => seq(&[op::CODECOPY]),
        // Zero by copying from beyond the end of calldata.
        InstKind::MemoryZero(..) => seq(&[op::CALLDATASIZE, op::CALLDATACOPY]),
        InstKind::ConstructorArgsBase => seq(&[op::PUSH2]),
        InstKind::ConstructorArgsEnd => seq(&[op::PUSH2, op::PUSH2, op::SUB, op::CODESIZE]),
        InstKind::InternalFrameAddr(_) => seq(&[op::PUSH1, op::ADD]),
        // Typed PUSH<N> placeholder patched at deploy time, cleaned for the narrower types.
        InstKind::LoadImmutable(id) => {
            let ty = module.immutable_type(*id);
            let encoding = ty.immutable_encoding().expect("validated immutable declaration");
            let type_size = immutable_push_type_size(
                encoding,
                gcx.sess.opts.optimization,
                gcx.sess.opts.evm_version.has_bitwise_shifting(),
            );
            let push = op::PUSH1 + (type_size.bytes() - 1);
            if type_size.bytes() == 32 {
                seq(&[push])
            } else {
                match encoding {
                    ImmutableEncoding::Unsigned(_) => seq(&[push]),
                    ImmutableEncoding::Signed(_) => seq(&[push, op::PUSH1, op::SIGNEXTEND]),
                    ImmutableEncoding::LeftAligned(_) => seq(&[push, op::PUSH1, op::SHL]),
                }
            }
        }
        // Expands to length load + data pointer + physical keccak.
        InstKind::Keccak256Bytes(_) => {
            seq(&[op::DUP1, op::MLOAD, op::SWAP1, op::ADD, op::KECCAK256])
        }
        InstKind::MappingSlot(..) => seq(&[op::MSTORE, op::MSTORE, op::KECCAK256]),
        InstKind::MappingSlotMemory(..) => seq(&[
            op::MLOAD,
            op::ADD,
            op::MSTORE,
            op::MLOAD,
            op::ADD,
            op::MSTORE,
            op::ADD,
            op::KECCAK256,
        ]),
        InstKind::MappingSlotCalldata(..) => seq(&[
            op::CALLDATALOAD,
            op::ADD,
            op::MSTORE,
            op::CALLDATALOAD,
            op::ADD,
            op::MSTORE,
            op::ADD,
            op::ADD,
            op::KECCAK256,
        ]),
        InstKind::StorageArrayDataSlot(..) => seq(&[op::MSTORE, op::MSTORE, op::KECCAK256]),
        InstKind::StorageArrayElementSlot { element_slots, .. } => {
            seq(&[op::MSTORE, op::MSTORE, op::KECCAK256, op::ADD]).plus(if *element_slots > 1 {
                seq(&[op::MUL])
            } else {
                Cost::ZERO
            })
        }
        // External calls lower to a sequence around the call opcode; the call itself
        // dominates, at the schedule's cold price.
        InstKind::Call { .. }
        | InstKind::CallCode { .. }
        | InstKind::StaticCall { .. }
        | InstKind::DelegateCall { .. }
        | InstKind::ExtCall { .. }
        | InstKind::ExtDelegateCall { .. }
        | InstKind::ExtStaticCall { .. } => seq(&[op::CALL]),
        InstKind::ICall { args, returns, .. } => target.icall(args.len(), *returns as usize, 0),
        // A phi or a select is a stack move at the join.
        InstKind::Phi(_) | InstKind::Select(..) => seq(&[op::DUP1]),
        // Every other operation lowers to one opcode and was priced above.
        _ => {
            debug_assert!(false, "operation without a single opcode is not priced: {kind}");
            seq(&[op::ADD])
        }
    };
    let instructions = match kind {
        InstKind::MappingSlot(..) | InstKind::StorageArrayDataSlot(..) => 3,
        InstKind::MappingSlotMemory(..) => 8,
        InstKind::MappingSlotCalldata(..) => 9,
        InstKind::StorageArrayElementSlot { .. } => 4,
        InstKind::AbiEncode { layout, .. } => abi_layout_expansion(layout),
        _ => 1,
    };
    (code, instructions)
}

/// Estimated code of a terminator, as the opcode sequence it emits priced by the target.
fn estimate_terminator_cost(target: Target, term: &Terminator) -> Cost {
    let seq =
        |codes: &[u8]| codes.iter().map(|&code| target.opcode(code)).fold(Cost::ZERO, Cost::plus);
    match term {
        Terminator::Jump(_) => seq(&[op::PUSH2, op::JUMP]),
        Terminator::Branch { .. } => seq(&[op::PUSH2, op::JUMPI]),
        // The dispatch on the value, then a compare and a jump per case.
        Terminator::Switch { cases, .. } => seq(&[op::PUSH2, op::JUMPI])
            .plus(seq(&[op::DUP1, op::PUSH1, op::EQ]).times(cases.len() as u32)),
        // The values are staged by the instructions above; the return is the protocol's jump.
        Terminator::Return { .. } => target.internal_return(0, 0),
        Terminator::Revert { .. } | Terminator::RevertReturndata => {
            seq(&[op::PUSH1, op::DUP1, op::REVERT])
        }
        Terminator::ReturnData { .. } => seq(&[op::PUSH1, op::DUP1, op::RETURN]),
        Terminator::Stop => seq(&[op::STOP]),
        Terminator::SelfDestruct { .. } => seq(&[op::SELFDESTRUCT]),
        // A jump with every argument staged.
        Terminator::TailCall { args, .. } => {
            seq(&[op::PUSH2, op::JUMP]).plus(seq(&[op::DUP1]).times(args.len() as u32))
        }
        Terminator::Invalid => seq(&[op::INVALID]),
    }
}

fn estimated_icall_savings(target: Target, site: CallSite, summary: MirInlineSummary) -> u64 {
    let frame_words = (summary.internal_frame_size / EvmMemoryLayout::WORD_SIZE)
        + (EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE / EvmMemoryLayout::WORD_SIZE)
        + (site.args_len + site.returns) as u64;
    let protocol = target.icall(site.args_len, site.returns, frame_words).gas;
    let return_protocol = target.internal_return(summary.param_count, site.returns).gas;
    let loop_multiplier = (site.loop_depth as u64).saturating_add(1);
    u64::from(protocol + return_protocol) * loop_multiplier
}

fn estimated_icall_code_size(target: Target, site: CallSite) -> usize {
    target.icall(site.args_len, site.returns, 0).bytes as usize
}

fn estimated_internal_return_code_size(
    target: Target,
    summary: MirInlineSummary,
    site: CallSite,
) -> usize {
    target.internal_return(summary.param_count, site.returns).bytes as usize
}

struct LoopCost {
    depth: usize,
    executions: u64,
    /// Whether every enclosing loop contributed a computed trip count.
    counted: bool,
}

fn block_loop_costs(func: &Function) -> FxHashMap<BlockId, LoopCost> {
    let mut analyzer = LoopAnalyzer::new();
    let loop_info = analyzer.analyze(func);
    let mut costs = FxHashMap::default();
    for loop_data in loop_info.all_loops() {
        let counted = loop_data.trip_count.filter(|_| {
            loop_data.trip_guard_is_header
                && loop_data.blocks.iter().all(|block| {
                    block == loop_data.header
                        || func.blocks[block].terminator.as_ref().is_some_and(|term| {
                            let successors = term.successors();
                            !successors.is_empty()
                                && successors.iter().all(|&next| loop_data.blocks.contains(next))
                        })
                })
        });
        for block in &loop_data.blocks {
            let cost =
                costs.entry(block).or_insert(LoopCost { depth: 0, executions: 1, counted: true });
            cost.depth += 1;
            if let Some(count) = counted
                && loop_data.back_edges.iter().all(|&latch| analyzer.dominates(block, latch))
            {
                let count = count.saturating_add(u64::from(block == loop_data.header));
                cost.executions = cost.executions.saturating_mul(count);
            } else {
                cost.counted = false;
            }
        }
    }
    costs
}

fn specialize_function_pointers(module: &mut Module) -> usize {
    let mut casts = DenseBitSet::new_empty(module.functions.len());
    let mut dispatchers = DenseBitSet::new_empty(module.functions.len());
    for (function, func) in module.functions.iter_enumerated() {
        if is_transparent_function_pointer_cast(func) {
            casts.insert(function);
        } else if func.attributes.is_function_pointer_dispatcher {
            dispatchers.insert(function);
        }
    }
    if casts.is_empty() && dispatchers.is_empty() {
        return 0;
    }

    let mut specialized = 0;
    for index in 0..module.functions.len() {
        let caller = MirFunctionId::from_usize(index);
        let mut cursor = (0, 0);
        while let Some((block, inst_index, callee, selector)) =
            find_next_constant_function_call(module.function(caller), cursor)
        {
            cursor = (block.index(), inst_index + 1);
            if caller == callee {
                continue;
            }

            if casts.contains(callee) {
                if propagate_function_pointer_cast(module.function_mut(caller), block, inst_index) {
                    specialized += 1;
                    cursor = (block.index(), 0);
                }
                continue;
            }
            if !dispatchers.contains(callee) {
                continue;
            }

            let dispatcher = module.function(callee).clone();
            if let Some(target) = direct_dispatch_target(&dispatcher, &selector) {
                if rewrite_dispatch_call(module.function_mut(caller), block, inst_index, target) {
                    specialized += 1;
                }
            } else if dispatcher.instructions().take(4097).count() <= 4096
                // Constructors resolve cloned `InternalFrameAddr` offsets through the
                // uninitialized internal-frame pointer, so a framed dispatcher must never
                // be inlined into one. Dispatchers are frameless by construction today;
                // this mirrors `framed_constructor_call` in case that ever changes.
                && !(dispatcher.internal_frame_size != 0
                    && module.function(caller).attributes.is_constructor)
                && inline_call(module.function_mut(caller), block, inst_index, &dispatcher)
            {
                specialized += 1;
                cursor = (block.index(), 0);
            }
        }
    }
    specialized
}

fn find_next_constant_function_call(
    func: &Function,
    start: (usize, usize),
) -> Option<(BlockId, usize, MirFunctionId, Immediate)> {
    for (block, bb) in func.blocks.iter_enumerated().skip(start.0) {
        let start_inst = if block.index() == start.0 { start.1 } else { 0 };
        for (inst_index, &inst_id) in bb.instructions.iter().enumerate().skip(start_inst) {
            if let InstKind::ICall { function, ref args, .. } = func.inst(inst_id).kind
                && let Some(selector) =
                    args.first().and_then(|&arg| func.value(arg).as_immediate()).cloned()
            {
                return Some((block, inst_index, function, selector));
            }
        }
    }
    None
}

fn direct_dispatch_target(dispatcher: &Function, selector: &Immediate) -> Option<MirFunctionId> {
    let selector = selector.as_u256()?;
    for block in dispatcher.blocks.iter() {
        let Terminator::Branch { condition, then_block, .. } = block.terminator.as_ref()? else {
            continue;
        };
        let Value::Inst(condition) = dispatcher.value(*condition) else {
            continue;
        };
        let InstKind::Eq(lhs, rhs) = dispatcher.inst(*condition).kind else {
            continue;
        };
        let matches_selector = [(lhs, rhs), (rhs, lhs)].into_iter().any(|(arg, value)| {
            matches!(dispatcher.value(arg), Value::Arg(index) if index.index() == 0)
                && dispatcher.value_ty(arg) == Some(MirType::Function)
                && dispatcher.value(value).as_immediate().and_then(Immediate::as_u256)
                    == Some(selector)
        });
        if matches_selector {
            return direct_dispatch_case_target(dispatcher, *then_block);
        }
    }
    None
}

fn direct_dispatch_case_target(dispatcher: &Function, block: BlockId) -> Option<MirFunctionId> {
    let block = &dispatcher.blocks[block];
    let [call] = block.instructions.as_slice() else {
        return None;
    };
    let InstKind::ICall { function, args, returns } = &dispatcher.inst(*call).kind else {
        return None;
    };
    if !args.iter().enumerate().all(|(index, &arg)| {
        matches!(
            dispatcher.value(arg),
            Value::Arg(arg_index) if arg_index.index() == index + 1
        )
    }) {
        return None;
    }

    let Some(Terminator::Return { values }) = &block.terminator else {
        return None;
    };
    match *returns {
        0 if values.is_empty() => Some(*function),
        1 if values.as_slice() == [dispatcher.inst_result_value(*call)?] => Some(*function),
        _ => None,
    }
}

fn rewrite_dispatch_call(
    caller: &mut Function,
    call_block: BlockId,
    call_inst_index: usize,
    target: MirFunctionId,
) -> bool {
    let Some(&call) = caller.blocks[call_block].instructions.get(call_inst_index) else {
        return false;
    };
    let InstKind::ICall { function, args, .. } = &mut caller.inst_mut(call).kind else {
        return false;
    };
    if args.is_empty() {
        return false;
    }
    *function = target;
    *args = args[1..].into();
    true
}

fn propagate_function_pointer_cast(
    caller: &mut Function,
    call_block: BlockId,
    call_inst_index: usize,
) -> bool {
    let Some(&call_inst) = caller.blocks[call_block].instructions.get(call_inst_index) else {
        return false;
    };
    let InstKind::ICall { ref args, returns: 1, .. } = caller.inst(call_inst).kind else {
        return false;
    };
    let Some(&arg) = args.first() else {
        return false;
    };
    let Some(result) = caller.inst_result_value(call_inst) else {
        return false;
    };

    caller.blocks[call_block].instructions.remove(call_inst_index);
    caller.replace_uses(&FxHashMap::from_iter([(result, arg)]));
    true
}

fn inline_call(
    caller: &mut Function,
    call_block: BlockId,
    call_inst_index: usize,
    callee: &Function,
) -> bool {
    let snapshot = caller.clone();
    if inline_call_impl(caller, call_block, call_inst_index, callee).is_some() {
        true
    } else {
        *caller = snapshot;
        false
    }
}

fn inline_call_impl(
    caller: &mut Function,
    call_block: BlockId,
    call_inst_index: usize,
    callee: &Function,
) -> Option<()> {
    let call_inst = caller.blocks[call_block].instructions[call_inst_index];
    let InstKind::ICall { args, returns, .. } = caller.inst(call_inst).kind.clone() else {
        return None;
    };
    let returns = returns as usize;
    if returns != callee.returns.len() {
        return None;
    }

    let call_result = caller.inst_result_value(call_inst);
    if returns > 0 && call_result.is_none() {
        return None;
    }

    if is_small_literal_return(callee) {
        return inline_literal_call(
            caller,
            call_block,
            call_inst_index,
            callee,
            args,
            call_result?,
        );
    }

    let continuation = caller.alloc_block();
    let (old_terminator, metadata) = caller.blocks[call_block].take_terminator();
    let old_successors = old_terminator.as_ref().map(Terminator::successors).unwrap_or_default();
    let suffix = {
        let block = &mut caller.blocks[call_block];
        block.instructions.split_off(call_inst_index + 1)
    };
    caller.blocks[call_block].instructions.pop();
    // continuation: suffix; old_terminator !metadata(caller)
    caller.blocks[continuation].instructions = suffix;
    if let Some(terminator) = old_terminator {
        caller.blocks[continuation].set_terminator(terminator, metadata);
    }
    redirect_phi_predecessors(caller, &old_successors, call_block, continuation);

    let caller_is_external = caller.selector.is_some()
        || caller.attributes.is_constructor
        || caller.attributes.is_receive
        || caller.attributes.is_fallback;
    let caller_frame_prefix = if caller_is_external {
        0
    } else {
        let signature_slots = caller.params.len().checked_add(caller.returns.len())?;
        let signature_size =
            u64::try_from(signature_slots).ok()?.checked_mul(EvmMemoryLayout::WORD_SIZE)?;
        EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE.checked_add(signature_size)?
    };
    let frame_base = caller_frame_prefix.checked_add(caller.internal_frame_size)?;
    let callee_signature_slots = callee.params.len().checked_add(callee.returns.len())?;
    let callee_signature_size =
        u64::try_from(callee_signature_slots).ok()?.checked_mul(EvmMemoryLayout::WORD_SIZE)?;
    let callee_frame_prefix =
        EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE.checked_add(callee_signature_size)?;
    caller.internal_frame_size =
        caller.internal_frame_size.checked_add(callee.internal_frame_size)?;

    let mut cloner = InlineCloner::new(caller, callee, frame_base, callee_frame_prefix, args);
    let cloned_entry = cloner.clone_blocks(continuation)?;
    // icall @callee !metadata(call) => jump cloned_entry !metadata(call)
    cloner.caller.blocks[call_block].terminator = Some(Terminator::Jump(cloned_entry));
    cloner.caller.blocks[call_block].terminator_metadata =
        cloner.caller.inst(call_inst).metadata.debug_context();

    let mut replacements = FxHashMap::default();
    if returns > 0 {
        let return_values = build_return_values(
            cloner.caller,
            continuation,
            &callee.returns,
            &cloner.return_edges,
        )?;
        replacements.insert(call_result?, return_values[0]);
        insert_return_buffer_stores(
            cloner.caller,
            continuation,
            &return_values,
            &callee.returns,
            caller_is_external,
            caller_frame_prefix,
        )?;
    }

    cloner.caller.replace_uses(&replacements);
    recompute_cfg(cloner.caller);
    prune_phi_incoming_to_predecessors(cloner.caller);
    Some(())
}

/// Splices a literal initializer into the call block so ABI lowering can see
/// the object directly, without an intermediate single-edge phi. The checked
/// initializer shape is frameless and cannot require control-flow remapping.
fn inline_literal_call(
    caller: &mut Function,
    block: BlockId,
    index: usize,
    callee: &Function,
    args: Box<[ValueId]>,
    result: ValueId,
) -> Option<()> {
    let mut cloner = InlineCloner::new(caller, callee, 0, 0, args);
    let mut instructions = Vec::new();
    // object = icall @literal
    //   => object = alloc memorybytes, size
    //      set_memory_object_len object, length
    //      memory_object_store_word object, 0, word
    for &inst in &callee.blocks[BlockId::ENTRY].instructions {
        let source = callee.inst(inst);
        let kind = cloner.clone_inst_kind(source.kind.clone())?;
        let mut instruction = Instruction::new(kind, source.result_ty);
        instruction.metadata.copy_debug_context(&source.metadata);
        if let InstKind::MStore(ptr, value) = instruction.kind
            && let Value::Inst(data) = cloner.caller.value(ptr)
            && let InstKind::MemoryObjectData(object, MemoryObjectKind::Bytes) =
                cloner.caller.inst(*data).kind
        {
            // mstore memory_object_data(object), word
            //   => memory_object_store_word object, 0, word
            let offset =
                cloner.caller.alloc_value(Value::Immediate(Immediate::uint256(Default::default())));
            instruction.kind = InstKind::MemoryObjectStoreWord { object, offset, value };
        }
        let new_inst = if let Some(value) = callee.inst_result_value(inst) {
            let (new_inst, new_value) = cloner.caller.alloc_value_inst(instruction);
            cloner.value_map.insert(value, new_value);
            new_inst
        } else {
            cloner.caller.alloc_inst(instruction)
        };
        instructions.push(new_inst);
    }
    let Terminator::Return { values } = callee.blocks[BlockId::ENTRY].terminator.as_ref()? else {
        return None;
    };
    let replacement = cloner.clone_value(values[0])?;
    // prefix; icall @literal; suffix => prefix; initializer; suffix[object/result]
    cloner.caller.blocks[block].instructions.splice(index..=index, instructions);
    cloner.caller.replace_uses(&FxHashMap::from_iter([(result, replacement)]));
    Some(())
}

/// Splices an externally terminating wrapper into one dispatcher route.
/// The caller owns rollback if cloning fails; eligibility and frame exclusion
/// are checked by the dispatcher pass before any module mutation.
pub(super) fn inline_dispatch_route(
    caller: &mut Function,
    block: BlockId,
    callee: &Function,
    args: Box<[ValueId]>,
) -> Option<()> {
    let mut cloner = InlineCloner::new(caller, callee, 0, 0, args);
    cloner.external_exit = true;
    let entry = cloner.clone_blocks(BlockId::ENTRY)?;
    // tail_call @wrapper => jump cloned_entry
    // cloned exits retain returndata/revert/stop rather than returning to a caller
    // NOTE: The wrapper call boundary disappears; its source checkpoint cannot
    // describe this generated jump. Cloned body instructions keep their origins.
    cloner.caller.blocks[block].set_generated_terminator(Terminator::Jump(entry));
    recompute_cfg(cloner.caller);
    Some(())
}

struct InlineCloner<'a> {
    caller: &'a mut Function,
    callee: &'a Function,
    frame_base: u64,
    callee_frame_prefix: u64,
    args: Box<[ValueId]>,
    value_map: FxHashMap<ValueId, ValueId>,
    block_map: IndexVec<BlockId, BlockId>,
    return_edges: Vec<(BlockId, SmallVec<[ValueId; 2]>)>,
    external_exit: bool,
}

impl<'a> InlineCloner<'a> {
    fn new(
        caller: &'a mut Function,
        callee: &'a Function,
        frame_base: u64,
        callee_frame_prefix: u64,
        args: Box<[ValueId]>,
    ) -> Self {
        Self {
            caller,
            callee,
            frame_base,
            callee_frame_prefix,
            args,
            value_map: FxHashMap::default(),
            block_map: IndexVec::with_capacity(callee.blocks.len()),
            return_edges: Vec::new(),
            external_exit: false,
        }
    }

    fn clone_blocks(&mut self, continuation: BlockId) -> Option<BlockId> {
        for _ in self.callee.blocks.indices() {
            self.block_map.push(self.caller.alloc_block());
        }

        for (callee_block, block) in self.callee.blocks.iter_enumerated() {
            let caller_block = self.block_map[callee_block];
            let mut instructions = Vec::with_capacity(block.instructions.len());
            for &inst_id in &block.instructions {
                let inst = self.callee.inst(inst_id).clone();
                let mut instruction = Instruction::new(inst.kind.clone(), inst.result_ty);
                instruction.metadata.copy_debug_context(&inst.metadata);
                let new_inst = if let Some(callee_result) = self.callee.inst_result_value(inst_id) {
                    let (new_inst, new_result) = self.caller.alloc_value_inst(instruction);
                    self.value_map.insert(callee_result, new_result);
                    new_inst
                } else {
                    self.caller.alloc_inst(instruction)
                };
                instructions.push(new_inst);
            }
            self.caller.blocks[caller_block].instructions = instructions;
        }

        for (callee_block, block) in self.callee.blocks.iter_enumerated() {
            let caller_block = self.block_map[callee_block];
            for (index, &inst_id) in block.instructions.iter().enumerate() {
                let kind = self.clone_inst_kind(self.callee.inst(inst_id).kind.clone())?;
                let new_inst = self.caller.blocks[caller_block].instructions[index];
                self.caller.inst_mut(new_inst).kind = kind;
            }
        }

        for (callee_block, block) in self.callee.blocks.iter_enumerated() {
            let caller_block = self.block_map[callee_block];
            let term =
                self.clone_terminator(block.terminator.as_ref()?, caller_block, continuation)?;
            // cloned_block: cloned_terminator !metadata(callee block)
            self.caller.blocks[caller_block].terminator = Some(term);
            self.caller.blocks[caller_block].terminator_metadata =
                block.terminator_metadata.clone();
        }

        Some(self.block_map[BlockId::ENTRY])
    }

    fn clone_value(&mut self, value: ValueId) -> Option<ValueId> {
        if let Some(&mapped) = self.value_map.get(&value) {
            return Some(mapped);
        }

        let cloned = match self.callee.value(value).clone() {
            Value::Arg(index) => *self.args.get(index.index())?,
            Value::Immediate(imm) => self.caller.alloc_value(Value::Immediate(imm)),
            Value::Undef(ty) => self.caller.alloc_value(Value::Undef(ty)),
            Value::Error(guar) => self.caller.alloc_value(Value::Error(guar)),
            Value::Inst(_) => return None,
        };
        self.value_map.insert(value, cloned);
        Some(cloned)
    }

    fn clone_block(&self, block: BlockId) -> Option<BlockId> {
        self.block_map.get(block).copied()
    }

    fn clone_inst_kind(&mut self, mut kind: InstKind) -> Option<InstKind> {
        if let InstKind::InternalFrameAddr(offset) = &mut kind {
            let local_offset = offset.checked_sub(self.callee_frame_prefix)?;
            *offset = self.frame_base.checked_add(local_offset)?;
        }
        if let InstKind::Phi(incoming) = &mut kind {
            for (block, _) in incoming {
                *block = self.clone_block(*block)?;
            }
        }

        let mut failed = false;
        kind.visit_operands_mut(|value| {
            if failed {
                return;
            }
            if let Some(cloned) = self.clone_value(*value) {
                *value = cloned;
            } else {
                failed = true;
            }
        });
        (!failed).then_some(kind)
    }

    fn clone_terminator(
        &mut self,
        term: &Terminator,
        cloned_block: BlockId,
        continuation: BlockId,
    ) -> Option<Terminator> {
        Some(match term {
            Terminator::Jump(target) => Terminator::Jump(self.clone_block(*target)?),
            Terminator::Branch { condition, then_block, else_block } => Terminator::Branch {
                condition: self.clone_value(*condition)?,
                then_block: self.clone_block(*then_block)?,
                else_block: self.clone_block(*else_block)?,
            },
            Terminator::Switch { value, default, cases } => Terminator::Switch {
                value: self.clone_value(*value)?,
                default: self.clone_block(*default)?,
                cases: cases
                    .iter()
                    .map(|(value, block)| {
                        Some((self.clone_value(*value)?, self.clone_block(*block)?))
                    })
                    .collect::<Option<Vec<_>>>()?,
            },
            // returndata offset, size => returndata cloned(offset), cloned(size)
            Terminator::ReturnData { offset, size } if self.external_exit => {
                Terminator::ReturnData {
                    offset: self.clone_value(*offset)?,
                    size: self.clone_value(*size)?,
                }
            }
            Terminator::Stop if self.external_exit => Terminator::Stop,
            Terminator::Return { values } if self.external_exit && values.is_empty() => {
                Terminator::Stop
            }
            Terminator::Return { values } => {
                let mapped = values
                    .iter()
                    .map(|value| self.clone_value(*value))
                    .collect::<Option<SmallVec<[ValueId; 2]>>>()?;
                self.return_edges.push((cloned_block, mapped));
                Terminator::Jump(continuation)
            }
            // A void callee's `Stop` is an internal return with no values.
            Terminator::Stop if self.callee.returns.is_empty() => {
                self.return_edges.push((cloned_block, SmallVec::new()));
                Terminator::Jump(continuation)
            }
            Terminator::Revert { offset, size } => Terminator::Revert {
                offset: self.clone_value(*offset)?,
                size: self.clone_value(*size)?,
            },
            Terminator::RevertReturndata => Terminator::RevertReturndata,
            Terminator::TailCall { function, args } => Terminator::TailCall {
                function: *function,
                args: args
                    .iter()
                    .map(|arg| self.clone_value(*arg))
                    .collect::<Option<SmallVec<_>>>()?,
            },
            Terminator::ReturnData { .. } | Terminator::Stop | Terminator::SelfDestruct { .. } => {
                return None;
            }
            Terminator::Invalid => Terminator::Invalid,
        })
    }
}

fn build_return_values(
    caller: &mut Function,
    continuation: BlockId,
    return_tys: &[MirType],
    return_edges: &[(BlockId, SmallVec<[ValueId; 2]>)],
) -> Option<Vec<ValueId>> {
    let mut values = Vec::with_capacity(return_tys.len());
    for (index, &ty) in return_tys.iter().enumerate() {
        let incoming = return_edges
            .iter()
            .map(|(block, edge_values)| Some((*block, *edge_values.get(index)?)))
            .collect::<Option<Vec<_>>>()?;
        let (phi, value) = caller.alloc_value_inst(
            Instruction::new(InstKind::Phi(incoming), Some(ty)).with_debug_info_dropped(),
        );
        caller.blocks[continuation].instructions.insert(index, phi);
        values.push(value);
    }
    Some(values)
}

fn insert_return_buffer_stores(
    caller: &mut Function,
    continuation: BlockId,
    values: &[ValueId],
    return_tys: &[MirType],
    caller_is_external: bool,
    caller_frame_prefix: u64,
) -> Option<()> {
    debug_assert_eq!(values.len(), return_tys.len());
    if values.len() < 2 {
        return Some(());
    }

    let phi_count = caller.blocks[continuation]
        .instructions
        .iter()
        .take_while(|&&inst_id| matches!(caller.inst(inst_id).kind, InstKind::Phi(_)))
        .count();
    let existing_len = caller.blocks[continuation].instructions.len();
    let instructions = {
        let mut builder = FunctionBuilder::new(caller);
        builder.switch_to_block(continuation);
        let mut stored_values = Vec::with_capacity(return_tys.len());
        for (index, (&value, &ty)) in values.iter().zip(return_tys).enumerate() {
            if let MirType::Slice(_) = ty {
                if index != 0 {
                    stored_values.push(builder.slice_ptr(value));
                }
                stored_values.push(builder.slice_len(value));
            } else if index != 0 {
                stored_values.push(value);
            }
        }

        let size =
            u64::try_from(stored_values.len() + 1).ok()?.checked_mul(EvmMemoryLayout::WORD_SIZE)?;
        let local_offset = builder.func().internal_frame_size;
        builder.func_mut().internal_frame_size =
            builder.func().internal_frame_size.checked_add(size)?;
        let frame_offset = caller_frame_prefix.checked_add(local_offset)?;
        for (index, value) in stored_values.into_iter().enumerate() {
            let offset = u64::try_from(index + 1).ok()?.checked_mul(EvmMemoryLayout::WORD_SIZE)?;
            let offset = if caller_is_external {
                builder.imm(
                    EvmMemoryLayout::HEAP_START.checked_add(local_offset.checked_add(offset)?)?,
                )
            } else {
                builder.internal_frame_addr(frame_offset.checked_add(offset)?)
            };
            builder.mstore(offset, value);
        }
        let base = if caller_is_external {
            builder.imm(EvmMemoryLayout::HEAP_START.checked_add(local_offset)?)
        } else {
            builder.internal_frame_addr(frame_offset)
        };
        builder.frame_store(0, FrameMode::MultiReturn, FrameSlotKind::Word, base);
        builder.func_mut().blocks[continuation]
            .instructions
            .drain(existing_len..)
            .collect::<Vec<_>>()
    };
    let consumer_start = phi_count + instructions.len();
    caller.blocks[continuation].instructions.splice(phi_count..phi_count, instructions);
    if return_tys.iter().all(|ty| !matches!(ty, MirType::Slice(_) | MirType::MemoryObject(_))) {
        forward_inline_return_loads(caller, continuation, consumer_start, values);
    }
    Some(())
}

/// Forward scalar return-buffer loads before any instruction can overwrite its contents.
fn forward_inline_return_loads(
    func: &mut Function,
    block: BlockId,
    start: usize,
    returned: &[ValueId],
) {
    let mut offsets = FxHashMap::default();
    let mut replacements = FxHashMap::default();
    let mut removed = Vec::new();
    for &inst in &func.blocks[block].instructions[start..] {
        let instruction = func.inst(inst);
        if instruction
            .metadata
            .effect()
            .is_some_and(|effect| effect != instruction.kind.effect_kind())
        {
            break;
        }
        match instruction.kind {
            InstKind::FrameLoad {
                offset: 0,
                mode: FrameMode::MultiReturn,
                kind: FrameSlotKind::Word,
            } => {
                if let Some(value) = func.inst_result_value(inst) {
                    offsets.insert(value, 0u64);
                }
            }
            InstKind::Add(a, b) => {
                if let Some(offset) = offsets
                    .get(&a)
                    .and_then(|&offset| func.value_u64(b)?.checked_add(offset))
                    .or_else(|| {
                        offsets.get(&b).and_then(|&offset| func.value_u64(a)?.checked_add(offset))
                    })
                    && let Some(value) = func.inst_result_value(inst)
                {
                    offsets.insert(value, offset);
                }
            }
            InstKind::MLoad(address) => {
                if let Some(&offset) = offsets.get(&address)
                    && offset >= 32
                    && offset % 32 == 0
                    && let Some(&value) =
                        returned.get(usize::try_from(offset / 32).unwrap_or(usize::MAX))
                    && let Some(result) = func.inst_result_value(inst)
                    && func.value_ty(value) == instruction.result_ty
                {
                    replacements.insert(result, value);
                    removed.push(inst);
                }
            }
            _ if instruction.kind.effect_kind() == EffectKind::Pure => {}
            _ => break,
        }
    }
    if !replacements.is_empty() {
        // publish [_, result1, ...]; ptr = frame_load multi-return
        // load(ptr + 32 * n) => resultN
        func.for_each_instruction_mut(|_, instruction| {
            instruction.rewrite_operands(|value| {
                *value = resolve_replacement(*value, &replacements);
            });
        });
        for block in &mut func.blocks {
            if let Some(term) = &mut block.terminator {
                replace_terminator_uses_canonicalized(term, &replacements);
            }
        }
        func.blocks[block].instructions.retain(|inst| !removed.contains(inst));
    }
}

fn redirect_phi_predecessors(
    func: &mut Function,
    successors: &[BlockId],
    old_pred: BlockId,
    new_pred: BlockId,
) {
    if successors.is_empty() {
        return;
    }

    for &succ in successors {
        let instruction_count = func.blocks[succ].instructions.len();
        for index in 0..instruction_count {
            let inst_id = func.blocks[succ].instructions[index];
            if let InstKind::Phi(incoming) = &mut func.inst_mut(inst_id).kind {
                for (pred, _) in incoming {
                    if *pred == old_pred {
                        *pred = new_pred;
                    }
                }
            }
        }
    }
}

fn recompute_cfg(func: &mut Function) {
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
}

fn prune_phi_incoming_to_predecessors(func: &mut Function) {
    for block_id in func.blocks.indices() {
        let predecessors = func.blocks[block_id].predecessors.clone();
        let instruction_count = func.blocks[block_id].instructions.len();
        for index in 0..instruction_count {
            let inst_id = func.blocks[block_id].instructions[index];
            if let InstKind::Phi(incoming) = &mut func.inst_mut(inst_id).kind {
                incoming.retain(|(pred, _)| predecessors.contains(pred));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::FunctionBuilder;
    use solar_ast::Ident;

    #[test]
    fn call_counts_include_tail_calls() {
        let mut module = Module::new(Ident::DUMMY);
        let callee = module.add_function(Function::new(Ident::DUMMY));

        let mut ordinary = Function::new(Ident::DUMMY);
        let mut builder = FunctionBuilder::new(&mut ordinary);
        builder.icall_void(callee, Vec::new(), 0);
        builder.stop();
        module.add_function(ordinary);

        let mut tail = Function::new(Ident::DUMMY);
        FunctionBuilder::new(&mut tail).tail_call(callee, Vec::new());
        module.add_function(tail);

        assert_eq!(MirInliner::default().call_counts(&module).get(&callee), Some(&2));
    }

    #[test]
    fn frame_overflow_skips_inlining_without_mutation() {
        let callee_id = MirFunctionId::from_usize(0);
        let mut callee = Function::new(Ident::DUMMY);
        FunctionBuilder::new(&mut callee).ret(Vec::new());

        let mut caller = Function::new(Ident::DUMMY);
        caller.internal_frame_size = u64::MAX;
        let mut builder = FunctionBuilder::new(&mut caller);
        builder.icall_void(callee_id, Vec::new(), 0);
        builder.stop();
        let call = caller.blocks[BlockId::ENTRY].instructions[0];
        let call_index = caller.blocks[BlockId::ENTRY]
            .instructions
            .iter()
            .position(|&inst| inst == call)
            .unwrap();

        assert!(!inline_call(&mut caller, BlockId::ENTRY, call_index, &callee));
        assert_eq!(caller.internal_frame_size, u64::MAX);
        assert_eq!(caller.blocks.len(), 1);
        assert!(matches!(caller.inst(call).kind, InstKind::ICall { .. }));
    }
}
