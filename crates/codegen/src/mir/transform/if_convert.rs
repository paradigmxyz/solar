//! If-conversion of small branch diamonds and triangles into selects.
//!
//! A branch whose arms only compute a few pure values and merge them in a
//! join block pays a `JUMPI`, a `JUMP` on one side, a `JUMPDEST`, and a stack
//! layout reconciliation at the join on every execution. When the arms are
//! cheap enough to run unconditionally, the pass moves their instructions
//! into the branching block, replaces each join phi that merges the arm
//! values by a branch-free select on the branch condition, and turns the
//! branch into a jump. A diamond has two single-predecessor arms that jump
//! to the same join; a triangle has one such arm while the other edge
//! reaches the join directly.
//!
//! A boolean condition scales an amount, so every select is arithmetic:
//!
//! - a value merged with one derived from it, `c ? a + k : a`, `c ? a | k : a`, `c ? a >> k : a`,
//!   and `c ? t : 0`, becomes `a + c * k`, `a | c * k`, `a >> c * k`, and `c * t`, the shapes
//!   hand-written bit-search assembly uses;
//! - two literals, `c ? A : B`, become `B + c * (A - B)` with the difference folded, so a lookup
//!   among constant table words needs no branch;
//! - a literal chosen over a select the condition implies, `x < a ? A : (x < b ? B : C)` with `a <=
//!   b`, becomes `C + [x < b] * (B - C) + [x < a] * (A - B)`, so a ladder of literal selections on
//!   one value collapses one level at a time; and
//! - any other pair becomes `f + c * (t - f)`.
//!
//! A non-boolean condition is normalized with two `iszero`s first. Later
//! simplification turns a power-of-two multiplier into a shift.
//!
//! Safety: an arm qualifies only when the branching block is its sole
//! predecessor, its terminator is a jump to the join, and every instruction
//! is a pure computation (no memory or state reads, calls, or effects), so
//! speculating it cannot trap, expand memory, or change observable state.
//! Pointer-typed phis keep their branch so allocation provenance survives.
//! Profitability is priced through the target: at most three instructions
//! per arm and two phis per join are considered, and a site converts only
//! when running both arms plus the selects on every execution costs no more
//! than the branch, the arm jump, their labels, and the arm that runs on an
//! average path. Runs after the late CFG cleanup, so folded conditions never
//! reach it, and before hot-leaf inlining, so cloned lookup helpers arrive
//! already branch-free.

use super::cfg_simplify::simplify_function;
use crate::{
    backend::evm::op,
    mir::{
        BlockId, EffectKind, Function, Immediate, InstId, InstKind, Instruction, MirType, Module,
        Terminator, Value, ValueId,
        pass::{MirPass, run_function_pass},
    },
    target::{Cost, Target},
};
use alloy_primitives::U256;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::FxHashMap,
};
use std::cmp::Ordering;

/// Function pass that converts small branch diamonds and triangles into selects.
pub(crate) struct IfConvert;

impl MirPass for IfConvert {
    fn name(&self) -> &'static str {
        "if-convert"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let target = Target::new(gcx);
        run_function_pass(module, analyses, |func, _| if_convert_function(func, target))
    }
}

/// Instructions an arm may hold; each is executed unconditionally afterwards.
const MAX_ARM_INSTRUCTIONS: usize = 3;
/// Phis a join may merge; each becomes a select.
const MAX_SELECTS: usize = 2;

/// One convertible branch: the branching block, its condition, the arms that
/// hold speculatable instructions, the join their values reach, and the
/// select each join phi becomes.
struct Site {
    block: BlockId,
    condition: ValueId,
    /// The arm reached when the condition holds, when it is a separate block.
    then_arm: Option<BlockId>,
    /// The arm reached when the condition fails, when it is a separate block.
    else_arm: Option<BlockId>,
    join: BlockId,
    selects: Vec<Select>,
}

/// A join phi and the branch-free form that replaces it.
struct Select {
    phi: InstId,
    then_value: ValueId,
    else_value: ValueId,
    form: SelectForm,
}

