"""Generate the fixed-fuel resource proof of a contract's one runtime loop.

From this run's own artifacts (the stage export's final EVM IR and runtime bytecode, the loop
numbers of tools/certify/fixed_fuel.py: header, B, X, E and the gas parts, and the loop checker's
certificate from tools/certify/asm_cert.py), write a Lean module that proves, on the run's runtime
bytes and with no premise left open, the fixed-fuel premises of the erasure join's
`GeneratedTarget` (solidity-lean's Verify/LoopSource/Equiv/EraseLoopCounterJoin.lean):

* `loops`: `Loops` from the program's entry for every call of `Valid`, by the base's
  `Verify.Gas.FixedFuel.Measured.loops_of` (its loop record: a flag and the iterations left), every
  path of the entry, of an iteration and of the exit a chain of block facts this module states and
  proves by `sym_exec`;
* `check_ok`: `checkL`, the loop checker, on the exact runtime bytes;
* `fit`: `gasBoundL cert (fixedFuel E B X M) < 2 ^ 64`;
* `premises`: the three in the form `GeneratedTarget` takes them, at `Shapes.oracle`;
* the verdict `verdictOf (some M) perIter exit = .bounded (M * perIter + exit)`, an inhabited
  `Valid`, totality of the final EVM IR on `Valid`, and `Shapes.Resources` of its semantics on the
  bytes through `Verify.Gas.FixedFuel.resources_of_verdict` (stage: the final EVM IR to itself);
* with a lifecycle run's composed theorem, the joint theorem: that theorem, the run's bytes equal to
  the bytes it names, and all of the above at every `K` it admits.

`M` is the loop's iteration bound. A limit the call chooses (a calldata argument, or a storage word
of the call's initial state) keeps the bounded form: `Valid K` bounds that limit by `K`, `M = K`,
and every theorem holds for every `K ≤ Kmax`, the largest bound whose gas fits in 64 bits. The
universal form (any limit) is refuted, since a call can ask for `2^256 - 1` iterations. A literal
limit gives a constant `M` and no `K`.

The supported layouts of a counted loop in the final EVM IR (one loop, one header block, entered
with stacks of one depth, inside the loop checker's fragment) are the ones the backend gives the
loops `eraseLoopStmt` erases, each a guard `G` on header stacks and a measure `μ` of them:

* **down**: the header's first branch tests one header word `w` directly (nonzero: iterate); every
  iteration returns with `w` replaced by `w + 1`; `μ = (2^256 - w) % 2^256`, `G` none;
* **up**: the header's first branch is `w_k < w_j`; every iteration returns with `w_k + 1` and `w_j`
  unchanged; `μ = w_j - w_k` when `w_k < w_j`, else `0`, `G` none;
* **rotated**: the test ends the body block, so the header is the body: `G` is `w ≠ 0`, every
  iteration returns with `w + 1` after testing it nonzero, and the entry tests `w` nonzero; `μ`
  as for down.

On every entry path the measure at the header is bounded by one limit: for down and rotated the
counter enters as `0 - L` (or a literal), for up the compared word is `L` (or both are literals),
with `L` a calldata argument, a storage word of the initial state, or a literal. Blocks may write
memory and storage on any path: a storage read after a write in the same block is placed when both
slots are literals and refused otherwise. Anything else is refused by name (`Unsupported`), never a
weaker proof, and so is an exit alone that needs `2^64` gas.

What it does not give: the source join. `resources_of_verdict` over the source semantics needs a
stage from the source to the final EVM IR; the erasure join (`eraseLoop_certified_generated`) takes
this module's `premises` for its fixed-fuel part.
"""

import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import asm_cert
import export_ns
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
    decided one). Returns (jumpis, end) where jumpis are (word, taken, decided). A storage read
    sees the block's own earlier writes: a literal slot it wrote reads the word written, another
    literal slot reads the entry state; a read the writes could alias is refused. A write at a
    computed slot may overwrite any slot, so it forgets every literal slot written before it, and
    every later read is refused unless a later write at a literal slot names that slot again."""
    st = list(stack)
    js = []
    ci = 0
    i = 0
    stored, computed = {}, False
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
            key = st.pop(0)
            if key[0] == "lit" and key[1] in stored:
                st.insert(0, stored[key[1]])
            elif computed or (key[0] != "lit" and stored):
                raise Unsupported(
                    "a storage read after a write in the same block, at a slot the write may alias"
                )
            else:
                st.insert(0, ("sload", state, key))
        elif op == "effect" and ins[1] == "sstore":
            key, value = st.pop(0), st.pop(0)
            if key[0] == "lit":
                stored[key[1]] = value
            else:
                stored.clear()
                computed = True
        elif op == "store":
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


def facts_of(e):
    """The names of the call's facts (callvalue, calldata size, selector) that evaluate reads."""
    k = e[0]
    if k == "cv":
        return {"hcv"}
    if k == "cds":
        return {"hsz"}
    if k == "op" and e[1] == "shr" and e[2] == lit(224) and e[3] == ("cdw", lit(0)):
        return {"hsel"}
    if k == "op":
        return set().union(*(facts_of(a) for a in e[2:]))
    return set()


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


