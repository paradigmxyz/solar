//! Pass infrastructure for MIR transformations and analyses.
//!
//! Transformation pipelines follow rustc MIR's pass-manager shape: passes
//! implement [`MirPass`] and pipelines are slices of trait-object references.
//! Analyses retain their LLVM/MLIR-style cache: read-only [`AnalysisPass`]es
//! produce results cached in an [`AnalysisManager`].
//!
//! # Usage
//!
//! ```ignore
//! // Read-only analysis pipeline (codegen):
//! let mut am = AnalysisManager::new();
//! let liveness = am.get_or_compute(&LivenessAnalysis, &func);
//!
//! let changed = run_passes(
//!     gcx,
//!     &mut module,
//!     &[&DCE_PASS],
//!     None,
//!     Optimizations::Allowed,
//! );
//! ```

pub use crate::pass_manager::{MirPass, Optimizations, run_passes, run_passes_no_validate};
use crate::{
    mir::{Function, MirPhase, Module},
    transform::{
        AdcePass, CfgSimplifyPass, CheckElimPass, CsePass, DcePass, FrameSlotPromotionPass,
        FunctionDcePass, GvnPass, IndVarSimplifyPass, InlinePass, InstSimplifyPass,
        JumpThreadingPass, LicmPass, LoadPrePass, LoopCanonicalizePass, LowerAbiPass,
        LowerDispatchPass, LowerEvmShapedPass, LowerMappingSlotsPass, MemoryDsePass,
        OutlineRevertsPass, PrePass, PureEvalPass, SccpTransformPass, StaticAllocPass,
        StorageDsePass, StorageLoadCsePass, StorageScalarPromotionPass,
    },
};
use solar_data_structures::map::FxHashMap;
use std::any::{Any, TypeId};