/// The arithmetic that computes `c ? then : else` for a zero-or-one `c`.
#[derive(Clone, Copy)]
enum SelectForm {
    /// Both edges carry the same value.
    Same,
    /// `then = else op k`: `else op c * k`, or `c * then` from a zero else.
    Scaled(Derivation),
    /// `else = then op k`: `then op (iszero c) * k`, or `(iszero c) * else`.
    ScaledNegated(Derivation),
    /// A literal `then` over an `else` that already selects literals on a
    /// condition implied by this one: `else + c * delta`.
    Ladder(U256),
    /// `else + c * (then - else)`, with the difference folded when both are
    /// literals.
    General(Option<U256>),
}

fn if_convert_function(func: &mut Function, target: Target) -> bool {
    let mut changed = false;
    while let Some(site) = find_site(func, target) {
        convert(func, &site);
        simplify_function(func);
        changed = true;
    }
    changed
}

/// Predecessor lists over the blocks reachable from the entry.
fn predecessors(func: &Function) -> IndexVec<BlockId, Vec<BlockId>> {
    let mut preds = index_vec![Vec::new(); func.blocks.len()];
    let mut reachable = DenseBitSet::new_empty(func.blocks.len());
    let mut worklist = vec![BlockId::ENTRY];
    reachable.insert(BlockId::ENTRY);
    while let Some(block) = worklist.pop() {
        let Some(terminator) = &func.blocks[block].terminator else { continue };
        for successor in terminator.successors() {
            preds[successor].push(block);
            if reachable.insert(successor) {
                worklist.push(successor);
            }
        }
    }
    preds
}

fn find_site(func: &Function, target: Target) -> Option<Site> {
    let preds = predecessors(func);
    for (block, body) in func.blocks.iter_enumerated() {
        let Some(Terminator::Branch { condition, then_block, else_block }) = body.terminator else {
            continue;
        };
        if then_block == else_block || (block != BlockId::ENTRY && preds[block].is_empty()) {
            continue;
        }
        let then_join = arm_join(func, &preds, block, then_block);
        let else_join = arm_join(func, &preds, block, else_block);
        let (then_arm, else_arm, join) = match (then_join, else_join) {
            // then_arm -> join <- else_arm
            (Some(join), Some(other)) if join == other => {
                (Some(then_block), Some(else_block), join)
            }
            // then_arm -> join, block -> join
            (Some(join), _) if join == else_block => (Some(then_block), None, join),
            // block -> join, else_arm -> join
            (_, Some(join)) if join == then_block => (None, Some(else_block), join),
            _ => continue,
        };
        if join == block {
            continue;
        }
        let mut site = Site { block, condition, then_arm, else_arm, join, selects: Vec::new() };
        if let Some(selects) = join_selects(func, &site)
            && profitable(func, target, &site, &selects)
        {
            site.selects = selects;
            return Some(site);
        }
    }
    None
}

/// The join an arm jumps to, when the arm is speculatable from `block`.
fn arm_join(
    func: &Function,
    preds: &IndexVec<BlockId, Vec<BlockId>>,
    block: BlockId,
    arm: BlockId,
) -> Option<BlockId> {
    if arm == block || preds[arm].as_slice() != [block] {
        return None;
    }
    let body = &func.blocks[arm];
    let Some(Terminator::Jump(join)) = body.terminator else { return None };
    if join == arm || join == block || body.instructions.len() > MAX_ARM_INSTRUCTIONS {
        return None;
    }
    let speculatable = body.instructions.iter().all(|&inst| {
        let kind = &func.inst(inst).kind;
        !matches!(kind, InstKind::Phi(_))
            && !kind.has_side_effects()
            && kind.effect_kind() == EffectKind::Pure
            && func.inst_result_value(inst).is_some()
    });
    speculatable.then_some(join)
}

