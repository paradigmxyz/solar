#!/usr/bin/env python3
"""A loop's header invariant on solar's first lowered MIR, from solar-verify's inference.

solar-verify infers each loop's invariants and termination measure over the source
('solar-verify --show-inferred') and checks them as verification conditions. This module carries
what it printed for one function's loop to the loop's header in solar's first lowered MIR (the
stage the exporter writes as Stage0.lean) and writes them as a Lean module, 'Inv' and 'rank',
which the cut-point proof of that function checks segment by segment (SolarTV.CutPointHoare).
Nothing in it is written by hand for one contract:

- the source variables of the inferred clauses are bound to MIR values mechanically: the loop
  variable and its bound to the operands of the header's 'lt' branch into the loop (the for
  statement's guard), a state variable to the header phi whose entry input loads its slot (else
  to the slot), a parameter to its calldata word; 'old(v)' and 'entry(v)' to the value at the
  call's start and at the loop's entry, read off the definitions before the loop;
- each clause becomes one conjunct at every predecessor of the header, the phis read through
  their inputs from that predecessor;
- the frame the compiler keeps around the loop is read off the MIR alone: storage unchanged when
  no block that reaches the header stores, the definitions of the values the loop reads from
  before it, the conditions of the branches on the one path from the function's entry to the
  loop, and the facts ('assume') the header and its successors check of memory loads before any
  store;
- the measure becomes 'rank', read in Nat;
- arrays: a 'uint256[] memory' parameter of a library function the compiler inlined is the memory
  array whose length word the header checks equals the loop's bound ('xs.length'), 'xs[e]' its
  word at 'xs + 32 + 32 * e', and 'forall'/'exists' clauses over a uint256 quantify over words;
  the function's one local is the header's one remaining phi, a phi that steps by a constant as
  the loop variable steps by one is related to it, and a calldata copy before the loop that
  nothing later overwrites gives its words as the calldata's.

Nothing here is trusted: a clause that does not hold or that the loop does not keep makes a
segment proof fail by mismatch, and so does a wrong binding. Shapes it cannot read (several
loops in the function, a guard that is not 'lt', an expression outside its grammar) fail closed.

Usage: inferred.py STAGE0.lean SOURCE.sol INFERRED.txt FUNCTION OUT.lean NAMESPACE MODULE
where INFERRED.txt is what 'solar-verify --show-inferred SOURCE.sol' printed.
"""

import hashlib
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cutpoints import Unsupported, mir_headers, parse_mir, split_top

WORD = "EvmRules.Word"
BT = chr(96)


def code(s):
    return BT + s + BT


# --- The source: state variables, parameters, the loop -------------------------------------


def source_facts(sol, function):
    """State variables of one slot each (in declaration order), the function's parameters with
    their calldata offsets, and its for loop: (variable, start, bound)."""
    body = re.sub(r"//[^\n]*", "", sol)
    contract = body[body.index("contract") :]
    depth, cur = 0, []
    for ch in contract[contract.index("{") + 1 :]:
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
        if depth == 0:
            cur.append(ch)
    slots = {}
    for decl in "".join(cur).split(";"):
        m = re.fullmatch(
            r"\s*uint256(?:\s+(?:public|private|internal))?\s+(\w+)\s*", decl
        )
        if m:
            slots[m[1]] = len(slots)
    m = re.search(rf"function\s+{function}\s*\(([^)]*)\)", body)
    if not m:
        raise Unsupported(f"no function {function}")
    params, arrays = {}, {}
    for k, p in enumerate(x for x in m[1].split(",") if x.strip()):
        am = re.fullmatch(r"\s*uint256\[\]\s+(memory|calldata)\s+(\w+)\s*", p)
        if am:
            arrays[am[2]] = am[1]
            continue
        if p.split()[0] != "uint256":
            raise Unsupported(f"{function}: parameter {p.split()[-1]} is not a uint256")
        params[p.split()[-1]] = 4 + 32 * k
    if arrays and params:
        raise Unsupported(f"{function}: array and word parameters together")
    fm = re.search(
        r"for\s*\(\s*uint256\s+(\w+)\s*=\s*(\d+)\s*;\s*(\w+)\s*<\s*(\w+(?:\.length)?)\s*;",
        body[m.end() :],
    )
    if not fm or fm[1] != fm[3]:
        raise Unsupported(
            f"{function}: no for loop of the shape 'for (uint256 i = c; i < b; ...)'"
        )
    if fm[4].endswith(".length") and fm[4][: -len(".length")] not in arrays:
        raise Unsupported(
            f"{function}: the loop's bound {fm[4]} is not a parameter's length"
        )
    return slots, params, arrays, (fm[1], int(fm[2]), fm[4])


