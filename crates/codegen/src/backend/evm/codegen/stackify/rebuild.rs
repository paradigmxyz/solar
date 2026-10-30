//! Rebuilding cheap values instead of carrying them across a crowded stack.
//!
//! Common-subexpression elimination merges equal computations, such as the address of an array
//! element that a function loads early and stores back much later, and the merged value then
//! stays on the stack from its first use to its last. Where more words are live than `DUP` and
//! `SWAP` reach, every such word adds to the overflow, and the planner spills words to the
//! function's fixed frame. A frame costs more than its stores and reloads: it lies below the heap
//! of every external entry that can reach the function, so every call of those entries pays
//! memory expansion for it, whether or not the function runs.
//!
//! This rewrite builds a second candidate for the planner, a copy of the function in which the
//! values that cross a crowded point, a point where more words than the reach are live, are cheap
//! to compute again:
//!
//! - Values that differ by a constant: an affine form, a wrapping sum of other words times
//!   constants plus a constant, groups the crossing values whose words and factors agree, such as a
//!   sorting network's element addresses `a + 32 * (low + k) + 32`. The member whose definition
//!   dominates the others' becomes the group's base, and every other member is computed as `base +
//!   (c - c_base)`, or a subtraction when that constant is shorter. The base then stays live in
//!   place of the members, and each member costs one addition.
//! - Every crossing value is computed again right before each of its uses past a crowded point, in
//!   its own block after the point or in later blocks: a member from its base, any other value from
//!   its own operation chain of at most [`MAX_REBUILT_OPS`] opcodes when every stack word the chain
//!   reads stays live at that use anyway. A copy never makes another word live longer, and the
//!   original definition keeps serving the uses before the crowded point.
//!
//! Only stable operations qualify, the ones the scheduler already recomputes for values live
//! across writes into the spill area: pure single-opcode arithmetic, calldata loads, and casts,
//! whose result cannot change between the definition and the copy. A use inside a loop that does
//! not contain the definition never receives a copy, which would run on every iteration, and
//! neither does a phi input, which belongs to its predecessor's edge. A value with such a use past
//! a crowded point keeps all its uses, as the copies would leave it live across the point anyway.
//! Computations that only the replaced uses read are removed.
//!
//! The rewrite does not decide: the planner plans both functions and keeps the copy only when its
//! plan, charged the target's cost of the frame words each plan spills, beats the original's under
//! the objective. The copies carry the debug context of the instruction or terminator that reads
//! them, as the code they emit runs as part of it, and no choice reads debug metadata.
//!
//! The selection is a liveness estimate, not a search over subsets: the candidate rebuilds every
//! qualifying value or none. Copies are not shared: two uses of one value in a block receive two
//! copies, and two chains with a common operation repeat it. A group whose members no single
//! member dominates stays as it is.

use super::materialized_at_use;
use crate::{
    backend::evm::codegen::select::InstKindOpcode,
    mir::{
        BlockId, EffectKind, Function, Immediate, InstId, InstKind, Instruction, MirType, OpTraits,
        PtrLocation, Value, ValueId,
        analysis::{CfgInfo, Liveness, LoopInfo},
        utils::replace_terminator_uses,
    },
};
use alloy_primitives::U256;
use smallvec::{SmallVec, smallvec};
use solar_data_structures::{
    bit_set::DenseBitSet,
    map::{FxHashMap, FxHashSet},
};

/// Most opcodes a value's operation chain takes to be computed again.
const MAX_REBUILT_OPS: u32 = 4;

/// Most stack words an operation chain reads.
const MAX_REBUILT_LEAVES: usize = 2;

/// Most casts an operation chain looks through. They emit no opcode, but each is a step of the
/// walk that collects the chain.
const MAX_REBUILT_CASTS: u32 = 4;

/// Most operations an affine form looks through.
const MAX_AFFINE_OPS: u32 = 8;

/// Most words an affine form sums.
const MAX_AFFINE_TERMS: usize = 4;

