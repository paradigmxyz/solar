//! Congruence-class global value numbering.
//!
//! Unlike CSE, which keys expressions by operand `ValueId` and therefore only
//! unifies expressions whose operands were already collapsed, this pass builds
//! congruence classes first: two values are congruent when they compute the
//! same pure expression over pairwise-congruent operands. That catches
//! transitive congruence (`a = x + y; b = x + y; c = a * 2; d = b * 2` makes
//! `d` congruent to `c`) and phi congruence (two phis in one block with
//! pairwise-congruent incoming values).
//!
//! ## Algorithm
//!
//! Value numbering is pessimistic and iterated to a fixed point (Simpson's RPO
//! algorithm). A value number is represented by its class representative
//! `ValueId`:
//! - Initially every value is its own class, except equal immediates (same payload, hence same
//!   value and `MirType`) and function arguments with the same index, which share a class. Each
//!   `Undef` stays unique.
//! - Each sweep walks all instructions in reverse postorder and recomputes each result's class from
//!   a per-sweep hash-consing table keyed by the instruction's expression over its operands'
//!   current classes, plus the result `MirType` so differently-typed results never merge.
//!   Commutative operands are sorted by class; `gt`/`sgt` key as the swapped `lt`/`slt`.
//! - A phi keys as its block plus the per-predecessor incoming classes; a phi whose incoming values
//!   all share one class joins that class directly.
//! - Sweeps repeat until no class changes, with a hard cap. If the cap is hit before convergence
//!   the numbering is discarded: only a converged fixed point has a self-consistent congruence
//!   proof, and bailing only loses optimization.
//!
//! Only pure word expressions (and `calldataload`/`blockhash`/`blobhash`,
//! which are pure within one execution) participate. Memory, storage, and
//! account-environment reads, `gas`/`msize`/`returndatasize`, and calls never
//! merge here; CSE keeps covering those with its clobber tracking.
//!
//! ## Replacement
//!
//! A dominator-tree preorder walk (over reachable blocks only) carries a
//! scoped leader map from class to the first value of that class seen on the
//! current tree path. An instruction whose class already has an in-scope
//! leader is removed and its result redirected to the leader; otherwise it
//! becomes the leader for its subtree. Sibling subtrees never see each other's
//! leaders, so congruent values without a dominance relation are left alone.
//! Classes represented by an immediate, argument, or undef are pre-seeded with
//! that value, which folds phi-of-same over constants. Orphaned arena entries
//! are left behind for DCE, matching the other passes.

use crate::{
    analysis::CfgInfo,
    mir::{
        BlockId, Function, Immediate, InstId, InstKind, MirType, Value, ValueId, utils as mir_utils,
    },
    pass::FunctionPass,
};
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};

/// Hard cap on value-numbering sweeps per round.
const MAX_VN_SWEEPS: usize = 10;
/// Hard cap on whole-pass rounds (number, then replace) per function.
const MAX_ROUNDS: usize = 4;

/// A value number, named by its congruence-class representative.
type ClassId = ValueId;

/// Congruence-class global value numbering pass.
#[derive(Debug, Default)]
pub(crate) struct GlobalValueNumberer {
    /// Number of instructions folded onto a congruent leader.
    pub eliminated_count: usize,
}

/// Function pass for congruence-class global value numbering.
pub(crate) struct GvnPass;

impl FunctionPass for GvnPass {
    fn run_on_function(&mut self, func: &mut Function) -> bool {
        GlobalValueNumberer::new().run(func) != 0
    }
}

/// A hash-consing key for one instruction: its expression over operand
/// classes plus the result type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ExprKey {
    kind: ExprKind,
    /// Result type; differently-typed results never merge.
    ty: MirType,
}