def inferred_clauses(text, function):
    """The invariants and the measure solar-verify printed for the function's one loop."""
    invs, measures, lines = [], [], set()
    for m in re.finditer(rf"inferred \w+\.{function} loop at line (\d+): (.*)", text):
        lines.add(m[1])
        what = m[2].strip()
        if what.startswith("invariant "):
            im = re.fullmatch(r"invariant (.*) \((\w+)\)", what)
            invs.append((im[1].strip(), im[2]))
        elif what.startswith("terminates, measure "):
            measures.append(what[len("terminates, measure ") :])
    if len(lines) != 1:
        raise Unsupported(f"{function}: {len(lines)} inferred loops, one supported")
    if len(measures) != 1:
        raise Unsupported(f"{function}: no inferred termination measure")
    return invs, measures[0], lines.pop()


# --- Expressions -----------------------------------------------------------------------------

TOKEN = re.compile(r"\s*(\d+|\w+\.length\b|\w+|==>|::|<=|>=|==|!=|&&|[-+*<>()\[\]])")


def tokens(s):
    out, pos, s = [], 0, s.rstrip()
    while pos < len(s):
        m = TOKEN.match(s, pos)
        if not m:
            raise Unsupported(f"cannot read {s!r}")
        out.append(m[1])
        pos = m.end()
    return out


def parse(s):
    """A clause: comparisons and conjunctions of sums of variables, old(v), entry(v), array
    lengths 'a.length', elements 'a[e]' and numbers, or a quantifier over a uint256
    ('forall uint256 j :: A ==> B', 'exists uint256 j :: A && B') of such, with an optional
    substitution suffix '[v := w]'."""
    subst, bound = {}, set()
    sm = re.fullmatch(r"(.*?)\s*\[([\w.]+) := (\w+)\]", s)
    if sm:
        s, subst = sm[1], {sm[2]: sm[3]}
    ts = tokens(s)
    pos = [0]

    def peek():
        return ts[pos[0]] if pos[0] < len(ts) else None

    def take(t=None):
        if pos[0] >= len(ts):
            raise Unsupported(f"cannot read {s!r}")
        tok = ts[pos[0]]
        if t is not None and tok != t:
            raise Unsupported(f"cannot read {s!r}: expected {t}")
        pos[0] += 1
        return tok

    def atom():
        tok = take()
        if tok == "(":
            e = conj()
            take(")")
            return e
        if tok.isdigit():
            return ("num", int(tok))
        if tok in ("old", "entry"):
            take("(")
            v = take()
            take(")")
            return (tok, subst.get(v, v))
        if tok in bound:
            return ("bound", tok)
        if re.fullmatch(r"[A-Za-z_]\w*", tok) and peek() == "[":
            take("[")
            e = term()
            take("]")
            return ("index", tok, e)
        if re.fullmatch(r"[A-Za-z_]\w*(?:\.length)?", tok):
            return ("var", subst.get(tok, tok))
        raise Unsupported(f"cannot read {s!r} at {tok}")

    def term():
        e = atom()
        while peek() in ("+", "-", "*"):
            e = (take(), e, atom())
        return e

    def cmp():
        e = term()
        if peek() in ("<=", ">=", "<", ">", "==", "!="):
            e = (take(), e, term())
        return e

    def conj():
        e = cmp()
        while peek() == "&&":
            take()
            e = ("&&", e, cmp())
        return e

    def clause():
        if peek() in ("forall", "exists"):
            q = take()
            take("uint256")
            j = take()
            if not re.fullmatch(r"[A-Za-z_]\w*", j):
                raise Unsupported(f"cannot read {s!r}: bound variable {j}")
            take("::")
            bound.add(j)
            e = conj()
            if peek() == "==>":
                take()
                e = ("==>", e, conj())
            bound.discard(j)
            return (q, j, e)
        return conj()

    e = clause()
    if pos[0] != len(ts):
        raise Unsupported(f"cannot read {s!r}")
    return e


