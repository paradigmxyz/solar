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
//! The pass performs dominator-tree CSE with path-local invalidation for
//! alias-sensitive memory/storage reads, then runs a local cleanup pass.
//!
//! Safety contract:
//! - cache only pure expressions, classified memory reads, and exact storage or transient-storage
//!   reads
//! - invalidate memory reads by overlapping memory writes and unknown memory effects
//! - invalidate storage reads by possibly-aliasing writes or calls that may re-enter and mutate the
//!   current contract
//! - when inheriting a cache across a dominator-tree edge, also invalidate state-dependent reads by
//!   clobbers in every block that can lie on a CFG path between the dominator and its child
//!   (diamond arms, loop bodies), including the child itself when it sits on a cycle

use crate::{
    analysis::{CfgInfo, DominatorTree},
    mir::{
        BlockId, Function, Immediate, InstId, InstKind, Instruction, MemoryRegion, MirType,
        StorageAlias, Value, ValueId,
    },
    pass::FunctionPass,
    utils::mir as mir_utils,
};
use alloy_primitives::U256;
use solar_data_structures::map::{FxHashMap, FxHashSet};
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
    Offset(OperandKey, U256),
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
    /// Also keys `Gt(a, b)`, normalized as `Lt(b, a)`.
    Lt(OperandKey, OperandKey),
    /// Also keys `SGt(a, b)`, normalized as `SLt(b, a)`.
    SLt(OperandKey, OperandKey),
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
    ExtCodeSize(OperandKey),
    ExtCodeHash(OperandKey),
    BlockHash(OperandKey),
    Balance(OperandKey),
    SelfBalance,
    BlobHash(OperandKey),
    LoadImmutable(u32),
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
    /// The canonical size operand when `size` is not a known constant.
    ///
    /// Participates only in key equality so that reads with different dynamic sizes never
    /// unify while reads with the same dynamic size operand still do. Aliasing checks ignore
    /// it: `memory_ranges_may_alias` stays conservative whenever `size` is `None`, so write
    /// keys can always leave it unset.
    dyn_size: Option<ValueId>,
}

struct GlobalCseContext<'a> {
    dom_tree: &'a DominatorTree,
    inst_results: &'a FxHashMap<InstId, ValueId>,
    block_clobbers: &'a FxHashMap<BlockId, Vec<Clobber>>,
    reachability: &'a FxHashMap<BlockId, FxHashSet<BlockId>>,
    replacements: &'a mut FxHashMap<ValueId, ValueId>,
    dead: &'a mut FxHashSet<InstId>,
}

/// A single cache-invalidating effect of a side-effecting instruction.
#[derive(Clone, Copy, Debug)]
enum Clobber {
    /// A memory write; `None` clobbers all of memory.
    Memory(Option<MemRangeKey>),
    /// A storage write to a possibly-aliasing slot.
    Storage(StorageAlias),
    /// An effect that may mutate any storage slot (e.g. a re-entering call).
    AllStorage,
    /// A transient-storage write to a possibly-aliasing slot.
    Transient(StorageAlias),
    /// An effect that may mutate any transient-storage slot.
    AllTransient,
    /// An effect that may change account balances or deployed code.
    AccountEnvironment,
}

struct PhiExpressionCandidate {
    block_id: BlockId,
    phi_inst: InstId,
    phi_result: ValueId,
    kind: InstKind,
    result_ty: MirType,
    incoming: Vec<(ValueId, InstId)>,
}

