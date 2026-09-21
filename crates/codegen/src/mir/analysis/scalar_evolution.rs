//! Scalar-evolution-style affine analysis for MIR loops.
//!
//! This is intentionally small: it recognizes expressions of the form
//! `base + sum(invariant * scale) + c + sum(iv * scale)` inside one natural loop. Optimization
//! passes can use this to avoid ad hoc pattern matching when reasoning about memory/storage
//! addresses derived from loop indices.
//!
//! Analysis contract:
//! - one loop-invariant value is the optional unscaled base; further invariant values, and the base
//!   once it is scaled, are kept as scaled invariant terms
//! - constants and scales use checked signed arithmetic, and scales may be negative
//! - unrecognized or overflowing expressions are omitted instead of guessed
//! - checked unsigned word expressions describe successful results, not removable checks.

use crate::mir::{ArithmeticKind, CheckedOp, Function, InstKind, Value, ValueId, analysis::Loop};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::map::FxHashMap;

/// One affine induction-variable term.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct AffineTerm {
    /// Loop induction variable.
    pub value: ValueId,
    /// Signed scale applied to the induction variable.
    pub scale: i128,
}

/// An affine expression in one loop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AffineExpr {
    /// Optional unscaled loop-invariant base value.
    pub base: Option<ValueId>,
    /// Scaled loop-invariant terms: a second invariant, or the base once it is scaled.
    pub invariants: SmallVec<[AffineTerm; 2]>,
    /// Signed constant offset.
    pub(crate) constant: i128,
    /// Induction-variable terms.
    pub terms: SmallVec<[AffineTerm; 2]>,
}

impl AffineExpr {
    fn constant(constant: i128) -> Self {
        Self { base: None, invariants: SmallVec::new(), constant, terms: SmallVec::new() }
    }

    fn base(base: ValueId) -> Self {
        Self { base: Some(base), invariants: SmallVec::new(), constant: 0, terms: SmallVec::new() }
    }

    fn induction(value: ValueId) -> Self {
        Self {
            base: None,
            invariants: SmallVec::new(),
            constant: 0,
            terms: smallvec::smallvec![AffineTerm { value, scale: 1 }],
        }
    }

    fn add(mut self, other: Self) -> Option<Self> {
        self.constant = self.constant.checked_add(other.constant)?;
        match (self.base, other.base) {
            (None, base) => self.base = base,
            (Some(_), None) => {}
            (Some(_), Some(other_base)) => add_term(&mut self.invariants, other_base, 1)?,
        }
        for term in other.invariants {
            add_term(&mut self.invariants, term.value, term.scale)?;
        }
        for term in other.terms {
            add_term(&mut self.terms, term.value, term.scale)?;
        }
        Some(self)
    }

    fn sub(mut self, other: Self) -> Option<Self> {
        self.constant = self.constant.checked_sub(other.constant)?;
        if let Some(other_base) = other.base {
            add_term(&mut self.invariants, other_base, -1)?;
        }
        for term in other.invariants {
            add_term(&mut self.invariants, term.value, term.scale.checked_neg()?)?;
        }
        for term in other.terms {
            add_term(&mut self.terms, term.value, term.scale.checked_neg()?)?;
        }
        Some(self)
    }

    fn mul_const(mut self, scale: i128) -> Option<Self> {
        if scale == 1 {
            return Some(self);
        }
        // A scaled base is an invariant term like any other; it joins the list
        // unscaled and is scaled with the rest below.
        if let Some(base) = self.base.take() {
            add_term(&mut self.invariants, base, 1)?;
        }
        self.constant = self.constant.checked_mul(scale)?;
        for term in &mut self.invariants {
            term.scale = term.scale.checked_mul(scale)?;
        }
        for term in &mut self.terms {
            term.scale = term.scale.checked_mul(scale)?;
        }
        Some(self)
    }
}

impl AffineExpr {
    /// Whether the expression is a plain constant.
    fn is_constant(&self) -> bool {
        self.base.is_none() && self.invariants.is_empty() && self.terms.is_empty()
    }
}

/// Adds `scale` copies of `value` to a term list, merging with an existing term.
fn add_term(terms: &mut SmallVec<[AffineTerm; 2]>, value: ValueId, scale: i128) -> Option<()> {
    if scale == 0 {
        return Some(());
    }
    if let Some(term) = terms.iter_mut().find(|term| term.value == value) {
        term.scale = term.scale.checked_add(scale)?;
        if term.scale == 0 {
            terms.retain(|term| term.value != value);
        }
    } else {
        terms.push(AffineTerm { value, scale });
    }
    Some(())
}

/// Affine expressions recognized for one loop.
#[derive(Clone, Debug, Default)]
pub(crate) struct ScalarEvolution {
    expressions: FxHashMap<ValueId, AffineExpr>,
}

impl ScalarEvolution {
    /// Computes affine expressions for values used by `loop_data`.
    #[must_use]
    pub(crate) fn analyze(func: &Function, loop_data: &Loop) -> Self {
        let mut analysis = Self::default();
        for block_id in &loop_data.blocks {
            let block = &func.blocks[block_id];
            for &inst_id in &block.instructions {
                let inst = func.inst(inst_id);
                for operand in inst.kind.operands() {
                    let _ = analysis.affine_expr(func, loop_data, operand);
                }
                if let Some(result) = func.inst_result_value(inst_id) {
                    let _ = analysis.affine_expr(func, loop_data, result);
                }
            }
            if let Some(terminator) = &block.terminator {
                for operand in terminator.operands() {
                    let _ = analysis.affine_expr(func, loop_data, operand);
                }
            }
        }
        analysis
    }

