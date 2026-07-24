//! Function inlining optimization pass.
//!
//! This module inlines profitable MIR internal calls to remove their call
//! protocol and expose further optimization opportunities.

use crate::{
    analysis::LoopAnalyzer,
    memory::{EvmMemoryLayout, MemoryLayoutPolicy},
    mir::{
        BlockId, Function, FunctionId as MirFunctionId, Immediate, InstKind, Instruction, MirType,
        Module, Terminator, Value, ValueId,
    },
    pass::MirPass,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
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
        inliner.run(module).inlined != 0
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
    /// Maximum number of blocks to clone from one callee.
    max_blocks: usize,
    /// Whether a single call site may use the larger threshold.
    inline_single_call: bool,
    /// Maximum estimated runtime bytecode growth for a cold call site.
    max_cold_code_growth: usize,
    /// Maximum estimated runtime bytecode growth for a call site inside a loop.
    max_hot_code_growth: usize,
    /// Maximum number of instructions a single caller may gain from inlining
    /// multi-use callees, bounding total code growth per function.
    max_caller_inlined_instructions: usize,
    /// Minimum estimated internal-call protocol gas saved before inlining.
    min_call_savings: u64,
    /// Size-aware backstop: once a module's estimated runtime bytecode reaches
    /// this many bytes, stop inlining (which grows code) so the contract stays
    /// under the EIP-170 deployable-code limit. Small contracts never reach it
    /// and inline normally.
    max_module_code_size: usize,
}

impl Default for MirInliner {
    fn default() -> Self {
        Self {
            max_instructions: 96,
            max_single_call_sanity_instructions: 4096,
            max_blocks: 10,
            inline_single_call: true,
            max_cold_code_growth: 256,
            max_hot_code_growth: 512,
            max_caller_inlined_instructions: 64,
            min_call_savings: 120,
            // Budget in `estimated_code_size` units (a per-instruction proxy that
            // runs well below final bytecode because it does not model stack
            // scheduling/spills). Calibrated as a conservative backstop: a module
            // already this large has little headroom under the EIP-170 24576-byte
            // limit, so further (growth-only) inlining is skipped to keep it
            // deployable. Ordinary contracts are far smaller and inline normally.
            max_module_code_size: 7450,
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
    has_internal_call: bool,
    has_phi: bool,
    has_external_call: bool,
    has_storage_write: bool,
    has_log: bool,
    has_control_flow: bool,
    has_unsupported_terminator: bool,
    is_entry_point: bool,
    is_constructor: bool,
    no_inline: bool,
}

impl MirInliner {
    /// Runs the inliner over the whole module.
    fn run(&mut self, module: &mut Module) -> MirInlineStats {
        let mut stats = MirInlineStats::default();

        // A zero budget is an explicit off switch (used by `-O size`). Avoid
        // summarizing the module or building its call graph when no call site
        // can be accepted.
        if self.max_module_code_size == 0 {
            return stats;
        }

        let mut summaries = self.summarize_module(module);

        // Size-aware backstop: inlining grows emitted code, so track the module's
        // estimated runtime bytecode and stop inlining once it reaches the budget,
        // keeping large contracts under the EIP-170 deployable-code limit. Small
        // contracts never reach the budget and inline normally.
        let mut module_code_size: usize = summaries.values().map(|s| s.estimated_code_size).sum();
        if module_code_size >= self.max_module_code_size {
            return stats;
        }

        let mut call_counts = self.call_counts(module);
        let recursive_functions = self.recursive_functions(module);

        for caller_id in module.functions.indices().collect::<Vec<_>>() {
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
                if module_code_size >= self.max_module_code_size
                    || grew_too_much
                    || recursive_functions.contains(site.callee)
                    || !self.is_inlineable(caller_id, site, summary, call_count)
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
                    let new_summary = summarize_function(module.function(caller_id));
                    module_code_size = module_code_size
                        .saturating_sub(old_size)
                        .saturating_add(new_summary.estimated_code_size);
                    summaries.insert(caller_id, new_summary);
                    call_counts = self.call_counts(module);
                    cursor = (site.block.index(), 0);
                } else {
                    stats.skipped += 1;
                }
            }
        }

        stats
    }

