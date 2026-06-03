//! Local dead memory-store elimination.
//!
//! This pass removes full-word `mstore` instructions that are overwritten by a
//! later full-word `mstore` to the same exact address within the same basic
//! block, before any operation can observe memory or gas.

use crate::mir::{BlockId, Function, InstId, InstKind, Value, ValueId};
use rustc_hash::FxHashSet;

/// Local dead memory-store elimination pass.
#[derive(Debug, Default)]
pub struct MemoryStoreEliminator {
    /// Number of stores eliminated.
    pub eliminated_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum MemAddrKey {
    Const(u64),
    Value(ValueId),
}

impl MemoryStoreEliminator {
    /// Creates a new memory DSE pass.
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs memory DSE on a function.
    pub fn run(&mut self, func: &mut Function) -> usize {
        self.eliminated_count = 0;

        let block_ids: Vec<BlockId> = func.blocks.indices().collect();
        for block_id in block_ids {
            self.process_block(func, block_id);
        }

        self.eliminated_count
    }

    /// Runs memory DSE until no more stores can be eliminated.
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::FunctionBuilder;
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
    fn keeps_store_observed_by_load() {
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
        assert_eq!(pass.run(&mut func), 0);
        assert_eq!(func.blocks[func.entry_block].instructions.len(), 3);
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
}
