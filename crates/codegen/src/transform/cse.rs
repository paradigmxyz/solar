//! Common Subexpression Elimination (CSE) optimization pass.
//!
//! This pass identifies and eliminates redundant computations within basic blocks.
//! When the same expression is computed multiple times with the same operands,
//! only the first computation is kept and subsequent uses reference the cached result.
//!
//! ## Example
//!
//! Before CSE:
//! ```text
//! v1 = add v0, 42
//! v2 = mul v1, 2
//! v3 = add v0, 42  // redundant - same as v1
//! v4 = mul v3, 3
//! ```
//!
//! After CSE:
//! ```text
//! v1 = add v0, 42
//! v2 = mul v1, 2
//! // v3 removed, uses of v3 replaced with v1
//! v4 = mul v1, 3
//! ```
//!
//! ## Limitations
//!
//! - Only operates within basic blocks (local CSE)
//! - Does not track across memory operations (conservative for side effects)
//! - Does not handle expressions with different but equivalent orderings

use crate::mir::{BlockId, Function, InstId, InstKind, Value, ValueId};
use rustc_hash::FxHashMap;

/// Common Subexpression Elimination pass.
#[derive(Debug, Default)]
pub struct CommonSubexprEliminator {
    /// Number of instructions eliminated.
    pub eliminated_count: usize,
}

/// A normalized expression key for CSE lookup.
/// Expressions are normalized so that equivalent computations map to the same key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ExprKey {
    Add(ValueId, ValueId),
    Sub(ValueId, ValueId),
    Mul(ValueId, ValueId),
    Div(ValueId, ValueId),
    Mod(ValueId, ValueId),
    And(ValueId, ValueId),
    Or(ValueId, ValueId),
    Xor(ValueId, ValueId),
    Shl(ValueId, ValueId),
    Shr(ValueId, ValueId),
    Lt(ValueId, ValueId),
    Gt(ValueId, ValueId),
    Eq(ValueId, ValueId),
    IsZero(ValueId),
    Not(ValueId),
    SLoad(ValueId),
}

impl CommonSubexprEliminator {
    /// Creates a new CSE pass.
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs CSE on a function.
    /// Returns the number of expressions eliminated.
    pub fn run(&mut self, func: &mut Function) -> usize {
        self.eliminated_count = 0;

        // Process each block independently (local CSE)
        let block_ids: Vec<BlockId> = func.blocks.indices().collect();
        for block_id in block_ids {
            self.process_block(func, block_id);
        }

        self.eliminated_count
    }

    /// Runs CSE iteratively until no more changes.
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

    /// Processes a single basic block.
    fn process_block(&mut self, func: &mut Function, block_id: BlockId) {
        // Map from expression key to the ValueId that computed it
        let mut expr_cache: FxHashMap<ExprKey, ValueId> = FxHashMap::default();

        // Map from ValueId to its replacement ValueId
        let mut replacements: FxHashMap<ValueId, ValueId> = FxHashMap::default();

        // Instructions to remove (marked by position)
        let mut to_remove: Vec<InstId> = Vec::new();

        // Get instruction list for this block
        let block = func.block(block_id);
        let inst_ids: Vec<InstId> = block.instructions.clone();

        for inst_id in inst_ids {
            let inst = &func.instructions[inst_id];

            // Skip side-effecting instructions
            if inst.kind.has_side_effects() {
                // Memory/storage writes invalidate cached SLOADs
                if matches!(inst.kind, InstKind::SStore(_, _)) {
                    // Conservative: invalidate all SLOADs on any SSTORE
                    expr_cache.retain(|k, _| !matches!(k, ExprKey::SLoad(_)));
                }
                continue;
            }

            // Try to create an expression key
            if let Some(key) = self.make_expr_key(&inst.kind, &replacements) {
                // Find the result ValueId for this instruction
                let result_value = self.find_result_value(func, inst_id);

                if let Some(result) = result_value {
                    if let Some(&cached_value) = expr_cache.get(&key) {
                        // This expression was already computed - mark for elimination
                        replacements.insert(result, cached_value);
                        to_remove.push(inst_id);
                        self.eliminated_count += 1;
                    } else {
                        // First occurrence - cache it
                        expr_cache.insert(key, result);
                    }
                }
            }
        }

        // Apply replacements to all remaining instructions in the block
        if !replacements.is_empty() {
            self.apply_replacements(func, block_id, &replacements);
        }

        // Remove eliminated instructions
        let block = func.block_mut(block_id);
        block.instructions.retain(|id| !to_remove.contains(id));
    }