    fn summarize_module(&self, module: &Module) -> FxHashMap<MirFunctionId, MirInlineSummary> {
        module
            .functions
            .iter_enumerated()
            .map(|(id, func)| (id, summarize_function(func)))
            .collect()
    }

    fn call_counts(&self, module: &Module) -> FxHashMap<MirFunctionId, usize> {
        let mut counts = FxHashMap::default();
        for func in module.functions.iter() {
            for block in func.blocks.iter() {
                for &inst_id in &block.instructions {
                    if let InstKind::InternalCall { function, .. } = func.instructions[inst_id].kind
                    {
                        *counts.entry(function).or_default() += 1;
                    }
                }
            }
        }
        counts
    }

    fn recursive_functions(&self, module: &Module) -> DenseBitSet<MirFunctionId> {
        let mut recursive = DenseBitSet::new_empty(module.functions.len());
        let mut visiting = DenseBitSet::new_empty(module.functions.len());
        for func_id in module.functions.indices() {
            visiting.clear();
            if self.function_reaches(module, func_id, func_id, &mut visiting) {
                recursive.insert(func_id);
            }
        }
        recursive
    }

    fn function_reaches(
        &self,
        module: &Module,
        current: MirFunctionId,
        target: MirFunctionId,
        visiting: &mut DenseBitSet<MirFunctionId>,
    ) -> bool {
        if !visiting.insert(current) {
            return false;
        }

        for callee in self.function_callees(module.function(current)) {
            if callee == target || self.function_reaches(module, callee, target, visiting) {
                return true;
            }
        }

        false
    }