/// How a value is computed again.
enum Recipe {
    /// From another value plus a wrapping constant; its definition is replaced too.
    Offset(ValueId, U256),
    /// From its own operations, operands first.
    Chain(Chain),
}

/// The positions of a block's crowded points in order, see [`crowded_points`].
type Points = SmallVec<[usize; 4]>;

/// The operations of a chain, operands first.
type Chain = SmallVec<[InstId; 4]>;

/// The stack words an operation chain reads.
type Leaves = SmallVec<[ValueId; 2]>;

/// A word as a wrapping sum of other words times constants, plus a constant.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Affine {
    /// The summed words with their factors, in value order, without zero factors.
    terms: SmallVec<[(ValueId, U256); 2]>,
    constant: U256,
}

impl Affine {
    fn constant(constant: U256) -> Self {
        Self { terms: SmallVec::new(), constant }
    }

    fn word(value: ValueId) -> Self {
        Self { terms: smallvec![(value, U256::ONE)], constant: U256::ZERO }
    }

    /// `self + other`, or `self - other`.
    fn add(mut self, other: &Self, subtract: bool) -> Self {
        let sign = |x: U256| if subtract { x.wrapping_neg() } else { x };
        for &(value, factor) in &other.terms {
            match self.terms.iter_mut().find(|(existing, _)| *existing == value) {
                Some((_, existing)) => *existing = existing.wrapping_add(sign(factor)),
                None => self.terms.push((value, sign(factor))),
            }
        }
        self.terms.retain(|(_, factor)| !factor.is_zero());
        self.terms.sort_unstable_by_key(|&(value, _)| value);
        self.constant = self.constant.wrapping_add(sign(other.constant));
        self
    }

    /// `self * factor`.
    fn scale(mut self, factor: U256) -> Self {
        for (_, existing) in &mut self.terms {
            *existing = existing.wrapping_mul(factor);
        }
        self.terms.retain(|(_, factor)| !factor.is_zero());
        self.constant = self.constant.wrapping_mul(factor);
        self
    }
}