def nonzero_by(c, taken, w):
    """Whether a decision proves the word W nonzero: a jump on W taken, or on W = 0 not taken."""
    return (c == w and taken is True) or (c == ("op", "iszero", w) and taken is False)


def paren(s):
    return s if re.fullmatch(r"[\w.']+", s) else f"({s})"


class Module:
    def __init__(self, prog):
        self.prog = prog
        self.lemmas, self.cache = [], {}

    def block_lemma(self, b, stack, choices, end_kind, facts):
        """The fact for block B entered with STACK on the path CHOICES, by name. Its words are
        implicit: a use names only its hypotheses, one per jumpi, and the goal's configuration
        fixes the rest."""
        abs_st, args = abstract(stack)
        js, end = run_path(
            BLOCKS[b], abs_st, "s", {}, choices_for(BLOCKS[b], stack, facts, choices)
        )
        key = (b, repr(abs_st), repr([(repr(c), t) for c, t, _ in js]), end_kind)
        if key not in self.cache:
            name = f"b{b}_{len(self.lemmas)}"
            vs = " ".join(f"v{i}" for i in range(len(args)))
            binder = f" {{{vs} : EvmRules.Word}}" if args else ""
            hs = "".join(f" (h{i} : {hyp(c, t)})" for i, (c, t, _) in enumerate(js))
            stl = "[" + ", ".join(render(e) for e in abs_st) + "]"
            st = reads_after_write(BLOCKS[b])
            if end_kind == "goto":
                nst = "[" + ", ".join(render(e) for e in end[2]) + "]"
                stmt = f"∃ s', EVM.Program.block o call {self.prog} ({b}, {stl}, s) =\n      some (.inr ({end[1]}, {nst}, s'))"
                proof = (
                    f"⟨_, by{STORE_READS.replace(chr(10), chr(10) + '  ')}⟩"
                    if st
                    else "⟨_, by sym_exec⟩"
                )
            else:
                stmt = f"halts (EVM.Program.block o call {self.prog} ({b}, {stl}, s)) = true"
                proof = f"by{STORE_READS}" if st else "by\n  sym_exec"
            self.lemmas.append(
                f"theorem {name} {{o : Oracle}} {{call : Call}}{binder} {{s : State}}{hs} :\n    {stmt} := {proof}\n"
            )
            self.cache[key] = name
        return self.cache[key]


# The proof of a block fact that reads storage after writing it: Measured's sym_exec_st, which
# resolves the reads through the machine's update after every split, after resolving them once
# before any split. A path that splits on no condition (a straight-line block, or one whose
# conditions its hypotheses decide) otherwise reaches sym_exec_st's closing tactics with the reads
# unresolved and fails. Resolving them can close a path outright (its hypotheses contradict).
STORE_READS = """
  obtain ⟨cd, hcd⟩ : ∃ cd, cd = calldataWord call := ⟨_, rfl⟩
  sym_norm
  try simp (config := { decide := true }) only
    [Verify.Gas.FixedFuel.Measured.update_apply, ite_true, ite_false] at *
  all_goals sym_exec_st"""


def reads_after_write(instrs):
    """Whether a block reads storage after writing it: its fact is proved by STORE_READS."""
    wrote = False
    for ins in instrs:
        if ins[0] == "effect" and ins[1] == "sstore":
            wrote = True
        elif ins[0] == "effect" and ins[1] == "sload" and wrote:
            return True
    return False


def choices_for(instrs, stack, facts, choices):
    """The abstracted block's choices: the concrete path's decision for every jumpi, decided or not."""
    js, _ = run_path(instrs, stack, "s", facts, choices)
    return [t for _, t, _ in js]


BLOCKS = None


# --- the proof trees ---------------------------------------------------------------------------


