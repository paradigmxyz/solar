//! The octagon abstract domain (Miné, "The Octagon Abstract Domain", HOSC 2006) over
//! mathematical integers: conjunctions of `±x ± y <= c` and `±x <= c`, kept as a difference-bound
//! matrix over the 2n signed forms of the variables and closed with the integer tight closure of
//! Bagnara, Hill and Zaffanella ("An improved tight closure algorithm for integer octagonal
//! constraints", VMCAI 2008).
//!
//! Variables are named; [`Lin`] is a linear form over them. Bounds are exact [`BigInt`]s, so
//! 256-bit ranges are represented without loss.

use num_bigint::BigInt;
use num_traits::{One, Signed, Zero};
use std::collections::BTreeMap;

/// An upper bound, `None` for +∞.
pub(crate) type Bound = Option<BigInt>;

fn add(a: &Bound, b: &Bound) -> Bound {
    Some(a.as_ref()? + b.as_ref()?)
}

fn le(a: &Bound, b: &Bound) -> bool {
    match (a, b) {
        (_, None) => true,
        (None, Some(_)) => false,
        (Some(a), Some(b)) => a <= b,
    }
}

fn min(a: &Bound, b: &Bound) -> Bound {
    if le(a, b) { a.clone() } else { b.clone() }
}

fn max(a: &Bound, b: &Bound) -> Bound {
    if le(a, b) { b.clone() } else { a.clone() }
}

/// ⌊x / 2⌋.
fn floor_half(x: &BigInt) -> BigInt {
    if x.is_negative() { -((-x + BigInt::one()) / 2u32) } else { x / 2u32 }
}

/// A linear form `Σ coeff·var + constant` over named variables.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct Lin {
    pub terms: BTreeMap<String, BigInt>,
    pub constant: BigInt,
}

impl Lin {
    pub(crate) fn constant(c: BigInt) -> Self {
        Self { terms: BTreeMap::new(), constant: c }
    }
    pub(crate) fn var(name: &str) -> Self {
        let mut terms = BTreeMap::new();
        terms.insert(name.to_string(), BigInt::one());
        Self { terms, constant: BigInt::zero() }
    }
    pub(crate) fn add(&self, other: &Self) -> Self {
        let mut out = self.clone();
        for (v, c) in &other.terms {
            let e = out.terms.entry(v.clone()).or_default();
            *e += c;
            if e.is_zero() {
                out.terms.remove(v);
            }
        }
        out.constant += &other.constant;
        out
    }
    pub(crate) fn scale(&self, k: &BigInt) -> Self {
        if k.is_zero() {
            return Self::default();
        }
        Self {
            terms: self.terms.iter().map(|(v, c)| (v.clone(), c * k)).collect(),
            constant: &self.constant * k,
        }
    }
    pub(crate) fn neg(&self) -> Self {
        self.scale(&-BigInt::one())
    }
    pub(crate) fn sub(&self, other: &Self) -> Self {
        self.add(&other.neg())
    }
    pub(crate) fn as_constant(&self) -> Option<&BigInt> {
        self.terms.is_empty().then_some(&self.constant)
    }
    pub(crate) fn plus(&self, k: i64) -> Self {
        self.add(&Self::constant(BigInt::from(k)))
    }
}

/// `sign_x·x + sign_y·y <= c` over variable indices (`y` None for a unary constraint).
pub(crate) type Constraint = (usize, bool, Option<(usize, bool)>, BigInt);

/// A conjunction of octagonal constraints over named integer variables, or bottom.
#[derive(Clone, Debug)]
pub(crate) struct Octagon {
    names: Vec<String>,
    /// `m[i * 2n + j]` bounds `V_j - V_i`, where `V_2k = x_k` and `V_2k+1 = -x_k`.
    m: Vec<Bound>,
    bottom: bool,
}

/// The signed form `V_i` of variable `k` with sign `positive`.
fn form(k: usize, positive: bool) -> usize {
    2 * k + usize::from(!positive)
}

