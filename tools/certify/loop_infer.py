#!/usr/bin/env python3
"""Loop invariant and measure discovery for cut-point leaves (untrusted).

Reads the leaves of a loop's cut-point check ('Cut.check', exported as JSON by a Lean file such as
'Verify/Phase3/Infer/HornExport.lean' in solidity-lean), encodes the loop as constrained Horn
clauses over the header valuation's words, asks z3's Spacer for an interpretation of the unknown
invariant, and translates it into 'Verify.LoopSource.Equiv.Inv.F' formulas. It then looks for a
decreasing measure, screens every obligation the Lean side will state (the sequents of
'Verify.Phase3.Infer.allSeqsW', hypotheses sliced the same way) with z3, and writes a Lean
generator whose output module proves the obligations in the kernel.

The property: leaves exported by solidity-lean's 'Verify.Phase3.Route.GenInfer.exportLeaves' carry
the tree of the source's own loop erasure ("tree_source": "erasure"); that tree is the property and
--target is refused for them, so the invariant is proposed for exactly the obligations the published
theorem ('Verify.Phase3.Route.final_of_generatedW') states, and the NatSpec property enters only
through the spec bridge (TreeLoop.LoopSpec on the same erased tree). --target remains for leaves
without a tree (the Horn.ok_transfer backend demo).

Engines, in order (--engines): Spacer on the Horn clauses, then an interval abstract interpretation
with thresholds (bounds of each header word drawn from 0, the call's words, literals and exit-test
bounds, pruned by concrete runs, greatest inductive subset by Houdini). The diagnosis separates a
refutation (Spacer unsat, or a concrete run that ends differently from the property) from a refusal
to discover (unknown, timeout, no usable model).

Nothing here is trusted. z3's answers only choose candidates; only the Lean-checked 'obl' theorem
counts. A failing candidate is reported with the sequent that fails (its task kind) and z3's
counterexample valuation, and '--lean-neg' writes a module that refuses it in the kernel
('rejectsAny_sound').

Words are naturals below 2^256 and every operation is 'WExpr.eval''s: add and sub modulo 2^256,
comparisons 0 or 1, iszero, not, sel.

Usage:
  loop_infer.py LEAVES.json [--target returns-arg:0] [--candidate CAND.json] [--engines spacer,interval]
      [--emit-candidate CAND.json]
      [--stage0 Stage0.lean --cut Cut.lean --seqw SeqW.lean --ns NS --gen GEN.lean --obl OBL.lean
       --prelude-out PRE.lean] [--lean-neg NEG.lean]
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile

W = 1 << 256
Z3 = os.environ.get("Z3", os.path.expanduser("~/.local/bin/z3"))


class Unsupported(Exception):
    """A word or formula outside what this encoder or translator handles."""


# ---------------------------------------------------------------------------
# Words (WExpr) as JSON lists: ["lit",n] ["arg",i] ["init",k] ["un",op,a] ["bin",op,a,b]
# ["sinit",k] ["sel",c,a,b] ["hash",a,b] ["caller"].


def lit(n):
    return ["lit", n]


def arg(i):
    return ["arg", i]


def badd(a, b):
    return ["bin", "add", a, b]


def bin_eval(op, a, b):
    if op == "add":
        return (a + b) % W
    if op == "sub":
        return (a + (W - b)) % W
    if op == "mul":
        return (a * b) % W
    if op == "div":
        return 0 if b == 0 else a // b
    if op == "lt":
        return 1 if a < b else 0
    if op == "gt":
        return 1 if b < a else 0
    if op == "eq":
        return 1 if a == b else 0
    if op == "and":
        return a & b
    if op == "or":
        return a | b
    if op == "xor":
        return a ^ b
    if op == "shl":
        return (b << a) % W if a < 256 else 0
    if op == "shr":
        return b >> a if a < 256 else 0
    if op == "byte":
        return b // 256 ** (31 - a) % 256 if a < 32 else 0
    raise Unsupported(op)


def un_eval(op, a):
    if op == "iszero":
        return 1 if a == 0 else 0
    if op == "not":
        return W - 1 - a
    raise Unsupported(op)


def weval(e, args, store=None):
    """WExpr.eval at a valuation with argument words 'args' and storage 'store'."""
    store = store or {}
    t = e[0]
    if t == "lit":
        return e[1] % W
    if t == "arg":
        return args[e[1]] % W if e[1] < len(args) else 0
    if t == "init":
        return store.get(e[1], 0) % W
    if t == "un":
        return un_eval(e[1], weval(e[2], args, store)) % W
    if t == "bin":
        return bin_eval(e[1], weval(e[2], args, store), weval(e[3], args, store)) % W
    if t == "sinit":
        return store.get(weval(e[1], args, store), 0) % W
    if t == "sel":
        return weval(e[3], args, store) if weval(e[1], args, store) == 0 else weval(e[2], args, store)
    if t == "caller":
        return 0
    raise Unsupported(t)


def wnorm(e):
    """WExpr.norm: constant operations folded."""
    t = e[0]
    if t == "lit":
        return lit(e[1] % W)
    if t == "un":
        a = wnorm(e[2])
        return lit(un_eval(e[1], a[1] % W) % W) if a[0] == "lit" else ["un", e[1], a]
    if t == "bin":
        a, b = wnorm(e[2]), wnorm(e[3])
        if a[0] == "lit" and b[0] == "lit":
            return lit(bin_eval(e[1], a[1] % W, b[1] % W) % W)
        return ["bin", e[1], a, b]
    if t == "sinit":
        return ["sinit", wnorm(e[1])]
    if t == "sel":
        c, a, b = wnorm(e[1]), wnorm(e[2]), wnorm(e[3])
        if c[0] == "lit":
            return b if c[1] % W == 0 else a
        return ["sel", c, a, b]
    if t == "hash":
        return ["hash", wnorm(e[1]), wnorm(e[2])]
    return e


def bnorm(b):
    """OByte.norm."""
    if b[0] == "blit":
        return ["blit", b[1] % 256]
    en = wnorm(b[1])
    if en[0] == "lit":
        return ["blit", en[1] % W // 256 ** (31 - b[2]) % 256]
    return ["byteOf", en, b[2]]


def wsub(n, hs, e):
    """Inv.sub: argument n + k replaced by header word k."""
    t = e[0]
    if t == "arg":
        i = e[1]
        if i < n:
            return e
        return hs[i - n] if i - n < len(hs) else lit(0)
    if t == "un":
        return ["un", e[1], wsub(n, hs, e[2])]
    if t == "bin":
        return ["bin", e[1], wsub(n, hs, e[2]), wsub(n, hs, e[3])]
    if t == "sinit":
        return ["sinit", wsub(n, hs, e[1])]
    if t == "sel":
        return ["sel", wsub(n, hs, e[1]), wsub(n, hs, e[2]), wsub(n, hs, e[3])]
    if t == "hash":
        return ["hash", wsub(n, hs, e[1]), wsub(n, hs, e[2])]
    return e


def wleaves(e):
    """Inv.leaves: the argument and storage words a word reads."""
    t = e[0]
    if t == "lit":
        return []
    if t in ("arg", "init"):
        return [e]
    if t == "un":
        return wleaves(e[2])
    if t == "bin":
        return wleaves(e[2]) + wleaves(e[3])
    if t == "sinit":
        return [e] + wleaves(e[1])
    if t == "sel":
        return wleaves(e[1]) + wleaves(e[2]) + wleaves(e[3])
    if t == "hash":
        return [e] + wleaves(e[1]) + wleaves(e[2])
    return [e]


def dedup(xs):
    seen, out = set(), []
    for x in xs:
        k = json.dumps(x)
        if k not in seen:
            seen.add(k)
            out.append(x)
    return out


# ---------------------------------------------------------------------------
# Formulas (F) as JSON lists: ["fact",e,b] ["le",a,b] ["lt",a,b] ["eq",a,b] ["lin",a,b,c,d]
# ["or",f,g] ["not",f]; lin a b c d is a + d = b + c.

FALSE = ["lt", lit(0), lit(0)]


def fholds(f, args, store=None):
    """F.Holds at a valuation."""
    t = f[0]

    def ev(e):
        return weval(e, args, store)

    if t == "fact":
        return (ev(f[1]) == 0) == (not f[2])
    if t == "le":
        return ev(f[1]) <= ev(f[2])
    if t == "lt":
        return ev(f[1]) < ev(f[2])
    if t == "eq":
        return ev(f[1]) == ev(f[2])
    if t == "lin":
        return ev(f[1]) + ev(f[4]) == ev(f[2]) + ev(f[3])
    if t == "or":
        return fholds(f[1], args, store) or fholds(f[2], args, store)
    if t == "not":
        return not fholds(f[1], args, store)
    raise Unsupported(t)


def fwords(f):
    t = f[0]
    if t == "fact":
        return [f[1]]
    if t in ("le", "lt", "eq"):
        return [f[1], f[2]]
    if t == "lin":
        return list(f[1:5])
    if t == "or":
        return fwords(f[1]) + fwords(f[2])
    if t == "not":
        return fwords(f[1])
    raise Unsupported(t)


def fleaves(f):
    return [x for w in fwords(f) for x in wleaves(w)]


def fsub(n, hs, f):
    t = f[0]
    if t == "fact":
        return ["fact", wsub(n, hs, f[1]), f[2]]
    if t in ("le", "lt", "eq"):
        return [t, wsub(n, hs, f[1]), wsub(n, hs, f[2])]
    if t == "lin":
        return ["lin"] + [wsub(n, hs, w) for w in f[1:5]]
    if t == "or":
        return ["or", fsub(n, hs, f[1]), fsub(n, hs, f[2])]
    if t == "not":
        return ["not", fsub(n, hs, f[1])]
    raise Unsupported(t)


def facts_f(K):
    return [["fact", e, b] for e, b in K]


def no_case(c):
    g = FALSE
    for e, b in reversed(c):
        g = ["or", ["not", ["fact", e, b]], g]
    return g


# Slicing, exactly as Inv.slice and Seq.hyp (the Lean sequents keep only these hypotheses).


def slice_step(hyps, ls):
    keys = {json.dumps(x) for x in ls}
    more = [x for h in hyps if any(json.dumps(y) in keys for y in fleaves(h)) for x in fleaves(h)]
    return dedup(ls + more)


def slice_leaves(hyps, k, ls):
    while k > 0:
        ls2 = slice_step(hyps, ls)
        if len(ls2) == len(ls):
            return ls
        ls, k = ls2, k - 1
    return ls


def slice_(k, hyps, seed):
    ls = slice_leaves(hyps, k, dedup([x for f in seed for x in fleaves(f)]))
    keys = {json.dumps(x) for x in ls}
    return [h for h in hyps if all(json.dumps(y) in keys for y in fleaves(h))]


def hyp(S, K, g):
    return slice_(1, S, ([g] if g is not None else []) + K) + K


# ---------------------------------------------------------------------------
# Trees: ["halt", ok, out, writes, nlogs] | ["ite", c, t, e]; bytes ["blit",b] | ["byteOf",e,i].


def paths(T):
    if T[0] == "halt":
        return [([], T)]
    _, c, t, e = T
    return [([(c, True)] + p, h) for p, h in paths(t)] + [([(c, False)] + p, h) for p, h in paths(e)]


def read_lit(K, ws):
    for k, e in ws:
        if k[0] != "lit":
            return None
        if k[1] % W == K:
            return e
    return ["init", K]


def halt_goals(H, H2):
    """Inv.haltGoals: None outside its procedure."""
    _, ok, out, ws, nl = H
    _, ok2, out2, ws2, nl2 = H2
    if ok != ok2:
        return [FALSE]
    no, no2 = [bnorm(b) for b in out], [bnorm(b) for b in out2]
    if (no == no2 and all(b[0] == "blit" for b in no) and nl == 0 and nl2 == 0
            and all(w[0][0] == "lit" for w in ws) and all(w[0][0] == "lit" for w in ws2)):
        if not ok:
            return []
        gs = []
        for K in [w[0][1] % W for w in ws] + [w[0][1] % W for w in ws2]:
            gs.append(["eq", read_lit(K, ws), read_lit(K, ws2)])
        return gs
    return None


def byte_goal(a, b):
    if bnorm(a) == bnorm(b):
        return []
    if a[0] == "byteOf" and b[0] == "byteOf" and a[2] == b[2]:
        return [["eq", a[1], b[1]]]
    return None


def halt_goals_w(H, H2):
    """Verify.Phase3.Infer.haltGoalsW."""
    gs = halt_goals(H, H2)
    if gs is not None:
        return gs
    _, ok, out, ws, nl = H
    _, ok2, out2, ws2, nl2 = H2
    if ok == ok2 and ws == ws2 and nl == nl2 and len(out) == len(out2):
        gs = []
        for a, b in zip(out, out2):
            g = byte_goal(a, b)
            if g is None:
                return None
            gs += g
        return gs
    return None


def end_seqs(S, K, T2, T, kind):
    out = []
    for p2, h2 in paths(T2):
        for p, h in paths(T):
            k = K + facts_f(p2) + facts_f(p)
            gs = halt_goals_w(h2, h)
            if gs is not None:
                out += [(hyp(S, k, g), g, kind) for g in gs]
            else:
                out.append((hyp(S, k, None), None, kind + " (ends not comparable: must be unreachable)"))
    return out


def tree_below(n, T):
    if T[0] == "ite":
        return (all(l[0] != "arg" or l[1] < n for l in wleaves(T[1]))
                and tree_below(n, T[2]) and tree_below(n, T[3]))
    ws = [b[1] for b in T[2] if b[0] == "byteOf"] + [x for w in T[3] for x in w]
    return all(l[0] != "arg" or l[1] < n for w in ws for l in wleaves(w))


def all_seqs(n, cases, T, le, li, I, mu):
    """The sequents of allSeqsW, each tagged with its task kind (for diagnostics)."""
    out = []
    if not tree_below(n, T):
        out.append(([], FALSE, "below: the property's tree reads a header word"))
    for fi, f in enumerate(I):
        for j, (K, o) in enumerate(le):
            if o[0] == "back":
                g = fsub(n, o[1], f)
                out.append((hyp([], facts_f(K), g), g, f"initiation: I[{fi}] after entry arrival le[{j}]"))
        for j, (K, o) in enumerate(li):
            if o[0] == "back":
                g = fsub(n, o[1], f)
                out.append((hyp(I, facts_f(K), g), g, f"preservation: I[{fi}] after iteration back edge li[{j}]"))
    for j, (K, o) in enumerate(le):
        if o[0] == "exit":
            out += end_seqs([], facts_f(K), o[1], T, f"sufficiency: entry end le[{j}] is the property")
    for j, (K, o) in enumerate(li):
        if o[0] == "exit":
            out += end_seqs(I, facts_f(K), o[1], T, f"sufficiency: iteration exit li[{j}] is the property")
        else:
            g = ["lt", wsub(n, o[1], mu), mu]
            out.append((hyp(I, facts_f(K), g), g, f"decrease: the measure falls on back edge li[{j}]"))
    out.append((hyp(I, [no_case(c) for c in cases], None), None, "cover: some case holds under I"))
    return out


# ---------------------------------------------------------------------------
# SMT encoding of words, exactly WExpr.eval over naturals below 2^256.


def smt_word(e, env):
    """env(e) gives the SMT term of an argument or storage word."""
    t = e[0]
    if t == "lit":
        return str(e[1] % W)
    if t in ("arg", "init"):
        return env(e)
    if t == "un":
        a = smt_word(e[2], env)
        if e[1] == "iszero":
            return f"(ite (= {a} 0) 1 0)"
        if e[1] == "not":
            return f"(- {W - 1} {a})"
    if t == "bin":
        a, b = smt_word(e[2], env), smt_word(e[3], env)
        op = e[1]
        if op == "add":
            return f"(ite (< (+ {a} {b}) {W}) (+ {a} {b}) (- (+ {a} {b}) {W}))"
        if op == "sub":
            return f"(ite (>= {a} {b}) (- {a} {b}) (+ (- {a} {b}) {W}))"
        if op == "lt":
            return f"(ite (< {a} {b}) 1 0)"
        if op == "gt":
            return f"(ite (> {a} {b}) 1 0)"
        if op == "eq":
            return f"(ite (= {a} {b}) 1 0)"
        if op == "mul":
            return f"(mod (* {a} {b}) {W})"
        if op == "div":
            return f"(ite (= {b} 0) 0 (div {a} {b}))"
    if t == "sel":
        c, a, b = (smt_word(x, env) for x in e[1:4])
        return f"(ite (= {c} 0) {b} {a})"
    raise Unsupported("word " + json.dumps(e)[:80])


def smt_fact(e, b, env):
    """'e != 0' (b) or 'e == 0' (not b), with comparison words read as their comparisons: exact,
    since iszero, eq, lt and gt words are 0 or 1, and a word is never negative, so 'e != 0' is
    'e >= 1' and 'e == 0' is 'e <= 0' (Spacer diverges on the equation forms here and solves the
    inequality forms at once)."""
    if e[0] == "un" and e[1] == "iszero":
        return smt_fact(e[2], not b, env)
    if e[0] == "bin" and e[1] == "eq" and lit(0) in (wnorm(e[2]), wnorm(e[3])):
        return smt_fact(e[3] if wnorm(e[2]) == lit(0) else e[2], not b, env)
    if e[0] == "bin" and e[1] in ("eq", "lt", "gt"):
        a, c = smt_word(e[2], env), smt_word(e[3], env)
        rel = {"eq": f"(= {a} {c})", "lt": f"(< {a} {c})", "gt": f"(> {a} {c})"}[e[1]]
        return rel if b else f"(not {rel})"
    x = smt_word(e, env)
    return f"(>= {x} 1)" if b else f"(<= {x} 0)"


def smt_f(f, env):
    t = f[0]

    def w(e):
        return smt_word(e, env)

    if t == "fact":
        return smt_fact(f[1], f[2], env)
    if t == "le":
        return f"(<= {w(f[1])} {w(f[2])})"
    if t == "lt":
        return f"(< {w(f[1])} {w(f[2])})"
    if t == "eq":
        return f"(= {w(f[1])} {w(f[2])})"
    if t == "lin":
        return f"(= (+ {w(f[1])} {w(f[4])}) (+ {w(f[2])} {w(f[3])}))"
    if t == "or":
        return f"(or {smt_f(f[1], env)} {smt_f(f[2], env)})"
    if t == "not":
        return f"(not {smt_f(f[1], env)})"
    raise Unsupported(t)


def run_z3(text, timeout=60):
    with tempfile.NamedTemporaryFile("w", suffix=".smt2", delete=False) as fh:
        fh.write(text)
        path = fh.name
    try:
        r = subprocess.run(["nice", "-n", "10", Z3, f"-T:{timeout}", path], capture_output=True, text=True)
        return r.stdout
    finally:
        os.unlink(path)


def word_var(e):
    if e[0] == "arg":
        return f"a{e[1]}"
    if e[0] == "init":
        return f"s{e[1]}"
    raise Unsupported(f"word {e[0]}")


def seq_smt(hyps, goal):
    fs = hyps + ([["not", goal]] if goal is not None else [])
    ls = dedup([x for f in fs for x in fleaves(f)])
    body = [smt_f(f, word_var) for f in fs]
    decl = []
    for l in ls:
        v = word_var(l)
        decl += [f"(declare-const {v} Int)", f"(assert (and (<= 0 {v}) (< {v} {W})))"]
    return ls, "\n".join(decl + [f"(assert {b})" for b in body] + ["(check-sat)", "(get-model)"]) + "\n"


def check_seq(hyps, goal, timeout=30):
    """z3 on one sequent: ('valid', None) | ('invalid', valuation) | ('unknown', why)."""
    try:
        ls, text = seq_smt(hyps, goal)
    except Unsupported as e:
        return "unknown", f"unsupported: {e}"
    out = run_z3(text, timeout)
    first = out.strip().split("\n", 1)[0] if out.strip() else ""
    if first == "unsat":
        return "valid", None
    if first == "sat":
        model = parse_model_values(out)
        return "invalid", {word_var(l): model.get(word_var(l), 0) for l in ls}
    return "unknown", out.strip()[:200]


# ---------------------------------------------------------------------------
# S-expressions and z3 models.


def sexp_parse(text):
    toks = re.findall(r"\(|\)|\|[^|]*\||[^\s()]+", text)
    pos = 0

    def one():
        nonlocal pos
        tok = toks[pos]
        pos += 1
        if tok == "(":
            out = []
            while toks[pos] != ")":
                out.append(one())
            pos += 1
            return out
        return tok

    res = []
    while pos < len(toks):
        res.append(one())
    return res


def int_of(t):
    if isinstance(t, str):
        return int(t)
    if t[0] == "-" and len(t) == 2:
        return -int_of(t[1])
    raise Unsupported(f"not a numeral: {sexp_str(t)}")


def parse_model_values(out):
    vals = {}
    for s in sexp_parse(out):
        if not isinstance(s, list):
            continue
        for d in s:
            if isinstance(d, list) and len(d) == 5 and d[0] == "define-fun" and d[2] == [] and d[3] == "Int":
                vals[d[1]] = int_of(d[4])
    return vals


def sexp_str(t):
    return t if isinstance(t, str) else "(" + " ".join(sexp_str(x) for x in t) + ")"


# ---------------------------------------------------------------------------
# The Horn clauses.


def leaf_words(o):
    """The words a leaf reads: a back edge's header words; an exit's path facts, output-byte words
    and storage writes (keys and values), which halt_goals_w compares with the property's."""
    if o[0] == "back":
        return list(o[1])
    return [w for p, h in paths(o[1])
            for w in [e for e, _ in p] + [b[1] for b in h[2] if b[0] == "byteOf"]
            + [x for kv in h[3] for x in kv]]