macro_rules! declare_passes {
    ($(
        $(#[doc = $description:literal])+
        $vis:vis const $const_name:ident = $pass:ident;
    )+) => {
        $(
            $(#[doc = $description])+
            $vis const $const_name: $pass = $pass;
        )+

        /// All known MIR passes exposed to `solar mir-opt`.
        pub static PASS_REGISTRY: &[&dyn MirPass] = &[$(&$const_name),+];

        static PASS_DESCRIPTIONS: &[(&dyn MirPass, &str)] = &[
            $((
                &$const_name,
                concat!($($description, "\n"),+).trim_ascii(),
            )),+
        ];
    };
}

declare_passes! {
    /// Internal MIR function inlining.
    pub(crate) const INLINE_PASS = InlinePass;

    /// Outline duplicate constant revert blocks before backend lowering.
    pub(crate) const OUTLINE_REVERTS_PASS = OutlineRevertsPass;

    /// Dead internal function elimination.
    pub(crate) const FUNCTION_DCE_PASS = FunctionDcePass;

    /// Sparse Conditional Constant Propagation.
    pub(crate) const SCCP_PASS = SccpTransformPass;

    /// Bounded evaluator for closed pure MIR loops/functions.
    pub(crate) const PURE_EVAL_PASS = PureEvalPass;

    /// Local MIR instruction simplification.
    pub(crate) const INST_SIMPLIFY_PASS = InstSimplifyPass;

    /// Common Subexpression Elimination (fixed-point).
    pub(crate) const CSE_PASS = CsePass;

    /// Partial redundancy elimination for pure expressions.
    pub(crate) const PRE_PASS = PrePass;

    /// Congruence-class global value numbering.
    pub(crate) const GVN_PASS = GvnPass;

    /// Reuse storage loads across definitely-disjoint stores.
    pub(crate) const STORAGE_LOAD_CSE_PASS = StorageLoadCsePass;

    /// Eliminate overwritten or repeated storage stores.
    pub(crate) const STORAGE_DSE_PASS = StorageDsePass;

    /// Availability-dataflow redundancy elimination and PRE for memory-dependent reads.
    pub(crate) const LOAD_PRE_PASS = LoadPrePass;

    /// Canonicalize natural loops with explicit preheaders.
    pub(crate) const LOOP_CANONICALIZE_PASS = LoopCanonicalizePass;

    /// Strength-reduce affine induction-variable address expressions.
    pub(crate) const INDVAR_SIMPLIFY_PASS = IndVarSimplifyPass;

    /// Promote simple loop-carried storage updates to memory.
    pub(crate) const STORAGE_PROMOTION_PASS = StorageScalarPromotionPass;

    /// Loop-Invariant Code Motion.
    pub(crate) const LICM_PASS = LicmPass;

    /// Range-based elimination of provably dead overflow-check branches.
    pub(crate) const CHECK_ELIM_PASS = CheckElimPass;

    /// Jump Threading (fixed-point).
    pub(crate) const JUMP_THREADING_PASS = JumpThreadingPass;

    /// CFG Simplification (fixed-point).
    pub(crate) const CFG_SIMPLIFY_PASS = CfgSimplifyPass;

    /// Promote non-escaping compiler-local slots to SSA values.
    pub(crate) const FRAME_SLOT_PROMOTION_PASS = FrameSlotPromotionPass;

    /// Local dead memory-store elimination.
    pub(crate) const MEMORY_DSE_PASS = MemoryDsePass;

    /// Place provably local fmp-bump allocations at static frame addresses.
    pub(crate) const STATIC_ALLOC_PASS = StaticAllocPass;

    /// Dead Code Elimination (fixed-point).
    pub(crate) const DCE_PASS = DcePass;

    /// Aggressive dead-code elimination for dead control regions.
    pub(crate) const ADCE_PASS = AdcePass;

    /// ABI phase lowering: external functions become self-decoding wrappers.
    pub(crate) const LOWER_ABI_PASS = LowerAbiPass;

    /// Dispatch phase lowering: synthesize the selector-switch `entry` function.
    pub(crate) const LOWER_DISPATCH_PASS = LowerDispatchPass;

    /// EVM-shape lowering: non-returning internal calls become tail calls.
    pub(crate) const LOWER_EVM_SHAPED_PASS = LowerEvmShapedPass;

    /// Lower mapping-slot hash builtins to memory operations.
    pub(crate) const LOWER_MAPPING_SLOTS_PASS = LowerMappingSlotsPass;
}

/// Finds a pass in the global MIR pass registry by command-line name.
pub fn lookup_pass(name: &str) -> Option<&'static dyn MirPass> {
    PASS_REGISTRY.iter().copied().find(|pass| pass.name() == name)
}

/// Returns the human-readable help text for a registered MIR pass.
pub fn pass_description(pass: &dyn MirPass) -> &'static str {
    PASS_DESCRIPTIONS
        .iter()
        .find_map(|(candidate, description)| {
            (candidate.name() == pass.name()).then_some(*description)
        })
        .unwrap_or("")
}

/// The canonical MIR optimization pipeline used by EVM codegen.
pub static DEFAULT_PIPELINE: &[&dyn MirPass] = &[
    &INLINE_PASS,
    &FUNCTION_DCE_PASS,
    &SCCP_PASS,
    &PURE_EVAL_PASS,
    &INST_SIMPLIFY_PASS,
    &CSE_PASS,
    // Reuse mapping slots before their scratch-memory expansion can obscure
    // the semantic expression from the remaining optimization passes.
    &LOWER_MAPPING_SLOTS_PASS,
    &GVN_PASS,
    &PRE_PASS,
    &STORAGE_LOAD_CSE_PASS,
    &STORAGE_DSE_PASS,
    &LOAD_PRE_PASS,
    &FRAME_SLOT_PROMOTION_PASS,
    &LOOP_CANONICALIZE_PASS,
    &INDVAR_SIMPLIFY_PASS,
    &STORAGE_PROMOTION_PASS,
    &LICM_PASS,
    &CHECK_ELIM_PASS,
    &JUMP_THREADING_PASS,
    &CFG_SIMPLIFY_PASS,
    &MEMORY_DSE_PASS,
    &STATIC_ALLOC_PASS,
    &ADCE_PASS,
    &DCE_PASS,
];

