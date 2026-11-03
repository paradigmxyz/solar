//! Bounded word-expression recipes after e-graph extraction.
//!
//! ISLE can propose a small tree of operations, including intermediate results
//! that do not exist in MIR yet. Matching uses earlier definitions in the same
//! pure block segment. Recipes have a private value namespace: trying a rule
//! never allocates instructions or values in the function. Only the cheapest
//! legal recipe is materialized, immediately before the original root, whose
//! value identity and semantic metadata are retained.
//!
//! Profitability charges target opcode prices, immediate materialization and
//! operand copies. The old cost includes only removable, single-use producers;
//! shared producers and ABI validation instructions earn no deletion credit.
//! Removed producers must belong to the current segment, so no work moves across
//! effects, block edges or gas observations. Recipe and producer-cone sizes are
//! bounded independently of function size. This is a local tree estimate, not
//! a proof of scheduled cost; gas and size corpus measurements remain required.
//! An enabling recipe is accepted without being cheaper: its rule guard shows that it pays for
//! the rewrite it exposes to a later pass.
//! Scalar selects can also be roots: their arithmetic recipes preserve zero/nonzero
//! conditions, while pointer and aggregate selects retain their provenance.
//! Shared integer casts can move outside a select when their source widths match;
//! temporary selects retain that source type. Pointer and wider-than-word roots
//! remain excluded from arithmetic selection.
//! Inequality recipes use the backend's EQ/ISZERO expansion and its target cost, except
//! that a nonzero test whose every use is a branch or a check, or an `or` that only such a test
//! joins, costs nothing: `JUMPI` reads the word.
//! Pure, nontrapping operations without recipes stay in place without breaking
//! the segment; they never earn deletion credit.
//! Mask blends wait until storage forwarding and mask simplification have run,
//! so early recipes do not hide overwritten packed fields from those passes.
//! A low-bit mask that follows a right shift or precedes a left shift becomes one shift pair
//! in gas builds, when no other instruction reads the mask constant; masks alone are left to
//! the EVM IR `late-word` peephole, which sees the mask's materialization.
//!
//! A test that a word fits in `k` bits is the comparison `x < 2 ** k`, or `x > 2 ** k - 1` for its
//! negation, in every run but the last, and the early and late runs turn the shift and mask shapes
//! of such a test into that comparison as enabling recipes. The last run, the
//! `spell-width-tests` pass in the lowered pipeline, sees the final uses and chooses the spelling
//! with [`Target::shifts_bits_test`]: a branch or the `or` that joins tests for a branch reads a
//! shifted word for free, so those tests become `(x >> k) != 0` and the join one word; an `and` of
//! a fits test and a zero test becomes one word zero test; a test of the width of a shift the block
//! already computes reads that shift. Only comparisons, `or` and `and` are roots there.

use super::lower_checks::is_check;
use crate::{
    backend::evm::{op, select},
    mir::{
        BlockId, EffectKind, Function, Immediate, InstId, InstKind, Instruction, MirType, Module,
        Op, Terminator, Value, ValueId,
        analysis::CfgInfo,
        pass::{MirPass, run_function_pass},
    },
    target::{Cost, Target},
};
use alloy_primitives::U256;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};

mod isle;
mod mask_uses;

const MAX_CONE: usize = 16;

/// How far the pipeline has come when a run happens.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// Before the late rules apply.
    Early,
    /// The late rules apply, and abstract allocations remain.
    Late,
    /// The late rules apply, and `lower-alloc` has run.
    LateLowered,
    /// The module is lowered, and the width tests take their final spelling.
    Spelling,
}

pub(crate) enum WordSequence {
    Early,
    Late,
    Spelling,
}

impl MirPass for WordSequence {
    fn name(&self) -> &'static str {
        match self {
            Self::Early => "word-sequence-early",
            Self::Late => "word-sequence",
            Self::Spelling => "spell-width-tests",
        }
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, analyses| run(func, &analyses.target(gcx), self))
    }
}

#[derive(Clone)]
enum Temporary {
    Operation(Op),
    Constant(U256),
}

struct Recipe {
    root: Op,
    scalar_ty: MirType,
    temporaries: FxHashMap<ValueId, Temporary>,
    /// Whether the recipe is worth its local cost for what it lets later passes do.
    enabling: bool,
}