def chc(le, li, n, nhw, T):
    """The CHC system over Inv(x_0..x_{n-1}, h_0..h_{nhw-1}, s_k...): init from the entry run's
    arrivals, preservation from each iteration back edge under its facts, one query per end and
    path of T (the end is the property, or the path is unreachable)."""
    slots = sorted({l[1] for _, o in le + li for w in leaf_words(o) for l in wleaves(w) if l[0] == "init"}
                   | {l[1] for K, _ in le + li for e, _ in K for l in wleaves(e) if l[0] == "init"})
    xs = [f"x{i}" for i in range(n)]
    hs = [f"h{k}" for k in range(nhw)]
    ss = [f"s{k}" for k in slots]
    # Header words no leaf reads (not in a fact, an end or a back edge's words) cannot matter to
    # any clause: leave them out of Inv's parameters (discovery only; Lean checks the result).
    live = live_header(le + li, n, nhw)
    hs_live = [hs[k] for k in live]
    params = xs + hs_live + ss
    sig = " ".join("Int" for _ in params)

    def env_entry(e):
        if e[0] == "arg":
            return xs[e[1]] if e[1] < n else "0"
        return f"s{e[1]}"

    def env_iter(e):
        if e[0] == "arg":
            if e[1] < n:
                return xs[e[1]]
            return hs[e[1] - n] if e[1] - n < nhw else "0"
        return f"s{e[1]}"

    def rng(vs):
        return [f"(<= 0 {v})" for v in vs] + [f"(< {v} {W})" for v in vs]

    def clause(vs, body, head):
        q = " ".join(f"({v} Int)" for v in vs)
        b = "(and " + " ".join(body) + ")" if body else "true"
        return f"(assert (forall ({q}) (=> {b} {head})))"

    inv_cur = f"(Inv {' '.join(params)})"
    out = ["(set-logic HORN)", f"(declare-fun Inv ({sig}) Bool)"]
    for K, o in le:
        body = rng(xs + ss) + [smt_f(f, env_entry) for f in facts_f(K)]
        if o[0] == "back":
            nxt = [smt_word(o[1][k], env_entry) for k in live]
            out.append(clause(xs + ss, body, f"(Inv {' '.join(xs + nxt + ss)})"))
        else:
            out += query(xs + ss, body, o[1], T, env_entry, clause)
    for K, o in li:
        body = rng(params) + [inv_cur] + [smt_f(f, env_iter) for f in facts_f(K)]
        if o[0] == "back":
            nxt = [smt_word(o[1][k], env_iter) for k in live]
            out.append(clause(params, body, f"(Inv {' '.join(xs + nxt + ss)})"))
        else:
            out += query(params, body, o[1], T, env_iter, clause)
    out += ["(check-sat)", "(get-model)"]
    return "\n".join(out) + "\n", params, slots, live