BINOPS = ("+", "-", "*", "<=", ">=", "<", ">", "==", "!=", "&&", "==>")


def fold(e):
    """Drops additions of zero."""
    if e[0] in ("+", "-") and e[2] == ("num", 0):
        return fold(e[1])
    if e[0] == "+" and e[1] == ("num", 0):
        return fold(e[2])
    if e[0] in BINOPS:
        return (e[0], fold(e[1]), fold(e[2]))
    if e[0] in ("forall", "exists"):
        return (e[0], e[1], fold(e[2]))
    if e[0] == "index":
        return ("index", e[1], fold(e[2]))
    return e


def lean_word(e, val):
    if e[0] == "num":
        return f"({e[1]} : {WORD})"
    if e[0] in ("var", "old", "entry"):
        return val(e)
    if e[0] == "bound":
        return e[1]
    if e[0] == "index":
        return val(("index", e[1], lean_word(e[2], val)))
    if e[0] in ("+", "-", "*"):
        a, b = lean_word(e[1], val), lean_word(e[2], val)
        zero = f"(0 : {WORD})"
        if e[0] in ("+", "-") and b == zero:
            return a
        if e[0] == "+" and a == zero:
            return b
        return f"({a} {e[0]} {b})"
    raise Unsupported(f"not a word: {e}")


def lean_prop(e, val):
    op = e[0]
    if op == "&&":
        return f"({lean_prop(e[1], val)} ∧ {lean_prop(e[2], val)})"
    if op == "==>":
        return f"({lean_prop(e[1], val)} → {lean_prop(e[2], val)})"
    if op == "forall":
        return f"(∀ {e[1]} : {WORD}, {lean_prop(e[2], val)})"
    if op == "exists":
        return f"(∃ {e[1]} : {WORD}, {lean_prop(e[2], val)})"
    if op not in ("<=", ">=", "<", ">", "==", "!="):
        raise Unsupported(f"not a proposition: {e}")
    a, b = lean_word(e[1], val), lean_word(e[2], val)
    return {
        "<=": f"{a} ≤ {b}",
        ">=": f"{b} ≤ {a}",
        "<": f"{a} < {b}",
        ">": f"{b} < {a}",
        "==": f"{a} = {b}",
        "!=": f"{a} ≠ {b}",
    }[op]


def lean_nat(e, val):
    if e[0] == "num":
        return str(e[1])
    if e[0] in ("var", "old", "entry"):
        return f"({val(e)}).toNat"
    if e[0] in ("+", "-", "*"):
        return f"({lean_nat(e[1], val)} {e[0]} {lean_nat(e[2], val)})"
    raise Unsupported(f"not a measure: {e}")


# --- The MIR ---------------------------------------------------------------------------------


def definitions(func):
    """Each value's defining instruction, with its block last."""
    out = {}
    for b, (phis, insts, _) in func["blocks"].items():
        for d, _ in phis:
            out[d] = ("phi", b)
        for ins in insts:
            m = re.match(r"\.pure (\d+) \.(\w+) \[(.*)\]$", ins)
            if m:
                out[int(m[1])] = ("pure", m[2], split_top(m[3]), b)
                continue
            m = re.match(r"\.effect \(some (\d+)\) \.(\w+) \[(.*)\]$", ins)
            if m:
                out[int(m[1])] = ("effect", m[2], split_top(m[3]), b)
                continue
            m = re.match(r"\.heapFloor (\d+) (\S+)$", ins)
            if m:
                out[int(m[1])] = ("floor", m[2], b)
                continue
            m = re.match(r"\.(?:ne|word|low|select) (\d+) ", ins)
            if m:
                out[int(m[1])] = ("other", b)
    return out


def reach(start, nxt):
    seen, todo = set(), [start]
    while todo:
        for m in nxt(todo.pop()):
            if m not in seen:
                seen.add(m)
                todo.append(m)
    return seen


