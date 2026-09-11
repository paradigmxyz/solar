//! Shared specialization of internal leaf helpers for constant arguments.
//!
//! Calls with the same typed constants share one specialized body. A common
//! constant across every direct caller specializes the original body in place,
//! including a helper with a single caller that the inliners left shared (a
//! reference-returning codec whose mode flags are literals at its only call);
//! otherwise a repeated call shape may get one clone. SCCP, range reasoning,
//! e-graph extraction and CFG cleanup price the simplified body before committing.
//! The body plus all affected call protocols must shrink under the target model,
//! and execution cost must not grow. This is a whole-module size budget: retained
//! generic bodies are charged in full, rather than amortized over call counts.
//!
//! Only private-signature, non-recursive leaf bodies are cloned or priced. A body
//! that still calls other helpers is not priced, but a one-byte literal that every
//! caller passes for one of its parameters is substituted in place: the literal
//! costs no more to materialize than duplicating the argument, and dead-argument
//! elimination then drops the parameter. Tail calls, public entries, baked frame
//! addresses and explicitly non-inlineable helpers are excluded. Cloning is bounded to one
//! candidate per original body per pass; subsequent dead-argument elimination removes the
//! specialized parameters. Run after function-pointer specialization and before dead-argument
//! elimination.

use crate::{
    mir::{
        ArgIdx, FunctionId, Immediate, InstId, InstKind, Module, Terminator, Value,
        analysis::CallGraphInfo,
        pass::{MirPass, ModuleAnalyses},
        transform::{cfg_simplify, check_elim, dce, egraph, sccp},
    },
    target::Target,
};
use alloy_primitives::U256;
use solar_data_structures::{
    index::{IndexVec, index_vec},
    map::FxHashMap,
};

/// Module pass for sharing constant-argument specializations.
pub(crate) struct Specialize;

type Constants = Vec<(ArgIdx, Immediate)>;

#[derive(Clone)]
struct CallSite {
    caller: FunctionId,
    inst: InstId,
    constants: Constants,
}

impl MirPass for Specialize {
    fn name(&self) -> &'static str {
        "specialize"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _: &mut ModuleAnalyses,
    ) -> bool {
        // A literal substituted into one body turns the arguments it forwards
        // into literals for the helpers that body calls, so sweep again until
        // nothing changes; the sweep count is bounded by the call-graph depth.
        const MAX_ROUNDS: usize = 4;
        let mut changed = false;
        for _ in 0..MAX_ROUNDS {
            if !specialize_round(gcx, module) {
                break;
            }
            changed = true;
        }
        changed
    }
}