class Tree:
    """Emits the tactic proof of one obligation path by path, one block fact per block: NXT chains a
    block that continues (Reaches.next or ends_next of Verify.Gas.FixedFuel.Counted), and
    LEAF(kind, depth, stack, app, hyps) is the term that closes a path DEPTH blocks in, at the
    header ("header", with its STACK) or at the block fact APP that halts ("end"). HYPS are the
    path's free decisions so far, (word, taken, hypothesis name)."""

    def __init__(self, mod, header, facts, fact_names, nxt, leaf):
        self.mod, self.header, self.facts, self.fact_names = (
            mod,
            header,
            facts,
            fact_names,
        )
        self.nxt, self.leaf = nxt, leaf
        self.n = 0

    def fresh(self, p):
        self.n += 1
        return f"{p}{self.n}"

    def decided(self, c):
        used = facts_of(c)
        fs = ", ".join(f for f in self.fact_names if f in used)
        return f"(by simp only [{fs}]; decide)"

    def walk(self, b, stack, state, depth, forced, ind, acc=()):
        outs = outcomes(BLOCKS[b], stack, state, self.facts)
        return self.split(b, stack, state, depth, forced, outs, [], {}, ind, acc)

    def split(self, b, stack, state, depth, forced, outs, prefix, names, ind, acc):
        pad = "  " * ind
        here = [o for o in outs if o[0][: len(prefix)] == prefix]
        done = [o for o in here if len(o[0]) == len(prefix)]
        if done:
            return self.leaf_lines(b, stack, depth, done[0], names, ind, acc)
        # the next free decision's word
        o = here[0]
        free = [c for c, _, d in o[1] if not d]
        c = free[len(prefix)]
        q, pos = base(c)
        if forced is not None and c == forced[0]:
            names = {**names, len(prefix): forced[1]}
            return self.split(
                b,
                stack,
                state,
                depth,
                None,
                outs,
                prefix + [forced[2]],
                names,
                ind,
                acc,
            )
        h = self.fresh("hc")
        lines = [f"{pad}by_cases {h} : {q}"]
        for branch in (pos, not pos):
            sub = self.split(
                b,
                stack,
                state,
                depth,
                forced,
                outs,
                prefix + [branch],
                {**names, len(prefix): h},
                ind + 1,
                acc,
            )
            lines.append(f"{pad}· " + sub[0].lstrip())
            lines += sub[1:]
        return lines

    def leaf_lines(self, b, stack, depth, out, names, ind, acc):
        pad = "  " * ind
        choices, js, end = out
        kind = "goto" if end[0] == "goto" else "end"
        name = self.mod.block_lemma(b, stack, choices, kind, self.facts)
        proofs, fi, hyps = [], 0, list(acc)
        for c, t, d in js:
            if d:
                proofs.append(self.decided(c))
            else:
                proofs.append(names[fi])
                hyps.append((c, t, names[fi]))
                fi += 1
        app = paren(" ".join([name] + proofs))
        if kind == "end":
            return [f"{pad}exact {self.leaf('end', depth, None, app, hyps)}"]
        nb, nst = end[1], end[2]
        if nb == self.header:
            leaf = self.leaf("header", depth + 1, nst, None, hyps)
            return [f"{pad}exact {self.nxt} {app} fun _ => {leaf}"]
        # the next block's words name the state they were read from
        s2 = self.fresh("s")
        sub = self.walk(nb, nst, s2, depth + 1, None, ind, tuple(hyps))
        bind = s2 if any(re.search(rf"\b{s2}\b", l) for l in sub) else "_"
        if len(sub) == 1 and sub[0].lstrip().startswith("exact "):
            return [f"{pad}exact {self.nxt} {app} fun {bind} => {sub[0].lstrip()[6:]}"]
        return [f"{pad}refine {self.nxt} {app} fun {bind} => ?_"] + sub


# --- the loop's shape ----------------------------------------------------------------------------


