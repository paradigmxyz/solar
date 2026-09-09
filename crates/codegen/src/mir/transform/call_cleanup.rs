//! Remove redundant word masks using facts from every internal call site.
//!
//! Direct calls and tail transfers supply upper bounds on the significant bits
//! of each internal argument. Starting at the unknown 256-bit bound, eight
//! rounds intersect the facts from all callers. Each round is independently
//! sound; recursive arguments do not acquire facts by assuming their own
//! cleanliness. Value evaluation follows a bounded number of SSA definitions,
//! sharing the same opcode semantics as local e-graph width inference.
//!
//! External lazy arguments have the canonicality invariant established by ABI
//! lowering: validation loads the raw calldata word separately before the body
//! can use `Value::Arg`. Only after that phase may their retained unsigned or
//! boolean input type seed a bound. Internal nominal types never seed facts;
//! assembly can pass dirty values through those signatures. Public functions,
//! constructors, and dispatch entries cannot be specialized from direct callers.
//!
//! A contiguous low-bit mask disappears only when the proved bound fits inside
//! it. Instructions stay in place until their uses are redirected; no code is
//! inserted, moved, or cloned. The pass runs after memory lowering and before
//! final local simplification, when the complete call graph and ABI guards are
//! explicit. Unknown return values and path-dependent bounds remain conservative.
//! Explicit frame-address functions do not receive argument facts: parsed MIR
//! may write into argument homes through those pointers. Masks live across calls
//! are retained for profitability, since replacing their materialized result
//! with an argument changes which values the stack ABI must preserve or spill.

use super::egraph::max_bits_with_args;
use crate::mir::{
    AbiWordValidator, ArgIdx, Function, FunctionId, InstId, InstKind, MirPhase, Module, Terminator,
    ValueId,
    analysis::Liveness,
    pass::{MirPass, ModuleAnalyses},
    utils,
};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};

/// Removes masks covered by bounds proved across every direct caller.
pub(crate) struct CallCleanup;

const MAX_ROUNDS: usize = 8;
const MAX_VALUE_DEPTH: u32 = 8;
type ArgumentBits = FxHashMap<(FunctionId, ArgIdx), u32>;

struct CallSite<'a> {
    caller: FunctionId,
    callee: FunctionId,
    args: &'a [ValueId],
}

impl MirPass for CallCleanup {
    fn name(&self) -> &'static str {
        "call-cleanup"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut ModuleAnalyses,
    ) -> bool {
        let facts = infer_arguments(module);
        let abi_lowered = module.phase >= MirPhase::Abi;
        let mut changed = false;
        for (id, func) in module.functions.iter_mut_enumerated() {
            let argument_bits = |index| argument_bits(func, id, index, &facts, abi_lowered);
            let mut replacements = FxHashMap::default();
            let mut dead = DenseBitSet::<InstId>::new_empty(func.num_insts());
            for inst in func.instructions() {
                if let InstKind::And(a, b) = func.inst(inst).kind
                    && let Some((value, mask)) = func
                        .value_u256(b)
                        .map(|mask| (a, mask))
                        .or_else(|| func.value_u256(a).map(|mask| (b, mask)))
                    && mask.wrapping_add(U256::ONE) & mask == U256::ZERO
                    && max_bits_with_args(func, value, MAX_VALUE_DEPTH, &argument_bits)
                        <= mask.bit_len() as u32
                    && let Some(result) = func.inst_result_value(inst)
                {
                    replacements.insert(result, value);
                    dead.insert(inst);
                }
            }
            if !replacements.is_empty() {
                let protected = masks_live_across_calls(func);
                replacements.retain(|result, _| !protected.contains(*result));
                for inst in dead.iter().collect::<Vec<_>>() {
                    if func.inst_result_value(inst).is_some_and(|value| protected.contains(value)) {
                        dead.remove(inst);
                    }
                }
            }
            if !replacements.is_empty() {
                // result = and value, low_mask; use result -> use value
                // NOTE: Removed mask instructions lose their debug checkpoints;
                // their source locations must not be assigned to the argument.
                func.for_each_instruction_mut(|_, inst| {
                    inst.rewrite_operands(|value| {
                        *value = utils::resolve_replacement(*value, &replacements);
                    });
                });
                for block in &mut func.blocks {
                    block.instructions.retain(|&inst| !dead.contains(inst));
                    if let Some(term) = &mut block.terminator {
                        utils::replace_terminator_uses_canonicalized(term, &replacements);
                    }
                }
                changed = true;
            }
        }
        changed
    }
}

