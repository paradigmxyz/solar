//! Inline profitable calls and expose their bodies to caller analyses.
//!
//! One policy admits small leaves and forwarders, consumes sole-use helpers,
//! and allows bounded clones where constants, loops, or shared storage reads
//! offer further optimization. All candidates use the same legality checks and
//! lifetime cost comparison. Four bounded rounds remove consumed functions and
//! revisit their callees with updated call counts. The pipeline repeats this
//! pass after lowering and cleanup expose smaller bodies.
//!
//! Size mode admits tiny bodies whose estimated bytes do not grow, and consumes
//! sole-use callees, adding to the body the code the backend would stop sharing
//! with other functions (see `shared_runs`). It also inlines every call of an
//! arithmetic leaf or forwarder with several sites when the copies cost less than
//! the shared body and its call protocols (see `all_sites`). Gas mode also
//! considers scalar loop helpers, constant-argument wrappers, and callees sharing
//! a direct constant-slot read with the call block. The latter
//! must not write storage or observe gas; later CSE removes redundant reads.
//! Reference-returning helpers stay shared except for transparent forwarders
//! and one-word literals used once per emitted artifact. Shared void forwarders
//! that add arguments stay shared to avoid duplicating their setup.
//!
//! After memory lowering, frame allocation and multi-word return staging cannot
//! be introduced again. At this stage the pass also partially clones returning
//! guards in loops and moves eligible external wrappers into the dispatcher.
//! These use the same cost comparison, but need distinct rewrites because they
//! retain the slow-path call or preserve an external exit, respectively.
//!
//! Profitability compares the call protocol a clone removes, weighted by expected
//! executions and, for statically counted loops, by iteration count, with the
//! deposit cost of the cloned bytes. Only blocks that dominate every latch of a
//! guarded loop with no other exit receive the trip-count weight. These are
//! estimates, never legality facts.
//!
//! Callees with phis need a stack estimate: backward liveness gives the callee's
//! peak live words and the caller words surviving the call, and their sum must fit
//! twelve words, leaving headroom for operand staging. This bounds spills but does
//! not promise the scheduler emits none.
//!
//! A callee documented `@custom:solar-inline` skips every profitability test, the stack
//! estimate below included: each call where cloning is legal is cloned, while the module's
//! tagged clones fit a budget of their own over all runs. Tagged helpers that call each other
//! would otherwise multiply without limit; past the budget, tagged calls are weighed like any
//! other.
//! `@custom:solar-noinline` marks a function no-inline. The `inline-tagged` variant clones only
//! such callees, and runs first in the semantic pipeline: a helper that spells a word operation
//! as a byte loop then meets the caller's objects, so `byte-loops` can tell a fresh destination
//! from an argument, and constant arguments reach the loop bounds.
//!
//! Recursive calls, explicit no-inline functions, large helpers, and aggregate
//! allocation semantics keep the call. A tail call to a helper that never returns,
//! as `outline-reverts` leaves on failure paths, counts as a revert; any other tail
//! call keeps its callee shared. A phi-free helper with one return may call such a
//! failure helper, since caller values never survive the call; more complex helpers
//! with calls stay shared rather than duplicating their diamonds under the scalar
//! leaf estimate.
//!
//! After a multi-word return is inlined, immediate reads of the published return
//! buffer use the returned SSA words directly. Only scalar word loads at constant
//! offsets before the next memory or effect barrier qualify. The publication stores
//! stay, so later observers of the buffer keep their behavior; memory DSE decides
//! whether they can go.

use crate::{
    backend::evm::{op, select},
    mir::{
        AbiLayout, AbiType, AllocationSemantics, BlockId, Builtin, Callee, EffectKind, FrameMode,
        FrameSlotKind, Function, FunctionBuilder, FunctionId as MirFunctionId, Immediate,
        ImmutableEncoding, InstId, InstKind, Instruction, MirPhase, MirType, Module, PtrLocation,
        StorageAlias, Terminator, Value, ValueId,
        analysis::{
            AddressSpace, CallGraphInfo, Liveness, LoopAnalyzer, MemoryCallSummaries,
            direct_callees,
        },
        immutable::immutable_push_type_size,
        memory::EvmMemoryLayout,
        pass::MirPass,
        utils::{rebuild_predecessors, replace_terminator_uses_canonicalized, resolve_replacement},
    },
    target::{Cost, Target},
};
use smallvec::SmallVec;
use solar_ast::StateMutability;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
use solar_sema::Gcx;

mod all_sites;
mod dispatch;
mod guards;
mod shared_runs;

use shared_runs::SharedRuns;

/// One inlining policy, before and after physical memory lowering, and its
/// `@custom:solar-inline` subset.
pub(crate) enum Inline {
    Semantic,
    Physical,
    Tagged,
}

impl MirPass for Inline {
    fn name(&self) -> &'static str {
        match self {
            Self::Semantic | Self::Physical => "inline",
            Self::Tagged => "inline-tagged",
        }
    }

    fn is_enabled(&self, gcx: Gcx<'_>, module: &Module) -> bool {
        module.optimization(gcx) != solar_config::OptimizationMode::None
    }

    fn run_pass(
        &self,
        gcx: Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let target = module.target(gcx);
        let physical = matches!(self, Self::Physical) || module.phase >= MirPhase::Lowered;
        let tagged_only = matches!(self, Self::Tagged);
        if tagged_only && !module.functions.iter().any(|function| function.attributes.always_inline)
        {
            return false;
        }
        let mut changed = false;
        for _ in 0..4 {
            let calls = analyses.fresh_call_summaries(module);
            let straight_line = module.creation_pricing_functions().1;
            let shared_runs = (!target.optimization().is_gas()).then(|| SharedRuns::new(module));
            let stats =
                MirInliner { target, physical, tagged_only, calls, straight_line, shared_runs }
                    .run(module);
            for &caller in &stats.modified {
                analyses.invalidate_local_summary(caller);
            }
            if !stats.consumed.is_empty()
                && let Some(keep) =
                    super::cfg_simplify::remove_unreferenced_functions(module, &stats.consumed)
            {
                analyses.retain_local_summaries(&keep);
            }
            if stats.inlined == 0 {
                break;
            }
            changed = true;
        }
        let mut edited_later = false;
        if physical && !tagged_only {
            edited_later = guards::inline_guards(module, &target);
            if dispatch::inline_dispatch(module, &target) {
                let _ = super::cfg_simplify::FunctionDce.run_pass(gcx, module, analyses);
                edited_later = true;
            }
        }
        if !edited_later {
            analyses.preserve_local_summaries();
        }
        changed | edited_later
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
    target: Target,
    physical: bool,
    /// Clone only `@custom:solar-inline` callees.
    tagged_only: bool,
    calls: MemoryCallSummaries,
    /// The creation-only functions that run a bounded number of times, whose code the creation
    /// model prices.
    straight_line: DenseBitSet<MirFunctionId>,
    /// The runs of instructions the backend shares between functions. Size mode only.
    shared_runs: Option<SharedRuns>,
}

