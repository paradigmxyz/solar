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
//! that a nonzero test whose every use is a branch or a check costs nothing: `JUMPI` reads
//! the word.
//! Pure, nontrapping operations without recipes stay in place without breaking
//! the segment; they never earn deletion credit.
//! Mask blends wait until storage forwarding and mask simplification have run,
//! so early recipes do not hide overwritten packed fields from those passes.

use super::lower_checks::is_check;
use crate::{
    backend::evm::{op, select},
    mir::{
        EffectKind, Function, Immediate, InstId, InstKind, Instruction, MirType, Module, Op,
        Terminator, Value, ValueId,
        pass::{MirPass, run_function_pass},
    },
    target::{Cost, Target},
};
use alloy_primitives::U256;
use solar_data_structures::{
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};

mod isle;
mod mask_uses;

const MAX_CONE: usize = 16;

pub(crate) enum WordSequence {
    Early,
    Late,
}

impl MirPass for WordSequence {
    fn name(&self) -> &'static str {
        "word-sequence"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, analyses| {
            run(func, &analyses.target(gcx), matches!(self, Self::Late))
        })
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
        matches!(
            lowering,
            select::OpcodeLowering::Unary { .. }
                | select::OpcodeLowering::Binary { .. }
                | select::OpcodeLowering::Nary { .. }
        ) && op::definition(lowering.opcode())
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
    let mut cost = if let Op::Select { cond, .. } = op {
        target.select(!super::egraph::is_bool_value(func, *cond))
    } else {
        target.op(op, immediate)
    };
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

/// The cost of a recipe root. A nonzero test whose every use is a branch or check condition
/// costs nothing, because `JUMPI` takes its operand as the condition.
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
        let kind = func.inst(last).kind.clone();
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
    let result = builder.emit_inst(kind, Some(operation_ty));
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

fn run(func: &mut Function, target: &Target, late: bool) -> bool {
    let mut uses = super::egraph::use_counts(func);
    let branch_only = branch_only_conditions(func, &uses);
    let mut masks = None;
    let mut changed = false;
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
                if !is_pure(instruction) || !instruction.kind.effects().can_speculate() {
                    seen.clear();
                }
                ordered.push(inst);
                continue;
            }
            let op = func.inst(inst).kind.op();
            let is_branch_only =
                func.inst_result_value(inst).is_some_and(|value| branch_only.contains(&value));
            let mut best = None;
            for recipe in isle::alternatives(func, &original, &seen, &mut masks, &op, target, late)
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

/// Values whose every use is the condition of a branch or check in their defining block, the
/// shape the backend folds into `JUMPI`. Lowering a check branches on its condition.
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
    branch_uses.into_iter().filter_map(|(value, n)| (uses[value] == n).then_some(value)).collect()
}