fn legal(op: &Op, target: &Target) -> bool {
    if matches!(op, Op::Select { .. } | Op::Zext { .. } | Op::Sext { .. } | Op::Trunc { .. }) {
        return true;
    }
    if matches!(op, Op::Ne { .. }) {
        return [op::EQ, op::ISZERO].into_iter().all(|opcode| {
            op::definition(opcode).is_some_and(|def| def.is_available(target.evm_version()))
        });
    }
    select::opcode_lowering(op).is_some_and(|lowering| {
        matches!(lowering, select::OpcodeLowering::Valued { .. })
            && op::definition(lowering.opcode())
                .is_some_and(|def| def.is_available(target.evm_version()))
    }) && op.into_kind().is_some_and(|kind| kind.effect_kind() == EffectKind::Pure)
}

/// Whether an instruction and its effect override are pure.
pub(super) fn is_pure(inst: &Instruction) -> bool {
    inst.kind.effect_kind() == EffectKind::Pure
        && inst.metadata.effect().is_none_or(|effect| effect == EffectKind::Pure)
}

fn removable(func: &Function, inst: &Instruction, target: &Target) -> bool {
    // Selects and legal operations are pure, so other instructions need no view.
    if !is_pure(inst) {
        return false;
    }
    let op = inst.kind.op();
    match op {
        Op::Select { true_val, false_val, .. } => {
            [inst.result_ty, func.value_ty(true_val), func.value_ty(false_val)]
                .into_iter()
                .all(|ty| matches!(ty, Some(MirType::Int(_))))
        }
        _ => legal(&op, target),
    }
}

/// The constant a value holds in a recipe: a temporary constant or a function immediate.
fn recipe_immediate(
    func: &Function,
    constants: &FxHashMap<ValueId, Temporary>,
    value: ValueId,
) -> Option<U256> {
    match constants.get(&value) {
        Some(Temporary::Constant(value)) => Some(*value),
        Some(Temporary::Operation(_)) => None,
        None => func.value_u256(value),
    }
}

fn operation_cost(
    func: &Function,
    op: &Op,
    target: &Target,
    constants: &FxHashMap<ValueId, Temporary>,
) -> Cost {
    let immediate = |value| recipe_immediate(func, constants, value);
    let mut cost = target.op(op, immediate);
    let operand_cost =
        |value| immediate(value).map_or_else(|| target.dup(), |value| target.push(value));
    if let Some(value) = Target::zero_test_input(op, immediate) {
        cost += operand_cost(value);
    } else {
        let _ = op.map_values(|value| {
            cost += operand_cost(value);
            value
        });
    }
    cost
}

/// The cost of a recipe root. A nonzero test whose every use is a branch or check condition, or
/// an `or` that only such a test joins, costs nothing, because `JUMPI` takes its operand as the
/// condition.
fn root_cost(
    func: &Function,
    op: &Op,
    target: &Target,
    constants: &FxHashMap<ValueId, Temporary>,
    branch_only: bool,
) -> Cost {
    let immediate = |value| recipe_immediate(func, constants, value);
    if branch_only
        && matches!(op, Op::Ne { .. })
        && Target::zero_test_input(op, immediate).is_some()
    {
        return Cost::ZERO;
    }
    operation_cost(func, op, target, constants)
}

impl Recipe {
    /// Visits only reachable temporary definitions, in dependency order.
    fn collect(
        &self,
        node: &Op,
        visited: &mut FxHashSet<ValueId>,
        nodes: &mut Vec<Op>,
        leaves: &mut FxHashSet<ValueId>,
    ) {
        let _ = node.map_values(|value| {
            match self.temporaries.get(&value) {
                Some(Temporary::Operation(op)) if visited.insert(value) => {
                    self.collect(op, visited, nodes, leaves)
                }
                Some(_) => {}
                None => {
                    leaves.insert(value);
                }
            }
            value
        });
        nodes.push(*node);
    }

    fn cost_and_leaves(
        &self,
        func: &Function,
        target: &Target,
        branch_only: bool,
    ) -> Option<(Cost, FxHashSet<ValueId>)> {
        let mut nodes = Vec::new();
        let mut leaves = FxHashSet::default();
        self.collect(&self.root, &mut FxHashSet::default(), &mut nodes, &mut leaves);
        if nodes.len() > MAX_CONE || nodes.iter().any(|node| !legal(node, target)) {
            return None;
        }
        let root = nodes.pop().expect("a recipe has a root");
        let root_cost = root_cost(func, &root, target, &self.temporaries, branch_only);
        Some((
            nodes
                .iter()
                .map(|node| operation_cost(func, node, target, &self.temporaries))
                .sum::<Cost>()
                + root_cost,
            leaves,
        ))
    }

