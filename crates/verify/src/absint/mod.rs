//! Loop invariant inference by abstract interpretation over the function's AST.
//!
//! The analysis runs each function body forward in a product of two domains:
//!
//! - [`octagon`]: relations `±x ± y <= c` between integer locals, parameters, return variables and
//!   array lengths, with widening up to thresholds drawn from the function's constants and type
//!   bounds, then two descending iterations;
//! - array segment facts: `forall j in [lo, hi): xs[j] <= v` (or `>=`) and `exists j in [lo, hi):
//!   xs[j] == v`, for arrays the code does not write, with segment bounds `var + k` or constants
//!   (after Cousot, Cousot and Logozzo, "A Parametric Segmentation Functor for Fully Automatic and
//!   Scalable Array Content Analysis", POPL 2011). Reads `xs[e]` become cell variables in the
//!   octagon, so guards such as `xs[i] > m` relate them to scalars.
//!
//! What the analysis cannot model forgets what it may touch (assembly, calls, array writes, other
//! statements), so its results over-approximate the reachable states. Nothing it produces is
//! trusted: each loop's head state is written as spec-language invariants
//! ([`infer_function`]) that the verifier proves like any other (entry, preservation, use).

pub(crate) mod octagon;

use num_bigint::BigInt;
use num_traits::{One, Signed, Zero};
use octagon::{Lin, Octagon};
use solar_parse::{
    ast::{
        BinOpKind, DataLocation, ElementaryType, Expr, ExprKind, IndexKind, ItemFunction, LitKind,
        Stmt, StmtKind, TypeKind, UnOpKind, VariableDefinition,
    },
    interface::{
        Span,
        data_structures::map::{FxHashMap, FxHashSet},
    },
};
use std::collections::BTreeSet;

/// Whether `--infer-absint` asked for the analysis (set once, before any program is built).
static ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Turns the analysis on for every program built from now on.
pub(crate) fn enable() {
    ENABLED.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Whether the analysis runs.
pub(crate) fn enabled() -> bool {
    ENABLED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Segment facts kept per state, beyond which the analysis keeps the first ones.
const MAX_FACTS: usize = 48;
/// Ascending iterations at a loop head before the analysis gives up on the loop.
const MAX_ITERATIONS: usize = 40;
/// Joins before widening starts.
const WIDEN_DELAY: usize = 2;

/// What a fact says of the array's cells in its segment, relative to the scalar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Kind {
    /// every cell is `<= v`
    Le,
    /// every cell is `>= v`
    Ge,
    /// some cell is `== v`
    Eq,
}

/// A segment fact about array `arr` over `[lo, hi)` and scalar `val`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Fact {
    kind: Kind,
    arr: String,
    val: String,
    lo: SegBound,
    hi: SegBound,
}

/// A segment bound: `var + k`, or the constant `k`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct SegBound {
    var: Option<String>,
    k: BigInt,
}

impl SegBound {
    fn lin(&self) -> Lin {
        let base = self.var.as_deref().map(Lin::var).unwrap_or_default();
        base.add(&Lin::constant(self.k.clone()))
    }
    fn of(lin: &Lin) -> Option<Self> {
        match lin.terms.len() {
            0 => Some(Self { var: None, k: lin.constant.clone() }),
            1 => {
                let (v, c) = lin.terms.iter().next()?;
                c.is_one().then(|| Self { var: Some(v.clone()), k: lin.constant.clone() })
            }
            _ => None,
        }
    }
    fn mentions(&self, v: &str) -> bool {
        self.var.as_deref() == Some(v)
    }
}

/// A cell `arr[idx]` read by the code, tracked as an octagon variable.
#[derive(Clone, Debug, PartialEq)]
struct Cell {
    name: String,
    arr: String,
    idx: SegBound,
}

#[derive(Clone, Debug)]
struct State {
    oct: Octagon,
    facts: Vec<Fact>,
    cells: Vec<Cell>,
}

impl State {
    fn bottom() -> Self {
        Self { oct: Octagon::bottom(), facts: vec![], cells: vec![] }
    }
    fn is_bottom(&self) -> bool {
        self.oct.is_bottom()
    }
}

#[derive(Clone, Debug)]
enum VarKind {
    /// an integer of this range
    Int(BigInt, BigInt),
    /// a memory or calldata array, with the range of its integer elements (None: not integers)
    Arr(Option<(BigInt, BigInt)>),
}

struct LoopFrame {
    breaks: Vec<State>,
    conts: Vec<State>,
}

struct Analyzer {
    vars: FxHashMap<String, VarKind>,
    scopes: Vec<Vec<String>>,
    thresholds: Vec<BigInt>,
    loops: Vec<LoopFrame>,
    unchecked: usize,
    ncells: usize,
    /// loop statement span -> invariant texts at its head (the last analysis of the loop wins,
    /// which is the one under the enclosing loops' final head states)
    out: FxHashMap<Span, Vec<String>>,
}

fn pow2(n: u32) -> BigInt {
    BigInt::one() << n
}

fn int_range(t: &solar_parse::ast::Type<'_>) -> Option<(BigInt, BigInt)> {
    match &t.kind {
        TypeKind::Elementary(ElementaryType::UInt(n)) => {
            Some((BigInt::zero(), pow2(n.bits() as u32) - 1))
        }
        TypeKind::Elementary(ElementaryType::Int(n)) => {
            let b = n.bits() as u32;
            Some((-pow2(b - 1), pow2(b - 1) - 1))
        }
        _ => None,
    }
}

fn var_kind(d: &VariableDefinition<'_>, param: bool) -> Option<VarKind> {
    if let Some(r) = int_range(&d.ty) {
        return Some(VarKind::Int(r.0, r.1));
    }
    match &d.ty.kind {
        // dynamic memory and calldata arrays (storage arrays can change length through aliases)
        TypeKind::Array(a)
            if a.size.is_none()
                && matches!(
                    d.data_location,
                    Some(DataLocation::Memory | DataLocation::Calldata)
                )
                && (param || d.data_location.is_some()) =>
        {
            Some(VarKind::Arr(int_range(&a.element)))
        }
        _ => None,
    }
}