/// The select for every phi of the join, when there are few enough and each
/// merges word values along the two converted edges.
fn join_selects(func: &Function, site: &Site) -> Option<Vec<Select>> {
    let then_pred = site.then_arm.unwrap_or(site.block);
    let else_pred = site.else_arm.unwrap_or(site.block);
    let mut selects = Vec::new();
    for &phi in &func.blocks[site.join].instructions {
        let InstKind::Phi(incoming) = &func.inst(phi).kind else { continue };
        let incoming_from =
            |pred| incoming.iter().find(|&&(from, _)| from == pred).map(|&(_, v)| v);
        let (then_value, else_value) = (incoming_from(then_pred)?, incoming_from(else_pred)?);
        if is_pointer(func, then_value) || is_pointer(func, else_value) {
            return None;
        }
        let form = select_form(func, site.condition, then_value, else_value);
        selects.push(Select { phi, then_value, else_value, form });
        if selects.len() > MAX_SELECTS {
            return None;
        }
    }
    Some(selects)
}

/// Whether a value carries memory, storage, or calldata provenance that the
/// arithmetic forms would erase.
fn is_pointer(func: &Function, value: ValueId) -> bool {
    matches!(
        func.value_ty(value),
        Some(
            MirType::MemPtr
                | MirType::MemoryObject(_)
                | MirType::StoragePtr
                | MirType::CalldataPtr
                | MirType::Slice(_)
        )
    )
}

fn select_form(
    func: &Function,
    condition: ValueId,
    then_value: ValueId,
    else_value: ValueId,
) -> SelectForm {
    if then_value == else_value {
        return SelectForm::Same;
    }
    if let Some(derived) = derivation(func, then_value, else_value) {
        return SelectForm::Scaled(derived);
    }
    if let Some(derived) = derivation(func, else_value, then_value) {
        return SelectForm::ScaledNegated(derived);
    }
    let then_literal = func.value_u256(then_value);
    let else_literal = func.value_u256(else_value);
    // A ? then : (base + c2 * k) with c => c2 and literal then, base, and k.
    if let Some(then) = then_literal
        && let Some((base, scaled)) = binary_operands(func, else_value, |kind| match *kind {
            InstKind::Add(a, b) => Some((a, b)),
            _ => None,
        })
        && let Some((amount, c2)) = binary_operands(func, scaled, |kind| match *kind {
            InstKind::Mul(a, b) => Some((a, b)),
            _ => None,
        })
        && let Some(base) = func.value_u256(base)
        && let Some(amount) = func.value_u256(amount)
        && implies(func, condition, c2)
    {
        return SelectForm::Ladder(then.wrapping_sub(base).wrapping_sub(amount));
    }
    SelectForm::General(match (then_literal, else_literal) {
        (Some(then), Some(else_)) => Some(then.wrapping_sub(else_)),
        _ => None,
    })
}

/// The operands of a commutative binary instruction defining `value`, ordered
/// so the first is a literal when either is.
fn binary_operands(
    func: &Function,
    value: ValueId,
    operands: impl Fn(&InstKind) -> Option<(ValueId, ValueId)>,
) -> Option<(ValueId, ValueId)> {
    let Value::Inst(inst) = func.value(value) else { return None };
    let (a, b) = operands(&func.inst(*inst).kind)?;
    if func.value_u256(a).is_some() || func.value_u256(b).is_none() {
        Some((a, b))
    } else {
        Some((b, a))
    }
}

/// Whether a nonzero `first` implies a nonzero `second`: the same value, or
/// bounds of one value by ordered literals (`x < a` implies `x < b` when
/// `a <= b`, and `x > a` implies `x > b` when `a >= b`).
fn implies(func: &Function, first: ValueId, second: ValueId) -> bool {
    if first == second {
        return true;
    }
    let kind = |value| match func.value(value) {
        Value::Inst(inst) => Some(&func.inst(*inst).kind),
        _ => None,
    };
    match (kind(first), kind(second)) {
        (Some(&InstKind::Lt(x, a)), Some(&InstKind::Lt(y, b))) if x == y => {
            matches!((func.value_u256(a), func.value_u256(b)), (Some(a), Some(b)) if a <= b)
        }
        (Some(&InstKind::Gt(x, a)), Some(&InstKind::Gt(y, b))) if x == y => {
            matches!((func.value_u256(a), func.value_u256(b)), (Some(a), Some(b)) if a >= b)
        }
        _ => false,
    }
}

