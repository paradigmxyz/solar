//! Local dead memory optimization.
//!
//! This pass removes full-word `mstore` instructions that are overwritten by a
//! later full-word `mstore` to the same exact address within the same basic
//! block, before any operation can observe memory or gas. It also forwards
//! same-block `mload` instructions from the latest exact-address `mstore` when
//! no intervening operation can mutate memory.

use crate::mir::{BlockId, Function, InstId, InstKind, Terminator, Value, ValueId};
use rustc_hash::{FxHashMap, FxHashSet};

/// Local dead memory optimization pass.
#[derive(Debug, Default)]
pub struct MemoryStoreEliminator {
    /// Number of memory instructions eliminated.
    pub eliminated_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum MemAddrKey {
    Const(u64),
    Value(ValueId),
}

impl MemoryStoreEliminator {
    /// Creates a new memory optimization pass.
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs local memory optimization on a function.
    pub fn run(&mut self, func: &mut Function) -> usize {
        self.eliminated_count = 0;

        let block_ids: Vec<BlockId> = func.blocks.indices().collect();
        for block_id in block_ids {
            self.process_block(func, block_id);
        }

        self.eliminated_count
    }

    /// Runs local memory optimization until no more instructions can be eliminated.
    pub fn run_to_fixpoint(&mut self, func: &mut Function) -> usize {
        let mut total = 0;
        loop {
            let eliminated = self.run(func);
            if eliminated == 0 {
                break;
            }
            total += eliminated;
        }
        total
    }

    fn process_block(&mut self, func: &mut Function, block_id: BlockId) {
        self.forward_loads(func, block_id);

        let inst_ids = func.blocks[block_id].instructions.clone();
        let mut overwritten: FxHashSet<MemAddrKey> = FxHashSet::default();
        let mut dead: FxHashSet<InstId> = FxHashSet::default();

        for &inst_id in inst_ids.iter().rev() {
            let inst = &func.instructions[inst_id];
            match &inst.kind {
                InstKind::MStore(addr, _) => {
                    if let Some(key) = self.mem_addr_key(func, *addr) {
                        if overwritten.contains(&key) {
                            dead.insert(inst_id);
                            self.eliminated_count += 1;
                        } else {
                            overwritten.insert(key);
                        }
                    } else {
                        overwritten.clear();
                    }
                }
                InstKind::MLoad(addr) => {
                    if let Some(key) = self.mem_addr_key(func, *addr) {
                        overwritten.remove(&key);
                    } else {
                        overwritten.clear();
                    }
                }
                kind if Self::is_memory_or_gas_observer(kind) => {
                    overwritten.clear();
                }
                _ => {}
            }
        }

        if dead.is_empty() {
            return;
        }

        func.blocks[block_id].instructions.retain(|id| !dead.contains(id));
    }

    fn forward_loads(&mut self, func: &mut Function, block_id: BlockId) {
        let inst_ids = func.blocks[block_id].instructions.clone();
        let inst_results = Self::inst_results(func);
        let mut stored_values: FxHashMap<MemAddrKey, ValueId> = FxHashMap::default();
        let mut replacements: FxHashMap<ValueId, ValueId> = FxHashMap::default();
        let mut dead: FxHashSet<InstId> = FxHashSet::default();

        for &inst_id in &inst_ids {
            let inst = &func.instructions[inst_id];
            match &inst.kind {
                InstKind::MStore(addr, value) => {
                    if let Some(key) = self.mem_addr_key(func, *addr) {
                        stored_values.insert(key, Self::resolve_replacement(&replacements, *value));
                    } else {
                        stored_values.clear();
                    }
                }
                InstKind::MLoad(addr) => {
                    let Some(key) = self.mem_addr_key(func, *addr) else {
                        continue;
                    };
                    let Some(&stored_value) = stored_values.get(&key) else {
                        continue;
                    };
                    if let Some(&loaded_value) = inst_results.get(&inst_id) {
                        replacements.insert(loaded_value, stored_value);
                        dead.insert(inst_id);
                    }
                }
                kind if Self::can_mutate_memory(kind) => {
                    stored_values.clear();
                }
                _ => {}
            }
        }

        if dead.is_empty() {
            return;
        }

        Self::replace_uses(func, &replacements);
        self.eliminated_count += dead.len();
        func.blocks[block_id].instructions.retain(|id| !dead.contains(id));
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

    fn mem_addr_key(&self, func: &Function, value: ValueId) -> Option<MemAddrKey> {
        match &func.values[value] {
            Value::Immediate(imm) => {
                let addr = imm.as_u256()?;
                u64::try_from(addr).ok().map(MemAddrKey::Const)
            }
            Value::Arg { .. } | Value::Inst(_) | Value::Phi { .. } => {
                Some(MemAddrKey::Value(value))
            }
            Value::Undef(_) => None,
        }
    }

    fn is_memory_or_gas_observer(kind: &InstKind) -> bool {
        matches!(
            kind,
            InstKind::MStore8(_, _)
                | InstKind::MSize
                | InstKind::MCopy(_, _, _)
                | InstKind::CalldataCopy(_, _, _)
                | InstKind::CodeCopy(_, _, _)
                | InstKind::ReturnDataCopy(_, _, _)
                | InstKind::ExtCodeCopy(_, _, _, _)
                | InstKind::Keccak256(_, _)
                | InstKind::Call { .. }
                | InstKind::StaticCall { .. }
                | InstKind::DelegateCall { .. }
                | InstKind::InternalCall { .. }
                | InstKind::Create(_, _, _)
                | InstKind::Create2(_, _, _, _)
                | InstKind::Log0(_, _)
                | InstKind::Log1(_, _, _)
                | InstKind::Log2(_, _, _, _)
                | InstKind::Log3(_, _, _, _, _)
                | InstKind::Log4(_, _, _, _, _, _)
                | InstKind::Gas
        )
    }

    fn can_mutate_memory(kind: &InstKind) -> bool {
        matches!(
            kind,
            InstKind::MStore8(_, _)
                | InstKind::MCopy(_, _, _)
                | InstKind::CalldataCopy(_, _, _)
                | InstKind::CodeCopy(_, _, _)
                | InstKind::ReturnDataCopy(_, _, _)
                | InstKind::ExtCodeCopy(_, _, _, _)
                | InstKind::Call { .. }
                | InstKind::StaticCall { .. }
                | InstKind::DelegateCall { .. }
                | InstKind::InternalCall { .. }
        )
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
    use crate::mir::{FunctionBuilder, FunctionId};
    use solar_interface::Ident;

    fn test_func() -> Function {
        Function::new(Ident::DUMMY)
    }

    #[test]
    fn removes_overwritten_store() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.imm_u64(128);
        let zero = builder.imm_u64(0);
        let value = builder.imm_u64(42);
        builder.mstore(addr, zero);
        builder.mstore(addr, value);
        builder.stop();

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 1);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 1);
    }