    /// Returns the affine expression for a value, if recognized.
    #[must_use]
    pub(crate) fn get(&self, value: ValueId) -> Option<&AffineExpr> {
        self.expressions.get(&value)
    }

    fn affine_expr(
        &mut self,
        func: &Function,
        loop_data: &Loop,
        value: ValueId,
    ) -> Option<AffineExpr> {
        if let Some(expr) = self.expressions.get(&value) {
            return Some(expr.clone());
        }

        let expr = match func.value(value) {
            Value::Immediate(imm) => AffineExpr::constant(u256_to_i128(imm.as_u256()?)?),
            Value::Arg(_) => AffineExpr::base(value),
            Value::Undef(_) | Value::Error(_) => return None,
            Value::Inst(_) if !value_defined_in_loop(func, value, loop_data) => {
                AffineExpr::base(value)
            }
            Value::Inst(inst_id) => {
                if let Some(iv) = loop_data.induction_vars.iter().find(|iv| iv.value == value) {
                    if super::integers::integer_bits(func, value) != 256
                        && !matches!(func.inst(iv.update_inst).kind, InstKind::CheckedBinary { .. })
                    {
                        let start = func.value_u256(iv.init)?;
                        let travel = func
                            .value_u256(iv.step)?
                            .checked_mul(U256::from(loop_data.trip_count?))?;
                        let end = if iv.descending {
                            start.checked_sub(travel)?
                        } else {
                            start.checked_add(travel)?
                        };
                        if end > super::integers::integer_max(func, value) {
                            return None;
                        }
                    }
                    AffineExpr::induction(value)
                } else {
                    let known = |value| {
                        let iv = loop_data.induction_vars.iter().find(|iv| iv.value == value)?;
                        let start = func.value_u256(iv.init)?;
                        let travel = func
                            .value_u256(iv.step)?
                            .checked_mul(U256::from(loop_data.trip_count?))?;
                        if iv.descending {
                            Some((start.checked_sub(travel)?, start))
                        } else {
                            Some((start, start.checked_add(travel)?))
                        }
                    };
                    if super::integers::integer_bits(func, value) != 256
                        && matches!(
                            func.inst(*inst_id).kind,
                            InstKind::Add(..)
                                | InstKind::Sub(..)
                                | InstKind::Mul(..)
                                | InstKind::Shl(..)
                        )
                        && !super::integers::no_wrap_with(func, value, &known)
                    {
                        return None;
                    }
                    match func.inst(*inst_id).kind {
                        InstKind::Zext(source) => self.affine_expr(func, loop_data, source)?,
                        InstKind::Add(a, b)
                        | InstKind::CheckedBinary {
                            op: CheckedOp::Add,
                            arithmetic: ArithmeticKind::Unsigned(_),
                            lhs: a,
                            rhs: b,
                        } => {
                            let a = self.affine_expr(func, loop_data, a)?;
                            let b = self.affine_expr(func, loop_data, b)?;
                            a.add(b)?
                        }
                        InstKind::Sub(a, b)
                        | InstKind::CheckedBinary {
                            op: CheckedOp::Sub,
                            arithmetic: ArithmeticKind::Unsigned(_),
                            lhs: a,
                            rhs: b,
                        } => {
                            let a = self.affine_expr(func, loop_data, a)?;
                            let b = self.affine_expr(func, loop_data, b)?;
                            a.sub(b)?
                        }
                        InstKind::Mul(a, b)
                        | InstKind::CheckedBinary {
                            op: CheckedOp::Mul,
                            arithmetic: ArithmeticKind::Unsigned(_),
                            lhs: a,
                            rhs: b,
                        } => {
                            let a_expr = self.affine_expr(func, loop_data, a);
                            let b_expr = self.affine_expr(func, loop_data, b);
                            match (a_expr, b_expr) {
                                (Some(expr), Some(scale)) if scale.is_constant() => {
                                    expr.mul_const(scale.constant)?
                                }
                                (Some(scale), Some(expr)) if scale.is_constant() => {
                                    expr.mul_const(scale.constant)?
                                }
                                _ => return None,
                            }
                        }
                        InstKind::Shl(shift, value) => {
                            let shift = self.affine_expr(func, loop_data, shift)?;
                            if !shift.is_constant() {
                                return None;
                            }
                            let shift = u32::try_from(shift.constant).ok()?;
                            let scale = 1i128.checked_shl(shift)?;
                            self.affine_expr(func, loop_data, value)?.mul_const(scale)?
                        }
                        _ => return None,
                    }
                }
            }
        };

        self.expressions.insert(value, expr.clone());
        Some(expr)
    }
}

fn value_defined_in_loop(func: &Function, value: ValueId, loop_data: &Loop) -> bool {
    match func.value(value) {
        Value::Inst(inst_id) => loop_data
            .blocks
            .iter()
            .any(|block_id| func.blocks[block_id].instructions.contains(inst_id)),
        Value::Undef(_) | Value::Error(_) => true,
        Value::Arg(_) | Value::Immediate(_) => false,
    }
}

fn u256_to_i128(value: U256) -> Option<i128> {
    if value <= U256::from(i128::MAX as u128) { Some(value.to::<u128>() as i128) } else { None }
}
