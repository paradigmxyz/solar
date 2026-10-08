//! LLM-backed rewriting of lowered MIR functions.
//!
//! This pass asks a rewriter, usually a language model behind [`crate::llm::set_rewriter`], for
//! cheaper versions of small word-level functions, and keeps the candidates it can check. It runs
//! only with `-Zllm-optimize`, on lowered MIR, in the lowered pipeline after `dce` and before
//! stack scheduling. Without the flag it does nothing, so no default build depends on a model.
//!
//! # Sessions
//!
//! [`eligibility`] decides which functions are offered: internal, non-recursive functions of a few
//! words, outside the constructor and calling nothing recursive, whose every reachable operation
//! the tests run, storage and logs included. Each offered
//! function is printed as candidate text, lowered MIR without metadata, parsed back, and printed
//! again; that fixed point is what the rewriter sees and what the cache is keyed by, so debug
//! options cannot change which rewrites apply. A rewriter session then proposes candidates over at
//! most `-Zllm-rounds` rounds. Each candidate gets a [`Verdict`], and the rewriter is asked for
//! something cheaper than the best so far, until it has nothing, fails, or is rejected three times
//! in a row. A rewriter failure never fails the compilation.
//!
//! # Checks
//!
//! Candidates are untrusted, and each check below is a [`Stage`] of the verdict:
//!
//! 1. Parse: [`Module::parse_function`] accepts one function with no metadata, `undef`, numeric
//!    function references, or declarations. The backend trusts metadata such as memory regions, and
//!    a model cannot be trusted to state it.
//! 2. Constraints: the candidate keeps the name, parameters, and return type, uses only operations
//!    the interpreter runs and the target EVM version has, calls only what the original calls, tail
//!    calls only as `lower-evm-shaped` would, to functions that never return, switches on distinct
//!    constants, and keeps no more values live at once than the original. The live values bound the
//!    stack pressure the function-level cost model cannot see.
//! 3. Validation: the MIR validator checks the candidate's body in place of the original's.
//! 4. Equivalence: [`equivalence`] runs it against the original on generated inputs, with seeded
//!    storage and context; it must end the same way, write only memory the original writes and
//!    exactly the storage slots it writes, leave them the same, log the same events, run every
//!    block, and run on wherever the original runs past the tests' fuel.
//! 5. Cost: [`cost`] prices it with the target cost model, and it must beat the best so far: by a
//!    stack copy's lifetime gas when optimizing gas, and in bytes, then gas, for size.
//!
//! The adopted function keeps the original's name, signature, attributes, spans, and memory
//! layout, and takes the candidate's body after dead code elimination, with every instruction's
//! metadata empty and its debug information marked as dropped. Every function is decided against
//! the unchanged module and the rewrites are applied afterwards in function order, so the result
//! does not depend on parallel compilation.
//!
//! # Cache
//!
//! With `-Zllm-cache`, [`cache`] records accepted rewrites, keyed by the candidate text of the
//! original and the target, and replays them before asking a rewriter; `-Zllm-optimize=replay`
//! only replays. A replayed rewrite goes through every check again.
//!
//! # Limits
//!
//! Testing is not proof: a candidate that differs only on inputs no generator reaches passes.
//! This is why the pass stays behind an unstable flag. The cost model ignores stack scheduling
//! beyond one stack copy per operand, so accepted rewrites need benchmarks. `-Zllm-trace` prints
//! every offer, verdict, and decision.

use self::{
    cache::{Cache, Entry, ScriptRewriter},
    equivalence::{Rejection, Tests},
};
use crate::{
    backend::evm::op,
    llm::{self, CostReport, LlmRewriter, Proposal, RewriteRequest, Stage, Verdict},
    mir::{
        Callee, Function, FunctionId, InstKind, InstructionMetadata, MirPhase, Module, Terminator,
        Value, ValueId,
        analysis::{CallGraphInfo, validate_function_at_phase},
        pass::{MirPass, ModuleAnalyses},
        transform::{dce::DeadCodeEliminator, lower_evm_shaped::is_tail_callable},
        utils::interp,
    },
    target::{Cost, Target},
};
use alloy_primitives::{B256, keccak256};
use solar_config::{LlmOptimizeMode, OptimizationMode};
use solar_data_structures::map::FxHashSet;
use solar_interface::{
    ColorChoice,
    diagnostics::DiagCtxt,
    source_map::{FileName, SourceMap},
};
use solar_sema::Gcx;
use std::{fmt, sync::Arc};

