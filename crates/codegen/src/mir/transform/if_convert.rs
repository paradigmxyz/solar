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
//!   one value collapses one level at a time;
//! - a boolean merged with `true`, `c ? true : e` and `c ? t : true`, becomes `c | e` and `!c | t`,
//!   the shape a chain of `||` operands leaves at its last join;
//! - a boolean merged with `false`, `c ? t : false` or `c ? false : e`, becomes an `and`;
//! - a select whose arms agree, `x == C ? K : f(x)` with `f(C) == K` for an addition, subtraction
//!   or exclusive-or of `x` and a literal, becomes `f(x)`: checked Solidity spells a wrapping
//!   increment `x == type(uint256).max ? 0 : x + 1`, which is `x + 1`; and
//! - any other pair becomes `f + c * (t - f)`.
//!
//! Conditions are canonical booleans and cast to words before arithmetic.
//! Later simplification turns a power-of-two multiplier into a shift.
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
//! average path. The selects read the condition as a zero-or-one word, one
//! copy each, while the branch reads a word tested against zero directly, so
//! such a test pays for its comparison only after conversion. Runs after the
//! late CFG cleanup, so folded conditions never reach it, and before hot-leaf
//! inlining, so cloned lookup helpers arrive already branch-free. Converts a
//! sweep of sites with updated predecessor lists before running CFG cleanup,
//! avoiding a whole-function cleanup for each diamond.

use super::{cfg_simplify::simplify_function, egraph::is_bool_value};
use crate::{
    backend::evm::op,
    mir::{
        BlockId, EffectKind, Function, FunctionBuilder, Immediate, InstId, InstKind, MirType,
        Module, Terminator, Value, ValueId,
        analysis::{CfgInfo, integers::integer_mask},
        pass::{MirPass, run_function_pass},
        utils::{invalidate_unreachable_block, invalidate_unreachable_blocks, replace_terminator},
    },
    target::{Cost, Target},
};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashMap;
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
        run_function_pass(module, analyses, |func, analyses| {
            if_convert_function(func, &analyses.target(gcx))
        })
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
    /// `else = then op k`: `then op (c == 0) * k`, or `(c == 0) * else`.
    ScaledNegated(Derivation),
    /// A literal `then` over an `else` that already selects literals on a
    /// condition implied by this one: `else + c * delta`.
    Ladder(U256),
    /// `else + c * (then - else)`, with the difference folded when both are
    /// literals.
    General(Option<U256>),
    /// A boolean merged with `true`: `c | e`, or `!c | t` when `else` is the literal.
    Or { negated: bool },
    /// The value both edges compute: a literal on the edge taken when `x == C`, and on the
    /// other edge a word operation of `x` that gives that literal at `x = C`.
    Either(ValueId),
}

fn if_convert_function(func: &mut Function, target: &Target) -> bool {
    let cfg = CfgInfo::new(func);
    let mut changed = invalidate_unreachable_blocks(func, cfg.reachable()) != 0;
    loop {
        let mut converted = false;
        for block in func.blocks.indices() {
            if let Some(site) = find_site(func, target, block) {
                convert(func, &site);
                converted = true;
            }
        }
        if !converted {
            break;
        }
        simplify_function(func);
        changed = true;
    }
    changed
}

fn find_site(func: &Function, target: &Target, block: BlockId) -> Option<Site> {
    let body = &func.blocks[block];
    let Some(Terminator::Branch { condition, then_block, else_block }) = body.terminator else {
        return None;
    };
    if then_block == else_block || (block != BlockId::ENTRY && body.predecessors.is_empty()) {
        return None;
    }
    let then_join = arm_join(func, block, then_block);
    let else_join = arm_join(func, block, else_block);
    let (then_arm, else_arm, join) = match (then_join, else_join) {
        // then_arm -> join <- else_arm
        (Some(join), Some(other)) if join == other => (Some(then_block), Some(else_block), join),
        // then_arm -> join, block -> join
        (Some(join), _) if join == else_block => (Some(then_block), None, join),
        // block -> join, else_arm -> join
        (_, Some(join)) if join == then_block => (None, Some(else_block), join),
        _ => return None,
    };
    if join == block {
        return None;
    }
    let mut site = Site { block, condition, then_arm, else_arm, join, selects: Vec::new() };
    if let Some(selects) = join_selects(func, &site)
        && profitable(func, target, &site, &selects)
    {
        site.selects = selects;
        return Some(site);
    }
    None
}

