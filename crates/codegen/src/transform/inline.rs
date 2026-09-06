//! Function inlining optimization pass.
//!
//! This module inlines profitable MIR internal calls to remove their call
//! protocol and expose further optimization opportunities.

use crate::{
    analysis::{CallGraphInfo, LoopAnalyzer},
    immutable::immutable_push_type_size,
    memory::{EvmMemoryLayout, MemoryLayoutPolicy},
    mir::{
        AbiLayout, AbiType, BlockId, FrameMode, FrameSlotKind, Function, FunctionBuilder,
        FunctionId as MirFunctionId, Immediate, ImmutableEncoding, InstId, InstKind, Instruction,
        MirType, Module, Terminator, Value, ValueId,
    },
    pass::MirPass,
};
use smallvec::SmallVec;
use solar_ast::StateMutability;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
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
        _analyses: &mut crate::pass::ModuleAnalyses,
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
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        let mut inliner = MirInliner::for_tiny_leaves();
        inliner.run(gcx, module).inlined != 0
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
        _analyses: &mut crate::pass::ModuleAnalyses,
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
        _analyses: &mut crate::pass::ModuleAnalyses,
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
    /// Optional hard ceiling for the module size estimator. Normal gas-mode
    /// profitability is governed by lifetime cost instead of this ceiling;
    /// zero remains the explicit off switch used by size mode.
    max_module_code_size: usize,
    mode: InlineMode,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InlineMode {
    Normal,
    TinyLeaves,
    ConstantLeaves,
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
            expected_executions_per_deployment: 200,
            max_module_code_size: usize::MAX,
            mode: InlineMode::Normal,
        }
    }
}