class Shape:
    """A guard and a measure of header stacks: KIND down (word K steps up to zero), up (word K
    steps below word J) or rotated (down, with the guard that word K is nonzero). FIRST is the
    header's first decision when the shape cases on it (down, up), else None."""

    def __init__(self, kind, k, j=None, first=None):
        self.kind, self.k, self.j, self.first = kind, k, j, first

    def __repr__(self):
        return f"{self.kind}(k={self.k}, j={self.j})"

    def guard(self):
        return f"(Measured.nonzero {self.k})" if self.kind == "rotated" else "Measured.always"

    def measure(self):
        if self.kind == "up":
            return f"(Measured.up {self.k} {self.j})", f"(Measured.up_lt {self.k} {self.j})"
        return f"(Measured.down {self.k})", f"(Measured.down_lt {self.k})"

    def iter_have(self):
        w = f"w{self.k}"
        if self.kind == "up":
            return f"have hk : {w} < w{self.j} := Measured.upM_pos (i := {w}) (n := w{self.j}) hμ0"
        if self.kind == "rotated":
            return f"have hk : ¬{w} = 0 := hg"
        return f"have hk : ¬{w} = 0 := Measured.down_ne (w := {w}) hμ0"

    def done_have(self):
        w = f"w{self.k}"
        if self.kind == "up":
            return f"have hk : ¬{w} < w{self.j} := Measured.upM_zero (i := {w}) (n := w{self.j}) hμ0"
        return f"have hk : {w} = 0 := Measured.down_zero (w := {w}) hμ0"

    def guard_proof(self, nst, hyps):
        """The guard at a header stack NST, from the path's decisions."""
        if self.kind != "rotated":
            return "trivial"
        for c, t, name in hyps:
            if nonzero_by(c, t, nst[self.k]):
                return name
        raise Unsupported(
            "a rotated loop reaches its header without testing its counter word nonzero"
        )

    def step_proof(self, nst, n):
        """The lower measure at the header stack NST an iteration returns with."""
        k = self.k
        if len(nst) != n:
            raise Unsupported("an iteration returns to the header with another stack depth")
        w = ("var", f"w{k}")
        if self.kind == "up":
            if nst[self.j] != ("var", f"w{self.j}"):
                raise Unsupported("an iteration changes the loop's limit word")
            steps = {
                ("op", "add", lit(1), w): "Measured.upM_step hk",
                ("op", "add", w, lit(1)): "Measured.upM_step' hk",
            }
        else:
            steps = {
                ("op", "add", lit(1), w): "Counted.meas_add_one hk",
                ("op", "add", w, lit(1)): "Counted.meas_add_one' hk",
            }
        if nst[k] not in steps:
            raise Unsupported(
                "an iteration returns to the header without stepping the counter by one"
            )
        return steps[nst[k]]


def shapes_of(houts, n):
    """The shapes to try, in order, from the header's paths at symbolic stack words."""
    out = []
    firsts = {repr(js[0][0]) for _, js, _ in houts if js and not js[0][2]}
    c0 = houts[0][1][0][0] if houts[0][1] else None
    if len(firsts) == 1 and c0 is not None:
        if c0[0] == "var":
            out.append(Shape("down", int(c0[1][1:]), first=c0))
        cmp = None
        if c0[0] == "op" and c0[1] == "lt":
            cmp = (c0[2], c0[3])
        elif c0[0] == "op" and c0[1] == "gt":
            cmp = (c0[3], c0[2])
        if cmp and cmp[0][0] == "var" and cmp[1][0] == "var":
            out.append(Shape("up", int(cmp[0][1][1:]), int(cmp[1][1][1:]), first=c0))
    out += [Shape("rotated", k) for k in range(n)]
    return out


