"""Cut points at recursive call sites: the stage proofs of a self-recursive function.

cutpoints.py relates two stages through cut points with one relation per call string that
reaches a cut point, so a recursive call, whose call strings are unbounded, fails closed there
(mir_contexts). This module relates the stages of a module with one self-recursive function F
instead, with relations over the whole call stack:

- Cut points: F's entry (block 0, entered by a call) and, for each recursive call site in F,
  the point where F resumes after it (MIR .resume F b ..; in EVM IR the block the call's
  return label names, as Schedule.json records it).
- Call frames: an inductive relation Fr between the two sides' call stacks below a
  configuration of F. In MIR a frame is the Return a call pushes (the caller's block, the
  rest of it, the caller's values); in EVM IR it is the return label and the caller's words
  below the callee's arguments (Schedule.json "below", with the caller's own return label among
  them). Fr has one constructor per call site of F: an outer site (a caller outside F, entered
  by tail calls only, so nothing is below its frame but words it never reads) or a recursive
  site, whose constructor takes Fr of the stack below it. MIR to MIR relates the caller values
  each side reads after the call; EVM IR to EVM IR relates labels by the compiler's block
  numbers and keeps the caller's words the same.
- Relations: the first configurations; F's entry (the arguments related, Fr of the stack);
  F's resume points (the values live after the call related, Fr of the stack below).
- Segments: each proof first splits Fr into its constructors (cases), so the frame a leave
  or return jump pops is a known call site's, and then runs sym_seg as cutpoints.py's proofs
  do, with relation lemmas that build Fr for a pushed frame from the hypothesis and take Fr of
  the stack below a popped one.

The relations are not trusted: a wrong frame relation makes a segment end differently or a
relation lemma fail, and the Lean proof fails by mismatch. The scope of this slice is refused by
name (Unsupported): mutual recursion, a recursive tail call, a loop anywhere in the module, a
function F calls other than itself, an outer caller that is itself called, several results, and
words the schedule leaves below a frame that are not the caller's values or its return label.
"""

import json
import re

import cutpoints as cp

MIR_CFG = cp.MIR_CFG
EVM_CFG = cp.EVM_CFG


class Unsupported(cp.Unsupported):
    """A recursive module outside this slice's shape."""


def call_graph(funcs):
    """Edges f -> g of internal calls and tail calls."""
    edges = {}
    for f, fn in enumerate(funcs):
        out = edges.setdefault(f, set())
        for _, _, callee, _ in cp.mir_calls(fn):
            out.add(callee)
        for _, _, term in fn["blocks"].values():
            m = re.match(r"\.tailCall (\d+)", term)
            if m:
                out.add(int(m[1]))
    return edges


def recursive_fns(funcs):
    """The functions that can reach themselves through calls."""
    edges = call_graph(funcs)
    out = []
    for f in edges:
        seen, work = set(), list(edges[f])
        while work:
            g = work.pop()
            if g in seen or g not in edges:
                continue
            seen.add(g)
            work.extend(edges[g])
        if f in seen:
            out.append(f)
    return out


def is_recursive(gen):
    """Whether the run's first MIR stage has a recursive function."""
    stage0 = gen / "Stage0.lean"
    return stage0.exists() and bool(recursive_fns(cp.parse_mir(stage0.read_text())))


def shape(pass_name, funcs):
    """F, its recursive call sites (F, block, ordinal) and its outer call sites."""
    rec = recursive_fns(funcs)
    if len(rec) != 1:
        raise Unsupported(
            f"{pass_name}: mutual recursion ("
            + ", ".join("@" + funcs[f]["name"] for f in rec)
            + ")"
        )
    F = rec[0]
    fn = funcs[F]
    for _, _, term in fn["blocks"].values():
        if re.match(rf"\.tailCall {F}\b", term):
            raise Unsupported(f"{pass_name}: a recursive tail call in @{fn['name']}")
    for g in funcs:
        if g["blocks"] and cp.mir_headers(g)[0]:
            raise Unsupported(f"{pass_name}: a loop in @{g['name']} beside recursion")
    if any(callee != F for _, _, callee, _ in cp.mir_calls(fn)):
        raise Unsupported(
            f"{pass_name}: @{fn['name']} calls other functions than itself"
        )
    sites = [(F, b, k) for b, k, callee, _ in cp.mir_calls(fn) if callee == F]
    outer = []
    for f, g in enumerate(funcs):
        if f == F:
            continue
        for b, k, callee, _ in cp.mir_calls(g):
            if callee == F:
                outer.append((f, b, k))
    # Every outer caller runs only under tail calls from the entry: nothing below its frame.
    called = {callee for g in funcs for _, _, callee, _ in cp.mir_calls(g)}
    for f, _, _ in outer:
        if f in called:
            raise Unsupported(
                f"{pass_name}: the caller @{funcs[f]['name']} of a recursive function is itself called"
            )
    for f, b, k in sites + outer:
        _, t = cp.call_site(funcs[f], b, k)
        if len(t) > 4 and t[4] != "[]":
            raise Unsupported(f"{pass_name}: a recursive call with several results")
        if len(t) > 5 and t[5] != "none":
            raise Unsupported(
                f"{pass_name}: a recursive call that publishes its results"
            )
    if not fn["params"]:
        raise Unsupported(f"{pass_name}: a recursive function without parameters")
    return F, sites, outer