mod cache;
mod cost;
mod eligibility;
mod equivalence;

/// Candidates asked for per function unless `-Zllm-rounds` says otherwise.
const DEFAULT_ROUNDS: usize = 6;
/// Test inputs per function unless `-Zllm-samples` says otherwise.
const DEFAULT_SAMPLES: usize = 512;
/// Rejections in a row that end a session.
const MAX_REJECTIONS: usize = 3;

/// Pass rewriting functions with checked candidates from an LLM rewriter.
pub(crate) struct LlmOptimize;

impl MirPass for LlmOptimize {
    fn name(&self) -> &'static str {
        "llm-optimize"
    }

    fn is_enabled(&self, gcx: Gcx<'_>, module: &Module) -> bool {
        gcx.sess.opts.unstable.llm_optimize.is_some()
            && !matches!(gcx.sess.opts.optimization, OptimizationMode::None)
            && module.phase() == MirPhase::Lowered
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, _analyses: &mut ModuleAnalyses) -> bool {
        let Some(optimizer) = Optimizer::new(gcx) else { return false };
        let rewrites = optimizer.run(module);
        let changed = !rewrites.is_empty();
        for (id, rewrite) in rewrites {
            *module.function_mut(id) = rewrite.function;
            module.llm_rewrites.push(rewrite.digest);
        }
        changed
    }
}

/// A rewrite the pass applies.
struct Rewrite {
    function: Function,
    /// Names the original and its replacement, for build identities.
    digest: B256,
}

/// A candidate that passed every check.
struct Accepted {
    /// The original with the candidate's body.
    function: Function,
    /// Its candidate text.
    text: String,
    cost: CostReport,
}

/// What the cache holds for a function.
enum Replay {
    /// A rewrite that passed every check again.
    Accepted(Box<Accepted>),
    /// A rewrite that did not, or an entry that could not be used.
    Rejected,
    /// Nothing.
    Missing,
}

/// The settings of one run of the pass.
struct Optimizer<'gcx> {
    gcx: Gcx<'gcx>,
    target: Target,
    /// Where candidates come from, unless only replaying.
    rewriter: Option<Arc<dyn LlmRewriter>>,
    cache: Option<Cache>,
    /// Whether accepted rewrites are recorded in the cache.
    record: bool,
    /// Where candidates come from, as cache evidence.
    source: String,
    rounds: usize,
    samples: usize,
    vocabulary: String,
}

impl<'gcx> Optimizer<'gcx> {
    fn new(gcx: Gcx<'gcx>) -> Option<Self> {
        let unstable = &gcx.sess.opts.unstable;
        let (rewriter, source): (Option<Arc<dyn LlmRewriter>>, _) = match unstable.llm_optimize? {
            LlmOptimizeMode::Replay => (None, "replay".to_string()),
            LlmOptimizeMode::Script => {
                let path = unstable.llm_script.as_deref()?;
                match ScriptRewriter::load(path) {
                    Ok(script) => (Some(Arc::new(script)), "script".to_string()),
                    Err(error) => {
                        gcx.dcx().err(error).emit();
                        return None;
                    }
                }
            }
            LlmOptimizeMode::Live => {
                let Some(rewriter) = llm::rewriter() else {
                    let message = "`-Zllm-optimize=live` requires an installed rewriter";
                    gcx.dcx()
                        .err(message)
                        .help("build the compiler with its `llm` feature, or install a rewriter")
                        .emit();
                    return None;
                };
                let model = unstable.llm_model.as_deref().unwrap_or("default");
                (Some(rewriter), format!("live {model}"))
            }
            _ => return None,
        };
        let target = Target::new(gcx);
        Some(Self {
            gcx,
            target,
            rewriter,
            cache: unstable.llm_cache.as_deref().map(Cache::new),
            record: unstable.llm_optimize != Some(LlmOptimizeMode::Replay),
            source,
            rounds: unstable.llm_rounds.unwrap_or(DEFAULT_ROUNDS),
            samples: unstable.llm_samples.unwrap_or(DEFAULT_SAMPLES),
            vocabulary: vocabulary(target),
        })
    }