/// Whether running both arms and the selects on every execution costs no
/// more under the objective than the branch, the arm jump, their labels,
/// one stack move per phi at the join, and the arm and phi literal an
/// average path executes.
fn profitable(func: &Function, target: Target, site: &Site, selects: &[Select]) -> bool {
    // Arm instructions whose results the scaled forms recompute are dropped.
    let replaced: Vec<ValueId> = selects
        .iter()
        .filter_map(|select| match select.form {
            SelectForm::Scaled(_) => Some(select.then_value),
            SelectForm::ScaledNegated(_) => Some(select.else_value),
            _ => None,
        })
        .collect();
    let mut arms = Cost::ZERO;
    let mut kept_arms = Cost::ZERO;
    for arm in [site.then_arm, site.else_arm].into_iter().flatten() {
        for &inst in &func.blocks[arm].instructions {
            let cost = inst_cost(func, target, inst);
            arms = arms.plus(cost);
            if !func.inst_result_value(inst).is_some_and(|result| replaced.contains(&result)) {
                kept_arms = kept_arms.plus(cost);
            }
        }
    }
    let mut selected = Cost::ZERO;
    let mut joined = Cost::ZERO;
    for select in selects {
        selected = selected.plus(select_cost(func, target, select));
        // The join moves one stack slot per phi, and a literal reaching it
        // is pushed on its path.
        joined = joined.plus(target.dup().times(2));
        for value in [select.then_value, select.else_value] {
            if let Some(literal) = func.value_u256(value) {
                joined = joined.plus(target.push(literal));
            }
        }
    }
    // Doubled costs: the transfers and join moves run on every path, each
    // arm and phi literal on one of the two paths.
    let before = transfer_cost(target).plus(arms).plus(joined);
    let after = kept_arms.plus(selected).times(2);
    target.cmp(after, before) != Ordering::Greater
}

/// Doubled per-execution cost of the transfers a site removes: the branch
/// and the join label on every path, and the arm jump with its label on one
/// of the two paths.
fn transfer_cost(target: Target) -> Cost {
    let label = target.opcode(op::PUSH2);
    let every_path = target.opcode(op::JUMPI).plus(label).plus(target.opcode(op::JUMPDEST));
    let one_path = target.opcode(op::JUMP).plus(label).plus(target.opcode(op::JUMPDEST));
    every_path.times(2).plus(one_path)
}

/// Cost of one instruction: its operation and the pushes that materialize
/// its literal operands.
fn inst_cost(func: &Function, target: Target, inst: InstId) -> Cost {
    let kind = &func.inst(inst).kind;
    let mut cost = target.op(&kind.op(), |value| func.value_u256(value));
    for operand in kind.operands() {
        if let Some(literal) = func.value_u256(operand) {
            cost = cost.plus(target.push(literal));
        }
    }
    cost
}

/// Cost of the instructions a select adds: the pushes of its literal
/// operands, one duplicate of an operand it uses twice, and its operations.
fn select_cost(func: &Function, target: Target, select: &Select) -> Cost {
    let literal = |value| func.value_u256(value).map_or(Cost::ZERO, |literal| target.push(literal));
    let scaled = |derived: Derivation, negated: bool, scaled_value| {
        let mut cost = target.opcode(op::MUL);
        if negated {
            cost = cost.plus(target.opcode(op::ISZERO));
        }
        match derived {
            // c * t
            Derivation::FromZero => cost.plus(literal(scaled_value)),
            // e op c * k
            Derivation::Add(amount) => cost.plus(literal(amount)).plus(target.opcode(op::ADD)),
            Derivation::Or(amount) => cost.plus(literal(amount)).plus(target.opcode(op::OR)),
            Derivation::Shr(amount) => cost.plus(literal(amount)).plus(target.opcode(op::SHR)),
        }
    };
    match select.form {
        SelectForm::Same => Cost::ZERO,
        SelectForm::Scaled(derived) => scaled(derived, false, select.then_value),
        SelectForm::ScaledNegated(derived) => scaled(derived, true, select.else_value),
        // e + c * delta
        SelectForm::Ladder(delta) => {
            target.opcode(op::MUL).plus(target.push(delta)).plus(target.opcode(op::ADD))
        }
        // f + c * delta
        SelectForm::General(Some(delta)) => target
            .opcode(op::MUL)
            .plus(target.push(delta))
            .plus(target.opcode(op::ADD))
            .plus(literal(select.else_value)),
        // f + c * (t - f), with f used twice
        SelectForm::General(None) => {
            let reused = if func.value_u256(select.else_value).is_some() {
                literal(select.else_value).times(2)
            } else {
                target.dup()
            };
            target
                .opcode(op::SUB)
                .plus(literal(select.then_value))
                .plus(reused)
                .plus(target.opcode(op::MUL))
                .plus(target.opcode(op::ADD))
        }
    }
}