    #[test]
    fn forwards_store_observed_only_by_load() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.imm_u64(128);
        let zero = builder.imm_u64(0);
        let value = builder.imm_u64(42);
        builder.mstore(addr, zero);
        let loaded = builder.mload(addr);
        builder.mstore(addr, value);
        builder.ret(vec![loaded]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 2);

        let block = &func.blocks[func.entry_block];
        assert_eq!(block.instructions.len(), 1);
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(values.as_slice(), &[zero]);
    }

    #[test]
    fn gas_is_a_barrier() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.imm_u64(128);
        let zero = builder.imm_u64(0);
        let value = builder.imm_u64(42);
        builder.mstore(addr, zero);
        builder.gas();
        builder.mstore(addr, value);
        builder.stop();

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 0);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 3);
    }

    #[test]
    fn handles_distinct_immediate_values_for_same_address() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr1 = builder.imm_u64(128);
        let addr2 = builder.imm_u64(128);
        let zero = builder.imm_u64(0);
        let value = builder.imm_u64(42);
        builder.mstore(addr1, zero);
        builder.mstore(addr2, value);
        builder.stop();

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 1);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 1);
    }

    #[test]
    fn forwards_load_from_store() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.imm_u64(128);
        let value = builder.imm_u64(42);
        builder.mstore(addr, value);
        let loaded = builder.mload(addr);
        builder.ret(vec![loaded]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 1);

        let block = &func.blocks[func.entry_block];
        assert_eq!(block.instructions.len(), 1);
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(values.as_slice(), &[value]);
    }

    #[test]
    fn forwards_load_through_non_memory_operation() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.imm_u64(128);
        let value = builder.imm_u64(42);
        let one = builder.imm_u64(1);
        builder.mstore(addr, value);
        builder.add(one, one);
        let loaded = builder.mload(addr);
        builder.ret(vec![loaded]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 1);

        let block = &func.blocks[func.entry_block];
        assert_eq!(block.instructions.len(), 2);
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(values.as_slice(), &[value]);
    }

    #[test]
    fn does_not_forward_load_across_memory_write() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.imm_u64(128);
        let other_addr = builder.imm_u64(160);
        let value = builder.imm_u64(42);
        let len = builder.imm_u64(32);
        builder.mstore(addr, value);
        builder.mcopy(other_addr, addr, len);
        let loaded = builder.mload(addr);
        builder.ret(vec![loaded]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 0);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 3);
    }

    #[test]
    fn does_not_forward_load_across_internal_call() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr = builder.imm_u64(128);
        let value = builder.imm_u64(42);
        builder.mstore(addr, value);
        builder.internal_call(FunctionId::from_usize(0), Vec::new(), None, 0);
        let loaded = builder.mload(addr);
        builder.ret(vec![loaded]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 0);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 3);
    }

    #[test]
    fn resolves_chained_forwarded_loads() {
        let mut func = test_func();
        let mut builder = FunctionBuilder::new(&mut func);
        let addr1 = builder.imm_u64(128);
        let addr2 = builder.imm_u64(160);
        let value = builder.imm_u64(42);
        builder.mstore(addr1, value);
        let loaded1 = builder.mload(addr1);
        builder.mstore(addr2, loaded1);
        let loaded2 = builder.mload(addr2);
        builder.ret(vec![loaded2]);

        let mut pass = MemoryStoreEliminator::new();
        assert_eq!(pass.run(&mut func), 2);

        let block = &func.blocks[func.entry_block];
        assert_eq!(block.instructions.len(), 2);
        let Some(Terminator::Return { values }) = &block.terminator else {
            panic!("expected return terminator");
        };
        assert_eq!(values.as_slice(), &[value]);
    }
}