def limit_of(e, nargs, initial):
    """The limit word E bounds a measure by: ("arg", m) for calldata argument m, ("slot", n) for
    storage slot n of the initial state INITIAL, or None."""
    if e[0] == "cdw" and e[1][0] == "lit":
        off = e[1][1]
        if off >= 4 and (off - 4) % 32 == 0 and (off - 4) // 32 < nargs:
            return ("arg", (off - 4) // 32)
    if e[0] == "sload" and e[1] == initial and e[2][0] == "lit":
        return ("slot", e[2][1])
    return None


def meas_val(w):
    return (WORD - w) % WORD


def entry_bound(shape, nst, nargs, initial):
    """How the measure of the entry's header stack NST is bounded: ("arg", m) or ("slot", n) for
    a limit word, or ("const", M) for a measure the words fix."""
    if shape.kind == "up":
        vi, vn = evaluate(nst[shape.k], {}), evaluate(nst[shape.j], {})
        if vi is not None and vn is not None:
            return ("const", vn - vi if vi < vn else 0)
        lim = limit_of(nst[shape.j], nargs, initial)
        if lim is None:
            raise Unsupported(
                f"the loop's limit enters the header as {render(nst[shape.j])}, not a call argument, an initial storage word or a literal"
            )
        return lim
    e = nst[shape.k]
    v = evaluate(e, {})
    if v is not None:
        return ("const", meas_val(v))
    if e[0] == "op" and e[1] == "sub" and e[2] == lit(0):
        lim = limit_of(e[3], nargs, initial)
        if lim is not None:
            return lim
    raise Unsupported(
        f"the loop's counter word enters the header as {render(e)}, not 0 - a call argument, an initial storage word or a literal"
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
    """The Lean module's text. OB is fixed_fuel.obligation's result for this export, and code_term
    the run's fresh runtime bytes. The shapes are tried in order (shapes_of); the first whose every
    path closes is the proof, and when none does every refusal is named."""
    global BLOCKS
    text = (gen_dir / f"{last[0].upper() + last[1:]}.lean").read_text()
    outside = asm_cert.outside(text)
    if outside:
        raise Unsupported(
            f"the final EVM IR is outside the loop checker's fragment: {outside}"
        )
    BLOCKS = asm_cert.parse_program(text)
    H, B, X, E = ob["header"], ob["B"], ob["X"], ob["E"]
    if X < 1:
        raise Unsupported("the exit takes no block")
    facts = {"cv": 0, "cds": 4 + 32 * nargs, "sel": sel}

    # The header: its depth from every entry path.
    arrivals = []

    def scan(b, st, depth):
        if depth > E:
            raise Unsupported(f"an entry path is longer than E = {E} blocks")
        for _ch, _js, end in outcomes(BLOCKS[b], st, f"s{depth}", facts):
            if end[0] == "halt":
                continue
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
    reasons = []
    for shape in shapes_of(houts, n):
        try:
            return emit(
                shape,
                ns=ns,
                contract=contract,
                last=last,
                gen_dir=gen_dir,
                ob=ob,
                code_term=code_term,
                code_import=code_import,
                sel=sel,
                nargs=nargs,
                limit_arg=limit_arg,
                joint=joint,
                source=source,
                n=n,
                facts=facts,
                text=text,
            )
        except Unsupported as e:
            reasons.append(f"{shape}: {e}")
    raise Unsupported("no loop shape closes: " + "; ".join(reasons))


def emit(
    shape,
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
    limit_arg,
    joint,
    source,
    n,
    facts,
    text,
):
    """The module for one shape, or Unsupported."""
    prog = f"{export_ns.of_gen(gen_dir)}.{last}"
    H, B, X, E = ob["header"], ob["B"], ob["X"], ob["E"]
    mod = Module(prog)
    fact_names = ["hcv", "hsz", "hsel"]
    W = [("var", f"w{i}") for i in range(n)]
    first = shape.first
    initial = "(State.initial call)"

    # Iteration: from the header with the guard and a nonzero measure, within B blocks the header
    # again with the guard and a lower measure, or a block that halts.
    def iter_leaf(kind, depth, nst, app, hyps):
        if depth > B:
            raise Unsupported("an iteration takes more than B blocks")
        if kind == "end":
            return f".here (.inr (ends_of_halts {app}))"
        mp = shape.step_proof(nst, n)
        return f".here (.inl ⟨_, _, rfl, rfl, {shape.guard_proof(nst, hyps)}, {mp}⟩)"

    it = Tree(mod, H, {}, [], ".next", iter_leaf)
    iter_lines = it.walk(H, W, "s", 0, (first, "hk", True) if first else None, 3)

    # Exit: from the header with the guard and measure zero, a block that halts within X blocks.
    def done_leaf(kind, depth, _nst, app, _hyps):
        if kind == "header":
            raise Unsupported("the loop's exit returns to its header")
        if depth + 1 > X:
            raise Unsupported(f"the exit takes more than X = {X} blocks")
        return f"ends_of_halts {app}"

    if shape.kind == "rotated":
        done_lines = [f"      exact absurd (Measured.down_zero (w := w{shape.k}) hμ0) hg"]
    else:
        dn = Tree(mod, H, {}, [], "ends_next", done_leaf)
        done_lines = [f"      {shape.done_have()}"] + dn.walk(
            H, W, "s", 0, (first, "hk", False), 3
        )

    # Entry: from the program's entry, within E blocks the header with the guard at a measure of
    # at most M, or a block that halts.
    bounds = []

    def entry_leaf(kind, depth, nst, app, hyps):
        if depth > E:
            raise Unsupported("an entry path takes more than E blocks")
        if kind == "end":
            return f".here (.inr (ends_of_halts {app}))"
        if len(nst) != n:
            raise Unsupported("the header is entered with stacks of different depths")
        i = len(bounds)
        bounds.append((entry_bound(shape, nst, nargs, initial), nst))
        gp = shape.guard_proof(nst, hyps)
        # Tactic blocks: elaborated once rfl has fixed the header stack.
        return f".here (.inl ⟨_, _, rfl, rfl, by exact {gp}, by exact @@M{i}@@⟩)"

    en = Tree(mod, H, facts, fact_names, ".next", entry_leaf)
    entry_lines = en.walk(0, [], initial, 0, None, 2)
    kinds = {b[0][0] for b in bounds}
    if len(kinds) != 1:
        raise Unsupported(
            f"the loop's limit is not of one kind on every entry path: {sorted(kinds)}"
        )
    lkind = kinds.pop()
    lval = None
    if lkind == "const":
        Mc = max(b[0][1] for b in bounds)
    else:
        found = {b[0][1] for b in bounds}
        if len(found) != 1:
            raise Unsupported(
                f"the loop's limit is not one {lkind} on every entry path: {sorted(found)}"
            )
        lval = found.pop()
        if lkind == "arg" and limit_arg is not None and lval != limit_arg:
            raise Unsupported(
                f"the loop's limit is argument {lval}, not argument {limit_arg}"
            )
    Mterm = "M" if lkind == "const" else "K"

    def mproof(nst):
        if lkind == "const":
            if shape.kind == "up":
                return f"(by exact (by decide : Measured.upM ({render(nst[shape.k])}) ({render(nst[shape.j])}) ≤ {Mc}))"
            return f"(by exact (by decide : Counted.meas ({render(nst[shape.k])}).toNat ≤ {Mc}))"
        lp = "Measured.arg_le ha hK" if lkind == "arg" else "hst"
        if shape.kind == "up":
            # The limit word is read off the header stack first: unifying List.getD with the word
            # itself could unfold the word (a calldata read) instead.
            return f"(by refine Measured.up_le ?_; simp only [List.getD_cons_succ, List.getD_cons_zero]; exact {lp})"
        return f"(Measured.meas_neg_le ({lp}))"

    entry_text = "\n".join(entry_lines)
    for i, (_lim, nst) in enumerate(bounds):
        entry_text = entry_text.replace(f"@@M{i}@@", mproof(nst))

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
    if lkind == "const" and Mc > Kmax:
        raise Unsupported(
            f"the loop's {Mc} iterations need 2^64 gas or more: out of gas, not a resource proof"
        )
    if joint and lkind == "const":
        raise Unsupported(
            "the joint consumer takes an iteration bound K; this loop's limit is a literal"
        )

    args = " ".join(f"a{i}" for i in range(nargs))
    arglist = ", ".join(f"a{i}" for i in range(nargs))
    the_call = f"({{ calldata := Abi.calldata {sel} [{arglist}], callvalue := 0, storage := σs }} : Call)"
    ws = ", ".join(f"w{i}" for i in range(n))
    A, L = (
        "SolidCore.Refinement.Solar.Assemble",
        "SolidCore.Refinement.Solar.AssembleLoop",
    )
    # Valid, by the limit's kind.
    conj = []
    if nargs:
        conj.append(" ∧ ".join(f"a{i} < 2 ^ 256" for i in range(nargs)))
    if lkind == "arg":
        conj.append(f"a{lval} ≤ K")
        lim_doc = f"argument {lval} (the loop's limit) at most `K`"
    elif lkind == "slot":
        conj.append(f"SolarTV.slotValue σ {lval} ≤ K")
        lim_doc = f"storage slot {lval} (the loop's limit) at most `K`"
    else:
        lim_doc = f"no bound on the call: the loop's literal limit gives at most {Mc} iterations"
    conj_s = "".join(c + " ∧ " for c in conj)
    pats = [f"h{i}" for i in range(nargs)] + (["hK"] if lkind != "const" else [])
    pat = "".join(p + ", " for p in pats)
    exist = " ".join(["σ"] + ([args] if nargs else []))
    kpar, kapp = (" (K : Nat)", " K") if lkind != "const" else ("", "")
    kb = " {K : Nat} (hK : K ≤ Kmax)" if lkind != "const" else ""
    kb_ = " {K : Nat} (_hK : K ≤ Kmax)" if lkind != "const" else ""
    if nargs:
        has = (
            "(fun x hx => by simp only [List.mem_cons, List.not_mem_nil, or_false] at hx; rcases hx with "
            + " | ".join(["rfl"] * nargs)
            + (" <;> " if nargs > 1 else "; ")
            + "assumption)"
        )
    else:
        has = "(fun x hx => by simp at hx)"
    if lkind == "arg":
        facts_line = f"""obtain ⟨hcv, hsz, hsel, ha⟩ := call_facts {sel} (by decide) [{arglist}]
    {has} σs
    (n := {nargs}) rfl {lval} (by decide) (by decide)"""
        gen_hyps = "hcv hsz hsel ha"
    else:
        facts_line = f"""obtain ⟨hcv, hsz, hsel⟩ := Measured.call_facts0 {sel} (by decide) [{arglist}]
    {has} σs (n := {nargs}) rfl"""
        gen_hyps = "hcv hsz hsel"
    if lkind == "slot":
        facts_line += f"""
  have hst : ((State.initial {the_call}).storage {lval}).toNat ≤ K :=
    Measured.initial_storage_le hK"""
        gen_hyps += " hst"
    inh = ", ".join(
        ["by decide"] * nargs + (["Nat.zero_le K"] if lkind != "const" else []) + ["rfl"]
    )
    zeros = "".join("0, " for _ in range(nargs))
    inh_term = f"⟨_, [], {zeros}{inh}⟩"
    mu, hmu = shape.measure()
    if lkind == "const":
        mdef = f"""
/-- The loop's iteration bound: its limit is a literal. -/
abbrev M : Nat := {Mc}
"""
        kmax_doc = f"`M = {Mc} ≤ Kmax = {Kmax}`"
        verdict_s = f"""theorem verdict :
    verdictOf (some M) (perIterGas cert {B}) (exitGas cert {E} {X}) = .bounded (M * {pi} + {ex}) :=
  verdict_of (by decide +kernel) (by decide +kernel) (by decide)"""
        fit_s = f"""theorem fit : {L}.gasBoundL cert (fixedFuel {E} {B} {X} M) < 2 ^ 64 :=
  Measured.fit_of (pi := {pi}) (ex := {ex}) (by decide +kernel) (by decide +kernel) (by decide)"""
        fitapp, verapp, totapp = "fit", "verdict", "total"
    else:
        mdef = ""
        kmax_doc = f"`K ≤ Kmax = {Kmax}`"
        verdict_s = f"""theorem verdict {{K : Nat}} (hK : K ≤ Kmax) :
    verdictOf (some K) (perIterGas cert {B}) (exitGas cert {E} {X}) = .bounded (K * {pi} + {ex}) :=
  verdict_of (by decide +kernel) (by decide +kernel) (by unfold Kmax at hK; omega)"""
        fit_s = f"""theorem fit {{K : Nat}} (hK : K ≤ Kmax) :
    {L}.gasBoundL cert (fixedFuel {E} {B} {X} K) < 2 ^ 64 :=
  Measured.fit_of (pi := {pi}) (ex := {ex}) (by decide +kernel) (by decide +kernel)
    (by unfold Kmax at hK; omega)"""
        fitapp, verapp, totapp = "fit hK", "verdict hK", "total hK"
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
    imports = [
        "Verify.Gas.FixedFuel.Measured",
        "Verify.ShapeGeneric",
        "W1Proofs.Tactic",
        "SolarTV.Composition",
        code_import,
    ]
    if joint:
        imports.append(joint["import"])
    imports += (source or {}).get("imports", [])
    seen = []
    for m in imports:
        if m not in seen:
            seen.append(m)
    shape_doc = {
        "down": f"header word {shape.k} steps by one up to zero",
        "up": f"header word {shape.k} steps by one below header word {shape.j}",
        "rotated": f"rotated: header word {shape.k} is nonzero at the header and steps by one up to zero",
    }[shape.kind]
    body = f"""{"".join("import " + m + chr(10) for m in seen)}
-- Generated by solar's tools/certify/loop_resources.py from this run's stage export
-- ({gen_dir.name}/{last}), tools/certify/fixed_fuel.py's obligation and tools/certify/asm_cert.py's
-- certificate; do not edit. Untrusted data: the kernel re-derives every claim.

/-!
# {contract}: the fixed-fuel premises of its runtime loop

Header block {H} (depth {n}; {shape_doc}), B = {B}, X = {X}, E = {E}; for {kmax_doc} the gas
bound is `verdictOf (some {Mterm}) {pi} {ex} = .bounded ({Mterm} * {pi} + {ex})`. `Valid`: a call of
selector {sel:#x} with {nargs} ABI word argument(s), no value, any storage, and {lim_doc}.
`loops` is `Verify.Gas.FixedFuel.Measured.loops_of` on this program's block facts; `premises` are
`check_ok`, `loops` and `fit` in the form the erasure join's `GeneratedTarget` takes them.
-/

namespace {ns}

open SolarTV SolarTV.Machine SolarTV.Milestone.W1
open SolidCore.Refinement SolidCore.Refinement.Solar SolidCore.Refinement.Generic Verify.Gas.FixedFuel
open Verify.LoopSource.Interface
open Verify.Gas.FixedFuel.Counted (halts ends_of_halts ends_next call_facts checkL_of verdict_of
  total_of_loops)

/-- The runtime bytes this run emitted. -/
abbrev code : ByteArray := {code_term}

/-- The loop checker's certificate (tools/certify/asm_cert.py). -/
def cert : {A}.Cert :=
  {cert}

/-- The highest stack each block is entered with. -/
def maxs : List Nat := {maxs}

set_option maxRecDepth 100000 in
/-- The checker of code with loops accepts the exact bytecode for the final EVM IR. -/
theorem check_ok : {L}.checkL {prog} code cert maxs = true :=
  checkL_of (by decide +kernel) (by decide +kernel) (by decide +kernel) (by decide)

/-- The largest iteration bound whose fixed gas is below `2 ^ 64`. -/
def Kmax : Nat := {Kmax}
{mdef}
/-- **The resource verdict**: bounded. -/
{verdict_s}

/-- **The fit**: the fixed fuel's gas is below `2 ^ 64`. -/
{fit_s}

/-- The valid calls. -/
def Valid{kpar} (call : Call) : Prop :=
  ∃ {exist}, {conj_s}call = {{ calldata := Abi.calldata {sel} [{arglist}], callvalue := 0, storage := σ }}

/-- **Inhabited**: the call with every argument zero, from empty storage. -/
theorem inhabited{kpar} : ∃ call, Valid{kapp} call :=
  {inh_term}

/-- The loop's record: a flag and the iterations left. -/
abbrev record : LoopRecord := Measured.record

/-! ## Block facts of the final EVM IR -/

section
set_option maxHeartbeats 4000000

{chr(10).join(mod.lemmas)}
end

/-- **Loops**: from the entry, every valid call reaches the header within E blocks with the guard
at a measure at most {Mterm}, or ends; from the header, an iteration takes at most B blocks and the
exit X. -/
theorem loops (o : Oracle){kpar} (call : Call) (hv : Valid{kapp} call) :
    Loops (EVM.Program.block o call {prog}) (0, [], State.initial call) record {E} {B} {X} {Mterm} := by
  obtain ⟨σs, {(args.replace(" ", ", ") + ", ") if nargs else ""}{pat}rfl⟩ := hv
  {facts_line}
  generalize {the_call} = call at {gen_hyps} ⊢
  refine Measured.loops_of (H := {H}) (n := {n}) {shape.guard()} {mu} {hmu}
    (fun ws s hn hg hμ0 => ?_) (fun ws s hn hg hμ0 => ?_) ?_
  · match ws, hn with
    | [{ws}], _ =>
      {shape.iter_have()}
{chr(10).join(iter_lines)}
  · match ws, hn with
    | [{ws}], _ =>
{chr(10).join(done_lines)}
  · {entry_text.lstrip()}

/-- **The premises of the erasure join** (`GeneratedTarget`'s `checkL`, `Loops` and fit), at
`Shapes.oracle`. -/
theorem premises{kb} :
    {L}.checkL {prog} code cert maxs = true ∧
    (∀ call, Valid{kapp} call → Loops (EVM.Program.block Shapes.oracle call {prog})
      (0, [], SolarTV.Machine.State.initial call) record {E} {B} {X} {Mterm}) ∧
    {L}.gasBoundL cert (fixedFuel {E} {B} {X} {Mterm}) < 2 ^ 64 :=
  ⟨check_ok, fun call hv => loops Shapes.oracle{kapp} call hv, {fitapp}⟩

/-- **Totality** of the final EVM IR on the valid calls, from `Loops`. -/
theorem total{kb_} :
    ∀ call, Valid{kapp} call → ∃ obs, (evmirSem Shapes.oracle).run {prog} call = some obs :=
  total_of_loops fun call hv => loops Shapes.oracle{kapp} call hv

/-- **The final EVM IR's resources** on the exact bytes: `resources_of_verdict` with the stage of
the final EVM IR to itself and the loop's bound for totality. -/
theorem resources_evmir{kb} :
    SolidCore.Refinement.Shapes.Resources (evmirSem Shapes.oracle) {prog} code (Valid{kapp})
      (fun _ => {Mterm} * {pi} + {ex}) :=
  resources_of_verdict ({verapp}) check_ok Shapes.oracle (fun _ => Shapes.oracle_ok.1)
    (fun _ => Shapes.oracle_ok.2)
    ({totapp})
    (Stage.refl _ _ _) (fun call hv => loops Shapes.oracle{kapp} call hv)

/-- info: '{ns}.premises' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms premises

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
