"""Generate the fixed-fuel resource proof of a contract's one runtime loop.

From this run's own artifacts (the stage export's final EVM IR and runtime bytecode, the loop
numbers of tools/certify/fixed_fuel.py: header, B, X, E and the gas parts, and the loop checker's
certificate from tools/certify/asm_cert.py), write a Lean module that proves, on the run's runtime
bytes and with no premise left open:

* the loop record (condition: the counter word; measure: the iterations it has left) and its
  `Bridge` (At, iter, done) over the final EVM IR, every path of an iteration and of the exit a
  block fact this module states and proves by `sym_exec`;
* `Loops` from the program's entry for every call of `Valid K`;
* an inhabited `Valid K`;
* `checkL`, the loop checker, on the exact runtime bytes, and the verdict
  `verdictOf (some K) perIter exit = .bounded (K * perIter + exit)` for every `K ≤ Kmax`;
* totality of the final EVM IR on `Valid K`, and `Shapes.Resources` of its semantics on the bytes
  through `Verify.Gas.FixedFuel.resources_of_verdict` (stage: the final EVM IR to itself);
* with a lifecycle run's composed theorem, the joint theorem: that theorem, the run's bytes equal to
  the bytes it names, and all of the above at every `K` it admits.

`Valid K`: a call of the loop's function, its arguments ABI-encoded uint256 words, no value, any
storage, and the loop's limit argument at most `K`.

The supported loop shape; anything else is refused by name (`Unsupported`), never a weaker proof:

* the final EVM IR is inside the loop checker's fragment and has exactly one loop, with one header
  block (fixed_fuel.cfg_loop), entered with stacks of one depth;
* the header's first branch tests one header stack word `w` directly (nonzero: iterate), and every
  iteration path returns to the header with `w` replaced by `w + 1`; the measure is
  `(2^256 - w) % 2^256`;
* on every entry path the counter word enters the header as `0 - a` for one calldata argument
  `a`, the loop's limit, so the entry measure is `a ≤ K`;
* no block on a path that continues (entry to header, header to header) writes memory or storage;
  a path that halts may, within the exit's X blocks;
* the gas of the exit alone is below `2^64`.

What it does not give: the source join. `resources_of_verdict` over the source semantics needs a
stage from the source to the final EVM IR, and the generic one for a loop body needs `BodyRuns`
of it, which no generic Lean lemma derives yet (generic.py's LOOP_SOURCE_GAP names the lemma).
"""

import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import asm_cert
import fixed_fuel

Unsupported = fixed_fuel.Unsupported

WORD = 2**256

# --- symbolic words ----------------------------------------------------------------------------


def lit(n):
    return ("lit", n % WORD)


BIN = {
    "add",
    "sub",
    "mul",
    "div",
    "lt",
    "gt",
    "eq",
    "and",
    "or",
    "xor",
    "shl",
    "shr",
    "byte",
}
UN = {"iszero", "not"}


def render(e, st="s"):
    """The Lean term of a word."""
    k = e[0]
    if k == "lit":
        n = e[1]
        return f"0x{n:x}" if n >= 2**16 else str(n)
    if k == "var":
        return e[1]
    if k == "op":
        args = " ".join(
            f"({render(a, st)})" if a[0] not in ("lit", "var") else render(a, st)
            for a in e[2:]
        )
        return f"EvmRules.Evm.{e[1]} {args}"
    if k == "cdw":
        a = e[1]
        return f"calldataWord call {render(a, st) if a[0] in ('lit', 'var') else '(' + render(a, st) + ')'}"
    if k == "sload":
        a = e[2]
        return f"{e[1]}.storage {render(a, st) if a[0] in ('lit', 'var') else '(' + render(a, st) + ')'}"
    if k == "cv":
        return "BitVec.ofNat 256 call.callvalue"
    if k == "cds":
        return "BitVec.ofNat 256 call.calldata.size"
    raise ValueError(e)


def evaluate(e, facts):
    """The value of a word under the call's facts (callvalue, calldata size, selector), or None."""
    k = e[0]
    if k == "lit":
        return e[1]
    if k == "cv":
        return facts.get("cv")
    if k == "cds":
        return facts.get("cds")
    if k == "op" and e[1] == "shr" and e[2] == lit(224) and e[3] == ("cdw", lit(0)):
        return facts.get("sel")
    if k == "op":
        vs = [evaluate(a, facts) for a in e[2:]]
        if None in vs:
            return None
        a = vs[0]
        b = vs[1] if len(vs) > 1 else None
        return {
            "add": lambda: (a + b) % WORD,
            "sub": lambda: (a - b) % WORD,
            "mul": lambda: (a * b) % WORD,
            "div": lambda: a // b if b else 0,
            "lt": lambda: int(a < b),
            "gt": lambda: int(a > b),
            "eq": lambda: int(a == b),
            "and": lambda: a & b,
            "or": lambda: a | b,
            "xor": lambda: a ^ b,
            "shl": lambda: (b << a) % WORD if a < 256 else 0,
            "shr": lambda: b >> a if a < 256 else 0,
            "iszero": lambda: int(a == 0),
            "not": lambda: WORD - 1 - a,
        }.get(e[1], lambda: None)()
    return None