/// One specialization sweep over every call site in the module.
fn specialize_round(gcx: solar_sema::Gcx<'_>, module: &mut Module) -> bool {
    let graph = CallGraphInfo::new(module);
    let target = Target::new(gcx);
    let mut sites: IndexVec<FunctionId, Vec<CallSite>> =
        index_vec![Vec::new(); module.functions.len()];
    let mut tail_called =
        solar_data_structures::bit_set::DenseBitSet::new_empty(module.functions.len());
    for (caller, func) in module.functions.iter_enumerated() {
        for inst in func.instructions() {
            if let InstKind::ICall { function, args, .. } = &func.inst(inst).kind {
                let constants = args
                    .iter()
                    .enumerate()
                    .filter_map(|(index, &arg)| {
                        let Value::Immediate(value) = func.value(arg) else { return None };
                        let index = ArgIdx::new(index);
                        (module.function(*function).params.get(index) == Some(&value.ty()))
                            .then(|| (index, value.clone()))
                    })
                    .collect();
                sites[*function].push(CallSite { caller, inst, constants });
            }
        }
        for block in &func.blocks {
            if let Some(Terminator::TailCall { function, .. }) = block.terminator {
                tail_called.insert(function);
            }
        }
    }
    let mut changed = false;
    for (callee, calls) in sites.iter_enumerated() {
        let body = module.function(callee);
        if calls.is_empty()
            || graph.is_recursive(callee)
            || tail_called.contains(callee)
            || body.params.len() != body.arg_indices().count()
            || body.is_public()
            || body.selector.is_some()
            || body.attributes.is_constructor
            || body.attributes.is_fallback
            || body.attributes.is_receive
            || body.attributes.no_inline
            || module.dispatch_entry() == Some(callee)
        {
            continue;
        }
        let leaf = !body.instructions().any(|inst| {
            matches!(body.inst(inst).kind, InstKind::ICall { .. } | InstKind::InternalFrameAddr(_))
        }) && !body
            .blocks
            .iter()
            .any(|block| matches!(block.terminator, Some(Terminator::TailCall { .. })));

        let mut constants = calls[0].constants.clone();
        constants.retain(|pair| calls.iter().all(|site| site.constants.contains(pair)));
        if !leaf {
            // A body with calls cannot be priced in a trial module, but a
            // one-byte literal that every caller passes costs no more to
            // materialize than the argument it replaces costs to
            // duplicate, so substitute it in place; dead-argument
            // elimination then drops the parameter and the later cleanup
            // folds the branches it decided.
            // helper(argK, ...) => helper(literalK, ...)
            constants.retain(|(_, immediate)| {
                immediate.as_u256().is_some_and(|value| value <= U256::from(u8::MAX))
            });
            if constants.is_empty() {
                continue;
            }
            let func = module.function_mut(callee);
            let uses = func.arg_uses();
            let mut replacements = FxHashMap::default();
            for (index, immediate) in &constants {
                let value = func.alloc_value(Value::Immediate(immediate.clone()));
                for &arg in &uses[*index] {
                    replacements.insert(arg, value);
                }
            }
            func.replace_uses_canonicalized(&replacements);
            changed = true;
            continue;
        }
        let mut selected: Vec<_> = (0..calls.len()).collect();
        if constants.is_empty() {
            let mut groups = FxHashMap::<Constants, Vec<usize>>::default();
            for (index, site) in calls.iter().enumerate() {
                if !site.constants.is_empty() {
                    groups.entry(site.constants.clone()).or_default().push(index);
                }
            }
            let Some((key, indices)) = groups
                .into_iter()
                .filter(|(_, sites)| sites.len() >= 2)
                .max_by(|a, b| a.1.len().cmp(&b.1.len()).then_with(|| b.1[0].cmp(&a.1[0])))
            else {
                continue;
            };
            constants = key;
            selected = indices;
        }
        let mut candidate = body.clone();
        let uses = candidate.arg_uses();
        let mut replacements = FxHashMap::default();
        for (index, immediate) in &constants {
            let value = candidate.alloc_value(Value::Immediate(immediate.clone()));
            for &arg in &uses[*index] {
                replacements.insert(arg, value);
            }
        }
        // helper(argK, ...) => shared_helper(constantK, ...)
        candidate.replace_uses_canonicalized(&replacements);
        let mut trial = Module::new(module.name);
        let trial_id = trial.add_function(candidate);
        for pass in [
            &sccp::Sccp as &dyn MirPass,
            &egraph::Egraph,
            &check_elim::CheckElim,
            &cfg_simplify::CfgSimplify,
            &dce::Dce,
        ] {
            // Use fresh analyses after each speculative rewrite.
            let _ = pass.run_pass(gcx, &mut trial, &mut ModuleAnalyses::default());
        }
        let candidate = trial.function(trial_id);
        let old = target.code_estimate(body);
        let new = target.code_estimate(candidate);
        let removed_args = candidate.arg_uses().iter().filter(|uses| uses.is_empty()).count();
        let old_call = target.icall(body.params.len(), body.returns.len(), 0);
        let new_call = target.icall(body.params.len() - removed_args, body.returns.len(), 0);
        let count = selected.len() as u32;
        let all_calls = selected.len() == calls.len();
        let before = old.plus(old_call.times(count));
        let after = new.plus(new_call.times(count)).plus(if all_calls {
            crate::target::Cost::ZERO
        } else {
            old
        });
        if after.bytes >= before.bytes || new.gas > old.gas || after.gas > before.gas {
            continue;
        }

        let mut candidate = candidate.clone();
        // Keep one shared body through subsequent inlining passes.
        candidate.attributes.no_inline = true;
        let specialized = if all_calls {
            module.functions[callee] = candidate;
            callee
        } else {
            candidate.name.disambiguator = None;
            module.add_function(candidate)
        };
        // icall generic, constants, args => icall shared_specialization, constants, args
        for index in selected {
            let site = &calls[index];
            let instruction = module.functions[site.caller].inst_mut(site.inst);
            let mut kind = instruction.kind.clone();
            if let InstKind::ICall { function, .. } = &mut kind {
                *function = specialized;
            }
            instruction.replace_kind(kind);
        }
        changed = true;
    }
    changed
}