    /// Decides every offered function of `module` and returns the rewrites, in function order.
    fn run(&self, module: &Module) -> Vec<(FunctionId, Rewrite)> {
        if let Some(reason) = eligibility::module_exclusion(module) {
            self.trace(module, None, format_args!("offers nothing: {reason}"));
            return Vec::new();
        }
        let graph = CallGraphInfo::new(module);
        let mut rewrites = Vec::new();
        for id in module.functions.indices() {
            if let Some(reason) = eligibility::exclusion(module, &graph, id) {
                self.trace(module, Some(id), format_args!("not offered: {reason}"));
            } else if let Some(rewrite) = self.optimize(module, id) {
                rewrites.push((id, rewrite));
            }
        }
        rewrites
    }

    /// Replays or asks for a rewrite of function `id`.
    fn optimize(&self, module: &Module, id: FunctionId) -> Option<Rewrite> {
        let original = module.function(id);
        let canonical = match self.parse(module, &module.candidate_text(original).to_string()) {
            Ok(parsed) => module.candidate_text(&parsed).to_string(),
            Err(error) => {
                let message = format_args!("not offered: its text does not parse back:\n{error}");
                self.trace(module, Some(id), message);
                return None;
            }
        };
        let seed = u64::from_be_bytes(keccak256(&canonical).0[..8].try_into().unwrap());
        let tests = match Tests::new(self.target, module, id, seed, self.samples) {
            Ok(tests) => tests,
            Err(reason) => {
                self.trace(module, Some(id), format_args!("not offered: {reason}"));
                return None;
            }
        };
        let baseline = tests.baseline();
        let key = Cache::key(self.target, &canonical);
        let digest = |accepted: &Accepted| keccak256(format!("{key}\n{}", accepted.text));
        let request = self.rewriter.as_ref().map(|_| self.request(module, id, &tests, &canonical));
        let cached = match self.replay(module, id, &tests, &key, &canonical) {
            Replay::Accepted(accepted) => {
                if let (Some(rewriter), Some(request)) = (&self.rewriter, &request) {
                    rewriter.cached(request, accepted.cost);
                }
                return Some(Rewrite { digest: digest(&accepted), function: accepted.function });
            }
            Replay::Rejected => true,
            Replay::Missing => false,
        };
        let (Some(rewriter), Some(request)) = (&self.rewriter, request) else {
            if !cached {
                self.trace(module, Some(id), "no cached rewrite");
            }
            return None;
        };
        let best = self.ask(rewriter.as_ref(), module, id, &tests, &request)?;
        self.trace(module, Some(id), format_args!("rewritten from {baseline} to {}", best.cost));
        if self.record
            && let Some(cache) = &self.cache
        {
            let evidence = format!("source: {}\nsamples: {}\n", self.source, self.samples);
            let entry = Entry { original: canonical, rewrite: best.text.clone(), evidence };
            if let Err(error) = cache.store(&key, &entry) {
                let message = format!("cannot record an `llm-optimize` rewrite: {error}");
                self.gcx.dcx().warn(message).emit();
            }
        }
        Some(Rewrite { digest: digest(&best), function: best.function })
    }

    /// What a rewriter is told about function `id`, whose candidate text is `canonical`.
    fn request(
        &self,
        module: &Module,
        id: FunctionId,
        tests: &Tests<'_>,
        canonical: &str,
    ) -> RewriteRequest {
        let original = module.function(id);
        RewriteRequest {
            module_name: module.name.to_string(),
            function_name: original.name.to_string(),
            function_text: canonical.to_string(),
            context: callees(original, module.functions.len())
                .into_iter()
                .map(|callee| module.candidate_text(module.function(callee)).to_string())
                .collect::<Vec<_>>()
                .join("\n"),
            objective: self.target.optimization(),
            optimizer_runs: self.target.expected_executions(),
            evm_version: self.target.evm_version(),
            baseline: tests.baseline(),
            vocabulary: self.vocabulary.clone(),
        }
    }

