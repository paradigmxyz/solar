//! Local MIR instruction simplification.
//!
//! This pass removes algebraic no-ops and rewrites a few equivalent EVM
//! instruction patterns before stack scheduling. It is intentionally local and
//! conservative: it only applies identities that are exact for EVM word
//! semantics.

use crate::mir::{Function, Immediate, InstId, InstKind, Terminator, Value, ValueId};
use alloy_primitives::U256;
use rustc_hash::{FxHashMap, FxHashSet};

/// Local MIR instruction simplification pass.
#[derive(Debug, Default)]
pub struct InstSimplifier {
    /// Number of instructions simplified in the last run.
    pub simplified_count: usize,
}

impl InstSimplifier {
    /// Creates a new instruction simplifier.
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs instruction simplification on a function.
    pub fn run(&mut self, func: &mut Function) -> usize {
        self.simplified_count = 0;

        let inst_results = Self::inst_results(func);
        let mut replacements: FxHashMap<ValueId, ValueId> = FxHashMap::default();
        let mut dead: FxHashSet<InstId> = FxHashSet::default();
        let block_ids: Vec<_> = func.blocks.indices().collect();

        for block_id in block_ids {
            let inst_ids = func.blocks[block_id].instructions.clone();
            for inst_id in inst_ids {
                let kind = func.instructions[inst_id].kind.clone();

                if let Some(new_kind) = self.rewrite_inst(func, &kind, &replacements) {
                    func.instructions[inst_id].kind = new_kind;
                    self.simplified_count += 1;
                    continue;
                }

                let Some(&result) = inst_results.get(&inst_id) else {
                    continue;
                };
                let Some(replacement) = self.simplify_inst(func, &kind, &replacements) else {
                    continue;
                };
                let replacement = Self::resolve_replacement(&replacements, replacement);
                if replacement == result {
                    continue;
                }
                replacements.insert(result, replacement);
                dead.insert(inst_id);
                self.simplified_count += 1;
            }
        }

        if !replacements.is_empty() {
            Self::replace_uses(func, &replacements);
        }
        if !dead.is_empty() {
            for block in func.blocks.iter_mut() {
                block.instructions.retain(|id| !dead.contains(id));
            }
        }
        self.simplified_count += self.rewrite_terminators(func, &replacements);

        self.simplified_count
    }

    /// Runs instruction simplification until no more changes are found.
    pub fn run_to_fixpoint(&mut self, func: &mut Function) -> usize {
        let mut total = 0;
        loop {
            let simplified = self.run(func);
            if simplified == 0 {
                break;
            }
            total += simplified;
        }
        total
    }

    fn rewrite_inst(
        &mut self,
        func: &Function,
        kind: &InstKind,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> Option<InstKind> {
        match kind {
            InstKind::Eq(a, b) => {
                let (a, b) = (
                    Self::resolve_replacement(replacements, *a),
                    Self::resolve_replacement(replacements, *b),
                );
                if a != b && Self::is_zero(func, a) {
                    Some(InstKind::IsZero(b))
                } else if a != b && Self::is_zero(func, b) {
                    Some(InstKind::IsZero(a))
                } else {
                    None
                }
            }
            InstKind::Balance(addr) => {
                let addr = Self::resolve_replacement(replacements, *addr);
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
        let resolve = |value| Self::resolve_replacement(replacements, value);

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
                } else if Self::is_one(func, b) {
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
                } else {
                    None
                }
            }
            InstKind::Or(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                if a == b {
                    Some(a)
                } else if Self::is_zero(func, a) {
                    Some(b)
                } else if Self::is_zero(func, b) {
                    Some(a)
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
                } else {
                    None
                }
            }
            InstKind::Not(a) => {
                let a = resolve(*a);
                Self::not_operand(func, a)
                    .or_else(|| Self::as_u256(func, a).map(|v| Self::imm_u256(func, !v)))
            }
            InstKind::IsZero(a) => {
                let a = resolve(*a);
                Self::as_u256(func, a).map(|v| Self::imm_bool(func, v.is_zero()))
            }
            InstKind::Shl(a, b) | InstKind::Shr(a, b) | InstKind::Sar(a, b) => {
                let (shift, value) = (resolve(*a), resolve(*b));
                if Self::is_zero(func, shift) || Self::is_zero(func, value) {
                    Some(value)
                } else {
                    None
                }
            }
            InstKind::Eq(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                (a == b).then(|| Self::imm_bool(func, true))
            }
            InstKind::Lt(a, b) | InstKind::Gt(a, b) | InstKind::SLt(a, b) | InstKind::SGt(a, b) => {
                let (a, b) = (resolve(*a), resolve(*b));
                (a == b).then(|| Self::imm_bool(func, false))
            }
            InstKind::Select(_, then_value, else_value) => {
                let (then_value, else_value) = (resolve(*then_value), resolve(*else_value));
                (then_value == else_value).then_some(then_value)
            }
            _ => None,
        }
    }