    fn function_callees(&self, func: &Function) -> Vec<MirFunctionId> {
        let mut callees = Vec::new();
        for block in func.blocks.iter() {
            for &inst_id in &block.instructions {
                if let InstKind::InternalCall { function, .. } = func.instructions[inst_id].kind {
                    callees.push(function);
                }
            }
        }
        callees
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
                if let InstKind::InternalCall { function, ref args, returns } =
                    func.instructions[inst_id].kind
                {
                    return Some(CallSite {
                        block,
                        inst_index,
                        callee: function,
                        args_len: args.len(),
                        returns: returns as usize,
                        loop_depth: loop_depths.get(&block).copied().unwrap_or_default(),
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
    ) -> bool {
        let single_call = self.inline_single_call && call_count == 1;

        // `no_inline` prevents cloning a shared helper into every caller; with
        // a single call site there is nothing to duplicate, and absorbing the
        // helper removes the call protocol around its only use.
        if caller == site.callee
            || (summary.no_inline && !single_call)
            || summary.is_entry_point
            || summary.is_constructor
            || summary.has_phi
            || summary.has_unsupported_terminator
            || summary.return_count == 0
        {
            return false;
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
            && (summary.has_storage_write || summary.has_external_call || summary.has_log)
            && summary.estimated_code_size
                > estimated_internal_call_code_size(site)
                    + estimated_internal_return_code_size(summary, site)
        {
            return false;
        }

        let code_growth = estimated_inline_code_growth(summary, site, single_call);
        let max_growth =
            if site.loop_depth > 0 { self.max_hot_code_growth } else { self.max_cold_code_growth };
        if code_growth > max_growth {
            return false;
        }

        let savings = estimated_internal_call_savings(site, summary);
        savings >= self.min_call_savings
    }
}

#[derive(Clone, Copy)]
struct CallSite {
    block: BlockId,
    inst_index: usize,
    callee: MirFunctionId,
    args_len: usize,
    returns: usize,
    loop_depth: usize,
}

fn summarize_function(func: &Function) -> MirInlineSummary {
    let mut summary = MirInlineSummary {
        block_count: func.blocks.len(),
        param_count: func.params.len(),
        internal_frame_size: func.internal_frame_size,
        is_entry_point: func.is_public()
            || func.attributes.is_fallback
            || func.attributes.is_receive
            || func.selector.is_some(),
        is_constructor: func.attributes.is_constructor,
        no_inline: func.attributes.no_inline,
        ..MirInlineSummary::default()
    };

    for block in func.blocks.iter() {
        for &inst_id in &block.instructions {
            let kind = &func.instructions[inst_id].kind;
            summary.instruction_count += match kind {
                InstKind::MappingSlot(..) => 3,
                InstKind::MappingSlotMemory(..) => 8,
                InstKind::MappingSlotCalldata(..) => 9,
                _ => 1,
            };
            let inst_cost = estimate_inst_cost(kind);
            summary.estimated_code_size += inst_cost.code_size;
            summary.estimated_runtime_gas += inst_cost.runtime_gas;
            match kind {
                InstKind::InternalCall { .. } => summary.has_internal_call = true,
                InstKind::Phi(_) => summary.has_phi = true,
                InstKind::Call { .. }
                | InstKind::StaticCall { .. }
                | InstKind::DelegateCall { .. }
                | InstKind::Create(..)
                | InstKind::Create2(..) => {
                    summary.has_external_call = true;
                }
                InstKind::SStore(..) | InstKind::TStore(..) => summary.has_storage_write = true,
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

#[derive(Clone, Copy, Debug, Default)]
struct MirCost {
    runtime_gas: u64,
    code_size: usize,
}

fn estimate_inst_cost(kind: &InstKind) -> MirCost {
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
        InstKind::MemoryObjectLen(_, _) | InstKind::SetMemoryObjectLen(_, _, _) => (3, 1),
        InstKind::Fmp | InstKind::SetFmp(_) => (3, 1),
        InstKind::Alloc { .. } => (9, 3),
        InstKind::AbiEncode { args, layout, .. } => {
            let words = layout.head_size() / 32;
            (30 + words * 12, 8 + args.len() * 3)
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
        | InstKind::MStore(..)
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
        | InstKind::ChainId
        | InstKind::Address
        | InstKind::SelfBalance
        | InstKind::Gas
        | InstKind::BaseFee
        | InstKind::BlobBaseFee => (3, 1),
        InstKind::Mul(..)
        | InstKind::Div(..)
        | InstKind::SDiv(..)
        | InstKind::Mod(..)
        | InstKind::SMod(..) => (5, 1),
        InstKind::Exp(..) => (50, 1),
        InstKind::AddMod(..) | InstKind::MulMod(..) => (8, 1),
        InstKind::SLoad(..) | InstKind::TLoad(..) => (100, 1),
        InstKind::SStore(..) | InstKind::TStore(..) => (5_000, 1),
        InstKind::MCopy(..)
        | InstKind::CalldataCopy(..)
        | InstKind::CodeCopy(..)
        | InstKind::ExtCodeCopy(..)
        | InstKind::ReturnDataCopy(..) => (12, 1),
        InstKind::MSize | InstKind::CodeSize | InstKind::ReturnDataSize => (2, 1),
        InstKind::InternalFrameAddr(_) => (6, 3),
        // PUSH32 placeholder patched at deploy time.
        InstKind::LoadImmutable(_) => (3, 33),
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
        InstKind::Call { .. } | InstKind::StaticCall { .. } | InstKind::DelegateCall { .. } => {
            (700, 1)
        }
        InstKind::InternalCall { args, returns, .. } => {
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
    MirCost { runtime_gas, code_size }
}

fn estimate_terminator_cost(term: &Terminator) -> MirCost {
    let (runtime_gas, code_size) = match term {
        Terminator::Jump(_) => (8, 3),
        Terminator::Branch { .. } => (13, 4),
        Terminator::Switch { cases, .. } => (13 + (cases.len() as u64) * 10, 4 + cases.len() * 4),
        Terminator::Return { values } => (20 + (values.len() as u64) * 12, 8),
        Terminator::Revert { .. } | Terminator::ReturnData { .. } => (20, 4),
        Terminator::Stop => (0, 1),
        Terminator::SelfDestruct { .. } => (5_000, 1),
        Terminator::TailCall { args, .. } => (8 + 3 * args.len() as u64, 4 + args.len()),
        Terminator::Invalid => (0, 1),
    };
    MirCost { runtime_gas, code_size }
}

fn estimated_internal_call_savings(site: CallSite, summary: MirInlineSummary) -> u64 {
    let frame_words = (summary.internal_frame_size / EvmMemoryLayout::WORD_SIZE)
        + (EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE / EvmMemoryLayout::WORD_SIZE)
        + (site.args_len + site.returns) as u64;
    let protocol = 90 + ((site.args_len + site.returns) as u64) * 24 + frame_words * 6;
    let return_protocol = 24 + (summary.param_count as u64 + site.returns as u64) * 8;
    let loop_multiplier = (site.loop_depth as u64).saturating_add(1);
    (protocol + return_protocol) * loop_multiplier
}

fn estimated_internal_call_code_size(site: CallSite) -> usize {
    18 + (site.args_len + site.returns) * 5
}

fn estimated_internal_return_code_size(summary: MirInlineSummary, site: CallSite) -> usize {
    8 + (summary.param_count + site.returns) * 4
}

fn estimated_inline_code_growth(
    summary: MirInlineSummary,
    site: CallSite,
    single_call: bool,
) -> usize {
    let removed_call = estimated_internal_call_code_size(site);
    if single_call {
        let removed_callee =
            summary.estimated_code_size + estimated_internal_return_code_size(summary, site);
        summary.estimated_code_size.saturating_sub(removed_call + removed_callee)
    } else {
        summary.estimated_code_size.saturating_sub(removed_call)
    }
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
    let InstKind::InternalCall { args, returns, .. } = caller.instructions[call_inst].kind.clone()
    else {
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
    let old_terminator = caller.blocks[call_block].terminator.take();
    let old_successors = old_terminator.as_ref().map(Terminator::successors).unwrap_or_default();
    let suffix = {
        let block = &mut caller.blocks[call_block];
        block.instructions.split_off(call_inst_index + 1)
    };
    caller.blocks[call_block].instructions.pop();
    caller.blocks[continuation].instructions = suffix;
    caller.blocks[continuation].terminator = old_terminator;
    redirect_phi_predecessors(caller, &old_successors, call_block, continuation);

    let caller_is_external =
        caller.selector.is_some() || caller.attributes.is_receive || caller.attributes.is_fallback;
    let caller_frame_prefix = if caller_is_external {
        0
    } else {
        EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
            + ((caller.params.len() + caller.returns.len()) as u64) * EvmMemoryLayout::WORD_SIZE
    };
    let frame_base = caller_frame_prefix + caller.internal_frame_size;
    let callee_frame_prefix = EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
        + ((callee.params.len() + callee.returns.len()) as u64) * EvmMemoryLayout::WORD_SIZE;
    caller.internal_frame_size += callee.internal_frame_size;

    let mut cloner = InlineCloner::new(caller, callee, frame_base, callee_frame_prefix, &args);
    let cloned_entry = cloner.clone_blocks(continuation)?;
    cloner.caller.blocks[call_block].terminator = Some(Terminator::Jump(cloned_entry));

    let mut replacements = FxHashMap::default();
    if returns > 0 {
        let return_values = build_return_values(
            cloner.caller,
            continuation,
            &callee.returns,
            &cloner.return_edges,
        )?;
        replacements.insert(call_result?, return_values[0]);
        insert_extra_return_stores(cloner.caller, continuation, &return_values[1..]);
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
    value_map: FxHashMap<ValueId, ValueId>,
    block_map: FxHashMap<BlockId, BlockId>,
    return_edges: Vec<(BlockId, SmallVec<[ValueId; 2]>)>,
}

impl<'a> InlineCloner<'a> {
    fn new(
        caller: &'a mut Function,
        callee: &'a Function,
        frame_base: u64,
        callee_frame_prefix: u64,
        args: &[ValueId],
    ) -> Self {
        let mut value_map = FxHashMap::default();
        for (callee_value, value) in callee.values.iter_enumerated() {
            if let Value::Arg { index, .. } = value
                && let Some(&arg) = args.get(*index as usize)
            {
                value_map.insert(callee_value, arg);
            }
        }
        Self {
            caller,
            callee,
            frame_base,
            callee_frame_prefix,
            value_map,
            block_map: FxHashMap::default(),
            return_edges: Vec::new(),
        }
    }

    fn clone_blocks(&mut self, continuation: BlockId) -> Option<BlockId> {
        for block_id in self.callee.blocks.indices() {
            self.block_map.insert(block_id, self.caller.alloc_block());
        }

        for (callee_block, block) in self.callee.blocks.iter_enumerated() {
            let caller_block = self.block_map[&callee_block];
            let mut instructions = Vec::with_capacity(block.instructions.len());
            for &inst_id in &block.instructions {
                let inst = self.callee.instructions[inst_id].clone();
                let kind = self.clone_inst_kind(inst.kind)?;
                let new_inst = self.caller.alloc_inst(Instruction::new(kind, inst.result_ty));
                instructions.push(new_inst);
                if let Some(callee_result) = self.callee.inst_result_value(inst_id) {
                    let new_result = self.caller.alloc_value(Value::Inst(new_inst));
                    self.value_map.insert(callee_result, new_result);
                }
            }
            self.caller.blocks[caller_block].instructions = instructions;
        }

        for (callee_block, block) in self.callee.blocks.iter_enumerated() {
            let caller_block = self.block_map[&callee_block];
            let term =
                self.clone_terminator(block.terminator.as_ref()?, caller_block, continuation)?;
            self.caller.blocks[caller_block].terminator = Some(term);
        }

        Some(self.block_map[&BlockId::ENTRY])
    }

    fn clone_value(&mut self, value: ValueId) -> Option<ValueId> {
        if let Some(&mapped) = self.value_map.get(&value) {
            return Some(mapped);
        }

        let cloned = match self.callee.values[value].clone() {
            Value::Immediate(imm) => self.caller.alloc_value(Value::Immediate(imm)),
            Value::Undef(ty) => self.caller.alloc_value(Value::Undef(ty)),
            Value::Error(guar) => self.caller.alloc_value(Value::Error(guar)),
            Value::Arg { .. } | Value::Inst(_) => return None,
        };
        self.value_map.insert(value, cloned);
        Some(cloned)
    }

    fn clone_block(&self, block: BlockId) -> Option<BlockId> {
        self.block_map.get(&block).copied()
    }

    #[allow(clippy::too_many_lines)]
    fn clone_inst_kind(&mut self, kind: InstKind) -> Option<InstKind> {
        Some(match kind {
            InstKind::MakeSlice { ptr, len, location } => InstKind::MakeSlice {
                ptr: self.clone_value(ptr)?,
                len: self.clone_value(len)?,
                location,
            },
            InstKind::SlicePtr(slice) => InstKind::SlicePtr(self.clone_value(slice)?),
            InstKind::SliceLen(slice) => InstKind::SliceLen(self.clone_value(slice)?),
            InstKind::MemoryObjectLen(object, kind) => {
                InstKind::MemoryObjectLen(self.clone_value(object)?, kind)
            }
            InstKind::SetMemoryObjectLen(object, len, kind) => InstKind::SetMemoryObjectLen(
                self.clone_value(object)?,
                self.clone_value(len)?,
                kind,
            ),
            InstKind::MemoryObjectData(object, kind) => {
                InstKind::MemoryObjectData(self.clone_value(object)?, kind)
            }
            InstKind::MemoryObjectFieldAddr { object, layout, field } => {
                InstKind::MemoryObjectFieldAddr { object: self.clone_value(object)?, layout, field }
            }
            InstKind::MemoryObjectElementAddr { object, layout, index } => {
                InstKind::MemoryObjectElementAddr {
                    object: self.clone_value(object)?,
                    layout,
                    index: self.clone_value(index)?,
                }
            }
            InstKind::StorageToMemory { storage, memory, layout } => InstKind::StorageToMemory {
                storage: self.clone_value(storage)?,
                memory: self.clone_value(memory)?,
                layout,
            },
            InstKind::MemoryToStorage { memory, storage, layout } => InstKind::MemoryToStorage {
                memory: self.clone_value(memory)?,
                storage: self.clone_value(storage)?,
                layout,
            },
            InstKind::ClearStorage { storage, layout } => {
                InstKind::ClearStorage { storage: self.clone_value(storage)?, layout }
            }
            InstKind::Add(a, b) => InstKind::Add(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Sub(a, b) => InstKind::Sub(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Mul(a, b) => InstKind::Mul(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Div(a, b) => InstKind::Div(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::SDiv(a, b) => InstKind::SDiv(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Mod(a, b) => InstKind::Mod(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::SMod(a, b) => InstKind::SMod(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Exp(a, b) => InstKind::Exp(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::AddMod(a, b, c) => {
                InstKind::AddMod(self.clone_value(a)?, self.clone_value(b)?, self.clone_value(c)?)
            }
            InstKind::MulMod(a, b, c) => {
                InstKind::MulMod(self.clone_value(a)?, self.clone_value(b)?, self.clone_value(c)?)
            }
            InstKind::And(a, b) => InstKind::And(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Or(a, b) => InstKind::Or(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Xor(a, b) => InstKind::Xor(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Not(a) => InstKind::Not(self.clone_value(a)?),
            InstKind::Shl(a, b) => InstKind::Shl(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Shr(a, b) => InstKind::Shr(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Sar(a, b) => InstKind::Sar(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Byte(a, b) => InstKind::Byte(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Lt(a, b) => InstKind::Lt(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Gt(a, b) => InstKind::Gt(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::SLt(a, b) => InstKind::SLt(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::SGt(a, b) => InstKind::SGt(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Eq(a, b) => InstKind::Eq(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::IsZero(a) => InstKind::IsZero(self.clone_value(a)?),
            InstKind::MLoad(a) => InstKind::MLoad(self.clone_value(a)?),
            InstKind::MStore(a, b) => InstKind::MStore(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::MStore8(a, b) => {
                InstKind::MStore8(self.clone_value(a)?, self.clone_value(b)?)
            }
            InstKind::MSize => InstKind::MSize,
            InstKind::Fmp => InstKind::Fmp,
            InstKind::SetFmp(ptr) => InstKind::SetFmp(self.clone_value(ptr)?),
            InstKind::Alloc { size, kind, semantics } => {
                InstKind::Alloc { size: self.clone_value(size)?, kind, semantics }
            }
            InstKind::AbiEncode { selector, args, layout } => InstKind::AbiEncode {
                selector: match selector {
                    Some(selector) => Some(self.clone_value(selector)?),
                    None => None,
                },
                args: args
                    .iter()
                    .map(|&arg| self.clone_value(arg))
                    .collect::<Option<Vec<_>>>()?
                    .into(),
                layout,
            },
            InstKind::MCopy(a, b, c) => {
                InstKind::MCopy(self.clone_value(a)?, self.clone_value(b)?, self.clone_value(c)?)
            }
            InstKind::SLoad(a) => InstKind::SLoad(self.clone_value(a)?),
            InstKind::SStore(a, b) => InstKind::SStore(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::TLoad(a) => InstKind::TLoad(self.clone_value(a)?),
            InstKind::TStore(a, b) => InstKind::TStore(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::CalldataLoad(a) => InstKind::CalldataLoad(self.clone_value(a)?),
            InstKind::CalldataCopy(a, b, c) => InstKind::CalldataCopy(
                self.clone_value(a)?,
                self.clone_value(b)?,
                self.clone_value(c)?,
            ),
            InstKind::CalldataSize => InstKind::CalldataSize,
            InstKind::InternalFrameAddr(offset) => {
                let local_offset = offset.checked_sub(self.callee_frame_prefix)?;
                InstKind::InternalFrameAddr(self.frame_base + local_offset)
            }
            InstKind::CodeSize => InstKind::CodeSize,
            InstKind::LoadImmutable(offset) => InstKind::LoadImmutable(offset),
            InstKind::CodeCopy(a, b, c) => {
                InstKind::CodeCopy(self.clone_value(a)?, self.clone_value(b)?, self.clone_value(c)?)
            }
            InstKind::ExtCodeSize(a) => InstKind::ExtCodeSize(self.clone_value(a)?),
            InstKind::ExtCodeCopy(a, b, c, d) => InstKind::ExtCodeCopy(
                self.clone_value(a)?,
                self.clone_value(b)?,
                self.clone_value(c)?,
                self.clone_value(d)?,
            ),
            InstKind::ExtCodeHash(a) => InstKind::ExtCodeHash(self.clone_value(a)?),
            InstKind::ReturnDataSize => InstKind::ReturnDataSize,
            InstKind::ReturnDataCopy(a, b, c) => InstKind::ReturnDataCopy(
                self.clone_value(a)?,
                self.clone_value(b)?,
                self.clone_value(c)?,
            ),
            InstKind::Caller => InstKind::Caller,
            InstKind::CallValue => InstKind::CallValue,
            InstKind::Origin => InstKind::Origin,
            InstKind::GasPrice => InstKind::GasPrice,
            InstKind::BlockHash(a) => InstKind::BlockHash(self.clone_value(a)?),
            InstKind::Coinbase => InstKind::Coinbase,
            InstKind::Timestamp => InstKind::Timestamp,
            InstKind::BlockNumber => InstKind::BlockNumber,
            InstKind::PrevRandao => InstKind::PrevRandao,
            InstKind::GasLimit => InstKind::GasLimit,
            InstKind::ChainId => InstKind::ChainId,
            InstKind::Address => InstKind::Address,
            InstKind::Balance(a) => InstKind::Balance(self.clone_value(a)?),
            InstKind::SelfBalance => InstKind::SelfBalance,
            InstKind::Gas => InstKind::Gas,
            InstKind::BaseFee => InstKind::BaseFee,
            InstKind::BlobBaseFee => InstKind::BlobBaseFee,
            InstKind::BlobHash(a) => InstKind::BlobHash(self.clone_value(a)?),
            InstKind::Keccak256Bytes(object) => InstKind::Keccak256Bytes(self.clone_value(object)?),
            InstKind::Keccak256(a, b) => {
                InstKind::Keccak256(self.clone_value(a)?, self.clone_value(b)?)
            }
            InstKind::MappingSlot(a, b) => {
                InstKind::MappingSlot(self.clone_value(a)?, self.clone_value(b)?)
            }
            InstKind::MappingSlotMemory(a, b) => {
                InstKind::MappingSlotMemory(self.clone_value(a)?, self.clone_value(b)?)
            }
            InstKind::MappingSlotCalldata(a, b) => {
                InstKind::MappingSlotCalldata(self.clone_value(a)?, self.clone_value(b)?)
            }
            InstKind::Call { gas, addr, value, args_offset, args_size, ret_offset, ret_size } => {
                InstKind::Call {
                    gas: self.clone_value(gas)?,
                    addr: self.clone_value(addr)?,
                    value: self.clone_value(value)?,
                    args_offset: self.clone_value(args_offset)?,
                    args_size: self.clone_value(args_size)?,
                    ret_offset: self.clone_value(ret_offset)?,
                    ret_size: self.clone_value(ret_size)?,
                }
            }
            InstKind::StaticCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                InstKind::StaticCall {
                    gas: self.clone_value(gas)?,
                    addr: self.clone_value(addr)?,
                    args_offset: self.clone_value(args_offset)?,
                    args_size: self.clone_value(args_size)?,
                    ret_offset: self.clone_value(ret_offset)?,
                    ret_size: self.clone_value(ret_size)?,
                }
            }
            InstKind::DelegateCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                InstKind::DelegateCall {
                    gas: self.clone_value(gas)?,
                    addr: self.clone_value(addr)?,
                    args_offset: self.clone_value(args_offset)?,
                    args_size: self.clone_value(args_size)?,
                    ret_offset: self.clone_value(ret_offset)?,
                    ret_size: self.clone_value(ret_size)?,
                }
            }
            InstKind::InternalCall { function, args, returns } => InstKind::InternalCall {
                function,
                args: args
                    .into_iter()
                    .map(|arg| self.clone_value(arg))
                    .collect::<Option<Vec<_>>>()?
                    .into(),
                returns,
            },
            InstKind::Create(a, b, c) => {
                InstKind::Create(self.clone_value(a)?, self.clone_value(b)?, self.clone_value(c)?)
            }
            InstKind::Create2(a, b, c, d) => InstKind::Create2(
                self.clone_value(a)?,
                self.clone_value(b)?,
                self.clone_value(c)?,
                self.clone_value(d)?,
            ),
            InstKind::Log0(a, b) => InstKind::Log0(self.clone_value(a)?, self.clone_value(b)?),
            InstKind::Log1(a, b, c) => {
                InstKind::Log1(self.clone_value(a)?, self.clone_value(b)?, self.clone_value(c)?)
            }
            InstKind::Log2(a, b, c, d) => InstKind::Log2(
                self.clone_value(a)?,
                self.clone_value(b)?,
                self.clone_value(c)?,
                self.clone_value(d)?,
            ),
            InstKind::Log3(a, b, c, d, e) => InstKind::Log3(
                self.clone_value(a)?,
                self.clone_value(b)?,
                self.clone_value(c)?,
                self.clone_value(d)?,
                self.clone_value(e)?,
            ),
            InstKind::Log4(a, b, c, d, e, f) => InstKind::Log4(
                self.clone_value(a)?,
                self.clone_value(b)?,
                self.clone_value(c)?,
                self.clone_value(d)?,
                self.clone_value(e)?,
                self.clone_value(f)?,
            ),
            InstKind::Phi(_) => return None,
            InstKind::Select(a, b, c) => {
                InstKind::Select(self.clone_value(a)?, self.clone_value(b)?, self.clone_value(c)?)
            }
            InstKind::SignExtend(a, b) => {
                InstKind::SignExtend(self.clone_value(a)?, self.clone_value(b)?)
            }
        })
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
            Terminator::ReturnData { .. }
            | Terminator::Stop
            | Terminator::SelfDestruct { .. }
            | Terminator::TailCall { .. } => {
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
        let phi = caller.alloc_inst(Instruction::new(InstKind::Phi(incoming), Some(ty)));
        caller.blocks[continuation].instructions.insert(index, phi);
        values.push(caller.alloc_value(Value::Inst(phi)));
    }
    Some(values)
}

fn insert_extra_return_stores(caller: &mut Function, continuation: BlockId, values: &[ValueId]) {
    if values.is_empty() {
        return;
    }

    // Insert the stores right after the continuation block's leading phis.
    let phi_count = caller.blocks[continuation]
        .instructions
        .iter()
        .take_while(|&&inst_id| matches!(caller.instructions[inst_id].kind, InstKind::Phi(_)))
        .count();

    let base_load = caller.alloc_inst(Instruction::new(InstKind::Fmp, Some(MirType::MemPtr)));
    let base = caller.alloc_value(Value::Inst(base_load));
    let mut insert_at = phi_count;
    caller.blocks[continuation].instructions.insert(insert_at, base_load);
    insert_at += 1;

    for (index, &value) in values.iter().enumerate() {
        let offset = caller
            .alloc_value(Value::Immediate(Immediate::uint256(U256::from((index as u64 + 1) * 32))));
        let addr = caller
            .alloc_inst(Instruction::new(InstKind::Add(base, offset), Some(MirType::uint256())));
        let addr_value = caller.alloc_value(Value::Inst(addr));
        let store = caller.alloc_inst(Instruction::new(InstKind::MStore(addr_value, value), None));
        caller.blocks[continuation].instructions.insert(insert_at, addr);
        caller.blocks[continuation].instructions.insert(insert_at + 1, store);
        insert_at += 2;
    }

    let ptr_slot = caller.alloc_value(Value::Immediate(Immediate::uint256(U256::from(
        EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT,
    ))));
    let publish = caller.alloc_inst(Instruction::new(InstKind::MStore(ptr_slot, base), None));
    caller.blocks[continuation].instructions.insert(insert_at, publish);
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
        for &inst_id in &func.blocks[succ].instructions {
            if let InstKind::Phi(incoming) = &mut func.instructions[inst_id].kind {
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
        for &inst_id in &func.blocks[block_id].instructions {
            if let InstKind::Phi(incoming) = &mut func.instructions[inst_id].kind {
                incoming.retain(|(pred, _)| predecessors.contains(pred));
            }
        }
    }
}