/// Returns a copy of `func` that computes the values crossing its crowded points again, as the
/// module documentation describes, or `None` when no value qualifies. `extra` counts the words
/// the stack holds besides the live values, such as the return address.
pub(super) fn rebuild_crowded_values(
    func: &Function,
    internal: bool,
    liveness: &Liveness,
    cfg: &CfgInfo,
    loops: &LoopInfo,
    reach: usize,
    extra: usize,
) -> Option<Function> {
    let fresh = |value: ValueId| materialized_at_use(func, internal, value);
    let (crowded, points) = crowded_points(func, liveness, &fresh, reach, extra)?;
    let mut defs = FxHashMap::<ValueId, (BlockId, usize)>::default();
    for (block, data) in func.blocks.iter_enumerated() {
        for (index, &inst) in data.instructions.iter().enumerate() {
            if let Some(result) = func.inst_result_value(inst) {
                defs.insert(result, (block, index));
            }
        }
    }
    let candidates: Vec<ValueId> = crowded
        .iter()
        .filter(|&value| !fresh(value) && defs.contains_key(&value) && rebuildable(func, value))
        .collect();

    // The uses of a value past a crowded point, or `None` when one of them cannot take a copy.
    let containing = |block: BlockId| {
        loops.all_loops().filter(move |lp| lp.blocks.contains(block)).map(|lp| lp.header)
    };
    let uses_past_crowding = |value: ValueId| -> Option<SmallVec<[(BlockId, usize); 4]>> {
        let (home, def_index) = defs[&value];
        let first_point = points
            .get(&home)
            .and_then(|points| points.iter().copied().find(|&point| point > def_index));
        let home_loops: SmallVec<[BlockId; 2]> = containing(home).collect();
        let mut uses = SmallVec::new();
        for (block, data) in func.blocks.iter_enumerated() {
            let past = |index| block != home || first_point.is_some_and(|point| point <= index);
            for (index, &inst) in data.instructions.iter().enumerate() {
                let kind = &func.inst(inst).kind;
                if past(index) && kind.operands().contains(&value) {
                    if matches!(kind, InstKind::Phi(_)) {
                        return None;
                    }
                    uses.push((block, index));
                }
            }
            let end = data.instructions.len();
            if past(end)
                && data.terminator.as_ref().is_some_and(|term| term.operands().contains(&value))
            {
                uses.push((block, end));
            }
        }
        // A copy inside a loop that does not contain the definition runs on every iteration.
        for &(block, _) in &uses {
            if !containing(block).all(|header| home_loops.contains(&header)) {
                return None;
            }
        }
        Some(uses)
    };
    // Whether `leaf` stays live at a use anyway: read there or later in its block, or live out.
    let live_at = |leaf: ValueId, (block, index): (BlockId, usize)| {
        let data = &func.blocks[block];
        fresh(leaf)
            || liveness.live_out(block).contains(leaf)
            || data.terminator.as_ref().is_some_and(|term| term.operands().contains(&leaf))
            || data.instructions[index..]
                .iter()
                .any(|&inst| func.inst(inst).kind.operands().contains(&leaf))
    };

    // Values whose words and factors agree differ by a constant, and are computed from the one
    // whose definition dominates the others'.
    let mut forms = FxHashMap::<ValueId, Affine>::default();
    let mut groups = FxHashMap::<SmallVec<[(ValueId, U256); 2]>, SmallVec<[ValueId; 4]>>::default();
    for &value in &candidates {
        if let Some(form) = affine_form(func, value, &mut 0)
            && !form.terms.is_empty()
            && form.terms.len() <= MAX_AFFINE_TERMS
        {
            groups.entry(form.terms.clone()).or_default().push(value);
            forms.insert(value, form);
        }
    }
    let dominates = |a: ValueId, b: ValueId| {
        let ((block_a, index_a), (block_b, index_b)) = (defs[&a], defs[&b]);
        if block_a == block_b {
            index_a <= index_b
        } else {
            cfg.dominators().dominates(block_a, block_b)
        }
    };
    let mut recipes = FxHashMap::<ValueId, Recipe>::default();
    let mut bases = FxHashSet::<ValueId>::default();
    for members in groups.values().filter(|members| members.len() > 1) {
        let Some(&base) =
            members.iter().find(|&&base| members.iter().all(|&other| dominates(base, other)))
        else {
            continue;
        };
        bases.insert(base);
        for &member in members.iter().filter(|&&member| member != base) {
            let offset = forms[&member].constant.wrapping_sub(forms[&base].constant);
            recipes.insert(member, Recipe::Offset(base, offset));
        }
    }
    for &value in &candidates {
        if recipes.contains_key(&value) || bases.contains(&value) {
            continue;
        }
        if let Some((insts, leaves)) = operation_chain(func, internal, value)
            && let Some(uses) = uses_past_crowding(value)
            && !uses.is_empty()
            && uses.iter().all(|&site| leaves.iter().all(|&leaf| live_at(leaf, site)))
        {
            recipes.insert(value, Recipe::Chain(insts));
        }
    }
    if recipes.is_empty() {
        return None;
    }

    // Each use that reads a copy, by block and position.
    let mut sites = FxHashMap::<BlockId, Vec<(usize, ValueId)>>::default();
    for &value in recipes.keys() {
        for (block, index) in uses_past_crowding(value).unwrap_or_default() {
            sites.entry(block).or_default().push((index, value));
        }
    }

    let mut result = func.clone();
    // v = <operations over the base's words>  ->  v = add base, offset
    let mut members: Vec<(ValueId, ValueId, U256)> = recipes
        .iter()
        .filter_map(|(&value, recipe)| match *recipe {
            Recipe::Offset(base, offset) => Some((value, base, offset)),
            Recipe::Chain(_) => None,
        })
        .collect();
    members.sort_unstable_by_key(|&(value, ..)| value);
    for (value, base, offset) in members {
        let Value::Inst(def) = *result.value(value) else { unreachable!("rebuilt instruction") };
        let kind = offset_kind(&mut result, base, offset);
        result.inst_mut(def).replace_kind(kind);
    }
    let mut blocks: Vec<BlockId> = sites.keys().copied().collect();
    blocks.sort_unstable();
    for block in blocks {
        let mut uses = sites.remove(&block).unwrap();
        uses.sort_unstable();
        let original = std::mem::take(&mut result.blocks[block].instructions);
        let end = original.len();
        let mut copies = Vec::<(usize, InstId)>::new();
        for &(index, value) in &uses {
            let reader = if index == end {
                result.blocks[block].terminator_metadata.debug_context()
            } else {
                result.inst(original[index]).metadata.debug_context()
            };
            let copy = match recipes[&value] {
                Recipe::Offset(base, offset) if offset.is_zero() => base,
                Recipe::Offset(base, offset) => {
                    // v' = add base, offset
                    let kind = offset_kind(&mut result, base, offset);
                    let mut copy = Instruction::new(kind, Some(MirType::I256));
                    copy.metadata = reader;
                    let (copy_id, copy_value) = result.alloc_value_inst(copy);
                    copies.push((index, copy_id));
                    copy_value
                }
                Recipe::Chain(ref insts) => {
                    // v' = op(operands') for each operation, operands first
                    let mut renamed = FxHashMap::<ValueId, ValueId>::default();
                    for &inst in insts {
                        let source = result.inst(inst);
                        let mut kind = source.kind.clone();
                        kind.visit_operands_mut(|operand| {
                            if let Some(&copy) = renamed.get(operand) {
                                *operand = copy;
                            }
                        });
                        let mut copy = Instruction::new(kind, source.result_ty);
                        copy.metadata = reader.debug_context();
                        let original_result = result.inst_result_value(inst).unwrap();
                        let (copy_id, copy_value) = result.alloc_value_inst(copy);
                        renamed.insert(original_result, copy_value);
                        copies.push((index, copy_id));
                    }
                    renamed[&value]
                }
            };
            if index == end {
                if let Some(term) = &mut result.blocks[block].terminator {
                    replace_terminator_uses(term, &FxHashMap::from_iter([(value, copy)]));
                }
            } else {
                // The copy equals the value it replaces, so the reader keeps its facts.
                result.inst_mut(original[index]).kind.visit_operands_mut(|operand| {
                    if *operand == value {
                        *operand = copy;
                    }
                });
            }
        }
        // Each copy goes right before its reader.
        let mut instructions = Vec::with_capacity(original.len() + copies.len());
        let mut copies = copies.into_iter().peekable();
        for (index, inst) in original.into_iter().enumerate() {
            while let Some((_, copy)) = copies.next_if(|&(at, _)| at == index) {
                instructions.push(copy);
            }
            instructions.push(inst);
        }
        instructions.extend(copies.map(|(_, copy)| copy));
        result.blocks[block].instructions = instructions;
    }
    remove_unused(&mut result);
    Some(result)
}

