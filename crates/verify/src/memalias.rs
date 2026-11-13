//! Memory references that may share memory.
//!
//! The executor models memory structs and arrays as values: `b = a` copies. In Solidity they are
//! references, so after `b = a` (or passing `a` to an internal function, or storing it in
//! another memory value) a write through `b` changes what `a` reads. Rather than model memory
//! as a heap, [`check`] rejects every in-place write (`v[i] = x`, `v.f = x`, `delete v.f`,
//! `v.f++`) through a memory reference `v` that may share its memory with another reference.
//! A write through a reference that owns its memory gives the same results under both models.
//!
//! `v` owns its memory when every value it holds was freshly allocated (a struct constructor,
//! `new`, an array literal, `abi.decode`, no initializer, or a copy out of storage), and
//! no reference into it (`v`, `v.inner`, `v[i]` of a struct or array type) is used anywhere but
//! as the base of a further access, as an assignment target, as a returned value, or as an
//! argument that is copied (`abi.*`, hashing, events, a write to storage), at any point before
//! the write or in a loop around it. A container that receives such a reference shares too. The
//! analysis is over names, in source order, and counts every statement of a modifier as both
//! before and after the body, so it only ever rejects more.

use crate::{
    prog::{Contract, Func, Program},
    ty::Ty,
    vc::ty_of,
};
use solar_parse::{
    ast::{self, Expr, ExprKind, Stmt, StmtKind, UnOpKind},
    interface::{
        Span,
        data_structures::map::{FxHashMap, FxHashSet},
    },
};

type R<T> = Result<T, (Span, String)>;

struct Pass<'p, 'a> {
    prog: &'p Program<'a>,
    /// Memory locals and parameters of a struct or array type.
    mem: FxHashMap<String, Ty>,
    /// Storage variables and storage pointers: writes to them copy.
    storage: FxHashSet<String>,
    /// Memory references that may hold memory not freshly allocated for them.
    borrowed: FxHashSet<String>,
    /// Where a reference into a memory reference escaped: the reference, the position, and the
    /// loops around it.
    shares: Vec<(String, usize, Vec<usize>)>,
    /// In-place writes: the reference written through, the position, the loops around it.
    writes: Vec<(String, usize, Vec<usize>, Span)>,
    /// The position of the statement being read (0 and usize::MAX for modifiers).
    pos: usize,
    loops: Vec<usize>,
    modifier: bool,
}

fn is_ref(t: &Ty) -> bool {
    matches!(t, Ty::Struct(_) | Ty::Arr(..))
}

/// Rejects in-place writes through memory references of `f`'s body and modifiers that may share
/// their memory. `params_own`: `f` runs as an entry point, whose memory parameters are fresh
/// copies of the call's arguments.
pub(crate) fn check<'p, 'a>(
    prog: &'p Program<'a>,
    c: &'p Contract<'a>,
    f: &'p Func<'a>,
    params_own: bool,
) -> R<()> {
    let mut p = Pass {
        prog,
        mem: FxHashMap::default(),
        storage: c.state.iter().map(|s| s.name.clone()).collect(),
        borrowed: FxHashSet::default(),
        shares: vec![],
        writes: vec![],
        pos: 1,
        loops: vec![],
        modifier: false,
    };
    let mut bodies: Vec<(&'a ast::Block<'a>, bool)> = vec![];
    if let Some(b) = &f.item.body {
        bodies.push((b, false));
    }
    let mut params: Vec<(&'a ast::VariableDefinition<'a>, bool)> = vec![];
    // A function's own memory parameters are written back to the caller's argument after an
    // inlined call (see `Vc::write_back_storage_params`), so they count as owned here; two
    // arguments naming one object are refused at the call.
    let _ = params_own;
    for v in f.item.header.parameters.iter() {
        params.push((v, false));
    }
    for v in f.item.header.returns().iter() {
        params.push((v, false));
    }
    for m in f.item.header.modifiers.iter() {
        if let Some(md) = c.modifiers.get(m.name.last().as_str()) {
            if let Some(b) = &md.body {
                bodies.push((b, true));
            }
            for v in md.header.parameters.iter() {
                params.push((v, true));
            }
        }
    }
    for (v, borrowed) in params {
        p.declare(v, borrowed);
    }
    let mut stmts: Vec<&'a Stmt<'a>> = vec![];
    for (b, _) in &bodies {
        for s in b.stmts.iter() {
            crate::walk::all_stmts(s, &mut |s| stmts.push(s));
        }
    }
    for s in &stmts {
        match &s.kind {
            StmtKind::DeclSingle(v) => {
                let fresh = v.initializer.as_deref().is_none_or(|i| p.fresh(i));
                p.declare(v, !fresh);
            }
            StmtKind::DeclMulti(vs, _) => {
                for v in vs.iter().filter_map(|v| v.as_ref().unspan()) {
                    p.declare(v, true);
                }
            }
            _ => {}
        }
    }
    for (b, modifier) in &bodies {
        for s in b.stmts.iter() {
            p.walk(s, *modifier);
        }
    }
    for (v, w, wl, span) in &p.writes {
        let shared = p
            .shares
            .iter()
            .any(|(r, at, sl)| r == v && (at < w || sl.iter().any(|l| wl.contains(l))));
        if p.borrowed.contains(v) || shared {
            return Err((
                *span,
                format!(
                    "write through memory reference `{v}`, which may share its memory with another reference (memory structs and arrays are modelled as values)"
                ),
            ));
        }
    }
    Ok(())
}