# --- one path at a time --------------------------------------------------------------------------


def run_path(instrs, stack, state, facts, choices):
    """The block on one path: CHOICES says, for each jumpi in order, whether it is taken (None for a
    decided one). Returns (jumpis, end) where jumpis are (word, taken, decided)."""
    st = list(stack)
    js = []
    ci = 0
    i = 0
    while i < len(instrs):
        ins = instrs[i]
        op = ins[0]
        i += 1
        if op == "push":
            st.insert(0, lit(ins[1]))
        elif op == "dup":
            st.insert(0, st[ins[1]])
        elif op == "swap":
            j = ins[1] + 1
            st[0], st[j] = st[j], st[0]
        elif op == "stack":
            name = ins[1]
            if name == "pop":
                st.pop(0)
            elif name == "calldatasize":
                st.insert(0, ("cds",))
            elif name in BIN:
                a, b = st.pop(0), st.pop(0)
                st.insert(0, ("op", name, a, b))
            elif name in UN:
                st.insert(0, ("op", name, st.pop(0)))
            else:
                raise Unsupported(f"stack operation {name}")
        elif op == "effect" and ins[1] == "callvalue":
            st.insert(0, ("cv",))
        elif op == "effect" and ins[1] == "calldataload":
            st.insert(0, ("cdw", st.pop(0)))
        elif op == "effect" and ins[1] == "sload":
            st.insert(0, ("sload", state, st.pop(0)))
        elif op == "effect" and ins[1] == "sstore" or op == "store":
            st.pop(0), st.pop(0)
        elif op == "jump":
            return js, ("goto", ins[1], st)
        elif op == "jumpi":
            c = st.pop(0)
            v = evaluate(c, facts)
            if v is not None:
                taken = v != 0
                js.append((c, taken, True))
            else:
                if ci >= len(choices):
                    return js + [(c, None, False)], None
                taken = choices[ci]
                ci += 1
                js.append((c, taken, False))
            if taken:
                return js, ("goto", ins[1], st)
        elif op in ("stop", "ret", "revert"):
            return js, ("halt",)
        else:
            raise Unsupported(f"instruction {ins}")
    raise Unsupported("a block falls through")


def outcomes(instrs, stack, state, facts):
    """Every path through a block: (choices, jumpis, end)."""
    out, todo = [], [[]]
    while todo:
        ch = todo.pop(0)
        js, end = run_path(instrs, stack, state, facts, ch)
        if end is None:
            todo += [ch + [True], ch + [False]]
        else:
            out.append((ch, js, end))
    return out


def writes(instrs):
    return any(
        i[0] == "store" or (i[0] == "effect" and i[1] == "sstore") for i in instrs
    )


def abstract(stack):
    """The stack with every word that is not a literal replaced by a variable, and the map back."""
    out, args = [], []
    for e in stack:
        if e[0] == "lit":
            out.append(e)
        else:
            out.append(("var", f"v{len(args)}"))
            args.append(e)
    return out, args


def base(c):
    """The proposition a jumpi's word is cased on, and whether the jump is taken when it holds."""
    if c[0] == "op" and c[1] == "lt":
        return f"{render(c[2])} < {render(c[3])}", True
    if c[0] == "op" and c[1] == "gt":
        return f"{render(c[3])} < {render(c[2])}", True
    if c[0] == "op" and c[1] == "eq":
        return f"{render(c[2])} = {render(c[3])}", True
    if c[0] == "op" and c[1] == "iszero":
        return f"{render(c[2])} = 0", True
    return f"{render(c)} = 0", False


def hyp(c, taken):
    q, pos = base(c)
    return q if taken == pos else f"¬{q}"


def paren(s):
    return s if re.fullmatch(r"[\w.']+", s) else f"({s})"


class Module:
    def __init__(self, prog, X):
        self.prog, self.X = prog, X
        self.lemmas, self.cache = [], {}

    def block_lemma(self, b, stack, choices, end_kind, facts):
        """The fact for block B entered with STACK on the path CHOICES: its name and the words it
        is applied to (the abstracted stack's arguments)."""
        abs_st, args = abstract(stack)
        js, end = run_path(
            BLOCKS[b], abs_st, "s", {}, choices_for(BLOCKS[b], stack, facts, choices)
        )
        key = (b, repr(abs_st), repr([(repr(c), t) for c, t, _ in js]), end_kind)
        if key not in self.cache:
            name = f"b{b}_{len(self.lemmas)}"
            vs = " ".join(f"v{i}" for i in range(len(args)))
            binder = f" ({vs} : EvmRules.Word)" if args else ""
            hs = "".join(f" (h{i} : {hyp(c, t)})" for i, (c, t, _) in enumerate(js))
            stl = "[" + ", ".join(render(e) for e in abs_st) + "]"
            if end_kind == "goto":
                nst = "[" + ", ".join(render(e) for e in end[2]) + "]"
                stmt = f"∃ s', EVM.Program.block o call {self.prog} ({b}, {stl}, s) =\n      some (.inr ({end[1]}, {nst}, s'))"
                proof = "⟨_, by sym_exec⟩"
            else:
                stmt = f"halts (EVM.Program.block o call {self.prog} ({b}, {stl}, s)) = true"
                proof = "by\n  sym_exec"
            self.lemmas.append(
                "set_option maxHeartbeats 4000000 in\n"
                f"theorem {name} (o : Oracle) (call : Call){binder} (s : State){hs} :\n    {stmt} := {proof}\n"
            )
            self.cache[key] = (name, len(js))
        return self.cache[key], args