/// An expression shape over operand congruence classes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ExprKind {
    Add(ClassId, ClassId),
    Sub(ClassId, ClassId),
    Mul(ClassId, ClassId),
    Div(ClassId, ClassId),
    SDiv(ClassId, ClassId),
    Mod(ClassId, ClassId),
    SMod(ClassId, ClassId),
    Exp(ClassId, ClassId),
    AddMod(ClassId, ClassId, ClassId),
    MulMod(ClassId, ClassId, ClassId),
    And(ClassId, ClassId),
    Or(ClassId, ClassId),
    Xor(ClassId, ClassId),
    Not(ClassId),
    Shl(ClassId, ClassId),
    Shr(ClassId, ClassId),
    Sar(ClassId, ClassId),
    Byte(ClassId, ClassId),
    /// Also keys `Gt(a, b)`, normalized as `Lt(b, a)`.
    Lt(ClassId, ClassId),
    /// Also keys `SGt(a, b)`, normalized as `SLt(b, a)`.
    SLt(ClassId, ClassId),
    Eq(ClassId, ClassId),
    IsZero(ClassId),
    Select(ClassId, ClassId, ClassId),
    SignExtend(ClassId, ClassId),
    CalldataLoad(ClassId),
    BlockHash(ClassId),
    BlobHash(ClassId),
    LoadImmutable(u32),
    Phi(BlockId, Vec<(BlockId, ClassId)>),
}

struct ReplaceCtx<'a> {
    vn: &'a IndexVec<ValueId, ClassId>,
    cfg: &'a CfgInfo,
    inst_results: &'a FxHashMap<InstId, ValueId>,
    replacements: &'a mut FxHashMap<ValueId, ValueId>,
    dead: &'a mut DenseBitSet<InstId>,
}

impl GlobalValueNumberer {
    /// Creates a new GVN pass.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Runs GVN on a function to a fixed point of number-then-replace rounds.
    /// Returns the number of instructions eliminated.
    pub(crate) fn run(&mut self, func: &mut Function) -> usize {
        self.eliminated_count = 0;
        for _ in 0..MAX_ROUNDS {
            if !self.run_round(func) {
                break;
            }
        }
        self.eliminated_count
    }

