//! Dimensional-analysis domain: units and decimal scales.
//!
//! A value's dimension is a product of base units with integer exponents, such as
//! `tok/share`, together with a decimal scale such as `D18`. The algebra follows the
//! annotation language of Trail of Bits' dimensional-analysis plugin: multiplication adds
//! exponents and scales, division subtracts them, and addition, subtraction, and comparison
//! require equal dimensions and scales. Powers of ten are pure scales, other constants are
//! dimensionless, and unannotated values are unknown so they never produce findings.
//!
//! Seeds come from NatSpec: `@param amount D18{tok}` annotates a parameter and
//! `@return D18{share}` a result. A mismatch is reported where it happens; the result becomes
//! unknown so one error is not reported again downstream.

use super::{
    lattice::JoinSemiLattice,
    value::{DomainCx, Seeded, ValueDomain},
};
use crate::mir::{CheckedOp, Op};
use alloy_primitives::U256;
use std::{collections::BTreeMap, fmt};

/// A product of base units with nonzero exponents and a decimal scale.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Dimension {
    /// Exponent of each base unit.
    pub(crate) units: BTreeMap<String, i32>,
    /// Power of ten the stored integer is scaled by.
    pub(crate) scale: i32,
}

impl Dimension {
    fn dimensionless(scale: i32) -> Self {
        Self { units: BTreeMap::new(), scale }
    }

    fn combine(&self, other: &Self, sign: i32) -> Self {
        let mut units = self.units.clone();
        for (unit, &exponent) in &other.units {
            let entry = units.entry(unit.clone()).or_insert(0);
            *entry += sign * exponent;
            if *entry == 0 {
                units.remove(unit);
            }
        }
        Self { units, scale: self.scale + sign * other.scale }
    }

    /// Parses `D18{tok/share}`, `{tok}`, or `D4{1}`.
    fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let open = text.find('{')?;
        let close = text.rfind('}')?;
        let scale = match &text[..open] {
            "" => 0,
            prefix => prefix.strip_prefix('D')?.parse().ok()?,
        };
        let mut dimension = Self::dimensionless(scale);
        let mut sign = 1;
        let mut atom = String::new();
        let flush = |dimension: &mut Self, atom: &mut String, sign: i32| {
            let name = std::mem::take(atom);
            let name = name.trim();
            if !name.is_empty() && name != "1" {
                let unit = Self { units: [(name.to_owned(), 1)].into(), scale: 0 };
                *dimension = dimension.combine(&unit, sign);
            }
        };
        for character in text[open + 1..close].chars() {
            match character {
                '*' | '/' => {
                    flush(&mut dimension, &mut atom, sign);
                    sign = if character == '/' { -1 } else { 1 };
                }
                character => atom.push(character),
            }
        }
        flush(&mut dimension, &mut atom, sign);
        Some(dimension)
    }
}

impl fmt::Display for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "D{}{{", self.scale)?;
        let numerator = self.units.iter().filter(|&(_, &e)| e > 0).collect::<Vec<_>>();
        let denominator = self.units.iter().filter(|&(_, &e)| e < 0).collect::<Vec<_>>();
        let write_part = |f: &mut fmt::Formatter<'_>, part: &[(&String, &i32)]| -> fmt::Result {
            for (i, (unit, exponent)) in part.iter().enumerate() {
                if i != 0 {
                    f.write_str("*")?;
                }
                f.write_str(unit)?;
                if exponent.abs() != 1 {
                    write!(f, "^{}", exponent.abs())?;
                }
            }
            Ok(())
        };
        if numerator.is_empty() {
            f.write_str("1")?;
        } else {
            write_part(f, &numerator)?;
        }
        if !denominator.is_empty() {
            f.write_str("/")?;
            write_part(f, &denominator)?;
        }
        f.write_str("}")
    }
}

/// A dimension, unknown, or no value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Units {
    /// A literal: dimensionless with this decimal scale in products, and adopting the other
    /// operand's dimension in sums and comparisons, so `amount + 1` stays consistent.
    Literal(i32),
    /// Exactly this dimension on every path.
    Known(Dimension),
    /// Unknown or conflicting dimensions.
    Unknown,
}

impl Units {
    fn as_product_factor(&self) -> Option<Dimension> {
        match self {
            Self::Literal(scale) => Some(Dimension::dimensionless(*scale)),
            Self::Known(dimension) => Some(dimension.clone()),
            Self::Unknown => None,
        }
    }
}

impl JoinSemiLattice for Units {
    fn join(&mut self, other: &Self) -> bool {
        match (&*self, other) {
            (Self::Unknown, _) => false,
            (a, b) if a == b => false,
            _ => {
                *self = Self::Unknown;
                true
            }
        }
    }
}

