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
//! - Does not perform PRE or global value numbering across branches
//!
//! Safety contract:
//! - cache only pure expressions, classified memory reads, and exact storage or transient-storage
//!   reads
//! - invalidate memory reads by overlapping memory writes and unknown memory effects
//! - invalidate storage reads by possibly-aliasing writes or calls that may re-enter and mutate the
//!   current contract

use crate::{
    mir::{
        BlockId, Function, Immediate, InstId, InstKind, MemoryRegion, StorageAlias, Value, ValueId,
    },
    pass::FunctionPass,
};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashMap;
use std::cmp::Ordering;

/// Common Subexpression Elimination pass.
#[derive(Debug, Default)]
pub struct CommonSubexprEliminator {
    /// Number of instructions eliminated.
    pub eliminated_count: usize,
}

/// Function pass for local common subexpression elimination.
pub struct CsePass;

impl FunctionPass for CsePass {
    fn name(&self) -> &str {
        "cse"
    }

    fn run_on_function(&mut self, func: &mut Function) -> bool {
        CommonSubexprEliminator::new().run_to_fixpoint(func) != 0
    }
}

/// A normalized expression key for CSE lookup.
/// Expressions are normalized so that equivalent computations map to the same key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ExprKey {
    Add(OperandKey, OperandKey),
    Sub(OperandKey, OperandKey),
    Mul(OperandKey, OperandKey),
    Div(OperandKey, OperandKey),
    SDiv(OperandKey, OperandKey),
    Mod(OperandKey, OperandKey),
    SMod(OperandKey, OperandKey),
    Exp(OperandKey, OperandKey),
    AddMod(OperandKey, OperandKey, OperandKey),
    MulMod(OperandKey, OperandKey, OperandKey),
    And(OperandKey, OperandKey),
    Or(OperandKey, OperandKey),
    Xor(OperandKey, OperandKey),
    Shl(OperandKey, OperandKey),
    Shr(OperandKey, OperandKey),
    Sar(OperandKey, OperandKey),
    Byte(OperandKey, OperandKey),
    Lt(OperandKey, OperandKey),
    Gt(OperandKey, OperandKey),
    SLt(OperandKey, OperandKey),
    SGt(OperandKey, OperandKey),
    Eq(OperandKey, OperandKey),
    IsZero(OperandKey),
    Not(OperandKey),
    SignExtend(OperandKey, OperandKey),
    Select(OperandKey, OperandKey, OperandKey),
    MLoad(MemRangeKey),
    Keccak256(MemRangeKey),
    SLoad(StorageAlias),
    TLoad(StorageAlias),
    CalldataLoad(OperandKey),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum OperandKey {
    Value(ValueId),
    Immediate(Immediate),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct MemRangeKey {
    region: MemoryRegion,
    base: Option<ValueId>,
    offset: Option<u64>,
    size: Option<u64>,
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
            let kind = inst.kind.clone();

            if kind.has_side_effects() {
                self.invalidate_for_side_effect(
                    func,
                    inst_id,
                    &kind,
                    &replacements,
                    &mut expr_cache,
                );
                continue;
            }

            // Try to create an expression key
            if let Some(key) = self.make_expr_key(func, inst_id, &kind, &replacements) {
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
        func: &Function,
        inst_id: InstId,
        kind: &InstKind,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> Option<ExprKey> {
        // Helper to get canonical operands after in-block replacements.
        let operand = |v: ValueId| Self::operand_key(func, v, replacements);
        let value = |v: ValueId| Self::canonical_value(v, replacements);

        match kind {
            // Commutative operations - normalize operand order
            InstKind::Add(a, b) => {
                let (a, b) = Self::ordered_pair(operand(*a), operand(*b));
                Some(ExprKey::Add(a, b))
            }
            InstKind::Mul(a, b) => {
                let (a, b) = Self::ordered_pair(operand(*a), operand(*b));
                Some(ExprKey::Mul(a, b))
            }
            InstKind::And(a, b) => {
                let (a, b) = Self::ordered_pair(operand(*a), operand(*b));
                Some(ExprKey::And(a, b))
            }
            InstKind::Or(a, b) => {
                let (a, b) = Self::ordered_pair(operand(*a), operand(*b));
                Some(ExprKey::Or(a, b))
            }
            InstKind::Xor(a, b) => {
                let (a, b) = Self::ordered_pair(operand(*a), operand(*b));
                Some(ExprKey::Xor(a, b))
            }
            InstKind::Eq(a, b) => {
                let (a, b) = Self::ordered_pair(operand(*a), operand(*b));
                Some(ExprKey::Eq(a, b))
            }

            // Non-commutative operations - preserve order
            InstKind::Sub(a, b) => Some(ExprKey::Sub(operand(*a), operand(*b))),
            InstKind::Div(a, b) => Some(ExprKey::Div(operand(*a), operand(*b))),
            InstKind::SDiv(a, b) => Some(ExprKey::SDiv(operand(*a), operand(*b))),
            InstKind::Mod(a, b) => Some(ExprKey::Mod(operand(*a), operand(*b))),
            InstKind::SMod(a, b) => Some(ExprKey::SMod(operand(*a), operand(*b))),
            InstKind::Exp(a, b) => Some(ExprKey::Exp(operand(*a), operand(*b))),
            InstKind::AddMod(a, b, n) => {
                let (a, b) = Self::ordered_pair(operand(*a), operand(*b));
                Some(ExprKey::AddMod(a, b, operand(*n)))
            }
            InstKind::MulMod(a, b, n) => {
                let (a, b) = Self::ordered_pair(operand(*a), operand(*b));
                Some(ExprKey::MulMod(a, b, operand(*n)))
            }
            InstKind::Shl(a, b) => Some(ExprKey::Shl(operand(*a), operand(*b))),
            InstKind::Shr(a, b) => Some(ExprKey::Shr(operand(*a), operand(*b))),
            InstKind::Sar(a, b) => Some(ExprKey::Sar(operand(*a), operand(*b))),
            InstKind::Byte(a, b) => Some(ExprKey::Byte(operand(*a), operand(*b))),
            InstKind::Lt(a, b) => Some(ExprKey::Lt(operand(*a), operand(*b))),
            InstKind::Gt(a, b) => Some(ExprKey::Gt(operand(*a), operand(*b))),
            InstKind::SLt(a, b) => Some(ExprKey::SLt(operand(*a), operand(*b))),
            InstKind::SGt(a, b) => Some(ExprKey::SGt(operand(*a), operand(*b))),
            InstKind::SignExtend(a, b) => Some(ExprKey::SignExtend(operand(*a), operand(*b))),

            // Unary operations
            InstKind::IsZero(a) => Some(ExprKey::IsZero(operand(*a))),
            InstKind::Not(a) => Some(ExprKey::Not(operand(*a))),
            InstKind::CalldataLoad(a) => Some(ExprKey::CalldataLoad(operand(*a))),

            InstKind::Select(condition, then_value, else_value) => Some(ExprKey::Select(
                operand(*condition),
                operand(*then_value),
                operand(*else_value),
            )),

            InstKind::MLoad(addr) => {
                let key = self.memory_range_key(func, inst_id, value(*addr), Some(32))?;
                Some(ExprKey::MLoad(key))
            }
            InstKind::Keccak256(offset, size) => {
                let size = Self::const_u64(func, value(*size));
                let key = self.memory_range_key(func, inst_id, value(*offset), size)?;
                Some(ExprKey::Keccak256(key))
            }

            InstKind::SLoad(slot) => {
                Some(ExprKey::SLoad(self.storage_alias(func, inst_id, *slot, replacements)))
            }
            InstKind::TLoad(slot) => {
                Some(ExprKey::TLoad(self.storage_alias(func, inst_id, *slot, replacements)))
            }

            // Don't cache these:
            // - Cheap nullary environment reads can extend stack lifetimes more than they save
            // - Memory size/gas/returndata-size reads can change inside a block
            // - Storage writes - side effects
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

    fn invalidate_for_side_effect(
        &self,
        func: &Function,
        inst_id: InstId,
        kind: &InstKind,
        replacements: &FxHashMap<ValueId, ValueId>,
        expr_cache: &mut FxHashMap<ExprKey, ValueId>,
    ) {
        match *kind {
            InstKind::MStore(addr, _) => {
                let addr = Self::canonical_value(addr, replacements);
                self.invalidate_memory(
                    expr_cache,
                    self.memory_range_key(func, inst_id, addr, Some(32)),
                );
            }
            InstKind::MStore8(addr, _) => {
                let addr = Self::canonical_value(addr, replacements);
                self.invalidate_memory(
                    expr_cache,
                    self.memory_range_key(func, inst_id, addr, Some(1)),
                );
            }
            InstKind::MCopy(dest, _, size)
            | InstKind::CalldataCopy(dest, _, size)
            | InstKind::CodeCopy(dest, _, size)
            | InstKind::ReturnDataCopy(dest, _, size) => {
                let dest = Self::canonical_value(dest, replacements);
                let size = Self::const_u64(func, Self::canonical_value(size, replacements));
                self.invalidate_memory(
                    expr_cache,
                    self.memory_range_key(func, inst_id, dest, size),
                );
            }
            InstKind::ExtCodeCopy(_, dest, _, size) => {
                let dest = Self::canonical_value(dest, replacements);
                let size = Self::const_u64(func, Self::canonical_value(size, replacements));
                self.invalidate_memory(
                    expr_cache,
                    self.memory_range_key(func, inst_id, dest, size),
                );
            }
            InstKind::SStore(slot, _) => {
                let alias = self.storage_alias(func, inst_id, slot, replacements);
                expr_cache.retain(|key, _| match key {
                    ExprKey::SLoad(cached) => !cached.may_alias(alias),
                    _ => true,
                });
            }
            InstKind::TStore(slot, _) => {
                let alias = self.storage_alias(func, inst_id, slot, replacements);
                expr_cache.retain(|key, _| match key {
                    ExprKey::TLoad(cached) => !cached.may_alias(alias),
                    _ => true,
                });
            }
            _ if kind.may_mutate_memory() => {
                expr_cache.retain(|key, _| !Self::is_memory_expr(key));
                if kind.may_mutate_storage() {
                    expr_cache.retain(|key, _| !matches!(key, ExprKey::SLoad(_)));
                }
                if kind.may_mutate_transient_storage() {
                    expr_cache.retain(|key, _| !matches!(key, ExprKey::TLoad(_)));
                }
            }
            _ if kind.may_mutate_storage() => {
                expr_cache.retain(|key, _| !matches!(key, ExprKey::SLoad(_)));
            }
            _ if kind.may_mutate_transient_storage() => {
                expr_cache.retain(|key, _| !matches!(key, ExprKey::TLoad(_)));
            }
            _ => {}
        }
    }

    fn invalidate_memory(
        &self,
        expr_cache: &mut FxHashMap<ExprKey, ValueId>,
        write: Option<MemRangeKey>,
    ) {
        expr_cache.retain(|key, _| match key {
            ExprKey::MLoad(read) | ExprKey::Keccak256(read) => {
                write.is_some_and(|write| !Self::memory_ranges_may_alias(*read, write))
            }
            _ => true,
        });
    }

    fn is_memory_expr(key: &ExprKey) -> bool {
        matches!(key, ExprKey::MLoad(_) | ExprKey::Keccak256(_))
    }

    fn storage_alias(
        &self,
        func: &Function,
        inst_id: InstId,
        slot: ValueId,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> StorageAlias {
        let original_slot = slot;
        let slot = Self::canonical_value(slot, replacements);
        if slot == original_slot {
            func.instructions[inst_id]
                .metadata
                .storage_alias
                .unwrap_or_else(|| StorageAlias::for_value(func, slot))
        } else {
            StorageAlias::for_value(func, slot)
        }
    }

    fn memory_range_key(
        &self,
        func: &Function,
        inst_id: InstId,
        addr: ValueId,
        size: Option<u64>,
    ) -> Option<MemRangeKey> {
        let region = func.instructions[inst_id]
            .metadata
            .memory_region
            .unwrap_or_else(|| Self::memory_region_for_addr(func, addr));
        let (base, offset) = Self::memory_addr_base_offset(func, addr);
        Some(MemRangeKey { region, base, offset, size })
    }

    fn memory_region_for_addr(func: &Function, addr: ValueId) -> MemoryRegion {
        match func.value(addr) {
            Value::Immediate(imm)
                if imm.as_u256().is_some_and(|value| value < U256::from(0x80)) =>
            {
                MemoryRegion::Scratch
            }
            _ => MemoryRegion::Unknown,
        }
    }

    fn memory_addr_base_offset(func: &Function, addr: ValueId) -> (Option<ValueId>, Option<u64>) {
        match func.value(addr) {
            Value::Immediate(imm) => (None, imm.as_u256().and_then(Self::u256_to_u64)),
            Value::Inst(inst_id) => match func.instructions[*inst_id].kind {
                InstKind::Add(lhs, rhs) => {
                    if let Some(offset) = Self::const_u64(func, rhs) {
                        (Some(lhs), Some(offset))
                    } else if let Some(offset) = Self::const_u64(func, lhs) {
                        (Some(rhs), Some(offset))
                    } else {
                        (Some(addr), Some(0))
                    }
                }
                InstKind::Sub(lhs, rhs) => {
                    if Self::const_u64(func, rhs).is_some() {
                        (Some(lhs), None)
                    } else {
                        (Some(addr), Some(0))
                    }
                }
                _ => (Some(addr), Some(0)),
            },
            Value::Arg { .. } | Value::Phi { .. } | Value::Undef(_) => (Some(addr), Some(0)),
        }
    }

    fn memory_ranges_may_alias(read: MemRangeKey, write: MemRangeKey) -> bool {
        if read.region != MemoryRegion::Unknown
            && write.region != MemoryRegion::Unknown
            && read.region != write.region
        {
            return false;
        }
        if read.base != write.base {
            return true;
        }
        let (Some(read_offset), Some(read_size), Some(write_offset), Some(write_size)) =
            (read.offset, read.size, write.offset, write.size)
        else {
            return true;
        };
        Self::ranges_overlap(read_offset, read_size, write_offset, write_size)
    }

    fn ranges_overlap(a_start: u64, a_size: u64, b_start: u64, b_size: u64) -> bool {
        let Some(a_end) = a_start.checked_add(a_size) else { return true };
        let Some(b_end) = b_start.checked_add(b_size) else { return true };
        a_start < b_end && b_start < a_end
    }

    fn const_u64(func: &Function, value: ValueId) -> Option<u64> {
        let Value::Immediate(imm) = func.value(value) else { return None };
        imm.as_u256().and_then(Self::u256_to_u64)
    }

    fn u256_to_u64(value: U256) -> Option<u64> {
        value.try_into().ok()
    }

    fn canonical_value(mut value: ValueId, replacements: &FxHashMap<ValueId, ValueId>) -> ValueId {
        while let Some(&replacement) = replacements.get(&value) {
            if replacement == value {
                break;
            }
            value = replacement;
        }
        value
    }

    fn operand_key(
        func: &Function,
        value: ValueId,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> OperandKey {
        let value = Self::canonical_value(value, replacements);
        match func.value(value) {
            Value::Immediate(imm) => OperandKey::Immediate(imm.clone()),
            _ => OperandKey::Value(value),
        }
    }

    fn ordered_pair(a: OperandKey, b: OperandKey) -> (OperandKey, OperandKey) {
        if Self::cmp_operand_key(&a, &b).is_gt() { (b, a) } else { (a, b) }
    }

    fn cmp_operand_key(a: &OperandKey, b: &OperandKey) -> Ordering {
        match (a, b) {
            (OperandKey::Immediate(a), OperandKey::Immediate(b)) => Self::cmp_immediate(a, b),
            (OperandKey::Immediate(_), OperandKey::Value(_)) => Ordering::Less,
            (OperandKey::Value(_), OperandKey::Immediate(_)) => Ordering::Greater,
            (OperandKey::Value(a), OperandKey::Value(b)) => a.index().cmp(&b.index()),
        }
    }

    fn cmp_immediate(a: &Immediate, b: &Immediate) -> Ordering {
        let rank = |imm: &Immediate| match imm {
            Immediate::Bool(_) => 0,
            Immediate::UInt(_, _) => 1,
            Immediate::Int(_, _) => 2,
            Immediate::Address(_) => 3,
            Immediate::FixedBytes(_, _) => 4,
        };
        rank(a).cmp(&rank(b)).then_with(|| match (a, b) {
            (Immediate::Bool(a), Immediate::Bool(b)) => a.cmp(b),
            (Immediate::UInt(a_value, a_bits), Immediate::UInt(b_value, b_bits))
            | (Immediate::Int(a_value, a_bits), Immediate::Int(b_value, b_bits)) => {
                a_bits.cmp(b_bits).then_with(|| a_value.cmp(b_value))
            }
            (Immediate::Address(a), Immediate::Address(b)) => a.cmp(b),
            (Immediate::FixedBytes(a_value, a_len), Immediate::FixedBytes(b_value, b_len)) => {
                a_len.cmp(b_len).then_with(|| a_value.cmp(b_value))
            }
            _ => Ordering::Equal,
        })
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
            if Self::replace_operands(&mut inst.kind, replacements) {
                if Self::is_memory_inst(&inst.kind) {
                    inst.metadata.memory_region = None;
                }
                if matches!(
                    inst.kind,
                    InstKind::SLoad(_)
                        | InstKind::SStore(_, _)
                        | InstKind::TLoad(_)
                        | InstKind::TStore(_, _)
                ) {
                    inst.metadata.storage_alias = None;
                }
            }
        }

        // Also update terminator if present
        let block = func.block_mut(block_id);
        if let Some(term) = &mut block.terminator {
            Self::replace_terminator_operands(term, replacements);
        }
    }

    fn replace_operands(kind: &mut InstKind, replacements: &FxHashMap<ValueId, ValueId>) -> bool {
        let mut changed = false;
        kind.visit_operands_mut(|v| {
            let new_v = Self::canonical_value(*v, replacements);
            if new_v != *v {
                *v = new_v;
                changed = true;
            }
        });

        changed
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
            Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
                replace(offset);
                replace(size);
            }
            Terminator::SelfDestruct { recipient } => {
                replace(recipient);
            }
        }
    }

    fn is_memory_inst(kind: &InstKind) -> bool {
        matches!(
            kind,
            InstKind::MLoad(_)
                | InstKind::MStore(_, _)
                | InstKind::MStore8(_, _)
                | InstKind::MCopy(_, _, _)
                | InstKind::CalldataCopy(_, _, _)
                | InstKind::CodeCopy(_, _, _)
                | InstKind::ReturnDataCopy(_, _, _)
                | InstKind::ExtCodeCopy(_, _, _, _)
                | InstKind::Keccak256(_, _)
        )
    }
}