fn len_name(arr: &str) -> String {
    format!("{arr}.length")
}

/// Infers invariants for every loop in `f`'s body, assuming `requires` at entry. Returns
/// spec-language texts keyed by the loop statement's span.
pub(crate) fn infer_function<'a>(
    f: &'a ItemFunction<'a>,
    requires: &[&'a Expr<'a>],
) -> FxHashMap<Span, Vec<String>> {
    let Some(body) = &f.body else { return FxHashMap::default() };
    let mut a = Analyzer {
        vars: FxHashMap::default(),
        scopes: vec![vec![]],
        thresholds: vec![],
        loops: vec![],
        unchecked: 0,
        ncells: 0,
        out: FxHashMap::default(),
    };
    let mut consts: BTreeSet<BigInt> = BTreeSet::new();
    for s in body.stmts.iter() {
        crate::walk::all_stmts(s, &mut |s| {
            crate::walk::stmt_exprs(s, &mut |e| {
                crate::walk::sub_exprs(e, &mut |x| {
                    if let ExprKind::Lit(l, None) = &x.kind
                        && let LitKind::Number(n) = &l.kind
                        && let Ok(n) = n.to_string().parse::<BigInt>()
                    {
                        consts.insert(n);
                    }
                })
            })
        });
    }
    let mut st = State { oct: Octagon::top(), facts: vec![], cells: vec![] };
    let params = f.header.parameters.iter().map(|p| (p, true));
    let rets = f.header.returns().iter().map(|p| (p, false));
    for (p, is_param) in params.chain(rets) {
        let Some(name) = p.name else { continue };
        let Some(kind) = var_kind(p, true) else { continue };
        let name = name.as_str().to_string();
        a.declare(&mut st, &name, kind.clone());
        if !is_param {
            // return variables start zeroed
            if let VarKind::Int(..) = kind {
                st.oct.assign(&name, &Lin::constant(BigInt::zero()));
            } else {
                st.oct.assign(&len_name(&name), &Lin::constant(BigInt::zero()));
            }
        }
    }
    for kind in a.vars.values() {
        if let VarKind::Int(lo, hi) = kind {
            consts.insert(lo.clone());
            consts.insert(hi.clone());
        }
    }
    consts.insert(pow2(256) - 1);
    let mut th: BTreeSet<BigInt> = BTreeSet::new();
    for c in &consts {
        for d in [-1i64, 0, 1] {
            th.insert(c + d);
            th.insert(-(c + d));
        }
    }
    a.thresholds = th.into_iter().collect();
    for r in requires {
        st = a.assume(st, r, true);
    }
    for s in body.stmts.iter() {
        st = a.stmt(st, s);
    }
    a.out
}

impl Analyzer {
    fn declare(&mut self, st: &mut State, name: &str, kind: VarKind) {
        self.vars.insert(name.to_string(), kind.clone());
        self.scopes.last_mut().unwrap().push(name.to_string());
        self.kill_var(st, name);
        match &kind {
            VarKind::Int(lo, hi) => {
                st.oct.add_var(name);
                st.oct.assume_range(name, lo, hi);
            }
            VarKind::Arr(_) => {
                let l = len_name(name);
                st.oct.add_var(&l);
                st.oct.assume_range(&l, &BigInt::zero(), &(pow2(256) - 1));
            }
        }
    }

    /// Forgets `name` (a scalar or an array) and every fact and cell that mentions it.
    fn kill_var(&mut self, st: &mut State, name: &str) {
        let l = len_name(name);
        st.facts.retain(|f| {
            f.arr != name
                && f.val != name
                && !f.lo.mentions(name)
                && !f.hi.mentions(name)
                && !f.lo.mentions(&l)
                && !f.hi.mentions(&l)
        });
        let dead: Vec<String> = st
            .cells
            .iter()
            .filter(|c| c.arr == name || c.idx.mentions(name) || c.idx.mentions(&l))
            .map(|c| c.name.clone())
            .collect();
        for d in dead {
            self.drop_cell(st, &d);
        }
        st.oct.forget(name);
        st.oct.forget(&l);
    }

    fn drop_cell(&mut self, st: &mut State, name: &str) {
        st.cells.retain(|c| c.name != name);
        st.oct.remove_var(name);
    }

    /// Array contents may have changed (an array write, a call, assembly): drop cells and facts.
    fn kill_memory(&mut self, st: &mut State) {
        for c in st.cells.clone() {
            self.drop_cell(st, &c.name);
        }
        st.facts.clear();
    }

    /// Forgets everything a statement the analysis does not model could change.
    fn havoc_all(&mut self, st: &mut State) {
        self.kill_memory(st);
        for (n, k) in self.vars.clone() {
            if st.oct.has(&n) || st.oct.has(&len_name(&n)) {
                self.kill_var(st, &n);
                self.restore_range(st, &n, &k);
            }
        }
    }

    fn restore_range(&mut self, st: &mut State, n: &str, k: &VarKind) {
        match k {
            VarKind::Int(lo, hi) => st.oct.assume_range(n, lo, hi),
            VarKind::Arr(_) => st.oct.assume_range(&len_name(n), &BigInt::zero(), &(pow2(256) - 1)),
        }
    }

    fn is_int(&self, n: &str) -> bool {
        matches!(self.vars.get(n), Some(VarKind::Int(..)))
    }