    /// Commits a winning recipe, retaining the original root's result and obligations.
    fn materialize(&self, func: &mut Function, root: InstId) -> Vec<InstId> {
        let mut inserted = Vec::new();
        let mut values = FxHashMap::default();
        let op = self.root.map_values(|value| {
            self.materialize_value(func, root, value, &mut values, &mut inserted)
        });
        // %temporary = recipe_child(...)
        // %original_result = recipe_root(%temporary, ...)
        let result = emit_recipe(
            func,
            root,
            op.into_kind().expect("legal recipe root"),
            func.inst(root).result_ty.unwrap(),
            self.scalar_ty,
            &mut inserted,
        );
        let Value::Inst(last) = *func.value(result) else { unreachable!() };
        let kind = func.canonical_kind(func.inst(last).kind.clone());
        inserted.retain(|&id| id != last);
        func.inst_mut(root).replace_kind(kind);
        inserted
    }

    fn materialize_value(
        &self,
        func: &mut Function,
        root: InstId,
        value: ValueId,
        values: &mut FxHashMap<ValueId, ValueId>,
        inserted: &mut Vec<InstId>,
    ) -> ValueId {
        if let Some(&existing) = values.get(&value) {
            return existing;
        }
        let Some(temporary) = self.temporaries.get(&value) else { return value };
        let actual = match temporary {
            Temporary::Constant(value) => func
                .alloc_value(Value::Immediate(Immediate::for_type(Some(self.scalar_ty), *value))),
            Temporary::Operation(op) => {
                let op = op.map_values(|value| {
                    self.materialize_value(func, root, value, values, inserted)
                });
                // %temporary = recipe_child(earlier_values)
                let kind = op.into_kind().expect("legal recipe child");
                let ty = recipe_type(func, &kind, self.scalar_ty);
                emit_recipe(func, root, kind, ty, self.scalar_ty, inserted)
            }
        };
        values.insert(value, actual);
        actual
    }
}

/// Emits a recipe with explicit scalar conversions, returning its detached instruction list.
fn emit_recipe(
    func: &mut Function,
    root: InstId,
    kind: InstKind,
    ty: MirType,
    scalar_ty: MirType,
    inserted: &mut Vec<InstId>,
) -> ValueId {
    let metadata = func.inst(root).metadata.debug_context();
    let block = crate::mir::BlockId::ENTRY;
    let start = func.blocks[block].instructions.len();
    // operands = zext i1 boolean_operands to i256
    // result = op operands
    // typed_result = cast result
    let operation_ty = recipe_type(func, &kind, scalar_ty);
    let mut builder = crate::mir::FunctionBuilder::new(func);
    let result = builder.emit_inst_unfolded(kind, Some(operation_ty));
    let result = builder.cast(result, ty);
    let generated = func.blocks[block].instructions.split_off(start);
    for &inst in &generated {
        func.inst_mut(inst).metadata = metadata.clone();
    }
    inserted.extend(generated);
    result
}

fn recipe_type(func: &Function, kind: &InstKind, scalar_ty: MirType) -> MirType {
    if let InstKind::Select(_, a, _) = *kind {
        func.value_ty(a).expect("typed select arm")
    } else if kind.op_def().result == crate::mir::ResultKind::Integer {
        widest_int_operand(func, kind).unwrap_or(scalar_ty)
    } else {
        kind.op_def().result.default_type().unwrap_or(scalar_ty)
    }
}

/// The widest integer type among the operands of an instruction.
fn widest_int_operand(func: &Function, kind: &InstKind) -> Option<MirType> {
    kind.operands()
        .iter()
        .filter_map(|&value| func.value_ty(value))
        .filter(|ty| matches!(ty, MirType::Int(_)))
        .max_by_key(|ty| ty.integer_bits())
}

/// Whether `lower-alloc` has run: no abstract free-memory-pointer operation remains, apart from
/// the deferred allocations it leaves for the backend.
fn allocations_lowered(func: &Function) -> bool {
    func.instructions().all(|inst| match func.inst(inst).kind {
        InstKind::Fmp | InstKind::SetFmp(_) => false,
        InstKind::Alloc { .. } => func.inst(inst).metadata.deferred_alloc(),
        _ => true,
    })
}