impl<'p, 'a> Pass<'p, 'a> {
    /// Reads statement s and the statements nested in it, in source order, with the loops
    /// around each.
    fn walk(&mut self, s: &'a Stmt<'a>, modifier: bool) {
        self.pos += 1;
        self.stmt(s, modifier);
        let is_loop =
            matches!(s.kind, StmtKind::For { .. } | StmtKind::While(..) | StmtKind::DoWhile(..));
        if is_loop {
            self.loops.push(self.pos);
        }
        let mut subs = vec![];
        crate::walk::sub_stmts(s, &mut |x| subs.push(x));
        for x in subs {
            self.walk(x, modifier);
        }
        if is_loop {
            self.loops.pop();
        }
    }

    fn share(&mut self, r: String) {
        // A modifier's statements may run before the body.
        let at = if self.modifier { 0 } else { self.pos };
        self.shares.push((r, at, self.loops.clone()));
    }

    fn declare(&mut self, v: &'a ast::VariableDefinition<'a>, borrowed: bool) {
        let Some(n) = v.name.map(|n| n.as_str().to_string()) else { return };
        match v.data_location {
            Some(ast::DataLocation::Storage) => {
                self.storage.insert(n);
            }
            Some(ast::DataLocation::Memory) => {
                let ty = ty_of(self.prog, &v.ty);
                if is_ref(&ty) {
                    self.mem.insert(n.clone(), ty);
                    if borrowed {
                        self.borrowed.insert(n);
                    }
                }
            }
            _ => {}
        }
    }

    /// The type of an access chain rooted at a memory reference, and that reference.
    fn chain(&self, e: &'a Expr<'a>) -> Option<(String, Ty)> {
        match &e.peel_parens().kind {
            ExprKind::Ident(i) => {
                self.mem.get(i.as_str()).map(|t| (i.as_str().to_string(), t.clone()))
            }
            ExprKind::Member(b, m) => {
                let (r, t) = self.chain(b)?;
                let Ty::Struct(sn) = t else { return None };
                let ft = self.prog.struct_fields(&sn)?.iter().find(|(f, _)| f == m.as_str())?;
                Some((r, ft.1.clone()))
            }
            ExprKind::Index(b, ast::IndexKind::Index(_)) => {
                let (r, t) = self.chain(b)?;
                let Ty::Arr(el, _) = t else { return None };
                Some((r, *el))
            }
            _ => None,
        }
    }

    /// A reference into a memory reference: the reference it is rooted at.
    fn ref_root(&self, e: &'a Expr<'a>) -> Option<String> {
        self.chain(e).filter(|(_, t)| is_ref(t)).map(|(r, _)| r)
    }

    fn is_storage(&self, e: &'a Expr<'a>) -> bool {
        crate::walk::root(e).iter().any(|r| self.storage.contains(r))
    }

    /// Whether a value of a struct or array type is freshly allocated: what a variable it
    /// initializes then owns.
    fn fresh(&self, e: &'a Expr<'a>) -> bool {
        let mut inner_ref = false;
        crate::walk::sub_exprs(e, &mut |x| inner_ref |= self.ref_root(x).is_some());
        if inner_ref {
            return false;
        }
        match &e.peel_parens().kind {
            ExprKind::Array(_) => true,
            ExprKind::Call(callee, _) => match &callee.peel_parens().kind {
                ExprKind::New(_) => true,
                ExprKind::Ident(i) => self.prog.struct_fields(i.as_str()).is_some(),
                ExprKind::Member(b, m) => {
                    matches!(&b.kind, ExprKind::Ident(i) if i.as_str() == "abi")
                        && m.as_str() == "decode"
                }
                _ => false,
            },
            ExprKind::Ident(_) | ExprKind::Member(..) | ExprKind::Index(..) => self.is_storage(e),
            _ => false,
        }
    }