    /// Creates a normalized expression key for an instruction.
    /// Returns None for instructions that shouldn't be cached.
    fn make_expr_key(
        &self,
        kind: &InstKind,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> Option<ExprKey> {
        // Helper to get canonical ValueId (after replacements)
        let canonical = |v: ValueId| *replacements.get(&v).unwrap_or(&v);

        match kind {
            // Commutative operations - normalize operand order
            InstKind::Add(a, b) => {
                let (a, b) = (canonical(*a), canonical(*b));
                Some(ExprKey::Add(a.min(b), a.max(b)))
            }
            InstKind::Mul(a, b) => {
                let (a, b) = (canonical(*a), canonical(*b));
                Some(ExprKey::Mul(a.min(b), a.max(b)))
            }
            InstKind::And(a, b) => {
                let (a, b) = (canonical(*a), canonical(*b));
                Some(ExprKey::And(a.min(b), a.max(b)))
            }
            InstKind::Or(a, b) => {
                let (a, b) = (canonical(*a), canonical(*b));
                Some(ExprKey::Or(a.min(b), a.max(b)))
            }
            InstKind::Xor(a, b) => {
                let (a, b) = (canonical(*a), canonical(*b));
                Some(ExprKey::Xor(a.min(b), a.max(b)))
            }
            InstKind::Eq(a, b) => {
                let (a, b) = (canonical(*a), canonical(*b));
                Some(ExprKey::Eq(a.min(b), a.max(b)))
            }

            // Non-commutative operations - preserve order
            InstKind::Sub(a, b) => Some(ExprKey::Sub(canonical(*a), canonical(*b))),
            InstKind::Div(a, b) => Some(ExprKey::Div(canonical(*a), canonical(*b))),
            InstKind::Mod(a, b) => Some(ExprKey::Mod(canonical(*a), canonical(*b))),
            InstKind::Shl(a, b) => Some(ExprKey::Shl(canonical(*a), canonical(*b))),
            InstKind::Shr(a, b) => Some(ExprKey::Shr(canonical(*a), canonical(*b))),
            InstKind::Lt(a, b) => Some(ExprKey::Lt(canonical(*a), canonical(*b))),
            InstKind::Gt(a, b) => Some(ExprKey::Gt(canonical(*a), canonical(*b))),

            // Unary operations
            InstKind::IsZero(a) => Some(ExprKey::IsZero(canonical(*a))),
            InstKind::Not(a) => Some(ExprKey::Not(canonical(*a))),

            // Storage reads can be cached within a block (until a write)
            InstKind::SLoad(slot) => Some(ExprKey::SLoad(canonical(*slot))),

            // Don't cache these:
            // - Memory operations (MLOAD) - memory can be modified
            // - Environment reads - values might change
            // - Phi nodes - not expressions
            // - Calls - side effects
            _ => None,
        }
    }

    /// Finds the ValueId that represents the result of an instruction.
    fn find_result_value(&self, func: &Function, inst_id: InstId) -> Option<ValueId> {
        for (value_id, value) in func.values.iter_enumerated() {
            if let Value::Inst(id) = value
                && *id == inst_id
            {
                return Some(value_id);
            }
        }
        None
    }

    /// Applies value replacements to all instructions in a block.
    fn apply_replacements(
        &self,
        func: &mut Function,
        block_id: BlockId,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) {
        let block = func.block(block_id);
        let inst_ids: Vec<InstId> = block.instructions.clone();

        for inst_id in inst_ids {
            let inst = &mut func.instructions[inst_id];
            Self::replace_operands(&mut inst.kind, replacements);
        }

        // Also update terminator if present
        let block = func.block_mut(block_id);
        if let Some(term) = &mut block.terminator {
            Self::replace_terminator_operands(term, replacements);
        }
    }

    /// Replaces operands in an instruction kind.
    fn replace_operands(kind: &mut InstKind, replacements: &FxHashMap<ValueId, ValueId>) {
        let replace = |v: &mut ValueId| {
            if let Some(&new_v) = replacements.get(v) {
                *v = new_v;
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

            InstKind::Phi(incoming) => {
                for (_, val) in incoming {
                    replace(val);
                }
            }

            // Nullary operations - no operands
            InstKind::MSize
            | InstKind::CalldataSize
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

    /// Replaces operands in a terminator.
    fn replace_terminator_operands(
        term: &mut crate::mir::Terminator,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) {
        use crate::mir::Terminator;

        let replace = |v: &mut ValueId| {
            if let Some(&new_v) = replacements.get(v) {
                *v = new_v;
            }
        };

        match term {
            Terminator::Jump(_) | Terminator::Stop | Terminator::Invalid => {}
            Terminator::Branch { condition, .. } => {
                replace(condition);
            }
            Terminator::Switch { value, cases, .. } => {
                replace(value);
                for (case_val, _) in cases {
                    replace(case_val);
                }
            }
            Terminator::Return { values } => {
                for val in values {
                    replace(val);
                }
            }
            Terminator::Revert { offset, size } => {
                replace(offset);
                replace(size);
            }
            Terminator::SelfDestruct { recipient } => {
                replace(recipient);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{Function, Immediate, Instruction, MirType, Terminator, Value};
    use smallvec::smallvec;
    use solar_interface::Ident;

    fn make_test_func() -> Function {
        let name = Ident::DUMMY;
        Function::new(name)
    }

    #[test]
    fn test_cse_eliminates_duplicate_add() {
        let mut func = make_test_func();

        // Create values: v0 = 10, v1 = 20
        let v0 = func
            .alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(10))));
        let v1 = func
            .alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(20))));

        // Use the existing entry block
        let entry = func.entry_block;

        // Add instruction: v2 = add v0, v1
        let add1_inst =
            func.alloc_inst(Instruction::new(InstKind::Add(v0, v1), Some(MirType::uint256())));
        let v2 = func.alloc_value(Value::Inst(add1_inst));
        func.block_mut(entry).instructions.push(add1_inst);

        // Duplicate add: v3 = add v0, v1 (should be eliminated)
        let add2_inst =
            func.alloc_inst(Instruction::new(InstKind::Add(v0, v1), Some(MirType::uint256())));
        let _v3 = func.alloc_value(Value::Inst(add2_inst));
        func.block_mut(entry).instructions.push(add2_inst);

        // Return v2
        func.block_mut(entry).terminator = Some(Terminator::Return { values: smallvec![v2] });

        // Run CSE
        let mut cse = CommonSubexprEliminator::new();
        let eliminated = cse.run(&mut func);

        assert_eq!(eliminated, 1, "Should eliminate one duplicate expression");
        assert_eq!(func.block(entry).instructions.len(), 1, "Should have only one ADD instruction");
    }

    #[test]
    fn test_cse_commutative() {
        let mut func = make_test_func();

        let v0 = func
            .alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(10))));
        let v1 = func
            .alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(20))));

        let entry = func.entry_block;

        // v2 = add v0, v1
        let add1 =
            func.alloc_inst(Instruction::new(InstKind::Add(v0, v1), Some(MirType::uint256())));
        let _v2 = func.alloc_value(Value::Inst(add1));
        func.block_mut(entry).instructions.push(add1);

        // v3 = add v1, v0 (should be eliminated - commutative)
        let add2 =
            func.alloc_inst(Instruction::new(InstKind::Add(v1, v0), Some(MirType::uint256())));
        let _v3 = func.alloc_value(Value::Inst(add2));
        func.block_mut(entry).instructions.push(add2);

        func.block_mut(entry).terminator = Some(Terminator::Stop);

        let mut cse = CommonSubexprEliminator::new();
        let eliminated = cse.run(&mut func);

        assert_eq!(eliminated, 1, "Commutative operations should be recognized as equal");
    }

    #[test]
    fn test_cse_non_commutative_not_eliminated() {
        let mut func = make_test_func();

        let v0 = func
            .alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(10))));
        let v1 = func
            .alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(20))));

        let entry = func.entry_block;

        // v2 = sub v0, v1
        let sub1 =
            func.alloc_inst(Instruction::new(InstKind::Sub(v0, v1), Some(MirType::uint256())));
        let _v2 = func.alloc_value(Value::Inst(sub1));
        func.block_mut(entry).instructions.push(sub1);

        // v3 = sub v1, v0 (different - should NOT be eliminated)
        let sub2 =
            func.alloc_inst(Instruction::new(InstKind::Sub(v1, v0), Some(MirType::uint256())));
        let _v3 = func.alloc_value(Value::Inst(sub2));
        func.block_mut(entry).instructions.push(sub2);

        func.block_mut(entry).terminator = Some(Terminator::Stop);

        let mut cse = CommonSubexprEliminator::new();
        let eliminated = cse.run(&mut func);

        assert_eq!(
            eliminated, 0,
            "Non-commutative operations with swapped operands should not be eliminated"
        );
    }
}
