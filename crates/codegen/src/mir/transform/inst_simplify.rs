//! Local MIR instruction simplification.
//!
//! This pass removes algebraic no-ops and rewrites a few equivalent EVM
//! instruction patterns before stack scheduling. It is intentionally local and
//! conservative: it only applies identities that are exact for EVM word
//! semantics. Fixed aggregate projections also forward through bounded insertion chains, exposing
//! scalar facts before aggregate lowering without allocating memory or expanding aggregate phis.
//! Masked shifted words fold when a constant OR operand determines every selected bit.
//! Consecutive shifts in the same direction combine constant amounts, capped at the word width.
//!
//! The `const-fold` adapter runs after representation lowering. It removes zero-length memory
//! operations and instructions with constant results, including identities such as `sub x, x`.
//! It keeps other value identities and instruction choices intact to avoid extending the live
//! ranges of nonconstant values before stack scheduling.
//!
//! Safety contract:
//! - do not remove or reorder side effects
//! - replace an instruction with a value only when the equality is exact for all 256-bit EVM words
//! - preserve boolean-only rewrites behind explicit MIR boolean type checks

use crate::mir::{
    Builtin, Callee, Function, Immediate, InstId, InstKind, MirType, Module, Terminator, ToUint,
    Value, ValueId,
    memory::{EvmMemoryLayout, MemoryLayoutPolicy},
    pass::{MirPass, run_function_pass},
    utils as mir_utils,
    utils::eval,
};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};

/// Function pass for local instruction simplification.
pub(crate) struct InstSimplify;

impl MirPass for InstSimplify {
    fn name(&self) -> &'static str {
        "inst-simplify"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let changed = run_function_pass(module, analyses, |func, _| {
            InstSimplifier::new(gcx.sess.opts.evm_version).run_to_fixpoint(func) != 0
        });
        // Exact value rewrites and removed effects keep old call summaries conservative.
        analyses.preserve_call_summaries();
        changed
    }
}

/// Folds constant results without changing instruction choices or forwarding other values.
pub(crate) struct ConstFold;

impl MirPass for ConstFold {
    fn name(&self) -> &'static str {
        "const-fold"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let changed = run_function_pass(module, analyses, |func, _| {
            let mut simplifier = InstSimplifier::new(gcx.sess.opts.evm_version);
            simplifier.constants_only = true;
            simplifier.run_to_fixpoint(func) != 0
        });
        analyses.preserve_call_summaries();
        changed
    }
}

/// Applies local canonicalization before bounded word-expression extraction.
///
/// Retain the upstream semantic, masked-shift, and offset folds alongside
/// ISLE's wider search. Other word identities stay under target cost selection.
/// Control flow stays unchanged so the caller can retain its CFG.
pub(super) fn simplify_before_egraph(func: &mut Function, evm_version: EvmVersion) -> usize {
    let mut simplifier = InstSimplifier::new(evm_version);
    simplifier.preserve_cfg = true;
    simplifier.run_to_fixpoint(func)
}

/// Local MIR instruction simplification pass.
#[derive(Debug)]
struct InstSimplifier {
    /// Number of instructions simplified in the last run.
    simplified_count: usize,
    evm_version: EvmVersion,
    constants_only: bool,
    preserve_cfg: bool,
}

struct RunState {
    replacements: FxHashMap<ValueId, ValueId>,
    dead: DenseBitSet<InstId>,
}

impl RunState {
    fn new(func: &Function) -> Self {
        Self { replacements: FxHashMap::default(), dead: DenseBitSet::new_empty(func.num_insts()) }
    }
}

impl InstSimplifier {
    /// Creates a new instruction simplifier.
    fn new(evm_version: EvmVersion) -> Self {
        Self { simplified_count: 0, evm_version, constants_only: false, preserve_cfg: false }
    }

    fn run_with_state(&mut self, func: &mut Function, state: &mut RunState) -> usize {
        self.simplified_count = 0;

        state.replacements.clear();
        state.dead.clear();
        let block_ids = func.blocks.indices();

        for block_id in block_ids {
            let instruction_count = func.blocks[block_id].instructions.len();
            for index in 0..instruction_count {
                let inst_id = func.blocks[block_id].instructions[index];
                loop {
                    let kind = func.inst(inst_id).kind.clone();
                    if self.preserve_cfg
                        && !matches!(
                            kind,
                            InstKind::InsertValue { .. }
                                | InstKind::ExtractValue { .. }
                                | InstKind::MemoryObjectFromPtr { .. }
                                | InstKind::WordCast { .. }
                                | InstKind::Trunc160(_)
                                | InstKind::CheckedBinary { .. }
                                | InstKind::ValidateAbi { .. }
                                | InstKind::And(..)
                                | InstKind::Shr(..)
                                | InstKind::Sub(..)
                                | InstKind::ICall { function: Callee::Builtin(_), .. }
                        )
                    {
                        break;
                    }

                    if self.is_dead_noop_inst(func, &kind, &state.replacements) {
                        tracing::trace!(
                            target: "solar::codegen::mir::inst_simplify",
                            function = %func.name,
                            action = "delete",
                            instruction = %kind,
                            "mir_inst_simplify"
                        );
                        state.dead.insert(inst_id);
                        self.simplified_count += 1;
                        break;
                    }

                    if !self.constants_only
                        && let Some(new_kind) = self.rewrite_inst(func, &kind, &state.replacements)
                        && new_kind.scalar_types_match(func, func.inst(inst_id).result_ty)
                    {
                        tracing::trace!(
                            target: "solar::codegen::mir::inst_simplify",
                            function = %func.name,
                            action = "rewrite",
                            input = %kind,
                            output = %new_kind,
                            "mir_inst_simplify"
                        );
                        func.inst_mut(inst_id).replace_kind(new_kind);
                        self.simplified_count += 1;
                        continue;
                    }

                    let Some(result) = func.inst_result_value(inst_id) else {
                        break;
                    };
                    let Some(replacement) = self.simplify_inst(func, &kind, &state.replacements)
                    else {
                        break;
                    };
                    let replacement =
                        mir_utils::resolve_replacement(replacement, &state.replacements);
                    if self.constants_only && func.value(replacement).as_immediate().is_none() {
                        break;
                    }
                    if func.value_ty(result) != func.value_ty(replacement) {
                        break;
                    }
                    if replacement != result {
                        tracing::trace!(
                            target: "solar::codegen::mir::inst_simplify",
                            function = %func.name,
                            action = "replace",
                            instruction = %kind,
                            ?result,
                            ?replacement,
                            "mir_inst_simplify"
                        );
                        state.replacements.insert(result, replacement);
                        state.dead.insert(inst_id);
                        self.simplified_count += 1;
                    }
                    break;
                }
            }
        }

        if !state.replacements.is_empty() {
            func.replace_uses_canonicalized(&state.replacements);
        }
        if !state.dead.is_empty() {
            for block in func.blocks.iter_mut() {
                block.instructions.retain(|&id| !state.dead.contains(id));
            }
        }
        if !self.constants_only && !self.preserve_cfg {
            self.simplified_count += self.rewrite_terminators(func, &state.replacements);
        }

        self.simplified_count
    }