def choices_for(instrs, stack, facts, choices):
    """The abstracted block's choices: the concrete path's decision for every jumpi, decided or not."""
    js, _ = run_path(instrs, stack, "s", facts, choices)
    return [t for _, t, _ in js]


BLOCKS = None


# --- the proof trees ---------------------------------------------------------------------------


class Tree:
    """Emits the tactic proof of one obligation (entry, iteration) path by path."""

    def __init__(self, mod, header, facts, fact_names, mode, leaf):
        self.mod, self.header, self.facts, self.fact_names = (
            mod,
            header,
            facts,
            fact_names,
        )
        self.mode, self.leaf = mode, leaf
        self.n = 0

    def fresh(self, p):
        self.n += 1
        return f"{p}{self.n}"

    def decided(self):
        fs = ", ".join(self.fact_names)
        return f"(by simp only [{fs}]; decide)"

    def walk(self, b, stack, state, reach, forced, ind):
        outs = outcomes(BLOCKS[b], stack, state, self.facts)
        return self.split(b, stack, state, reach, forced, outs, [], {}, ind)

    def split(self, b, stack, state, reach, forced, outs, prefix, names, ind):
        pad = "  " * ind
        here = [o for o in outs if o[0][: len(prefix)] == prefix]
        done = [o for o in here if len(o[0]) == len(prefix)]
        if done:
            return self.leaf_lines(b, stack, state, reach, done[0], names, ind)
        # the next free decision's word
        o = here[0]
        free = [c for c, _, d in o[1] if not d]
        c = free[len(prefix)]
        q, pos = base(c)
        if forced is not None and c == forced[0]:
            names = {**names, len(prefix): forced[1]}
            return self.split(
                b, stack, state, reach, None, outs, prefix + [forced[2]], names, ind
            )
        h = self.fresh("hc")
        lines = [f"{pad}by_cases {h} : {q}"]
        for branch in (pos, not pos):
            sub = self.split(
                b,
                stack,
                state,
                reach,
                forced,
                outs,
                prefix + [branch],
                {**names, len(prefix): h},
                ind + 1,
            )
            lines.append(f"{pad}· " + sub[0].lstrip())
            lines += sub[1:]
        return lines

    def leaf_lines(self, b, stack, state, reach, out, names, ind):
        pad = "  " * ind
        choices, js, end = out
        kind = "goto" if end[0] == "goto" else "end"
        (name, _), args = self.mod.block_lemma(b, stack, choices, kind, self.facts)
        proofs, fi = [], 0
        for c, t, d in js:
            if d:
                proofs.append(self.decided())
            else:
                proofs.append(names[fi])
                fi += 1
        app = " ".join(
            [name, "o", "call"] + [paren(render(a)) for a in args] + [state] + proofs
        )
        if kind == "end":
            return self.leaf(("end", b, stack, state, reach, app), ind)
        s2, hr = self.fresh("s"), self.fresh("hr")
        lines = [f"{pad}obtain ⟨{s2}, {hr}⟩ := {app}"]
        nb, nst = end[1], end[2]
        # the next block's words name the state they were read from
        if nb == self.header:
            lines += self.leaf(("header", nb, nst, s2, reach + [hr]), ind)
        else:
            lines += self.walk(nb, nst, s2, reach + [hr], None, ind)
        return lines


def reach_term(rs):
    t = "(Reach.refl _)"
    for r in reversed(rs):
        t = f"(Reach.cont {r} {t})"
    return t


def words(st):
    return ", ".join(
        f"({render(e)}).toNat"
        if e[0] not in ("lit",)
        else f"({render(e)} : EvmRules.Word).toNat"
        for e in st
    )


# --- the module --------------------------------------------------------------------------------