def generate(stage0_text, sol, inferred_text, function, stage0_name, namespace, module):
    funcs = parse_mir(stage0_text)
    # A function the compiler inlined into another keeps its source name before a ".N" suffix.
    fidx = next((i for i, f in enumerate(funcs) if f["name"] == function), None)
    if fidx is None:
        named = [i for i, f in enumerate(funcs) if f["name"].split(".")[0] == function]
        fidx = named[0] if len(named) == 1 else None
    if fidx is None:
        raise Unsupported(f"no MIR function @{function}")
    blocks = funcs[fidx]["blocks"]
    heads, preds, succ = mir_headers(funcs[fidx])
    if len(heads) != 1:
        raise Unsupported(f"@{function}: {len(heads)} loop headers, one supported")
    (h,) = heads
    slots, params, arrays, (ivar, istart, bound) = source_facts(sol, function)
    invs, measure, line = inferred_clauses(inferred_text, function)
    defs = definitions(funcs[fidx])
    pred_of = {b: [a for a in blocks if b in succ[a]] for b in blocks}
    loop = (reach(h, lambda n: succ[n]) & reach(h, lambda n: pred_of[n])) | {h}
    entry_preds = [p for p in preds[h] if p not in loop]
    if len(entry_preds) != 1:
        raise Unsupported(
            f"@{function} bb{h}: entered from {entry_preds}, one entry supported"
        )
    (pre,) = entry_preds
    reaching = reach(h, lambda n: pred_of[n]) | {h}
    if any(
        i.startswith(".effect none .sstore") for b in reaching for i in blocks[b][1]
    ):
        raise Unsupported(
            f"@{function}: a block that reaches the loop stores to storage"
        )
    hphis, _, hterm = blocks[h]
    phi_in = {d: dict(inc) for d, inc in hphis}

    def writes_memory(ins):
        return re.match(
            r"\.effect none \.(?:mstore|mstore8|calldatacopy|codecopy|mcopy|returndatacopy) ",
            ins,
        ) or ins.startswith(".alloc")

    loop_writes = any(writes_memory(i) for b in loop for i in blocks[b][1])

    def stable_load(v):
        """Whether the load defining v reads the memory the loop sees: it is in the loop's
        entry block after its last write, and the loop writes none."""
        b = defs[v][-1]
        if b != pre or loop_writes:
            return False
        insts = blocks[b][1]
        k = next(
            k for k, i in enumerate(insts) if re.match(rf"\.effect \(some {v}\) ", i)
        )
        return not any(writes_memory(i) for i in insts[k + 1 :])

    def expand(v):
        """A value defined before the loop as a term of the call, or None."""
        d = defs.get(v)
        if d is None or d[0] in ("phi", "other") or d[-1] in loop:
            return None
        if d[0] == "floor":
            return f"({d[1]} : {WORD})"
        args = d[2]
        if d[0] == "effect":
            if d[1] == "calldataload" and args[0].startswith(".const"):
                return f"calldataWord call {args[0].split()[1]}"
            if d[1] == "sload" and args[0].startswith(".const"):
                return f"(State.initial call).storage {args[0].split()[1]}"
            if d[1] == "calldataload":
                t = operand(args[0])
                return None if t is None else f"calldataWord call {t}"
            if d[1] == "mload" and stable_load(v):
                if args[0].startswith(".const"):
                    return f"s.memory.load {args[0].split()[1]}"
                t = operand(args[0])
                return None if t is None else f"s.memory.load ({t}).toNat"
            return None
        if d[1] in ("add", "sub", "mul"):
            terms = [operand(a) for a in args]
            if None in terms:
                return None
            sym = {"add": "+", "sub": "-", "mul": "*"}[d[1]]
            return f"({terms[0]} {sym} {terms[1]})"
        return None

    def operand(op):
        kind, n = op.split()
        return f"({n} : {WORD})" if kind == ".const" else expand(int(n))

    def at(op):
        kind, n = op.split()
        return f"({n} : {WORD})" if kind == ".const" else f"env {n}"

    # The loop's guard: the header's branch on 'lt [i, bound]' into the loop.
    bm = re.fullmatch(r"\.branch \(\.value (\d+)\) (\d+) (\d+)", hterm)
    gdef = defs.get(int(bm[1])) if bm else None
    if not gdef or gdef[:2] != ("pure", "lt") or int(bm[2]) not in loop:
        raise Unsupported(
            f"@{function} bb{h}: the header does not branch on 'lt' into the loop"
        )
    gi, gb = gdef[2]
    if not gi.startswith(".value") or int(gi.split()[1]) not in phi_in:
        raise Unsupported(f"@{function} bb{h}: the guard does not compare a header phi")
    binding = {ivar: ("phi", int(gi.split()[1]))}
    if phi_in[binding[ivar][1]].get(pre) != f".const {istart}":
        raise Unsupported(f"@{function}: the loop variable does not start at {istart}")
    btxt = operand(gb)
    if btxt is None:
        raise Unsupported(
            f"@{function} bb{h}: the loop's bound is not read before the loop"
        )
    if bound in params and btxt != f"calldataWord call {params[bound]}":
        raise Unsupported(
            f"@{function}: the guard's bound is {btxt}, not parameter {bound}"
        )
    binding[bound] = ("input", btxt)
    # The loop's other header phis: an induction pointer steps by a constant on every back
    # edge as the loop variable steps by one; the one left is the function's one local.
    back = [p for p in preds[h] if p in loop]
    iv = binding[ivar][1]

    def steps(q, by):
        for p in back:
            o = phi_in[q].get(p, "")
            d = defs.get(int(o.split()[1])) if o.startswith(".value") else None
            if not d or d[:2] != ("pure", "add") or d[2][0] != f".value {q}":
                return None
            if not d[2][1].startswith(".const") or (by and d[2][1] != f".const {by}"):
                return None
            c = d[2][1].split()[1]
        return c

    if steps(iv, "1") is None:
        raise Unsupported(f"@{function} bb{h}: the loop variable does not step by one")
    induction = {q: steps(q, None) for q in phi_in if q != iv}
    induction = {q: c for q, c in induction.items() if c is not None}
    rest = [q for q in phi_in if q != iv and q not in induction]
    # An array parameter is the memory array whose length word the header checks is the bound.
    for name in arrays:
        ptr = None
        for b in [h] + succ[h]:
            loads = {}
            for ins in blocks[b][1]:
                m = re.match(r"\.effect \(some (\d+)\) \.mload \[\.value (\d+)\]", ins)
                if m:
                    loads[int(m[1])] = int(m[2])
                m = re.match(r"\.assume false \.eq \[\.value (\d+), (.*)\]$", ins)
                if (
                    m
                    and int(m[1]) in loads
                    and m[2] == gb
                    and bound == f"{name}.length"
                ):
                    ptr = loads[int(m[1])]
        if ptr is None or ptr in phi_in or expand(ptr) is None:
            raise Unsupported(
                f"@{function}: no memory array of {name} read before the loop"
            )
        binding[name] = ("array", ptr)
    for name, slot in slots.items():
        sload = f"(State.initial call).storage {slot}"
        phi = next(
            (
                d
                for d, inc in phi_in.items()
                if d != binding[ivar][1]
                and inc[pre].startswith(".value")
                and expand(int(inc[pre].split()[1])) == sload
            ),
            None,
        )
        binding.setdefault(name, ("phi", phi) if phi is not None else ("input", sload))
    for name, off in params.items():
        binding.setdefault(name, ("input", f"calldataWord call {off}"))

    def names(e):
        if e[0] == "var":
            return {e[1]}
        if e[0] in ("old", "entry"):
            return {e[1]}
        if e[0] in ("num", "bound"):
            return set()
        if e[0] == "index":
            return {e[1]} | names(e[2])
        if e[0] in ("forall", "exists"):
            return names(e[2])
        return names(e[1]) | names(e[2])

    clauses = [(c, o, fold(parse(c))) for c, o in invs]
    mexpr = fold(parse(measure))
    free = set().union(*(names(e) for _, _, e in clauses), names(mexpr)) - set(binding)
    if free:
        if len(free) != 1 or len(rest) != 1:
            raise Unsupported(
                f"@{function} bb{h}: locals {sorted(free)} for header phis {rest}"
            )
        binding[free.pop()] = ("phi", rest[0])

    def start_value(name):
        if name in slots:
            return f"(State.initial call).storage {slots[name]}"
        if name in params:
            return f"calldataWord call {params[name]}"
        raise Unsupported(f"old({name}): not a state variable or parameter")

    def entry_value(name):
        kind, x = binding[name]
        if kind == "input":
            return x
        t = operand(phi_in[x][pre])
        if t is None:
            raise Unsupported(
                f"entry({name}): its value at the loop's entry is not read"
            )
        return t

    def reader(p):
        def val(e):
            kind, name = e[0], e[1]
            if kind == "old":
                return start_value(name)
            if kind == "entry":
                return entry_value(name)
            if kind == "index":
                k, x = binding.get(name, (None, None))
                if k != "array":
                    raise Unsupported(f"{name}: not an array")
                return f"s.memory.load (env {x} + 32 + 32 * {e[2]}).toNat"
            if name not in binding:
                raise Unsupported(f"{name}: no MIR value")
            k, x = binding[name]
            return x if k == "input" else at(phi_in[x][p])

        return val

    # The frame, read off the MIR.
    frame = ["s.storage = (State.initial call).storage"]
    reads = set()
    for b in loop:
        for ins in blocks[b][1] + [blocks[b][2]]:
            reads |= {int(n) for n in re.findall(r"\.value (\d+)", ins)}
    inside = {v for v, d in defs.items() if d[-1] in loop}
    for v in sorted(reads - inside):
        if expand(v) is not None:
            frame.append(f"env {v} = {expand(v)}")
    path, node = [], pre
    while node != 0:
        ps = [a for a in pred_of[node] if a not in loop]
        if len(ps) != 1:
            raise Unsupported(
                f"@{function}: bb{node} has {len(ps)} predecessors before the loop"
            )
        path.append((ps[0], node))
        node = ps[0]
    for a, b in reversed(path):
        m = re.fullmatch(r"\.branch \(\.value (\d+)\) (\d+) (\d+)", blocks[a][2])
        d = defs.get(int(m[1])) if m else None
        if not d or d[0] != "pure" or d[1] not in ("lt", "gt"):
            continue
        x, y = (operand(o) for o in d[2])
        if x is None or y is None:
            continue
        if d[1] == "gt":
            x, y = y, x
        frame.append(f"{x} < {y}" if int(m[2]) == b else f"{y} ≤ {x}")

    # A calldata copy before the loop, which neither its block's rest nor the loop overwrites:
    # its words are the calldata's.
    for b in dict.fromkeys([a for a, _ in reversed(path)] + [pre]):
        insts = blocks[b][1]
        for k, ins in enumerate(insts):
            m = re.fullmatch(r"\.effect none \.calldatacopy \[(.*)\]", ins)
            if not m or loop_writes or any(writes_memory(i) for i in insts[k + 1 :]):
                continue
            dst, src, size = split_top(m[1])
            sd = defs.get(int(size.split()[1])) if size.startswith(".value") else None
            if not sd or sd[:2] != ("pure", "shl") or sd[2][0] != ".const 5":
                continue
            d, c, n = operand(dst), operand(src), operand(sd[2][1])
            if None in (d, c, n):
                continue
            frame.append(
                f"(∀ j : {WORD}, j < {n} → s.memory.load ({d} + 32 * j).toNat = "
                f"calldataWord call ({c} + 32 * j))"
            )

    def induction_facts(p):
        start = istart
        out = []
        named = {x for k, x in binding.values() if k == "phi"}
        for q, c in sorted(induction.items()):
            if q in named:
                continue
            e0 = operand(phi_in[q][pre])
            if e0 is None:
                raise Unsupported(f"@{function} bb{h}: pointer phi {q} starts unread")
            out.append(
                f"{at(phi_in[q][p])} = {e0} + {c} * ({at(phi_in[iv][p])} - {start})"
            )
        return out

    def mem_facts(p):
        out = []
        firsts = [h] + succ[h]
        for b in firsts:
            if b != h and any(
                i.startswith(".effect none .mstore") for i in blocks[h][1]
            ):
                continue
            loads = {}
            for ins in blocks[b][1]:
                if ins.startswith(".effect none .mstore"):
                    break
                m = re.match(r"\.effect \(some (\d+)\) \.mload \[\.const (\d+)\]", ins)
                if m:
                    loads[int(m[1])] = int(m[2])
                m = re.match(r"\.effect \(some (\d+)\) \.mload \[\.value (\d+)\]", ins)
                if m and int(m[2]) not in inside and expand(int(m[2])) is not None:
                    loads[int(m[1])] = f"(env {m[2]}).toNat"
                m = re.match(
                    r"\.assume false \.eq \[\.value (\d+), \.value (\d+)\]", ins
                )
                if m and int(m[1]) in loads:
                    other = int(m[2])
                    if other in phi_in:
                        rhs = at(phi_in[other][p])
                    elif expand(other) is not None:
                        rhs = f"env {other}"
                    else:
                        continue
                    fact = f"s.memory.load {loads[int(m[1])]} = {rhs}"
                    if fact not in out:
                        out.append(fact)
        return out

    sha = lambda t: hashlib.sha256(t.encode()).hexdigest()
    out = [
        "-- Generated by solar-tv/milestone/inferred.py from what solar-verify --show-inferred printed",
        f"-- (sha256 {sha(inferred_text)}) and {stage0_name} (sha256 {sha(stage0_text)}); do not edit.",
        "import W1Proofs.Stage",
        f"import {module}",
        "",
        "/-!",
        f"# The inferred invariant of {code(function)}'s loop at its header",
        "",
        f"solar-verify inferred, for the loop at line {line} of the source:",
        "",
    ]
    out += [f"* {code(c)} ({o})" for c, o, _ in clauses]
    out += [f"* the measure {code(measure)}", ""]
    binds = ", ".join(
        f"{code(n)} to "
        + (
            f"phi {x}"
            if k == "phi"
            else f"the memory array at {x}"
            if k == "array"
            else code(x)
        )
        for n, (k, x) in binding.items()
    )
    out += [
        f"They are bound to bb{h} of function {fidx} of the first lowered MIR as {binds}; the",
        "frame (storage, the values the loop reads from before it, the branches before the loop and",
        "the facts the header checks of memory) is read off the MIR. The cut-point proof checks every",
        "conjunct on every segment; none is assumed.",
        "-/",
        "",
        f"namespace {namespace}",
        "",
        "open Machine SolarTV.Milestone.W1",
        "",
    ]
    for p in sorted(preds[h]):
        inputs = sorted(
            {
                int(inc[p].split()[1])
                for inc in phi_in.values()
                if inc[p].startswith(".value")
            }
        )
        entry_defs = [f"env {v} = {expand(v)}" for v in inputs if expand(v) is not None]
        conj = (
            [lean_prop(e, reader(p)) for _, _, e in clauses]
            + entry_defs
            + induction_facts(p)
            + frame
            + mem_facts(p)
        )
        kind = "entry" if p == pre else "back edge"
        out += [
            f"/-- At bb{h} entered from bb{p} (the loop's {kind}), before its phis bind. -/",
            f"def at{p} (call : Call) (env : MIR.Lowered.Env) (s : State) : Prop :=",
            "  " + " ∧\n    ".join(conj),
            "",
        ]
    out += [
        f"/-- The header invariant: the frame enters bb{h} of function {fidx} from one of its predecessors,",
        "and that predecessor's conjuncts hold. -/",
        "def Inv (call : Call) (x : MIR.Lowered.Frame) : Prop :=",
        "  "
        + " ∨\n  ".join(
            f"(∃ env s, x = ([], .enter {fidx} {p} {h}, env, s) ∧ at{p} call env s)"
            for p in sorted(preds[h])
        ),
        "",
        f"/-- The measure {code(measure)} at the header, in Nat. -/",
        "def rank (call : Call) : MIR.Lowered.Frame → Nat",
    ]
    for p in sorted(preds[h]):
        r = lean_nat(mexpr, reader(p))
        out.append(
            f"  | (_, .enter _ {p} _, {'env' if 'env ' in r else '_'}, _) => {r}"
        )
    out += ["  | _ => 0", "", f"end {namespace}", ""]
    return "\n".join(out)


def main():
    stage0, sol, inferred, function, out_path, namespace, module = sys.argv[1:8]
    texts = []
    for path in (stage0, sol, inferred):
        with open(path) as f:
            texts.append(f.read())
    text = generate(*texts, function, os.path.basename(stage0), namespace, module)
    with open(out_path, "w") as f:
        f.write(text)


if __name__ == "__main__":
    try:
        main()
    except Unsupported as e:
        sys.exit(f"inferred.py: unsupported: {e}")
