//! Remove redundant word masks using facts from every internal call site.
//!
//! Direct calls and tail transfers supply upper bounds on the significant bits
//! of each internal argument. Starting at the unknown 256-bit bound, eight
//! rounds intersect the facts from all callers. Each round is independently
//! sound; recursive arguments do not acquire facts by assuming their own
//! cleanliness. Value evaluation follows a bounded number of SSA definitions,
//! sharing the same opcode semantics as local e-graph width inference.
//!
//! ABI lowering supplies transient bounds for the lazy arguments whose raw
//! calldata words it validates. The same inference runs there before those
//! proofs are discarded; wrapper metadata alone never establishes a bound.
//! Booleans always carry the one-bit bound. Public functions,
//! constructors, and dispatch entries cannot be specialized from direct callers.
//!
//! A contiguous low-bit mask disappears only when the proved bound fits inside
//! it. A truncation followed by a zero extension disappears under the same
//! proof when the original and extended values have the same type. Zero tests
//! of proved-clean truncations compare the original value in its wider type.
//! Instructions stay in place until their uses are redirected; no code is
//! moved or cloned; comparisons may insert a zero-cost extension to keep operand
//! types equal. The pass runs after memory lowering and before
//! final local simplification, when the complete call graph and ABI guards are
//! explicit. Unknown return values and path-dependent bounds remain conservative.
//! Explicit frame-address functions do not receive argument facts: parsed MIR
//! may write into argument homes through those pointers. Masks live across calls
//! are retained for profitability, since replacing their materialized result
//! with an argument changes which values the stack ABI must preserve or spill.
//!
//! Boolean values are canonical by type, so double negation needs no call-graph proof.
//! Raw-word return bounds require every explicit return to fit within one bit;
//! tail calls and unknown recursive results remain conservative. These transient
//! proofs remove normalization from comparisons and zero-extended results without
//! changing raw call signatures or assuming that a Solidity boolean is clean.

use super::egraph::max_bits_with_args;
use crate::mir::{
    ArgIdx, Callee, Function, FunctionId, Immediate, InstId, InstKind, Instruction, MirType,
    Module, Terminator, Value, ValueId,
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
pub(super) type ArgumentBits = FxHashMap<(FunctionId, ArgIdx), u32>;
pub(super) type ReturnBits = FxHashMap<FunctionId, u32>;

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
        let returns = infer_returns(module);
        let mut changed = false;
        for (id, func) in module.functions.iter_mut_enumerated() {
            changed |=
                cleanup(func, |func, index| argument_bits(func, id, index, &facts), &returns);
        }
        changed
    }
}