/// The crowded points: per block, the positions right before an instruction, or before the
/// terminator at the block's length, where more stack words than `reach` are live, counting
/// `extra` words besides the live values that are not pushed fresh. Returns the values live at
/// any such point and the points of each block in order, or `None` when there is none.
fn crowded_points(
    func: &Function,
    liveness: &Liveness,
    fresh: &dyn Fn(ValueId) -> bool,
    reach: usize,
    extra: usize,
) -> Option<(DenseBitSet<ValueId>, FxHashMap<BlockId, Points>)> {
    let mut crowded = DenseBitSet::new_empty(func.num_values());
    let mut points = FxHashMap::<BlockId, Points>::default();
    let mut live = DenseBitSet::new_empty(func.num_values());
    for (block, data) in func.blocks.iter_enumerated() {
        live.clear();
        live.union(&liveness.live_out(block));
        let mut words = live.iter().filter(|&value| !fresh(value)).count() + extra;
        let mut block_points = Points::new();
        let mut note = |live: &DenseBitSet<ValueId>, words: usize, point: usize| {
            if words > reach {
                block_points.push(point);
                crowded.union(live);
            }
        };
        if let Some(term) = &data.terminator {
            term.visit_operands(|value| {
                if live.insert(value) && !fresh(value) {
                    words += 1;
                }
            });
        }
        note(&live, words, data.instructions.len());
        for (index, &inst) in data.instructions.iter().enumerate().rev() {
            if let Some(result) = func.inst_result_value(inst)
                && live.remove(result)
                && !fresh(result)
            {
                words -= 1;
            }
            let kind = &func.inst(inst).kind;
            if !matches!(kind, InstKind::Phi(_)) {
                kind.visit_operands(|value| {
                    if live.insert(value) && !fresh(value) {
                        words += 1;
                    }
                });
            }
            note(&live, words, index);
        }
        if !block_points.is_empty() {
            block_points.reverse();
            points.insert(block, block_points);
        }
    }
    (!points.is_empty()).then_some((crowded, points))
}