impl Octagon {
    pub(crate) fn top() -> Self {
        Self { names: vec![], m: vec![], bottom: false }
    }
    pub(crate) fn bottom() -> Self {
        Self { names: vec![], m: vec![], bottom: true }
    }
    pub(crate) fn is_bottom(&self) -> bool {
        self.bottom
    }
    pub(crate) fn names(&self) -> &[String] {
        &self.names
    }
    fn dim(&self) -> usize {
        2 * self.names.len()
    }
    fn at(&self, i: usize, j: usize) -> &Bound {
        &self.m[i * self.dim() + j]
    }
    fn at_mut(&mut self, i: usize, j: usize) -> &mut Bound {
        let d = self.dim();
        &mut self.m[i * d + j]
    }
    pub(crate) fn index(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }
    pub(crate) fn has(&self, name: &str) -> bool {
        self.index(name).is_some()
    }

    /// Adds `name` unconstrained, if it is not there yet; returns its index.
    pub(crate) fn add_var(&mut self, name: &str) -> usize {
        if let Some(k) = self.index(name) {
            return k;
        }
        let old = self.dim();
        let mut names = self.names.clone();
        names.push(name.to_string());
        let d = 2 * names.len();
        let mut m = vec![None; d * d];
        for i in 0..old {
            for j in 0..old {
                m[i * d + j] = self.m[i * old + j].clone();
            }
        }
        for i in 0..d {
            m[i * d + i] = Some(BigInt::zero());
        }
        self.names = names;
        self.m = m;
        self.names.len() - 1
    }

    /// Removes `name` and every constraint on it.
    pub(crate) fn remove_var(&mut self, name: &str) {
        let Some(k) = self.index(name) else { return };
        let old = self.dim();
        let keep: Vec<usize> = (0..old).filter(|&i| i / 2 != k).collect();
        let d = keep.len();
        let mut m = vec![None; d * d];
        for (a, &i) in keep.iter().enumerate() {
            for (b, &j) in keep.iter().enumerate() {
                m[a * d + b] = self.m[i * old + j].clone();
            }
        }
        self.names.remove(k);
        self.m = m;
    }

    /// Drops every constraint on `name`, keeping it as an unconstrained variable.
    pub(crate) fn forget(&mut self, name: &str) {
        let Some(k) = self.index(name) else { return };
        let d = self.dim();
        for i in 0..d {
            for f in [2 * k, 2 * k + 1] {
                if i != f {
                    *self.at_mut(i, f) = None;
                    *self.at_mut(f, i) = None;
                }
            }
        }
    }

    /// Makes both octagons range over the same variables, in the same order.
    fn unify(a: &mut Self, b: &mut Self) {
        for n in b.names.clone() {
            a.add_var(&n);
        }
        for n in a.names.clone() {
            b.add_var(&n);
        }
        if a.names != b.names {
            let names = a.names.clone();
            let mut c = Self::top();
            for n in &names {
                c.add_var(n);
            }
            for i in 0..c.dim() {
                for j in 0..c.dim() {
                    let (bi, bj) = Self::remap(&b.names, &names, i, j);
                    *c.at_mut(i, j) = b.at(bi, bj).clone();
                }
            }
            c.bottom = b.bottom;
            *b = c;
        }
    }

    fn remap(from: &[String], to: &[String], i: usize, j: usize) -> (usize, usize) {
        let f = |x: usize| {
            let k = from.iter().position(|n| *n == to[x / 2]).unwrap();
            2 * k + x % 2
        };
        (f(i), f(j))
    }

    /// Integer tight closure; sets bottom when the constraints are unsatisfiable.
    pub(crate) fn close(&mut self) {
        if self.bottom {
            return;
        }
        let d = self.dim();
        for k in 0..d {
            for i in 0..d {
                let ik = self.at(i, k).clone();
                if ik.is_none() {
                    continue;
                }
                for j in 0..d {
                    let via = add(&ik, self.at(k, j));
                    if !le(self.at(i, j), &via) {
                        *self.at_mut(i, j) = via;
                    }
                }
            }
        }
        for i in 0..d {
            if let Some(c) = self.at(i, i ^ 1).clone() {
                *self.at_mut(i, i ^ 1) = Some(floor_half(&c) * 2);
            }
        }
        for i in 0..d {
            for j in 0..d {
                if let (Some(a), Some(b)) = (self.at(i, i ^ 1).clone(), self.at(j ^ 1, j).clone()) {
                    let s = Some(floor_half(&(a + b)));
                    if !le(self.at(i, j), &s) {
                        *self.at_mut(i, j) = s;
                    }
                }
            }
        }
        for i in 0..d {
            if self.at(i, i).as_ref().is_some_and(|c| c.is_negative()) {
                *self = Self::bottom();
                return;
            }
            *self.at_mut(i, i) = Some(BigInt::zero());
        }
    }

