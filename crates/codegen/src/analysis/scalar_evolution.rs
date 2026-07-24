//! Scalar-evolution-style affine analysis for MIR loops.
//!
//! This is intentionally small: it recognizes expressions of the form
//! `base + c + sum(iv * scale)` inside one natural loop. Optimization passes can use this to avoid
//! ad hoc pattern matching when reasoning about memory/storage addresses derived from loop indices.
//!
//! Analysis contract:
//! - loop-invariant values are represented as a single optional base
//! - constants and induction scales use checked signed arithmetic
//! - unrecognized or overflowing expressions are omitted instead of guessed

use crate::{
    analysis::Loop,
    mir::{Function, InstKind, Value, ValueId},
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::map::FxHashMap;

/// One affine induction-variable term.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AffineTerm {
    /// Loop induction variable.
    pub value: ValueId,
    /// Signed scale applied to the induction variable.
    pub scale: i128,
}

/// An affine expression in one loop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AffineExpr {
    /// Optional loop-invariant base value.
    pub base: Option<ValueId>,
    /// Signed constant offset.
    pub(crate) constant: i128,
    /// Induction-variable terms.
    pub terms: SmallVec<[AffineTerm; 2]>,
}

impl AffineExpr {
    fn constant(constant: i128) -> Self {
        Self { base: None, constant, terms: SmallVec::new() }
    }

    fn base(base: ValueId) -> Self {
        Self { base: Some(base), constant: 0, terms: SmallVec::new() }
    }

    fn induction(value: ValueId) -> Self {
        Self { base: None, constant: 0, terms: smallvec::smallvec![AffineTerm { value, scale: 1 }] }
    }

    fn add(mut self, other: Self) -> Option<Self> {
        self.constant = self.constant.checked_add(other.constant)?;
        self.base = match (self.base, other.base) {
            (None, base) | (base, None) => base,
            (Some(_), Some(_)) => return None,
        };
        for term in other.terms {
            self.add_term(term.value, term.scale)?;
        }
        Some(self)
    }

    fn sub(mut self, other: Self) -> Option<Self> {
        self.constant = self.constant.checked_sub(other.constant)?;
        if other.base.is_some() {
            return None;
        }
        for term in other.terms {
            self.add_term(term.value, term.scale.checked_neg()?)?;
        }
        Some(self)
    }

    fn mul_const(mut self, scale: i128) -> Option<Self> {
        if self.base.is_some() && scale != 1 {
            return None;
        }
        self.constant = self.constant.checked_mul(scale)?;
        for term in &mut self.terms {
            term.scale = term.scale.checked_mul(scale)?;
        }
        Some(self)
    }

    fn add_term(&mut self, value: ValueId, scale: i128) -> Option<()> {
        if scale == 0 {
            return Some(());
        }
        if let Some(term) = self.terms.iter_mut().find(|term| term.value == value) {
            term.scale = term.scale.checked_add(scale)?;
            if term.scale == 0 {
                self.terms.retain(|term| term.value != value);
            }
        } else {
            self.terms.push(AffineTerm { value, scale });
        }
        Some(())
    }
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
        for value in func.value_ids() {
            let _ = analysis.affine_expr(func, loop_data, value);
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
            Value::Arg { .. } => AffineExpr::base(value),
            Value::Undef(_) | Value::Error(_) => return None,
            Value::Inst(_) if !value_defined_in_loop(func, value, loop_data) => {
                AffineExpr::base(value)
            }
            Value::Inst(inst_id) => {
                if loop_data.induction_vars.iter().any(|iv| iv.value == value) {
                    AffineExpr::induction(value)
                } else {
                    match func.instruction(*inst_id).kind {
                        InstKind::Add(a, b) => {
                            let a = self.affine_expr(func, loop_data, a)?;
                            let b = self.affine_expr(func, loop_data, b)?;
                            a.add(b)?
                        }
                        InstKind::Sub(a, b) => {
                            let a = self.affine_expr(func, loop_data, a)?;
                            let b = self.affine_expr(func, loop_data, b)?;
                            a.sub(b)?
                        }
                        InstKind::Mul(a, b) => {
                            let a_expr = self.affine_expr(func, loop_data, a);
                            let b_expr = self.affine_expr(func, loop_data, b);
                            match (a_expr, b_expr) {
                                (Some(expr), Some(scale))
                                    if scale.base.is_none() && scale.terms.is_empty() =>
                                {
                                    expr.mul_const(scale.constant)?
                                }
                                (Some(scale), Some(expr))
                                    if scale.base.is_none() && scale.terms.is_empty() =>
                                {
                                    expr.mul_const(scale.constant)?
                                }
                                _ => return None,
                            }
                        }
                        InstKind::Shl(shift, value) => {
                            let shift = self.affine_expr(func, loop_data, shift)?;
                            if shift.base.is_some() || !shift.terms.is_empty() {
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
            .any(|block_id| func.block(block_id).instructions.contains(inst_id)),
        Value::Undef(_) | Value::Error(_) => true,
        Value::Arg { .. } | Value::Immediate(_) => false,
    }
}

fn u256_to_i128(value: U256) -> Option<i128> {
    if value <= U256::from(i128::MAX as u128) { Some(value.to::<u128>() as i128) } else { None }
}