fn convert(func: &mut Function, site: &Site) {
    let block = site.block;
    // then_arm: t = f(...); jump join
    // else_arm: e = g(...); jump join
    // join: v = phi [then_arm: t], [else_arm: e]
    // =>
    // block: t = f(...); e = g(...); v = select cond, t, e; jump join
    let condition = boolean_condition(func, block, site.condition);
    for arm in [site.then_arm, site.else_arm].into_iter().flatten() {
        let moved = std::mem::take(&mut func.blocks[arm].instructions);
        func.blocks[block].instructions.extend(moved);
    }
    let then_pred = site.then_arm.unwrap_or(block);
    let else_pred = site.else_arm.unwrap_or(block);
    let mut replacements = FxHashMap::default();
    for select in &site.selects {
        let InstKind::Phi(incoming) = &func.inst(select.phi).kind else { continue };
        let remaining: Vec<_> = incoming
            .iter()
            .copied()
            .filter(|&(from, _)| from != then_pred && from != else_pred)
            .collect();
        let selected = select_value(func, block, condition, select);
        if remaining.is_empty() {
            // The join merged only the two converted edges: the phi is the select.
            if let Some(result) = func.inst_result_value(select.phi) {
                replacements.insert(result, selected);
            }
            func.blocks[site.join].instructions.retain(|&inst| inst != select.phi);
        } else {
            let mut incoming = remaining;
            incoming.push((block, selected));
            let InstKind::Phi(slot) = &mut func.inst_mut(select.phi).kind else { unreachable!() };
            *slot = incoming;
        }
    }
    if !replacements.is_empty() {
        func.replace_uses(&replacements);
    }
    let (_, metadata) = func.blocks[block].take_terminator();
    func.blocks[block].set_terminator(Terminator::Jump(site.join), metadata);
    // The arms are unreachable now; drop their edges and predecessor entries
    // so the join's phis and edge lists stay consistent until CFG cleanup
    // removes the blocks.
    for arm in [site.then_arm, site.else_arm].into_iter().flatten() {
        func.blocks[arm].set_generated_terminator(Terminator::Invalid);
        func.blocks[arm].predecessors.clear();
        func.blocks[site.join].predecessors.retain(|pred| *pred != arm);
    }
    if !func.blocks[site.join].predecessors.contains(&block) {
        func.blocks[site.join].predecessors.push(block);
    }
}

/// A zero-or-one form of the branch condition, appended to `block` when the
/// condition is not already boolean.
fn boolean_condition(func: &mut Function, block: BlockId, condition: ValueId) -> ValueId {
    if is_boolean(func, condition) {
        return condition;
    }
    // cond01 = iszero(iszero(cond))
    let zero = append(func, block, InstKind::IsZero(condition), Some(MirType::Bool));
    append(func, block, InstKind::IsZero(zero), Some(MirType::Bool))
}

fn is_boolean(func: &Function, value: ValueId) -> bool {
    if func.value_ty(value) == Some(MirType::Bool) {
        return true;
    }
    match func.value(value) {
        Value::Immediate(immediate) => {
            immediate.as_u256().is_some_and(|value| value <= U256::from(1))
        }
        Value::Inst(inst) => matches!(
            func.inst(*inst).kind,
            InstKind::Lt(..)
                | InstKind::Gt(..)
                | InstKind::SLt(..)
                | InstKind::SGt(..)
                | InstKind::Eq(..)
                | InstKind::IsZero(..)
        ),
        _ => false,
    }
}