fn run(func: &mut Function, target: &Target, pass: &WordSequence) -> bool {
    // The pass adds no allocation operation, so the answer holds for the whole run.
    let stage = match pass {
        WordSequence::Early => Stage::Early,
        WordSequence::Late if allocations_lowered(func) => Stage::LateLowered,
        WordSequence::Late => Stage::Late,
        WordSequence::Spelling => Stage::Spelling,
    };
    let spelling = stage == Stage::Spelling;
    // The blocks in a loop, which only the spelling of width tests needs.
    let cyclic = spelling.then(|| CfgInfo::new(func).cyclic_blocks().clone());
    let mut changed = cyclic.as_ref().is_some_and(|cyclic| spell_zero_tested(func, target, cyclic));
    let mut uses = super::egraph::use_counts(func);
    let branch_only = branch_only_conditions(func, &uses);
    let mut masks = None;
    let mut ordered = Vec::new();
    let mut seen = FxHashSet::default();
    let mut deleted = FxHashSet::default();
    let mut added = Vec::new();
    for block in func.blocks.indices() {
        let original = std::mem::take(&mut func.blocks[block].instructions);
        ordered.clear();
        seen.clear();
        deleted.clear();
        added.clear();
        for &inst in &original {
            if !removable(func, func.inst(inst), target) {
                let instruction = func.inst(inst);
                // Spelling a test moves no read: it adds pure operations at the test, so only a
                // write, a call or an observation of the execution ends the segment, and the
                // remaining gas is one.
                let reads_only = instruction.kind.effect_kind().is_read_only()
                    && !instruction.kind.observes_gas();
                if !(spelling && reads_only)
                    && (!is_pure(instruction) || !instruction.kind.effects().can_speculate())
                {
                    seen.clear();
                }
                ordered.push(inst);
                continue;
            }
            let op = func.inst(inst).kind.op();
            if spelling
                && !matches!(op, Op::Lt { .. } | Op::Gt { .. } | Op::Or { .. } | Op::And { .. })
            {
                seen.insert(inst);
                ordered.push(inst);
                continue;
            }
            let is_branch_only =
                func.inst_result_value(inst).is_some_and(|value| branch_only.contains(&value));
            let read = ConditionRead {
                branch: is_branch_only,
                in_loop: cyclic.as_ref().is_some_and(|cyclic| cyclic.contains(block)),
            };
            let mut best = None;
            for recipe in
                isle::alternatives(func, &original, &seen, &mut masks, &op, target, stage, read)
            {
                if let Some((after, leaves)) = recipe.cost_and_leaves(func, target, is_branch_only)
                {
                    let mut dead = FxHashSet::default();
                    collect_dead(func, &op, &uses, &seen, &leaves, target, &mut dead);
                    let empty = FxHashMap::default();
                    let before = root_cost(func, &op, target, &empty, is_branch_only)
                        + dead
                            .iter()
                            .map(|&inst| {
                                operation_cost(func, &func.inst(inst).kind.op(), target, &empty)
                            })
                            .sum::<Cost>();
                    if (recipe.enabling || target.cmp(after, before).is_lt())
                        && best.as_ref().is_none_or(|(_, cost, _)| target.cmp(after, *cost).is_lt())
                    {
                        best = Some((recipe, after, dead));
                    }
                }
            }
            if let Some((recipe, _, dead)) = best {
                // Remove dead single-use producers; insert recipe children; retain the root.
                for old in std::iter::once(inst).chain(dead.iter().copied()) {
                    func.inst(old).visit_operands(|operand| {
                        uses[operand] -= 1;
                    });
                }
                if let Some(masks) = &mut masks {
                    masks.remove(func, inst);
                }
                let inserted = recipe.materialize(func, inst);
                if let Some(masks) = &mut masks {
                    masks.insert(func, inst);
                    masks.refresh(func, inst);
                }
                uses.resize(func.num_values(), 0);
                for new in inserted.iter().copied().chain(std::iter::once(inst)) {
                    func.inst(new).visit_operands(|operand| {
                        uses[operand] += 1;
                    });
                }
                for &old in &dead {
                    seen.remove(&old);
                }
                deleted.extend(dead);
                seen.extend(inserted.iter().copied());
                added.extend(inserted.iter().copied());
                ordered.extend(inserted);
                changed = true;
            }
            seen.insert(inst);
            ordered.push(inst);
        }
        // Earlier producers replaced by recipes no longer occur in the block.
        ordered.retain(|inst| !deleted.contains(inst));
        // Reattaching the block adds the recipe instructions and drops the deleted producers.
        if let Some(masks) = &mut masks {
            for &inst in &added {
                masks.insert(func, inst);
            }
            for &inst in &deleted {
                masks.remove(func, inst);
            }
        }
        func.blocks[block].instructions = std::mem::replace(&mut ordered, original);
    }
    changed
}