def live_header(leaves, n, nhw):
    """The header words some leaf reads, closed under the back edges' words that feed them."""
    read = set()
    for K, o in leaves:
        ws = [e for e, _ in K] + (leaf_words(o) if o[0] == "exit" else [])
        read |= {l[1] - n for w in ws for l in wleaves(w) if l[0] == "arg" and l[1] >= n}
    while True:
        more = set(read)
        for _, o in leaves:
            if o[0] == "back":
                for k in read:
                    if k < len(o[1]):
                        more |= {l[1] - n for l in wleaves(o[1][k]) if l[0] == "arg" and l[1] >= n}
        if more == read:
            return sorted(k for k in read if k < nhw)
        read = more


def query(vs, body, T2, T, env, clause):
    out = []
    for p2, h2 in paths(T2):
        for p, h in paths(T):
            b = body + [smt_f(f, env) for f in facts_f(p2) + facts_f(p)]
            gs = halt_goals_w(h2, h)
            if gs is None:
                out.append(clause(vs, b, "false"))
            elif gs:
                g = "(and " + " ".join(dedup([smt_f(x, env) for x in gs])) + ")"
                out.append(clause(vs, b + [f"(not {g})"], "false"))
    return out


def spacer(le, li, n, nhw, T, timeout=60):
    """Spacer's answer and its interpretation of Inv as (formals, body), or None."""
    text, params, slots, live = chc(le, li, n, nhw, T)
    out = run_z3(text, timeout)
    first = out.strip().split("\n", 1)[0] if out.strip() else ""
    if first != "sat":
        return first or "no answer", None, text, (slots, live)
    for s in sexp_parse(out):
        for d in (s if isinstance(s, list) else []):
            if isinstance(d, list) and d and d[0] == "define-fun" and d[1] == "Inv":
                return "sat", ([p[0] for p in d[2]], d[4]), text, (slots, live)
    return "sat (no model)", None, text, (slots, live)