    /// Asks `rewriter` for candidates replacing function `id` until it has nothing cheaper, the
    /// rounds run out, or it fails, and returns the best that passed every check.
    fn ask(
        &self,
        rewriter: &dyn LlmRewriter,
        module: &Module,
        id: FunctionId,
        tests: &Tests<'_>,
        request: &RewriteRequest,
    ) -> Option<Accepted> {
        let baseline = tests.baseline();
        self.trace(module, Some(id), format_args!("offered at {baseline}"));
        let mut session = match rewriter.session(request) {
            Ok(session) => session,
            Err(error) => {
                self.trace(module, Some(id), format_args!("the rewriter failed: {error}"));
                return None;
            }
        };
        let mut best = None::<Accepted>;
        let mut verdict = None;
        // Whether the session has heard `verdict`.
        let mut heard = true;
        let mut rejections = 0;
        for round in 1..=self.rounds {
            let proposal = session.propose(verdict.as_ref());
            heard = true;
            let text = match proposal {
                Ok(Proposal::Candidate(text)) => text,
                Ok(Proposal::Done) => {
                    self.trace(module, Some(id), format_args!("round {round}: nothing cheaper"));
                    break;
                }
                Err(error) => {
                    let message = format_args!("round {round}: the rewriter failed: {error}");
                    self.trace(module, Some(id), message);
                    break;
                }
            };
            let bar = best.as_ref().map_or(baseline, |best| best.cost);
            match self.check(module, id, tests, &text, bar) {
                Ok(accepted) => {
                    let message = format_args!("round {round}: accepted at {}", accepted.cost);
                    self.trace(module, Some(id), message);
                    verdict = Some(Verdict::Accepted { cost: accepted.cost });
                    heard = false;
                    best = Some(accepted);
                    rejections = 0;
                }
                Err(Rejection { stage, reason, counterexample }) => {
                    let on = counterexample.as_ref().map(|input| format!("\n  on {input}"));
                    let message = format_args!(
                        "round {round}: rejected at {stage}: {reason}{}",
                        on.unwrap_or_default()
                    );
                    self.trace(module, Some(id), message);
                    verdict = Some(Verdict::Rejected { stage, reason, counterexample });
                    heard = false;
                    rejections += 1;
                    if rejections == MAX_REJECTIONS {
                        let message = format!("stopped after {MAX_REJECTIONS} rejections in a row");
                        self.trace(module, Some(id), message);
                        break;
                    }
                }
            }
        }
        session.finish(verdict.as_ref().filter(|_| !heard), best.as_ref().map(|best| best.cost));
        best
    }

    /// Checks the cached rewrite of function `id`, if there is one.
    fn replay(
        &self,
        module: &Module,
        id: FunctionId,
        tests: &Tests<'_>,
        key: &str,
        canonical: &str,
    ) -> Replay {
        let Some(cache) = &self.cache else { return Replay::Missing };
        let entry = match cache.load(key) {
            Ok(Some(entry)) if entry.original == canonical => entry,
            Ok(Some(_)) => {
                self.trace(module, Some(id), "the cached rewrite is for another original");
                return Replay::Rejected;
            }
            Ok(None) => return Replay::Missing,
            Err(error) => {
                self.trace(module, Some(id), error);
                return Replay::Rejected;
            }
        };
        let baseline = tests.baseline();
        match self.check(module, id, tests, &entry.rewrite, baseline) {
            Ok(accepted) => {
                let message = format_args!("replayed, from {baseline} to {}", accepted.cost);
                self.trace(module, Some(id), message);
                Replay::Accepted(Box::new(accepted))
            }
            Err(Rejection { stage, reason, .. }) => {
                let message = format_args!("the cached rewrite is rejected at {stage}: {reason}");
                self.trace(module, Some(id), message);
                Replay::Rejected
            }
        }
    }