    /// Adds `V_j - V_i <= c` and its coherent twin.
    fn tighten(&mut self, i: usize, j: usize, c: BigInt) {
        let c = Some(c);
        if !le(self.at(i, j), &c) {
            *self.at_mut(i, j) = c.clone();
        }
        let (bi, bj) = (j ^ 1, i ^ 1);
        if !le(self.at(bi, bj), &c) {
            *self.at_mut(bi, bj) = c;
        }
    }

    /// The octagonal reading of `lin <= 0`: `(V_j - V_i, bound)` with `V_j - V_i <= bound`.
    fn octagonal(&self, lin: &Lin) -> Option<(usize, usize, BigInt)> {
        let terms: Vec<(usize, &BigInt)> =
            lin.terms.iter().map(|(v, c)| Some((self.index(v)?, c))).collect::<Option<_>>()?;
        let one = BigInt::one();
        let c = -lin.constant.clone();
        match terms[..] {
            [(k, a)] if a.abs() == one => {
                Some((form(k, !a.is_positive()), form(k, a.is_positive()), 2 * c))
            }
            [(k, a)] if a.abs() == BigInt::from(2) => {
                Some((form(k, !a.is_positive()), form(k, a.is_positive()), c))
            }
            [(k, a), (l, b)] if a.abs() == one && b.abs() == one => {
                Some((form(l, !b.is_positive()), form(k, a.is_positive()), c))
            }
            _ => None,
        }
    }

    /// The least upper bound of `lin` this octagon proves (closed octagons only).
    pub(crate) fn upper(&self, lin: &Lin) -> Bound {
        if self.bottom {
            return Some(BigInt::zero());
        }
        for v in lin.terms.keys() {
            if !self.has(v) {
                return None;
            }
        }
        // the octagonal part over a pair (or one) of unit-coefficient variables, the rest by
        // interval bounds
        let interval = |lin: &Lin| -> Bound {
            let mut sum = Some(lin.constant.clone());
            for (v, c) in &lin.terms {
                let k = self.index(v)?;
                let b = if c.is_positive() {
                    self.at(form(k, false), form(k, true)).as_ref().map(|u| floor_half(u) * c)
                } else {
                    self.at(form(k, true), form(k, false)).as_ref().map(|u| floor_half(u) * -c)
                };
                sum = add(&sum, &b);
            }
            sum
        };
        let mut best = interval(lin);
        let unit: Vec<&String> =
            lin.terms.iter().filter(|(_, c)| c.abs() == BigInt::one()).map(|(v, _)| v).collect();
        for (a, x) in unit.iter().enumerate() {
            for y in unit.iter().skip(a + 1) {
                let mut pair = Lin::default();
                pair.terms.insert((*x).clone(), lin.terms[*x].clone());
                pair.terms.insert((*y).clone(), lin.terms[*y].clone());
                let rest = lin.sub(&pair);
                if let Some((i, j, _)) = self.octagonal(&pair) {
                    best = min(&best, &add(self.at(i, j), &interval(&rest)));
                }
            }
        }
        best
    }

    /// The interval of `lin`.
    pub(crate) fn range(&self, lin: &Lin) -> (Option<BigInt>, Option<BigInt>) {
        (self.upper(&lin.neg()).map(|b| -b), self.upper(lin))
    }

    /// Does the octagon prove `lin <= 0`?
    pub(crate) fn entails_le0(&self, lin: &Lin) -> bool {
        self.bottom || self.upper(lin).is_some_and(|b| !b.is_positive())
    }

    /// Does the octagon prove `a == b`?
    pub(crate) fn entails_eq(&self, a: &Lin, b: &Lin) -> bool {
        let d = a.sub(b);
        self.entails_le0(&d) && self.entails_le0(&d.neg())
    }

