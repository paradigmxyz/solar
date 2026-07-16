//! Local MIR instruction simplification.
//!
//! This pass removes algebraic no-ops and rewrites a few equivalent EVM
//! instruction patterns before stack scheduling. It is intentionally local and
//! conservative: it only applies identities that are exact for EVM word
//! semantics.
//!
//! Safety contract:
//! - do not remove or reorder side effects
//! - replace an instruction with a value only when the equality is exact for all 256-bit EVM words
//! - preserve boolean-only rewrites behind explicit MIR boolean type checks

use crate::{
    mir::{
        Function, Immediate, InstId, InstKind, MirType, Terminator, Value, ValueId,
        utils as mir_utils,
    },
    pass::FunctionPass,
    utils::evm_word,
};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};

/// Local MIR instruction simplification pass.
#[derive(Debug, Default)]
pub struct InstSimplifier {
    /// Number of instructions simplified in the last run.
    pub simplified_count: usize,
}

struct RunState {
    inst_results: FxHashMap<InstId, ValueId>,
    replacements: FxHashMap<ValueId, ValueId>,
    dead: DenseBitSet<InstId>,
}

impl RunState {
    fn new(func: &Function) -> Self {
        Self {
            inst_results: func.inst_results(),
            replacements: FxHashMap::default(),
            dead: DenseBitSet::new_empty(func.instructions.len()),
        }
    }
}

/// Function pass for local instruction simplification.
pub struct InstSimplifyPass;

impl FunctionPass for InstSimplifyPass {
    fn name(&self) -> &str {
        "inst-simplify"
    }

    fn run_on_function(&mut self, func: &mut Function) -> bool {
        InstSimplifier::new().run_to_fixpoint(func) != 0
    }
}

impl InstSimplifier {
    /// Creates a new instruction simplifier.
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs instruction simplification on a function.
    pub fn run(&mut self, func: &mut Function) -> usize {
        let mut state = RunState::new(func);
        self.run_with_state(func, &mut state)
    }