    /// Forgets what `e` may write: assigned and incremented variables, and memory on calls.
    fn havoc_effects<'a>(&mut self, st: &mut State, e: &'a Expr<'a>) {
        let mut writes: Vec<String> = vec![];
        let mut call = false;
        crate::walk::sub_exprs(e, &mut |x| match &x.kind {
            ExprKind::Assign(l, ..) => {
                writes.extend(crate::walk::root(l));
                if !matches!(l.peel_parens().kind, ExprKind::Ident(_)) {
                    call = true;
                }
            }
            ExprKind::Unary(op, x)
                if matches!(
                    op.kind,
                    UnOpKind::PreInc | UnOpKind::PreDec | UnOpKind::PostInc | UnOpKind::PostDec
                ) =>
            {
                writes.extend(crate::walk::root(x));
            }
            ExprKind::Call(..) | ExprKind::Delete(_) => call = true,
            _ => {}
        });
        if call {
            self.kill_memory(st);
        }
        for w in writes {
            if let Some(k) = self.vars.get(&w).cloned() {
                self.kill_var(st, &w);
                self.restore_range(st, &w, &k);
            }
        }
    }

    // ------------------------------------------------------------------ expressions

    /// The linear value of `e`, applying its effects to `st`; None when it is not linear in the
    /// tracked variables (its effects are then applied conservatively).
    fn eval<'a>(&mut self, st: &mut State, e: &'a Expr<'a>) -> Option<Lin> {
        let e = e.peel_parens();
        match &e.kind {
            ExprKind::Lit(l, None) => match &l.kind {
                LitKind::Number(n) => Some(Lin::constant(n.to_string().parse().ok()?)),
                _ => None,
            },
            ExprKind::Ident(id) if self.is_int(id.as_str()) && st.oct.has(id.as_str()) => {
                Some(Lin::var(id.as_str()))
            }
            // `type(T).max` and `type(T).min`
            ExprKind::Member(b, m)
                if matches!(m.as_str(), "max" | "min")
                    && let ExprKind::TypeCall(t) = &b.peel_parens().kind
                    && let Some((lo, hi)) = int_range(t) =>
            {
                Some(Lin::constant(if m.as_str() == "max" { hi } else { lo }))
            }
            ExprKind::Member(b, m) if m.as_str() == "length" => {
                if let ExprKind::Ident(id) = &b.peel_parens().kind
                    && matches!(self.vars.get(id.as_str()), Some(VarKind::Arr(_)))
                    && st.oct.has(&len_name(id.as_str()))
                {
                    return Some(Lin::var(&len_name(id.as_str())));
                }
                self.havoc_effects(st, e);
                None
            }
            ExprKind::Binary(l, op, r) => {
                let (a, b) = (self.eval(st, l), self.eval(st, r));
                let (a, b) = (a?, b?);
                // unchecked arithmetic wraps at a width this pass does not know: opaque
                if self.unchecked > 0 {
                    return None;
                }
                match op.kind {
                    BinOpKind::Add => Some(a.add(&b)),
                    BinOpKind::Sub => Some(a.sub(&b)),
                    BinOpKind::Mul => match (a.as_constant(), b.as_constant()) {
                        (Some(k), _) => Some(b.scale(k)),
                        (_, Some(k)) => Some(a.scale(k)),
                        _ => None,
                    },
                    _ => None,
                }
            }
            ExprKind::Unary(op, x) => match op.kind {
                UnOpKind::Neg => Some(self.eval(st, x)?.neg()),
                UnOpKind::PreInc | UnOpKind::PreDec | UnOpKind::PostInc | UnOpKind::PostDec => {
                    let ExprKind::Ident(id) = &x.peel_parens().kind else {
                        self.havoc_effects(st, e);
                        return None;
                    };
                    let n = id.as_str().to_string();
                    if !self.is_int(&n) {
                        self.havoc_effects(st, e);
                        return None;
                    }
                    let d = if matches!(op.kind, UnOpKind::PreInc | UnOpKind::PostInc) {
                        1
                    } else {
                        -1
                    };
                    self.assign_var(st, &n, Some(Lin::var(&n).plus(d)));
                    let post = matches!(op.kind, UnOpKind::PostInc | UnOpKind::PostDec);
                    Some(if post { Lin::var(&n).plus(-d) } else { Lin::var(&n) })
                }
                _ => {
                    self.havoc_effects(st, e);
                    None
                }
            },
            ExprKind::Index(b, IndexKind::Index(Some(i))) => {
                let arr = match &b.peel_parens().kind {
                    ExprKind::Ident(id)
                        if matches!(self.vars.get(id.as_str()), Some(VarKind::Arr(_))) =>
                    {
                        id.as_str().to_string()
                    }
                    _ => {
                        self.havoc_effects(st, e);
                        return None;
                    }
                };
                let idx = self.eval(st, i)?;
                // an out-of-bounds read reverts (or, with --strict, fails the proof)
                let len = Lin::var(&len_name(&arr));
                st.oct.assume_le0(&idx.sub(&len).plus(1));
                st.oct.assume_le0(&idx.neg());
                let Some(VarKind::Arr(Some((lo, hi)))) = self.vars.get(&arr).cloned() else {
                    return None;
                };
                let idx = SegBound::of(&idx)?;
                if let Some(c) = st.cells.iter().find(|c| c.arr == arr && c.idx == idx) {
                    return Some(Lin::var(&c.name));
                }
                self.ncells += 1;
                let name = format!("{arr}[{}]#{}", self.ncells, self.ncells);
                st.oct.add_var(&name);
                st.oct.assume_range(&name, &lo, &hi);
                st.cells.push(Cell { name: name.clone(), arr, idx });
                Some(Lin::var(&name))
            }
            ExprKind::Assign(l, op, r) => {
                self.assign(st, l, op.map(|o| o.kind), r);
                match &l.peel_parens().kind {
                    ExprKind::Ident(id) if self.is_int(id.as_str()) => Some(Lin::var(id.as_str())),
                    _ => None,
                }
            }
            // `uint256(x)`: exact when x's range fits
            ExprKind::Call(callee, args) => {
                if let ExprKind::Type(t) = &callee.kind
                    && let Some((lo, hi)) = int_range(t)
                    && let [x] = args.exprs().collect::<Vec<_>>()[..]
                {
                    let v = self.eval(st, x)?;
                    let (vlo, vhi) = st.oct.range(&v);
                    if vlo.is_some_and(|l| l >= lo) && vhi.is_some_and(|h| h <= hi) {
                        return Some(v);
                    }
                    return None;
                }
                self.havoc_effects(st, e);
                None
            }
            _ => {
                self.havoc_effects(st, e);
                None
            }
        }
    }

    /// `st` restricted to the states where `e` is `positive`.
    fn assume<'a>(&mut self, mut st: State, e: &'a Expr<'a>, positive: bool) -> State {
        if st.is_bottom() {
            return st;
        }
        let e = e.peel_parens();
        match &e.kind {
            ExprKind::Unary(op, x) if op.kind == UnOpKind::Not => self.assume(st, x, !positive),
            ExprKind::Lit(l, _) if matches!(l.kind, LitKind::Bool(_)) => {
                let LitKind::Bool(b) = l.kind else { unreachable!() };
                if b == positive { st } else { State::bottom() }
            }
            ExprKind::Binary(l, op, r) if matches!(op.kind, BinOpKind::And | BinOpKind::Or) => {
                let both = (op.kind == BinOpKind::And) == positive;
                if both {
                    let st = self.assume(st, l, positive);
                    self.assume(st, r, positive)
                } else {
                    // a || b: a, or (!a and b); !(a && b): !a, or (a and !b)
                    let first = self.assume(st.clone(), l, positive);
                    let rest = self.assume(st, l, !positive);
                    let second = self.assume(rest, r, positive);
                    self.join(&first, &second)
                }
            }
            ExprKind::Binary(l, op, r)
                if matches!(
                    op.kind,
                    BinOpKind::Lt
                        | BinOpKind::Le
                        | BinOpKind::Gt
                        | BinOpKind::Ge
                        | BinOpKind::Eq
                        | BinOpKind::Ne
                ) =>
            {
                let (a, b) = (self.eval(&mut st, l), self.eval(&mut st, r));
                let (Some(a), Some(b)) = (a, b) else { return st };
                use BinOpKind::*;
                let kind = match (op.kind, positive) {
                    (k, true) => k,
                    (Lt, false) => Ge,
                    (Le, false) => Gt,
                    (Gt, false) => Le,
                    (Ge, false) => Lt,
                    (Eq, false) => Ne,
                    (Ne, false) => Eq,
                    _ => unreachable!(),
                };
                let d = a.sub(&b); // compare d with 0
                match kind {
                    Lt => st.oct.assume_le0(&d.plus(1)),
                    Le => st.oct.assume_le0(&d),
                    Gt => st.oct.assume_le0(&d.neg().plus(1)),
                    Ge => st.oct.assume_le0(&d.neg()),
                    Eq => {
                        st.oct.assume_le0(&d);
                        st.oct.assume_le0(&d.neg());
                    }
                    _ => {
                        // a != b: refutes a == b, and splits only against a constant bound
                        if st.oct.entails_eq(&a, &b) {
                            return State::bottom();
                        }
                        let (lo, hi) = st.oct.range(&d);
                        if lo.as_ref().is_some_and(|l| l.is_zero()) {
                            st.oct.assume_le0(&d.neg().plus(1));
                        } else if hi.as_ref().is_some_and(|h| h.is_zero()) {
                            st.oct.assume_le0(&d.plus(1));
                        }
                    }
                }
                if st.is_bottom() {
                    return State::bottom();
                }
                self.saturate(&mut st);
                st
            }
            _ => {
                self.havoc_effects(&mut st, e);
                st
            }
        }
    }

    // ------------------------------------------------------------------ assignment

    fn assign<'a>(
        &mut self,
        st: &mut State,
        l: &'a Expr<'a>,
        op: Option<BinOpKind>,
        r: &'a Expr<'a>,
    ) {
        let lhs = l.peel_parens();
        let ExprKind::Ident(id) = &lhs.kind else {
            // an array element, struct field, mapping entry or tuple: contents change
            self.eval(st, r);
            self.havoc_effects(st, r);
            if let ExprKind::Tuple(xs) = &lhs.kind {
                for x in xs.iter().filter_map(|x| x.as_ref().unspan()) {
                    for w in crate::walk::root(x) {
                        if let Some(k) = self.vars.get(&w).cloned() {
                            self.kill_var(st, &w);
                            self.restore_range(st, &w, &k);
                        }
                    }
                }
            }
            self.kill_memory(st);
            return;
        };
        let n = id.as_str().to_string();
        match self.vars.get(&n).cloned() {
            Some(VarKind::Int(..)) => {
                // `x = c ? a : b`: each branch on its own
                if op.is_none()
                    && let ExprKind::Ternary(c, a, b) = &r.peel_parens().kind
                {
                    let mut t = self.assume(st.clone(), c, true);
                    let mut f = self.assume(st.clone(), c, false);
                    let va = self.eval(&mut t, a);
                    self.assign_var(&mut t, &n, va);
                    let vb = self.eval(&mut f, b);
                    self.assign_var(&mut f, &n, vb);
                    *st = self.join(&t, &f);
                    return;
                }
                let v = self.eval(st, r);
                let v = match op {
                    None => v,
                    Some(BinOpKind::Add) => v.map(|v| Lin::var(&n).add(&v)),
                    Some(BinOpKind::Sub) => v.map(|v| Lin::var(&n).sub(&v)),
                    Some(BinOpKind::Mul) => {
                        v.and_then(|v| v.as_constant().map(|k| Lin::var(&n).scale(k)))
                    }
                    _ => None,
                };
                self.assign_var(st, &n, v);
            }
            Some(VarKind::Arr(_)) => {
                // `ys = new T[](n)` sets the length; any other array value: unknown length
                let len = match &r.peel_parens().kind {
                    ExprKind::Call(callee, args) if matches!(callee.kind, ExprKind::New(_)) => {
                        args.exprs().next().and_then(|n| self.eval(st, n))
                    }
                    _ => {
                        self.havoc_effects(st, r);
                        None
                    }
                };
                self.kill_var(st, &n);
                let l = len_name(&n);
                st.oct.add_var(&l);
                match len {
                    Some(v) => st.oct.assign(&l, &v),
                    None => st.oct.assume_range(&l, &BigInt::zero(), &(pow2(256) - 1)),
                }
            }
            None => {
                self.eval(st, r);
                self.havoc_effects(st, r);
            }
        }
    }

    /// `n := v` (None: an unknown value of n's type). Checked arithmetic reverts out of range;
    /// unchecked arithmetic wraps, which the analysis models only when it cannot happen.
    fn assign_var(&mut self, st: &mut State, n: &str, v: Option<Lin>) {
        if st.is_bottom() {
            return;
        }
        let Some(VarKind::Int(lo, hi)) = self.vars.get(n).cloned() else { return };
        self.saturate(st);
        let v = v.filter(|v| {
            if self.unchecked == 0 {
                return true;
            }
            let (vl, vh) = st.oct.range(v);
            vl.is_some_and(|l| l >= lo) && vh.is_some_and(|h| h <= hi)
        });
        let old = st.oct.clone();
        let x = Lin::var(n);
        // segment facts through the assignment
        let shift = v.as_ref().and_then(|v| {
            (v.terms.len() == 1 && v.terms.get(n).is_some_and(|c| c.is_one()))
                .then(|| v.constant.clone())
        });
        let mut facts = vec![];
        for f in std::mem::take(&mut st.facts) {
            let mut f = f;
            // bounds over n
            let mut ok = true;
            for b in [&mut f.lo, &mut f.hi] {
                if !b.mentions(n) {
                    continue;
                }
                match &shift {
                    Some(c) => b.k -= c,
                    None => match Self::rewrite_bound(&old, n, b) {
                        Some(nb) => *b = nb,
                        None => ok = false,
                    },
                }
            }
            if !ok {
                continue;
            }
            if f.val == n {
                let Some(v) = &v else { continue };
                let keep = match f.kind {
                    Kind::Le => old.entails_le0(&x.sub(v)),
                    Kind::Ge => old.entails_le0(&v.sub(&x)),
                    Kind::Eq => old.entails_eq(&x, v),
                };
                if !keep {
                    continue;
                }
            }
            facts.push(f);
        }
        // `n := w`: n inherits w's facts
        if let Some(v) = &v
            && v.constant.is_zero()
            && v.terms.len() == 1
            && let Some((w, c)) = v.terms.iter().next()
            && c.is_one()
            && w != n
        {
            for f in &facts.clone() {
                if f.val == *w {
                    facts.push(Fact { val: n.to_string(), ..f.clone() });
                }
            }
        }
        st.facts = facts;
        // cells indexed by n
        let dead: Vec<String> =
            st.cells.iter().filter(|c| c.idx.mentions(n)).map(|c| c.name.clone()).collect();
        for d in dead {
            self.drop_cell(st, &d);
        }
        match v {
            Some(v) => st.oct.assign(n, &v),
            None => st.oct.forget(n),
        }
        st.oct.assume_range(n, &lo, &hi);
        if st.is_bottom() {
            *st = State::bottom();
            return;
        }
        self.saturate(st);
    }

    /// A bound `n + k` rewritten, over the octagon before n changes, without n.
    fn rewrite_bound(old: &Octagon, n: &str, b: &SegBound) -> Option<SegBound> {
        let x = Lin::var(n);
        if let (Some(lo), Some(hi)) = old.range(&x)
            && lo == hi
        {
            return Some(SegBound { var: None, k: lo + &b.k });
        }
        for w in old.names() {
            if w == n || w.contains('#') {
                continue;
            }
            let d = x.sub(&Lin::var(w));
            if let (Some(lo), Some(hi)) = old.range(&d)
                && lo == hi
            {
                return Some(SegBound { var: Some(w.clone()), k: lo + &b.k });
            }
        }
        None
    }

    // ------------------------------------------------------------------ segment facts

    /// Adds the point facts the cells' relations to scalars give, merges adjacent segments and
    /// drops facts that others imply.
    fn saturate(&mut self, st: &mut State) {
        if st.is_bottom() {
            return;
        }
        let scalars: Vec<String> =
            st.oct.names().iter().filter(|n| !n.contains('#') && self.is_int(n)).cloned().collect();
        for c in st.cells.clone() {
            let cl = Lin::var(&c.name);
            for v in &scalars {
                let vl = Lin::var(v);
                let lo = c.idx.clone();
                let hi = SegBound { k: &c.idx.k + 1, ..c.idx.clone() };
                let le = st.oct.entails_le0(&cl.sub(&vl));
                let ge = st.oct.entails_le0(&vl.sub(&cl));
                for (holds, kind) in [(le, Kind::Le), (ge, Kind::Ge), (le && ge, Kind::Eq)] {
                    if holds {
                        st.facts.push(Fact {
                            kind,
                            arr: c.arr.clone(),
                            val: v.clone(),
                            lo: lo.clone(),
                            hi: hi.clone(),
                        });
                    }
                }
            }
        }
        // merge forall segments that meet or overlap
        let mut changed = true;
        while changed {
            changed = false;
            let fs = st.facts.clone();
            'outer: for (i, f) in fs.iter().enumerate() {
                if f.kind == Kind::Eq {
                    continue;
                }
                for (j, g) in fs.iter().enumerate() {
                    if i == j || g.kind != f.kind || g.arr != f.arr || g.val != f.val {
                        continue;
                    }
                    // f = [a, b), g = [c, d) with a <= c <= b: [a, max(b, d))
                    let (a, b, c, d) = (f.lo.lin(), f.hi.lin(), g.lo.lin(), g.hi.lin());
                    if st.oct.entails_le0(&a.sub(&c)) && st.oct.entails_le0(&c.sub(&b)) {
                        let hi = if st.oct.entails_le0(&b.sub(&d)) {
                            g.hi.clone()
                        } else if st.oct.entails_le0(&d.sub(&b)) {
                            f.hi.clone()
                        } else {
                            continue;
                        };
                        let m = Fact { hi, ..f.clone() };
                        if !fs.contains(&m) {
                            st.facts.push(m);
                            changed = true;
                            break 'outer;
                        }
                    }
                }
            }
            if st.facts.len() > 4 * MAX_FACTS {
                break;
            }
        }
        self.prune(st);
    }

    /// Does `g` (in state `oct`) imply `f`?
    fn implies(oct: &Octagon, g: &Fact, f: &Fact) -> bool {
        if g.kind != f.kind || g.arr != f.arr || g.val != f.val {
            return false;
        }
        let (gl, gh, fl, fh) = (g.lo.lin(), g.hi.lin(), f.lo.lin(), f.hi.lin());
        match f.kind {
            // forall over a larger segment implies it over a smaller one
            Kind::Le | Kind::Ge => oct.entails_le0(&gl.sub(&fl)) && oct.entails_le0(&fh.sub(&gh)),
            // exists over a smaller segment implies it over a larger one
            Kind::Eq => oct.entails_le0(&fl.sub(&gl)) && oct.entails_le0(&gh.sub(&fh)),
        }
    }

    /// Is the forall fact `f` vacuous in `oct` (its segment empty)?
    fn vacuous(oct: &Octagon, f: &Fact) -> bool {
        f.kind != Kind::Eq && oct.entails_le0(&f.hi.lin().sub(&f.lo.lin()))
    }

    fn prune(&mut self, st: &mut State) {
        let mut fs: Vec<Fact> = std::mem::take(&mut st.facts);
        fs.sort();
        fs.dedup();
        fs.retain(|f| {
            let known = |b: &SegBound| b.var.as_ref().is_none_or(|v| st.oct.has(v));
            known(&f.lo) && known(&f.hi) && st.oct.has(&f.val) && !Self::vacuous(&st.oct, f)
        });
        let mut out: Vec<Fact> = vec![];
        for (i, f) in fs.iter().enumerate() {
            let dominated = fs.iter().enumerate().any(|(j, g)| {
                j != i && Self::implies(&st.oct, g, f) && !(Self::implies(&st.oct, f, g) && j > i)
            });
            if !dominated {
                out.push(f.clone());
            }
        }
        out.truncate(MAX_FACTS);
        st.facts = out;
    }

    /// Facts that hold in both states: candidate segments from either side's bounds, checked in
    /// each side's octagon (a forall over an empty segment holds vacuously).
    fn join_facts(a: &State, b: &State) -> Vec<Fact> {
        let mut out = vec![];
        let holds = |s: &State, f: &Fact| {
            Self::vacuous(&s.oct, f) || s.facts.iter().any(|g| Self::implies(&s.oct, g, f))
        };
        let mut cands: Vec<Fact> = vec![];
        for f in a.facts.iter().chain(&b.facts) {
            for g in a.facts.iter().chain(&b.facts) {
                if g.kind != f.kind || g.arr != f.arr || g.val != f.val {
                    continue;
                }
                let zero = SegBound { var: None, k: BigInt::zero() };
                for lo in [&f.lo, &g.lo, &zero] {
                    for hi in [&f.hi, &g.hi] {
                        cands.push(Fact { lo: lo.clone(), hi: hi.clone(), ..f.clone() });
                    }
                }
            }
        }
        cands.sort();
        cands.dedup();
        for c in cands {
            if holds(a, &c) && holds(b, &c) {
                out.push(c);
            }
        }
        out
    }

    fn join(&mut self, a: &State, b: &State) -> State {
        if a.is_bottom() {
            return b.clone();
        }
        if b.is_bottom() {
            return a.clone();
        }
        // cells survive only where both sides read the same cell under the same name
        let mut a = a.clone();
        let mut b = b.clone();
        for c in a.cells.clone() {
            if !b.cells.contains(&c) {
                self.drop_cell(&mut a, &c.name);
            }
        }
        for c in b.cells.clone() {
            if !a.cells.contains(&c) {
                self.drop_cell(&mut b, &c.name);
            }
        }
        let facts = Self::join_facts(&a, &b);
        let mut out = State { oct: a.oct.join(&b.oct), facts, cells: a.cells.clone() };
        self.prune(&mut out);
        out
    }

    fn leq(a: &State, b: &State) -> bool {
        if a.is_bottom() {
            return true;
        }
        if b.is_bottom() {
            return false;
        }
        a.oct.leq(&b.oct)
            && b.facts.iter().all(|f| a.facts.iter().any(|g| Self::implies(&a.oct, g, f)))
    }

    fn widen(&mut self, old: &State, new: &State) -> State {
        let mut j = self.join(old, new);
        j.oct = old.oct.widen(&j.oct, &self.thresholds);
        j.oct.close();
        // facts only shrink once the octagon is stable; keep those that still hold
        j.facts
            .retain(|f| old.facts.iter().any(|g| Self::implies(&old.oct, g, f)) || old.is_bottom());
        self.prune(&mut j);
        j
    }

    // ------------------------------------------------------------------ statements

    fn block<'a>(&mut self, mut st: State, stmts: &'a [Stmt<'a>]) -> State {
        self.scopes.push(vec![]);
        for s in stmts {
            st = self.stmt(st, s);
        }
        let scope = self.scopes.pop().unwrap();
        for n in scope {
            self.kill_var(&mut st, &n);
            st.oct.remove_var(&n);
            st.oct.remove_var(&len_name(&n));
            self.vars.remove(&n);
        }
        st
    }

    fn stmt<'a>(&mut self, mut st: State, s: &'a Stmt<'a>) -> State {
        if st.is_bottom() {
            // still record nothing: unreachable code needs no invariants
            return st;
        }
        match &s.kind {
            StmtKind::Block(b) => self.block(st, b.stmts),
            StmtKind::UncheckedBlock(b) => {
                self.unchecked += 1;
                let st = self.block(st, b.stmts);
                self.unchecked -= 1;
                st
            }
            StmtKind::DeclSingle(d) => {
                let Some(name) = d.name else { return st };
                let name = name.as_str().to_string();
                let Some(kind) = var_kind(d, false) else {
                    if let Some(i) = &d.initializer {
                        self.eval(&mut st, i);
                        self.havoc_effects(&mut st, i);
                    }
                    // a declaration of an untracked type shadows any tracked one
                    if self.vars.contains_key(&name) {
                        self.kill_var(&mut st, &name);
                        st.oct.remove_var(&name);
                        self.vars.remove(&name);
                    }
                    return st;
                };
                let init = d.initializer.as_deref();
                match kind {
                    VarKind::Int(..) => {
                        let v = match init {
                            Some(i) => self.eval(&mut st, i).map(Some),
                            None => Some(Some(Lin::constant(BigInt::zero()))),
                        };
                        self.declare(&mut st, &name, kind);
                        match init {
                            Some(i) if matches!(i.peel_parens().kind, ExprKind::Ternary(..)) => {
                                // reuse the assignment's per-branch handling
                                let ExprKind::Ternary(c, a, b) = &i.peel_parens().kind else {
                                    unreachable!()
                                };
                                let mut t = self.assume(st.clone(), c, true);
                                let mut f = self.assume(st.clone(), c, false);
                                let va = self.eval(&mut t, a);
                                self.assign_var(&mut t, &name, va);
                                let vb = self.eval(&mut f, b);
                                self.assign_var(&mut f, &name, vb);
                                st = self.join(&t, &f);
                            }
                            _ => self.assign_var(&mut st, &name, v.flatten()),
                        }
                    }
                    VarKind::Arr(_) => {
                        let len = match init.map(|i| &i.peel_parens().kind) {
                            Some(ExprKind::Call(callee, args))
                                if matches!(callee.kind, ExprKind::New(_)) =>
                            {
                                args.exprs().next().and_then(|n| self.eval(&mut st, n))
                            }
                            Some(_) => {
                                self.havoc_effects(&mut st, init.unwrap());
                                None
                            }
                            None => Some(Lin::constant(BigInt::zero())),
                        };
                        self.declare(&mut st, &name, kind);
                        if let Some(l) = len {
                            st.oct.assign(&len_name(&name), &l);
                        }
                    }
                }
                st
            }
            StmtKind::DeclMulti(defs, e) => {
                self.eval(&mut st, e);
                self.havoc_effects(&mut st, e);
                for d in defs.iter().filter_map(|d| d.as_ref().unspan()) {
                    if let Some(n) = d.name {
                        let n = n.as_str().to_string();
                        if let Some(k) = var_kind(d, false) {
                            self.declare(&mut st, &n, k);
                        } else if self.vars.remove(&n).is_some() {
                            self.kill_var(&mut st, &n);
                            st.oct.remove_var(&n);
                        }
                    }
                }
                st
            }
            StmtKind::Expr(e) => {
                if let ExprKind::Call(callee, args) = &e.peel_parens().kind
                    && let ExprKind::Ident(f) = &callee.peel_parens().kind
                    && matches!(f.as_str(), "require" | "assert")
                    && let Some(c) = args.exprs().next()
                {
                    for extra in args.exprs().skip(1) {
                        self.havoc_effects(&mut st, extra);
                    }
                    return self.assume(st, c, true);
                }
                if let ExprKind::Call(callee, _) = &e.peel_parens().kind
                    && let ExprKind::Ident(f) = &callee.peel_parens().kind
                    && f.as_str() == "revert"
                {
                    return State::bottom();
                }
                self.eval(&mut st, e);
                st
            }
            StmtKind::If(c, t, e) => {
                let ts = self.assume(st.clone(), c, true);
                let fs = self.assume(st, c, false);
                let ts = self.stmt(ts, t);
                let fs = match e {
                    Some(e) => self.stmt(fs, e),
                    None => fs,
                };
                self.join(&ts, &fs)
            }
            StmtKind::While(c, body) => self.exec_loop(st, s.span, Some(c), None, body),
            StmtKind::For { init, cond, next, body } => {
                self.scopes.push(vec![]);
                if let Some(i) = init {
                    st = self.stmt(st, i);
                }
                let mut st = self.exec_loop(st, s.span, cond.as_deref(), next.as_deref(), body);
                for n in self.scopes.pop().unwrap() {
                    self.kill_var(&mut st, &n);
                    st.oct.remove_var(&n);
                    st.oct.remove_var(&len_name(&n));
                    self.vars.remove(&n);
                }
                st
            }
            StmtKind::Return(e) => {
                if let Some(e) = e {
                    self.eval(&mut st, e);
                }
                State::bottom()
            }
            StmtKind::Revert(..) => State::bottom(),
            StmtKind::Break => {
                if let Some(f) = self.loops.last_mut() {
                    f.breaks.push(st);
                }
                State::bottom()
            }
            StmtKind::Continue => {
                if let Some(f) = self.loops.last_mut() {
                    f.conts.push(st);
                }
                State::bottom()
            }
            StmtKind::Emit(_, args) => {
                for a in args.exprs() {
                    self.eval(&mut st, a);
                }
                st
            }
            // assembly, try/catch, do-while, placeholders: forget everything they could change
            _ => {
                self.havoc_all(&mut st);
                st
            }
        }
    }

    /// One pass over the loop from head state `head`: the state back at the head, and the
    /// states that break out.
    fn iterate<'a>(
        &mut self,
        head: &State,
        cond: Option<&'a Expr<'a>>,
        next: Option<&'a Expr<'a>>,
        body: &'a Stmt<'a>,
    ) -> (State, Vec<State>) {
        let mut st = head.clone();
        if let Some(c) = cond {
            st = self.assume(st, c, true);
        }
        self.loops.push(LoopFrame { breaks: vec![], conts: vec![] });
        let mut st = self.stmt(st, body);
        let frame = self.loops.pop().unwrap();
        for c in &frame.conts {
            st = self.join(&st, c);
        }
        if let Some(n) = next
            && !st.is_bottom()
        {
            self.eval(&mut st, n);
        }
        (st, frame.breaks)
    }

    fn exec_loop<'a>(
        &mut self,
        entry: State,
        span: Span,
        cond: Option<&'a Expr<'a>>,
        next: Option<&'a Expr<'a>>,
        body: &'a Stmt<'a>,
    ) -> State {
        let mut entry = entry;
        self.saturate(&mut entry);
        let mut head = entry.clone();
        let mut stable = false;
        for k in 0..MAX_ITERATIONS {
            let (back, _) = self.iterate(&head, cond, next, body);
            let new = self.join(&entry, &back);
            if Self::leq(&new, &head) {
                stable = true;
                break;
            }
            head = if k < WIDEN_DELAY { self.join(&head, &new) } else { self.widen(&head, &new) };
        }
        if !stable {
            // no fixpoint: forget everything the loop may write
            head = entry.clone();
            self.havoc_all(&mut head);
            let (back, _) = self.iterate(&head, cond, next, body);
            head = self.join(&head, &back);
        }
        // descending iterations, each kept only while it stays a post-fixpoint
        for _ in 0..2 {
            let (back, _) = self.iterate(&head, cond, next, body);
            let cand = self.join(&entry, &back);
            let (back2, _) = self.iterate(&cand, cond, next, body);
            if Self::leq(&self.join(&entry, &back2), &cand) && Self::leq(&cand, &head) {
                head = cand;
            } else {
                break;
            }
        }
        // the final pass records the nested loops' invariants under this head
        let (_, breaks) = self.iterate(&head, cond, next, body);
        let texts = self.render(&head, &entry, body, next);
        self.out.insert(span, texts);
        let mut exit = match cond {
            Some(c) => self.assume(head.clone(), c, false),
            None => State::bottom(),
        };
        for b in &breaks {
            exit = self.join(&exit, b);
        }
        exit
    }

    // ------------------------------------------------------------------ rendering

    fn render<'a>(
        &self,
        head: &State,
        _entry: &State,
        body: &'a Stmt<'a>,
        next: Option<&'a Expr<'a>>,
    ) -> Vec<String> {
        if head.is_bottom() {
            return vec![];
        }
        let mut modified: FxHashSet<String> = FxHashSet::default();
        let note = |e: &'a Expr<'a>, m: &mut FxHashSet<String>| {
            crate::walk::sub_exprs(e, &mut |x| match &x.kind {
                ExprKind::Assign(l, ..) => m.extend(crate::walk::root(l)),
                ExprKind::Unary(_, x) => m.extend(crate::walk::root(x)),
                _ => {}
            })
        };
        crate::walk::all_stmts(body, &mut |s| {
            crate::walk::stmt_exprs(s, &mut |e| note(e, &mut modified));
            if let StmtKind::Assembly(_) = s.kind {
                modified.insert("*".into());
            }
        });
        if let Some(n) = next {
            note(n, &mut modified);
        }
        let all = modified.contains("*");
        let names: Vec<String> = head.oct.names().to_vec();
        let name_of = |n: &str| -> Option<String> {
            if n.contains('#') {
                return None;
            }
            Some(n.to_string())
        };
        let is_mod = |n: &str| all || modified.contains(n.trim_end_matches(".length"));
        let ty_range = |n: &str| -> (BigInt, BigInt) {
            match self.vars.get(n) {
                Some(VarKind::Int(lo, hi)) => (lo.clone(), hi.clone()),
                _ => (BigInt::zero(), pow2(256) - 1),
            }
        };
        let mut out = vec![];
        let cons = head.oct.constraints();
        // equalities first
        let mut done: FxHashSet<(usize, Option<usize>)> = FxHashSet::default();
        for (x, sx, y, c) in &cons {
            let Some(xn) = name_of(&names[*x]) else { continue };
            match y {
                None => {
                    if !is_mod(&xn) {
                        continue;
                    }
                    let (lo, hi) = ty_range(&xn);
                    let (l, h) = head.oct.range(&Lin::var(&names[*x]));
                    if let Some(v) = &l
                        && l == h
                    {
                        if done.insert((*x, None)) {
                            out.push(format!("{xn} == {v}"));
                        }
                        continue;
                    }
                    if *sx && *c < hi {
                        out.push(format!("{xn} <= {c}"));
                    } else if !*sx && -c.clone() > lo {
                        out.push(format!("{xn} >= {}", -c.clone()));
                    }
                }
                Some((y, sy)) => {
                    let Some(yn) = name_of(&names[*y]) else { continue };
                    if !is_mod(&xn) && !is_mod(&yn) {
                        continue;
                    }
                    // skip what the variables' own ranges imply
                    let mut lin =
                        if *sx { Lin::var(&names[*x]) } else { Lin::var(&names[*x]).neg() };
                    lin = lin.add(&if *sy {
                        Lin::var(&names[*y])
                    } else {
                        Lin::var(&names[*y]).neg()
                    });
                    let box_ub = {
                        let (xl, xh) = head.oct.range(&Lin::var(&names[*x]));
                        let (yl, yh) = head.oct.range(&Lin::var(&names[*y]));
                        let px = if *sx { xh } else { xl.map(|v| -v) };
                        let py = if *sy { yh } else { yl.map(|v| -v) };
                        match (px, py) {
                            (Some(a), Some(b)) => Some(a + b),
                            _ => None,
                        }
                    };
                    if box_ub.as_ref().is_some_and(|b| b <= c) {
                        continue;
                    }
                    let lower = head.oct.upper(&lin.neg()).map(|b| -b);
                    let pretty = match (sx, sy) {
                        (true, true) => format!("{xn} + {yn}"),
                        (true, false) => format!("{xn} - {yn}"),
                        (false, true) => format!("{yn} - {xn}"),
                        (false, false) => {
                            // -x - y <= c: x + y >= -c
                            if lower.as_ref() == Some(c) {
                                continue;
                            }
                            out.push(format!("{xn} + {yn} >= {}", -c.clone()));
                            continue;
                        }
                    };
                    if lower.as_ref() == Some(c) {
                        if done.insert((*x, Some(*y))) {
                            out.push(format!("{pretty} == {c}"));
                        }
                        continue;
                    }
                    out.push(format!("{pretty} <= {c}"));
                }
            }
        }
        // segment facts that the loop changes
        for f in &head.facts {
            let mentions_mod = is_mod(&f.val)
                || f.lo.var.as_deref().is_some_and(is_mod)
                || f.hi.var.as_deref().is_some_and(is_mod);
            if !mentions_mod {
                continue;
            }
            let b = |s: &SegBound| match (&s.var, s.k.is_zero()) {
                (None, _) => s.k.to_string(),
                (Some(v), true) => v.clone(),
                (Some(v), false) if s.k.is_negative() => format!("{v} - {}", -s.k.clone()),
                (Some(v), false) => format!("{v} + {}", s.k),
            };
            let j = "__inv_j";
            let range = format!("{} <= {j} && {j} < {}", b(&f.lo), b(&f.hi));
            out.push(match f.kind {
                Kind::Le => {
                    format!("forall uint256 {j} :: {range} ==> {}[{j}] <= {}", f.arr, f.val)
                }
                Kind::Ge => {
                    format!("forall uint256 {j} :: {range} ==> {}[{j}] >= {}", f.arr, f.val)
                }
                Kind::Eq => format!("exists uint256 {j} :: {range} && {}[{j}] == {}", f.arr, f.val),
            });
        }
        out.sort();
        out.dedup();
        out
    }
}