impl MirInliner {
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct MirInlineStats {
    /// Number of internal call sites considered.
    call_sites: usize,
    /// Number of call sites inlined.
    inlined: usize,
    /// Number of call sites skipped because the callee was not inlineable.
    skipped: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct MirInlineSummary {
    instruction_count: usize,
    block_count: usize,
    return_count: usize,
    param_count: usize,
    estimated_code_size: usize,
    estimated_runtime_gas: u64,
    internal_frame_size: u64,
    has_icall: bool,
    has_phi: bool,
    has_external_call: bool,
    has_storage_write: bool,
    has_immutable_write: bool,
    has_log: bool,
    has_control_flow: bool,
    has_unsupported_terminator: bool,
    has_reference_return: bool,
    /// A one-block helper that only returns an argument or forwards an internal call's result.
    /// Such wrappers are safe to inline even when the value is memory-backed.
    is_transparent_forwarder: bool,
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
        self.expected_executions_per_deployment = gcx.sess.opts.optimizer_runs.unwrap_or(200);

        // A zero budget is an explicit off switch (used by `-O size`). Avoid
        // summarizing the module or building its call graph when no call site
        // can be accepted.
        if self.max_module_code_size == 0 {
            return stats;
        }

        let mut summaries = self.summarize_module(gcx, module);

        // Track the estimator for explicit hard ceilings. Gas mode leaves the
        // ceiling unlimited and decides from lifetime execution/deposit cost.
        let mut module_code_size: usize = summaries.values().map(|s| s.estimated_code_size).sum();
        if module_code_size >= self.max_module_code_size {
            return stats;
        }

        let mut call_counts = self.call_counts(module);
        let call_graph = CallGraphInfo::new(module);
        let preferred_large_call_sites = self.preferred_large_call_sites(module, &summaries);

        // Specialize dispatcher calls before helper-local inlining introduces phis.
        let mut caller_ids = module.functions.indices().collect::<Vec<_>>();
        caller_ids.sort_by_key(|caller| {
            summaries.get(caller).is_some_and(|summary| {
                summary.is_function_pointer_dispatcher && summary.has_function_selector
            })
        });
        for caller_id in caller_ids {
            let loop_depths = block_loop_depths(module.function(caller_id));
            // Bound how much each caller may grow from inlining so a function
            // calling many internal helpers (e.g. a large verifier) cannot
            // balloon past the deployable code-size limit.
            let base_instructions =
                summaries.get(&caller_id).map(|s| s.instruction_count).unwrap_or_default();
            let mut cursor = (0, 0);
            while let Some(site) =
                self.find_next_call(module.function(caller_id), cursor, &loop_depths)
            {
                stats.call_sites += 1;
                cursor = (site.block.index(), site.inst_index + 1);

                let Some(summary) = summaries.get(&site.callee).copied() else {
                    stats.skipped += 1;
                    continue;
                };
                let call_count = call_counts.get(&site.callee).copied().unwrap_or_default();
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

                let callee = module.function(site.callee).clone();
                let old_size =
                    summaries.get(&caller_id).map(|s| s.estimated_code_size).unwrap_or_default();
                let caller = module.function_mut(caller_id);
                if inline_call(caller, site.block, site.inst_index, &callee) {
                    stats.inlined += 1;
                    let new_summary = summarize_function(gcx, module, module.function(caller_id));
                    module_code_size = module_code_size
                        .saturating_sub(old_size)
                        .saturating_add(new_summary.estimated_code_size);
                    summaries.insert(caller_id, new_summary);
                    if self.mode == InlineMode::TinyLeaves {
                        // Tiny-leaf candidates cannot contain internal calls, so inlining removes
                        // exactly one call to the callee and cannot introduce another call site.
                        if let Some(count) = call_counts.get_mut(&site.callee) {
                            *count = count.saturating_sub(1);
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
            .map(|(id, func)| (id, summarize_function(gcx, module, func)))
            .collect()
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
        loop_depths: &FxHashMap<BlockId, usize>,
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
                        loop_depth: loop_depths.get(&block).copied().unwrap_or_default(),
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
            || summary.has_phi
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
                || (summary.has_reference_return && !summary.is_transparent_forwarder)
                || (summary.has_icall && !summary.is_transparent_forwarder)
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
                > estimated_icall_code_size(site)
                    + estimated_internal_return_code_size(summary, site)
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
        const CODE_DEPOSIT_GAS_PER_BYTE: u128 = 200;

        let inlined_bytes = summary.estimated_code_size;
        let mut removed_bytes = estimated_icall_code_size(site);
        if single_call {
            removed_bytes = removed_bytes.saturating_add(
                summary.estimated_code_size + estimated_internal_return_code_size(summary, site),
            );
        }
        if inlined_bytes <= removed_bytes {
            return true;
        }

        let added_deposit_cost =
            (inlined_bytes - removed_bytes) as u128 * CODE_DEPOSIT_GAS_PER_BYTE;
        let execution_savings = u128::from(estimated_icall_savings(site, summary))
            * u128::from(self.expected_executions_per_deployment);
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
    has_constant_function_selector: bool,
    has_constant_argument: bool,
}

fn summarize_function(gcx: Gcx<'_>, module: &Module, func: &Function) -> MirInlineSummary {
    let mut summary = MirInlineSummary {
        block_count: func.blocks.len(),
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
        is_function_pointer_dispatcher: func.attributes.is_function_pointer_dispatcher,
        has_function_selector: func.params.first() == Some(&MirType::Function),
        is_pure: func.attributes.state_mutability == StateMutability::Pure,
        ..MirInlineSummary::default()
    };

    for block in func.blocks.iter() {
        for &inst_id in &block.instructions {
            let kind = &func.inst(inst_id).kind;
            let (inst_cost, instructions) = estimate_inst_cost(gcx, module, kind);
            summary.instruction_count += instructions;
            summary.estimated_code_size += inst_cost.code_size;
            summary.estimated_runtime_gas += inst_cost.runtime_gas;
            match kind {
                InstKind::ICall { .. } => summary.has_icall = true,
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
                let term_cost = estimate_terminator_cost(term);
                summary.estimated_code_size += term_cost.code_size;
                summary.estimated_runtime_gas += term_cost.runtime_gas;
            }
            Some(term @ Terminator::Revert { .. }) => {
                let term_cost = estimate_terminator_cost(term);
                summary.estimated_code_size += term_cost.code_size;
                summary.estimated_runtime_gas += term_cost.runtime_gas;
            }
            Some(term @ Terminator::RevertReturndata) => {
                let term_cost = estimate_terminator_cost(term);
                summary.estimated_code_size += term_cost.code_size;
                summary.estimated_runtime_gas += term_cost.runtime_gas;
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
                let term_cost = estimate_terminator_cost(block.terminator.as_ref().unwrap());
                summary.estimated_code_size += term_cost.code_size;
                summary.estimated_runtime_gas += term_cost.runtime_gas;
            }
            Some(Terminator::ReturnData { .. })
            | Some(Terminator::Stop)
            | Some(Terminator::SelfDestruct { .. })
            | Some(Terminator::TailCall { .. })
            | None => summary.has_unsupported_terminator = true,
            Some(Terminator::Invalid) => {}
        }
    }

    summary
}

fn is_transparent_forwarder(func: &Function) -> bool {
    if func.attributes.no_inline
        || func.selector.is_some()
        || func.attributes.is_constructor
        || func.attributes.is_fallback
        || func.attributes.is_receive
        || func.blocks.len() != 1
        || func.internal_frame_size != 0
        || func.returns.len() != 1
    {
        return false;
    }

    if is_identity_function(func) {
        return true;
    }

    let [call] = func.blocks[BlockId::ENTRY].instructions.as_slice() else { return false };
    let InstKind::ICall { returns: 1, .. } = func.inst(*call).kind else { return false };
    let Some(result) = func.inst_result_value(*call) else { return false };
    matches!(
        func.blocks[BlockId::ENTRY].terminator.as_ref(),
        Some(Terminator::Return { values }) if values.as_slice() == [result]
    )
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

#[derive(Clone, Copy, Debug)]
struct MirCost {
    runtime_gas: u64,
    code_size: usize,
}

fn estimate_inst_cost(gcx: Gcx<'_>, module: &Module, kind: &InstKind) -> (MirCost, usize) {
    let (runtime_gas, code_size) = match kind {
        InstKind::MakeSlice { .. } | InstKind::SlicePtr(_) | InstKind::SliceLen(_) => (0, 0),
        InstKind::MemoryObjectData(_, kind) => {
            if EvmMemoryLayout::object_data_offset(*kind) == 0 {
                (0, 0)
            } else {
                (3, 1)
            }
        }
        InstKind::MemoryObjectFieldAddr { layout, field, .. } => {
            if EvmMemoryLayout::field_offset(*layout, *field) == Some(0) { (0, 0) } else { (3, 1) }
        }
        InstKind::MemoryObjectElementAddr { layout, .. } => {
            let base_cost = u64::from(EvmMemoryLayout::object_data_offset(layout.kind()) != 0);
            (8 + base_cost * 3, 2 + base_cost as usize)
        }
        InstKind::MemoryObjectLoadField { layout, field, .. }
        | InstKind::MemoryObjectStoreField { layout, field, .. } => {
            if EvmMemoryLayout::field_offset(*layout, *field) == Some(0) { (3, 1) } else { (6, 2) }
        }
        InstKind::MemoryObjectLoadElement { layout, .. }
        | InstKind::MemoryObjectStoreElement { layout, .. } => {
            let base_cost = u64::from(EvmMemoryLayout::object_data_offset(layout.kind()) != 0);
            (11 + base_cost * 3, 3 + base_cost as usize)
        }
        InstKind::MemoryObjectLoadByte { .. } => (8, 2),
        InstKind::MemoryObjectStoreByte { .. } => (8, 2),
        InstKind::MemoryObjectStoreWord { .. } => (8, 2),
        InstKind::MemorySliceLoadWord { .. } => (6, 1),
        InstKind::CalldataSliceLoadWord { .. } => (6, 1),
        InstKind::MemoryObjectCopyFromSlice { .. } => (12, 1),
        InstKind::MemoryObjectCopyFromSliceAt { .. } => (12, 1),
        InstKind::MemoryObjectCopy { .. } => (12, 1),
        InstKind::MemoryObjectLen(_, _) | InstKind::SetMemoryObjectLen(_, _, _) => (3, 1),
        InstKind::Fmp | InstKind::SetFmp(_) => (3, 1),
        InstKind::Alloc { .. } => (9, 3),
        InstKind::AbiEncode { args, layout, .. } => {
            let words = layout.head_size() / 32;
            (30 + words * 12, 8 + args.len() * 3 + abi_layout_expansion(layout))
        }
        InstKind::AbiDecode { layout, .. } => {
            let words = layout.checked_head_size().expect("ABI head size exceeds u64 range") / 32;
            (30 + words * 12, 8 + layout.types.len() * 3)
        }
        InstKind::StorageToMemory { layout, .. } => {
            let slots = layout.storage_slots();
            (slots * 103, slots as usize * 2)
        }
        InstKind::MemoryToStorage { layout, .. } | InstKind::ClearStorage { layout, .. } => {
            let slots = layout.storage_slots();
            (slots * 5_000, slots as usize * 2)
        }
        InstKind::Add(..)
        | InstKind::Sub(..)
        | InstKind::Lt(..)
        | InstKind::Gt(..)
        | InstKind::SLt(..)
        | InstKind::SGt(..)
        | InstKind::Eq(..)
        | InstKind::IsZero(..)
        | InstKind::And(..)
        | InstKind::Or(..)
        | InstKind::Xor(..)
        | InstKind::Not(..)
        | InstKind::Byte(..)
        | InstKind::Shl(..)
        | InstKind::Shr(..)
        | InstKind::Sar(..)
        | InstKind::SignExtend(..)
        | InstKind::MLoad(..)
        | InstKind::FrameLoad { .. }
        | InstKind::MStore(..)
        | InstKind::FrameStore { .. }
        | InstKind::MStore8(..)
        | InstKind::CalldataLoad(..)
        | InstKind::CalldataSize
        | InstKind::Caller
        | InstKind::CallValue
        | InstKind::Origin
        | InstKind::GasPrice
        | InstKind::Coinbase
        | InstKind::Timestamp
        | InstKind::BlockNumber
        | InstKind::PrevRandao
        | InstKind::GasLimit
        | InstKind::SlotNum
        | InstKind::ChainId
        | InstKind::Address
        | InstKind::SelfBalance
        | InstKind::Gas
        | InstKind::BaseFee
        | InstKind::BlobBaseFee => (3, 1),
        InstKind::Clz(..)
        | InstKind::Mul(..)
        | InstKind::Div(..)
        | InstKind::SDiv(..)
        | InstKind::Mod(..)
        | InstKind::SMod(..) => (5, 1),
        InstKind::Exp(..) => (50, 1),
        InstKind::AddMod(..) | InstKind::MulMod(..) => (8, 1),
        InstKind::SLoad(..) | InstKind::TLoad(..) => (100, 1),
        InstKind::SStore(..) | InstKind::TStore(..) => (5_000, 1),
        InstKind::StoreImmutable(..) => (6, 4),
        InstKind::DataCopy(..)
        | InstKind::MCopy(..)
        | InstKind::CalldataCopy(..)
        | InstKind::CodeCopy(..)
        | InstKind::ExtCodeCopy(..)
        | InstKind::ReturnDataCopy(..) => (12, 1),
        InstKind::MemoryZero(..) => (15, 2),
        InstKind::MSize | InstKind::CodeSize | InstKind::ReturnDataSize => (2, 1),
        InstKind::ConstructorArgsBase => (3, 3),
        InstKind::ConstructorArgsEnd => (9, 8),
        InstKind::InternalFrameAddr(_) => (6, 3),
        // Typed PUSH<N> placeholder patched at deploy time.
        InstKind::LoadImmutable(id) => {
            let ty = module.immutable_type(*id);
            let encoding = ty.immutable_encoding().expect("validated immutable declaration");
            let type_size = immutable_push_type_size(
                encoding,
                gcx.sess.opts.optimization,
                gcx.sess.opts.evm_version.has_bitwise_shifting(),
            );
            let width = usize::from(type_size.bytes());
            if width == 32 {
                (3, 33)
            } else {
                match encoding {
                    ImmutableEncoding::Unsigned(_) => (3, width + 1),
                    ImmutableEncoding::Signed(_) => (11, width + 4),
                    ImmutableEncoding::LeftAligned(_) => (9, width + 4),
                }
            }
        }
        InstKind::ExtCodeSize(..)
        | InstKind::ExtCodeHash(..)
        | InstKind::Balance(..)
        | InstKind::BlockHash(..)
        | InstKind::BlobHash(..)
        | InstKind::Keccak256(..) => (30, 1),
        // Expands to length load + data pointer + physical keccak.
        InstKind::Keccak256Bytes(_) => (36, 5),
        InstKind::MappingSlot(..) => (36, 3),
        InstKind::MappingSlotMemory(..) => (60, 8),
        InstKind::MappingSlotCalldata(..) => (63, 9),
        InstKind::StorageArrayDataSlot(..) => (36, 3),
        InstKind::StorageArrayElementSlot { element_slots, .. } => {
            (36 + u64::from(*element_slots > 1) * 5, 4)
        }
        InstKind::Call { .. }
        | InstKind::CallCode { .. }
        | InstKind::StaticCall { .. }
        | InstKind::DelegateCall { .. }
        | InstKind::ExtCall { .. }
        | InstKind::ExtDelegateCall { .. }
        | InstKind::ExtStaticCall { .. } => (700, 1),
        InstKind::ICall { args, returns, .. } => {
            let returns = *returns as usize;
            (80 + ((args.len() + returns) as u64) * 20, 16 + (args.len() + returns) * 4)
        }
        InstKind::Create(..) | InstKind::Create2(..) => (32_000, 1),
        InstKind::Log0(..) => (375, 1),
        InstKind::Log1(..) => (750, 1),
        InstKind::Log2(..) => (1_125, 1),
        InstKind::Log3(..) => (1_500, 1),
        InstKind::Log4(..) => (1_875, 1),
        InstKind::Phi(_) | InstKind::Select(..) => (3, 1),
    };
    let instructions = match kind {
        InstKind::MappingSlot(..) | InstKind::StorageArrayDataSlot(..) => 3,
        InstKind::MappingSlotMemory(..) => 8,
        InstKind::MappingSlotCalldata(..) => 9,
        InstKind::StorageArrayElementSlot { .. } => 4,
        InstKind::AbiEncode { layout, .. } => abi_layout_expansion(layout),
        _ => 1,
    };
    (MirCost { runtime_gas, code_size }, instructions)
}

fn estimate_terminator_cost(term: &Terminator) -> MirCost {
    let (runtime_gas, code_size) = match term {
        Terminator::Jump(_) => (8, 3),
        Terminator::Branch { .. } => (13, 4),
        Terminator::Switch { cases, .. } => (13 + (cases.len() as u64) * 10, 4 + cases.len() * 4),
        Terminator::Return { values } => (20 + (values.len() as u64) * 12, 8),
        Terminator::Revert { .. }
        | Terminator::RevertReturndata
        | Terminator::ReturnData { .. } => (20, 4),
        Terminator::Stop => (0, 1),
        Terminator::SelfDestruct { .. } => (5_000, 1),
        Terminator::TailCall { args, .. } => (8 + 3 * args.len() as u64, 4 + args.len()),
        Terminator::Invalid => (0, 1),
    };
    MirCost { runtime_gas, code_size }
}

fn estimated_icall_savings(site: CallSite, summary: MirInlineSummary) -> u64 {
    let frame_words = (summary.internal_frame_size / EvmMemoryLayout::WORD_SIZE)
        + (EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE / EvmMemoryLayout::WORD_SIZE)
        + (site.args_len + site.returns) as u64;
    let protocol = 90 + ((site.args_len + site.returns) as u64) * 24 + frame_words * 6;
    let return_protocol = 24 + (summary.param_count as u64 + site.returns as u64) * 8;
    let loop_multiplier = (site.loop_depth as u64).saturating_add(1);
    (protocol + return_protocol) * loop_multiplier
}

fn estimated_icall_code_size(site: CallSite) -> usize {
    18 + (site.args_len + site.returns) * 5
}

fn estimated_internal_return_code_size(summary: MirInlineSummary, site: CallSite) -> usize {
    8 + (summary.param_count + site.returns) * 4
}

fn block_loop_depths(func: &Function) -> FxHashMap<BlockId, usize> {
    let mut analyzer = LoopAnalyzer::new();
    let loop_info = analyzer.analyze(func);
    let mut depths = FxHashMap::default();
    for loop_data in loop_info.all_loops() {
        for block in &loop_data.blocks {
            *depths.entry(block).or_default() += 1;
        }
    }
    depths
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

struct InlineCloner<'a> {
    caller: &'a mut Function,
    callee: &'a Function,
    frame_base: u64,
    callee_frame_prefix: u64,
    args: Box<[ValueId]>,
    value_map: FxHashMap<ValueId, ValueId>,
    block_map: IndexVec<BlockId, BlockId>,
    return_edges: Vec<(BlockId, SmallVec<[ValueId; 2]>)>,
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
    caller.blocks[continuation].instructions.splice(phi_count..phi_count, instructions);
    Some(())
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
