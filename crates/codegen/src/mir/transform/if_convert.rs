//! If-conversion of small branch diamonds and triangles into selects.
//!
//! A branch whose arms only compute a few pure values and merge them in a
//! join block pays a `JUMPI`, a `JUMP` on one side, a `JUMPDEST`, and a stack
//! layout reconciliation at the join on every execution. When the arms are
//! cheap enough to run unconditionally, the pass moves their instructions
//! into the branching block, replaces each join phi that merges the arm
//! values by a `select` on the branch condition, and turns the branch into a
//! jump. A diamond has two single-predecessor arms that jump to the same
//! join; a triangle has one such arm while the other edge reaches the join
//! directly.
//!
//! Only joins whose every phi merges a value with one derived from it by an
//! add, or, or shift, or with zero, are converted, because a boolean
//! condition scales the amount: `c ? a + k : a` becomes `a + c * k`,
//! `c ? a | k : a` becomes `a | c * k`, `c ? a >> k : a` becomes
//! `a >> c * k`, and `c ? t : 0` becomes `c * t`. Those are the branch-free
//! shapes hand-written bit-search assembly uses, and later simplification
//! turns a power-of-two multiplier into a shift. A non-boolean condition is
//! normalized with two `iszero`s first, since the scaled forms need a
//! zero-or-one condition. Unrelated arm values would need a general `select`,
//! whose `f + c * (t - f)` lowering duplicates both operands and, measured on
//! the runtime corpus, costs more gas and bytes than the branch it replaces;
//! those joins keep their branches.
//!
//! Safety: an arm qualifies only when the branching block is its sole
//! predecessor, its terminator is a jump to the join, and every instruction
//! is a pure computation (no memory or state reads, calls, or effects), so
//! speculating it cannot trap, expand memory, or change observable state.
//! Profitability: at most three instructions per arm and at most two phis
//! at the join, so the speculated work and the selects stay comparable to
//! the removed transfer and join costs. Runs after the late CFG cleanup, so
//! folded conditions never reach it, and before hot-leaf inlining, so cloned
//! lookup helpers arrive already branch-free.

use super::cfg_simplify::simplify_function;
use crate::mir::{
    BlockId, EffectKind, Function, InstId, InstKind, Instruction, MirType, Module, Terminator,
    Value, ValueId,
    pass::{MirPass, run_function_pass},
};
use alloy_primitives::U256;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::{IndexVec, index_vec},
    map::FxHashMap,
};

/// Function pass that converts small branch diamonds and triangles into selects.
pub(crate) struct IfConvert;

impl MirPass for IfConvert {
    fn name(&self) -> &'static str {
        "if-convert"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, _| if_convert_function(func))
    }
}

/// Instructions an arm may hold; each is executed unconditionally afterwards.
const MAX_ARM_INSTRUCTIONS: usize = 3;
/// Phis a join may merge; each becomes a scaled arithmetic form.
const MAX_SELECTS: usize = 2;

/// One convertible branch: the branching block, its condition, the arms that
/// hold speculatable instructions, and the join their values reach.
struct Site {
    block: BlockId,
    condition: ValueId,
    /// The arm reached when the condition holds, when it is a separate block.
    then_arm: Option<BlockId>,
    /// The arm reached when the condition fails, when it is a separate block.
    else_arm: Option<BlockId>,
    join: BlockId,
}