impl MirInliner {
    /// A hot leaf shared by more call sites than this stays a call: every
    /// clone deposits the whole body again.
    const MAX_HOT_LEAF_CALL_SITES: usize = 8;
    /// Live words a caller may hold across an inlined body plus the body's own
    /// peak, leaving stack-addressing headroom for operand staging. A hot-leaf
    /// clone inside a loop body also keeps the loop's carried words resident
    /// through every join it adds; at ten words, two of the Base64 decoder's four
    /// lookups stayed calls that drained every carried word, and twelve took 15%
    /// off decoding.
    const STACK_BUDGET: usize = 12;
    /// Instructions `@custom:solar-inline` clones may add to a module over every run of the
    /// inliner. Tagged helpers that each call the previous one twice would otherwise clone
    /// `2^depth` bodies, and compile time would grow with them. The largest safe-Solady module
    /// takes about 4,700.
    const TAGGED_BUDGET: usize = 16 * 1024;
}

/// Statistics for MIR-level inlining.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct MirInlineStats {
    /// Number of call sites inlined.
    inlined: usize,
    /// Callees whose only call site was inlined; nothing calls them afterwards.
    consumed: Vec<MirFunctionId>,
    /// Callers that received a clone.
    modified: Vec<MirFunctionId>,
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
    /// Calls on returning paths require a separate call/frame stack estimate.
    has_returning_icall: bool,
    has_phi: bool,
    phi_stack_peak: Option<usize>,
    /// Writes storage, transient storage, or an immutable, calls or creates an account, or logs.
    has_state_effect: bool,
    has_control_flow: bool,
    is_check_wrapper: bool,
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
    is_pure: bool,
    is_scalar: bool,
}

impl MirInliner {
    /// Runs the inliner over the whole module.
    fn run(&mut self, module: &mut Module) -> MirInlineStats {
        let mut stats = MirInlineStats::default();

        let call_graph = CallGraphInfo::new(module);
        let mut call_counts = call_counts(module, &call_graph);
        // Keep the initial candidate set stable as inlining removes call sites.
        let memory_wrappers = module
            .functions
            .iter_enumerated()
            .filter(|(id, func)| {
                call_counts.get(id).copied().unwrap_or(0) > 0 && is_memory_wrapper(func)
            })
            .map(|(id, func)| (id, scalar_stack_peak(func)))
            .collect::<FxHashMap<_, _>>();

        let mut summaries = self.summarize_module(module);

        let mut artifact_calls = (!self.straight_line.is_empty()
            || summaries.values().any(|summary| summary.is_small_literal_return))
        .then(|| ArtifactCallCounts::new(module, &call_graph));
        let preferred_large_call_sites = self.preferred_large_call_sites(module, &summaries);
        let mut consumed_everywhere =
            all_sites::consumed_everywhere(self, module, &summaries, &call_counts);

        // Specialize dispatcher calls before helper-local inlining introduces phis.
        // Inline only into the callers that `call_counts` covers, so the count updates below stay
        // in that domain.
        let reachable = call_graph.reachable_from_entries();
        let mut caller_ids = module
            .functions
            .indices()
            .filter(|&id| reachable.is_empty() || reachable.contains(id))
            .collect::<Vec<_>>();
        caller_ids.sort_by_key(|caller| {
            summaries.get(caller).is_some_and(|summary| summary.is_function_pointer_dispatcher)
        });
        for caller_id in caller_ids {
            // Leaf bodies cannot contain an inline candidate.
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
                self.find_next_call(module, module.function(caller_id), cursor, &loop_costs)
            {
                cursor = (site.block.index(), site.inst_index + 1);

                let Some(summary) = summaries.get(&site.callee).copied() else {
                    continue;
                };
                let artifact_counts =
                    artifact_calls.as_ref().and_then(|calls| calls.counts.get(&site.callee));
                let call_count = if summary.is_small_literal_return {
                    artifact_counts.map(|counts| counts[0].max(counts[1]))
                } else {
                    None
                }
                .unwrap_or_else(|| call_counts.get(&site.callee).copied().unwrap_or_default());
                // `@custom:solar-inline` skips the growth and stack budgets, as every other test,
                // while the module's tagged clones fit their own.
                let always = Self::always_inline(module, site.callee, &summary);
                let grew_too_much = !always
                    && summaries.get(&caller_id).is_some_and(|s| {
                        s.instruction_count.saturating_sub(base_instructions)
                            > if site.loop_depth > 0 { 128 } else { 64 }
                    });
                let framed_constructor_call = summaries.get(&caller_id).is_some_and(|caller| {
                    caller.is_constructor && summary.internal_frame_size != 0
                });
                if grew_too_much
                    || framed_constructor_call
                    || call_graph.is_recursive(site.callee)
                    || module.function(site.callee).attributes.no_inline
                {
                    continue;
                }
                let preferred = preferred_large_call_sites.get(&site.callee).copied();
                let consume_all = consumed_everywhere.contains(&site.callee);
                let inlineable = self.is_inlineable(
                    module,
                    caller_id,
                    site,
                    summary,
                    Pricing { target: self.target, call_count, consume_all },
                    preferred,
                );
                // Creation code carries its own copy of every callee, so a callee that only this
                // site calls there is removed from it by inlining, whatever the runtime calls.
                // Pricing the site as creation code only adds inlines to those the runtime
                // model chooses.
                let creation_inlineable = !inlineable
                    && self.straight_line.contains(caller_id)
                    && artifact_counts.is_some_and(|counts| {
                        let creation = Pricing {
                            target: self.target.priced_for(true),
                            call_count: counts[0],
                            consume_all: false,
                        };
                        self.is_inlineable(module, caller_id, site, summary, creation, preferred)
                    });
                if !inlineable && !creation_inlineable {
                    continue;
                }

                if !always
                    && let Some(peak) = (summary.has_phi || site.loop_depth > 0)
                        .then_some(summary.phi_stack_peak)
                        .flatten()
                        .or_else(|| memory_wrappers.get(&site.callee).copied())
                {
                    let caller = module.function(caller_id);
                    let liveness =
                        caller_liveness.get_or_insert_with(|| Liveness::compute_live_sets(caller));
                    if surviving_call_words(caller, liveness, site.block, site.inst_index)
                        .saturating_add(peak)
                        > Self::STACK_BUDGET
                    {
                        continue;
                    }
                }

                let callee = module.function(site.callee).clone();
                let caller = module.function_mut(caller_id);
                if inline_call(caller, site.block, site.inst_index, &callee) {
                    stats.inlined += 1;
                    stats.modified.push(caller_id);
                    // The price of its sites was for the body it had.
                    consumed_everywhere.remove(&caller_id);
                    if always {
                        module.tagged_inline_growth += summary.instruction_count;
                    }
                    if call_counts.get(&site.callee) == Some(&1) {
                        stats.consumed.push(site.callee);
                    }
                    caller_liveness = None;
                    // The clone split the call block, so the loop membership
                    // of the calls that followed it must be recomputed before
                    // they are weighed.
                    loop_costs = block_loop_costs(module.function(caller_id));
                    let new_summary = summarize_function(
                        &self.target,
                        module,
                        module.function(caller_id),
                        self.physical,
                    );
                    summaries.insert(caller_id, new_summary);
                    // Remove this call site and count the calls cloned into its caller.
                    // The original callee remains until function DCE runs.
                    if let Some(count) = call_counts.get_mut(&site.callee) {
                        *count = count.saturating_sub(1);
                    }
                    for function in direct_callees(&callee) {
                        *call_counts.entry(function).or_default() += 1;
                    }
                    if let Some(calls) = &mut artifact_calls {
                        calls.inline(caller_id, site.callee, &callee);
                    }
                    cursor = (site.block.index(), 0);
                }
            }
        }

        stats
    }