/// Builds `cond ? then_value : else_value` at the end of `block` in the
/// select's arithmetic form.
fn select_value(
    func: &mut Function,
    block: BlockId,
    condition: ValueId,
    select: &Select,
) -> ValueId {
    let (then_value, else_value) = (select.then_value, select.else_value);
    let ty = func.value_ty(then_value).or_else(|| func.value_ty(else_value));
    match select.form {
        SelectForm::Same => then_value,
        SelectForm::Scaled(derived) => {
            scaled_select(func, block, condition, then_value, else_value, derived, ty)
        }
        // c ? t : t op k => t op (iszero c) * k
        SelectForm::ScaledNegated(derived) => {
            let negated = append(func, block, InstKind::IsZero(condition), Some(MirType::Bool));
            scaled_select(func, block, negated, else_value, then_value, derived, ty)
        }
        // c ? A : e => e + c * (A - base - k), for e = base + c2 * k
        SelectForm::Ladder(delta) => {
            let delta = literal(func, delta);
            let scaled =
                append(func, block, InstKind::Mul(condition, delta), Some(MirType::uint256()));
            append(func, block, InstKind::Add(else_value, scaled), ty)
        }
        // c ? t : f => f + c * (t - f)
        SelectForm::General(delta) => {
            let delta = match delta {
                Some(delta) => literal(func, delta),
                None => append(
                    func,
                    block,
                    InstKind::Sub(then_value, else_value),
                    Some(MirType::uint256()),
                ),
            };
            let scaled =
                append(func, block, InstKind::Mul(condition, delta), Some(MirType::uint256()));
            append(func, block, InstKind::Add(else_value, scaled), ty)
        }
    }
}

/// `then = base op k` for a base equal to `else`, or a zero else value.
#[derive(Clone, Copy)]
enum Derivation {
    Add(ValueId),
    Or(ValueId),
    Shr(ValueId),
    FromZero,
}

fn derivation(func: &Function, derived: ValueId, base: ValueId) -> Option<Derivation> {
    if func.value_u256(base) == Some(U256::ZERO) {
        return Some(Derivation::FromZero);
    }
    let Value::Inst(inst) = func.value(derived) else { return None };
    match func.inst(*inst).kind {
        InstKind::Add(a, b) if a == base => Some(Derivation::Add(b)),
        InstKind::Add(a, b) if b == base => Some(Derivation::Add(a)),
        InstKind::Or(a, b) if a == base => Some(Derivation::Or(b)),
        InstKind::Or(a, b) if b == base => Some(Derivation::Or(a)),
        InstKind::Shr(amount, value) if value == base => Some(Derivation::Shr(amount)),
        _ => None,
    }
}

/// `c ? base op k : base` as `base op (c * k)`, and `c ? t : 0` as `c * t`,
/// for a zero-or-one `c`.
fn scaled_select(
    func: &mut Function,
    block: BlockId,
    condition: ValueId,
    then_value: ValueId,
    else_value: ValueId,
    derived: Derivation,
    ty: Option<MirType>,
) -> ValueId {
    let scale = |func: &mut Function, amount| {
        append(func, block, InstKind::Mul(condition, amount), Some(MirType::uint256()))
    };
    match derived {
        // c ? t : 0 => c * t
        Derivation::FromZero => append(func, block, InstKind::Mul(condition, then_value), ty),
        // c ? e + k : e => e + c * k
        Derivation::Add(amount) => {
            let scaled = scale(func, amount);
            append(func, block, InstKind::Add(else_value, scaled), ty)
        }
        // c ? e | k : e => e | c * k
        Derivation::Or(amount) => {
            let scaled = scale(func, amount);
            append(func, block, InstKind::Or(else_value, scaled), ty)
        }
        // c ? e >> k : e => e >> c * k
        Derivation::Shr(amount) => {
            let scaled = scale(func, amount);
            append(func, block, InstKind::Shr(scaled, else_value), ty)
        }
    }
}

fn literal(func: &mut Function, value: U256) -> ValueId {
    func.alloc_value(Value::Immediate(Immediate::uint256(value)))
}

fn append(func: &mut Function, block: BlockId, kind: InstKind, ty: Option<MirType>) -> ValueId {
    let (inst, value) = func.alloc_value_inst(Instruction::new(kind, ty).with_debug_info_dropped());
    func.blocks[block].instructions.push(inst);
    value
}