# ---------------------------------------------------------------------------
# Translating Spacer's model into F.


def subst_lets(t, env=None):
    env = env or {}
    if isinstance(t, str):
        return env.get(t, t)
    if t and t[0] == "let":
        env2 = dict(env)
        for name, val in t[1]:
            env2[name] = subst_lets(val, env)
        return subst_lets(t[2], env2)
    return [subst_lets(x, env) for x in t]


def linear(t, var):
    """A linear integer term as ({word-json: coef}, const)."""
    if isinstance(t, str):
        if re.fullmatch(r"\d+", t):
            return {}, int(t)
        return {json.dumps(var(t)): 1}, 0
    op = t[0]
    if op == "+":
        acc, c = {}, 0
        for x in t[1:]:
            m, k = linear(x, var)
            c += k
            for v, a in m.items():
                acc[v] = acc.get(v, 0) + a
        return acc, c
    if op == "-":
        m, k = linear(t[1], var)
        if len(t) == 2:
            return {v: -a for v, a in m.items()}, -k
        acc = dict(m)
        for x in t[2:]:
            m2, k2 = linear(x, var)
            k -= k2
            for v, a in m2.items():
                acc[v] = acc.get(v, 0) - a
        return acc, k
    if op == "*" and len(t) == 3:
        for i, j in ((1, 2), (2, 1)):
            try:
                c = int_of(t[i])
            except (Unsupported, ValueError):
                continue
            m, k = linear(t[j], var)
            return {v: c * a for v, a in m.items()}, c * k
    raise Unsupported(f"nonlinear or unknown term {sexp_str(t)}")


def wsum(ws):
    if not ws:
        return lit(0)
    acc = ws[0]
    for w in ws[1:]:
        acc = badd(acc, w)
    return acc


def sides(m, c):
    """sum m + c as word lists (left, right) with left - right = sum m + c, coefficients unrolled."""
    left, right = [], []
    for v, a in sorted(m.items()):
        if a == 0:
            continue
        if abs(a) > 4:
            raise Unsupported(f"coefficient {a}")
        (left if a > 0 else right).extend([json.loads(v)] * abs(a))
    if c > 0:
        left.append(lit(c))
    elif c < 0:
        right.append(lit(-c))
    return left, right