/// The affine form of a 256-bit word through at most [`MAX_AFFINE_OPS`] additions,
/// subtractions, and multiplications or left shifts by constants.
fn affine_form(func: &Function, value: ValueId, ops: &mut u32) -> Option<Affine> {
    if func.value_ty(value) != Some(MirType::I256) {
        return None;
    }
    if let Some(constant) = func.value_u256(value) {
        return Some(Affine::constant(constant));
    }
    let Value::Inst(def) = *func.value(value) else { return Some(Affine::word(value)) };
    let mut operand = |operand: ValueId| {
        *ops += 1;
        if *ops > MAX_AFFINE_OPS {
            return None;
        }
        affine_form(func, operand, ops)
    };
    Some(match func.inst(def).kind {
        InstKind::Add(a, b) => operand(a)?.add(&operand(b)?, false),
        InstKind::Sub(a, b) => operand(a)?.add(&operand(b)?, true),
        InstKind::Shl(shift, word) if let Some(shift) = func.value_u256(shift) => {
            let factor = u8::try_from(shift).map_or(U256::ZERO, |shift| U256::ONE << shift);
            operand(word)?.scale(factor)
        }
        InstKind::Mul(a, b) if let Some(factor) = func.value_u256(b) => operand(a)?.scale(factor),
        InstKind::Mul(a, b) if let Some(factor) = func.value_u256(a) => operand(b)?.scale(factor),
        _ => Affine::word(value),
    })
}

/// `base + offset` as one instruction, subtracting an offset whose negation is shorter.
fn offset_kind(func: &mut Function, base: ValueId, offset: U256) -> InstKind {
    if offset.bit(255) {
        let constant = func.alloc_value(Value::Immediate(Immediate::I256(offset.wrapping_neg())));
        InstKind::Sub(base, constant)
    } else {
        let constant = func.alloc_value(Value::Immediate(Immediate::I256(offset)));
        InstKind::Add(base, constant)
    }
}

/// The operations computing `value` from fresh words and at most [`MAX_REBUILT_LEAVES`] stack
/// words in at most [`MAX_REBUILT_OPS`] opcodes and [`MAX_REBUILT_CASTS`] casts, operands first,
/// with those stack words. Each operation is charged before its operands are visited, so a long
/// dependency chain stops at the budget instead of being walked back to its start.
fn operation_chain(func: &Function, internal: bool, value: ValueId) -> Option<(Chain, Leaves)> {
    fn collect(
        func: &Function,
        internal: bool,
        value: ValueId,
        insts: &mut Chain,
        leaves: &mut Leaves,
        ops: &mut u32,
        casts: &mut u32,
    ) -> bool {
        if materialized_at_use(func, internal, value) {
            return true;
        }
        if !rebuildable(func, value) {
            if !leaves.contains(&value) {
                leaves.push(value);
            }
            return leaves.len() <= MAX_REBUILT_LEAVES;
        }
        let Value::Inst(def) = *func.value(value) else { unreachable!("rebuildable instruction") };
        if insts.contains(&def) {
            return true;
        }
        let kind = &func.inst(def).kind;
        let (spent, budget) =
            if matches!(kind, InstKind::Zext(_) | InstKind::PtrToInt(..) | InstKind::IntToPtr(_)) {
                (&mut *casts, MAX_REBUILT_CASTS)
            } else {
                (&mut *ops, MAX_REBUILT_OPS)
            };
        *spent += 1;
        if *spent > budget
            || !kind
                .operands()
                .iter()
                .all(|&operand| collect(func, internal, operand, insts, leaves, ops, casts))
        {
            return false;
        }
        insts.push(def);
        true
    }
    let mut insts = SmallVec::new();
    let mut leaves = SmallVec::new();
    collect(func, internal, value, &mut insts, &mut leaves, &mut 0, &mut 0)
        .then_some((insts, leaves))
}