    /// Runs one numbering and replacement round. Returns true if MIR changed.
    fn run_round(&mut self, func: &mut Function) -> bool {
        let cfg = CfgInfo::new(func);
        let inst_results = func.inst_results();
        let Some(vn) = Self::compute_value_numbers(func, cfg.rpo(), &inst_results) else {
            return false;
        };

        // Immediates, arguments, and undefs are available everywhere, so their
        // classes start with a leader. This folds phi-of-same over constants.
        let mut leaders: FxHashMap<ClassId, ValueId> = FxHashMap::default();
        for (value_id, value) in func.values.iter_enumerated() {
            if !matches!(value, Value::Inst(_)) && vn[value_id] == value_id {
                leaders.insert(value_id, value_id);
            }
        }

        let mut replacements = FxHashMap::default();
        let mut dead = DenseBitSet::new_empty(func.instructions.len());
        let mut ctx = ReplaceCtx {
            vn: &vn,
            cfg: &cfg,
            inst_results: &inst_results,
            replacements: &mut replacements,
            dead: &mut dead,
        };
        self.replace_in_block(func, BlockId::ENTRY, &mut leaders, &mut ctx);

        if replacements.is_empty() {
            return false;
        }
        Self::apply_replacements_to_all_blocks(func, &replacements);
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|&id| !dead.contains(id));
        }
        true
    }

    // ----- value numbering -----

    /// Computes a converged congruence-class assignment, or `None` if the
    /// sweep cap was hit first.
    fn compute_value_numbers(
        func: &Function,
        rpo: &[BlockId],
        inst_results: &FxHashMap<InstId, ValueId>,
    ) -> Option<IndexVec<ValueId, ClassId>> {
        let mut vn = func.values.indices().collect::<IndexVec<ValueId, _>>();
        let mut immediate_reps: FxHashMap<Immediate, ValueId> = FxHashMap::default();
        let mut arg_reps: FxHashMap<u32, ValueId> = FxHashMap::default();
        for (value_id, value) in func.values.iter_enumerated() {
            match value {
                Value::Immediate(imm) => {
                    vn[value_id] = *immediate_reps.entry(imm.clone()).or_insert(value_id);
                }
                Value::Arg { index, .. } => {
                    vn[value_id] = *arg_reps.entry(*index).or_insert(value_id);
                }
                Value::Inst(_) | Value::Undef(_) | Value::Error(_) => {}
            }
        }

        for _ in 0..MAX_VN_SWEEPS {
            let mut table: FxHashMap<ExprKey, ClassId> = FxHashMap::default();
            let mut changed = false;
            for &block_id in rpo {
                for &inst_id in &func.blocks[block_id].instructions {
                    let Some(&result) = inst_results.get(&inst_id) else { continue };
                    let inst = &func.instructions[inst_id];
                    let Some(ty) = inst.result_ty else { continue };
                    let Some(class) =
                        Self::instruction_class(block_id, &inst.kind, ty, result, &vn, &mut table)
                    else {
                        continue;
                    };
                    if vn[result] != class {
                        vn[result] = class;
                        changed = true;
                    }
                }
            }
            if !changed {
                return Some(vn);
            }
        }
        None
    }

    /// Returns the class for one instruction's result given the current
    /// numbering, or `None` for instructions that never participate.
    ///
    /// Participating instructions always get a class (falling back to their
    /// own result), so a stale merge from an earlier sweep cannot survive a
    /// sweep that no longer justifies it.
    fn instruction_class(
        block_id: BlockId,
        kind: &InstKind,
        ty: MirType,
        result: ValueId,
        vn: &IndexVec<ValueId, ClassId>,
        table: &mut FxHashMap<ExprKey, ClassId>,
    ) -> Option<ClassId> {
        if let InstKind::Phi(incoming) = kind {
            let Some((&(_, first), rest)) = incoming.split_first() else { return Some(result) };
            // Phi-of-same: a phi over one class is that class.
            if rest.iter().all(|&(_, value)| vn[value] == vn[first]) {
                return Some(vn[first]);
            }
            let Some(incoming) = Self::phi_key_incoming(incoming, vn) else { return Some(result) };
            let key = ExprKey { kind: ExprKind::Phi(block_id, incoming), ty };
            return Some(*table.entry(key).or_insert(result));
        }
        let kind = Self::expr_kind(kind, vn)?;
        Some(*table.entry(ExprKey { kind, ty }).or_insert(result))
    }

    /// Normalizes a phi's incoming list for keying: per-predecessor classes,
    /// sorted by predecessor with exact duplicates removed.
    fn phi_key_incoming(
        incoming: &[(BlockId, ValueId)],
        vn: &IndexVec<ValueId, ClassId>,
    ) -> Option<Vec<(BlockId, ClassId)>> {
        let mut entries: Vec<(BlockId, ClassId)> =
            incoming.iter().map(|&(pred, value)| (pred, vn[value])).collect();
        entries.sort_by_key(|&(pred, class)| (pred.index(), class.index()));
        entries.dedup();
        // A predecessor listed with two distinct classes has no well-defined
        // per-edge value; leave such phis unmerged.
        if entries.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return None;
        }
        Some(entries)
    }

    /// Builds the expression shape over operand classes for pure word ops.
    /// Returns `None` for every other instruction.
    fn expr_kind(kind: &InstKind, vn: &IndexVec<ValueId, ClassId>) -> Option<ExprKind> {
        let class = |value: ValueId| vn[value];
        let sorted = |a: ValueId, b: ValueId| {
            let (a, b) = (class(a), class(b));
            if b.index() < a.index() { (b, a) } else { (a, b) }
        };
        Some(match *kind {
            InstKind::Add(a, b) => {
                let (a, b) = sorted(a, b);
                ExprKind::Add(a, b)
            }
            InstKind::Mul(a, b) => {
                let (a, b) = sorted(a, b);
                ExprKind::Mul(a, b)
            }
            InstKind::And(a, b) => {
                let (a, b) = sorted(a, b);
                ExprKind::And(a, b)
            }
            InstKind::Or(a, b) => {
                let (a, b) = sorted(a, b);
                ExprKind::Or(a, b)
            }
            InstKind::Xor(a, b) => {
                let (a, b) = sorted(a, b);
                ExprKind::Xor(a, b)
            }
            InstKind::Eq(a, b) => {
                let (a, b) = sorted(a, b);
                ExprKind::Eq(a, b)
            }
            InstKind::AddMod(a, b, n) => {
                let (a, b) = sorted(a, b);
                ExprKind::AddMod(a, b, class(n))
            }
            InstKind::MulMod(a, b, n) => {
                let (a, b) = sorted(a, b);
                ExprKind::MulMod(a, b, class(n))
            }
            InstKind::Sub(a, b) => ExprKind::Sub(class(a), class(b)),
            InstKind::Div(a, b) => ExprKind::Div(class(a), class(b)),
            InstKind::SDiv(a, b) => ExprKind::SDiv(class(a), class(b)),
            InstKind::Mod(a, b) => ExprKind::Mod(class(a), class(b)),
            InstKind::SMod(a, b) => ExprKind::SMod(class(a), class(b)),
            InstKind::Exp(a, b) => ExprKind::Exp(class(a), class(b)),
            InstKind::Shl(a, b) => ExprKind::Shl(class(a), class(b)),
            InstKind::Shr(a, b) => ExprKind::Shr(class(a), class(b)),
            InstKind::Sar(a, b) => ExprKind::Sar(class(a), class(b)),
            InstKind::Byte(a, b) => ExprKind::Byte(class(a), class(b)),
            InstKind::SignExtend(a, b) => ExprKind::SignExtend(class(a), class(b)),
            // Swapped comparisons - canonicalize `a > b` as `b < a` so they
            // unify; the surviving instruction keeps its own opcode.
            InstKind::Lt(a, b) => ExprKind::Lt(class(a), class(b)),
            InstKind::Gt(a, b) => ExprKind::Lt(class(b), class(a)),
            InstKind::SLt(a, b) => ExprKind::SLt(class(a), class(b)),
            InstKind::SGt(a, b) => ExprKind::SLt(class(b), class(a)),
            InstKind::IsZero(a) => ExprKind::IsZero(class(a)),
            InstKind::Not(a) => ExprKind::Not(class(a)),
            InstKind::Select(condition, then_value, else_value) => {
                ExprKind::Select(class(condition), class(then_value), class(else_value))
            }
            InstKind::CalldataLoad(a) => ExprKind::CalldataLoad(class(a)),
            InstKind::BlockHash(a) => ExprKind::BlockHash(class(a)),
            InstKind::BlobHash(a) => ExprKind::BlobHash(class(a)),
            // Immutable reads are constant once the runtime code is patched.
            InstKind::LoadImmutable(offset) => ExprKind::LoadImmutable(offset),
            // Everything else (memory, storage, environment reads, calls,
            // gas/msize/returndatasize, keccak) never merges in this pass.
            _ => return None,
        })
    }

    // ----- replacement -----

    /// Dominator-tree preorder walk with a scoped class-to-leader map.
    fn replace_in_block(
        &mut self,
        func: &Function,
        block_id: BlockId,
        leaders: &mut FxHashMap<ClassId, ValueId>,
        ctx: &mut ReplaceCtx<'_>,
    ) {
        for &inst_id in &func.blocks[block_id].instructions {
            let Some(&result) = ctx.inst_results.get(&inst_id) else { continue };
            let kind = &func.instructions[inst_id].kind;
            if !matches!(kind, InstKind::Phi(_)) && Self::expr_kind(kind, ctx.vn).is_none() {
                continue;
            }
            let class = ctx.vn[result];
            if let Some(&leader) = leaders.get(&class) {
                if leader != result {
                    ctx.replacements.insert(result, leader);
                    ctx.dead.insert(inst_id);
                    self.eliminated_count += 1;
                }
            } else {
                leaders.insert(class, result);
            }
        }

        for &child in ctx.cfg.dominators().children(block_id) {
            let mut child_leaders = leaders.clone();
            self.replace_in_block(func, child, &mut child_leaders, ctx);
        }
    }

    // ----- CFG helpers -----

    // ----- replacement application -----

    fn apply_replacements_to_all_blocks(
        func: &mut Function,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) {
        let block_ids: Vec<_> = func.blocks.indices().collect();
        for block_id in block_ids {
            Self::apply_replacements(func, block_id, replacements);
        }
    }

    /// Applies value replacements to all instructions in a block.
    fn apply_replacements(
        func: &mut Function,
        block_id: BlockId,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) {
        let instruction_count = func.blocks[block_id].instructions.len();
        for index in 0..instruction_count {
            let inst_id = func.blocks[block_id].instructions[index];
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

        if let Some(term) = &mut func.blocks[block_id].terminator {
            mir_utils::replace_terminator_uses_canonicalized(term, replacements);
        }
    }
}