/// The join an arm jumps to, when the arm is speculatable from `block`.
fn arm_join(func: &Function, block: BlockId, arm: BlockId) -> Option<BlockId> {
    if arm == block || func.blocks[arm].predecessors.as_slice() != [block] {
        return None;
    }
    let body = &func.blocks[arm];
    let Some(Terminator::Jump(join)) = body.terminator else { return None };
    if join == arm
        || join == block
        || body
            .instructions
            .iter()
            .filter(|&&inst| !matches!(func.inst(inst).kind, InstKind::Zext(_)))
            .count()
            > MAX_ARM_INSTRUCTIONS
    {
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
        if !matches!(func.value_ty(then_value), Some(MirType::Int(_)))
            || func.value_ty(then_value) != func.value_ty(else_value)
        {
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

fn select_form(
    func: &Function,
    condition: ValueId,
    then_value: ValueId,
    else_value: ValueId,
) -> SelectForm {
    if then_value == else_value {
        return SelectForm::Same;
    }
    if let Some(value) = agreeing_value(func, condition, then_value, else_value) {
        return SelectForm::Either(value);
    }
    // c ? true : e => c | e, c ? t : true => !c | t
    if func.value_ty(then_value) == Some(MirType::I1) {
        let is_true = |value| func.value_u256(value).is_some_and(|value| !value.is_zero());
        if is_true(then_value) || is_true(else_value) {
            return SelectForm::Or { negated: !is_true(then_value) };
        }
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
    let (a, b) = operands(func.value_inst_kind(value)?)?;
    if func.value_u256(a).is_some() || func.value_u256(b).is_none() {
        Some((a, b))
    } else {
        Some((b, a))
    }
}

/// Whether a nonzero `first` implies a nonzero `second`: the same value, or
/// bounds of one value by ordered literals (`x < a` implies `x < b` when
/// `a <= b`, and `x > a` implies `x > b` when `a >= b`).
fn implies(func: &Function, first: ValueId, mut second: ValueId) -> bool {
    if let Some(&InstKind::Zext(value)) = func.value_inst_kind(second) {
        second = value;
    }
    if first == second {
        return true;
    }
    match (func.value_inst_kind(first), func.value_inst_kind(second)) {
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
fn profitable(func: &Function, target: &Target, site: &Site, selects: &[Select]) -> bool {
    // Arm instructions whose results the scaled forms recompute are dropped.
    let replaced = selects
        .iter()
        .filter_map(|select| match select.form {
            SelectForm::Scaled(_) => Some(select.then_value),
            SelectForm::ScaledNegated(_) => Some(select.else_value),
            _ => None,
        })
        .collect::<Vec<_>>();
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
    // Selects that scale the same amount share one product of the condition.
    let mut products = Vec::new();
    let mut reads = 0;
    for select in selects {
        let product = scaled_amount(func, select);
        let shared = product.is_some_and(|product| products.contains(&product));
        if let Some(product) = product
            && !shared
        {
            products.push(product);
        }
        if !shared && !matches!(select.form, SelectForm::Same | SelectForm::Either(_)) {
            reads += 1;
        }
        selected = selected.plus(select_cost(func, target, select, shared));
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
    // Selects that read no condition leave its comparison dead.
    let condition =
        if reads == 0 { Cost::ZERO } else { condition_cost(func, target, site.condition, reads) };
    let after = kept_arms.plus(selected).plus(condition).times(2);
    target.cmp(after, before) != Ordering::Greater
}

/// Doubled per-execution cost of the transfers a site removes: the branch
/// and the join label on every path, and the arm jump with its label on one
/// of the two paths.
fn transfer_cost(target: &Target) -> Cost {
    target.branch().times(2).plus(target.jump())
}

/// Cost of one instruction: its operation and the pushes that materialize
/// its literal operands.
fn inst_cost(func: &Function, target: &Target, inst: InstId) -> Cost {
    let kind = &func.inst(inst).kind;
    let mut cost = target.op(&kind.op(), |value| func.value_u256(value));
    kind.visit_operands(|operand| {
        if let Some(literal) = func.value_u256(operand) {
            cost = cost.plus(target.push(literal));
        }
    });
    cost
}

/// Cost of the instructions a select adds: the pushes of its literal
/// operands, one duplicate of an operand it uses twice, and its operations.
/// A scaled select whose product an earlier select computes only combines it.
fn select_cost(func: &Function, target: &Target, select: &Select, shared: bool) -> Cost {
    let literal = |value| func.value_u256(value).map_or(Cost::ZERO, |literal| target.push(literal));
    let scaled = |derived: Derivation, negated: bool, scaled_value| {
        let mut product = target.opcode(op::MUL);
        if negated {
            product = product.plus(target.opcode(op::ISZERO));
        }
        let (amount, combine) = match derived {
            // c * t
            // NOTE: A boolean product is emitted as `and` but keeps this price, so it converts
            // exactly the sites it converted as a multiplication.
            Derivation::FromZero => (scaled_value, Cost::ZERO),
            // e op c * k
            Derivation::Add(amount) => (amount, target.opcode(op::ADD)),
            Derivation::Or(amount) => (amount, target.opcode(op::OR)),
            Derivation::Shr(amount) => (amount, target.opcode(op::SHR)),
        };
        if shared { combine } else { product.plus(literal(amount)).plus(combine) }
    };
    match select.form {
        SelectForm::Same | SelectForm::Either(_) => Cost::ZERO,
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
        // c | e, or !c | t
        SelectForm::Or { negated } => {
            let cost = target.opcode(op::OR);
            if negated { cost.plus(target.opcode(op::ISZERO)) } else { cost }
        }
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

/// Per-execution cost of the condition the selects read: a zero-or-one word,
/// copied for each read after the first. A nonzero test of a word, which the
/// branch read directly, or a word that is no boolean also pays for the
/// comparison that makes it one.
fn condition_cost(func: &Function, target: &Target, condition: ValueId, reads: u32) -> Cost {
    let copies = target.dup().times(reads.saturating_sub(1));
    let nonzero_test = func
        .value_inst_kind(condition)
        .is_some_and(|kind| kind.nonzero_test_operand(func).is_some());
    if nonzero_test || !is_bool_value(func, condition) {
        // ne word, 0: iszero iszero
        copies.plus(target.opcode(op::ISZERO).times(2))
    } else {
        copies
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
        // join: v = phi [kept edges], [block: select]
        let selected = select_value(func, block, condition, select);
        let InstKind::Phi(incoming) = &mut func.inst_mut(select.phi).kind else { continue };
        incoming.retain(|&(from, _)| from != then_pred && from != else_pred);
        if incoming.is_empty() {
            // The join merged only the two converted edges: the phi is the select.
            if let Some(result) = func.inst_result_value(select.phi) {
                replacements.insert(result, selected);
            }
            func.blocks[site.join].instructions.retain(|&inst| inst != select.phi);
        } else {
            incoming.push((block, selected));
        }
    }
    func.replace_uses(&replacements);
    // block: branch cond, then_arm, else_arm -> jump join
    replace_terminator(func, block, Terminator::Jump(site.join));
    // The arms are unreachable now; drop their edges and predecessor entries
    // so the join's phis and edge lists stay consistent until CFG cleanup
    // removes the blocks.
    for arm in [site.then_arm, site.else_arm].into_iter().flatten() {
        let _ = invalidate_unreachable_block(func, arm);
    }
}

/// A zero-or-one form of the branch condition, appended to `block` when the
/// condition is not already boolean.
fn boolean_condition(func: &mut Function, block: BlockId, condition: ValueId) -> ValueId {
    if is_bool_value(func, condition) {
        return condition;
    }
    // cond01 = ne cond, 0
    let zero = literal(func, U256::ZERO);
    append(func, block, InstKind::Ne(condition, zero), Some(MirType::I1))
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
        // x == C ? f(C) : f(x) => f(x)
        SelectForm::Either(value) => value,
        SelectForm::Scaled(derived) => {
            scaled_select(func, block, condition, then_value, else_value, derived, ty)
        }
        // c ? t : t op k => t op (c == 0) * k
        SelectForm::ScaledNegated(derived) => {
            let zero = literal(func, U256::ZERO);
            let negated = append(func, block, InstKind::Eq(condition, zero), Some(MirType::I1));
            scaled_select(func, block, negated, else_value, then_value, derived, ty)
        }
        // c ? A : e => e + c * (A - base - k), for e = base + c2 * k
        SelectForm::Ladder(delta) => {
            let delta = literal(func, delta);
            let scaled = append(func, block, InstKind::Mul(condition, delta), Some(MirType::I256));
            append(func, block, InstKind::Add(else_value, scaled), ty)
        }
        // c ? true : e => c | e
        SelectForm::Or { negated: false } => {
            append(func, block, InstKind::Or(condition, else_value), ty)
        }
        // c ? t : true => (c == 0) | t
        SelectForm::Or { negated: true } => {
            let zero = literal(func, U256::ZERO);
            let negated = append(func, block, InstKind::Eq(condition, zero), Some(MirType::I1));
            append(func, block, InstKind::Or(negated, then_value), ty)
        }
        // c ? t : f => f + c * (t - f)
        SelectForm::General(delta) => {
            let delta = match delta {
                Some(delta) => literal(func, delta),
                None => {
                    append(func, block, InstKind::Sub(then_value, else_value), Some(MirType::I256))
                }
            };
            let scaled = append(func, block, InstKind::Mul(condition, delta), Some(MirType::I256));
            append(func, block, InstKind::Add(else_value, scaled), ty)
        }
    }
}

/// The value a select gives on both edges: either arm when both add the same amount to one
/// value at the select's width, as `x > max - k ? x - (max - k + 1) : x + k` does, whatever the
/// condition; or, for `x == C ? K : f(x)` or `x != C ? f(x) : K`, `f(x)`, where `f` adds,
/// subtracts or exclusive-ors a literal and `f(C)` is `K` at the select's width. The condition
/// may be such a comparison or its negation.
fn agreeing_value(
    func: &Function,
    condition: ValueId,
    then_value: ValueId,
    else_value: ValueId,
) -> Option<ValueId> {
    if let Some(MirType::Int(bits)) = func.value_ty(else_value)
        && let (Some((a, then_amount)), Some((b, else_amount))) =
            (added_literal(func, then_value), added_literal(func, else_value))
        && a == b
        && then_amount.wrapping_sub(else_amount) & integer_mask(bits.get()) == U256::ZERO
    {
        return Some(else_value);
    }
    let (x, c, equal) = literal_comparison(func, condition, MAX_NEGATIONS)?;
    // The value on the edge taken when `x == c`, and the other edge's.
    let (at_c, other) = if equal { (then_value, else_value) } else { (else_value, then_value) };
    let literal = func.value_u256(at_c)?;
    let Some(MirType::Int(bits)) = func.value_ty(other) else { return None };
    let operand = |value: ValueId| if value == x { Some(c) } else { func.value_u256(value) };
    let result = match *func.value_inst_kind(other)? {
        InstKind::Add(a, b) if a == x || b == x => operand(a)?.wrapping_add(operand(b)?),
        InstKind::Sub(a, b) if a == x || b == x => operand(a)?.wrapping_sub(operand(b)?),
        InstKind::Xor(a, b) if a == x || b == x => operand(a)? ^ operand(b)?,
        _ => return None,
    };
    (result & integer_mask(bits.get()) == literal).then_some(other)
}

/// `(x, k)` for a value `x + k` or `x - (-k)`, with `k` a literal and `x` not.
fn added_literal(func: &Function, value: ValueId) -> Option<(ValueId, U256)> {
    match *func.value_inst_kind(value)? {
        InstKind::Add(a, b) => match (func.value_u256(a), func.value_u256(b)) {
            (None, Some(k)) => Some((a, k)),
            (Some(k), None) => Some((b, k)),
            _ => None,
        },
        InstKind::Sub(a, b) if func.value_u256(a).is_none() => {
            Some((a, U256::ZERO.wrapping_sub(func.value_u256(b)?)))
        }
        _ => None,
    }
}

/// Negations of a comparison a branch condition may wrap.
const MAX_NEGATIONS: u32 = 2;

/// `(x, C, true)` for a condition `x == C`, and `(x, C, false)` for `x != C`, with `C` a
/// literal and `x` not, through `eq(b, 0)` negations of a boolean `b`.
fn literal_comparison(
    func: &Function,
    condition: ValueId,
    negations: u32,
) -> Option<(ValueId, U256, bool)> {
    let (a, b, equal) = match *func.value_inst_kind(condition)? {
        InstKind::Eq(a, b) => (a, b, true),
        InstKind::Ne(a, b) => (a, b, false),
        _ => return None,
    };
    let (x, c) = match (func.value_u256(a), func.value_u256(b)) {
        (None, Some(c)) => (a, c),
        (Some(c), None) => (b, c),
        _ => return None,
    };
    // eq(b, 0) of a boolean comparison b negates it.
    if c.is_zero()
        && negations != 0
        && is_bool_value(func, x)
        && let Some((inner, literal, inner_equal)) = literal_comparison(func, x, negations - 1)
    {
        return Some((inner, literal, inner_equal != equal));
    }
    Some((x, c, equal))
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
    match *func.value_inst_kind(derived)? {
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
        append(func, block, InstKind::Mul(condition, amount), Some(MirType::I256))
    };
    match derived {
        // c ? t : false => c & t
        Derivation::FromZero if ty == Some(MirType::I1) => {
            append(func, block, InstKind::And(condition, then_value), ty)
        }
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
    func.alloc_value(Value::Immediate(Immediate::I256(value)))
}

fn append(func: &mut Function, block: BlockId, kind: InstKind, ty: Option<MirType>) -> ValueId {
    // operands = zext i1 boolean_operands to i256
    // result = op operands
    // typed_result = cast result
    let start = func.blocks[block].instructions.len();
    let mut builder = FunctionBuilder::new(func);
    builder.switch_to_block(block);
    let operation_ty = kind.op_def().result.default_type().or(ty);
    let value = builder.emit_inst(kind, operation_ty);
    let value = ty.map_or(value, |ty| builder.cast(value, ty));
    let insts = builder.func().blocks[block].instructions[start..].to_vec();
    for inst in insts {
        builder.func_mut().inst_mut(inst).metadata.mark_debug_info_dropped();
    }
    value
}

/// The negation and amount of the condition product a scaled select adds, by
/// literal value when the amount is one: selects with the same key share it.
fn scaled_amount(func: &Function, select: &Select) -> Option<(bool, Result<U256, ValueId>)> {
    let (negated, derived) = match select.form {
        SelectForm::Scaled(derived) => (false, derived),
        SelectForm::ScaledNegated(derived) => (true, derived),
        _ => return None,
    };
    match derived {
        Derivation::Add(amount) | Derivation::Or(amount) | Derivation::Shr(amount) => {
            Some((negated, func.value_u256(amount).ok_or(amount)))
        }
        Derivation::FromZero => None,
    }
}