/// Removes cleanup using argument bounds proved at the current call boundary.
pub(super) fn cleanup(
    func: &mut Function,
    argument_bits: impl Fn(&Function, ArgIdx) -> u32,
    returns: &ReturnBits,
) -> bool {
    let mut changed = false;
    let argument_bits = |index| argument_bits(func, index);
    let mut replacements = FxHashMap::default();
    let mut zero_tests = Vec::new();
    let mut boolean_comparisons = Vec::new();
    let mut replacement_inputs = FxHashMap::default();
    let mut dead = DenseBitSet::<InstId>::new_empty(func.num_insts());
    for inst in func.instructions() {
        if let Some(inner) = func.inst(inst).kind.zero_test_operand(func)
            && func
                .inst(inst)
                .metadata
                .effect()
                .is_none_or(|effect| effect == func.inst(inst).kind.effect_kind())
            && let Value::Inst(inner) = func.value(inner)
            && let Some(value) = func.inst(*inner).kind.zero_test_operand(func)
            && func.value_ty(value) == Some(crate::mir::MirType::I1)
            && let Some(result) = func.inst_result_value(inst)
        {
            replacements.insert(result, value);
            dead.insert(inst);
        }
        if let InstKind::And(a, b) = func.inst(inst).kind
            && native_effect(func, inst)
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
        if let InstKind::Zext(narrowed) = func.inst(inst).kind
            && native_effect(func, inst)
            && let Value::Inst(truncation) = func.value(narrowed)
            && let InstKind::Trunc(value, bits) = func.inst(*truncation).kind
            && let Some(result) = func.inst_result_value(inst)
            && func.value_ty(result) == func.value_ty(value)
            && max_bits_with_args(func, value, MAX_VALUE_DEPTH, &argument_bits) <= bits
        {
            replacements.insert(result, value);
            dead.insert(inst);
        }
        if let InstKind::Zext(boolean) = func.inst(inst).kind
            && let Some(value) = normalized_word(func, boolean, &argument_bits, returns)
            && let Some(result) = func.inst_result_value(inst)
            && func.value_ty(result) == func.value_ty(value)
            && native_effect(func, inst)
        {
            replacements.insert(result, value);
            replacement_inputs.insert(result, boolean);
            dead.insert(inst);
        }
        if let InstKind::Eq(a, b) | InstKind::Ne(a, b) = func.inst(inst).kind
            && func.value_ty(a) == Some(MirType::I1)
            && func.value_ty(b) == Some(MirType::I1)
            && native_effect(func, inst)
        {
            let left = normalized_word(func, a, &argument_bits, returns);
            let right = normalized_word(func, b, &argument_bits, returns);
            if left.is_some() || right.is_some() {
                boolean_comparisons.push((inst, [(a, left), (b, right)]));
            }
        }
        if let InstKind::Eq(a, b) | InstKind::Ne(a, b) = func.inst(inst).kind
            && let Some(narrowed) = [(a, b), (b, a)]
                .into_iter()
                .find_map(|(value, zero)| (func.value_u64(zero) == Some(0)).then_some(value))
            && let Value::Inst(truncation) = func.value(narrowed)
            && let InstKind::Trunc(value, bits) = func.inst(*truncation).kind
            && max_bits_with_args(func, value, MAX_VALUE_DEPTH, &argument_bits) <= bits
            && func
                .inst(inst)
                .metadata
                .effect()
                .is_none_or(|effect| effect == func.inst(inst).kind.effect_kind())
        {
            zero_tests.push((inst, narrowed, value));
        }
    }
    if !replacements.is_empty() || !zero_tests.is_empty() || !boolean_comparisons.is_empty() {
        let protected = masks_live_across_calls(func);
        zero_tests.retain(|(_, narrowed, _)| !protected.contains(*narrowed));
        boolean_comparisons.retain(|(_, operands)| {
            operands.iter().all(|(value, raw)| raw.is_none() || !protected.contains(*value))
        });
        replacements.retain(|result, _| {
            !protected.contains(*result)
                && replacement_inputs.get(result).is_none_or(|value| !protected.contains(*value))
        });
        for inst in dead.iter().collect::<Vec<_>>() {
            if func.inst_result_value(inst).is_some_and(|value| !replacements.contains_key(&value))
            {
                dead.remove(inst);
            }
        }
    }
    for (inst, _, value) in zero_tests {
        let zero = func
            .alloc_value(Value::Immediate(Immediate::for_type(func.value_ty(value), U256::ZERO)));
        let kind = match func.inst(inst).kind {
            InstKind::Eq(..) => InstKind::Eq(value, zero),
            InstKind::Ne(..) => InstKind::Ne(value, zero),
            _ => unreachable!(),
        };
        func.inst_mut(inst).replace_kind(kind);
        changed = true;
    }
    let mut insertions = FxHashMap::default();
    for (inst, operands) in boolean_comparisons {
        let [left, right] = operands.map(|(value, raw)| {
            raw.unwrap_or_else(|| {
                let (extension, value) = func.alloc_value_inst(
                    Instruction::new(InstKind::Zext(value), Some(MirType::I256))
                        .with_debug_info_dropped(),
                );
                insertions.insert(inst, extension);
                value
            })
        });
        let kind = match func.inst(inst).kind {
            InstKind::Eq(..) => InstKind::Eq(left, right),
            InstKind::Ne(..) => InstKind::Ne(left, right),
            _ => unreachable!(),
        };
        func.inst_mut(inst).replace_kind(kind);
        changed = true;
    }
    if !insertions.is_empty() {
        for block in &mut func.blocks {
            let mut index = 0;
            while index < block.instructions.len() {
                if let Some(extension) = insertions.remove(&block.instructions[index]) {
                    block.instructions.insert(index, extension);
                    index += 1;
                }
                index += 1;
            }
        }
    }
    if !replacements.is_empty() {
        // result = and value, low_mask; use result -> use value
        // result = zext (trunc value, bits); use result -> use value
        // result = eq (eq boolean, false), false; use result -> use boolean
        // NOTE: Removed cleanup instructions lose their debug checkpoints;
        // their source locations must not be assigned to the replacement value.
        func.for_each_instruction_mut(|_, inst| {
            inst.rewrite_operands(|value| {
                *value = utils::resolve_replacement(*value, &replacements);
            });
        });
        for block in &mut func.blocks {
            block
                .instructions
                .retain(|&inst| inst.index() >= dead.domain_size() || !dead.contains(inst));
            if let Some(term) = &mut block.terminator {
                utils::replace_terminator_uses_canonicalized(term, &replacements);
            }
        }
        changed = true;
    }
    changed
}