fn collect_dead(
    func: &Function,
    node: &Op,
    uses: &IndexVec<ValueId, u32>,
    seen: &FxHashSet<InstId>,
    leaves: &FxHashSet<ValueId>,
    target: &Target,
    dead: &mut FxHashSet<InstId>,
) {
    let _ = node.map_values(|value| {
        if dead.len() < MAX_CONE
            && !leaves.contains(&value)
            && uses.get(value) == Some(&1)
            && let Value::Inst(inst) = func.value(value)
            && seen.contains(inst)
            && removable(func, func.inst(*inst), target)
            && dead.insert(*inst)
        {
            collect_dead(func, &func.inst(*inst).kind.op(), uses, seen, leaves, target, dead);
        }
        value
    });
}

/// Spells the leaves of an `or` that only the zero test of a branch reads. That word may be any
/// word that is nonzero exactly when a leaf holds, so a leaf `zext (x != 0)` is `x`, and a width
/// test `zext (x > 2 ** k - 1)` is the shift `x >> k` where the target prices the shift below
/// the comparison for a test that a branch reads.
fn spell_zero_tested(func: &mut Function, target: &Target, cyclic: &DenseBitSet<BlockId>) -> bool {
    let uses = super::egraph::use_counts(func);
    let branch_only = branch_only_conditions(func, &uses);
    let tested = zero_tested_words(func, &uses, &branch_only);
    if tested.is_empty() {
        return false;
    }
    let mut changed = false;
    for block in func.blocks.indices() {
        let original = func.blocks[block].instructions.clone();
        let defined = original.iter().copied().collect::<FxHashSet<_>>();
        // The shifts, each placed where the comparison it replaces was computed; one comparison
        // can have several leaves.
        let mut shifts = FxHashMap::<InstId, Vec<InstId>>::default();
        for &inst in &original {
            if let InstKind::Or(a, b) = func.inst(inst).kind
                && func.inst_result_value(inst).is_some_and(|value| tested.contains(&value))
            {
                let in_loop = cyclic.contains(block);
                let [a, b] = [a, b].map(|leaf| {
                    let (word, shift) =
                        zero_tested_leaf(func, target, &uses, &defined, leaf, in_loop);
                    changed |= word != leaf;
                    if let Some((comparison, shift)) = shift {
                        shifts.entry(comparison).or_default().push(shift);
                    }
                    word
                });
                func.inst_mut(inst).replace_kind(InstKind::Or(a, b));
            }
        }
        let mut spelled = Vec::with_capacity(original.len() + shifts.len());
        for &inst in &original {
            spelled.extend(shifts.remove(&inst).into_iter().flatten());
            spelled.push(inst);
        }
        func.blocks[block].instructions = spelled;
    }
    changed
}

/// The word that stands for `leaf` in a word that only a zero test reads, and the instruction
/// that computes it with the comparison it replaces, when it is not `leaf` itself. The shift goes
/// where that comparison is, so no work moves across an observation of the execution.
fn zero_tested_leaf(
    func: &mut Function,
    target: &Target,
    uses: &IndexVec<ValueId, u32>,
    defined: &FxHashSet<InstId>,
    leaf: ValueId,
    in_loop: bool,
) -> (ValueId, Option<(InstId, InstId)>) {
    let Some(&InstKind::Zext(test)) = func.value_inst_kind(leaf) else { return (leaf, None) };
    if uses[leaf] != 1 {
        return (leaf, None);
    }
    match func.value_inst_kind(test).cloned() {
        // zext (x != 0) => x
        Some(InstKind::Ne(word, zero))
            if func.value_u256(zero) == Some(U256::ZERO)
                && func.value_ty(word) == Some(MirType::I256) =>
        {
            (word, None)
        }
        // zext (x > 2 ** k - 1) => x >> k
        Some(InstKind::Gt(word, limit))
            if func.value_ty(word) == Some(MirType::I256)
                && let Value::Inst(comparison) = *func.value(test)
                && defined.contains(&comparison)
                && let Some(limit) = func.value_u256(limit)
                && let Some(bits) = limit
                    .checked_add(U256::ONE)
                    .and_then(|power| {
                        (power > U256::ONE && power.is_power_of_two()).then_some(power)
                    })
                    .map(|power| power.trailing_zeros() as u32)
                && target.shifts_bits_test(bits, false, true, in_loop) =>
        {
            let shift = func.alloc_value(Value::Immediate(Immediate::I256(U256::from(bits))));
            let (inst, word) = func.alloc_value_inst(
                Instruction::new(InstKind::Shr(shift, word), Some(MirType::I256))
                    .with_debug_info_dropped(),
            );
            (word, Some((comparison, inst)))
        }
        _ => (leaf, None),
    }
}