    /// Runs instruction simplification until no more changes are found.
    fn run_to_fixpoint(&mut self, func: &mut Function) -> usize {
        let mut total = 0;
        let mut state = RunState::new(func);
        for round in 1.. {
            let simplified = self.run_with_state(func, &mut state);
            tracing::trace!(
                target: "solar::codegen::mir::inst_simplify",
                function = %func.name,
                round,
                simplified,
                "mir_inst_simplify_round"
            );
            if simplified == 0 {
                break;
            }
            total += simplified;
        }
        total
    }

    fn rewrite_inst(
        &mut self,
        func: &mut Function,
        kind: &InstKind,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> Option<InstKind> {
        let resolve = |value| mir_utils::resolve_replacement(value, replacements);

        if let InstKind::Eq(a, b) | InstKind::Ne(a, b) = *kind {
            let (a, b) = (resolve(a), resolve(b));
            let equal = matches!(kind, InstKind::Eq(..));
            let compare = |a, b| if equal { InstKind::Eq(a, b) } else { InstKind::Ne(a, b) };
            if Self::is_zero(func, a) && !Self::is_zero(func, b) {
                // 0 == x -> x == 0; 0 != x -> x != 0
                return Some(compare(b, a));
            }
            if let Value::Inst(id) = func.value(a)
                && let InstKind::WordCast(inner) = func.inst(*id).kind
                && Self::is_bool_value(func, inner)
                && let Some(constant) = func.value_u256(b)
                && constant <= U256::ONE
            {
                // word_cast boolean == i256 0 -> boolean == false
                let constant = Self::imm_bool(func, !constant.is_zero());
                return Some(compare(inner, constant));
            }
            if Self::is_zero(func, b)
                && let Value::Inst(id) = func.value(a)
            {
                let inner = func.inst(*id).kind.clone();
                if let InstKind::Eq(x, y) | InstKind::Ne(x, y) = inner {
                    // (x == y) == false -> x != y
                    // (x != y) == false -> x == y
                    let inner_equal = matches!(inner, InstKind::Eq(..));
                    return Some(if inner_equal != equal {
                        InstKind::Eq(x, y)
                    } else {
                        InstKind::Ne(x, y)
                    });
                }
            }
        }

        match kind {
            // check (condition == 0), polarity -> check condition, !polarity
            InstKind::ICall {
                function: Callee::Builtin(Builtin::Check { is_zero, failure }),
                args,
            } => {
                let condition = Self::zero_test_operand(func, resolve(args[0]))?;
                Some(InstKind::builtin(
                    Builtin::Check { is_zero: !is_zero, failure: *failure },
                    [condition],
                ))
            }
            InstKind::MemoryObjectData(object, kind)
                if EvmMemoryLayout::object_data_offset(*kind) == 0 =>
            {
                Some(InstKind::WordCast(resolve(*object)))
            }
            InstKind::MemoryObjectFieldAddr { object, layout, field }
                if EvmMemoryLayout::field_offset(*layout, *field) == Some(0) =>
            {
                Some(InstKind::WordCast(resolve(*object)))
            }
            InstKind::MemoryObjectElementAddr { object, layout, index }
                if EvmMemoryLayout::object_data_offset(layout.kind()) == 0
                    && Self::is_zero(func, resolve(*index)) =>
            {
                Some(InstKind::WordCast(resolve(*object)))
            }
            InstKind::Add(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                self.rewrite_add(func, a, b)
            }
            InstKind::Sub(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                self.rewrite_sub(func, a, b)
            }
            InstKind::Mul(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                self.rewrite_mul(func, a, b)
            }
            InstKind::Div(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                self.rewrite_div(func, a, b)
            }
            InstKind::Mod(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                self.rewrite_mod(func, a, b)
            }
            InstKind::Exp(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if Self::is_const(func, b, U256::from(2)) {
                    Some(InstKind::Mul(a, a))
                } else {
                    None
                }
            }
            InstKind::And(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                self.rewrite_and(func, a, b)
            }
            InstKind::Xor(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if Self::is_all_ones(func, a) {
                    Some(InstKind::Not(b))
                } else if Self::is_all_ones(func, b) {
                    Some(InstKind::Not(a))
                } else {
                    None
                }
            }
            InstKind::Eq(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if a != b && Self::is_zero(func, a) && !Self::is_zero(func, b) {
                    Some(InstKind::Eq(b, a))
                } else if Self::is_zero(func, b)
                    && let Some(input) = Self::clz_operand(func, a)
                {
                    Some(InstKind::SLt(input, b))
                } else if let Some((_, input, constant)) = Self::clz_const_operand(func, a, b) {
                    if constant == U256::from(255) {
                        let one = Self::imm(func, U256::from(1));
                        Some(InstKind::Eq(input, one))
                    } else if constant == U256::from(256) {
                        Some(InstKind::Eq(input, Self::imm(func, U256::ZERO)))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            // `a < 1` is `a == 0` for unsigned comparisons.
            InstKind::Lt(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if Self::is_zero(func, a) {
                    Some(InstKind::Ne(b, a))
                } else if let Some(input) = Self::clz_operand(func, a)
                    && Self::is_const(func, b, U256::from(256))
                {
                    let zero = Self::imm(func, U256::ZERO);
                    Some(InstKind::Ne(input, zero))
                } else if let Some(input) = Self::clz_operand(func, b)
                    && Self::is_const(func, a, U256::from(255))
                {
                    Some(InstKind::Eq(input, Self::imm(func, U256::ZERO)))
                } else {
                    Self::is_one(func, b).then_some(InstKind::Eq(a, Self::imm(func, U256::ZERO)))
                }
            }
            // `1 > b` is `b == 0` for unsigned comparisons.
            InstKind::Gt(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if Self::is_zero(func, b) {
                    Some(InstKind::Ne(a, b))
                } else if let Some(input) = Self::clz_operand(func, a)
                    && Self::is_const(func, b, U256::from(255))
                {
                    Some(InstKind::Eq(input, Self::imm(func, U256::ZERO)))
                } else if let Some(input) = Self::clz_operand(func, b)
                    && Self::is_const(func, a, U256::from(256))
                {
                    let zero = Self::imm(func, U256::ZERO);
                    Some(InstKind::Ne(input, zero))
                } else {
                    Self::is_one(func, a).then_some(InstKind::Eq(b, Self::imm(func, U256::ZERO)))
                }
            }
            InstKind::Shl(shift, value) | InstKind::Sar(shift, value) => {
                Self::rewrite_nested_shift(func, kind, resolve(*shift), resolve(*value))
            }
            InstKind::Shr(shift, value) => {
                let (shift, value) = (resolve(*shift), resolve(*value));
                Self::rewrite_nested_shift(func, kind, shift, value).or_else(|| {
                    if Self::is_const(func, shift, U256::from(8)) {
                        Self::clz_operand(func, value)
                            .map(|input| InstKind::Eq(input, Self::imm(func, U256::ZERO)))
                    } else {
                        None
                    }
                })
            }
            InstKind::Byte(index, value) => {
                let (index, value) = (resolve(*index), resolve(*value));
                if Self::is_const(func, index, U256::from(30)) {
                    Self::clz_operand(func, value)
                        .map(|input| InstKind::Eq(input, Self::imm(func, U256::ZERO)))
                } else {
                    None
                }
            }
            InstKind::Select(condition, then_value, else_value) => {
                let (condition, then_value, else_value) =
                    (resolve(*condition), resolve(*then_value), resolve(*else_value));
                if Self::is_bool_value(func, condition)
                    && Self::is_zero(func, then_value)
                    && Self::is_one(func, else_value)
                {
                    Some(InstKind::Eq(condition, Self::imm_bool(func, false)))
                } else {
                    None
                }
            }
            InstKind::Balance(addr) => {
                let addr = resolve(*addr);
                (self.evm_version.has_self_balance() && Self::is_current_address(func, addr))
                    .then_some(InstKind::SelfBalance)
            }
            _ => None,
        }
    }

    fn simplify_inst(
        &mut self,
        func: &mut Function,
        kind: &InstKind,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> Option<ValueId> {
        let resolve = |value| mir_utils::resolve_replacement(value, replacements);

        if let Some(value) = Self::const_fold_inst(func, kind, replacements) {
            return Some(value);
        }

        match kind {
            // extract_value(insert_value aggregate, index, value), index -> value
            InstKind::ExtractValue { ty, aggregate, index } => {
                let mut aggregate = resolve(*aggregate);
                // Bound work for long tuples and malformed cycles in unreachable blocks.
                for _ in 0..16 {
                    let Value::Inst(id) = func.value(aggregate) else { break };
                    let InstKind::InsertValue {
                        ty: inserted_ty,
                        aggregate: base,
                        index: field,
                        value,
                    } = &func.inst(*id).kind
                    else {
                        break;
                    };
                    if inserted_ty != ty {
                        break;
                    }
                    if field == index {
                        return Some(resolve(*value));
                    }
                    aggregate = resolve(*base);
                }
                None
            }

            InstKind::Add(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if Self::is_zero(func, b) {
                    Some(a)
                } else if Self::is_zero(func, a) {
                    Some(b)
                } else {
                    // add base, (sub end, base) -> end
                    [(a, b), (b, a)].into_iter().find_map(|(base, difference)| {
                        if let Value::Inst(inst) = func.value(difference)
                            && let InstKind::Sub(end, start) = func.inst(*inst).kind
                            && resolve(start) == base
                        {
                            Some(resolve(end))
                        } else {
                            None
                        }
                    })
                }
            }
            InstKind::Sub(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if Self::is_zero(func, b) {
                    Some(a)
                } else if a == b {
                    Some(Self::imm(func, U256::ZERO))
                } else if let Some((a_base, a_offset)) = Self::offset_base(func, a)
                    && let Some((b_base, b_offset)) = Self::offset_base(func, b)
                    && resolve(a_base) == resolve(b_base)
                {
                    // sub (base + a_offset), (base + b_offset) -> a_offset - b_offset
                    Some(Self::imm(func, a_offset.wrapping_sub(b_offset)))
                } else if let Value::Inst(inst) = func.value(a)
                    && let InstKind::Add(lhs, rhs) = func.inst(*inst).kind
                {
                    // sub (add base, offset), base -> offset
                    let (lhs, rhs) = (resolve(lhs), resolve(rhs));
                    if lhs == b {
                        Some(rhs)
                    } else if rhs == b {
                        Some(lhs)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            InstKind::Mul(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if Self::is_zero(func, a) || Self::is_one(func, b) {
                    Some(a)
                } else if Self::is_zero(func, b) || Self::is_one(func, a) {
                    Some(b)
                } else {
                    None
                }
            }
            InstKind::Div(a, b) | InstKind::SDiv(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if Self::is_zero(func, a) || Self::is_one(func, b) {
                    Some(a)
                } else if Self::is_zero(func, b)
                    || (matches!(kind, InstKind::Div(_, _))
                        && Self::clz_operand(func, a).is_some()
                        && func.value_u256(b).is_some_and(|b| b > U256::from(256)))
                {
                    Some(Self::imm(func, U256::ZERO))
                } else {
                    None
                }
            }
            InstKind::Mod(a, b) | InstKind::SMod(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if Self::is_zero(func, a) {
                    Some(a)
                } else if Self::is_zero(func, b) || Self::is_one(func, b) || a == b {
                    Some(Self::imm(func, U256::ZERO))
                } else if matches!(kind, InstKind::Mod(_, _))
                    && Self::clz_operand(func, a).is_some()
                    && func.value_u256(b).is_some_and(|b| b > U256::from(256))
                {
                    Some(a)
                } else {
                    None
                }
            }
            InstKind::Exp(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if Self::is_zero(func, b) {
                    Some(Self::imm(func, U256::from(1)))
                } else if Self::is_one(func, a) || Self::is_one(func, b) {
                    Some(a)
                } else {
                    None
                }
            }
            InstKind::And(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if let Some((clz, _, mask)) = Self::clz_const_operand(func, a, b)
                    && mask & U256::from(0x1ff) == U256::from(0x1ff)
                {
                    Some(clz)
                } else if a == b
                    || Self::is_zero(func, a)
                    || Self::is_all_ones(func, b)
                    || (Self::is_uint160_mask(func, b) && Self::is_clean_address(func, a))
                {
                    Some(a)
                } else if Self::is_zero(func, b)
                    || Self::is_all_ones(func, a)
                    || (Self::is_uint160_mask(func, a) && Self::is_clean_address(func, b))
                {
                    Some(b)
                } else if Self::is_bitwise_complement_pair(func, a, b) {
                    Some(Self::imm(func, U256::ZERO))
                } else {
                    // and (or (shift amount, value), constant), mask -> constant & mask
                    Self::masked_shifted_constant(func, a, b)
                        .map(|constant| Self::imm(func, constant))
                }
            }
            InstKind::Or(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if a == b || Self::is_all_ones(func, a) || Self::is_zero(func, b) {
                    Some(a)
                } else if Self::is_all_ones(func, b) || Self::is_zero(func, a) {
                    Some(b)
                } else if Self::is_bitwise_complement_pair(func, a, b) {
                    Some(Self::imm(func, U256::MAX))
                } else {
                    None
                }
            }
            InstKind::Xor(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if a == b {
                    Some(Self::imm(func, U256::ZERO))
                } else if Self::is_zero(func, a) {
                    Some(b)
                } else if Self::is_zero(func, b) {
                    Some(a)
                } else if Self::is_bitwise_complement_pair(func, a, b) {
                    Some(Self::imm(func, U256::MAX))
                } else {
                    None
                }
            }
            InstKind::Not(a) => {
                let a = resolve(*a);
                Self::not_operand(func, a)
                    .or_else(|| func.value_u256(a).map(|v| Self::imm(func, !v)))
            }
            InstKind::Clz(a) => {
                let a = resolve(*a);
                Self::has_known_sign_bit(func, a).then(|| Self::imm(func, U256::ZERO))
            }
            InstKind::Shl(a, b) | InstKind::Shr(a, b) | InstKind::Sar(a, b) => {
                let (shift, value) = (resolve(*a), resolve(*b));
                if Self::is_zero(func, shift) || Self::is_zero(func, value) {
                    Some(value)
                } else if (matches!(kind, InstKind::Shr(_, _))
                    && Self::clz_operand(func, value).is_some()
                    && func.value_u256(shift).is_some_and(|shift| shift >= U256::from(9)))
                    || (!matches!(kind, InstKind::Sar(_, _))
                        && func.value_u256(shift).is_some_and(|shift| shift >= U256::from(256)))
                {
                    Some(Self::imm(func, U256::ZERO))
                } else if !self.constants_only
                    && matches!(kind, InstKind::Shr(_, _))
                    && let Some(base) = Self::unshift_clean_address(func, shift, value)
                {
                    // shr amount, (or (shl amount, address), constant) -> address
                    Some(base)
                } else {
                    None
                }
            }
            InstKind::Byte(a, b) => {
                let (index, value) = (resolve(*a), resolve(*b));
                (Self::is_zero(func, value)
                    || func.value_u256(index).is_some_and(|index| {
                        index >= U256::from(32)
                            || (index < U256::from(30) && Self::clz_operand(func, value).is_some())
                    }))
                .then(|| Self::imm(func, U256::ZERO))
            }
            // trunc i160, (word_cast narrow) -> narrow
            InstKind::Trunc160(value) => {
                let value = resolve(*value);
                if let Value::Inst(id) = func.value(value)
                    && let InstKind::WordCast(inner) = func.inst(*id).kind
                    && func.value_ty(inner) == Some(crate::mir::MirType::I160)
                {
                    Some(inner)
                } else {
                    None
                }
            }
            InstKind::WordCast(value) => {
                let value = resolve(*value);
                if func.value_ty(value) == Some(crate::mir::MirType::I256) {
                    Some(value)
                } else if let Value::Inst(id) = func.value(value)
                    && let InstKind::MemoryObjectFromPtr { ptr, .. } = func.inst(*id).kind
                {
                    Some(ptr)
                } else {
                    None
                }
            }
            InstKind::Ne(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if a == b {
                    Some(Self::imm_bool(func, false))
                } else if Self::is_bool_value(func, a) && Self::is_zero(func, b) {
                    Some(a)
                } else if Self::is_bool_value(func, b) && Self::is_zero(func, a) {
                    Some(b)
                } else {
                    None
                }
            }
            InstKind::Eq(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if a == b {
                    Some(Self::imm_bool(func, true))
                } else if Self::clz_const_operand(func, a, b)
                    .is_some_and(|(_, _, constant)| constant > U256::from(256))
                {
                    Some(Self::imm_bool(func, false))
                } else if Self::is_bool_value(func, a) && Self::is_one(func, b) {
                    Some(a)
                } else if Self::is_bool_value(func, b) && Self::is_one(func, a) {
                    Some(b)
                } else {
                    None
                }
            }
            InstKind::Lt(a, b) | InstKind::Gt(a, b) | InstKind::SLt(a, b) | InstKind::SGt(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if let Some(value) = Self::fold_clz_range_comparison(func, kind, a, b) {
                    Some(Self::imm_bool(func, value))
                } else if a == b {
                    Some(Self::imm_bool(func, false))
                } else {
                    match kind {
                        InstKind::Lt(_, _) if Self::is_zero(func, b) => {
                            Some(Self::imm_bool(func, false))
                        }
                        InstKind::Lt(_, _)
                            if Self::is_zero(func, a) && Self::is_bool_value(func, b) =>
                        {
                            Some(b)
                        }
                        InstKind::Gt(_, _) if Self::is_zero(func, a) => {
                            Some(Self::imm_bool(func, false))
                        }
                        InstKind::Gt(_, _)
                            if Self::is_zero(func, b) && Self::is_bool_value(func, a) =>
                        {
                            Some(a)
                        }
                        InstKind::Lt(_, _)
                            if Self::is_bool_value(func, a)
                                && func
                                    .value_u256(b)
                                    .is_some_and(|constant| constant > U256::from(1)) =>
                        {
                            Some(Self::imm_bool(func, true))
                        }
                        InstKind::Lt(_, _)
                            if Self::is_bool_value(func, b)
                                && func
                                    .value_u256(a)
                                    .is_some_and(|constant| constant >= U256::from(1)) =>
                        {
                            Some(Self::imm_bool(func, false))
                        }
                        InstKind::Gt(_, _)
                            if Self::is_bool_value(func, b)
                                && func
                                    .value_u256(a)
                                    .is_some_and(|constant| constant > U256::from(1)) =>
                        {
                            Some(Self::imm_bool(func, true))
                        }
                        InstKind::Gt(_, _)
                            if Self::is_bool_value(func, a)
                                && func
                                    .value_u256(b)
                                    .is_some_and(|constant| constant >= U256::from(1)) =>
                        {
                            Some(Self::imm_bool(func, false))
                        }
                        _ => None,
                    }
                }
            }
            InstKind::AddMod(a, b, n) => {
                let (a, b, n) = (resolve(*a), resolve(*b), resolve(*n));
                if Self::is_zero(func, n)
                    || Self::is_one(func, n)
                    || (Self::is_zero(func, a) && Self::is_zero(func, b))
                {
                    Some(Self::imm(func, U256::ZERO))
                } else {
                    None
                }
            }
            InstKind::MulMod(a, b, n) => {
                let (a, b, n) = (resolve(*a), resolve(*b), resolve(*n));
                if Self::is_zero(func, n)
                    || Self::is_one(func, n)
                    || Self::is_zero(func, a)
                    || Self::is_zero(func, b)
                {
                    Some(Self::imm(func, U256::ZERO))
                } else {
                    None
                }
            }
            InstKind::SignExtend(a, b) => {
                let (byte, value) = (resolve(*a), resolve(*b));
                if Self::is_zero(func, value)
                    || func.value_u256(byte).is_some_and(|byte| byte >= U256::from(31))
                    || (Self::clz_operand(func, value).is_some()
                        && func.value_u256(byte).is_some_and(|byte| byte >= U256::from(1)))
                {
                    Some(value)
                } else {
                    None
                }
            }
            InstKind::Select(condition, then_value, else_value) => {
                let (condition, then_value, else_value) =
                    (resolve(*condition), resolve(*then_value), resolve(*else_value));
                if Self::is_one(func, condition) {
                    Some(then_value)
                } else if Self::is_zero(func, condition) {
                    Some(else_value)
                } else if Self::same_value(func, then_value, else_value) {
                    Some(then_value)
                } else if Self::is_bool_value(func, condition)
                    && Self::is_one(func, then_value)
                    && Self::is_zero(func, else_value)
                {
                    Some(condition)
                } else {
                    None
                }
            }
            InstKind::Phi(incoming) => {
                let &(_, first) = incoming.first()?;
                let first = resolve(first);
                incoming
                    .iter()
                    .all(|&(_, value)| Self::same_value(func, resolve(value), first))
                    .then_some(first)
            }
            _ => None,
        }
    }

    fn const_fold_inst(
        func: &mut Function,
        kind: &InstKind,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> Option<ValueId> {
        let resolve = |value| mir_utils::resolve_replacement(value, replacements);
        if let InstKind::Select(condition, then_value, else_value) = *kind {
            let condition = func.value_u256(resolve(condition))?;
            return Some(if condition.is_zero() {
                resolve(else_value)
            } else {
                resolve(then_value)
            });
        }

        let value = eval::eval_inst(kind, |value| func.value_u256(resolve(value)).ok_or(()))
            .ok()
            .flatten()?;
        match kind {
            InstKind::Lt(..)
            | InstKind::Gt(..)
            | InstKind::SLt(..)
            | InstKind::SGt(..)
            | InstKind::Eq(..)
            | InstKind::Ne(..) => Some(Self::imm_bool(func, !value.is_zero())),
            _ => Some(Self::imm(func, value)),
        }
    }

    fn is_dead_noop_inst(
        &self,
        func: &Function,
        kind: &InstKind,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> bool {
        let resolve = |value| mir_utils::resolve_replacement(value, replacements);
        match kind {
            // require a known passing condition, payload -> nothing
            InstKind::ICall { function: Callee::Builtin(Builtin::Require(_)), args } => {
                func.value_u256(resolve(args[0])).is_some_and(|condition| !condition.is_zero())
            }
            // check a known passing condition -> nothing
            InstKind::ICall { function: Callee::Builtin(Builtin::Check { is_zero, .. }), args } => {
                func.value_u256(resolve(args[0]))
                    .is_some_and(|condition| condition.is_zero() != *is_zero)
            }
            InstKind::MCopy(_, _, size)
            | InstKind::CalldataCopy(_, _, size)
            | InstKind::DataCopy(_, _, size)
            | InstKind::CodeCopy(_, _, size) => Self::is_zero(func, resolve(*size)),
            InstKind::ReturnDataCopy(_, offset, size) => {
                Self::is_zero(func, resolve(*offset)) && Self::is_zero(func, resolve(*size))
            }
            _ => false,
        }
    }

    fn rewrite_add(&self, func: &mut Function, a: ValueId, b: ValueId) -> Option<InstKind> {
        if Self::is_zero(func, a) || Self::is_zero(func, b) {
            return None;
        }
        match (func.value_u256(a), func.value_u256(b)) {
            (None, Some(offset)) => Self::offset_base(func, a).map(|(base, existing)| {
                self.add_offset_kind(func, base, existing.wrapping_add(offset))
            }),
            (Some(offset), None) => Self::offset_base(func, b)
                .map(|(base, existing)| {
                    self.add_offset_kind(func, base, existing.wrapping_add(offset))
                })
                .or(Some(InstKind::Add(b, a))),
            _ => None,
        }
    }

    fn rewrite_sub(&self, func: &mut Function, a: ValueId, b: ValueId) -> Option<InstKind> {
        let offset = func.value_u256(b)?;
        let (base, existing) = Self::offset_base(func, a)?;
        Some(self.add_offset_kind(func, base, existing.wrapping_sub(offset)))
    }

    fn rewrite_mul(&self, func: &mut Function, a: ValueId, b: ValueId) -> Option<InstKind> {
        if Self::is_zero(func, a)
            || Self::is_zero(func, b)
            || Self::is_one(func, a)
            || Self::is_one(func, b)
        {
            return None;
        }
        if func.value_u256(a).is_some() && func.value_u256(b).is_none() {
            return Some(InstKind::Mul(b, a));
        }
        if !self.evm_version.has_bitwise_shifting() {
            return None;
        }
        let (value, constant) = Self::const_operand(func, a, b)?;
        let shift = Self::power_of_two_shift(constant)?;
        if shift.is_zero() {
            return None;
        }
        let shift = Self::imm(func, shift);
        Some(InstKind::Shl(shift, value))
    }

    fn rewrite_div(&self, func: &mut Function, a: ValueId, b: ValueId) -> Option<InstKind> {
        if !self.evm_version.has_bitwise_shifting() {
            return None;
        }
        let shift = Self::power_of_two_shift(func.value_u256(b)?)?;
        if shift.is_zero() {
            return None;
        }
        let shift = Self::imm(func, shift);
        Some(InstKind::Shr(shift, a))
    }

    fn rewrite_mod(&self, func: &mut Function, a: ValueId, b: ValueId) -> Option<InstKind> {
        let constant = func.value_u256(b)?;
        let shift = Self::power_of_two_shift(constant)?;
        if shift.is_zero() {
            return None;
        }
        let mask = Self::imm(func, constant - U256::from(1));
        Some(InstKind::And(a, mask))
    }

    fn rewrite_and(&self, func: &mut Function, a: ValueId, b: ValueId) -> Option<InstKind> {
        if a == b
            || Self::is_zero(func, a)
            || Self::is_zero(func, b)
            || Self::is_all_ones(func, a)
            || Self::is_all_ones(func, b)
            || (Self::is_uint160_mask(func, a) && Self::is_clean_address(func, b))
            || (Self::is_uint160_mask(func, b) && Self::is_clean_address(func, a))
        {
            return None;
        }
        if func.value_u256(a).is_some() && func.value_u256(b).is_none() {
            return Some(InstKind::And(b, a));
        }
        let (value, mask) = Self::const_operand(func, a, b)?;
        let (base, existing_mask) = Self::and_mask_base(func, value)?;
        let combined = Self::imm(func, mask & existing_mask);
        Some(InstKind::And(base, combined))
    }

    fn add_offset_kind(&self, func: &mut Function, base: ValueId, offset: U256) -> InstKind {
        let offset = Self::imm(func, offset);
        InstKind::Add(base, offset)
    }

    fn const_operand(func: &Function, a: ValueId, b: ValueId) -> Option<(ValueId, U256)> {
        if let Some(constant) = func.value_u256(b) {
            Some((a, constant))
        } else {
            func.value_u256(a).map(|constant| (b, constant))
        }
    }

    fn clz_const_operand(
        func: &Function,
        a: ValueId,
        b: ValueId,
    ) -> Option<(ValueId, ValueId, U256)> {
        let (clz, constant) = Self::const_operand(func, a, b)?;
        let input = Self::clz_operand(func, clz)?;
        Some((clz, input, constant))
    }

    fn clz_operand(func: &Function, value: ValueId) -> Option<ValueId> {
        let Value::Inst(inst_id) = func.value(value) else { return None };
        match func.inst(*inst_id).kind {
            InstKind::Clz(input) => Some(input),
            _ => None,
        }
    }

    fn has_known_sign_bit(func: &Function, value: ValueId) -> bool {
        if let Some(value) = func.value_u256(value) {
            return value.bit(255);
        }
        let Value::Inst(inst_id) = func.value(value) else { return false };
        match func.inst(*inst_id).kind {
            InstKind::Or(a, b) => {
                Self::has_known_sign_bit(func, a) || Self::has_known_sign_bit(func, b)
            }
            InstKind::Sar(_, value) => Self::has_known_sign_bit(func, value),
            _ => false,
        }
    }

    fn fold_clz_range_comparison(
        func: &Function,
        kind: &InstKind,
        a: ValueId,
        b: ValueId,
    ) -> Option<bool> {
        match kind {
            InstKind::Lt(_, _) if Self::clz_operand(func, a).is_some() => {
                func.value_u256(b).and_then(|b| (b > U256::from(256)).then_some(true))
            }
            InstKind::Lt(_, _) if Self::clz_operand(func, b).is_some() => {
                func.value_u256(a).and_then(|a| (a >= U256::from(256)).then_some(false))
            }
            InstKind::Gt(_, _) if Self::clz_operand(func, a).is_some() => {
                func.value_u256(b).and_then(|b| (b >= U256::from(256)).then_some(false))
            }
            InstKind::Gt(_, _) if Self::clz_operand(func, b).is_some() => {
                func.value_u256(a).and_then(|a| (a > U256::from(256)).then_some(true))
            }
            _ => None,
        }
    }

    fn offset_base(func: &Function, value: ValueId) -> Option<(ValueId, U256)> {
        let Value::Inst(inst_id) = func.value(value) else { return None };
        match func.inst(*inst_id).kind {
            InstKind::Add(a, b) => Self::const_operand(func, a, b),
            InstKind::Sub(a, b) => {
                let offset = func.value_u256(b)?;
                Some((a, U256::ZERO.wrapping_sub(offset)))
            }
            _ => None,
        }
    }

    fn and_mask_base(func: &Function, value: ValueId) -> Option<(ValueId, U256)> {
        let Value::Inst(inst_id) = func.value(value) else { return None };
        match func.inst(*inst_id).kind {
            InstKind::And(a, b) => Self::const_operand(func, a, b),
            _ => None,
        }
    }

    fn rewrite_nested_shift(
        func: &mut Function,
        kind: &InstKind,
        shift: ValueId,
        value: ValueId,
    ) -> Option<InstKind> {
        let Value::Inst(inner) = func.value(value) else { return None };
        let (inner_shift, base) = match (kind, &func.inst(*inner).kind) {
            (InstKind::Shl(_, _), InstKind::Shl(shift, base))
            | (InstKind::Shr(_, _), InstKind::Shr(shift, base))
            | (InstKind::Sar(_, _), InstKind::Sar(shift, base)) => (*shift, *base),
            _ => return None,
        };
        let limit = U256::from(256);
        let total = func.value_u256(shift)?.min(limit) + func.value_u256(inner_shift)?.min(limit);
        let total = Self::imm(func, total.min(limit));
        // shift outer, (shift inner, base) -> shift min(outer + inner, 256), base
        Some(match kind {
            InstKind::Shl(_, _) => InstKind::Shl(total, base),
            InstKind::Shr(_, _) => InstKind::Shr(total, base),
            InstKind::Sar(_, _) => InstKind::Sar(total, base),
            _ => unreachable!(),
        })
    }

    fn unshift_clean_address(func: &Function, shift: ValueId, value: ValueId) -> Option<ValueId> {
        let shift = func.value_u256(shift)?;
        if shift > U256::from(256 - 160) {
            return None;
        }
        let Value::Inst(inst) = func.value(value) else { return None };
        let InstKind::Or(a, b) = func.inst(*inst).kind else { return None };
        let (value, constant) = Self::const_operand(func, a, b)?;
        let Value::Inst(inst) = func.value(value) else { return None };
        let InstKind::Shl(inner_shift, base) = func.inst(*inst).kind else { return None };
        (func.value_u256(inner_shift) == Some(shift)
            && (constant >> shift.to::<usize>()).is_zero()
            && Self::is_clean_address(func, base))
        .then_some(base)
    }

    fn masked_shifted_constant(func: &Function, a: ValueId, b: ValueId) -> Option<U256> {
        let (value, mask) = Self::const_operand(func, a, b)?;
        let Value::Inst(inst) = func.value(value) else { return None };
        let InstKind::Or(a, b) = func.inst(*inst).kind else { return None };
        let (value, constant) = Self::const_operand(func, a, b)?;
        let Value::Inst(inst) = func.value(value) else { return None };
        let (shift, left) = match func.inst(*inst).kind {
            InstKind::Shl(shift, _) => (shift, true),
            InstKind::Shr(shift, _) => (shift, false),
            _ => return None,
        };
        let shift = func.value_u256(shift)?;
        let unknown_mask = mask & !constant;
        let known = shift >= U256::from(256)
            || if left {
                (unknown_mask >> shift.to::<usize>()).is_zero()
            } else {
                (unknown_mask << shift.to::<usize>()).is_zero()
            };
        known.then_some(constant & mask)
    }

    fn power_of_two_shift(value: U256) -> Option<U256> {
        if value.is_zero() || (value & (value - U256::from(1))) != U256::ZERO {
            return None;
        }
        Some(U256::from(value.trailing_zeros()))
    }

    fn imm(func: &mut Function, value: impl ToUint) -> ValueId {
        func.alloc_value(Value::Immediate(Immediate::uint256(value.to_uint())))
    }

    fn imm_bool(func: &mut Function, value: bool) -> ValueId {
        func.alloc_value(Value::Immediate(Immediate::bool(value)))
    }

    fn is_const(func: &Function, value: ValueId, expected: U256) -> bool {
        func.value_u256(value) == Some(expected)
    }

    fn is_zero(func: &Function, value: ValueId) -> bool {
        Self::is_const(func, value, U256::ZERO)
    }

    fn is_one(func: &Function, value: ValueId) -> bool {
        Self::is_const(func, value, U256::from(1))
    }

    fn is_bool_value(func: &Function, value: ValueId) -> bool {
        func.value_ty(value) == Some(MirType::I1)
    }

    fn same_value(func: &Function, a: ValueId, b: ValueId) -> bool {
        a == b
            || match (func.value(a), func.value(b)) {
                (Value::Immediate(a), Value::Immediate(b)) => a == b,
                _ => false,
            }
    }

    fn is_all_ones(func: &Function, value: ValueId) -> bool {
        Self::is_const(func, value, U256::MAX)
    }

    fn is_uint160_mask(func: &Function, value: ValueId) -> bool {
        let mask = (U256::from(1) << 160) - U256::from(1);
        Self::is_const(func, value, mask)
    }

    fn is_clean_address(func: &Function, value: ValueId) -> bool {
        func.value_ty(value) == Some(MirType::I160)
            || matches!(func.value(value), Value::Inst(id)
                if matches!(func.inst(*id).kind, InstKind::WordCast(inner)
                    if func.value_ty(inner) == Some(MirType::I160)))
    }

    fn is_current_address(func: &Function, value: ValueId) -> bool {
        let Value::Inst(id) = func.value(value) else { return false };
        match func.inst(*id).kind {
            InstKind::Address => true,
            InstKind::WordCast(inner) => Self::is_current_address(func, inner),
            _ => false,
        }
    }

    fn rewrite_terminators(
        &mut self,
        func: &mut Function,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> usize {
        let externally_terminating =
            func.selector.is_some() || func.attributes.is_receive || func.attributes.is_fallback;
        let mut rewrites = 0;
        for block_id in func.blocks.indices() {
            loop {
                let Some(Terminator::Branch { condition, .. }) = func.blocks[block_id].terminator
                else {
                    break;
                };
                let condition = mir_utils::resolve_replacement(condition, replacements);
                let (inner, swap) = if let Some(inner) = Self::zero_test_operand(func, condition) {
                    (inner, true)
                } else if let Some(inner) = Self::nonzero_test_operand(func, condition) {
                    // `branch gt(x, 0)` / `branch lt(0, x)` test exactly `x != 0`,
                    // which is what `branch x` already does.
                    (inner, false)
                } else {
                    break;
                };
                let inner = mir_utils::resolve_replacement(inner, replacements);
                if func.value_ty(inner) != Some(crate::mir::MirType::I1) {
                    break;
                }
                let Some(Terminator::Branch { condition, then_block, else_block }) =
                    &mut func.blocks[block_id].terminator
                else {
                    unreachable!()
                };
                *condition = inner;
                if swap {
                    std::mem::swap(then_block, else_block);
                }
                rewrites += 1;
                tracing::trace!(
                    target: "solar::codegen::mir::inst_simplify",
                    function = %func.name,
                    action = "rewrite_terminator",
                    ?block_id,
                    swap,
                    "mir_inst_simplify"
                );
            }

            if externally_terminating
                && let Some(Terminator::ReturnData { size, .. }) = func.blocks[block_id].terminator
                && Self::is_zero(func, mir_utils::resolve_replacement(size, replacements))
            {
                func.blocks[block_id].terminator = Some(Terminator::Stop);
                rewrites += 1;
            }
        }

        rewrites
    }

    /// Returns `x` when `value` computes `gt(x, 0)` or `lt(0, x)`, both of
    /// which are the unsigned nonzero test.
    fn nonzero_test_operand(func: &Function, value: ValueId) -> Option<ValueId> {
        match func.value(value) {
            Value::Inst(inst_id) => match func.inst(*inst_id).kind {
                InstKind::Gt(a, b) if Self::is_zero(func, b) => Some(a),
                InstKind::Lt(a, b) if Self::is_zero(func, a) => Some(b),
                _ => None,
            },
            _ => None,
        }
    }

    fn zero_test_operand(func: &Function, value: ValueId) -> Option<ValueId> {
        match func.value(value) {
            Value::Inst(inst_id) => match func.inst(*inst_id).kind {
                InstKind::Eq(inner, zero) if Self::is_zero(func, zero) => Some(inner),
                _ => None,
            },
            _ => None,
        }
    }

    fn not_operand(func: &Function, value: ValueId) -> Option<ValueId> {
        match func.value(value) {
            Value::Inst(inst_id) => match func.inst(*inst_id).kind {
                InstKind::Not(inner) => Some(inner),
                _ => None,
            },
            _ => None,
        }
    }

    fn is_bitwise_complement_pair(func: &Function, a: ValueId, b: ValueId) -> bool {
        Self::not_operand(func, a) == Some(b) || Self::not_operand(func, b) == Some(a)
    }
}