def atom_f(op, m, c):
    """The formula 'sum m + c op 0', op in <=, <, =. Sums of more than two words on a side of an
    equation, and every sum in an inequality, are word additions: exact only without overflow,
    which the Lean sequents then have to show (or the candidate is dropped)."""
    left, right = sides(m, c)
    if op == "=":
        l2 = (left + [lit(0), lit(0)])[:2] if len(left) <= 2 else [left[0], wsum(left[1:])]
        r2 = (right + [lit(0), lit(0)])[:2] if len(right) <= 2 else [right[0], wsum(right[1:])]
        return ["lin", l2[0], r2[0], r2[1], l2[1]]
    return ["le" if op == "<=" else "lt", wsum(left), wsum(right)]


def norm_atom(t, var):
    """(op, m, c) with op in <=, <, = for 'sum m + c op 0'."""
    op, a, b = t[0], t[1], t[2]
    ma, ca = linear(a, var)
    mb, cb = linear(b, var)
    diff = dict(ma)
    for v, k in mb.items():
        diff[v] = diff.get(v, 0) - k
    c = ca - cb
    diff = {v: k for v, k in diff.items() if k != 0}
    if op in ("<=", "<", "="):
        return op, diff, c
    if op in (">=", ">"):
        return ("<=" if op == ">=" else "<"), {v: -k for v, k in diff.items()}, -c
    raise Unsupported(op)


NEG = {"<=": ">", "<": ">=", ">=": "<", ">": "<="}


def int_atom(p, var):
    """A comparison (possibly under 'not') as ('<=' | '=', m, c) for 'sum m + c op 0' over the
    integers: strict comparisons become non-strict by one (Spacer's integers are exact)."""
    if isinstance(p, list) and p and p[0] == "not" and isinstance(p[1], list) and p[1] and p[1][0] in NEG:
        p = [NEG[p[1][0]]] + p[1][1:]
    if not (isinstance(p, list) and p and p[0] in ("<=", "<", ">=", ">", "=") and len(p) == 3):
        raise Unsupported("not a comparison")
    o, m, c = norm_atom(p, var)
    if o == "<":
        o, c = "<=", c + 1
    return o, m, c


def to_f(t, var):
    if isinstance(t, str):
        if t == "true":
            return None
        if t == "false":
            return FALSE
        raise Unsupported(t)
    op = t[0]
    if op == "not":
        g = to_f(t[1], var)
        return ["not", g if g is not None else ["le", lit(0), lit(0)]]
    if op == "or":
        fs = [to_f(x, var) for x in t[1:]]
        if any(f is None for f in fs):
            return None
        acc = fs[-1]
        for f in reversed(fs[:-1]):
            acc = ["or", f, acc]
        return acc
    if op == "and":
        fs = [f for f in (to_f(x, var) for x in t[1:]) if f is not None]
        if not fs:
            return None
        acc = ["not", fs[-1]]
        for f in reversed(fs[:-1]):
            acc = ["or", ["not", f], acc]
        return ["not", acc]
    if op in ("<=", "<", "=", ">=", ">"):
        o, m, c = norm_atom(t, var)
        if not m:
            ok = {"<=": c <= 0, "<": c < 0, "=": c == 0}[o]
            return None if ok else FALSE
        return atom_f(o, m, c)
    raise Unsupported(f"connective {op}")


def conjuncts(t):
    if isinstance(t, list) and t and t[0] == "and":
        return [x for s in t[1:] for x in conjuncts(s)]
    return [t]


def translate(model, n, nhw, slots, live=None):
    """Spacer's Inv as F formulas: returns (formulas, dropped [(atom, why)]). Inv's parameters
    are the call words, the live header words 'live' (all by default), then the storage slots."""
    formals, body = model
    body = subst_lets(body)
    live = list(range(nhw)) if live is None else live

    def var(name):
        if name not in formals:
            raise Unsupported(f"free symbol {name}")
        k = formals.index(name)
        if k < n:
            return arg(k)
        if k < n + len(live):
            return arg(n + live[k - n])
        return ["init", slots[k - n - len(live)]]

    atoms, rest = [], []
    for p in conjuncts(body):
        try:
            atoms.append((int_atom(p, var), p))
            continue
        except Unsupported:
            pass
        rest.append(p)
    used, out, dropped = set(), [], []
    for i, ((o, m, c), p) in enumerate(atoms):
        if i in used:
            continue
        neg = ({v: -k for v, k in m.items()}, -c)
        j = next((j for j, ((o2, m2, c2), _) in enumerate(atoms) if j != i and j not in used
                  and o2 == "<=" and (m2, c2) == neg), None) if o == "<=" else None
        try:
            if o == "=":
                used.add(i)
                out.append(atom_f("=", m, c))
                continue
            if j is not None:
                # L <= 0 and -L <= 0: the equation L = 0, which lin states exactly.
                used |= {i, j}
                out.append(atom_f("=", m, c))
            else:
                used.add(i)
                if len(m) == 1 and c <= 0 and all(k == -1 for k in m.values()):
                    continue  # v >= -c with -c >= 0... i.e. c - v <= 0 with c <= 0: every word is >= 0
                out.append(atom_f(o, m, c))
        except Unsupported as e:
            dropped.append((sexp_str(p), str(e)))
    for p in rest:
        try:
            f = to_f(p, var)
            if f is not None:
                out.append(f)
        except Unsupported as e:
            dropped.append((sexp_str(p), str(e)))
    return dedup(out), dropped


# ---------------------------------------------------------------------------
# Measures, Houdini, the whole loop.


def measure_cands(n, nhw, li):
    """Header words every back edge lowers by a constant (under the edge's guard), exit-test
    differences (bound - h for h counting up), then 0 - h and h for every header word."""
    out = []
    backs = [o[1] for _, o in li if o[0] == "back"]
    for k in range(nhw):
        h = arg(n + k)
        if backs and all(len(b) > k and b[k][0] == "bin" and b[k][1] == "sub" and b[k][2] == h
                         and b[k][3][0] == "lit" for b in backs):
            out.append(h)
    for K, _ in li:
        for e, _ in K:
            if e[0] == "bin" and e[1] == "lt" and e[2][0] == "arg" and e[2][1] >= n:
                out.append(["bin", "sub", e[3], e[2]])
    for k in range(nhw):
        out.append(["bin", "sub", lit(0), arg(n + k)])
    for k in range(nhw):
        out.append(arg(n + k))
    return dedup(out)


def houdini(n, cases, T, le, li, I, log):
    """Drop formulas of I whose initiation or preservation sequent z3 does not prove, until none
    fails (the remaining set is inductive in the Lean sequents' sliced form)."""
    while True:
        bad = set()
        for hyps, g, kind in all_seqs(n, cases, T, le, li, I, lit(0)):
            if kind.startswith(("initiation", "preservation")):
                st, cex = check_seq(hyps, g)
                if st != "valid":
                    fi = int(re.search(r"I\[(\d+)\]", kind).group(1))
                    bad.add(fi)
                    log(f"  houdini: drop I[{fi}] = {fmt_f(I[fi])}: {kind}: {st} {cex}")
        if not bad:
            return I
        I = [f for i, f in enumerate(I) if i not in bad]


def check_all(n, cases, T, le, li, I, mu):
    """z3 on every sequent (identical sequents, such as one per output byte, once each)."""
    fails = []
    seqs = all_seqs(n, cases, T, le, li, I, mu)
    seen = set()
    for hyps, g, kind in seqs:
        key = json.dumps([hyps, g])
        if key in seen:
            continue
        seen.add(key)
        st, cex = check_seq(hyps, g)
        if st != "valid":
            fails.append({"kind": kind, "status": st, "goal": fmt_f(g) if g else "false",
                          "hyps": [fmt_f(h) for h in hyps], "counterexample": cex})
    return seqs, fails