    /// Meets with `lin <= 0` (variables it does not know are added), then closes.
    pub(crate) fn assume_le0(&mut self, lin: &Lin) {
        if self.bottom {
            return;
        }
        for v in lin.terms.keys() {
            self.add_var(v);
        }
        if let Some(c) = lin.as_constant() {
            if c.is_positive() {
                *self = Self::bottom();
            }
            return;
        }
        if let Some((i, j, c)) = self.octagonal(lin) {
            self.tighten(i, j, c);
        } else {
            // a wider constraint: bound each unit-coefficient variable (and pair) by the rest
            let mut derived = vec![];
            for (v, c) in &lin.terms {
                if c.abs() != BigInt::one() {
                    continue;
                }
                let mut me = Lin::default();
                me.terms.insert(v.clone(), c.clone());
                let rest = lin.sub(&me);
                // c·v <= -rest <= -lower(rest)
                if let Some(lo) = self.range(&rest).0 {
                    derived.push(me.add(&Lin::constant(lo)));
                }
            }
            for d in derived {
                if let Some((i, j, c)) = self.octagonal(&d) {
                    self.tighten(i, j, c);
                }
            }
        }
        self.close();
    }

    /// `lo <= var <= hi`.
    pub(crate) fn assume_range(&mut self, var: &str, lo: &BigInt, hi: &BigInt) {
        let v = Lin::var(var);
        self.assume_le0(&v.sub(&Lin::constant(hi.clone())));
        self.assume_le0(&Lin::constant(lo.clone()).sub(&v));
    }

    /// `var := lin`, over the octagon before the assignment.
    pub(crate) fn assign(&mut self, var: &str, lin: &Lin) {
        if self.bottom {
            return;
        }
        for v in lin.terms.keys() {
            self.add_var(v);
        }
        let k = self.add_var(var);
        let one = BigInt::one();
        // `x := x + c`: an exact shift of every constraint on x
        if lin.terms.len() == 1 && lin.terms.get(var) == Some(&one) {
            let c = lin.constant.clone();
            let d = self.dim();
            let delta = |i: usize| -> BigInt {
                if i == 2 * k {
                    c.clone()
                } else if i == 2 * k + 1 {
                    -c.clone()
                } else {
                    BigInt::zero()
                }
            };
            for i in 0..d {
                for j in 0..d {
                    let sh = delta(j) - delta(i);
                    if let Some(b) = self.at_mut(i, j) {
                        *b += sh;
                    }
                }
            }
            return;
        }
        // otherwise: bound x' and x' ± y by what the old octagon proves of lin and lin ∓ y
        let x = Lin::var(var);
        let mut facts: Vec<Lin> = vec![];
        let mut bound = |shape: Lin, value: Lin| {
            // shape(x') <= upper(value), as `shape - upper <= 0`
            if let Some(u) = self.upper(&value) {
                facts.push(shape.sub(&Lin::constant(u)));
            }
        };
        bound(x.clone(), lin.clone());
        bound(x.neg(), lin.neg());
        for y in self.names.clone() {
            if y == var {
                continue;
            }
            let yl = Lin::var(&y);
            bound(x.sub(&yl), lin.sub(&yl));
            bound(x.add(&yl), lin.add(&yl));
            bound(yl.sub(&x), yl.sub(lin));
            bound(x.neg().sub(&yl), lin.neg().sub(&yl));
        }
        self.forget(var);
        for f in facts {
            if let Some((i, j, c)) = self.octagonal(&f) {
                self.tighten(i, j, c);
            }
        }
        self.close();
    }

    /// Is every state of `self` one of `other`? (Both closed.)
    pub(crate) fn leq(&self, other: &Self) -> bool {
        if self.bottom {
            return true;
        }
        if other.bottom {
            return false;
        }
        let (mut a, mut b) = (self.clone(), other.clone());
        Self::unify(&mut a, &mut b);
        (0..a.m.len()).all(|i| le(&a.m[i], &b.m[i]))
    }

    /// The least octagon containing both (closed inputs give a closed result).
    pub(crate) fn join(&self, other: &Self) -> Self {
        if self.bottom {
            return other.clone();
        }
        if other.bottom {
            return self.clone();
        }
        let (mut a, mut b) = (self.clone(), other.clone());
        Self::unify(&mut a, &mut b);
        for i in 0..a.m.len() {
            a.m[i] = max(&a.m[i], &b.m[i]);
        }
        a
    }