impl fmt::Display for Units {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Literal(scale) => write!(f, "D{scale}{{1}}"),
            Self::Known(dimension) => dimension.fmt(f),
            Self::Unknown => f.write_str("?"),
        }
    }
}

/// Returns the decimal exponent of a power of ten.
fn power_of_ten(value: U256) -> Option<i32> {
    let mut exponent = 0;
    let mut remaining = value;
    let ten = U256::from(10);
    while remaining > U256::from(1) {
        if remaining % ten != U256::ZERO {
            return None;
        }
        remaining /= ten;
        exponent += 1;
    }
    (remaining == U256::from(1) && exponent > 0).then_some(exponent)
}

/// How an operation combines dimensions.
enum Algebra {
    Same,
    Product(i32),
    Compare,
    Pass,
    Other,
}

fn algebra(op: Op) -> Algebra {
    match op {
        Op::Add { .. } | Op::Sub { .. } => Algebra::Same,
        Op::Mul { .. } => Algebra::Product(1),
        Op::Div { .. } => Algebra::Product(-1),
        Op::Lt { .. } | Op::Gt { .. } | Op::Eq { .. } | Op::Ne { .. } => Algebra::Compare,
        Op::Zext { .. } | Op::Trunc { .. } | Op::Bitcast { .. } => Algebra::Pass,
        Op::CheckedBinary { op, .. } => match op {
            CheckedOp::Add | CheckedOp::Sub => Algebra::Same,
            CheckedOp::Mul => Algebra::Product(1),
            CheckedOp::Div | CheckedOp::WrappingDiv => Algebra::Product(-1),
            CheckedOp::Rem | CheckedOp::Pow => Algebra::Other,
        },
        _ => Algebra::Other,
    }
}

impl ValueDomain for Units {
    const NAME: &'static str = "units";

    fn top() -> Self {
        Self::Unknown
    }

    fn constant(value: U256) -> Self {
        Self::Literal(power_of_ten(value).unwrap_or(0))
    }

    fn transfer(_cx: &DomainCx<'_>, op: Op, operands: &[Self]) -> Self {
        let arg = |i: usize| operands.get(i).cloned().unwrap_or(Self::Unknown);
        match (algebra(op), arg(0), arg(1)) {
            (Algebra::Pass, a, _) => a,
            (Algebra::Same, Self::Literal(_), other) | (Algebra::Same, other, Self::Literal(_)) => {
                other
            }
            (Algebra::Same, Self::Known(a), Self::Known(b)) if a == b => Self::Known(a),
            (Algebra::Product(sign), a, b) => {
                match (a.as_product_factor(), b.as_product_factor(), &a, &b) {
                    (_, _, Self::Literal(x), Self::Literal(y)) => Self::Literal(x + sign * y),
                    (Some(a), Some(b), _, _) => Self::Known(a.combine(&b, sign)),
                    _ => Self::Unknown,
                }
            }
            (
                Algebra::Compare,
                Self::Known(_) | Self::Literal(_),
                Self::Known(_) | Self::Literal(_),
            ) => Self::Literal(0),
            _ if matches!(op, Op::Select { .. }) => {
                let mut joined = arg(1);
                joined.join(&arg(2));
                joined
            }
            _ => Self::Unknown,
        }
    }

    fn check(cx: &DomainCx<'_>, op: Op, operands: &[Self], findings: &mut Vec<String>) {
        let (Some(Self::Known(a)), Some(Self::Known(b))) = (operands.first(), operands.get(1))
        else {
            return;
        };
        if matches!(algebra(op), Algebra::Same | Algebra::Compare) && a != b {
            let text = crate::mir::display::display_instruction(cx.func, Some(cx.module), cx.inst);
            findings.push(format!("`{text}` combines {a} with {b}"));
        }
    }
}

impl Seeded for Units {
    fn parse_seed(text: &str) -> Option<Self> {
        let token = text.split_whitespace().find(|word| word.contains('{'))?;
        Dimension::parse(token).map(Self::Known)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_combine() {
        let price = Dimension::parse("D18{UoA/tok}").unwrap();
        let amount = Dimension::parse("D18{tok}").unwrap();
        let value = price.combine(&amount, 1);
        assert_eq!(value.to_string(), "D36{UoA}");
        assert_eq!(value.combine(&Dimension::dimensionless(18), -1).to_string(), "D18{UoA}");
        assert_eq!(power_of_ten(U256::from(1_000_000_000_000_000_000u128)), Some(18));
        assert_eq!(power_of_ten(U256::from(100)), Some(2));
        assert_eq!(power_of_ten(U256::from(3)), None);
    }
}