    fn summarize_module(&self, module: &Module) -> FxHashMap<MirFunctionId, MirInlineSummary> {
        module
            .functions
            .iter_enumerated()
            .map(|(id, func)| (id, summarize_function(&self.target, module, func, self.physical)))
            .collect()
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
                    let InstKind::ICall { function: Callee::Function(function), ref args, .. } =
                        func.inst(inst_id).kind
                    else {
                        continue;
                    };
                    if !summaries
                        .get(&function)
                        .is_some_and(|summary| summary.instruction_count > 4)
                    {
                        continue;
                    }
                    let instructions_after = block.instructions.len() - inst_index - 1;
                    let has_constant_argument =
                        args.iter().any(|&arg| func.value(arg).as_immediate().is_some());
                    let score = (
                        !has_constant_argument,
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
        module: &Module,
        func: &Function,
        start: (usize, usize),
        loop_costs: &FxHashMap<BlockId, LoopCost>,
    ) -> Option<CallSite> {
        for (block, bb) in func.blocks.iter_enumerated().skip(start.0) {
            let start_inst = if block.index() == start.0 { start.1 } else { 0 };
            for (inst_index, &inst_id) in bb.instructions.iter().enumerate().skip(start_inst) {
                if let InstKind::ICall { function: Callee::Function(function), ref args } =
                    func.inst(inst_id).kind
                {
                    return Some(CallSite {
                        block,
                        inst_index,
                        inst: inst_id,
                        callee: function,
                        args_len: args.len(),
                        returns: module.function(function).return_components().len(),
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
        module: &Module,
        caller: MirFunctionId,
        site: CallSite,
        summary: MirInlineSummary,
        Pricing { target, call_count, consume_all }: Pricing,
        preferred_large_call_site: Option<(MirFunctionId, InstId)>,
    ) -> bool {
        let single_call = call_count == 1;
        // Inlining every call site removes the callee, as it does for a sole-use helper.
        let sole = single_call || consume_all;
        let gas = self.target.optimization().is_gas();
        let hot = gas
            && site.loop_depth > 0
            && summary.is_scalar
            && summary.phi_stack_peak.is_some()
            && !summary.has_loop
            && !summary.has_returning_icall
            && summary.internal_frame_size == 0
            && !summary.has_reference_return
            && call_count <= Self::MAX_HOT_LEAF_CALL_SITES;
        let can_specialize_dispatcher =
            summary.is_function_pointer_dispatcher && site.has_constant_function_selector;
        // `@custom:solar-inline` asks for every legal clone, whatever it costs, the spills a
        // body with phis may add included, while the module's tagged budget lasts.
        let always = Self::always_inline(module, site.callee, &summary);
        if self.tagged_only && !always {
            return false;
        }
        if caller == site.callee
            || (summary.is_function_pointer_dispatcher
                && !single_call
                && !can_specialize_dispatcher)
            || summary.is_entry_point
            || summary.is_constructor
            || (summary.has_phi && !always && !(summary.phi_stack_peak.is_some() && (sole || hot)))
            || summary.has_unsupported_terminator
            || summary.return_count == 0
            || (self.physical
                && (summary.return_values > 1
                    || summary.internal_frame_size != 0
                    || module.function(site.callee).instructions().any(|inst| {
                        matches!(
                            module.function(site.callee).inst(inst).kind,
                            InstKind::Alloc { .. }
                        )
                    })))
        {
            return false;
        }
        if always {
            return true;
        }

        let has_reference_return = module
            .function(site.callee)
            .return_components()
            .iter()
            .any(|&ty| matches!(ty, MirType::MEMPTR | MirType::Slice(_)));
        let memory_wrapper = is_memory_wrapper(module.function(site.callee));
        let tiny = summary.block_count == 1
            && summary.return_count == 1
            && summary.instruction_count
                <= if memory_wrapper && gas {
                    6
                } else if single_call {
                    12
                } else {
                    4
                }
            && (!has_reference_return
                || summary.is_transparent_forwarder
                || (single_call && summary.is_small_literal_return))
            && (!summary.has_icall || summary.is_transparent_forwarder || (memory_wrapper && gas))
            && (single_call || !summary.void_forwarder_adds_args)
            && (!summary.has_control_flow || summary.is_check_wrapper);
        let consume = sole
            && summary.internal_frame_size == 0
            && !summary.has_reference_return
            && summary.instruction_count <= 256;
        let specialize = gas
            && call_count > 1
            && preferred_large_call_site == Some((caller, site.inst))
            && module.function(caller).blocks.len() == 1
            && module.function(caller).instructions().count() == 1
            && site.has_constant_argument
            && summary.is_pure
            && summary.is_scalar
            && summary.block_count == 1
            && summary.instruction_count <= 64
            && !summary.has_icall
            && !has_reference_return
            && !summary.has_control_flow;
        let storage = gas && self.shares_storage_read(module, caller, site);
        let bounded = summary.instruction_count <= 96
            && summary.block_count <= 16
            && (sole
                || hot
                || summary.block_count <= 10
                || preferred_large_call_site == Some((caller, site.inst)));
        if !(tiny
            || consume
            || specialize
            || (bounded && (hot || storage || can_specialize_dispatcher)))
        {
            return false;
        }

        // Multi-use stateful callees are usually not worth cloning unless the
        // call is hot or the body is no larger than the internal-call protocol
        // it replaces. Single-call callees disappear from emitted runtime
        // bytecode after inlining, so they are allowed through the normal
        // code-growth check below.
        if !sole
            && site.loop_depth == 0
            && summary.has_state_effect
            && summary.estimated_code_size
                > estimated_icall_code_size(&self.target, site)
                    + estimated_internal_return_code_size(&self.target, summary, site)
        {
            return false;
        }

        // `all_sites` priced the callee over every call site.
        consume_all
            || self.inline_lifetime_cost_improves(module, target, summary, site, single_call)
    }

    /// Compares runtime protocol savings over the expected contract lifetime
    /// with the bytecode deposit cost of cloning the body. The body itself
    /// executes in both alternatives and therefore contributes only to
    /// deposited bytes.
    ///
    /// Reference: <https://github.com/argotorg/solidity/blob/f401782df49be312ea4ef52a2d467cf5183b5906/libevmasm/Inliner.cpp#L139>
    fn inline_lifetime_cost_improves(
        &self,
        module: &Module,
        target: Target,
        summary: MirInlineSummary,
        site: CallSite,
        single_call: bool,
    ) -> bool {
        let mut inlined_bytes = summary.estimated_code_size;
        let mut removed_bytes = estimated_icall_code_size(&target, site);
        if single_call {
            // Consuming the callee leaves the code the backend shares with other functions
            // emitted again.
            inlined_bytes += self
                .shared_runs
                .as_ref()
                .map_or(0, |runs| runs.shared_bytes(&target, module, site.callee));
            removed_bytes = removed_bytes.saturating_add(
                summary.estimated_code_size
                    + estimated_internal_return_code_size(&target, summary, site),
            );
        }
        let loop_executions = if !target.optimization().is_gas() {
            1
        } else if site.loop_depth > 0 && !site.loop_counted {
            Target::UNCOUNTED_LOOP_ITERATIONS
        } else {
            site.loop_executions
        };
        profitable(
            &target,
            inlined_bytes,
            removed_bytes,
            estimated_icall_savings(&target, site, summary),
            loop_executions,
        )
    }

    fn shares_storage_read(&self, module: &Module, caller: MirFunctionId, site: CallSite) -> bool {
        let Some(summary) = self.calls.get(site.callee) else {
            return false;
        };
        let callee = module.function(site.callee);
        if summary.writes(AddressSpace::Storage)
            || summary.may_observe_gas()
            || callee.attributes.no_inline
            || site.returns > 1
        {
            return false;
        }
        let Some(slots) = summary.storage_slots(AddressSpace::Storage, false) else {
            return false;
        };
        let caller = module.function(caller);
        caller.blocks[site.block].instructions.iter().any(|&inst| {
            matches!(caller.inst(inst).kind, InstKind::Load(slot, PtrLocation::Storage)
            if matches!(caller.storage_alias(inst, slot), StorageAlias::Slot(slot)
                if slots.contains(&slot) && callee.instructions().any(|inst| {
                    matches!(callee.inst(inst).kind, InstKind::Load(value, PtrLocation::Storage)
                        if callee.storage_alias(inst, value) == StorageAlias::Slot(slot))
                })))
        })
    }

    /// Whether a call to `callee` is cloned as `@custom:solar-inline` asks: while the module's
    /// tagged clones, this one included, fit their budget.
    fn always_inline(module: &Module, callee: MirFunctionId, summary: &MirInlineSummary) -> bool {
        module.function(callee).attributes.always_inline
            && module.tagged_inline_growth.saturating_add(summary.instruction_count)
                <= Self::TAGGED_BUDGET
    }
}

fn profitable(
    target: &Target,
    added_bytes: usize,
    removed_bytes: usize,
    saved_gas: u64,
    executions: u64,
) -> bool {
    if added_bytes <= removed_bytes {
        return true;
    }
    target.optimization().is_gas()
        && u128::from(saved_gas)
            .saturating_mul(u128::from(target.expected_executions()))
            .saturating_mul(u128::from(executions))
            > (added_bytes - removed_bytes) as u128 * u128::from(target.byte_gas())
}

/// The cost model of a call site's caller and the calls to its callee that the code carrying the
/// caller holds.
#[derive(Clone, Copy)]
struct Pricing {
    target: Target,
    call_count: usize,
    /// Whether inlining every call site and deleting the callee is profitable.
    consume_all: bool,
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
            for function in direct_callees(func) {
                result.add(caller, function);
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
        for function in direct_callees(callee) {
            self.add(caller, function);
        }
    }
}

/// The number of direct internal calls and tail calls to each function from reachable functions.
///
/// An earlier inliner run can leave a wrapper that no one calls anymore until function DCE.
/// Its calls must not hide that the callee now has a single caller.
fn call_counts(module: &Module, call_graph: &CallGraphInfo) -> FxHashMap<MirFunctionId, usize> {
    let reachable = call_graph.reachable_from_entries();
    let mut counts = FxHashMap::default();
    for (id, func) in module.functions.iter_enumerated() {
        if !reachable.is_empty() && !reachable.contains(id) {
            continue;
        }
        for function in direct_callees(func) {
            *counts.entry(function).or_default() += 1;
        }
    }
    counts
}

/// Recognizes a complete constant bytes initializer without relaxing the
/// general reference-return guard. Inlining preserves the allocation and every
/// store at the call site; separate calls still produce separate objects.
fn is_small_literal_return(func: &Function) -> bool {
    if func.attributes.no_inline
        || func.internal_frame_size != 0
        || func.blocks.len() != 1
        || func.return_components() != [MirType::MEMPTR]
    {
        return false;
    }
    let block = &func.blocks[BlockId::ENTRY];
    let [alloc, rest @ ..] = block.instructions.as_slice() else { return false };
    if func.inst(*alloc).metadata.preserves_fmp() {
        return false;
    }
    let InstKind::Alloc { size, semantics: AllocationSemantics::INTERNAL } = func.inst(*alloc).kind
    else {
        return false;
    };
    let Some(object) = func.inst_result_value(*alloc) else { return false };
    if !matches!(&block.terminator, Some(Terminator::Return { values }) if values.as_slice() == [object])
    {
        return false;
    }
    let mut length = None;
    let mut words = 0;
    for &inst in rest {
        if func.is_object_derivation(inst) {
            continue;
        }
        let InstKind::Store(address, value, PtrLocation::Memory) = func.inst(inst).kind else {
            return false;
        };
        match func.object_address(address) {
            Some((base, 0)) if base == object && length.is_none() => {
                length = func.value_u64(value);
            }
            Some((base, 32)) if base == object && func.value_u256(value).is_some() => words += 1,
            _ => return false,
        }
    }
    let Some(length) = length else { return false };
    length <= 32
        && func.value_u64(size) == Some(32 + length.next_multiple_of(32))
        && words == usize::from(length != 0)
}

/// A direct call whose callee cannot resume the caller. Keep tail-call chains
/// conservative; this local test only accepts bodies with explicit message exits.
fn is_terminal_call(module: &Module, kind: &InstKind) -> bool {
    let InstKind::ICall { function: Callee::Function(callee), .. } = kind else {
        return false;
    };
    never_returns(module.function(*callee))
}

/// Whether every path through `callee` ends the message, as a shared revert helper does.
fn never_returns(callee: &Function) -> bool {
    !callee.blocks.is_empty()
        && callee.blocks.iter().all(|block| {
            matches!(
                block.terminator,
                Some(
                    Terminator::Jump(_)
                        | Terminator::Branch { .. }
                        | Terminator::Switch { .. }
                        | Terminator::Revert { .. }
                        | Terminator::RevertReturndata
                        | Terminator::Stop
                        | Terminator::Invalid
                        | Terminator::ReturnData { .. }
                        | Terminator::SelfDestruct { .. }
                )
            )
        })
}

fn summarize_function(
    target: &Target,
    module: &Module,
    func: &Function,
    objects_lowered: bool,
) -> MirInlineSummary {
    let mut summary = MirInlineSummary {
        block_count: func.blocks.len(),
        return_values: func.return_components().len(),
        param_count: func.params.len(),
        internal_frame_size: func.internal_frame_size,
        is_entry_point: func.attributes.is_fallback
            || func.attributes.is_receive
            || func.selector.is_some(),
        is_constructor: func.attributes.is_constructor,
        has_reference_return: func.return_components().iter().any(|&ty| match ty {
            MirType::MEMPTR => !objects_lowered,
            MirType::Slice(_) => true,
            _ => false,
        }),
        is_transparent_forwarder: is_transparent_forwarder(module, func),
        is_small_literal_return: is_small_literal_return(func),
        is_check_wrapper: is_check_wrapper(func),
        is_function_pointer_dispatcher: func.attributes.is_function_pointer_dispatcher,
        is_pure: func.attributes.state_mutability == StateMutability::Pure,
        has_loop: !func.blocks.is_empty() && has_back_edge(func),
        ..MirInlineSummary::default()
    };

    for block in func.blocks.iter() {
        for &inst_id in &block.instructions {
            let kind = &func.inst(inst_id).kind;
            let (inst_cost, instructions) = estimate_inst_cost(target, module, func, kind);
            summary.instruction_count += instructions;
            summary.estimated_code_size += inst_cost.bytes as usize;
            match kind {
                InstKind::ICall { function: Callee::Function(_), args } => {
                    summary.has_icall = true;
                    summary.has_returning_icall |= !is_terminal_call(module, kind);
                    if summary.is_transparent_forwarder && func.return_components().is_empty() {
                        summary.void_forwarder_adds_args = args.len() > func.params.len();
                    }
                }
                InstKind::Phi(_) => summary.has_phi = true,
                // ABI decoding validates its input through branches, and dynamic encoding
                // emits copy loops and padding branches, so neither operation is a tiny leaf.
                InstKind::AbiDecode { .. }
                | InstKind::ValidateStorageBytes(..)
                | InstKind::StorageBytesLoad(..)
                | InstKind::StorageArrayLoad { .. }
                | InstKind::StorageBytesStore(..)
                | InstKind::StorageBytesStoreLiteral { .. }
                | InstKind::StorageClearWords(..)
                | InstKind::CheckedBinary { .. }
                | InstKind::ICall {
                    function:
                        Callee::Builtin(
                            Builtin::CheckedAddMod
                            | Builtin::CheckedMulMod
                            | Builtin::Check { .. }
                            | Builtin::Require(_)
                            | Builtin::Transfer,
                        ),
                    ..
                } => {
                    summary.has_control_flow = true;
                }
                InstKind::AbiEncodePacked { parts, hash: false }
                    if parts.iter().any(|part| {
                        matches!(
                            part,
                            crate::mir::PackedPart::Bytes(_) | crate::mir::PackedPart::Array { .. }
                        )
                    }) =>
                {
                    summary.has_control_flow = true;
                }
                InstKind::AbiEncode { layout, .. } if abi_layout_has_loops(layout) => {
                    summary.has_control_flow = true;
                }
                _ => {}
            }
            summary.has_state_effect |= matches!(
                kind.effect_kind(),
                EffectKind::StorageWrite
                    | EffectKind::TransientWrite
                    | EffectKind::ImmutableWrite
                    | EffectKind::ExternalCall
                    | EffectKind::Create
                    | EffectKind::Log
            );
        }
        match block.terminator.as_ref() {
            Some(term @ Terminator::Return { .. }) => {
                summary.return_count += 1;
                summary.estimated_code_size +=
                    estimate_terminator_cost(target, term).bytes as usize;
            }
            Some(term @ (Terminator::Revert { .. } | Terminator::RevertReturndata)) => {
                summary.estimated_code_size +=
                    estimate_terminator_cost(target, term).bytes as usize;
            }
            // `outline-reverts` turns failure paths into tail calls to shared revert helpers.
            // Such a jump ends the message wherever it is cloned, but a tail call to a function
            // that returns would return from the caller instead.
            Some(term @ Terminator::TailCall { function, .. })
                if never_returns(module.function(*function)) =>
            {
                summary.estimated_code_size +=
                    estimate_terminator_cost(target, term).bytes as usize;
            }
            Some(
                term
                @ (Terminator::Jump(_) | Terminator::Branch { .. } | Terminator::Switch { .. }),
            ) => {
                summary.has_control_flow = true;
                summary.estimated_code_size +=
                    estimate_terminator_cost(target, term).bytes as usize;
            }
            Some(Terminator::ReturnData { .. })
            | Some(Terminator::Stop)
            | Some(Terminator::SelfDestruct { .. })
            | Some(Terminator::TailCall { .. })
            | None => summary.has_unsupported_terminator = true,
            Some(Terminator::Invalid) => {}
        }
    }

    summary.is_scalar = (!summary.has_icall
        || (!summary.has_phi && summary.return_count == 1 && !summary.has_returning_icall))
        && func.instructions().all(|inst| {
            is_terminal_call(module, &func.inst(inst).kind)
                || matches!(
                    func.inst(inst).kind.effect_kind(),
                    EffectKind::Pure | EffectKind::MemoryRead | EffectKind::EnvironmentRead
                )
        });
    // A single-use helper's effects and calls stay at its one call site, so any loop-free body may
    // fit the stack budget there. Loops outside the scalar class stay calls: consuming Solady's
    // insertion sort, which swaps memory words, raised its runtime gas by up to 0.35%.
    if summary.block_count <= 16
        && summary.instruction_count <= 112
        && summary.param_count <= 4
        && !func.params.iter().any(|ty| matches!(ty, MirType::Slice(_)))
        && summary.return_count <= 3
        && summary.return_count != 0
        && summary.internal_frame_size == 0
        && !summary.has_reference_return
        && (summary.is_scalar || (summary.has_phi && !summary.has_loop))
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
    let mut stack = Vec::new();
    stack.push((BlockId::ENTRY, 0));
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
    let liveness = Liveness::compute_live_sets(func);
    let mut peak = 0;
    for (block, body) in func.blocks.iter_enumerated() {
        let mut live = DenseBitSet::from(liveness.live_out(block));
        if let Some(term) = &body.terminator {
            term.visit_operands(|value| {
                live.insert(value);
            });
        }
        peak = peak.max(live_word_count(func, &live));
        for &inst in body.instructions.iter().rev() {
            if let Some(result) = func.inst_result_value(inst) {
                live.remove(result);
            }
            if !matches!(func.inst(inst).kind, InstKind::Phi(_)) {
                func.inst(inst).visit_operands(|value| {
                    live.insert(value);
                });
            }
            peak = peak.max(live_word_count(func, &live));
        }
    }
    peak
}

/// Caller words that survive the internal call and overlap an inline expansion.
fn surviving_call_words(
    func: &Function,
    liveness: &Liveness,
    block: BlockId,
    inst_index: usize,
) -> usize {
    let body = &func.blocks[block];
    let mut live = DenseBitSet::from(liveness.live_out(block));
    if let Some(term) = &body.terminator {
        term.visit_operands(|value| {
            live.insert(value);
        });
    }
    for &inst in body.instructions[inst_index + 1..].iter().rev() {
        if let Some(result) = func.inst_result_value(inst) {
            live.remove(result);
        }
        func.inst(inst).visit_operands(|value| {
            live.insert(value);
        });
    }
    if let Some(result) = func.inst_result_value(body.instructions[inst_index]) {
        live.remove(result);
    }
    live_word_count(func, &live)
}

fn live_word_count(func: &Function, live: &DenseBitSet<ValueId>) -> usize {
    live.iter().filter(|&value| matches!(func.value(value), Value::Arg(_) | Value::Inst(_))).count()
}

/// A call followed by bounded physical word operations, with no allocation or frame locals.
fn is_memory_wrapper(func: &Function) -> bool {
    if func.attributes.no_inline
        || func.internal_frame_size != 0
        || func.blocks.len() != 1
        || func.params.len() > 2
        || func.return_components() != [MirType::I256]
    {
        return false;
    }
    let block = &func.blocks[BlockId::ENTRY];
    let [call, rest @ ..] = block.instructions.as_slice() else { return false };
    rest.len() <= 5
        && matches!(func.inst(*call).kind, InstKind::ICall { function: Callee::Function(_), .. })
        && rest.iter().any(|&inst| {
            matches!(
                func.inst(inst).kind,
                InstKind::Store(.., PtrLocation::Memory) | InstKind::Store8(..)
            )
        })
        && rest.iter().all(|&inst| {
            matches!(
                func.inst(inst).kind,
                InstKind::Add(..)
                    | InstKind::Sub(..)
                    | InstKind::Load(_, PtrLocation::Memory)
                    | InstKind::Store(.., PtrLocation::Memory)
                    | InstKind::Store8(..)
            )
        })
        && matches!(&block.terminator, Some(Terminator::Return { values }) if values.len() == 1)
}

/// A semantic check has one returning path and no allocation on that path.
/// Keep the exception narrow: no other calls, memory effects, or computations.
fn is_check_wrapper(func: &Function) -> bool {
    if func.attributes.no_inline
        || func.blocks.len() != 1
        || func.internal_frame_size != 0
        || !func.return_components().is_empty()
    {
        return false;
    }
    let block = &func.blocks[BlockId::ENTRY];
    if !matches!(&block.terminator, Some(Terminator::Return { values }) if values.is_empty()) {
        return false;
    }
    let call = match block.instructions.as_slice() {
        [call] => *call,
        [negation, call]
            if matches!(func.inst(*negation).kind, InstKind::Eq(lhs, rhs)
                if func.value_u64(lhs) == Some(0) || func.value_u64(rhs) == Some(0)) =>
        {
            *call
        }
        _ => return false,
    };
    matches!(
        func.inst(call).kind,
        InstKind::ICall { function: Callee::Builtin(Builtin::Check { .. }), .. }
    )
}

fn is_transparent_forwarder(module: &Module, func: &Function) -> bool {
    if func.attributes.no_inline
        || func.selector.is_some()
        || func.attributes.is_constructor
        || func.attributes.is_fallback
        || func.attributes.is_receive
        || func.blocks.len() != 1
        || func.internal_frame_size != 0
        || func.return_components().len() > 1
    {
        return false;
    }

    if is_identity_function(func) {
        return true;
    }

    if matches!(func.return_components(), [MirType::Slice(_)])
        && matches!(func.blocks[BlockId::ENTRY].terminator.as_ref(),
            Some(Terminator::Return { values }) if values.len() == 1)
        && func.instructions().all(|inst| {
            matches!(
                func.inst(inst).kind,
                InstKind::MakeSlice { .. }
                    | InstKind::ExtractValue { ty: MirType::Slice(_), .. }
                    // `msg.data` measures its slice with `calldatasize`, a fixed-cost read.
                    | InstKind::CalldataSize
            )
        })
    {
        return true;
    }

    // Widening an argument emits no code, as `EnumerableSet` wrappers do with their values.
    let Some((call, casts)) = func.blocks[BlockId::ENTRY].instructions.split_last() else {
        return false;
    };
    if !casts.iter().all(|&inst| {
        matches!(func.inst(inst).kind, InstKind::Zext(value)
            if matches!(func.value(value), Value::Arg(_)))
    }) {
        return false;
    }
    let InstKind::ICall { function: Callee::Function(function), .. } = func.inst(*call).kind else {
        return false;
    };
    match (
        module.function(function).return_components().len(),
        func.blocks[BlockId::ENTRY].terminator.as_ref(),
    ) {
        (0, Some(Terminator::Return { values })) => {
            func.return_components().is_empty() && values.is_empty()
        }
        (1, Some(Terminator::Return { values })) => {
            func.return_components().len() == 1
                && func.inst_result_value(*call).is_some_and(|result| values.as_slice() == [result])
        }
        _ => false,
    }
}

fn is_identity_function(func: &Function) -> bool {
    let [param] = func.params.raw.as_slice() else { return false };
    let [return_ty] = func.return_components() else { return false };
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
    func.params == [MirType::I256]
        && func.return_components() == [MirType::I256]
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
        AbiType::FixedArray { element, len, .. } => {
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
fn estimate_inst_cost(
    target: &Target,
    module: &Module,
    func: &Function,
    kind: &InstKind,
) -> (Cost, usize) {
    let seq =
        |codes: &[u8]| codes.iter().map(|&code| target.opcode(code)).fold(Cost::ZERO, Cost::plus);
    if !matches!(kind, InstKind::ICall { .. } | InstKind::LoadImmutable(_)) {
        let op = kind.op();
        if select::opcode_lowering(&op).is_some() {
            return (target.op(&op, |_| None), 1);
        }
    }
    let code = match kind {
        InstKind::Ne(..) => seq(&[op::EQ, op::ISZERO]),
        InstKind::Trunc(..) | InstKind::Sext(..) | InstKind::PtrToInt(..) => {
            target.op(&kind.op(), |_| None)
        }
        InstKind::InsertValue { .. }
        | InstKind::ExtractValue { .. }
        | InstKind::MakeSlice { .. }
        | InstKind::IntToPtr(..)
        | InstKind::Zext(_) => Cost::ZERO,
        InstKind::PtrAdd(..) => seq(&[op::ADD]),
        InstKind::Fmp | InstKind::FrameLoad { .. } => seq(&[op::MLOAD]),
        InstKind::SetFmp(_) | InstKind::FrameStore { .. } => seq(&[op::MSTORE]),
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
        // A pushed offset into the immutables area and the store.
        InstKind::StoreImmutable(..) => seq(&[op::PUSH2, op::MSTORE]),
        InstKind::DataCopy(..) => seq(&[op::CODECOPY]),
        InstKind::DataSize(..) => seq(&[op::PUSH2]),
        // Zero by copying from beyond the end of calldata.
        InstKind::MemoryZero(..) => seq(&[op::CALLDATASIZE, op::CALLDATACOPY]),
        InstKind::ConstructorArgsBase => seq(&[op::PUSH2]),
        InstKind::ConstructorArgsEnd => seq(&[op::PUSH2, op::PUSH2, op::SUB, op::CODESIZE]),
        InstKind::HeapFloor => seq(&[op::PUSH2]),
        InstKind::InternalFrameAddr(_) => seq(&[op::PUSH1, op::ADD]),
        InstKind::LibraryAddress(_) => seq(&[op::PUSH20]),
        // Typed PUSH<N> placeholder patched at deploy time, cleaned for the narrower types.
        InstKind::LoadImmutable(id) => {
            let ty = module.immutable_type(*id);
            let encoding = ty.immutable_encoding().expect("validated immutable declaration");
            let type_size = immutable_push_type_size(
                encoding,
                target.optimization(),
                target.evm_version().has_bitwise_shifting(),
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
        // Includes allocation, argument packing, the precompile call, and result extraction.
        InstKind::ValidateStorageBytes(_) => Cost::new(80, 40),
        InstKind::StorageBytesLoad(_) => Cost::new(400, 150),
        InstKind::StorageArrayLoad { .. } => Cost::new(400, 150),
        InstKind::StorageBytesStore(..) => Cost::new(500, 180),
        InstKind::StorageBytesStoreLiteral { bytes, .. } => {
            Cost::new(500, 180 + bytes.len() as u32)
        }
        InstKind::StorageClearWords(..) => Cost::new(120, 32),
        InstKind::ICall { function: Callee::Builtin(Builtin::Erc7201), .. } => Cost::new(90, 30),
        InstKind::ICall {
            function: Callee::Builtin(Builtin::CheckedAddMod | Builtin::CheckedMulMod),
            ..
        } => Cost::new(32, 9),
        InstKind::ICall {
            function: Callee::Builtin(Builtin::Sha256 | Builtin::Ripemd160), ..
        } => Cost::new(800, 64),
        InstKind::ICall { function: Callee::Builtin(Builtin::EcRecover), .. } => {
            Cost::new(900, 100)
        }
        InstKind::ICall { function: Callee::Builtin(Builtin::Check { .. }), .. } => {
            Cost::new(24, 8)
        }
        InstKind::ICall { function: Callee::Builtin(Builtin::Require(_)), .. } => Cost::new(40, 24),
        InstKind::ValidateAbi(_) => Cost::new(0, 0),
        InstKind::CheckedBinary { op: crate::mir::CheckedOp::Pow, .. } => Cost::new(300, 128),
        InstKind::CheckedBinary { .. } => Cost::new(30, 20),
        InstKind::AbiEncodePacked { parts, .. } => {
            Cost::new(60 + parts.len() as u32 * 20, 24 + parts.len() as u32 * 12)
        }
        InstKind::MappingSlot(..) => seq(&[op::MSTORE, op::MSTORE, op::KECCAK256]),
        InstKind::MappingSlotBytes(key, _) if func.value_ty(*key) == Some(MirType::MEMPTR) => {
            seq(&[
                op::MLOAD,
                op::ADD,
                op::MSTORE,
                op::MLOAD,
                op::ADD,
                op::MSTORE,
                op::ADD,
                op::KECCAK256,
            ])
        }
        InstKind::MappingSlotBytes(..) => seq(&[
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
        InstKind::AddressCall { .. } => Cost::new(720, 12),
        InstKind::ICall { function: Callee::Builtin(Builtin::ReturndataBytes), .. } => {
            Cost::new(60, 24)
        }
        InstKind::ICall { function: Callee::Builtin(Builtin::Send), .. } => Cost::new(720, 12),
        InstKind::ICall { function: Callee::Builtin(Builtin::Transfer), .. } => Cost::new(740, 20),
        InstKind::Call { .. }
        | InstKind::CallCode { .. }
        | InstKind::StaticCall { .. }
        | InstKind::DelegateCall { .. } => seq(&[op::CALL]),
        InstKind::ICall { function: Callee::Function(function), args } => {
            target.icall(args.len(), module.function(*function).return_components().len(), 0)
        }
        // A phi or a select is a stack move at the join.
        InstKind::Phi(_) | InstKind::Select(..) => seq(&[op::DUP1]),
        // Every other operation lowers to one opcode and was priced above.
        _ => {
            debug_assert!(false, "operation without a single opcode is not priced: {kind}");
            seq(&[op::ADD])
        }
    };
    let instructions = match kind {
        // Casts, slice construction, and aggregate insertions and projections emit no code of their
        // own: `lower-structs` turns struct fields into separate words.
        InstKind::Zext(_)
        | InstKind::IntToPtr(..)
        | InstKind::PtrToInt(_, 256)
        | InstKind::MakeSlice { .. }
        | InstKind::InsertValue { .. }
        | InstKind::ExtractValue { .. } => 0,
        InstKind::MappingSlot(..) | InstKind::StorageArrayDataSlot(..) => 3,
        InstKind::MappingSlotBytes(key, _) if func.value_ty(*key) == Some(MirType::MEMPTR) => 8,
        InstKind::MappingSlotBytes(..) => 9,
        InstKind::StorageArrayElementSlot { .. } => 4,
        InstKind::AbiEncode { layout, .. } => abi_layout_expansion(layout),
        _ => 1,
    };
    (code, instructions)
}

/// Estimated code of a terminator, as the opcode sequence it emits priced by the target.
fn estimate_terminator_cost(target: &Target, term: &Terminator) -> Cost {
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

fn estimated_icall_savings(target: &Target, site: CallSite, summary: MirInlineSummary) -> u64 {
    let frame_words = (summary.internal_frame_size / EvmMemoryLayout::WORD_SIZE)
        + (EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE / EvmMemoryLayout::WORD_SIZE)
        + (site.args_len + site.returns) as u64;
    let protocol = target.runtime_gas(target.icall(site.args_len, site.returns, frame_words));
    let return_protocol =
        target.runtime_gas(target.internal_return(summary.param_count, site.returns));
    let loop_multiplier = (site.loop_depth as u64).saturating_add(1);
    u64::from(protocol + return_protocol) * loop_multiplier
}

fn estimated_icall_code_size(target: &Target, site: CallSite) -> usize {
    target.icall(site.args_len, site.returns, 0).bytes as usize
}

fn estimated_internal_return_code_size(
    target: &Target,
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
    let loop_info = analyzer.analyze_trip_counts(func);
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
            } else if counted.is_none() {
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
            if let Some(target) = direct_dispatch_target(module, &dispatcher, &selector) {
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
            if let InstKind::ICall { function: Callee::Function(function), ref args, .. } =
                func.inst(inst_id).kind
                && let Some(selector) =
                    args.first().and_then(|&arg| func.value(arg).as_immediate()).cloned()
            {
                return Some((block, inst_index, function, selector));
            }
        }
    }
    None
}

fn direct_dispatch_target(
    module: &Module,
    dispatcher: &Function,
    selector: &Immediate,
) -> Option<MirFunctionId> {
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
                && dispatcher.value_ty(arg) == Some(MirType::I256)
                && dispatcher.value(value).as_immediate().and_then(Immediate::as_u256)
                    == Some(selector)
        });
        if matches_selector {
            return direct_dispatch_case_target(module, dispatcher, *then_block);
        }
    }
    None
}

fn direct_dispatch_case_target(
    module: &Module,
    dispatcher: &Function,
    block: BlockId,
) -> Option<MirFunctionId> {
    let block = &dispatcher.blocks[block];
    let [call] = block.instructions.as_slice() else {
        return None;
    };
    let InstKind::ICall { function: Callee::Function(function), args } =
        &dispatcher.inst(*call).kind
    else {
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
    match module.function(*function).return_components().len() {
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
    let InstKind::ICall { function: Callee::Function(function), args, .. } =
        &mut caller.inst_mut(call).kind
    else {
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
    let InstKind::ICall { ref args, .. } = caller.inst(call_inst).kind else {
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
    let InstKind::ICall { args, .. } = caller.inst(call_inst).kind.clone() else {
        return None;
    };
    let returns = callee.return_components().len();

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

    // object_arg = inttoptr raw_arg
    // jump cloned_entry(object_arg)
    // Calls can carry raw pointer words; cloned semantic operations still require
    // the callee's pointer parameters. Materialize the zero-cost cast at the cloned entry.
    let mut args = args;
    let mut argument_views = Vec::new();
    for (arg, &ty) in args.iter_mut().zip(&callee.params) {
        if ty == MirType::MEMPTR && caller.value_ty(*arg) != Some(ty) {
            if !caller.value_ty(*arg).is_some_and(MirType::is_word) {
                return None;
            }
            let (inst, value) = caller.alloc_value_inst(
                Instruction::new(InstKind::IntToPtr(*arg), Some(ty)).with_debug_info_dropped(),
            );
            argument_views.push(inst);
            *arg = value;
        }
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
        let signature_slots = caller.params.len().checked_add(caller.return_components().len())?;
        let signature_size =
            u64::try_from(signature_slots).ok()?.checked_mul(EvmMemoryLayout::WORD_SIZE)?;
        EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE.checked_add(signature_size)?
    };
    let frame_base = caller_frame_prefix.checked_add(caller.internal_frame_size)?;
    let callee_signature_slots =
        callee.params.len().checked_add(callee.return_components().len())?;
    let callee_signature_size =
        u64::try_from(callee_signature_slots).ok()?.checked_mul(EvmMemoryLayout::WORD_SIZE)?;
    let callee_frame_prefix =
        EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE.checked_add(callee_signature_size)?;
    caller.internal_frame_size =
        caller.internal_frame_size.checked_add(callee.internal_frame_size)?;

    let mut cloner = InlineCloner::new(caller, callee, frame_base, callee_frame_prefix, args);
    let cloned_entry = cloner.clone_blocks(continuation)?;
    // cloned_entry: object argument casts; cloned callee body
    cloner.caller.blocks[cloned_entry].instructions.splice(0..0, argument_views);

    // icall @callee !metadata(call) => jump cloned_entry !metadata(call)
    cloner.caller.blocks[call_block].terminator = Some(Terminator::Jump(cloned_entry));
    cloner.caller.blocks[call_block].terminator_metadata =
        cloner.caller.inst(call_inst).metadata.debug_context();

    let mut replacements = FxHashMap::default();
    if returns > 0 {
        let return_values = build_return_values(
            cloner.caller,
            continuation,
            callee.return_components(),
            &cloner.return_edges,
        )?;
        replacements.insert(call_result?, return_values[0]);
        insert_return_buffer_stores(
            cloner.caller,
            continuation,
            &return_values,
            callee.return_components(),
            caller_is_external,
            caller_frame_prefix,
        )?;
    }

    cloner.caller.replace_uses(&replacements);
    rebuild_predecessors(cloner.caller);
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
    //   => object = alloc size
    //      mstore object, length
    //      mstore (ptradd object, 32), word
    for &inst in &callee.blocks[BlockId::ENTRY].instructions {
        let source = callee.inst(inst);
        let kind = cloner.clone_inst_kind(source.kind.clone())?;
        let mut instruction = Instruction::new(kind, source.result_ty);
        instruction.metadata.copy_debug_context(&source.metadata);
        if matches!(instruction.kind, InstKind::Alloc { .. }) {
            // object = alloc size
            //   => object = alloc size !metadata(preserves_fmp)
            //
            // The inlined result escapes the call. Preserve its heap reservation even when a
            // later ABI encoding embeds the literal bytes and removes its initialization stores.
            instruction.metadata.set_preserves_fmp(true);
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
    rebuild_predecessors(cloner.caller);
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
            builder.store(offset, value);
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
    if return_tys.iter().all(|ty| !matches!(*ty, MirType::Slice(_) | MirType::MEMPTR)) {
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
            InstKind::Load(address, PtrLocation::Memory) => {
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