def dst_of(funcs, site):
    f, b, k = site
    _, t = cp.call_site(funcs[f], b, k)
    m = re.fullmatch(r"\(?some (\d+)\)?", t[1])
    return int(m[1]) if m else None


class EvmView:
    """F's entry, the return blocks of its call sites and the words below each frame in one
    EVM IR program, from Schedule.json (compiler block numbers) and the program's block order."""

    def __init__(self, pass_name, blocks, schedule, F, sites, outer):
        ids = [i for i, _ in blocks]
        fmaps = {fm["function"]: fm for fm in schedule["functions"]}

        def pos(ident, what):
            if ids.count(ident) != 1:
                raise Unsupported(
                    f"{pass_name}: {what} bb{ident} is not one block of the program"
                )
            return ids.index(ident)

        fm = fmaps.get(F)
        if fm is None or fm["unsupported"] or fm["rebuilt"]:
            raise Unsupported(f"{pass_name}: no schedule map of the recursive function")
        b0 = next((x for x in fm["blocks"] if x["block"] == 0), None)
        if b0 is None or b0["start"] is None or b0["start"][1] != 0:
            raise Unsupported(
                f"{pass_name}: the recursive function's entry starts no scheduled block"
            )
        self.entry = pos(b0["start"][0], "the recursive function's entry")
        # Top of the stack first.
        self.entry_layout = list(reversed(b0["layout"]))
        self.ret, self.below = {}, {}
        for site in sites + outer:
            f, b, k = site
            calls = [
                c for c in fmaps.get(f, {}).get("calls", []) if c.get("block") == b
            ]
            if k >= len(calls):
                raise Unsupported(
                    f"{pass_name}: the schedule map records no return of a call in bb{b}"
                )
            self.ret[site] = pos(calls[k]["return"], "the return block")
            self.below[site] = list(reversed(calls[k]["below"]))
        for x in self.entry_layout:
            if x != "ret" and not re.fullmatch(r"v\d+", x):
                raise Unsupported(f"{pass_name}: the entry layout holds {x}")
        if self.entry_layout.count("ret") != 1:
            raise Unsupported(f"{pass_name}: the entry layout holds no return address")
        for site in sites:
            if self.below[site].count("ret") != 1:
                raise Unsupported(
                    f"{pass_name}: a recursive call keeps no return address below it"
                )
        for site in outer:
            if "ret" in self.below[site]:
                raise Unsupported(
                    f"{pass_name}: an outer caller keeps a return address"
                )
        for site in sites + outer:
            for x in self.below[site]:
                if x != "ret" and not re.fullmatch(r"v\d+", x):
                    raise Unsupported(
                        f"{pass_name}: a call keeps {x} below its arguments"
                    )


def stack(syms, word, label, tail):
    """A stack term: the symbols top first ('ret' the label, 'vK' a word), then the tail."""
    parts = [label if x == "ret" else word(int(x[1:])) for x in syms]
    return " :: ".join([f"({q})" for q in parts] + [tail])


# The stack below a frame of the recursive function, plus the measure's words for the frames it may
# still push, stays under this bound: 40 words below the EVM's 1024, room for the frame itself.
BOUND = 1024 - 40


def W(n):
    # A return label as the word pushLabel pushes (BitVec.ofNat), so that it matches the
    # evaluated stack syntactically rather than through the OfNat instance.
    return f"(BitVec.ofNat 256 {n})"


def resume_pos(funcs, site):
    """The position F resumes at after the call of site returns."""
    f, b, k = site
    _, insts, term = funcs[f]["blocks"][b]
    i, _ = cp.call_site(funcs[f], b, k)
    rest = "[" + ", ".join(insts[i + 1 :]) + "]"
    return f"MIR.Lowered.Pos.resume {f} {b} {rest} ({term})"


def word_vars(*belows):
    return sorted({int(x[1:]) for bl in belows for x in bl if x != "ret"})