    fn run_with_state(&mut self, func: &mut Function, state: &mut RunState) -> usize {
        self.simplified_count = 0;

        state.replacements.clear();
        state.dead.clear();
        let block_ids: Vec<_> = func.blocks.indices().collect();

        for block_id in block_ids {
            let instruction_count = func.blocks[block_id].instructions.len();
            for index in 0..instruction_count {
                let inst_id = func.blocks[block_id].instructions[index];
                let kind = func.instructions[inst_id].kind.clone();

                if self.is_dead_noop_inst(func, &kind, &state.replacements) {
                    state.dead.insert(inst_id);
                    self.simplified_count += 1;
                    continue;
                }

                if let Some(new_kind) = self.rewrite_inst(func, &kind, &state.replacements) {
                    func.instructions[inst_id].kind = new_kind;
                    self.simplified_count += 1;
                    continue;
                }

                let Some(&result) = state.inst_results.get(&inst_id) else {
                    continue;
                };
                let Some(replacement) = self.simplify_inst(func, &kind, &state.replacements) else {
                    continue;
                };
                let replacement = mir_utils::resolve_replacement(replacement, &state.replacements);
                if replacement == result {
                    continue;
                }
                state.replacements.insert(result, replacement);
                state.dead.insert(inst_id);
                self.simplified_count += 1;
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
        self.simplified_count += self.rewrite_terminators(func, &state.replacements);

        self.simplified_count
    }

    /// Runs instruction simplification until no more changes are found.
    pub fn run_to_fixpoint(&mut self, func: &mut Function) -> usize {
        let mut total = 0;
        let mut state = RunState::new(func);
        loop {
            let simplified = self.run_with_state(func, &mut state);
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

        match kind {
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
                if a != b && Self::is_zero(func, a) {
                    Some(InstKind::IsZero(b))
                } else if a != b && Self::is_zero(func, b) {
                    Some(InstKind::IsZero(a))
                } else {
                    None
                }
            }
            // `a < 1` is `a == 0` for unsigned comparisons.
            InstKind::Lt(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                Self::is_one(func, b).then_some(InstKind::IsZero(a))
            }
            // `1 > b` is `b == 0` for unsigned comparisons.
            InstKind::Gt(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                Self::is_one(func, a).then_some(InstKind::IsZero(b))
            }
            InstKind::Select(condition, then_value, else_value) => {
                let (condition, then_value, else_value) =
                    (resolve(*condition), resolve(*then_value), resolve(*else_value));
                if Self::is_bool_value(func, condition)
                    && Self::is_zero(func, then_value)
                    && Self::is_one(func, else_value)
                {
                    Some(InstKind::IsZero(condition))
                } else {
                    None
                }
            }
            InstKind::Balance(addr) => {
                let addr = resolve(*addr);
                Self::is_current_address(func, addr).then_some(InstKind::SelfBalance)
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
            InstKind::Add(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if Self::is_zero(func, b) {
                    Some(a)
                } else if Self::is_zero(func, a) {
                    Some(b)
                } else {
                    None
                }
            }
            InstKind::Sub(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if Self::is_zero(func, b) {
                    Some(a)
                } else if a == b {
                    Some(Self::imm_u256(func, U256::ZERO))
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
                } else if Self::is_zero(func, b) {
                    Some(Self::imm_u256(func, U256::ZERO))
                } else {
                    None
                }
            }
            InstKind::Mod(a, b) | InstKind::SMod(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if Self::is_zero(func, a) {
                    Some(a)
                } else if Self::is_zero(func, b) || Self::is_one(func, b) || a == b {
                    Some(Self::imm_u256(func, U256::ZERO))
                } else {
                    None
                }
            }
            InstKind::Exp(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if Self::is_zero(func, b) {
                    Some(Self::imm_u256(func, U256::from(1)))
                } else if Self::is_one(func, a) || Self::is_one(func, b) {
                    Some(a)
                } else {
                    None
                }
            }
            InstKind::And(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if a == b
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
                    Some(Self::imm_u256(func, U256::ZERO))
                } else {
                    None
                }
            }
            InstKind::Or(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if a == b || Self::is_all_ones(func, a) || Self::is_zero(func, b) {
                    Some(a)
                } else if Self::is_all_ones(func, b) || Self::is_zero(func, a) {
                    Some(b)
                } else if Self::is_bitwise_complement_pair(func, a, b) {
                    Some(Self::imm_u256(func, U256::MAX))
                } else {
                    None
                }
            }
            InstKind::Xor(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if a == b {
                    Some(Self::imm_u256(func, U256::ZERO))
                } else if Self::is_zero(func, a) {
                    Some(b)
                } else if Self::is_zero(func, b) {
                    Some(a)
                } else if Self::is_bitwise_complement_pair(func, a, b) {
                    Some(Self::imm_u256(func, U256::MAX))
                } else {
                    None
                }
            }
            InstKind::Not(a) => {
                let a = resolve(*a);
                Self::not_operand(func, a)
                    .or_else(|| func.value_u256(a).map(|v| Self::imm_u256(func, !v)))
            }
            InstKind::IsZero(a) => {
                let a = resolve(*a);
                func.value_u256(a).map(|v| Self::imm_bool(func, v.is_zero())).or_else(|| {
                    let inner = Self::iszero_operand(func, a)?;
                    Self::is_bool_value(func, inner).then_some(inner)
                })
            }
            InstKind::Shl(a, b) | InstKind::Shr(a, b) | InstKind::Sar(a, b) => {
                let (shift, value) = (resolve(*a), resolve(*b));
                if Self::is_zero(func, shift) || Self::is_zero(func, value) {
                    Some(value)
                } else if !matches!(kind, InstKind::Sar(_, _))
                    && func.value_u256(shift).is_some_and(|shift| shift >= U256::from(256))
                {
                    Some(Self::imm_u256(func, U256::ZERO))
                } else {
                    None
                }
            }
            InstKind::Byte(a, b) => {
                let (a, value) = (resolve(*a), resolve(*b));
                (Self::is_zero(func, value)
                    || func.value_u256(a).is_some_and(|index| index >= U256::from(32)))
                .then(|| Self::imm_u256(func, U256::ZERO))
            }
            InstKind::Eq(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if a == b {
                    Some(Self::imm_bool(func, true))
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
                if a == b {
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
                    Some(Self::imm_u256(func, U256::ZERO))
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
                    Some(Self::imm_u256(func, U256::ZERO))
                } else {
                    None
                }
            }
            InstKind::SignExtend(a, b) => {
                let (byte, value) = (resolve(*a), resolve(*b));
                if Self::is_zero(func, value)
                    || func.value_u256(byte).is_some_and(|byte| byte >= U256::from(31))
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
        let constant = |func: &Function, value| func.value_u256(resolve(value));

        match *kind {
            InstKind::Add(a, b) => {
                Some(Self::imm_u256(func, constant(func, a)?.wrapping_add(constant(func, b)?)))
            }
            InstKind::Sub(a, b) => {
                Some(Self::imm_u256(func, constant(func, a)?.wrapping_sub(constant(func, b)?)))
            }
            InstKind::Mul(a, b) => {
                Some(Self::imm_u256(func, constant(func, a)?.wrapping_mul(constant(func, b)?)))
            }
            InstKind::Div(a, b) => {
                let a = constant(func, a)?;
                let b = constant(func, b)?;
                Some(Self::imm_u256(func, if b.is_zero() { U256::ZERO } else { a / b }))
            }
            InstKind::SDiv(a, b) => {
                let a = constant(func, a)?;
                let b = constant(func, b)?;
                Some(Self::imm_u256(func, evm_word::signed_div(a, b)))
            }
            InstKind::Mod(a, b) => {
                let a = constant(func, a)?;
                let b = constant(func, b)?;
                Some(Self::imm_u256(func, if b.is_zero() { U256::ZERO } else { a % b }))
            }
            InstKind::SMod(a, b) => {
                let a = constant(func, a)?;
                let b = constant(func, b)?;
                Some(Self::imm_u256(func, evm_word::signed_mod(a, b)))
            }
            InstKind::Exp(a, b) => {
                Some(Self::imm_u256(func, constant(func, a)?.wrapping_pow(constant(func, b)?)))
            }
            InstKind::AddMod(a, b, n) => {
                let a = constant(func, a)?;
                let b = constant(func, b)?;
                let n = constant(func, n)?;
                if n.is_zero() {
                    Some(Self::imm_u256(func, U256::ZERO))
                } else {
                    Some(Self::imm_u256(func, a.add_mod(b, n)))
                }
            }
            InstKind::MulMod(a, b, n) => {
                let a = constant(func, a)?;
                let b = constant(func, b)?;
                let n = constant(func, n)?;
                if n.is_zero() {
                    Some(Self::imm_u256(func, U256::ZERO))
                } else {
                    Some(Self::imm_u256(func, a.mul_mod(b, n)))
                }
            }
            InstKind::And(a, b) => {
                Some(Self::imm_u256(func, constant(func, a)? & constant(func, b)?))
            }
            InstKind::Or(a, b) => {
                Some(Self::imm_u256(func, constant(func, a)? | constant(func, b)?))
            }
            InstKind::Xor(a, b) => {
                Some(Self::imm_u256(func, constant(func, a)? ^ constant(func, b)?))
            }
            InstKind::Not(a) => Some(Self::imm_u256(func, !constant(func, a)?)),
            InstKind::Shl(shift, value) => {
                let shift = constant(func, shift)?;
                let value = constant(func, value)?;
                let folded = if shift >= U256::from(256) {
                    U256::ZERO
                } else {
                    value << shift.to::<usize>()
                };
                Some(Self::imm_u256(func, folded))
            }
            InstKind::Shr(shift, value) => {
                let shift = constant(func, shift)?;
                let value = constant(func, value)?;
                let folded = if shift >= U256::from(256) {
                    U256::ZERO
                } else {
                    value >> shift.to::<usize>()
                };
                Some(Self::imm_u256(func, folded))
            }
            InstKind::Sar(shift, value) => Some(Self::imm_u256(
                func,
                evm_word::sar(constant(func, value)?, constant(func, shift)?),
            )),
            InstKind::Byte(index, value) => Some(Self::imm_u256(
                func,
                evm_word::byte(constant(func, index)?, constant(func, value)?),
            )),
            InstKind::SignExtend(size, value) => Some(Self::imm_u256(
                func,
                evm_word::signextend(constant(func, size)?, constant(func, value)?),
            )),
            InstKind::Lt(a, b) => {
                Some(Self::imm_bool(func, constant(func, a)? < constant(func, b)?))
            }
            InstKind::Gt(a, b) => {
                Some(Self::imm_bool(func, constant(func, a)? > constant(func, b)?))
            }
            InstKind::SLt(a, b) => Some(Self::imm_bool(
                func,
                evm_word::signed_lt(constant(func, a)?, constant(func, b)?),
            )),
            InstKind::SGt(a, b) => Some(Self::imm_bool(
                func,
                evm_word::signed_gt(constant(func, a)?, constant(func, b)?),
            )),
            InstKind::Eq(a, b) => {
                Some(Self::imm_bool(func, constant(func, a)? == constant(func, b)?))
            }
            InstKind::IsZero(a) => Some(Self::imm_bool(func, constant(func, a)?.is_zero())),
            InstKind::Select(condition, then_value, else_value) => {
                let condition = constant(func, condition)?;
                Some(if condition.is_zero() { resolve(else_value) } else { resolve(then_value) })
            }
            _ => None,
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
            InstKind::MCopy(_, _, size)
            | InstKind::CalldataCopy(_, _, size)
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
        let (value, constant) = Self::const_operand(func, a, b)?;
        let shift = Self::power_of_two_shift(constant)?;
        if shift.is_zero() {
            return None;
        }
        let shift = Self::imm_u256(func, shift);
        Some(InstKind::Shl(shift, value))
    }

    fn rewrite_div(&self, func: &mut Function, a: ValueId, b: ValueId) -> Option<InstKind> {
        let shift = Self::power_of_two_shift(func.value_u256(b)?)?;
        if shift.is_zero() {
            return None;
        }
        let shift = Self::imm_u256(func, shift);
        Some(InstKind::Shr(shift, a))
    }

    fn rewrite_mod(&self, func: &mut Function, a: ValueId, b: ValueId) -> Option<InstKind> {
        let constant = func.value_u256(b)?;
        let shift = Self::power_of_two_shift(constant)?;
        if shift.is_zero() {
            return None;
        }
        let mask = Self::imm_u256(func, constant - U256::from(1));
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
        let combined = Self::imm_u256(func, mask & existing_mask);
        Some(InstKind::And(base, combined))
    }

    fn add_offset_kind(&self, func: &mut Function, base: ValueId, offset: U256) -> InstKind {
        let offset = Self::imm_u256(func, offset);
        InstKind::Add(base, offset)
    }

    fn const_operand(func: &Function, a: ValueId, b: ValueId) -> Option<(ValueId, U256)> {
        if let Some(constant) = func.value_u256(b) {
            Some((a, constant))
        } else {
            func.value_u256(a).map(|constant| (b, constant))
        }
    }

    fn offset_base(func: &Function, value: ValueId) -> Option<(ValueId, U256)> {
        let Value::Inst(inst_id) = func.value(value) else { return None };
        match func.instructions[*inst_id].kind {
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
        match func.instructions[*inst_id].kind {
            InstKind::And(a, b) => Self::const_operand(func, a, b),
            _ => None,
        }
    }

    fn power_of_two_shift(value: U256) -> Option<U256> {
        if value.is_zero() || (value & (value - U256::from(1))) != U256::ZERO {
            return None;
        }
        Some(U256::from(value.trailing_zeros()))
    }

    fn imm_u256(func: &mut Function, value: U256) -> ValueId {
        func.alloc_value(Value::Immediate(Immediate::uint256(value)))
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
        match &func.values[value] {
            Value::Immediate(Immediate::Bool(_)) => true,
            Value::Arg { ty: MirType::Bool, .. } => true,
            Value::Inst(inst_id) => func.instructions[*inst_id].result_ty == Some(MirType::Bool),
            _ => false,
        }
    }

    fn same_value(func: &Function, a: ValueId, b: ValueId) -> bool {
        a == b
            || match (&func.values[a], &func.values[b]) {
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
        match &func.values[value] {
            Value::Immediate(Immediate::Address(_)) => true,
            Value::Inst(inst_id) => matches!(
                func.instructions[*inst_id].kind,
                InstKind::Address
                    | InstKind::Caller
                    | InstKind::Origin
                    | InstKind::Coinbase
                    | InstKind::Create(_, _, _)
                    | InstKind::Create2(_, _, _, _)
            ),
            _ => false,
        }
    }

    fn is_current_address(func: &Function, value: ValueId) -> bool {
        match &func.values[value] {
            Value::Inst(inst_id) => matches!(func.instructions[*inst_id].kind, InstKind::Address),
            _ => false,
        }
    }

    fn rewrite_terminators(
        &mut self,
        func: &mut Function,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> usize {
        let mut rewrites = Vec::new();
        for block_id in func.blocks.indices() {
            let Some(Terminator::Branch { condition, .. }) = func.blocks[block_id].terminator
            else {
                continue;
            };
            let condition = mir_utils::resolve_replacement(condition, replacements);
            if let Some(inner) = Self::iszero_operand(func, condition) {
                rewrites.push((
                    block_id,
                    mir_utils::resolve_replacement(inner, replacements),
                    true,
                ));
            } else if let Some(inner) = Self::nonzero_test_operand(func, condition) {
                // `branch gt(x, 0)` / `branch lt(0, x)` test exactly `x != 0`,
                // which is what `branch x` already does.
                rewrites.push((
                    block_id,
                    mir_utils::resolve_replacement(inner, replacements),
                    false,
                ));
            }
        }

        for (block_id, inner, swap) in rewrites.iter().copied() {
            {
                let Some(Terminator::Branch { condition, then_block, else_block }) =
                    &mut func.blocks[block_id].terminator
                else {
                    continue;
                };
                *condition = inner;
                if swap {
                    std::mem::swap(then_block, else_block);
                }
            }
        }

        rewrites.len()
    }

    /// Returns `x` when `value` computes `gt(x, 0)` or `lt(0, x)`, both of
    /// which are the unsigned nonzero test.
    fn nonzero_test_operand(func: &Function, value: ValueId) -> Option<ValueId> {
        match &func.values[value] {
            Value::Inst(inst_id) => match func.instructions[*inst_id].kind {
                InstKind::Gt(a, b) if Self::is_zero(func, b) => Some(a),
                InstKind::Lt(a, b) if Self::is_zero(func, a) => Some(b),
                _ => None,
            },
            _ => None,
        }
    }

    fn iszero_operand(func: &Function, value: ValueId) -> Option<ValueId> {
        match &func.values[value] {
            Value::Inst(inst_id) => match func.instructions[*inst_id].kind {
                InstKind::IsZero(inner) => Some(inner),
                _ => None,
            },
            _ => None,
        }
    }

    fn not_operand(func: &Function, value: ValueId) -> Option<ValueId> {
        match &func.values[value] {
            Value::Inst(inst_id) => match func.instructions[*inst_id].kind {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{FunctionBuilder, MirType};
    use solar_interface::Ident;

    fn test_func() -> Function {
        Function::new(Ident::DUMMY)
    }

    #[test]
    fn removes_add_zero() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let arg = builder.add_param(MirType::uint256());
        let zero = builder.imm_u64(0);
        let sum = builder.add(arg, zero);
        builder.ret(vec![sum]);

        let mut pass = InstSimplifier::new();
        assert_eq!(pass.run(&mut func), 1);

        let block = &func.blocks[func.entry_block];
        assert!(block.instructions.is_empty());
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(values.as_slice(), &[arg]);
    }

    #[test]
    fn preserves_non_constant_side_of_mul_one() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let arg = builder.add_param(MirType::uint256());
        let one = builder.imm_u64(1);
        let product = builder.mul(one, arg);
        builder.ret(vec![product]);

        let mut pass = InstSimplifier::new();
        assert_eq!(pass.run(&mut func), 1);

        let block = &func.blocks[func.entry_block];
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(values.as_slice(), &[arg]);
    }

    #[test]
    fn removes_self_sub() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let arg = builder.add_param(MirType::uint256());
        let diff = builder.sub(arg, arg);
        builder.ret(vec![diff]);

        let mut pass = InstSimplifier::new();
        assert_eq!(pass.run(&mut func), 1);

        let block = &func.blocks[func.entry_block];
        assert!(block.instructions.is_empty());
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(
            func.values[values[0]].as_immediate().and_then(Immediate::as_u256),
            Some(U256::ZERO)
        );
    }

    #[test]
    fn removes_idempotent_logic_ops() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let arg = builder.add_param(MirType::uint256());
        let and = builder.and(arg, arg);
        let or = builder.or(arg, arg);
        let xor = builder.xor(arg, arg);
        builder.ret(vec![and, or, xor]);

        let mut pass = InstSimplifier::new();
        assert_eq!(pass.run(&mut func), 3);

        let block = &func.blocks[func.entry_block];
        assert!(block.instructions.is_empty());
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(&values[..2], &[arg, arg]);
        assert_eq!(
            func.values[values[2]].as_immediate().and_then(Immediate::as_u256),
            Some(U256::ZERO)
        );
    }

    #[test]
    fn folds_not_not() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let arg = builder.add_param(MirType::uint256());
        let not = builder.not(arg);
        let restored = builder.not(not);
        builder.ret(vec![restored]);

        let mut pass = InstSimplifier::new();
        assert_eq!(pass.run_to_fixpoint(&mut func), 1);

        let block = &func.blocks[func.entry_block];
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(values.as_slice(), &[arg]);
    }

    #[test]
    fn folds_iszero_immediate() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let zero = builder.imm_u64(0);
        let is_zero = builder.iszero(zero);
        builder.ret(vec![is_zero]);

        let mut pass = InstSimplifier::new();
        assert_eq!(pass.run(&mut func), 1);

        let block = &func.blocks[func.entry_block];
        assert!(block.instructions.is_empty());
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(
            func.values[values[0]].as_immediate().and_then(Immediate::as_u256),
            Some(U256::from(1))
        );
    }

    #[test]
    fn rewrites_eq_zero_to_iszero() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let arg = builder.add_param(MirType::uint256());
        let zero = builder.imm_u64(0);
        let eq = builder.eq(arg, zero);
        builder.ret(vec![eq]);

        let mut pass = InstSimplifier::new();
        assert_eq!(pass.run(&mut func), 1);

        let block = &func.blocks[func.entry_block];
        assert_eq!(block.instructions.len(), 1);
        let eq_inst = func.instructions[block.instructions[0]].kind.clone();
        assert!(matches!(eq_inst, InstKind::IsZero(value) if value == arg));
    }

    #[test]
    fn preserves_non_constant_side_of_and_all_ones() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let arg = builder.add_param(MirType::uint256());
        let all_ones = builder.imm_u256(U256::MAX);
        let masked = builder.and(all_ones, arg);
        builder.ret(vec![masked]);

        let mut pass = InstSimplifier::new();
        assert_eq!(pass.run(&mut func), 1);

        let block = &func.blocks[func.entry_block];
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(values.as_slice(), &[arg]);
    }

    #[test]
    fn removes_address_mask() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.address();
        let mask = builder.imm_u256((U256::from(1) << 160) - U256::from(1));
        let masked = builder.and(addr, mask);
        builder.ret(vec![masked]);

        let mut pass = InstSimplifier::new();
        assert_eq!(pass.run(&mut func), 1);

        let block = &func.blocks[func.entry_block];
        assert_eq!(block.instructions.len(), 1);
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(values.as_slice(), &[addr]);
    }

    #[test]
    fn rewrites_own_balance_to_selfbalance() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.address();
        let balance = builder.balance(addr);
        builder.ret(vec![balance]);

        let mut pass = InstSimplifier::new();
        assert_eq!(pass.run(&mut func), 1);

        let block = &func.blocks[func.entry_block];
        assert_eq!(block.instructions.len(), 2);
        let balance_inst = func.instructions[*block.instructions.last().unwrap()].kind.clone();
        assert!(matches!(balance_inst, InstKind::SelfBalance));
    }

    #[test]
    fn inverts_iszero_branch_condition() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let arg = builder.add_param(MirType::uint256());
        let zero = builder.imm_u64(0);
        let cmp = builder.lt(zero, arg);
        let inverted = builder.iszero(cmp);
        let then_block = builder.create_block();
        let else_block = builder.create_block();
        builder.branch(inverted, then_block, else_block);

        let mut pass = InstSimplifier::new();
        assert_eq!(pass.run(&mut func), 1);

        let block = &func.blocks[func.entry_block];
        let Some(Terminator::Branch { condition, then_block: new_then, else_block: new_else }) =
            block.terminator
        else {
            panic!("expected branch terminator");
        };
        assert_eq!(condition, cmp);
        assert_eq!(new_then, else_block);
        assert_eq!(new_else, then_block);
    }

    #[test]
    fn mask_rewrite_feeds_selfbalance() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.address();
        let mask = builder.imm_u256((U256::from(1) << 160) - U256::from(1));
        let masked = builder.and(addr, mask);
        let balance = builder.balance(masked);
        builder.ret(vec![balance]);

        let mut pass = InstSimplifier::new();
        assert_eq!(pass.run_to_fixpoint(&mut func), 2);

        let block = &func.blocks[func.entry_block];
        assert_eq!(block.instructions.len(), 2);
        let balance_inst = func.instructions[*block.instructions.last().unwrap()].kind.clone();
        assert!(matches!(balance_inst, InstKind::SelfBalance));
    }
}