/// How the result of a test is read, which prices its spelling.
#[derive(Clone, Copy, Default)]
struct ConditionRead {
    /// A branch reads the word, through the `or` that joins it with other tests, if any.
    branch: bool,
    in_loop: bool,
}

/// The words that only a zero test of a branch reads, each through the `or`s that join it with
/// other words: such a word may be any word that is nonzero exactly when the original is.
fn zero_tested_words(
    func: &Function,
    uses: &IndexVec<ValueId, u32>,
    branch_only: &FxHashSet<ValueId>,
) -> FxHashSet<ValueId> {
    let mut tested = func
        .instructions()
        .filter_map(|inst| match func.inst(inst).kind {
            InstKind::Ne(word, zero)
                if func.value_u256(zero) == Some(U256::ZERO)
                    && func
                        .inst_result_value(inst)
                        .is_some_and(|test| branch_only.contains(&test))
                    && uses[word] == 1 =>
            {
                Some(word)
            }
            _ => None,
        })
        .collect();
    extend_through_or(func, uses, &mut tested);
    tested
}

/// Adds to `values` the operands that only an `or` of `values` reads, when the same block defines
/// them.
fn extend_through_or(
    func: &Function,
    uses: &IndexVec<ValueId, u32>,
    values: &mut FxHashSet<ValueId>,
) {
    let mut defined = FxHashSet::default();
    for block in &func.blocks {
        defined.clear();
        defined.extend(block.instructions.iter().copied());
        // Results come before their operands, so one backward walk reaches a whole tree.
        for &inst in block.instructions.iter().rev() {
            if func.inst_result_value(inst).is_some_and(|value| values.contains(&value))
                && let InstKind::Or(a, b) = func.inst(inst).kind
            {
                for operand in [a, b] {
                    if uses[operand] == 1
                        && let Value::Inst(def) = *func.value(operand)
                        && defined.contains(&def)
                    {
                        values.insert(operand);
                    }
                }
            }
        }
    }
}

/// Values whose every use is the condition of a branch or check in their defining block, the
/// shape the backend folds into `JUMPI`, and the tests that only the `or` of such a value joins:
/// the `or` costs the same as it would for the branch, so a test it joins is as free as one the
/// branch reads itself. Lowering a check branches on its condition.
fn branch_only_conditions(func: &Function, uses: &IndexVec<ValueId, u32>) -> FxHashSet<ValueId> {
    let mut branch_uses = FxHashMap::<ValueId, u32>::default();
    let mut defined = FxHashSet::<InstId>::default();
    for block in &func.blocks {
        defined.clear();
        let mut count = |defined: &FxHashSet<InstId>, condition| {
            if let Value::Inst(def) = *func.value(condition)
                && defined.contains(&def)
            {
                *branch_uses.entry(condition).or_default() += 1;
            }
        };
        for &inst in &block.instructions {
            let kind = &func.inst(inst).kind;
            if is_check(kind)
                && let InstKind::ICall { args, .. } = kind
            {
                count(&defined, args[0]);
            }
            defined.insert(inst);
        }
        if let Some(Terminator::Branch { condition, .. }) = block.terminator {
            count(&defined, condition);
        }
    }
    let mut read = branch_uses
        .into_iter()
        .filter_map(|(value, n)| (uses[value] == n).then_some(value))
        .collect();
    extend_through_or(func, uses, &mut read);
    read
}