    fn inst_results(func: &Function) -> FxHashMap<InstId, ValueId> {
        func.values
            .iter_enumerated()
            .filter_map(|(value_id, value)| {
                if let Value::Inst(inst_id) = value { Some((*inst_id, value_id)) } else { None }
            })
            .collect()
    }

    fn resolve_replacement(
        replacements: &FxHashMap<ValueId, ValueId>,
        mut value: ValueId,
    ) -> ValueId {
        while let Some(&replacement) = replacements.get(&value) {
            value = replacement;
        }
        value
    }

    fn imm_u256(func: &mut Function, value: U256) -> ValueId {
        func.alloc_value(Value::Immediate(Immediate::uint256(value)))
    }

    fn imm_bool(func: &mut Function, value: bool) -> ValueId {
        func.alloc_value(Value::Immediate(Immediate::bool(value)))
    }

    fn as_u256(func: &Function, value: ValueId) -> Option<U256> {
        func.values[value].as_immediate()?.as_u256()
    }

    fn is_const(func: &Function, value: ValueId, expected: U256) -> bool {
        Self::as_u256(func, value) == Some(expected)
    }

    fn is_zero(func: &Function, value: ValueId) -> bool {
        Self::is_const(func, value, U256::ZERO)
    }

    fn is_one(func: &Function, value: ValueId) -> bool {
        Self::is_const(func, value, U256::from(1))
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
            let condition = Self::resolve_replacement(replacements, condition);
            if let Some(inner) = Self::iszero_operand(func, condition) {
                rewrites.push((block_id, Self::resolve_replacement(replacements, inner)));
            }
        }

        for (block_id, inner) in rewrites.iter().copied() {
            let Some(Terminator::Branch { condition, then_block, else_block }) =
                &mut func.blocks[block_id].terminator
            else {
                continue;
            };
            *condition = inner;
            std::mem::swap(then_block, else_block);
        }

        rewrites.len()
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

