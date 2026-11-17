//! Spec evaluation at concrete states, for `--spec-check` (see `crate::speccheck`).
//!
//! A sample binds every name a spec can read to a concrete term: the parameters, the returns, the
//! state the harness observed before and after the call, and the transaction context. What it
//! does not observe (ghost state, mapping entries at keys it did not read, storage it does not
//! model) stays an unconstrained constant, so a spec over it evaluates to neither true nor false.
use super::*;

/// A concrete value for one name.
pub(crate) enum Concrete {
    /// A literal of the name's sort.
    Term(String),
    /// A mapping read at the listed keys; its other entries are unconstrained.
    Map(Vec<(String, String)>),
}

/// The observed values of one execution.
pub(crate) struct Sample<'s> {
    /// Values of `msg.sender`, `this`, `block.timestamp` and the like.
    pub ctx: &'s [(&'static str, String)],
    pub args: &'s [String],
    pub pre: &'s [(String, Concrete)],
    /// The state after a normal return.
    pub post: Option<&'s [(String, Concrete)]>,
    /// The returned values, with `post`.
    pub rets: &'s [String],
}

impl<'p, 'a> Vc<'p, 'a> {
    /// Evaluates `specs` of `f` at `s`: in the pre-state, or with `s.post` in the post-state
    /// (`old` then reads the pre-state). Each answer is None when the observed values do not
    /// determine the spec.
    pub(crate) fn eval_sample(
        &mut self,
        f: &'p Func<'a>,
        s: &Sample<'_>,
        specs: &[&'p Spec<'a>],
    ) -> R<Vec<Option<bool>>> {
        self.smt.send("(push 1)\n");
        let r = self.eval_sample_inner(f, s, specs);
        self.smt.send("(pop 1)\n");
        r
    }

    fn eval_sample_inner(
        &mut self,
        f: &'p Func<'a>,
        s: &Sample<'_>,
        specs: &[&'p Spec<'a>],
    ) -> R<Vec<Option<bool>>> {
        self.init_env(false)?;
        self.cur_fn = Some((f.contract.clone(), f.name.clone()));
        for (n, t) in s.ctx {
            if let Some(v) = self.env.get_mut(*n) {
                v.t = t.clone();
            }
        }
        self.set_concrete(s.pre);
        for ((n, ty), t) in f.params.iter().zip(s.args) {
            self.env.insert(n.clone(), Var { ty: ty.clone(), t: t.clone(), len: None });
        }
        self.rets = f.rets.iter().map(|(n, _)| n.clone()).collect();
        for (n, ty) in &f.rets {
            let z = self.zero_var(ty);
            self.env.insert(n.clone(), z);
        }
        if let Some(post) = s.post {
            self.old = Some(self.env.clone());
            self.set_concrete(post);
            for ((n, ty), t) in f.rets.iter().zip(s.rets) {
                self.env.insert(n.clone(), Var { ty: ty.clone(), t: t.clone(), len: None });
            }
        }
        let mut out = vec![];
        for sp in specs {
            let v = self.eval_spec(sp.expr)?;
            out.push(match v.t.as_str() {
                "true" => Some(true),
                "false" => Some(false),
                t => {
                    let t = t.to_string();
                    if matches!(self.smt.check("true", &t, &[]), Res::Unsat) {
                        Some(true)
                    } else if matches!(
                        self.smt.check("true", &format!("(not {t})"), &[]),
                        Res::Unsat
                    ) {
                        Some(false)
                    } else {
                        None
                    }
                }
            });
        }
        Ok(out)
    }

    /// Binds the state variables of `vals` that the contract declares.
    fn set_concrete(&mut self, vals: &[(String, Concrete)]) {
        for (n, c) in vals {
            let Some(ty) = self.env.get(n).map(|v| v.ty.clone()) else { continue };
            let t = match c {
                Concrete::Term(t) => t.clone(),
                Concrete::Map(entries) => {
                    // (store (store |m@k| k0 v0) k1 v1)
                    let mut t = self.decl(n, &ty);
                    for (k, v) in entries {
                        t = format!("(store {t} {k} {v})");
                    }
                    t
                }
            };
            self.env.insert(n.clone(), Var { ty, t, len: None });
        }
    }
}