    /// Runs every check on candidate `text` for function `id`, which must beat `bar`.
    fn check(
        &self,
        module: &Module,
        id: FunctionId,
        tests: &Tests<'_>,
        text: &str,
        bar: CostReport,
    ) -> Result<Accepted, Rejection> {
        let original = module.function(id);
        let candidate =
            self.parse(module, text).map_err(|error| Rejection::new(Stage::Parse, error))?;
        constraints(self.target, module, id, &candidate)
            .map_err(|reason| Rejection::new(Stage::Constraints, reason))?;
        let mut function = original.clone();
        function.replace_body(candidate);
        let dcx = private_diagnostics(None);
        validate_function_at_phase(&dcx, module, id, &function, MirPhase::Lowered)
            .map_err(|_| Rejection::new(Stage::Validation, emitted(&dcx)))?;
        DeadCodeEliminator::new().run_to_fixpoint(&mut function);
        drop_metadata(&mut function);
        let (live, limit) = (cost::max_live_values(&function), cost::max_live_values(original));
        if live > limit {
            let reason = format!(
                "keeps {live} values live at once, more than the original's {limit}, which the \
                 stack may not hold"
            );
            return Err(Rejection::new(Stage::Constraints, reason));
        }
        let cost = tests.check(&function)?;
        if !self.beats(cost, bar) {
            let reason = format!("costs {cost}, which does not beat {bar}");
            return Err(Rejection::new(Stage::Cost, reason));
        }
        let text = module.candidate_text(&function).to_string();
        Ok(Accepted { function, text, cost })
    }

    /// Returns whether `cost` beats `bar` under the objective: by a stack copy's lifetime gas
    /// when optimizing gas, which is below the model's resolution, or in bytes, then gas.
    fn beats(&self, cost: CostReport, bar: CostReport) -> bool {
        let target = self.target;
        let model = |report: CostReport| {
            let gas = u32::try_from(report.gas).unwrap_or(u32::MAX);
            Cost::new(gas, u32::try_from(report.bytes).unwrap_or(u32::MAX))
        };
        if target.optimization().is_size() {
            target.cmp(model(cost), model(bar)).is_lt()
        } else {
            let margin = target.lifetime_gas(target.dup());
            target.lifetime_gas(model(cost)) + margin <= target.lifetime_gas(model(bar))
        }
    }

    /// Parses candidate `text` for `module`, keeping its diagnostics private.
    fn parse(&self, module: &Module, text: &str) -> Result<Function, String> {
        let source_map = Arc::new(SourceMap::empty());
        let session =
            self.gcx.sess.with_diagnostics(private_diagnostics(Some(Arc::clone(&source_map))));
        let file = source_map
            .new_source_file(FileName::Custom("candidate".into()), text)
            .map_err(|error| error.to_string())?;
        module.parse_function(&session, &file).map_err(|_| emitted(&session.dcx))
    }

    /// Prints a trace line with `-Zllm-trace`.
    fn trace(&self, module: &Module, function: Option<FunctionId>, message: impl fmt::Display) {
        if !self.gcx.sess.opts.unstable.llm_trace {
            return;
        }
        match function {
            Some(id) => {
                println!("llm-optimize {} @{}: {message}", module.name, module.function(id).name)
            }
            None => println!("llm-optimize {}: {message}", module.name),
        }
    }
}