struct PhiSinkContext<'a> {
    dominators: &'a DominatorTree,
    inst_blocks: &'a FxHashMap<InstId, BlockId>,
    inst_results: &'a FxHashMap<InstId, ValueId>,
    replacements: &'a FxHashMap<ValueId, ValueId>,
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

        self.sink_redundant_phi_expressions(func);

        // Neither the global nor the local pass allocates values, so the map stays valid.
        let inst_results = mir_utils::inst_results(func);
        self.process_global_pure(func, &inst_results);

        // Process each block independently (local CSE)
        let block_ids: Vec<BlockId> = func.blocks.indices().collect();
        for block_id in block_ids {
            self.process_block(func, block_id, &inst_results);
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

    fn process_global_pure(
        &mut self,
        func: &mut Function,
        inst_results: &FxHashMap<InstId, ValueId>,
    ) {
        let mut cfg = CfgInfo::new(func);
        let block_clobbers = self.block_clobber_summaries(func);
        let reachability = if block_clobbers.is_empty() {
            FxHashMap::default()
        } else {
            cfg.transitive_reachability().clone()
        };
        let mut replacements = FxHashMap::default();
        let mut dead = FxHashSet::default();
        let mut cache = FxHashMap::default();
        let mut ctx = GlobalCseContext {
            dom_tree: cfg.dominators(),
            inst_results,
            block_clobbers: &block_clobbers,
            reachability: &reachability,
            replacements: &mut replacements,
            dead: &mut dead,
        };

        self.process_global_block(func, func.entry_block, &mut cache, &mut ctx);

        if !replacements.is_empty() {
            self.apply_replacements_to_all_blocks(func, &replacements);
        }
        if !dead.is_empty() {
            for block in func.blocks.iter_mut() {
                block.instructions.retain(|id| !dead.contains(id));
            }
        }
    }

    fn sink_redundant_phi_expressions(&mut self, func: &mut Function) {
        let cfg = CfgInfo::new(func);
        let inst_results = mir_utils::inst_results(func);
        let inst_blocks = mir_utils::inst_blocks(func);
        let use_counts = Self::value_use_counts(func);
        let replacements = FxHashMap::default();
        let ctx = PhiSinkContext {
            dominators: cfg.dominators(),
            inst_blocks: &inst_blocks,
            inst_results: &inst_results,
            replacements: &replacements,
        };
        let mut candidates = Vec::new();

        for block_id in func.blocks.indices() {
            let phi_insts: Vec<_> = func.blocks[block_id]
                .instructions
                .iter()
                .copied()
                .take_while(|&inst_id| matches!(func.instructions[inst_id].kind, InstKind::Phi(_)))
                .collect();
            for phi_inst in phi_insts {
                if let Some(candidate) =
                    self.phi_expression_candidate(func, block_id, phi_inst, &ctx)
                {
                    candidates.push(candidate);
                }
            }
        }

        if candidates.is_empty() {
            return;
        }

        let mut dead = FxHashSet::default();
        let mut replacements = FxHashMap::default();
        let mut inserted_by_block: FxHashMap<BlockId, usize> = FxHashMap::default();

        for candidate in candidates {
            let new_inst =
                func.alloc_inst(Instruction::new(candidate.kind, Some(candidate.result_ty)));
            let new_value = func.alloc_value(Value::Inst(new_inst));

            let phi_count = func.blocks[candidate.block_id]
                .instructions
                .iter()
                .take_while(|&&inst_id| matches!(func.instructions[inst_id].kind, InstKind::Phi(_)))
                .count();
            let inserted = inserted_by_block.entry(candidate.block_id).or_default();
            func.blocks[candidate.block_id].instructions.insert(phi_count + *inserted, new_inst);
            *inserted += 1;

            replacements.insert(candidate.phi_result, new_value);
            dead.insert(candidate.phi_inst);
            for (value, inst_id) in candidate.incoming {
                if use_counts.get(&value).copied().unwrap_or_default() == 1 {
                    dead.insert(inst_id);
                }
            }
            self.eliminated_count += 1;
        }

        self.apply_replacements_to_all_blocks(func, &replacements);
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|id| !dead.contains(id));
        }
    }

    fn phi_expression_candidate(
        &self,
        func: &Function,
        block_id: BlockId,
        phi_inst: InstId,
        ctx: &PhiSinkContext<'_>,
    ) -> Option<PhiExpressionCandidate> {
        let inst = &func.instructions[phi_inst];
        let result_ty = inst.result_ty?;
        let phi_result = *ctx.inst_results.get(&phi_inst)?;
        let InstKind::Phi(incoming) = &inst.kind else { return None };
        if incoming.len() < 2 {
            return None;
        }

        let mut expected_key = None;
        let mut candidate_kind = None;
        let mut incoming_insts = Vec::with_capacity(incoming.len());

        for &(_, value) in incoming {
            let Value::Inst(inst_id) = func.value(value) else { return None };
            let source_inst = &func.instructions[*inst_id];
            if source_inst.kind.has_side_effects()
                || !Self::operands_dominate_block(
                    func,
                    &source_inst.kind,
                    block_id,
                    ctx.inst_blocks,
                    ctx.dominators,
                )
            {
                return None;
            }

            let key = self.make_expr_key(func, *inst_id, &source_inst.kind, ctx.replacements)?;
            if !Self::is_sinkable_pure_expr(&key) {
                return None;
            }
            if expected_key.as_ref().is_some_and(|expected| expected != &key) {
                return None;
            }
            expected_key = Some(key);
            candidate_kind.get_or_insert_with(|| source_inst.kind.clone());
            incoming_insts.push((value, *inst_id));
        }

        Some(PhiExpressionCandidate {
            block_id,
            phi_inst,
            phi_result,
            kind: candidate_kind?,
            result_ty,
            incoming: incoming_insts,
        })
    }

    fn process_global_block(
        &mut self,
        func: &Function,
        block_id: BlockId,
        cache: &mut FxHashMap<ExprKey, ValueId>,
        ctx: &mut GlobalCseContext<'_>,
    ) {
        let inst_ids = func.blocks[block_id].instructions.clone();
        for inst_id in inst_ids {
            let kind = func.instructions[inst_id].kind.clone();
            if kind.has_side_effects() {
                self.invalidate_for_side_effect(func, inst_id, &kind, ctx.replacements, cache);
                continue;
            }

            let Some(key) = self.make_expr_key(func, inst_id, &kind, ctx.replacements) else {
                continue;
            };

            let Some(&result) = ctx.inst_results.get(&inst_id) else {
                continue;
            };
            if let Some(cached) = cache.get(&key) {
                ctx.replacements.insert(result, *cached);
                ctx.dead.insert(inst_id);
                self.eliminated_count += 1;
            } else {
                cache.insert(key, result);
            }
        }

        for &child in ctx.dom_tree.children(block_id) {
            let mut child_cache = cache.clone();
            self.filter_inherited_cache(block_id, child, &mut child_cache, ctx);
            self.process_global_block(func, child, &mut child_cache, ctx);
        }
    }

    /// Invalidates state-dependent cache entries inherited across the dominator-tree edge
    /// `parent -> child`.
    ///
    /// Dominance alone is sound only for pure expressions: memory, storage, transient-storage, and
    /// account-environment reads must also survive every CFG path from `parent` to `child`, which
    /// may pass through blocks that are not on the dominator-tree path (diamond arms, loop bodies).
    /// Applies the clobber summary of every such intermediate block, including `child` itself when
    /// it lies on a cycle (clobbers wrap around the backedge to the child's entry).
    fn filter_inherited_cache(
        &self,
        parent: BlockId,
        child: BlockId,
        cache: &mut FxHashMap<ExprKey, ValueId>,
        ctx: &GlobalCseContext<'_>,
    ) {
        if ctx.block_clobbers.is_empty() || !cache.keys().any(Self::is_path_sensitive_expr) {
            return;
        }
        let Some(reachable_from_parent) = ctx.reachability.get(&parent) else { return };
        for (&mid, clobbers) in ctx.block_clobbers {
            // Clobbers in `parent` itself were already applied while processing it sequentially.
            if mid == parent || !reachable_from_parent.contains(&mid) {
                continue;
            }
            if !ctx.reachability.get(&mid).is_some_and(|reachable| reachable.contains(&child)) {
                continue;
            }
            for clobber in clobbers {
                self.apply_clobber(cache, clobber);
            }
        }
    }

    /// Returns the per-block invalidation summaries for blocks with clobbering effects.
    fn block_clobber_summaries(&self, func: &Function) -> FxHashMap<BlockId, Vec<Clobber>> {
        let no_replacements = FxHashMap::default();
        let mut summaries = FxHashMap::default();
        for (block_id, block) in func.blocks.iter_enumerated() {
            let mut clobbers = Vec::new();
            for &inst_id in &block.instructions {
                let kind = &func.instructions[inst_id].kind;
                if kind.has_side_effects() {
                    self.side_effect_clobbers(func, inst_id, kind, &no_replacements, &mut clobbers);
                }
            }
            if !clobbers.is_empty() {
                summaries.insert(block_id, clobbers);
            }
        }
        summaries
    }

    fn is_path_sensitive_expr(key: &ExprKey) -> bool {
        Self::is_memory_expr(key)
            || Self::is_account_environment_expr(key)
            || matches!(key, ExprKey::SLoad(_) | ExprKey::TLoad(_))
    }

    /// Processes a single basic block.
    fn process_block(
        &mut self,
        func: &mut Function,
        block_id: BlockId,
        inst_results: &FxHashMap<InstId, ValueId>,
    ) {
        // Map from expression key to the ValueId that computed it
        let mut expr_cache: FxHashMap<ExprKey, ValueId> = FxHashMap::default();

        // Map from ValueId to its replacement ValueId
        let mut replacements: FxHashMap<ValueId, ValueId> = FxHashMap::default();

        // Instructions to remove
        let mut to_remove: FxHashSet<InstId> = FxHashSet::default();

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
            if let Some(key) = self.make_expr_key(func, inst_id, &kind, &replacements)
                && let Some(&result) = inst_results.get(&inst_id)
            {
                if let Some(&cached_value) = expr_cache.get(&key) {
                    // This expression was already computed - mark for elimination
                    replacements.insert(result, cached_value);
                    to_remove.insert(inst_id);
                    self.eliminated_count += 1;
                } else {
                    // First occurrence - cache it
                    expr_cache.insert(key, result);
                }
            }
        }

        // Apply replacements everywhere: the eliminated result may be used in dominated blocks.
        if !replacements.is_empty() {
            self.apply_replacements_to_all_blocks(func, &replacements);
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
        let value = |v: ValueId| mir_utils::resolve_replacement(v, replacements);

        match kind {
            // Commutative operations - normalize operand order
            InstKind::Add(a, b) => {
                if let Some((base, offset)) = Self::offset_expr_for_add(func, *a, *b, replacements)
                {
                    Some(ExprKey::Offset(base, offset))
                } else {
                    let (a, b) = Self::ordered_pair(operand(*a), operand(*b));
                    Some(ExprKey::Add(a, b))
                }
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
            InstKind::Sub(a, b) => {
                if let Some((base, offset)) = Self::offset_expr_for_sub(func, *a, *b, replacements)
                {
                    Some(ExprKey::Offset(base, offset))
                } else {
                    Some(ExprKey::Sub(operand(*a), operand(*b)))
                }
            }
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
            // Swapped comparisons - canonicalize `a > b` as `b < a` so they unify
            InstKind::Lt(a, b) => Some(ExprKey::Lt(operand(*a), operand(*b))),
            InstKind::Gt(a, b) => Some(ExprKey::Lt(operand(*b), operand(*a))),
            InstKind::SLt(a, b) => Some(ExprKey::SLt(operand(*a), operand(*b))),
            InstKind::SGt(a, b) => Some(ExprKey::SLt(operand(*b), operand(*a))),
            InstKind::SignExtend(a, b) => Some(ExprKey::SignExtend(operand(*a), operand(*b))),

            // Unary operations
            InstKind::IsZero(a) => Some(ExprKey::IsZero(operand(*a))),
            InstKind::Not(a) => Some(ExprKey::Not(operand(*a))),
            InstKind::CalldataLoad(a) => Some(ExprKey::CalldataLoad(operand(*a))),
            InstKind::ExtCodeSize(a) => Some(ExprKey::ExtCodeSize(operand(*a))),
            InstKind::ExtCodeHash(a) => Some(ExprKey::ExtCodeHash(operand(*a))),
            InstKind::Balance(a) => Some(ExprKey::Balance(operand(*a))),
            InstKind::BlockHash(a) => Some(ExprKey::BlockHash(operand(*a))),
            InstKind::BlobHash(a) => Some(ExprKey::BlobHash(operand(*a))),
            // Immutable reads are constant once the runtime code is patched.
            InstKind::LoadImmutable(offset) => Some(ExprKey::LoadImmutable(*offset)),

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
                let size = value(*size);
                let const_size = mir_utils::value_u64(func, size);
                let mut key = self.memory_range_key(func, inst_id, value(*offset), const_size)?;
                if const_size.is_none() {
                    // Key the dynamic size operand so reads of different lengths never unify.
                    key.dyn_size = Some(size);
                }
                Some(ExprKey::Keccak256(key))
            }

            InstKind::SLoad(slot) => Some(ExprKey::SLoad(
                mir_utils::storage_alias_after_replacements(func, inst_id, *slot, replacements),
            )),
            InstKind::TLoad(slot) => Some(ExprKey::TLoad(
                mir_utils::storage_alias_after_replacements(func, inst_id, *slot, replacements),
            )),

            InstKind::SelfBalance => Some(ExprKey::SelfBalance),

            // Don't cache these:
            // - Cheap nullary reads usually cost less than their extra stack lifetime
            // - Memory size/gas/returndata-size reads can change inside a block
            // - Storage writes - side effects
            // - Phi nodes - not expressions
            // - Calls - side effects
            _ => None,
        }
    }

    fn invalidate_for_side_effect(
        &self,
        func: &Function,
        inst_id: InstId,
        kind: &InstKind,
        replacements: &FxHashMap<ValueId, ValueId>,
        expr_cache: &mut FxHashMap<ExprKey, ValueId>,
    ) {
        let mut clobbers = Vec::new();
        self.side_effect_clobbers(func, inst_id, kind, replacements, &mut clobbers);
        for clobber in &clobbers {
            self.apply_clobber(expr_cache, clobber);
        }
    }

    /// Collects the cache-invalidating effects of a side-effecting instruction.
    fn side_effect_clobbers(
        &self,
        func: &Function,
        inst_id: InstId,
        kind: &InstKind,
        replacements: &FxHashMap<ValueId, ValueId>,
        clobbers: &mut Vec<Clobber>,
    ) {
        match *kind {
            InstKind::MStore(addr, _) => {
                let addr = mir_utils::resolve_replacement(addr, replacements);
                clobbers.push(Clobber::Memory(self.memory_range_key(
                    func,
                    inst_id,
                    addr,
                    Some(32),
                )));
            }
            InstKind::MStore8(addr, _) => {
                let addr = mir_utils::resolve_replacement(addr, replacements);
                clobbers.push(Clobber::Memory(self.memory_range_key(func, inst_id, addr, Some(1))));
            }
            InstKind::MCopy(dest, _, size)
            | InstKind::CalldataCopy(dest, _, size)
            | InstKind::CodeCopy(dest, _, size)
            | InstKind::ReturnDataCopy(dest, _, size)
            | InstKind::ExtCodeCopy(_, dest, _, size) => {
                let dest = mir_utils::resolve_replacement(dest, replacements);
                let size =
                    mir_utils::value_u64(func, mir_utils::resolve_replacement(size, replacements));
                clobbers.push(Clobber::Memory(self.memory_range_key(func, inst_id, dest, size)));
            }
            InstKind::SStore(slot, _) => {
                clobbers.push(Clobber::Storage(mir_utils::storage_alias_after_replacements(
                    func,
                    inst_id,
                    slot,
                    replacements,
                )));
            }
            InstKind::TStore(slot, _) => {
                clobbers.push(Clobber::Transient(mir_utils::storage_alias_after_replacements(
                    func,
                    inst_id,
                    slot,
                    replacements,
                )));
            }
            _ if kind.may_mutate_memory() => {
                clobbers.push(Clobber::Memory(None));
                if Self::may_change_account_environment(kind) {
                    clobbers.push(Clobber::AccountEnvironment);
                }
                if kind.may_mutate_storage() {
                    clobbers.push(Clobber::AllStorage);
                }
                if kind.may_mutate_transient_storage() {
                    clobbers.push(Clobber::AllTransient);
                }
            }
            _ if kind.may_mutate_storage() => clobbers.push(Clobber::AllStorage),
            _ if kind.may_mutate_transient_storage() => clobbers.push(Clobber::AllTransient),
            _ => {}
        }
    }

    /// Removes cache entries invalidated by a single clobbering effect.
    fn apply_clobber(&self, expr_cache: &mut FxHashMap<ExprKey, ValueId>, clobber: &Clobber) {
        match *clobber {
            Clobber::Memory(write) => self.invalidate_memory(expr_cache, write),
            Clobber::Storage(alias) => {
                expr_cache.retain(|key, _| match key {
                    ExprKey::SLoad(cached) => !cached.may_alias(alias),
                    _ => true,
                });
            }
            Clobber::AllStorage => {
                expr_cache.retain(|key, _| !matches!(key, ExprKey::SLoad(_)));
            }
            Clobber::Transient(alias) => {
                expr_cache.retain(|key, _| match key {
                    ExprKey::TLoad(cached) => !cached.may_alias(alias),
                    _ => true,
                });
            }
            Clobber::AllTransient => {
                expr_cache.retain(|key, _| !matches!(key, ExprKey::TLoad(_)));
            }
            Clobber::AccountEnvironment => {
                expr_cache.retain(|key, _| !Self::is_account_environment_expr(key));
            }
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

    fn is_account_environment_expr(key: &ExprKey) -> bool {
        matches!(
            key,
            ExprKey::ExtCodeSize(_)
                | ExprKey::ExtCodeHash(_)
                | ExprKey::Balance(_)
                | ExprKey::SelfBalance
        )
    }

    /// STATICCALL is excluded: the whole static context forbids value transfers, `SSTORE`,
    /// `CREATE`, and `SELFDESTRUCT`, so balances and deployed code cannot change. Its memory
    /// clobber (the return buffer write) is handled separately via `may_mutate_memory`.
    fn may_change_account_environment(kind: &InstKind) -> bool {
        matches!(
            kind,
            InstKind::Call { .. }
                | InstKind::DelegateCall { .. }
                | InstKind::InternalCall { .. }
                | InstKind::Create(_, _, _)
                | InstKind::Create2(_, _, _, _)
        )
    }

    fn is_sinkable_pure_expr(key: &ExprKey) -> bool {
        !matches!(
            key,
            ExprKey::MLoad(_)
                | ExprKey::Keccak256(_)
                | ExprKey::SLoad(_)
                | ExprKey::TLoad(_)
                | ExprKey::CalldataLoad(_)
                | ExprKey::ExtCodeSize(_)
                | ExprKey::ExtCodeHash(_)
                | ExprKey::BlockHash(_)
                | ExprKey::Balance(_)
                | ExprKey::SelfBalance
                | ExprKey::BlobHash(_)
        )
    }

    fn operands_dominate_block(
        func: &Function,
        kind: &InstKind,
        block_id: BlockId,
        inst_blocks: &FxHashMap<InstId, BlockId>,
        dominators: &DominatorTree,
    ) -> bool {
        kind.operands().into_iter().all(|value| {
            Self::value_dominates_block(func, value, block_id, inst_blocks, dominators)
        })
    }

    fn value_dominates_block(
        func: &Function,
        value: ValueId,
        block_id: BlockId,
        inst_blocks: &FxHashMap<InstId, BlockId>,
        dominators: &DominatorTree,
    ) -> bool {
        match func.value(value) {
            Value::Immediate(_) | Value::Arg { .. } | Value::Undef(_) => true,
            Value::Inst(inst_id) => inst_blocks
                .get(inst_id)
                .is_some_and(|&def_block| dominators.dominates(def_block, block_id)),
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
            .memory_region()
            .unwrap_or_else(|| mir_utils::memory_region_for_addr(func, addr));
        let (base, offset) = Self::memory_addr_base_offset(func, addr);
        Some(MemRangeKey { region, base, offset, size, dyn_size: None })
    }

    fn memory_addr_base_offset(func: &Function, addr: ValueId) -> (Option<ValueId>, Option<u64>) {
        if let Some((base, offset)) = Self::offset_value(func, addr, &FxHashMap::default(), 0) {
            if let (OperandKey::Value(base), Some(offset)) = (base, mir_utils::u256_to_u64(offset))
            {
                return (Some(base), Some(offset));
            }
            return (Some(addr), Some(0));
        }
        match func.value(addr) {
            Value::Immediate(imm) => (None, imm.as_u256().and_then(mir_utils::u256_to_u64)),
            Value::Arg { .. } | Value::Inst(_) | Value::Undef(_) => (Some(addr), Some(0)),
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
        mir_utils::ranges_overlap(read_offset, read_size, write_offset, write_size)
    }

    fn offset_expr_for_add(
        func: &Function,
        a: ValueId,
        b: ValueId,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> Option<(OperandKey, U256)> {
        if let Some(offset) = mir_utils::value_u256_after_replacements(func, b, replacements) {
            let (base, existing) = Self::offset_value(func, a, replacements, 0)?;
            Some((base, existing.wrapping_add(offset)))
        } else if let Some(offset) = mir_utils::value_u256_after_replacements(func, a, replacements)
        {
            let (base, existing) = Self::offset_value(func, b, replacements, 0)?;
            Some((base, existing.wrapping_add(offset)))
        } else {
            None
        }
    }

    fn offset_expr_for_sub(
        func: &Function,
        a: ValueId,
        b: ValueId,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> Option<(OperandKey, U256)> {
        let offset = mir_utils::value_u256_after_replacements(func, b, replacements)?;
        let (base, existing) = Self::offset_value(func, a, replacements, 0)?;
        Some((base, existing.wrapping_sub(offset)))
    }

    fn offset_value(
        func: &Function,
        value: ValueId,
        replacements: &FxHashMap<ValueId, ValueId>,
        depth: usize,
    ) -> Option<(OperandKey, U256)> {
        if depth >= 4 {
            return None;
        }

        let value = mir_utils::resolve_replacement(value, replacements);
        match func.value(value) {
            Value::Immediate(_) => None,
            Value::Arg { .. } | Value::Undef(_) => Some((OperandKey::Value(value), U256::ZERO)),
            Value::Inst(inst_id) => match func.instructions[*inst_id].kind {
                InstKind::Add(a, b) => {
                    if let Some(offset) =
                        mir_utils::value_u256_after_replacements(func, b, replacements)
                    {
                        let (base, existing) =
                            Self::offset_value(func, a, replacements, depth + 1)?;
                        Some((base, existing.wrapping_add(offset)))
                    } else if let Some(offset) =
                        mir_utils::value_u256_after_replacements(func, a, replacements)
                    {
                        let (base, existing) =
                            Self::offset_value(func, b, replacements, depth + 1)?;
                        Some((base, existing.wrapping_add(offset)))
                    } else {
                        Some((OperandKey::Value(value), U256::ZERO))
                    }
                }
                InstKind::Sub(a, b) => {
                    let offset = mir_utils::value_u256_after_replacements(func, b, replacements)?;
                    let (base, existing) = Self::offset_value(func, a, replacements, depth + 1)?;
                    Some((base, existing.wrapping_sub(offset)))
                }
                _ => Some((OperandKey::Value(value), U256::ZERO)),
            },
        }
    }

    fn operand_key(
        func: &Function,
        value: ValueId,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> OperandKey {
        let value = mir_utils::resolve_replacement(value, replacements);
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

    fn value_use_counts(func: &Function) -> FxHashMap<ValueId, usize> {
        let mut counts = FxHashMap::default();
        for inst in func.instructions.iter() {
            for value in inst.operands() {
                *counts.entry(value).or_insert(0) += 1;
            }
        }
        for block in func.blocks.iter() {
            if let Some(term) = &block.terminator {
                Self::count_terminator_uses(term, &mut counts);
            }
        }
        counts
    }

    fn count_terminator_uses(
        term: &crate::mir::Terminator,
        counts: &mut FxHashMap<ValueId, usize>,
    ) {
        use crate::mir::Terminator;

        let mut count = |value| {
            *counts.entry(value).or_insert(0) += 1;
        };

        match term {
            Terminator::Jump(_) | Terminator::Stop | Terminator::Invalid => {}
            Terminator::Branch { condition, .. } => count(*condition),
            Terminator::Switch { value, cases, .. } => {
                count(*value);
                for (case, _) in cases {
                    count(*case);
                }
            }
            Terminator::Return { values } => {
                for &value in values {
                    count(value);
                }
            }
            Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
                count(*offset);
                count(*size);
            }
            Terminator::SelfDestruct { recipient } => count(*recipient),
        }
    }

    fn apply_replacements_to_all_blocks(
        &self,
        func: &mut Function,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) {
        let block_ids: Vec<_> = func.blocks.indices().collect();
        for block_id in block_ids {
            self.apply_replacements(func, block_id, replacements);
        }
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
            if mir_utils::replace_inst_uses_canonicalized(&mut inst.kind, replacements) != 0 {
                if mir_utils::is_memory_inst(&inst.kind) {
                    inst.metadata.set_memory_region(None);
                }
                if matches!(
                    inst.kind,
                    InstKind::SLoad(_)
                        | InstKind::SStore(_, _)
                        | InstKind::TLoad(_)
                        | InstKind::TStore(_, _)
                ) {
                    inst.metadata.set_storage_alias(None);
                }
            }
        }

        // Also update terminator if present
        let block = func.block_mut(block_id);
        if let Some(term) = &mut block.terminator {
            mir_utils::replace_terminator_uses_canonicalized(term, replacements);
        }
    }
}