def find_measure(n, nhw, cases, T, le, li, I, log):
    for mu in measure_cands(n, nhw, li):
        ok = True
        for hyps, g, kind in all_seqs(n, cases, T, le, li, I, mu):
            if kind.startswith("decrease"):
                st, _ = check_seq(hyps, g)
                if st != "valid":
                    ok = False
                    break
        log(f"  measure candidate {fmt_w(mu)}: {'falls on every back edge' if ok else 'rejected'}")
        if ok:
            return mu
    return None


def eval_tree(T, vals):
    while T[0] == "ite":
        T = T[3] if weval(T[1], vals) == 0 else T[2]
    out = [b[1] % 256 if b[0] == "blit" else weval(b[1], vals) // 256 ** (31 - b[2]) % 256 for b in T[2]]
    return {"success": T[1], "output": int.from_bytes(bytes(out), "big") if out else None,
            "writes": [[weval(k, vals), weval(v, vals)] for k, v in T[3]] if T[1] else []}


def concrete_cex(n, le, li, T, calls, fuel=64):
    """A call whose run of the leaves ends differently from T: a concrete trace refuting the
    property (only the leaves are run, as Cut.check produced them)."""
    def leaf(ls, vals):
        for K, o in ls:
            if all((weval(e, vals) == 0) == (not b) for e, b in K):
                return o
        return None

    for args in calls:
        o = leaf(le, args)
        vals = list(args)
        steps = 0
        while o is not None and o[0] == "back" and steps < fuel:
            vals = list(args) + [weval(e, vals) for e in o[1]]
            o = leaf(li, vals)
            steps += 1
        if o is None or o[0] != "exit":
            continue
        end, want = eval_tree(o[1], vals), eval_tree(T, list(args))
        if end != want:
            return {"call": args, "iterations": steps, "mir": end, "property": want}
    return None


# ---------------------------------------------------------------------------
# Printing.


def fmt_w(e):
    t = e[0]
    if t == "lit":
        return str(e[1])
    if t == "arg":
        return f"x{e[1]}"
    if t == "init":
        return f"s[{e[1]}]"
    if t == "un":
        return f"{e[1]}({fmt_w(e[2])})"
    if t == "bin":
        sym = {"add": "+", "sub": "-", "lt": "<", "gt": ">", "eq": "=="}.get(e[1])
        return f"({fmt_w(e[2])} {sym} {fmt_w(e[3])})" if sym else f"{e[1]}({fmt_w(e[2])}, {fmt_w(e[3])})"
    return json.dumps(e)


def fmt_f(f):
    t = f[0]
    if t == "fact":
        return f"{fmt_w(f[1])} {'!=' if f[2] else '=='} 0"
    if t == "le":
        return f"{fmt_w(f[1])} <= {fmt_w(f[2])}"
    if t == "lt":
        return f"{fmt_w(f[1])} < {fmt_w(f[2])}"
    if t == "eq":
        return f"{fmt_w(f[1])} = {fmt_w(f[2])}"
    if t == "lin":
        return f"{fmt_w(f[1])} + {fmt_w(f[4])} = {fmt_w(f[2])} + {fmt_w(f[3])}"
    if t == "or":
        return f"({fmt_f(f[1])} or {fmt_f(f[2])})"
    if t == "not":
        return f"not ({fmt_f(f[1])})"
    return json.dumps(f)


def lean_w(e):
    t = e[0]
    if t == "lit":
        return f"(WExpr.lit {e[1]})"
    if t == "arg":
        return f"(WExpr.arg {e[1]})"
    if t == "init":
        return f"(WExpr.init {e[1]})"
    if t == "un":
        return f"(WExpr.un UnOp.{e[1]} {lean_w(e[2])})"
    if t == "bin":
        return f"(WExpr.bin BinOp.{e[1]} {lean_w(e[2])} {lean_w(e[3])})"
    if t == "sel":
        return f"(WExpr.sel {lean_w(e[1])} {lean_w(e[2])} {lean_w(e[3])})"
    raise Unsupported(t)


def lean_f(f):
    t = f[0]
    if t == "fact":
        return f"(F.fact {lean_w(f[1])} {'true' if f[2] else 'false'})"
    if t in ("le", "lt", "eq"):
        return f"(F.{t} {lean_w(f[1])} {lean_w(f[2])})"
    if t == "lin":
        return f"(F.lin {' '.join(lean_w(w) for w in f[1:5])})"
    if t == "or":
        return f"(F.or {lean_f(f[1])} {lean_f(f[2])})"
    if t == "not":
        return f"(F.not {lean_f(f[1])})"
    raise Unsupported(t)


def lean_b(b):
    return f"(OByte.lit {b[1]})" if b[0] == "blit" else f"(OByte.byteOf {lean_w(b[1])} {b[2]})"


def lean_tree(T):
    if T[0] == "ite":
        return f"(Tree.ite {lean_w(T[1])} {lean_tree(T[2])} {lean_tree(T[3])})"
    if T[4] != 0:
        raise Unsupported("logs in the property tree")
    out = "[" + ", ".join(lean_b(b) for b in T[2]) + "]"
    ws = "[" + ", ".join(f"({lean_w(k)}, {lean_w(v)})" for k, v in T[3]) + "]"
    return f"(Tree.halt {'true' if T[1] else 'false'} {out} {ws} [])"


def target_tree(spec):
    """'returns-arg:i': the call succeeds, returns argument i as one word and writes and logs
    nothing; 'returns-arg-plus:i:c': returns argument i plus c; else a JSON tree file."""
    m = re.fullmatch(r"returns-arg:(\d+)", spec)
    if m:
        w = arg(int(m.group(1)))
        return ["halt", True, [["byteOf", w, i] for i in range(32)], [], 0]
    m = re.fullmatch(r"returns-arg-plus:(\d+):(\d+)", spec)
    if m:
        w = badd(arg(int(m.group(1))), lit(int(m.group(2))))
        return ["halt", True, [["byteOf", w, i] for i in range(32)], [], 0]
    with open(spec) as fh:
        return json.load(fh)


# ---------------------------------------------------------------------------
# Lean emission.


def extract_def(path, name):
    text = open(path).read()
    m = re.search(rf"^def {name} .*?(?=^\S|\Z)", text, re.S | re.M)
    if not m:
        raise SystemExit(f"no 'def {name}' in {path}")
    d = m.group(0).rstrip() + "\n"
    return d.replace("def stage0 : MIR.Lowered.Module", "def stage0 : SolarTV.MIR.Lowered.Module")


def prelude(args):
    seqw = open(args.seqw).read()
    body = "\n".join(l for l in seqw.split("\n") if not l.startswith(("import ", "#print")))
    return ("import Verify.LoopSource.Equiv.Emit\nimport Verify.LoopSource.Equiv.Seq\n\n"
            f"-- Inlined from {os.path.basename(args.seqw)} (kernel-checked) so the module is self-contained.\n"
            + body.strip() + "\n\n"
            f"namespace {args.ns}\nopen SolidCore.Refinement.Generic Verify.LoopSource.Equiv\n\n"
            f"/-- solar's lowered MIR (verbatim from {os.path.basename(args.stage0)}). -/\n"
            + extract_def(args.stage0, "stage0") + "\n"
            f"/-- The cut-point certificate (from {os.path.basename(args.cut)}; checked by Cut.check). -/\n"
            + extract_def(args.cut, "cut") + f"\nend {args.ns}\n")


def defs_text(T, I, mu):
    return (f"def T : Tree := {lean_tree(T)}\n\n"
            f"def I : List F := [{', '.join(lean_f(f) for f in I)}]\n\n"
            f"def μ : WExpr := {lean_w(mu)}\n\n")


OPENS = ("open SolidCore.Refinement.Generic Verify.LoopSource.Equiv Verify.LoopSource.Equiv.Inv "
         "Verify.Phase3.Infer\n\n")


def write_gen(args, T, I, mu, n):
    pre = prelude(args)
    with open(args.prelude_out, "w") as fh:
        fh.write(pre)
    chk = f"cut.check .off stage0 {args.sel} {n}"
    text = (pre + f"\nnamespace {args.ns}\n" + OPENS
            + "/-! Generated by tools/certify/loop_infer.py: elaborating it writes the obligation module. -/\n\n"
            + defs_text(T, I, mu)
            + "#eval show IO Unit from do\n"
            f"  match {chk} with\n"
            "  | none => throw (IO.userError \"cut.check is none\")\n"
            "  | some (le, li) =>\n"
            f"    match allSeqsW {n} cut.cases T le li I μ with\n"
            "    | none => throw (IO.userError \"allSeqsW is none\")\n"
            "    | some ss =>\n"
            f"      let pre ← IO.FS.readFile {json.dumps(args.prelude_out)}\n"
            f"      IO.FS.writeFile {json.dumps(args.obl)} (emitW pre {json.dumps(args.ns)} {json.dumps(chk)} {n} \"cut.cases\" le li T I μ ss)\n"
            "      IO.println s!\"wrote {ss.length} sequents ({ss.eraseDups.length} unique)\"\n\n"
            f"end {args.ns}\n")
    with open(args.gen, "w") as fh:
        fh.write(text)


def write_neg(args, path, T, I, mu, n, nhw, cex, why):
    """A module refusing (T, I, mu) in the kernel at z3's counterexample valuation."""
    pre = prelude(args)
    vals = [cex.get(f"a{i}", 0) for i in range(n + nhw)]
    store = {int(k[1:]): v for k, v in cex.items() if k.startswith("s")}
    st = "fun _ => 0"
    for k, v in store.items():
        st = f"fun k => if k = {k} then {v} else ({st}) k"
    chk = f"cut.check .off stage0 {args.sel} {n}"
    text = (pre + f"\nnamespace {args.ns}\n" + OPENS
            + f"/-! Generated by tools/certify/loop_infer.py: a rejected candidate, refused in the kernel.\n\n{why}\n-/\n\n"
            + defs_text(T, I, mu)
            + f"def ss? : Option (List Seq) := ({chk}).bind fun r => allSeqsW {n} cut.cases T r.1 r.2 I μ\n\n"
            "/-- z3's counterexample valuation (call words, then header words). -/\n"
            f"def σ : Val := {{ args := {vals}, store := {st}, hash := fun _ _ => 0, caller := 0 }}\n\n"
            "theorem rejects : rejectsAny σ ss? = true := by decide +kernel\n\n"
            "/-- **The candidate is refused**: some sequent of its obligations has a counterexample, so\n"
            "obl_of_seqsW cannot be applied to it. -/\n"
            "theorem refused : ∀ ss, ss? = some ss → ¬ ∀ s ∈ ss, s.Valid :=\n"
            "  fun ss h => rejectsAny_sound (h ▸ rejects)\n\n"
            f"end {args.ns}\n\n#print axioms {args.ns}.refused\n")
    with open(path, "w") as fh:
        fh.write(text)


# ---------------------------------------------------------------------------
# The interval fallback: abstract interpretation over interval bounds with thresholds.


def subterms(e):
    yield e
    if e[0] == "un":
        yield from subterms(e[2])
    elif e[0] == "bin":
        yield from subterms(e[2])
        yield from subterms(e[3])
    elif e[0] == "sel":
        for x in e[1:4]:
            yield from subterms(x)
    elif e[0] == "sinit":
        yield from subterms(e[1])


def call_only(n, e):
    """A word of the call alone: literals and the call's first n words (no header word, no storage)."""
    return all(x[0] == "lit" or (x[0] == "arg" and x[1] < n) for x in subterms(e)
               if x[0] in ("lit", "arg", "init", "sinit", "caller", "hash"))


def run_leaves(n, le, li, args, fuel=64):
    """The header valuations a concrete call reaches by the leaves (as EmitSeq.traceFrom)."""
    def leaf(ls, vals):
        for K, o in ls:
            if all((weval(e, vals) == 0) == (not b) for e, b in K):
                return o
        return None

    out, o, vals = [], leaf(le, args), list(args)
    while o is not None and o[0] == "back" and len(out) < fuel:
        vals = list(args) + [weval(e, vals) for e in o[1]]
        out.append(vals)
        o = leaf(li, vals)
    return out


def interval_cands(n, nhw, le, li, calls):
    """Candidate bounds of each header word h: lo <= h and h <= hi, lo and hi drawn from the
    thresholds (0, the call's words, every literal and call-only word of a fact or a back edge,
    and the bounds of the exit tests), and h = e for a word every back edge keeps. Houdini then
    keeps the greatest inductive subset (initiation and preservation, z3 on the Lean sequents'
    sliced form): the abstract fixpoint of the interval domain over these thresholds. Candidates
    refuted by a concrete run of the leaves are dropped first."""
    th = [lit(0)] + [arg(i) for i in range(n)]
    for K, o in le + li:
        ws = [e for e, _ in K] + (list(o[1]) if o[0] == "back" else [])
        for w in ws:
            for x in subterms(w):
                if x[0] == "lit" and x[1] < W // 2:
                    th.append(x)
                elif x[0] == "bin" and x[1] in ("lt", "gt") and call_only(n, x[3]):
                    th.append(x[3])
                elif x[0] == "bin" and x[1] in ("lt", "gt") and call_only(n, x[2]):
                    th.append(x[2])
            if o[0] == "back":
                th += [w for w in o[1] if call_only(n, w)]
    th = dedup(th)
    cands = []
    backs = [o[1] for _, o in li if o[0] == "back"]
    for k in range(nhw):
        h = arg(n + k)
        cands += [["le", t, h] for t in th if t != lit(0)]
        cands += [["le", h, t] for t in th]
        if backs and all(len(b) > k and b[k] == h for b in backs):
            ents = [o[1][k] for _, o in le if o[0] == "back" and len(o[1]) > k]
            if ents and all(e == ents[0] for e in ents) and call_only(n, ents[0]):
                cands.append(["eq", h, ents[0]])
    trace = [v for c in calls for v in run_leaves(n, le, li, c)]
    return dedup([f for f in cands if all(fholds(f, v) for v in trace)])


# ---------------------------------------------------------------------------
# The driver.


def discover(engine, n, nhw, cases, T, le, li, calls, report, args, log):
    """One engine's invariant and measure: (I, mu) when every sequent screens valid, else None,
    with what happened in report[engine]."""
    rep = report.setdefault("engines", {}).setdefault(engine, {})
    if engine == "spacer":
        log("spacer: solving the Horn clauses (init, preservation, query: every end is the property)")
        st, model, _, (slots, live) = spacer(le, li, n, nhw, T, args.timeout)
        rep["status"] = st
        if model is None:
            if st == "unsat":
                rep["verdict"] = "refuted"
                log("spacer: unsat: the Horn clauses have no solution, so no invariant over these words "
                    "makes every end the property (a refutation of this encoding, not of the source)")
            else:
                rep["verdict"] = "undecided"
                log(f"spacer: {st}: no usable model (unknown, timeout or no model): this is a refusal to "
                    "discover, not a refutation")
            return None
        log(f"spacer: Inv({', '.join(model[0])}) := {sexp_str(model[1])}")
        I, dropped = translate(model, n, nhw, slots, live)
        for a, why in dropped:
            log(f"  untranslatable atom dropped: {a}: {why}")
        rep["dropped"] = dropped
    elif engine == "interval":
        I = interval_cands(n, nhw, le, li, calls)
        log(f"interval: {len(I)} candidate bounds survive the concrete runs")
    else:
        raise SystemExit(f"unknown engine {engine}")
    I = houdini(n, cases, T, le, li, I, log)
    log(f"{engine}: inductive: " + ("; ".join(fmt_f(f) for f in I) or "(none)"))
    mu = find_measure(n, nhw, cases, T, le, li, I, log)
    if mu is None:
        rep["verdict"] = "no measure"
        log(f"{engine}: no measure falls on every back edge")
        return None
    seqs, fails = check_all(n, cases, T, le, li, I, mu)
    rep.update({"I_math": [fmt_f(f) for f in I], "mu_math": fmt_w(mu), "failed": len(fails)})
    if fails:
        rep["verdict"] = "candidate fails"
        for fl in fails[:5]:
            log(f"{engine}: FAILED {fl['kind']}: goal {fl['goal']} ({fl['status']}); counterexample {fl['counterexample']}")
        return None
    rep["verdict"] = "found"
    return I, mu


def main(argv=None):
    ap = argparse.ArgumentParser(description="Loop invariant and measure discovery (untrusted).")
    ap.add_argument("leaves")
    ap.add_argument("--target", help="the property tree, only for leaves that carry no source tree")
    ap.add_argument("--candidate", help='JSON {"I": [...], "mu": ...}: check it instead of discovering')
    ap.add_argument("--engines", default="spacer,interval",
                    help="discovery engines in order (spacer: Horn clauses; interval: interval bounds)")
    ap.add_argument("--emit-candidate", help="write the found I and mu, as JSON and as Lean terms")
    ap.add_argument("--stage0")
    ap.add_argument("--cut")
    ap.add_argument("--seqw")
    ap.add_argument("--ns", default="Verify.Phase3.Infer.Gen")
    ap.add_argument("--gen")
    ap.add_argument("--obl")
    ap.add_argument("--prelude-out")
    ap.add_argument("--lean-neg")
    ap.add_argument("--timeout", type=int, default=60)
    args = ap.parse_args(argv)

    def log(s):
        print(s, file=sys.stderr)

    L = json.load(open(args.leaves))
    n, nhw, le, li, cases = L["n"], L["nhw"], L["le"], L["li"], L["cases"]
    args.sel = L["sel"]
    report = {"format": "loop infer 1", "fn": L.get("fn")}
    if "T" in L:
        # The leaves carry the tree of the source's own erasure (GenInfer.exportLeaves): that is the
        # property, and the obligations the Lean side states are over it. A separate target would
        # replace the source's property with one the caller chose.
        if args.target:
            log("refused: the leaves carry the source's erased tree; --target would replace it")
            report["refused"] = "target given for leaves with a source tree"
            print(json.dumps(report))
            return 2
        T = L["T"]
        report["target"] = f"the source's tree ({L.get('tree_source', 'leaves file')})"
    elif args.target:
        T = target_tree(args.target)
        report["target"] = args.target
    else:
        raise SystemExit("the leaves carry no tree: give --target")
    calls = [[v] * n for v in (0, 1, 2, 3, 7, W - 1)]

    found = None
    if args.candidate:
        c = json.load(open(args.candidate))
        I, mu = c["I"], c.get("mu")
        report["source"] = "candidate"
        if mu is None:
            mu = find_measure(n, nhw, cases, T, le, li, I, log)
        found = (I, mu) if mu is not None else None
    else:
        for engine in args.engines.split(","):
            found = discover(engine, n, nhw, cases, T, le, li, calls, report, args, log)
            if found:
                report["source"] = engine
                break
    if found is None and not args.candidate:
        verdicts = {e: r.get("verdict") for e, r in report.get("engines", {}).items()}
        report["discovery"] = "refuted" if verdicts and all(v == "refuted" for v in verdicts.values()) else "undecided"
        cex = concrete_cex(n, le, li, T, calls)
        if cex:
            report["discovery"] = "refuted"
            log(f"  concrete counterexample: call {cex['call']} after {cex['iterations']} iterations: "
                f"the MIR ends {cex['mir']}, the property says {cex['property']}")
            report["counterexample"] = cex
        log(f"no invariant: {report['discovery']} ({verdicts})")
        print(json.dumps(report))
        return 2
    if found is None:
        log("no measure falls on every back edge")
        report["measure"] = None
        print(json.dumps(report))
        return 2
    I, mu = found
    seqs, fails = check_all(n, cases, T, le, li, I, mu)
    report.update({"I": I, "I_math": [fmt_f(f) for f in I], "mu": mu, "mu_math": fmt_w(mu),
                   "sequents": len(seqs), "failed": fails})
    for fl in fails:
        log(f"FAILED {fl['kind']}: goal {fl['goal']} ({fl['status']})")
        log(f"  hypotheses: {'; '.join(fl['hyps'])}")
        log(f"  counterexample: {fl['counterexample']}")
    if fails:
        cex = concrete_cex(n, le, li, T, calls)
        if cex:
            log(f"  concrete run: call {cex['call']} after {cex['iterations']} iterations: the MIR ends "
                f"{cex['mir']}, the property says {cex['property']}")
            report["concrete"] = cex
        if args.lean_neg:
            fl = next((x for x in fails if isinstance(x["counterexample"], dict)), None)
            if fl:
                write_neg(args, args.lean_neg, T, I, mu, n, nhw, fl["counterexample"],
                          f"Failing sequent: {fl['kind']}: goal {fl['goal']}; counterexample {fl['counterexample']}.")
                report["neg"] = args.lean_neg
        print(json.dumps(report))
        return 2
    report["discovery"] = "found"
    log(f"all {len(seqs)} sequents valid by z3 (screening only); I = {report['I_math']}, mu = {report['mu_math']}")
    if args.emit_candidate:
        with open(args.emit_candidate, "w") as fh:
            json.dump({"format": "loop infer candidate 1", "source": report.get("source"), "I": I, "mu": mu,
                       "I_lean": "[" + ", ".join(lean_f(f) for f in I) + "]", "mu_lean": lean_w(mu),
                       "I_math": report["I_math"], "mu_math": report["mu_math"]}, fh)
        report["candidate_out"] = args.emit_candidate
    if args.gen:
        write_gen(args, T, I, mu, n)
        report["gen"] = args.gen
    print(json.dumps(report))
    return 0


if __name__ == "__main__":
    sys.exit(main())