/// Checks what the candidate may be before its body replaces the body of function `id`.
fn constraints(
    target: Target,
    module: &Module,
    id: FunctionId,
    candidate: &Function,
) -> Result<(), String> {
    let original = module.function(id);
    if candidate.name != original.name {
        return Err(format!("renames the function to `@{}`", candidate.name));
    }
    if candidate.params != original.params {
        return Err("changes the parameters".into());
    }
    if candidate.return_components() != original.return_components() {
        return Err("changes the return type".into());
    }
    let callees = callees(original, module.functions.len());
    let callee = |callee: FunctionId| {
        if callees.contains(&callee) {
            return Ok(());
        }
        Err(format!("calls `@{}`, which the original does not call", module.function(callee).name))
    };
    for inst in candidate.instructions() {
        let kind = &candidate.inst(inst).kind;
        let mnemonic = kind.op_def().mnemonic;
        if !equivalence::runs(kind) {
            return Err(format!("uses `{mnemonic}`, which the tests cannot run"));
        }
        if let Some(opcode) = kind.evm_opcode()
            && !op::is_available(opcode, target.evm_version())
        {
            return Err(format!("uses `{mnemonic}`, which {} lacks", target.evm_version()));
        }
        if let InstKind::ICall { function: Callee::Function(function), .. } = kind {
            callee(*function)?;
        }
    }
    for block in &candidate.blocks {
        let Some(terminator) = &block.terminator else { continue };
        if !interp::supports_terminator(terminator) {
            return Err("ends a block with a terminator the tests cannot run".into());
        }
        match terminator {
            Terminator::TailCall { function, .. } => {
                callee(*function)?;
                // The backend lowers only the tail calls `lower-evm-shaped` forms. Code the
                // constructor runs is never offered, so these are runtime tail calls.
                let graph = CallGraphInfo::new(module);
                let name = module.function(*function).name;
                if !is_tail_callable(module, &graph, *function) {
                    return Err(format!(
                        "tail calls `@{name}`, which is recursive or an entry point; call it \
                         instead"
                    ));
                }
                // The backend never expects a tail call's target back, and a returning callee
                // leaves its results where its own caller would read them.
                if module.returning_functions().contains(*function) {
                    return Err(format!(
                        "tail calls `@{name}`, which returns; call it and return its results \
                         instead"
                    ));
                }
            }
            Terminator::Switch { cases, .. } => {
                let mut seen = FxHashSet::default();
                for &(case, _) in cases {
                    let Value::Immediate(immediate) = candidate.value(case) else {
                        return Err("switches on a case that is not a constant".into());
                    };
                    if !seen.insert(immediate.as_u256()) {
                        return Err("switches on the same constant twice".into());
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Returns the functions `function` calls, in function order.
fn callees(function: &Function, function_count: usize) -> Vec<FunctionId> {
    CallGraphInfo::collect_internal_callees(function, function_count).iter().collect()
}

/// Clears the metadata of every instruction and terminator of `function`.
///
/// NOTE: The backend trusts metadata such as memory regions and effects, which a rewriter cannot
/// be trusted to state, and a rewritten body has no source locations of its own, so its debug
/// information is marked as intentionally dropped.
fn drop_metadata(function: &mut Function) {
    let instructions = function.instructions().collect::<Vec<_>>();
    for inst in instructions {
        let metadata = &mut function.inst_mut(inst).metadata;
        *metadata = InstructionMetadata::EMPTY;
        metadata.mark_debug_info_dropped();
    }
    for block in &mut function.blocks {
        block.terminator_metadata = InstructionMetadata::EMPTY;
        block.terminator_metadata.mark_debug_info_dropped();
    }
}

/// Returns the operations a candidate may use on `target`: each operand-only operation the
/// interpreter runs and the EVM version has, and the forms that carry attributes.
fn vocabulary(target: Target) -> String {
    let mut words = InstKind::MNEMONICS
        .iter()
        .copied()
        .filter(|&name| {
            InstKind::operand_only(name).is_some_and(|(arity, build)| {
                let kind = build(&(0..arity).map(ValueId::from_usize).collect::<Vec<_>>());
                equivalence::runs(&kind)
                    && kind
                        .evm_opcode()
                        .is_none_or(|opcode| op::is_available(opcode, target.evm_version()))
            })
        })
        .collect::<Vec<_>>();
    // These carry attributes, so they have no operand-only form to test.
    words.extend(["trunc", "sext", "ptrtoint", "phi", "icall"]);
    words.join(" ")
}

/// Creates a diagnostic context that renders into a buffer, without tracking notes.
fn private_diagnostics(source_map: Option<Arc<SourceMap>>) -> DiagCtxt {
    DiagCtxt::with_buffer_emitter(source_map, ColorChoice::Never)
        .with_flags(|flags| flags.track_diagnostics = false)
}

/// Returns what `dcx` rendered.
fn emitted(dcx: &DiagCtxt) -> String {
    dcx.emitted_diagnostics().map(|diagnostics| diagnostics.to_string()).unwrap_or_default()
}