fn infer_arguments(module: &Module) -> ArgumentBits {
    let mut eligible = DenseBitSet::new_empty(module.functions.len());
    for (id, func) in module.functions.iter_enumerated() {
        if func.selector.is_none()
            && !func.is_public()
            && !func.attributes.is_constructor
            && !func.attributes.is_receive
            && !func.attributes.is_fallback
            && module.dispatch_entry() != Some(id)
            && !func.params.is_empty()
            && !func
                .instructions()
                .any(|inst| matches!(func.inst(inst).kind, InstKind::InternalFrameAddr(_)))
        {
            eligible.insert(id);
        }
    }
    let mut calls = Vec::new();
    for (caller, func) in module.functions.iter_enumerated() {
        for inst in func.instructions() {
            if let InstKind::ICall { function, args, .. } = &func.inst(inst).kind {
                calls.push(CallSite { caller, callee: *function, args });
            }
        }
        for block in &func.blocks {
            if let Some(Terminator::TailCall { function, args }) = &block.terminator {
                calls.push(CallSite { caller, callee: *function, args });
            }
        }
    }
    // A malformed or deliberately incomplete call edge cannot contribute a
    // proof. Exclude the entire target, not just the offending edge.
    for call in &calls {
        if call.args.len() != module.functions[call.callee].params.len() {
            eligible.remove(call.callee);
        }
    }
    calls.retain(|call| eligible.contains(call.callee));

    let mut facts = ArgumentBits::default();
    for _ in 0..MAX_ROUNDS {
        let mut next = ArgumentBits::default();
        let mut seen = DenseBitSet::new_empty(module.functions.len());
        for call in &calls {
            let first = seen.insert(call.callee);
            let caller = &module.functions[call.caller];
            let bounds = |index| {
                argument_bits(caller, call.caller, index, &facts, module.phase >= MirPhase::Abi)
            };
            for (index, &arg) in call.args.iter().enumerate() {
                let key = (call.callee, ArgIdx::new(index));
                if first || next.contains_key(&key) {
                    let bits = max_bits_with_args(caller, arg, MAX_VALUE_DEPTH, &bounds);
                    let bits = bits.max(next.get(&key).copied().unwrap_or(0));
                    if bits < 256 {
                        next.insert(key, bits);
                    } else {
                        next.remove(&key);
                    }
                }
            }
        }
        if next == facts {
            break;
        }
        facts = next;
    }
    facts
}

fn argument_bits(
    func: &Function,
    id: FunctionId,
    index: ArgIdx,
    facts: &ArgumentBits,
    abi_lowered: bool,
) -> u32 {
    if abi_lowered && func.selector.is_some() && func.params.is_empty() {
        // ABI validation establishes this bound on the lazy argument. Raw
        // calldata loads used by the validation itself do not enter this arm.
        return match AbiWordValidator::from_mir_type(func.arg_ty(index)) {
            Some(AbiWordValidator::Unsigned(bits)) => u32::from(bits),
            Some(AbiWordValidator::Bool) => 1,
            _ => 256,
        };
    }
    facts.get(&(id, index)).copied().unwrap_or(256)
}

/// Keeps materialized masks whose identity avoids argument traffic across calls.
fn masks_live_across_calls(func: &Function) -> DenseBitSet<ValueId> {
    let mut protected = DenseBitSet::new_empty(func.num_values());
    if !func.instructions().any(|inst| is_call(&func.inst(inst).kind)) {
        return protected;
    }
    let liveness = Liveness::compute(func);
    for (id, block) in func.blocks.iter_enumerated() {
        let mut live = liveness.live_out(id).clone();
        if let Some(term) = &block.terminator {
            for value in term.operands() {
                live.insert(value);
            }
        }
        for &inst in block.instructions.iter().rev() {
            let instruction = func.inst(inst);
            if is_call(&instruction.kind) {
                for value in live.iter() {
                    protected.insert(value);
                }
            }
            if let Some(value) = func.inst_result_value(inst) {
                live.remove(value);
            }
            for value in instruction.operands() {
                live.insert(value);
            }
        }
    }
    protected
}

fn is_call(kind: &InstKind) -> bool {
    matches!(
        kind,
        InstKind::ICall { .. }
            | InstKind::Call { .. }
            | InstKind::StaticCall { .. }
            | InstKind::DelegateCall { .. }
            | InstKind::CallCode { .. }
            | InstKind::Create(..)
            | InstKind::Create2(..)
    )
}