    fn replace_uses(func: &mut Function, replacements: &FxHashMap<ValueId, ValueId>) {
        if replacements.is_empty() {
            return;
        }

        for inst in func.instructions.iter_mut() {
            Self::replace_inst_operands(&mut inst.kind, replacements);
        }
        for value in func.values.iter_mut() {
            if let Value::Phi { incoming, .. } = value {
                for (_, value) in incoming {
                    if replacements.contains_key(value) {
                        *value = Self::resolve_replacement(replacements, *value);
                    }
                }
            }
        }
        for block in func.blocks.iter_mut() {
            if let Some(term) = &mut block.terminator {
                Self::replace_terminator_operands(term, replacements);
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn replace_inst_operands(kind: &mut InstKind, replacements: &FxHashMap<ValueId, ValueId>) {
        let replace = |value: &mut ValueId| {
            if replacements.contains_key(value) {
                *value = Self::resolve_replacement(replacements, *value);
            }
        };

        match kind {
            InstKind::Add(a, b)
            | InstKind::Sub(a, b)
            | InstKind::Mul(a, b)
            | InstKind::Div(a, b)
            | InstKind::SDiv(a, b)
            | InstKind::Mod(a, b)
            | InstKind::SMod(a, b)
            | InstKind::Exp(a, b)
            | InstKind::And(a, b)
            | InstKind::Or(a, b)
            | InstKind::Xor(a, b)
            | InstKind::Shl(a, b)
            | InstKind::Shr(a, b)
            | InstKind::Sar(a, b)
            | InstKind::Byte(a, b)
            | InstKind::Lt(a, b)
            | InstKind::Gt(a, b)
            | InstKind::SLt(a, b)
            | InstKind::SGt(a, b)
            | InstKind::Eq(a, b)
            | InstKind::MStore(a, b)
            | InstKind::MStore8(a, b)
            | InstKind::SStore(a, b)
            | InstKind::TStore(a, b)
            | InstKind::Keccak256(a, b)
            | InstKind::Log0(a, b)
            | InstKind::SignExtend(a, b) => {
                replace(a);
                replace(b);
            }
            InstKind::Not(a)
            | InstKind::IsZero(a)
            | InstKind::MLoad(a)
            | InstKind::SLoad(a)
            | InstKind::TLoad(a)
            | InstKind::CalldataLoad(a)
            | InstKind::ExtCodeSize(a)
            | InstKind::ExtCodeHash(a)
            | InstKind::Balance(a)
            | InstKind::BlockHash(a)
            | InstKind::BlobHash(a) => {
                replace(a);
            }
            InstKind::AddMod(a, b, c)
            | InstKind::MulMod(a, b, c)
            | InstKind::MCopy(a, b, c)
            | InstKind::CalldataCopy(a, b, c)
            | InstKind::CodeCopy(a, b, c)
            | InstKind::ReturnDataCopy(a, b, c)
            | InstKind::Create(a, b, c)
            | InstKind::Log1(a, b, c)
            | InstKind::Select(a, b, c) => {
                replace(a);
                replace(b);
                replace(c);
            }
            InstKind::ExtCodeCopy(a, b, c, d)
            | InstKind::Create2(a, b, c, d)
            | InstKind::Log2(a, b, c, d) => {
                replace(a);
                replace(b);
                replace(c);
                replace(d);
            }
            InstKind::Log3(a, b, c, d, e) => {
                replace(a);
                replace(b);
                replace(c);
                replace(d);
                replace(e);
            }
            InstKind::Log4(a, b, c, d, e, f) => {
                replace(a);
                replace(b);
                replace(c);
                replace(d);
                replace(e);
                replace(f);
            }
            InstKind::Call { gas, addr, value, args_offset, args_size, ret_offset, ret_size } => {
                replace(gas);
                replace(addr);
                replace(value);
                replace(args_offset);
                replace(args_size);
                replace(ret_offset);
                replace(ret_size);
            }
            InstKind::StaticCall { gas, addr, args_offset, args_size, ret_offset, ret_size }
            | InstKind::DelegateCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                replace(gas);
                replace(addr);
                replace(args_offset);
                replace(args_size);
                replace(ret_offset);
                replace(ret_size);
            }
            InstKind::InternalCall { args, .. } => {
                for arg in args {
                    replace(arg);
                }
            }
            InstKind::Phi(incoming) => {
                for (_, value) in incoming {
                    replace(value);
                }
            }
            InstKind::MSize
            | InstKind::CalldataSize
            | InstKind::InternalFrameAddr(_)
            | InstKind::CodeSize
            | InstKind::ReturnDataSize
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
            | InstKind::BlobBaseFee => {}
        }
    }

    fn replace_terminator_operands(
        term: &mut Terminator,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) {
        let replace = |value: &mut ValueId| {
            if replacements.contains_key(value) {
                *value = Self::resolve_replacement(replacements, *value);
            }
        };

        match term {
            Terminator::Jump(_) | Terminator::Stop | Terminator::Invalid => {}
            Terminator::Branch { condition, .. } => replace(condition),
            Terminator::Switch { value, cases, .. } => {
                replace(value);
                for (case_value, _) in cases {
                    replace(case_value);
                }
            }
            Terminator::Return { values } => {
                for value in values {
                    replace(value);
                }
            }
            Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
                replace(offset);
                replace(size);
            }
            Terminator::SelfDestruct { recipient } => replace(recipient),
        }
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
        let cmp = builder.lt(arg, zero);
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