fn if_convert_function(func: &mut Function) -> bool {
    let mut changed = false;
    while let Some(site) = find_site(func) {
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

fn find_site(func: &Function) -> Option<Site> {
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
        let site = match (then_join, else_join) {
            // then_arm -> join <- else_arm
            (Some(join), Some(other)) if join == other => {
                Site { block, condition, then_arm: Some(then_block), else_arm: None, join }
                    .with_else(else_block)
            }
            // then_arm -> join, block -> join
            (Some(join), _) if join == else_block => {
                Site { block, condition, then_arm: Some(then_block), else_arm: None, join }
            }
            // block -> join, else_arm -> join
            (_, Some(join)) if join == then_block => {
                Site { block, condition, then_arm: None, else_arm: Some(else_block), join }
            }
            _ => continue,
        };
        if site.join == block || !join_fits(func, &site) {
            continue;
        }
        return Some(site);
    }
    None
}

impl Site {
    fn with_else(mut self, else_arm: BlockId) -> Self {
        self.else_arm = Some(else_arm);
        self
    }
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

/// Whether the join's phis are few enough and each merges a value with one
/// derived from it, so every phi has a scaled arithmetic form.
fn join_fits(func: &Function, site: &Site) -> bool {
    let then_pred = site.then_arm.unwrap_or(site.block);
    let else_pred = site.else_arm.unwrap_or(site.block);
    let mut phis = 0;
    for &inst in &func.blocks[site.join].instructions {
        let InstKind::Phi(incoming) = &func.inst(inst).kind else { continue };
        phis += 1;
        let incoming_from =
            |pred| incoming.iter().find(|&&(from, _)| from == pred).map(|&(_, v)| v);
        let (Some(then_value), Some(else_value)) =
            (incoming_from(then_pred), incoming_from(else_pred))
        else {
            return false;
        };
        if then_value != else_value
            && derivation(func, then_value, else_value).is_none()
            && derivation(func, else_value, then_value).is_none()
        {
            return false;
        }
    }
    phis <= MAX_SELECTS
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
    let phis: Vec<InstId> = func.blocks[site.join]
        .instructions
        .iter()
        .copied()
        .filter(|&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)))
        .collect();
    let mut replacements = FxHashMap::default();
    for phi in phis {
        let InstKind::Phi(incoming) = &func.inst(phi).kind else { continue };
        let incoming_from =
            |pred| incoming.iter().find(|&&(from, _)| from == pred).map(|&(_, v)| v);
        let (Some(then_value), Some(else_value)) =
            (incoming_from(then_pred), incoming_from(else_pred))
        else {
            continue;
        };
        let remaining: Vec<_> = incoming
            .iter()
            .copied()
            .filter(|&(from, _)| from != then_pred && from != else_pred)
            .collect();
        let selected = select_value(func, block, condition, then_value, else_value);
        if remaining.is_empty() {
            // The join merged only the two converted edges: the phi is the select.
            if let Some(result) = func.inst_result_value(phi) {
                replacements.insert(result, selected);
            }
            func.blocks[site.join].instructions.retain(|&inst| inst != phi);
        } else {
            let mut incoming = remaining;
            incoming.push((block, selected));
            let InstKind::Phi(slot) = &mut func.inst_mut(phi).kind else { unreachable!() };
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

/// Builds `cond ? then_value : else_value` at the end of `block`, in the
/// scaled arithmetic form when one arm derives its value from the other.
fn select_value(
    func: &mut Function,
    block: BlockId,
    condition: ValueId,
    then_value: ValueId,
    else_value: ValueId,
) -> ValueId {
    if then_value == else_value {
        return then_value;
    }
    let ty = func.value_ty(then_value).or_else(|| func.value_ty(else_value));
    if let Some(value) = scaled_select(func, block, condition, then_value, else_value, ty) {
        return value;
    }
    // c ? t : t op k => t op (iszero c) * k
    let negated = append(func, block, InstKind::IsZero(condition), Some(MirType::Bool));
    scaled_select(func, block, negated, else_value, then_value, ty)
        .expect("join_fits admits only phis with a scaled form")
}

/// `then_value = base op k` for a base equal to `else_value`, or a zero
/// else value.
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
    ty: Option<MirType>,
) -> Option<ValueId> {
    let scale = |func: &mut Function, amount| {
        append(func, block, InstKind::Mul(condition, amount), Some(MirType::uint256()))
    };
    Some(match derivation(func, then_value, else_value)? {
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
    })
}

fn append(func: &mut Function, block: BlockId, kind: InstKind, ty: Option<MirType>) -> ValueId {
    let (inst, value) = func.alloc_value_inst(Instruction::new(kind, ty).with_debug_info_dropped());
    func.blocks[block].instructions.push(inst);
    value
}