def generate(
    *,
    ns,
    contract,
    last,
    gen_dir,
    ob,
    code_term,
    code_import,
    sel,
    nargs,
    limit_arg=None,
    K=None,
    joint=None,
    source=None,
):
    """The Lean module's text. OB is fixed_fuel.obligation's result for this export, with M the
    iteration bound (K for a word limit), and code_term the run's fresh runtime bytes."""
    global BLOCKS
    text = (gen_dir / f"{last[0].upper() + last[1:]}.lean").read_text()
    outside = asm_cert.outside(text)
    if outside:
        raise Unsupported(
            f"the final EVM IR is outside the loop checker's fragment: {outside}"
        )
    BLOCKS = asm_cert.parse_program(text)
    prog = f"SolarTV.Export.{contract}.{last}"
    H, B, X, E, _M = ob["header"], ob["B"], ob["X"], ob["E"], ob["M"]
    if X < 1:
        raise Unsupported("the exit takes no block")
    mod = Module(prog, X)
    facts = {"cv": 0, "cds": 4 + 32 * nargs, "sel": sel}
    fact_names = ["hcv", "hsz", "hsel"]

    # The header: its depth from every entry path, and the word it tests.
    arrivals = []

    def scan(b, st, depth):
        if depth > E:
            raise Unsupported(f"an entry path is longer than E = {E} blocks")
        for ch, js, end in outcomes(BLOCKS[b], st, f"s{depth}", facts):
            if end[0] == "halt":
                continue
            if writes(BLOCKS[b]):
                raise Unsupported(
                    f"entry block {b} writes memory or storage on a path that continues"
                )
            if end[1] == H:
                arrivals.append(end[2])
            else:
                scan(end[1], end[2], depth + 1)

    scan(0, [], 0)
    if not arrivals or len({len(a) for a in arrivals}) != 1:
        raise Unsupported("the header is entered with stacks of different depths")
    n = len(arrivals[0])
    W = [("var", f"w{i}") for i in range(n)]
    houts = outcomes(BLOCKS[H], W, "s", {})
    firsts = {repr(js[0][0]) for _, js, _ in houts if js and not js[0][2]}
    c0 = houts[0][1][0][0] if houts[0][1] else None
    if len(firsts) != 1 or c0 is None or c0[0] != "var":
        raise Unsupported("the header does not test one header word directly")
    k = int(c0[1][1:])
    if writes(BLOCKS[H]) and any(js[0][1] for _, js, _ in houts if js):
        pass  # the exit may write; the iteration paths are checked block by block below
    # Entry: the counter word must be 0 - a for one call argument a (the loop's limit), the same
    # argument on every entry path.
    found = set()
    for a in arrivals:
        e = a[k]
        m = None
        if (
            e[0] == "op"
            and e[1] == "sub"
            and e[2] == lit(0)
            and e[3][0] == "cdw"
            and e[3][1][0] == "lit"
        ):
            off = e[3][1][1]
            if off >= 4 and (off - 4) % 32 == 0 and (off - 4) // 32 < nargs:
                m = (off - 4) // 32
        if m is None:
            raise Unsupported(
                f"the loop's counter word enters the header as {render(e)}, not 0 - a call argument"
            )
        found.add(m)
    if len(found) != 1 or (limit_arg is not None and found != {limit_arg}):
        raise Unsupported(
            f"the loop's limit is not one call argument on every entry path: {sorted(found)}"
        )
    limit_arg = found.pop()
    bound_ok = lambda w: (
        w == ("op", "add", lit(1), ("var", f"w{k}"))
        or w == ("op", "add", ("var", f"w{k}"), lit(1))
    )

    hs = " ".join(f"w{i}" for i in range(n))
    hsc = ", ".join(f"w{i}" for i in range(n))
    wl = ", ".join(f"w{i}.toNat" for i in range(n))
    at = f"∃ {hs} s, x = ({H}, [{', '.join(f'w{i}' for i in range(n))}], s) ∧ σ = val [1, {wl}]"

    # Iteration tree.
    def iter_leaf(leaf, ind):
        pad = "  " * ind
        if leaf[0] == "header":
            _, _, nst, s2, reach = leaf
            if len(nst) != n or not bound_ok(nst[k]):
                raise Unsupported(
                    "an iteration returns to the header without stepping the counter by one"
                )
            step = "measA_step" if nst[k][2] == lit(1) else "measA_step'"
            ws = ", ".join(paren(render(e)) for e in nst)
            return [
                f"{pad}exact ⟨val [1, {words(nst)}], ({H}, [{', '.join(render(e) for e in nst)}], {s2}),",
                f"{pad}  ⟨trivial, hc, trivial, by simp only [m_head, List.getD_cons_succ, List.getD_cons_zero]; exact {step} _ hk⟩,",
                f"{pad}  Or.inl ⟨{ws}, {s2}, rfl, rfl⟩, ⟨{len(reach)}, by omega, {reach_term(reach)}⟩⟩",
            ]
        _, b, st, s0, reach, app = leaf
        if len(reach) > B:
            raise Unsupported("an iteration ends after more than B blocks")
        return [
            f"{pad}exact ⟨val [0], ({b}, [{', '.join(render(e) for e in st)}], {s0}),",
            f"{pad}  ⟨trivial, hc, trivial, by simp only [m_zero, m_head, List.getD_cons_succ, List.getD_cons_zero]; exact meas_pos _ hk⟩,",
            f"{pad}  Or.inr ⟨rfl, ends_of_halts ({app}) {X - 1}⟩, ⟨{len(reach)}, by omega, {reach_term(reach)}⟩⟩",
        ]

    it = Tree(mod, H, {}, [], "iter", iter_leaf)
    iter_lines = it.walk(H, W, "s", [], (c0, "hk", True), 3)

    # Exit tree: from the header with the counter word zero, the run ends within X blocks.
    def done_leaf(leaf, ind):
        pad = "  " * ind
        if leaf[0] == "header":
            raise Unsupported("the loop's exit returns to its header")
        _, _b, _st, _s0, reach, app = leaf
        if len(reach) + 1 > X:
            raise Unsupported(f"the exit takes more than X = {X} blocks")
        return [
            f"{pad}obtain ⟨r, hr⟩ := ends_of_halts ({app}) 0",
            f"{pad}exact ⟨r, iterate_mono_le (by omega) (iterate_of_reach {reach_term(reach)} hr)⟩",
        ]

    dn = Tree(mod, H, {}, [], "done", done_leaf)
    done_lines = dn.walk(H, W, "s", [], (c0, "hk", False), 3)

    # Entry tree.
    def entry_leaf(leaf, ind):
        pad = "  " * ind
        if leaf[0] == "header":
            _, _, nst, s2, reach = leaf
            ws = ", ".join(paren(render(e)) for e in nst)
            e = nst[k]
            meas = (
                "by simp only [m_head, List.getD_cons_succ, List.getD_cons_zero, negA, ha]; exact hK"
                if e[0] == "op"
                else "by simp only [m_head, List.getD_cons_succ, List.getD_cons_zero]; decide"
            )
            return [
                f"{pad}exact ⟨val [1, {words(nst)}], ({H}, [{', '.join(render(e) for e in nst)}], {s2}),",
                f"{pad}  ⟨{len(reach)}, by omega, {reach_term(reach)}⟩, Or.inl ⟨{ws}, {s2}, rfl, rfl⟩, trivial,",
                f"{pad}  {meas}⟩",
            ]
        _, b, st, s0, reach, app = leaf
        if len(reach) > E:
            raise Unsupported("an entry path ends after more than E blocks")
        return [
            f"{pad}exact ⟨val [0], ({b}, [{', '.join(render(e) for e in st)}], {s0}),",
            f"{pad}  ⟨{len(reach)}, by omega, {reach_term(reach)}⟩,",
            f"{pad}  Or.inr ⟨rfl, ends_of_halts ({app}) {X - 1}⟩, trivial, by rw [m_zero]; exact Nat.zero_le _⟩",
        ]

    en = Tree(mod, H, facts, fact_names, "entry", entry_leaf)
    entry_lines = en.walk(0, [], "(State.initial call)", [], None, 1)

    # The certificate of the loop checker on the exact runtime bytes.
    bt = (gen_dir / "Bytecode.lean").read_text()
    hx = re.search(r'def bytecodeHex : String :=\n  "([0-9a-f]+)"', bt)
    code = bytes.fromhex(hx.group(1))
    inferred = asm_cert.infer(text, code)
    cert, maxs = asm_cert.lean_cert(inferred)
    bmax = max(inferred["costs"]) if inferred["costs"] else 0
    if bmax != ob["bmax"]:
        raise Unsupported("the certificate's block cost differs from the obligation's")
    pi, ex = ob["per_iteration_gas"], ob["exit_gas"]
    Kmax = (2**64 - 1 - ex) // pi if ex < 2**64 else -1
    if Kmax < 0:
        raise Unsupported(
            "the exit alone needs 2^64 gas: out of gas, not a resource proof"
        )
    Kmax * pi + ex
    args = " ".join(f"a{i}" for i in range(nargs))
    arglist = ", ".join(f"a{i}" for i in range(nargs))
    lt = " ∧ ".join(f"a{i} < 2 ^ 256" for i in range(nargs))
    if limit_arg is None:
        raise Unsupported(
            "the loop's limit is not a call argument (literal limits are outside this generator)"
        )
    bound = f" ∧ a{limit_arg} ≤ K"
    A, L = (
        "SolidCore.Refinement.Solar.Assemble",
        "SolidCore.Refinement.Solar.AssembleLoop",
    )
    ha = ""
    if limit_arg is not None:
        ha = (
            f"  have ha : (calldataWord ({{ calldata := Abi.calldata {sel} [{arglist}], callvalue := 0, storage := σs }} : Call) {4 + 32 * limit_arg}).toNat = a{limit_arg} := by\n"
            f"    have := hf.args {limit_arg} (by simp) (by decide)\n"
            f"    simpa [WExpr.eval, Nat.mod_eq_of_lt h{limit_arg}] using this\n"
        )
    hcases = " ".join(f"h{i}" for i in range(nargs))
    meas_lemmas = """
/-- The iterations left from a counter word that the body steps by one up to zero. -/
def meas (w : Nat) : Nat := (2 ^ 256 - w) % 2 ^ 256

theorem measA_step (h : EvmRules.Word) (hh : ¬h = 0) :
    meas (EvmRules.Evm.add 1 h).toNat < meas h.toNat := by
  unfold meas EvmRules.Evm.add
  have := h.isLt
  have : h.toNat ≠ 0 := fun e => hh (BitVec.eq_of_toNat_eq (by simpa using e))
  rw [BitVec.toNat_add]
  have h1 : (1 : BitVec 256).toNat = 1 := rfl
  rw [h1]
  omega

theorem measA_step' (h : EvmRules.Word) (hh : ¬h = 0) :
    meas (EvmRules.Evm.add h 1).toNat < meas h.toNat := by
  have e : EvmRules.Evm.add h 1 = EvmRules.Evm.add 1 h := by
    unfold EvmRules.Evm.add; exact BitVec.add_comm _ _
  rw [e]; exact measA_step h hh

theorem meas_pos (h : EvmRules.Word) (hh : ¬h = 0) : 0 < meas h.toNat := by
  unfold meas
  have := h.isLt
  have : h.toNat ≠ 0 := fun e => hh (BitVec.eq_of_toNat_eq (by simpa using e))
  omega

theorem negA (x : EvmRules.Word) : meas (EvmRules.Evm.sub 0 x).toNat = x.toNat := by
  unfold meas EvmRules.Evm.sub
  have := x.isLt
  rw [BitVec.toNat_sub]
  have h0 : (0 : BitVec 256).toNat = 0 := rfl
  rw [h0]
  omega
"""
    srcpart = ""
    if joint:
        srcpart = f"""
/-- **The joint consumer**: at every iteration bound `K` the run's composed lifecycle theorem
admits, that theorem; this run's runtime bytes are the bytes it names; and on the same bytes the
final EVM IR's stage (to itself), totality, `Loops` from the entry, an inhabited `Valid K` and
`Shapes.Resources` at `K * {pi} + {ex}` gas. -/
theorem joint {{K : Nat}} (hK : K ≤ {joint["bound"]}) :
    type_of% ({joint["composed"]} hK) ∧
    code = SolidCore.Verify.Generic.runtimeCode {joint["runtime"]} ∧
    (∃ call, Valid K call) ∧
    Stage (evmirSem Shapes.oracle) (evmirSem Shapes.oracle) {prog} {prog} (Valid K) ∧
    (∀ call, Valid K call → ∃ obs, (evmirSem Shapes.oracle).run {prog} call = some obs) ∧
    (∀ o call, Valid K call →
      Loops (EVM.Program.block o call {prog}) (0, [], State.initial call) record {E} {B} {X} K) ∧
    SolidCore.Refinement.Shapes.Resources (evmirSem Shapes.oracle) {prog} code (Valid K)
      (fun _ => K * {pi} + {ex}) := by
  have hK' : K ≤ Kmax := Nat.le_trans hK (by decide)
  exact ⟨{joint["composed"]} hK, {joint["code_same"]}, inhabited K, Stage.refl _ _ _,
    total hK', fun o call hv => loops o K call hv, resources_evmir hK'⟩

/-- info: '{ns}.joint' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms joint
"""
    body = f"""import Verify.Gas.FixedFuel.Gas
import W1Proofs.Tactic
import SolarTV.Composition
import {code_import}
{("import " + joint["import"] + chr(10)) if joint else ""}{"".join("import " + m + chr(10) for m in (source or {}).get("imports", []))}
-- Generated by solar's tools/certify/loop_resources.py from this run's stage export
-- ({gen_dir.name}/{last}), tools/certify/fixed_fuel.py's obligation and tools/certify/asm_cert.py's
-- certificate; do not edit. Untrusted data: the kernel re-derives every claim.

/-!
# {contract}: the fixed-fuel resource bound of its runtime loop

Header block {H} (depth {n}, counter word {k}), B = {B}, X = {X}, E = {E}; for an iteration bound
`K ≤ Kmax = {Kmax}` the gas bound is `verdictOf (some K) {pi} {ex} = .bounded (K * {pi} + {ex})`. `Valid`: a call of selector {sel:#x} with {nargs}
ABI word argument(s), no value, any storage, and argument {limit_arg} (the loop limit) at most `K`.
-/

namespace {ns}

open SolarTV SolarTV.Machine SolarTV.Milestone.W1
open SolidCore.Refinement SolidCore.Refinement.Solar SolidCore.Refinement.Generic Verify.Gas.FixedFuel
open Verify.LoopSource.Interface

/-- The runtime bytes this run emitted. -/
abbrev code : ByteArray := {code_term}

/-- The loop checker's certificate (tools/certify/asm_cert.py). -/
def cert : {A}.Cert :=
  {cert}

/-- The highest stack each block is entered with. -/
def maxs : List Nat := {maxs}

set_option maxRecDepth 100000 in
theorem walk_ok : (SolarTV.Assembly.walk ({A}.bytesOf code) cert.offset 0
    ({A}.lower {prog} cert)).isSome = true := by
  have hw := {A}.walkS_drop ({A}.bytesOf code) cert.offset ({A}.lower {prog} cert) 0
    (Nat.zero_le _)
  rw [List.drop_zero] at hw
  rw [← hw]
  decide +kernel

set_option maxRecDepth 100000 in
theorem blocks_ok : {L}.blocksOkL {prog} cert maxs 0 {prog}.blocks = true := by
  decide +kernel

theorem labels_ok : {A}.labelsValid {prog} cert code = true :=
  {A}.labelsValid_of_walk walk_ok

theorem size_ok : code.size < 2 ^ 32 := by decide +kernel

/-- The checker of code with loops accepts the exact bytecode for the final EVM IR. -/
theorem check_ok : {L}.checkL {prog} code cert maxs = true := by
  simp only [{L}.checkL, walk_ok, blocks_ok, labels_ok, decide_eq_true size_ok, Bool.and_true,
    Bool.true_and]
  decide

/-- The largest iteration bound whose fixed gas is below `2 ^ 64`. -/
def Kmax : Nat := {Kmax}

theorem perIter_eq : perIterGas cert {B} = {pi} := by decide +kernel

theorem exit_eq : exitGas cert {E} {X} = {ex} := by decide +kernel

/-- **The resource verdict**: bounded for every iteration bound up to `Kmax`. -/
theorem verdict {{K : Nat}} (hK : K ≤ Kmax) :
    verdictOf (some K) (perIterGas cert {B}) (exitGas cert {E} {X}) = .bounded (K * {pi} + {ex}) := by
  rw [perIter_eq, exit_eq]
  have h : K * {pi} + {ex} < 2 ^ 64 := by unfold Kmax at hK; omega
  simp only [verdictOf, h, ite_true]

/-- The valid calls at iteration bound `K`. -/
def Valid (K : Nat) (call : Call) : Prop :=
  ∃ σ {args}, {lt}{bound} ∧ call = {{ calldata := Abi.calldata {sel} [{arglist}], callvalue := 0, storage := σ }}

/-- **Inhabited**: the call with every argument zero, from empty storage. -/
theorem inhabited (K : Nat) : ∃ call, Valid K call :=
  ⟨_, [], {" ".join("0" for _ in range(nargs))}, {", ".join(["by decide"] * nargs + ["Nat.zero_le K"])}, rfl⟩

/-- A valuation of header words. -/
def val (as : List Nat) : Val := ⟨as, fun _ => 0, fun _ _ => 0, 0⟩
{meas_lemmas}
/-- The loop's record: word 0 tells the header (1) from an end (0); the condition is the counter
word, the measure the iterations it has left. -/
def record : LoopRecord where
  H := {{ scopes := [], writes := [], logs := [] }}
  P _ := True
  m σ := if σ.args.getD 0 0 = 0 then 0 else meas (σ.args.getD {k + 1} 0)
  c := .sel (.arg 0) (.arg {k + 1}) (.lit 0)
  bb := .leaf (.normal {{ scopes := [], writes := [], logs := [] }})

@[reducible] def halts : Option ((Halt × State) ⊕ (Nat × EVM.Stack × State)) → Bool
  | some (.inl _) => true
  | _ => false

theorem ends_of_halts {{o : Oracle}} {{call : Call}} {{x : Nat × EVM.Stack × State}}
    (h : halts (EVM.Program.block o call {prog} x) = true) (k : Nat) :
    ∃ r, iterate (EVM.Program.block o call {prog}) (k + 1) x = some r := by
  cases hs : EVM.Program.block o call {prog} x with
  | none => simp [hs] at h
  | some v =>
    cases v with
    | inl r => exact ⟨r, by simp [iterate, hs]⟩
    | inr _ => simp [hs] at h

theorem m_head (ws : List Nat) : record.m (val (1 :: ws)) = meas (ws.getD {k} 0) := by
  simp [record, val]

theorem m_zero : record.m (val [0]) = 0 := by simp [record, val]

theorem c_head (ws : List Nat) : record.c.eval (val (1 :: ws)) = ws.getD {k} 0 % Generic.wordSize := by
  simp [record, val, WExpr.eval]

theorem c_zero : record.c.eval (val [0]) = 0 := by simp [record, val, WExpr.eval]

{chr(10).join(mod.lemmas)}
/-- **The bridge** of the loop over the final EVM IR. -/
def bridge (o : Oracle) (call : Call) :
    Bridge record (EVM.Program.block o call {prog}) {B} {X} where
  At σ x := ({at}) ∨ (σ = val [0] ∧ ∃ r, iterate (EVM.Program.block o call {prog}) {X} x = some r)
  iter := by
    intro σ x hx _ hc
    rcases hx with ⟨{hsc}, s, rfl, rfl⟩ | ⟨rfl, -⟩
    · have hk : ¬w{k} = 0 := by
        intro h0; apply hc; simp [c_head, h0]
{chr(10).join(iter_lines)}
    · exact absurd c_zero hc
  done := by
    intro σ x hx _ hc
    rcases hx with ⟨{hsc}, s, rfl, rfl⟩ | ⟨-, h⟩
    · have hk : w{k} = 0 := by
        have := w{k}.isLt
        simp only [c_head, List.getD_cons_succ, List.getD_cons_zero, Generic.wordSize, Nat.mod_eq_of_lt this] at hc
        exact BitVec.eq_of_toNat_eq (by simpa using hc)
{chr(10).join(done_lines)}
    · exact h

/-- **Loops**: from the entry, every valid call reaches the header within E blocks at a measure
at most M, or ends. -/
theorem loops (o : Oracle) (K : Nat) (call : Call) (hv : Valid K call) :
    Loops (EVM.Program.block o call {prog}) (0, [], State.initial call) record {E} {B} {X} K := by
  obtain ⟨σs, {args}, {hcases}{" , hK" if limit_arg is not None else ""}, rfl⟩ := hv
  have hf := callFacts {sel} (by decide) [{arglist}]
    (fun x hx => by simp only [List.mem_cons, List.not_mem_nil, or_false] at hx; rcases hx with {" | ".join(["rfl"] * nargs)}{" <;> " if nargs > 1 else "; "}assumption) σs
  have hcv : ({{ calldata := Abi.calldata {sel} [{arglist}], callvalue := 0, storage := σs }} : Call).callvalue = 0 := rfl
  have hsz : ({{ calldata := Abi.calldata {sel} [{arglist}], callvalue := 0, storage := σs }} : Call).calldata.size = {4 + 32 * nargs} := hf.size
  have hsel := hf.selector
{ha}  generalize ({{ calldata := Abi.calldata {sel} [{arglist}], callvalue := 0, storage := σs }} : Call) = call at hcv hsz hsel {"ha" if limit_arg is not None else ""} ⊢
  refine ⟨bridge o call, ?_⟩
{chr(10).join(entry_lines)}

/-- **Totality** of the final EVM IR on the valid calls, from `Loops`. -/
theorem total {{K : Nat}} (_hK : K ≤ Kmax) :
    ∀ call, Valid K call → ∃ obs, (evmirSem Shapes.oracle).run {prog} call = some obs := by
  intro call hv
  obtain ⟨r, hr⟩ := ends_of_loops (loops Shapes.oracle K call hv)
  have hr' : EVM.Program.run Shapes.oracle {prog} call (fixedFuel {E} {B} {X} K) = some r := hr
  exact ⟨_, by
    change (SolarTV.EVM.Program.layer.result Shapes.oracle {prog} call).map _ = _
    rw [SolarTV.EVM.Program.layer.result_of_ends ⟨fixedFuel {E} {B} {X} K, hr'⟩]
    rfl⟩

/-- **The final EVM IR's resources** on the exact bytes: `resources_of_verdict` with the stage of
the final EVM IR to itself and the loop's bound for totality. -/
theorem resources_evmir {{K : Nat}} (hK : K ≤ Kmax) :
    SolidCore.Refinement.Shapes.Resources (evmirSem Shapes.oracle) {prog} code (Valid K) (fun _ => K * {pi} + {ex}) :=
  resources_of_verdict (verdict hK) check_ok Shapes.oracle (fun _ => Shapes.oracle_ok.1)
    (fun _ => Shapes.oracle_ok.2)
    (total hK)
    (Stage.refl _ _ _) (fun call hv => loops Shapes.oracle K call hv)

/-- info: '{ns}.resources_evmir' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms resources_evmir
{srcpart}
end {ns}
"""
    return body