    /// Widening with thresholds: a bound that grew jumps to the next threshold (twice it for the
    /// unary entries), or to +∞ past the last one.
    pub(crate) fn widen(&self, next: &Self, thresholds: &[BigInt]) -> Self {
        if self.bottom {
            return next.clone();
        }
        if next.bottom {
            return self.clone();
        }
        let (mut a, mut b) = (self.clone(), next.clone());
        Self::unify(&mut a, &mut b);
        let d = a.dim();
        for i in 0..d {
            for j in 0..d {
                let (old, new) = (a.at(i, j).clone(), b.at(i, j).clone());
                if le(&new, &old) {
                    continue;
                }
                let scale = if j == (i ^ 1) { BigInt::from(2) } else { BigInt::one() };
                let n = new.unwrap();
                *a.at_mut(i, j) = thresholds.iter().map(|t| t * &scale).find(|t| *t >= n);
            }
        }
        a
    }

    /// Every finite constraint over `(x, sign_x, y, sign_y)` (`y` None for a unary one), as
    /// `sign_x·x + sign_y·y <= c`.
    pub(crate) fn constraints(&self) -> Vec<Constraint> {
        let mut out = vec![];
        if self.bottom {
            return out;
        }
        let n = self.names.len();
        for k in 0..n {
            // x <= c: V_2k - V_2k+1 <= 2c
            if let Some(c) = self.at(form(k, false), form(k, true)) {
                out.push((k, true, None, floor_half(c)));
            }
            if let Some(c) = self.at(form(k, true), form(k, false)) {
                out.push((k, false, None, floor_half(c)));
            }
            for l in k + 1..n {
                for sx in [true, false] {
                    for sy in [true, false] {
                        // sx·x + sy·y <= c: V_form(x, sx) - V_form(y, !sy) <= c
                        if let Some(c) = self.at(form(l, !sy), form(k, sx)) {
                            out.push((k, sx, Some((l, sy)), c.clone()));
                        }
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(x: i64) -> BigInt {
        BigInt::from(x)
    }

    #[test]
    fn closure_and_entailment() {
        let mut o = Octagon::top();
        let (i, n) = (Lin::var("i"), Lin::var("n"));
        o.assume_le0(&i.sub(&n)); // i <= n
        o.assume_le0(&n.sub(&Lin::constant(b(10)))); // n <= 10
        assert!(o.entails_le0(&i.sub(&Lin::constant(b(10)))));
        assert!(!o.entails_le0(&i.sub(&Lin::constant(b(9)))));
        o.assign("i", &i.plus(1));
        assert!(o.entails_le0(&i.sub(&n).plus(-1)));
        o.assume_le0(&Lin::constant(b(12)).sub(&i)); // i >= 12: contradiction with i <= 11
        assert!(o.is_bottom());
    }

    #[test]
    fn integer_tightening() {
        let mut o = Octagon::top();
        let (x, y) = (Lin::var("x"), Lin::var("y"));
        // x + y <= 3 and x - y <= 0 give 2x <= 3, so x <= 1 over the integers
        o.assume_le0(&x.add(&y).plus(-3));
        o.assume_le0(&x.sub(&y));
        assert!(o.entails_le0(&x.plus(-1)));
    }

    #[test]
    fn widening_with_thresholds() {
        let mut a = Octagon::top();
        a.assume_range("i", &b(0), &b(0));
        let mut c = Octagon::top();
        c.assume_range("i", &b(0), &b(1));
        let w = a.widen(&c, &[b(0), b(1), b(100)]);
        let mut w2 = w.clone();
        w2.close();
        assert_eq!(w2.range(&Lin::var("i")), (Some(b(0)), Some(b(1))));
        let mut d = Octagon::top();
        d.assume_range("i", &b(0), &b(2));
        let mut w3 = w2.widen(&d, &[b(0), b(1), b(100)]);
        w3.close();
        assert_eq!(w3.range(&Lin::var("i")), (Some(b(0)), Some(b(100))));
    }

    #[test]
    fn assignment_relations() {
        let mut o = Octagon::top();
        o.assume_range("y", &b(0), &b(5));
        o.assign("x", &Lin::var("y").plus(2));
        assert!(o.entails_eq(&Lin::var("x"), &Lin::var("y").plus(2)));
        o.assign("z", &Lin::var("x").add(&Lin::var("y")));
        assert_eq!(o.range(&Lin::var("z")), (Some(b(2)), Some(b(12))));
        assert!(o.entails_le0(&Lin::var("x").sub(&Lin::var("z"))));
    }
}
