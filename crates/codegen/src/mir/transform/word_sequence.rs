//! Bounded, proved word-expression recipes after e-graph extraction.
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

use crate::{
    backend::evm::{op, select},
    mir::{
        EffectKind, Function, Immediate, InstId, Instruction, MirType, Module, Op, Value, ValueId,
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

const MAX_CONE: usize = 16;

pub(crate) struct WordSequence;

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
        run_function_pass(module, analyses, |func, _| run(func, Target::new(gcx)))
    }
}

#[derive(Clone)]
enum Temporary {
    Operation(Op),
    Constant(U256),
}

#[derive(Clone)]
struct Recipe {
    root: Op,
    temporaries: FxHashMap<ValueId, Temporary>,
}

fn legal(op: &Op, target: Target) -> bool {
    op.into_kind().is_some_and(|kind| kind.effect_kind() == EffectKind::Pure)
        && select::opcode_lowering(op).is_some_and(|lowering| {
            matches!(
                lowering,
                select::OpcodeLowering::Unary { .. } | select::OpcodeLowering::Binary { .. }
            ) && op::definition(lowering.opcode())
                .is_some_and(|def| def.is_available(target.evm_version()))
        })
}

fn removable(inst: &Instruction, target: Target) -> bool {
    !inst.metadata.abi_validation()
        && inst.metadata.effect().is_none_or(|effect| effect == EffectKind::Pure)
        && legal(&inst.kind.op(), target)
}

fn operation_cost(
    func: &Function,
    op: &Op,
    target: Target,
    constants: &FxHashMap<ValueId, Temporary>,
) -> Cost {
    let immediate = |value| match constants.get(&value) {
        Some(Temporary::Constant(value)) => Some(*value),
        Some(Temporary::Operation(_)) => None,
        None => func.value_u256(value),
    };
    let mut cost = target.op(op, immediate);
    let _ = op.map_values(|value| {
        cost += immediate(value).map_or_else(|| target.dup(), |value| target.push(value));
        value
    });
    cost
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
        target: Target,
    ) -> Option<(Cost, FxHashSet<ValueId>)> {
        let mut nodes = Vec::new();
        let mut leaves = FxHashSet::default();
        self.collect(&self.root, &mut FxHashSet::default(), &mut nodes, &mut leaves);
        if nodes.len() > MAX_CONE || nodes.iter().any(|node| !legal(node, target)) {
            return None;
        }
        Some((
            nodes.iter().map(|node| operation_cost(func, node, target, &self.temporaries)).sum(),
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
        func.inst_mut(root).replace_kind(op.into_kind().expect("legal recipe root"));
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
            Temporary::Constant(value) => {
                func.alloc_value(Value::Immediate(Immediate::uint256(*value)))
            }
            Temporary::Operation(op) => {
                let op = op.map_values(|value| {
                    self.materialize_value(func, root, value, values, inserted)
                });
                // %temporary = recipe_child(earlier_values)
                let mut inst = Instruction::new(
                    op.into_kind().expect("legal recipe child"),
                    Some(MirType::uint256()),
                );
                inst.metadata = func.inst(root).metadata.debug_context();
                let (inst, value) = func.alloc_value_inst(inst);
                inserted.push(inst);
                value
            }
        };
        values.insert(value, actual);
        actual
    }
}

fn run(func: &mut Function, target: Target) -> bool {
    let mut uses = super::egraph::use_counts(func);
    let mut changed = false;
    for block in func.blocks.indices().collect::<Vec<_>>() {
        let original = std::mem::take(&mut func.blocks[block].instructions);
        let mut ordered = Vec::with_capacity(original.len());
        let mut seen = FxHashSet::default();
        let mut deleted = FxHashSet::default();
        for inst in original {
            if !removable(func.inst(inst), target) {
                seen.clear();
                ordered.push(inst);
                continue;
            }
            let op = func.inst(inst).kind.op();
            let mut best = None;
            for recipe in isle::alternatives(func, &seen, &op) {
                if let Some((after, leaves)) = recipe.cost_and_leaves(func, target) {
                    let mut dead = FxHashSet::default();
                    collect_dead(func, &op, &uses, &seen, &leaves, target, &mut dead);
                    let empty = FxHashMap::default();
                    let before = operation_cost(func, &op, target, &empty)
                        + dead
                            .iter()
                            .map(|&inst| {
                                operation_cost(func, &func.inst(inst).kind.op(), target, &empty)
                            })
                            .sum::<Cost>();
                    if target.cmp(after, before).is_lt()
                        && best.as_ref().is_none_or(|(_, cost, _)| target.cmp(after, *cost).is_lt())
                    {
                        best = Some((recipe, after, dead));
                    }
                }
            }
            if let Some((recipe, _, dead)) = best {
                // Remove dead single-use producers; insert recipe children; retain the root.
                for old in std::iter::once(inst).chain(dead.iter().copied()) {
                    for operand in func.inst(old).operands() {
                        let count = &mut uses[operand];
                        *count -= 1;
                    }
                }
                let inserted = recipe.materialize(func, inst);
                uses.resize(func.num_values(), 0);
                for new in inserted.iter().copied().chain(std::iter::once(inst)) {
                    for operand in func.inst(new).operands() {
                        uses[operand] += 1;
                    }
                }
                for &old in &dead {
                    seen.remove(&old);
                }
                deleted.extend(dead);
                seen.extend(inserted.iter().copied());
                ordered.extend(inserted);
                changed = true;
            }
            seen.insert(inst);
            ordered.push(inst);
        }
        // Earlier producers replaced by recipes no longer occur in the block.
        ordered.retain(|inst| !deleted.contains(inst));
        func.blocks[block].instructions = ordered;
    }
    changed
}

#[allow(clippy::too_many_arguments)]
fn collect_dead(
    func: &Function,
    node: &Op,
    uses: &IndexVec<ValueId, u32>,
    seen: &FxHashSet<InstId>,
    leaves: &FxHashSet<ValueId>,
    target: Target,
    dead: &mut FxHashSet<InstId>,
) {
    let _ = node.map_values(|value| {
        if dead.len() < MAX_CONE
            && !leaves.contains(&value)
            && uses.get(value) == Some(&1)
            && let Value::Inst(inst) = func.value(value)
            && seen.contains(inst)
            && removable(func.inst(*inst), target)
            && dead.insert(*inst)
        {
            collect_dead(func, &func.inst(*inst).kind.op(), uses, seen, leaves, target, dead);
        }
        value
    });
}