def obligation(gen_dir, plain):
    """fixed_fuel's numbers for the export's one loop, with no iteration bound: the bound is the
    valid calls' (K), so the verdict is proved for every K up to Kmax."""
    final = fixed_fuel.last_evm(gen_dir)
    text = final.read_text()
    outside = asm_cert.outside(text)
    if outside:
        raise Unsupported(
            f"the final EVM IR is outside the loop checker's fragment: {outside}"
        )
    blocks = asm_cert.parse_program(text)
    f = fixed_fuel.fuel(blocks, 1)
    inferred = asm_cert.infer(text, bytes.fromhex(plain))
    bmax = max(inferred["costs"]) if inferred["costs"] else 0
    words = inferred["top"] // 32
    mem = 3 * words + words * words // 512
    return {
        **f,
        "M": "K",
        "final_evm_ir": final.stem,
        "bmax": bmax,
        "memory": mem,
        "per_iteration_gas": fixed_fuel.ITEM * f["B"] * bmax,
        "exit_gas": fixed_fuel.ITEM * (f["E"] + f["X"]) * bmax + mem,
    }


def for_run(
    solar, src, contract, ast, functions, tmp, *, ns, code_term, code_import, joint=None
):
    """The module for this run's source: the stage export of the run's own compiler, the loop's
    function from the source (fixed_fuel.source_loops) and its selector and arity from the run's
    ABI. Returns (text, facts)."""
    import generic

    loops = fixed_fuel.source_loops(ast, contract)
    if len(loops) != 1:
        raise Unsupported(
            f"the source has {len(loops)} loops; this generator takes exactly one"
        )
    fname = loops[0].get("function") or loops[0].get("fn") or loops[0].get("name")
    fns = [f for f in functions if f["signature"].split("(")[0] == fname]
    if len(fns) != 1:
        raise Unsupported(
            f"the loop's function {fname} is not one external function of the ABI"
        )
    fn = fns[0]
    if any(p != "uint256" for p in fn["params"]):
        raise Unsupported(f"{fn['signature']}: only uint256 arguments are supported")
    plain, err = generic.export_stages(solar, src, contract, tmp)
    if plain is None:
        raise Unsupported(
            "the stage exporter rejects the contract: " + (err or "").strip()[:300]
        )
    gen = pathlib.Path(tmp) / "gen" / contract
    ob = obligation(gen, plain)
    last = ob["final_evm_ir"]
    last = last[0].lower() + last[1:]
    text = generate(
        ns=ns,
        contract=contract,
        last=last,
        gen_dir=gen,
        ob=ob,
        code_term=code_term,
        code_import=code_import,
        sel=int(fn["selector"], 16),
        nargs=len(fn["params"]),
        joint=joint,
    )
    return text, {
        **ob,
        "function": fn["signature"],
        "selector": fn["selector"],
        "runtime": plain,
    }