def stage_body(gen, a, b, heartbeats=2000000, recdepth=None):
    """The cut-point proof of the stage from a to b of a module with one self-recursive
    function, after the module header, or raise Unsupported."""
    ta = (gen / a["file"]).read_text()
    tb = (gen / b["file"]).read_text()
    pass_name = b["pass"]
    F, sites, outer = shape(pass_name, cp.parse_mir((gen / "Stage0.lean").read_text()))
    sf = gen / "Schedule.json"
    schedule = json.loads(sf.read_text()) if sf.exists() else None
    A, B, p = a["def"], b["def"], b["def"]
    ka, kb = a["kind"], b["kind"]
    fa = cp.parse_mir(ta) if ka == "mir" else None
    fb = cp.parse_mir(tb) if kb == "mir" else None
    for fs in (fa, fb):
        if fs is not None and shape(pass_name, fs) != (F, sites, outer):
            raise Unsupported(
                f"{pass_name}: the recursion's call sites differ across the pass"
            )
    if kb != "mir" and schedule is None:
        raise Unsupported(f"{pass_name}: no Schedule.json from the exporter")
    va = (
        EvmView(pass_name, cp.parse_evm(ta), schedule, F, sites, outer)
        if ka != "mir"
        else None
    )
    vb = (
        EvmView(pass_name, cp.parse_evm(tb), schedule, F, sites, outer)
        if kb != "mir"
        else None
    )
    names = {s: f"r{n}" for n, s in enumerate(sites)}
    names.update({s: f"o{n}" for n, s in enumerate(outer)})
    Fr = f"{p}_Fr"
    ca = MIR_CFG if ka == "mir" else EVM_CFG
    cb = MIR_CFG if kb == "mir" else EVM_CFG
    LIST, ENV, WD, STK = (
        "List MIR.Lowered.Return",
        "MIR.Lowered.Env",
        "EvmRules.Word",
        "EVM.Stack",
    )
    kind = "mm" if kb == "mir" else ("me" if ka == "mir" else "ee")
    out = []

    # The measure: the parameter of F every recursive call site keeps below its arguments (it bounds
    # how many frames may still be pushed), and w, the most words a recursive frame keeps.
    views = [v for v in (va, vb) if v is not None]
    mp = None
    if views:
        for cand in [int(x[1:]) for x in views[0].entry_layout if x != "ret"]:
            if all(f"v{cand}" in v.below[s] for v in views for s in sites):
                mp = cand
                break
        if mp is None:
            raise Unsupported(
                f"{pass_name}: no parameter of the recursive function is kept below every recursive call (no measure)"
            )
    w = max((len(v.below[s]) for v in views for s in sites), default=0)

    def bnd(tail, word):
        return f"({tail}).length + {w} * ({word}).toNat ≤ {BOUND}"

    def mir_cut(side):
        name = f"{p}_cut{side}"
        enter = f"(f == {F} && b == 0)"
        res = " || ".join(f"b == {blk}" for blk in sorted({s[1] for s in sites}))
        resume = f"(f == {F} && ({res}))"
        return (
            name,
            [f"{name}_enter", f"{name}_resume"],
            (
                f"def {name} : {MIR_CFG} → Bool := fun x =>\n"
                f"  match x.2.1 with\n"
                f"  | .enter f _ b => {enter}\n"
                f"  | .resume f b .. => {resume}\n\n"
                f"theorem {name}_enter (rs : {LIST}) (f p b : Nat) (env : {ENV}) (s : State) :\n"
                f"    {name} (rs, .enter f p b, env, s) = {enter} := rfl\n\n"
                f"theorem {name}_resume (rs : {LIST}) (f b : Nat) (is : List MIR.Lowered.Inst) "
                f"(t : MIR.Lowered.Terminator) (env : {ENV}) (s : State) :\n"
                f"    {name} (rs, .resume f b is t, env, s) = {resume} := rfl\n\n"
            ),
        )

    def evm_cut(side, v):
        name = f"{p}_cut{side}"
        tests = [v.entry] + sorted({v.ret[s] for s in sites})
        test = " || ".join(f"b == {i}" for i in tests)
        return (
            name,
            [f"{name}_mk"],
            (
                f"def {name} : {EVM_CFG} → Bool := fun x => {test.replace('b ==', 'x.1 ==')}\n\n"
                f"theorem {name}_mk (b : Nat) (st : {STK}) (s : State) :\n"
                f"    {name} (b, st, s) = ({test}) := rfl\n\n"
            ),
        )

    cut_a, unf_a, t = mir_cut("A") if ka == "mir" else evm_cut("A", va)
    out.append(t)
    cut_b, unf_b, t = mir_cut("B") if kb == "mir" else evm_cut("B", vb)
    out.append(t)
    unf = ", ".join(unf_a + unf_b)

    def fra(s, env):
        return cp.frame_term(fa, s, env)

    def frb(s, env):
        return cp.frame_term(fb, s, env)

    def live(s):
        f = s[0]
        return sorted(cp.live_after_call(fa[f], s) | cp.live_after_call(fb[f], s))

    if kind == "mm" and len(fa[F]["params"]) != len(fb[F]["params"]):
        raise Unsupported(
            f"{pass_name}: the recursive function's parameters differ across the pass"
        )
    if kind == "ee" and va.entry_layout != vb.entry_layout:
        raise Unsupported(
            f"{pass_name}: the recursive function's entry layout differs across the pass"
        )
    P = "'"

    # Stack texts.
    def me_below(s, env, L, tail):
        return stack(vb.below[s], lambda i: f"{env} {i}", L, tail)

    def me_entry(env, L, tail):
        return stack(vb.entry_layout, lambda i: f"{env} {i}", L, tail)

    def ee_below(v, s, L, tail):
        return stack(v.below[s], lambda i: f"w{i}", L, tail)

    def ee_entry(v, L, tail):
        return stack(v.entry_layout, lambda i: f"a{i}", L, tail)

    # The frame relation.
    ctors = []
    for s in outer + sites:
        o = s in outer
        if kind == "mm":
            eqs = " ".join(f"(h{i} : c' {x} = c {x})" for i, x in enumerate(live(s)))
            if o:
                ctors.append(
                    f"  | {names[s]} (c c' : {ENV}) {eqs} :\n      {Fr} [{fra(s, 'c')}] [{frb(s, 'c' + P)}]"
                )
            else:
                ctors.append(
                    f"  | {names[s]} (c c' : {ENV}) (rs rs' : {LIST}) {eqs} (t : {Fr} rs rs') :\n"
                    f"      {Fr} ({fra(s, 'c')} :: rs) ({frb(s, 'c' + P)} :: rs')"
                )
        elif kind == "me":
            if o:
                ctors.append(
                    f"  | {names[s]} (c : {ENV}) (junk : {STK}) (hb : junk.length ≤ {BOUND}) :\n"
                    f"      {Fr} [{fra(s, 'c')}] {W(vb.ret[s])} ({me_below(s, 'c', 'L', 'junk')})"
                )
            else:
                ctors.append(
                    f"  | {names[s]} (c : {ENV}) (rs : {LIST}) (L : {WD}) (T : {STK}) (hb : {bnd('T', f'c {mp}')})\n"
                    f"      (t : {Fr} rs L T) :\n"
                    f"      {Fr} ({fra(s, 'c')} :: rs) {W(vb.ret[s])} ({me_below(s, 'c', 'L', 'T')})"
                )
        else:
            wb = " ".join(f"(w{i} : {WD})" for i in word_vars(va.below[s], vb.below[s]))
            if o:
                ctors.append(
                    f"  | {names[s]} {wb} (T T' : {STK}) (hb : T.length ≤ {BOUND}) (hb' : T'.length ≤ {BOUND}) :\n"
                    f"      {Fr} {W(va.ret[s])} ({ee_below(va, s, 'L', 'T')}) {W(vb.ret[s])} ({ee_below(vb, s, 'L', 'T' + P)})"
                )
            else:
                ctors.append(
                    f"  | {names[s]} {wb} (L L' : {WD}) (T T' : {STK}) (hb : {bnd('T', f'w{mp}')}) (hb' : {bnd('T' + P, f'w{mp}')})\n"
                    f"      (t : {Fr} L T L' T') :\n"
                    f"      {Fr} {W(va.ret[s])} ({ee_below(va, s, 'L', 'T')}) {W(vb.ret[s])} ({ee_below(vb, s, 'L' + P, 'T' + P)})"
                )
    sig = {
        "mm": f"{LIST} → {LIST} → Prop",
        "me": f"{LIST} → {WD} → {STK} → Prop",
        "ee": f"{WD} → {STK} → {WD} → {STK} → Prop",
    }[kind]
    out.append(
        "/-- The call frames below a configuration of the recursive function, related frame by frame: one\n"
        "constructor per call site, a recursive site's over the frames below it"
        + (
            ""
            if kind == "mm"
            else ", with the stack below each frame\nbounded by the measure, so that no segment overflows the 1024-word stack"
        )
        + ". -/\n"
        f"inductive {Fr} : {sig}\n" + "\n".join(ctors) + "\n\n"
    )

    # The relations: the first configurations, F's entry, then each resume point.
    rels = []
    if kind == "mm":
        par = [f"env' {y} = env {x}" for x, y in zip(fa[F]["params"], fb[F]["params"])]
        sv, tv = (
            [("rs", LIST), ("env", ENV), ("s", "State")],
            [("rs'", LIST), ("env'", ENV)],
        )
        rels.append(
            {
                "src_vars": sv,
                "tgt_vars": tv,
                "src": f"(rs, .enter {F} 0 0, env, s)",
                "tgt": f"(rs', .enter {F} 0 0, env', s)",
                "conds": par + [f"{Fr} rs rs'"],
                "kind": "entry",
            }
        )
        for s in sites:
            d = dst_of(fa, s)
            eqs = [
                f"env' {x} = env {x}"
                for x in sorted(set(live(s)) | ({d} if d is not None else set()))
            ]
            rels.append(
                {
                    "src_vars": sv,
                    "tgt_vars": tv,
                    "src": f"(rs, {resume_pos(fa, s)}, env, s)",
                    "tgt": f"(rs', {resume_pos(fb, s)}, env', s)",
                    "conds": eqs + [f"{Fr} rs rs'"],
                    "kind": "resume",
                    "site": s,
                }
            )
    elif kind == "me":
        sv, tv = [("rs", LIST), ("env", ENV), ("s", "State")], [("L", WD), ("T", STK)]
        rels.append(
            {
                "src_vars": sv,
                "tgt_vars": tv,
                "src": f"(rs, .enter {F} 0 0, env, s)",
                "tgt": f"({vb.entry}, {me_entry('env', 'L', 'T')}, s)",
                "conds": [bnd("T", f"env {mp}"), f"{Fr} rs L T"],
                "kind": "entry",
            }
        )
        for s in sites:
            d = dst_of(fa, s)
            if d is None:
                raise Unsupported(
                    f"{pass_name}: a recursive call whose result no value binds"
                )
            rels.append(
                {
                    "src_vars": sv,
                    "tgt_vars": tv,
                    "src": f"(rs, {resume_pos(fa, s)}, env, s)",
                    "tgt": f"({vb.ret[s]}, (env {d}) :: {me_below(s, 'env', 'L', 'T')}, s)",
                    "conds": [bnd("T", f"env {mp}"), f"{Fr} rs L T"],
                    "kind": "resume",
                    "site": s,
                }
            )
    else:
        ews = word_vars(va.entry_layout)
        rels.append(
            {
                "src_vars": [(f"a{i}", WD) for i in ews]
                + [("L", WD), ("T", STK), ("s", "State")],
                "tgt_vars": [("L'", WD), ("T'", STK)],
                "src": f"({va.entry}, {ee_entry(va, 'L', 'T')}, s)",
                "tgt": f"({vb.entry}, {ee_entry(vb, 'L' + P, 'T' + P)}, s)",
                "conds": [bnd("T", f"a{mp}"), bnd("T'", f"a{mp}"), f"{Fr} L T L' T'"],
                "kind": "entry",
            }
        )
        for s in sites:
            rels.append(
                {
                    "src_vars": [("r", WD)]
                    + [(f"w{i}", WD) for i in word_vars(va.below[s], vb.below[s])]
                    + [("L", WD), ("T", STK), ("s", "State")],
                    "tgt_vars": [("L'", WD), ("T'", STK)],
                    "src": f"({va.ret[s]}, r :: {ee_below(va, s, 'L', 'T')}, s)",
                    "tgt": f"({vb.ret[s]}, r :: {ee_below(vb, s, 'L' + P, 'T' + P)}, s)",
                    "conds": [
                        bnd("T", f"w{mp}"),
                        bnd("T'", f"w{mp}"),
                        f"{Fr} L T L' T'",
                    ],
                    "kind": "resume",
                    "site": s,
                }
            )
    init = {
        "src_vars": [],
        "tgt_vars": [],
        "src": cp.init_term(ka, A),
        "tgt": cp.init_term(kb, B),
        "conds": [],
        "kind": "init",
    }
    allrels = [init] + rels

    def body(r):
        vs = r["src_vars"] + r["tgt_vars"]
        inner = " ∧ ".join([f"x = {r['src']}", f"x' = {r['tgt']}"] + r["conds"])
        return f"∃ {cp.binder_text(vs)}, {inner}" if vs else inner

    out.append(
        "/-- The relations at the first configurations, at the recursive function's entry and at each point it\n"
        "resumes after a recursive call, each over every depth of recursion through the frame relation. -/\n"
        f"def {p}_rels (call : Call) : List ({ca} → {cb} → Prop) :=\n  ["
        + ",\n   ".join(f"fun x x' => {body(r)}" for r in allrels)
        + "]\n\n"
    )

    # Relation lemmas: an outer call (from the first segment), a recursive call (the pushed frame's
    # constructor from the hypothesis on the stack below) and a resume (Fr of the stack below a
    # popped frame).
    mk_outer, mk_call, mk_resume = {}, {}, {}
    m1 = cp.mem_proof(1)
    ews = word_vars(va.entry_layout) if kind == "ee" else []
    abind = " ".join(f"{{a{i} : {WD}}}" for i in ews)
    aw = "".join(f"a{i}, " for i in ews)
    for n, s in enumerate(outer):
        name = f"{p}_mkO{n}"
        mk_outer[s] = name
        if kind == "mm":
            lv = live(s)
            par = list(zip(fa[F]["params"], fb[F]["params"]))
            concl = " ∧ ".join(
                [f"y = ([{frb(s, 'c' + P)}], .enter {F} 0 0, env', s)"]
                + [f"env' {y} = env {x}" for x, y in par]
                + [f"c' {x} = c {x}" for x in lv]
            )
            hp = [f"hp{i}" for i in range(len(par))]
            hc = [f"hc{i}" for i in range(len(lv))]
            out.append(
                f"theorem {name} {{call : Call}} {{c env : {ENV}}} {{s : State}} {{y : {cb}}}\n"
                f"    (h : ∃ (c' env' : {ENV}), {concl}) :\n"
                f"    ∃ Q ∈ {p}_rels call, Q ([{fra(s, 'c')}], .enter {F} 0 0, env, s) y := by\n"
                f"  obtain ⟨c', env', {', '.join(['hy'] + hp + hc)}⟩ := h\n"
                f"  exact rel_mem ({m1}) ⟨_, env, s, _, env', rfl, hy, {', '.join(hp + [(f'{Fr}.{names[s]} c c{P} ' + ' '.join(hc)).strip()])}⟩\n\n"
            )
        elif kind == "me":
            tl = me_below(s, "c", "L", "junk")
            tgt = f"({vb.entry}, {me_entry('env', W(vb.ret[s]), '(' + tl + ')')}, s)"
            out.append(
                f"theorem {name} {{call : Call}} {{c env : {ENV}}} {{s : State}} {{y : {cb}}}\n"
                f"    (h : ∃ (junk : {STK}), y = {tgt} ∧ {bnd(tl, f'env {mp}')} ∧ junk.length ≤ {BOUND}) :\n"
                f"    ∃ Q ∈ {p}_rels call, Q ([{fra(s, 'c')}], .enter {F} 0 0, env, s) y := by\n"
                f"  obtain ⟨junk, hy, hb, hj⟩ := h\n"
                f"  exact rel_mem ({m1}) ⟨_, env, s, _, _, rfl, hy, hb, {Fr}.{names[s]} c junk hj⟩\n\n"
            )
        else:
            ws = word_vars(va.below[s], vb.below[s])
            tla, tlb = ee_below(va, s, "L", "T"), ee_below(vb, s, "L", "T'")
            srcc = f"({va.entry}, {ee_entry(va, W(va.ret[s]), '(' + tla + ')')}, s)"
            tgt = f"({vb.entry}, {ee_entry(vb, W(vb.ret[s]), '(' + tlb + ')')}, s)"
            binds = " ".join([abind] + [f"{{w{i} : {WD}}}" for i in ws])
            out.append(
                f"theorem {name} {{call : Call}} {binds} {{T : {STK}}} {{s : State}} {{y : {cb}}}\n"
                f"    (h : ∃ (T' : {STK}), y = {tgt} ∧ {bnd(tla, f'a{mp}')} ∧ {bnd(tlb, f'a{mp}')} ∧\n"
                f"      T.length ≤ {BOUND} ∧ T'.length ≤ {BOUND}) :\n"
                f"    ∃ Q ∈ {p}_rels call, Q {srcc} y := by\n"
                f"  obtain ⟨T', hy, h1, h2, h3, h4⟩ := h\n"
                f"  exact rel_mem ({m1}) ⟨{aw}_, _, s, _, _, rfl, hy, h1, h2, {Fr}.{names[s]} {''.join(f'w{i} ' for i in ws)}T T' h3 h4⟩\n\n"
            )
    for n, s in enumerate(sites):
        name = f"{p}_mkC{n}"
        mk_call[s] = name
        if kind == "mm":
            lv = live(s)
            par = list(zip(fa[F]["params"], fb[F]["params"]))
            concl = " ∧ ".join(
                [f"y = ({frb(s, 'c' + P)} :: rs', .enter {F} 0 0, env', s)"]
                + [f"env' {y} = env {x}" for x, y in par]
                + [f"c' {x} = c {x}" for x in lv]
            )
            hp = [f"hp{i}" for i in range(len(par))]
            hc = [f"hc{i}" for i in range(len(lv))]
            out.append(
                f"theorem {name} {{call : Call}} {{rs rs' : {LIST}}} (hF : {Fr} rs rs') {{c env : {ENV}}} {{s : State}} {{y : {cb}}}\n"
                f"    (h : ∃ (c' env' : {ENV}), {concl}) :\n"
                f"    ∃ Q ∈ {p}_rels call, Q ({fra(s, 'c')} :: rs, .enter {F} 0 0, env, s) y := by\n"
                f"  obtain ⟨c', env', {', '.join(['hy'] + hp + hc)}⟩ := h\n"
                f"  exact rel_mem ({m1}) ⟨_, env, s, _, env', rfl, hy, {', '.join(hp + [f'{Fr}.{names[s]} c c{P} rs rs{P} ' + ''.join(h + ' ' for h in hc) + 'hF'])}⟩\n\n"
            )
        elif kind == "me":
            tl = me_below(s, "c", "L", "T")
            tgt = f"({vb.entry}, {me_entry('env', W(vb.ret[s]), '(' + tl + ')')}, s)"
            out.append(
                f"theorem {name} {{call : Call}} {{rs : {LIST}}} {{L : {WD}}} {{T : {STK}}} (hF : {Fr} rs L T)\n"
                f"    {{c env : {ENV}}} {{s : State}} {{y : {cb}}}\n"
                f"    (h : y = {tgt} ∧ {bnd('T', f'c {mp}')} ∧ {bnd(tl, f'env {mp}')}) :\n"
                f"    ∃ Q ∈ {p}_rels call, Q ({fra(s, 'c')} :: rs, .enter {F} 0 0, env, s) y := by\n"
                f"  obtain ⟨hy, hb, hn⟩ := h\n"
                f"  exact rel_mem ({m1}) ⟨_, env, s, _, _, rfl, hy, hn, {Fr}.{names[s]} c rs L T hb hF⟩\n\n"
            )
        else:
            ws = word_vars(va.below[s], vb.below[s])
            tla, tlb = ee_below(va, s, "L", "T"), ee_below(vb, s, "L'", "T'")
            srcc = f"({va.entry}, {ee_entry(va, W(va.ret[s]), '(' + tla + ')')}, s)"
            tgt = f"({vb.entry}, {ee_entry(vb, W(vb.ret[s]), '(' + tlb + ')')}, s)"
            binds = " ".join([abind] + [f"{{w{i} : {WD}}}" for i in ws])
            out.append(
                f"theorem {name} {{call : Call}} {{L L' : {WD}}} {{T T' : {STK}}} (hF : {Fr} L T L' T')\n"
                f"    {binds} {{s : State}} {{y : {cb}}}\n"
                f"    (h : y = {tgt} ∧ {bnd('T', f'w{mp}')} ∧ {bnd('T' + P, f'w{mp}')} ∧\n"
                f"      {bnd(tla, f'a{mp}')} ∧ {bnd(tlb, f'a{mp}')}) :\n"
                f"    ∃ Q ∈ {p}_rels call, Q {srcc} y := by\n"
                f"  obtain ⟨hy, h1, h2, h3, h4⟩ := h\n"
                f"  exact rel_mem ({m1}) ⟨{aw}_, _, s, _, _, rfl, hy, h3, h4, {Fr}.{names[s]} {''.join(f'w{i} ' for i in ws)}L L' T T' h1 h2 hF⟩\n\n"
            )
    for n, s in enumerate(sites):
        name = f"{p}_mkR{n}"
        mk_resume[s] = name
        r = rels[1 + n]
        mi = cp.mem_proof(2 + n)
        if kind == "mm":
            k = len(r["conds"]) - 1
            concl = " ∧ ".join([f"y = {r['tgt']}"] + r["conds"][:-1])
            hs = [f"h{i}" for i in range(k)]
            out.append(
                f"theorem {name} {{call : Call}} {{rs rs' : {LIST}}} (hF : {Fr} rs rs') {{env : {ENV}}} {{s : State}} {{y : {cb}}}\n"
                f"    (h : ∃ (env' : {ENV}), {concl}) :\n"
                f"    ∃ Q ∈ {p}_rels call, Q {r['src']} y := by\n"
                f"  obtain ⟨env', {', '.join(['hy'] + hs)}⟩ := h\n"
                f"  exact rel_mem ({mi}) ⟨rs, env, s, rs', env', rfl, hy, {', '.join(hs + ['hF'])}⟩\n\n"
            )
        elif kind == "me":
            out.append(
                f"theorem {name} {{call : Call}} {{rs : {LIST}}} {{L : {WD}}} {{T : {STK}}} (hF : {Fr} rs L T)\n"
                f"    {{env : {ENV}}} {{s : State}} {{y : {cb}}} (h : y = {r['tgt']} ∧ {r['conds'][0]}) :\n"
                f"    ∃ Q ∈ {p}_rels call, Q {r['src']} y := by\n"
                f"  obtain ⟨hy, hb⟩ := h\n"
                f"  exact rel_mem ({mi}) ⟨rs, env, s, L, T, rfl, hy, hb, hF⟩\n\n"
            )
        else:
            vs = [x for x, _ in r["src_vars"] if x not in ("L", "T", "s")]
            binds = " ".join(f"{{{x} : {WD}}}" for x in vs)
            out.append(
                f"theorem {name} {{call : Call}} {{L L' : {WD}}} {{T T' : {STK}}} (hF : {Fr} L T L' T')\n"
                f"    {binds} {{s : State}} {{y : {cb}}} (h : y = {r['tgt']} ∧ {r['conds'][0]} ∧ {r['conds'][1]}) :\n"
                f"    ∃ Q ∈ {p}_rels call, Q {r['src']} y := by\n"
                f"  obtain ⟨hy, h1, h2⟩ := h\n"
                f"  exact rel_mem ({mi}) ⟨{', '.join(vs)}, L, T, s, L', T', rfl, hy, h1, h2, hF⟩\n\n"
            )

    # Fuel: between two cut points a segment enters each block at most once.
    def fuel(kind_, text):
        if kind_ == "mir":
            fs = cp.parse_mir(text)
            return (
                sum(len(f["blocks"]) for f in fs)
                + sum(len(cp.mir_calls(f)) for f in fs)
                + 2
            )
        return len(cp.parse_evm(text)) + 1

    fuel_a, fuel_b = fuel(ka, ta), fuel(kb, tb)
    seg_rel = f"SegRel Agree (fun y y' => ∃ Q ∈ {p}_rels call, Q y y')"
    holes = {"mm": "_ _", "me": "_ _ _", "ee": "_ _ _ _"}[kind]
    segs = []
    for n, r in enumerate(allrels):
        name = f"{p}_seg{n}"
        segs.append(name)
        vs = r["src_vars"] + r["tgt_vars"]
        hyps = " ".join(f"(h{i} : {c})" for i, c in enumerate(r["conds"]))
        args = " ".join(x for x in [cp.binder_text(vs), hyps] if x)
        head = (
            f"set_option maxHeartbeats {heartbeats} in\n"
            + (f"set_option maxRecDepth {recdepth} in\n" if recdepth else "")
            + f"theorem {name} (o : Oracle) (call : Call) {args} :\n"
            f"    ∃ k, {seg_rel}\n"
            f"      (seg ({cp.step_term(ka, A)}) {cut_a} k {r['src']})\n"
            f"      (seg ({cp.step_term(kb, B)}) {cut_b} {fuel_b} {r['tgt']}) := by\n"
            f"  refine ⟨{fuel_a}, ?_⟩\n"
        )
        if r["kind"] == "init":
            out.append(
                head
                + f"  rec_seg [{unf}] [{', '.join(mk_outer[s] for s in outer)}]\n\n"
            )
            continue
        hF = f"h{len(r['conds']) - 1}"
        lines = [head]
        calls = []
        for m, s in enumerate(sites):
            lines.append(f"  have c{m} := @{mk_call[s]} call {holes} {hF}\n")
            calls.append(f"c{m}")
        lines.append(f"  cases {hF} with\n")
        for s in outer:
            if kind == "mm":
                pat = "c c' " + " ".join(f"e{i}" for i in range(len(live(s))))
            elif kind == "me":
                pat = "c junk hj"
            else:
                pat = " ".join(
                    [f"q{i}" for i in word_vars(va.below[s], vb.below[s])]
                    + ["T0", "T0'", "hj", "hj'"]
                )
            lines.append(
                f"  | {names[s]} {pat.strip()} =>\n    rec_seg [{unf}] [{', '.join(calls)}]\n"
            )
        for s in sites:
            if kind == "mm":
                pat = (
                    "c c' rs0 rs0' "
                    + "".join(f"e{i} " for i in range(len(live(s))))
                    + "t"
                )
            elif kind == "me":
                pat = "c rs0 L0 T0 hb t"
            else:
                pat = " ".join(
                    [f"q{i}" for i in word_vars(va.below[s], vb.below[s])]
                    + ["L0", "L0'", "T0", "T0'", "hb", "hb'", "t"]
                )
            lines.append(
                f"  | {names[s]} {pat} =>\n"
                f"    have b0 := @{mk_resume[s]} call {holes} t\n"
                f"    rec_seg [{unf}] [{', '.join(calls + ['b0'])}]\n"
            )
        out.append("".join(lines) + "\n")

    def case_lines():
        lines = []
        for n, r in enumerate(allrels):
            vs = [x for x, _ in r["src_vars"] + r["tgt_vars"]]
            hs = [f"h{i}" for i in range(len(r["conds"]))]
            pat = "⟨" + ", ".join(vs + ["rfl", "rfl"] + hs) + "⟩"
            lines.append(
                f"  · obtain {pat} := holds\n    exact {segs[n]} o call {' '.join(vs + hs)}".rstrip()
            )
        return lines

    out.append(
        f"theorem {p}_segments (o : Oracle) (call : Call) :\n"
        f"    ∀ P ∈ {p}_rels call, ∀ x x', P x x' → ∃ k, {seg_rel}\n"
        f"      (seg ({cp.step_term(ka, A)}) {cut_a} k x)\n"
        f"      (seg ({cp.step_term(kb, B)}) {cut_b} {fuel_b} x') := by\n"
        f"  intro P mem x x' holds\n"
        f"  simp only [{p}_rels, List.mem_cons, List.not_mem_nil, or_false] at mem\n"
        f"  rcases mem with {' | '.join(['rfl'] * len(allrels))}\n"
        + "\n".join(case_lines())
        + "\n\n"
    )
    out.append(
        f"/-- {A} and {B} simulated through their cut points at the recursive function's entry and\n"
        f"resume points, on every call: one relation for every depth of recursion. -/\n"
        f"def {p}_sim (o : Oracle) :\n"
        f"    CutPoint.LayerSim {cp.layer_term(ka)} {cp.layer_term(kb)} o {A} {B} anyCall where\n"
        f"  σ := {ca}\n  τ := {cb}\n"
        f"  step call := {cp.step_term(ka, A)}\n"
        f"  step' call := {cp.step_term(kb, B)}\n"
        f"  init call := {cp.init_term(ka, A)}\n"
        f"  init' call := {cp.init_term(kb, B)}\n"
        f"  runs _ _ := rfl\n  runs' _ _ := rfl\n"
        f"  cut := {cut_a}\n  cut' := {cut_b}\n"
        f"  Rel call := fun x x' => ∃ P ∈ {p}_rels call, P x x'\n"
        f"  start _ _ := ⟨_, {cp.mem_proof(0)}, rfl, rfl⟩\n"
        f"  sim call _ := CutPoint.Sim.of_each fun P m x x' hP =>\n"
        f"    match {p}_segments o call P m x x' hP with | ⟨k, s⟩ => ⟨k, {fuel_b}, s⟩\n\n"
        f"/-- Every segment of {B} takes at most {fuel_b} steps. -/\n"
        f"theorem {p}_bounded (o : Oracle) (call : Call) :\n"
        f"    CutPoint.BoundedSim ({cp.step_term(ka, A)}) {cut_a}\n"
        f"      ({cp.step_term(kb, B)}) {cut_b} Agree\n"
        f"      (fun x x' => ∃ P ∈ {p}_rels call, P x x') {fuel_b} :=\n"
        f"  fun x x' ⟨P, m, h⟩ => {p}_segments o call P m x x' h\n\n"
    )
    return "".join(out)