fn native_effect(func: &Function, inst: InstId) -> bool {
    let instruction = func.inst(inst);
    instruction.metadata.effect().is_none_or(|effect| effect == instruction.kind.effect_kind())
}

fn normalized_word(
    func: &Function,
    value: ValueId,
    argument_bits: &impl Fn(ArgIdx) -> u32,
    returns: &ReturnBits,
) -> Option<ValueId> {
    let Value::Inst(inst) = func.value(value) else { return None };
    let InstKind::Ne(a, b) = func.inst(*inst).kind else { return None };
    if !native_effect(func, *inst) {
        return None;
    }
    let word = [(a, b), (b, a)]
        .into_iter()
        .find_map(|(word, zero)| (func.value_u64(zero) == Some(0)).then_some(word))?;
    if func.value_ty(word) != Some(MirType::I256) {
        return None;
    }
    let bits = if let Value::Inst(call) = func.value(word)
        && let InstKind::ICall { function: Callee::Function(callee), .. } = func.inst(*call).kind
    {
        returns.get(&callee).copied().unwrap_or(256)
    } else {
        max_bits_with_args(func, word, MAX_VALUE_DEPTH, argument_bits)
    };
    (bits <= 1).then_some(word)
}

/// Proves raw-word return widths without assuming facts about recursive calls.
pub(super) fn infer_returns(module: &Module) -> ReturnBits {
    module
        .functions
        .iter_enumerated()
        .filter_map(|(id, func)| {
            if func.return_components() != [MirType::I256]
                || func
                    .blocks
                    .iter()
                    .any(|block| matches!(block.terminator, Some(Terminator::TailCall { .. })))
            {
                return None;
            }
            let mut widest = None;
            for block in &func.blocks {
                if let Some(Terminator::Return { values }) = &block.terminator {
                    let [value] = values.as_slice() else { return None };
                    let bits = max_bits_with_args(func, *value, MAX_VALUE_DEPTH, &|_| 256);
                    widest = Some(widest.unwrap_or(0).max(bits));
                }
            }
            widest.filter(|&bits| bits <= 1).map(|bits| (id, bits))
        })
        .collect()
}

pub(super) fn infer_arguments(module: &Module) -> ArgumentBits {
    infer_arguments_with(module, argument_bits)
}

pub(super) fn infer_arguments_with(
    module: &Module,
    argument_bits: impl Fn(&Function, FunctionId, ArgIdx, &ArgumentBits) -> u32,
) -> ArgumentBits {
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
            if let InstKind::ICall {
                function: crate::mir::Callee::Function(function), args, ..
            } = &func.inst(inst).kind
            {
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
            let bounds = |index| argument_bits(caller, call.caller, index, &facts);
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

pub(super) fn argument_bits(
    func: &Function,
    id: FunctionId,
    index: ArgIdx,
    facts: &ArgumentBits,
) -> u32 {
    if func.params.get(index) == Some(&crate::mir::MirType::I1) {
        return 1;
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