/// Whether `value` is computed by one stable opcode or a cast, which a copy repeats exactly.
fn rebuildable(func: &Function, value: ValueId) -> bool {
    let Value::Inst(def) = *func.value(value) else { return false };
    let kind = &func.inst(def).kind;
    match kind {
        InstKind::Zext(_)
        | InstKind::PtrToInt(..)
        | InstKind::IntToPtr(_)
        | InstKind::Load(_, PtrLocation::Calldata) => true,
        _ => {
            let def = kind.op_def();
            def.traits.contains(OpTraits::REMATERIALIZABLE)
                && def.effect == EffectKind::Pure
                && kind.evm_opcode().is_some()
                && !kind.operands().is_empty()
        }
    }
}

/// Removes the stable computations that nothing reads any more, such as the operations of a
/// value now computed from its base.
fn remove_unused(func: &mut Function) {
    let mut uses = FxHashMap::<ValueId, u32>::default();
    for block in &func.blocks {
        for &inst in &block.instructions {
            func.inst(inst).kind.visit_operands(|value| *uses.entry(value).or_default() += 1);
        }
        if let Some(term) = &block.terminator {
            term.visit_operands(|value| *uses.entry(value).or_default() += 1);
        }
    }
    let mut work: Vec<ValueId> = func
        .instructions()
        .filter_map(|inst| func.inst_result_value(inst))
        .filter(|value| !uses.contains_key(value))
        .collect();
    let mut removed = FxHashSet::<InstId>::default();
    while let Some(value) = work.pop() {
        if uses.get(&value).is_some_and(|&count| count != 0) || !rebuildable(func, value) {
            continue;
        }
        let Value::Inst(def) = *func.value(value) else { continue };
        if !removed.insert(def) {
            continue;
        }
        func.inst(def).kind.visit_operands(|operand| {
            if let Some(count) = uses.get_mut(&operand) {
                *count -= 1;
                if *count == 0 {
                    work.push(operand);
                }
            }
        });
    }
    for block in &mut func.blocks {
        block.instructions.retain(|inst| !removed.contains(inst));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::FunctionBuilder;
    use solar_interface::Ident;

    /// A function adding one to its parameter `len` times, and the last sum.
    fn add_chain(len: usize) -> (Function, ValueId) {
        let mut func = Function::new(Ident::DUMMY);
        let mut builder = FunctionBuilder::new(&mut func);
        let mut value = builder.add_param(MirType::I256);
        let one = builder.imm(1);
        for _ in 0..len {
            value = builder.add(value, one);
        }
        (func, value)
    }

    #[test]
    fn operation_chains_stop_at_their_budget() {
        // Four additions fit, reading the parameter as their only stack word.
        let (func, value) = add_chain(4);
        let (insts, leaves) = operation_chain(&func, true, value).expect("four additions fit");
        assert_eq!((insts.len(), leaves.len()), (4, 1));
        // A longer chain is rejected within the budget, not walked back to its start, which
        // takes one stack frame per operation.
        let (func, value) = add_chain(100_000);
        assert!(operation_chain(&func, true, value).is_none());
    }
}