    fn stmt(&mut self, s: &'a Stmt<'a>, modifier: bool) {
        self.modifier = modifier;
        match &s.kind {
            StmtKind::Return(Some(e)) => match &e.peel_parens().kind {
                ExprKind::Tuple(xs) => {
                    for x in xs.iter().filter_map(|x| x.as_ref().unspan()) {
                        self.expr(x, true);
                    }
                }
                _ => self.expr(e, true),
            },
            StmtKind::Emit(_, args) => {
                for a in args.exprs() {
                    self.expr(a, true);
                }
            }
            StmtKind::Expr(e) => self.expr(e, true),
            StmtKind::DeclSingle(v) => {
                if let Some(i) = &v.initializer {
                    let copy = v.data_location == Some(ast::DataLocation::Storage);
                    self.expr(i, copy);
                }
            }
            StmtKind::For { cond, next, .. } => {
                for e in [cond, next].into_iter().flatten() {
                    self.expr(e, false);
                }
            }
            StmtKind::DeclMulti(_, e) => self.expr(e, false),
            _ => crate::walk::stmt_exprs(s, &mut |e| self.expr(e, false)),
        }
    }

    /// `ok`: a reference here is copied (or only read), so it does not escape.
    fn expr(&mut self, e: &'a Expr<'a>, ok: bool) {
        if !ok && let Some(r) = self.ref_root(e) {
            self.share(r);
        }
        match &e.peel_parens().kind {
            ExprKind::Ident(_) | ExprKind::Lit(..) => {}
            ExprKind::Member(b, _) => self.expr(b, true),
            ExprKind::Index(b, k) => {
                self.expr(b, true);
                match k {
                    ast::IndexKind::Index(Some(i)) => self.expr(i, false),
                    ast::IndexKind::Index(None) => {}
                    ast::IndexKind::Range(lo, hi) => {
                        for x in [lo, hi].into_iter().flatten() {
                            self.expr(x, false);
                        }
                    }
                }
            }
            ExprKind::Assign(l, op, r) => {
                self.target(l);
                if op.is_none()
                    && let ExprKind::Ident(i) = &l.peel_parens().kind
                    && self.mem.contains_key(i.as_str())
                    && !self.fresh(r)
                {
                    self.borrowed.insert(i.as_str().to_string());
                }
                let copy = self.is_storage(l);
                // A container that receives a reference shares it.
                if !copy
                    && !self.fresh(r)
                    && let Some(w) = self.ref_root(l)
                    && !matches!(l.peel_parens().kind, ExprKind::Ident(_))
                {
                    self.share(w);
                }
                self.expr(r, copy);
            }
            ExprKind::Unary(op, x)
                if matches!(
                    op.kind,
                    UnOpKind::PreInc | UnOpKind::PostInc | UnOpKind::PreDec | UnOpKind::PostDec
                ) =>
            {
                self.target(x)
            }
            ExprKind::Delete(x) => self.target(x),
            ExprKind::Call(callee, args) => {
                let copies = self.copies(callee);
                match &callee.peel_parens().kind {
                    // `v.f(..)` through `using for` passes `v` along.
                    ExprKind::Member(b, _) => self.expr(b, false),
                    ExprKind::CallOptions(c, _) => {
                        if let ExprKind::Member(b, _) = &c.peel_parens().kind {
                            self.expr(b, false);
                        }
                    }
                    _ => {}
                }
                for a in args.exprs() {
                    self.expr(a, copies);
                }
            }
            _ => crate::walk::sub_exprs_direct(e, &mut |x| self.expr(x, false)),
        }
    }

    /// An assignment target: a write through a memory reference when it is an access into one.
    fn target(&mut self, l: &'a Expr<'a>) {
        match &l.peel_parens().kind {
            ExprKind::Tuple(xs) => {
                for x in xs.iter().filter_map(|x| x.as_ref().unspan()) {
                    self.target(x);
                }
            }
            ExprKind::Ident(_) => {}
            ExprKind::Member(..) | ExprKind::Index(..) => {
                if let Some(r) = crate::walk::root(l).into_iter().next()
                    && self.mem.contains_key(&r)
                {
                    let at = if self.modifier { usize::MAX } else { self.pos };
                    self.writes.push((r, at, self.loops.clone(), l.span));
                }
                // Index expressions inside the target are reads.
                let mut cur = l.peel_parens();
                loop {
                    match &cur.kind {
                        ExprKind::Member(b, _) => cur = b.peel_parens(),
                        ExprKind::Index(b, k) => {
                            if let ast::IndexKind::Index(Some(i)) = k {
                                self.expr(i, false);
                            }
                            cur = b.peel_parens();
                        }
                        _ => break,
                    }
                }
            }
            _ => self.expr(l, false),
        }
    }

    /// Calls whose arguments are copied: ABI encoding and decoding, hashing, events.
    fn copies(&self, callee: &'a Expr<'a>) -> bool {
        match &callee.peel_parens().kind {
            ExprKind::Member(b, _) => {
                matches!(&b.kind, ExprKind::Ident(i) if i.as_str() == "abi")
            }
            ExprKind::Ident(i) => {
                matches!(i.as_str(), "keccak256" | "sha256" | "ripemd160" | "require" | "assert")
            }
            _ => false,
        }
    }
}