/// Cleanup passes rerun after the primary pipeline until no pass changes MIR.
///
/// Keep this group focused on simplification and canonicalization. Structural
/// profitability passes such as inlining and storage promotion run once in
/// [`DEFAULT_PIPELINE`], while this loop cleans up opportunities exposed by
/// those transforms.
pub static DEFAULT_CLEANUP_PIPELINE: &[&dyn MirPass] = &[
    &SCCP_PASS,
    &PURE_EVAL_PASS,
    &INST_SIMPLIFY_PASS,
    &CSE_PASS,
    &GVN_PASS,
    &PRE_PASS,
    &STORAGE_LOAD_CSE_PASS,
    &STORAGE_DSE_PASS,
    &LOAD_PRE_PASS,
    &CHECK_ELIM_PASS,
    &JUMP_THREADING_PASS,
    &CFG_SIMPLIFY_PASS,
    &FRAME_SLOT_PROMOTION_PASS,
    &MEMORY_DSE_PASS,
    &ADCE_PASS,
    &DCE_PASS,
];

const DEFAULT_CLEANUP_MAX_ROUNDS: usize = 3;

/// Runs the canonical MIR optimization pipeline used by EVM codegen.
///
/// This is a phase transition: the module comes out in `MirPhase::Optimized`.
/// Ad-hoc `solar mir-opt` pass lists deliberately do not advance the phase.
#[tracing::instrument(
    name = "mir_pipeline",
    level = "debug",
    skip_all,
    fields(module = %module.name),
)]
pub fn run_default_pipeline(gcx: solar_sema::Gcx<'_>, module: &mut Module) -> bool {
    let optimizations = Optimizations::for_gcx(gcx);
    let mut changed =
        run_passes(gcx, module, DEFAULT_PIPELINE, Some(MirPhase::Optimized), optimizations);
    changed |=
        run_cleanup_pipeline_to_fixpoint(gcx, module, DEFAULT_CLEANUP_PIPELINE, optimizations);
    changed
}

fn run_cleanup_pipeline_to_fixpoint(
    gcx: solar_sema::Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn MirPass],
    optimizations: Optimizations,
) -> bool {
    let mut changed = false;
    for _ in 1..=DEFAULT_CLEANUP_MAX_ROUNDS {
        let round_changed = run_passes(gcx, module, passes, None, optimizations);
        if !round_changed {
            break;
        }
        changed = true;
    }
    changed
}

/// A key identifying a particular analysis, derived from its result type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct AnalysisKey(TypeId);

impl AnalysisKey {
    /// Creates a key from a type.
    pub(crate) fn of<T: 'static>() -> Self {
        Self(TypeId::of::<T>())
    }
}

/// A read-only analysis pass.
///
/// Analysis passes inspect a function without modifying it and produce a
/// cacheable result that downstream passes can query via [`AnalysisManager`].
pub(crate) trait AnalysisPass {
    /// The result type produced by this analysis.
    type Result: 'static;

    /// Computes the analysis result for the given function.
    fn run(&self, func: &Function) -> Self::Result;
}

/// Runs a function-local transform over every bodied function in a module.
pub(crate) fn run_function_pass(
    module: &mut Module,
    mut run: impl FnMut(&mut Function) -> bool,
) -> bool {
    let mut changed = false;
    for func in module.functions.iter_mut().filter(|func| !func.blocks.is_empty()) {
        changed |= run(func);
    }
    changed
}

/// Manages cached analysis results for a function.
///
/// Analyses are keyed by their result type via [`AnalysisKey`].
#[derive(Default)]
pub(crate) struct AnalysisManager {
    results: FxHashMap<AnalysisKey, Box<dyn Any>>,
}

impl AnalysisManager {
    /// Creates a new, empty analysis manager.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Returns the result of the analysis, computing and caching it if not already present.
    ///
    /// This is the recommended way to obtain analysis results, matching
    /// LLVM's `AnalysisManager::getResult<AnalysisT>(F)` pattern.
    pub(crate) fn get_or_compute<A: AnalysisPass>(
        &mut self,
        analysis: &A,
        func: &Function,
    ) -> &A::Result {
        let key = AnalysisKey::of::<A::Result>();
        self.results.entry(key).or_insert_with(|| {
            let result = analysis.run(func);
            Box::new(result)
        });
        self.results[&key].downcast_ref::<A::Result>().unwrap()
    }
}

/// Liveness analysis pass.
pub(crate) struct LivenessAnalysis;

impl AnalysisPass for LivenessAnalysis {
    type Result = crate::analysis::Liveness;

    fn run(&self, func: &Function) -> Self::Result {
        crate::analysis::Liveness::compute(func)
    }
}
