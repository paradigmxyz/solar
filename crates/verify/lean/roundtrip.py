"""Round trip of solar-verify --emit-lean: SMT-LIB and Lean agree on random models.

    python3 roundtrip.py DIR [--seed N] [--obligations N] [--terms N]

DIR is an --emit-lean output. For a sample of its obligations, draws a random finite model of the
script's symbols, asks Z3 to simplify each term of the query (assertions from the script, the
guard and the goal) under that model, and checks in the Lean kernel that the exported term
evaluates to the same Bool under the same model (`decide +kernel` on `Term.evalBool`). Terms Z3
does not reduce to a literal (quantifiers, division by zero, array equality) are skipped. Then
it checks that a wrong expectation fails by a mismatch, not by getting stuck.

Run from crates/verify/lean after `lake build`, with z3 on PATH (or SOLAR_VERIFY_Z3).
"""

import argparse
import json
import os
import pathlib
import random
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
Z3 = os.environ.get("SOLAR_VERIFY_Z3", "z3")


def tokens(text):
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if c.isspace():
            i += 1
        elif c == ";":
            j = text.find("\n", i)
            i = n if j < 0 else j
        elif c in "()":
            yield c
            i += 1
        elif c == "|":
            j = text.index("|", i + 1)
            yield text[i : j + 1]
            i = j + 1
        else:
            j = i
            while j < n and not text[j].isspace() and text[j] not in "()":
                j += 1
            yield text[i:j]
            i = j


def parse(text):
    stack = [[]]
    for t in tokens(text):
        if t == "(":
            stack.append([])
        elif t == ")":
            x = stack.pop()
            stack[-1].append(x)
        else:
            stack[-1].append(t)
    return stack[0]


def show(e):
    return e if isinstance(e, str) else "(" + " ".join(show(x) for x in e) + ")"


class Sorts:
    """Sorts as tuples: ("Int",), ("Bool",), ("BV", w), ("Arr", I, E), ("DT", name, ctor, fields)."""

    def __init__(self):
        self.datatypes = {}
        self.selectors = {}

    def read(self, s):
        if s == "Int":
            return ("Int",)
        if s == "Bool":
            return ("Bool",)
        if isinstance(s, str):
            ctor, fields = self.datatypes[s]
            return ("DT", s, ctor, tuple(self.read(f) for f in fields))
        if s[0] == "_" and s[1] == "BitVec":
            return ("BV", int(s[2]))
        if s[0] == "Array":
            return ("Arr", self.read(s[1]), self.read(s[2]))
        raise ValueError(f"sort {show(s)}")


def classical(e, syms, sorts, bound=()):
    """Whether evaluating e needs classical logic in Lean: a quantifier, or equality of arrays.
    Returns (needs, sort) with sort None when unknown."""
    if isinstance(e, str):
        if e in bound:
            return False, None
        if e in syms:
            return False, syms[e]
        if e in ("true", "false"):
            return False, ("Bool",)
        return False, ("Int",) if e.lstrip("-").isdigit() else None
    head = e[0]
    if head in ("forall", "exists"):
        return True, ("Bool",)
    if head == "!":
        return classical(e[1], syms, sorts, bound)
    if isinstance(head, list):
        if head[0] == "as":
            return classical(e[1], syms, sorts, bound)[0], sorts.read(head[2])
        return any(classical(x, syms, sorts, bound)[0] for x in e[1:]), None
    if head == "_":
        return False, None
    parts = [classical(x, syms, sorts, bound) for x in e[1:]]
    needs = any(p[0] for p in parts)
    if head in ("=", "distinct") and any(
        p[1] is None or has_array(p[1]) for p in parts
    ):
        needs = True
    if head == "select" and parts and parts[0][1] and parts[0][1][0] == "Arr":
        return needs, parts[0][1][2]
    if head == "store" and parts:
        return needs, parts[0][1]
    if head == "ite" and len(parts) == 3:
        return needs, parts[1][1] or parts[2][1]
    if head in syms:
        return needs, syms[head]
    if head in ("+", "-", "*", "div", "mod", "abs", "bv2nat", "bv2int"):
        return needs, ("Int",)
    if head in ("not", "and", "or", "xor", "=>", "=", "distinct", "<", "<=", ">", ">="):
        return needs, ("Bool",)
    if head.startswith("bv") and head[2:] in (
        "ult",
        "ule",
        "ugt",
        "uge",
        "slt",
        "sle",
        "sgt",
        "sge",
    ):
        return needs, ("Bool",)
    if head.startswith("bv") or head == "concat":
        return needs, ("BV", 0)
    for name, (ctor, fields) in sorts.datatypes.items():
        if head == ctor:
            return needs, sorts.read(name)
        for sel, srt in fields_named(sorts, name):
            if head == sel:
                return needs, sorts.read(srt)
    return needs, None


def has_array(s):
    return s[0] == "Arr" or (s[0] == "DT" and any(has_array(f) for f in s[3]))


def fields_named(sorts, name):
    return sorts.selectors.get(name, [])


def smt_sort(s):
    k = s[0]
    if k in ("Int", "Bool"):
        return k
    if k == "BV":
        return f"(_ BitVec {s[1]})"
    if k == "Arr":
        return f"(Array {smt_sort(s[1])} {smt_sort(s[2])})"
    return s[1]


def lean_sort(s):
    k = s[0]
    if k == "Int":
        return ".int"
    if k == "Bool":
        return ".bool"
    if k == "BV":
        return f"(.bv {s[1]})"
    if k == "Arr":
        return f"(.array {lean_sort(s[1])} {lean_sort(s[2])})"
    return "(.tup [" + ", ".join(lean_sort(f) for f in s[3]) + "])"


def first_order(s):
    return s[0] in ("Int", "Bool", "BV") or (
        s[0] == "DT" and all(first_order(f) for f in s[3])
    )


INTS = [0, 1, 2, 3, 7, 2**160 - 1, 2**255, 2**256 - 1, 2**256, -1, -5]


def draw(rng, s):
    """A random value of sort s, as (smt, lean)."""
    k = s[0]
    if k == "Int":
        n = rng.choice(INTS) if rng.random() < 0.5 else rng.randrange(-(2**64), 2**257)
        if rng.random() < 0.3:
            n = rng.randrange(0, 10)
        smt = str(n) if n >= 0 else f"(- {-n})"
        return smt, f"(SolarVerify.Model.int ({n}))"
    if k == "Bool":
        b = rng.random() < 0.5
        return (
            "true" if b else "false"
        ), f"(SolarVerify.Model.bool {'true' if b else 'false'})"
    if k == "BV":
        w = s[1]
        n = rng.choice([0, 1, 2**w - 1, rng.randrange(0, 2**w)])
        return f"(_ bv{n} {w})", f"(SolarVerify.Model.bv {w} {n})"
    if k == "Arr":
        d_smt, d_lean = draw(rng, s[2])
        smt = f"((as const {smt_sort(s)}) {d_smt})"
        entries = []
        if first_order(s[1]):
            for _ in range(rng.randrange(0, 3)):
                ks, kl = draw(rng, s[1])
                vs, vl = draw(rng, s[2])
                entries.append((kl, vl))
                smt = f"(store {smt} {ks} {vs})"
        # SMT-LIB's store puts the last entry on top: the Lean table takes the first match.
        entries.reverse()
        rows = ", ".join(f"({kk}, {vv})" for kk, vv in entries)
        return (
            smt,
            f"(SolarVerify.Model.array {lean_sort(s[1])} {lean_sort(s[2])} {d_lean} [{rows}])",
        )
    fields = [draw(rng, f) for f in s[3]]
    smt = f"({s[2]} {' '.join(f[0] for f in fields)})" if fields else s[2]
    return smt, "(SolarVerify.Model.tup [" + ", ".join(f[1] for f in fields) + "])"


def model(rng, decls):
    """SMT-LIB definitions and Lean functions for every declared symbol."""
    smt, lean = [], []
    for name, args, ret in decls:
        d_smt, d_lean = draw(rng, ret)
        rows = []
        if args and all(first_order(a) for a in args):
            for _ in range(rng.randrange(0, 3)):
                rows.append(([draw(rng, a) for a in args], draw(rng, ret)))
        params = " ".join(f"(x{i} {smt_sort(a)})" for i, a in enumerate(args))
        body = d_smt
        for vals, (r_smt, _) in reversed(rows):
            cond = " ".join(f"(= x{i} {v[0]})" for i, v in enumerate(vals))
            body = f"(ite (and true {cond}) {r_smt} {body})"
        smt.append(f"(define-fun {name} ({params}) {smt_sort(ret)} {body})")
        lean_rows = ", ".join(
            "([" + ", ".join(v[1] for v in vals) + f"], {r[1]})" for vals, r in rows
        )
        lean.append(f"SolarVerify.Model.table {d_lean} [{lean_rows}]")
    return smt, lean


def z3_values(script, terms):
    text = "\n".join(script) + "\n" + "".join(f"(simplify {t})\n" for t in terms)
    out = subprocess.run(
        [Z3, "-in", "-smt2"],
        check=False,
        input=text,
        capture_output=True,
        text=True,
        timeout=120,
    ).stdout
    answers = parse(out)
    if len(answers) != len(terms):
        raise RuntimeError(
            f"z3 answered {len(answers)} of {len(terms)} terms:\n{out[:2000]}"
        )
    return [a if a in ("true", "false") else None for a in answers]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("dir", type=pathlib.Path)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--obligations", type=int, default=4, help="per unit")
    ap.add_argument("--terms", type=int, default=12, help="assertions per obligation")
    a = ap.parse_args()
    rng = random.Random(a.seed)
    manifest = json.loads((a.dir / "manifest.json").read_text())
    if manifest["version"] != 1:
        sys.exit(f"format version {manifest['version']} is not 1")
    checked = skipped = classical_skipped = 0
    tests = []
    for unit in manifest["units"]:
        if "obligations" not in unit:
            continue
        scripts = []
        for sc in unit["scripts"]:
            cmds = [
                parse(line)[0]
                for line in (a.dir / sc["smt"]).read_text().splitlines()
                if line.strip()
            ]
            scripts.append(cmds)
        obs = [o for o in unit["obligations"]]
        rng.shuffle(obs)
        lean_examples = []
        for ob in obs[: a.obligations]:
            sorts = Sorts()
            decls, defs, asserts, keep = [], [], [], []
            lean_index = 0
            for c in scripts[ob["script"]]:
                head = c[0]
                if head == "declare-datatypes":
                    for (name, _), (ctor,) in zip(c[1], c[2]):
                        sorts.datatypes[name] = (ctor[0], [f[1] for f in ctor[1:]])
                        sorts.selectors[name] = [(f[0], f[1]) for f in ctor[1:]]
                    keep.append(show(c))
                elif head == "declare-const":
                    decls.append((c[1], [], sorts.read(c[2])))
                elif head == "declare-fun":
                    decls.append(
                        (c[1], [sorts.read(x) for x in c[2]], sorts.read(c[3]))
                    )
                elif head == "define-fun":
                    decls.append((c[1], None, None))
                    defs.append(show(c))
                    lean_index += 1
                elif head == "assert":
                    asserts.append((lean_index, show(c[1])))
                    lean_index += 1
            n_asserts = ob["asserts"]
            in_scope = [x for x in asserts if x[0] < n_asserts]
            sample = rng.sample(in_scope, min(a.terms, len(in_scope)))
            terms = [
                (f"(({{ns}}.script{ob['script']}).asserts.getD {i} default)", t)
                for i, t in sample
            ]
            terms += [(f"{{ns}}.ob{ob['lean'].rsplit('.ob', 1)[1]}.guard", ob["guard"])]
            terms += [(f"{{ns}}.ob{ob['lean'].rsplit('.ob', 1)[1]}.goal", ob["goal"])]
            # Defined symbols keep their definitions; the rest get random values.
            free = [(n, ar, r) for n, ar, r in decls if ar is not None]
            m_smt, m_lean = model(rng, free)
            lean_fns, it = [], iter(m_lean)
            for n, ar, r in decls:
                lean_fns.append(
                    next(it) if ar is not None else "fun _ => SolarVerify.Model.int 0"
                )
            # The model does not interpret defined symbols as their definitions: skip their terms.
            defined = [n for n, ar, _ in decls if ar is None]
            terms = [(lt, t) for lt, t in terms if not set(tokens(t)) & set(defined)]
            # Quantifiers and equality of arrays evaluate classically in Lean: skip them.
            syms = {n: r for n, ar, r in decls if ar is not None}
            kept = [
                (lt, t) for lt, t in terms if not classical(parse(t)[0], syms, sorts)[0]
            ]
            classical_skipped += len(terms) - len(kept)
            terms = kept
            values = z3_values(keep + m_smt + defs, [t for _, t in terms])
            env = f"env_{len(tests)}_{len(lean_examples)}"
            body = [
                f"noncomputable def {env} : Env := SolarVerify.Model.env [\n  "
                + ",\n  ".join(lean_fns)
                + "]"
            ]
            for (lean_t, _), v in zip(terms, values):
                if v is None:
                    skipped += 1
                    continue
                checked += 1
                body.append(
                    f"example : Term.evalBool {env} ({{ns}}.script{ob['script']}).decls {lean_t} = some {v} := by decide +kernel"
                )
            lean_examples.append("\n".join(body))
        if lean_examples:
            tests.append((unit, lean_examples))
    if not tests:
        sys.exit("no obligations to check")
    failures = 0
    with tempfile.TemporaryDirectory(delete="ROUNDTRIP_KEEP" not in os.environ) as tmp:
        for i, (unit, examples) in enumerate(tests):
            ns = unit["namespace"]
            src = (a.dir / unit["module"]).read_text()
            test = (
                src
                + "\nopen SolarVerify.Smt SolarVerify.Vc\n\n"
                + "\n\n".join(examples).replace("{ns}", ns)
                + "\n"
            )
            path = pathlib.Path(tmp) / f"T{i}.lean"
            path.write_text(test)
            r = subprocess.run(
                ["lake", "env", "lean", "-M", "3000", str(path)],
                check=False,
                cwd=HERE,
                capture_output=True,
                text=True,
                stdin=subprocess.DEVNULL,
            )
            if r.returncode != 0:
                failures += 1
                print(f"MISMATCH in {unit['name']}:\n{(r.stdout + r.stderr)[:3000]}")
        # A wrong expectation fails by a mismatch the kernel computes.
        unit, examples = tests[0]
        flipped = None
        for ex in examples:
            for line in ex.splitlines():
                if line.startswith("example") and line.endswith(
                    "= some true := by decide +kernel"
                ):
                    flipped = (
                        ex.split("\nexample", 1)[0],
                        line.replace("= some true :=", "= some false :="),
                    )
                    break
            if flipped:
                break
        if flipped is None:
            sys.exit("no true term to flip for the negative check")
        src = (a.dir / unit["module"]).read_text()
        neg = (
            src
            + "\nopen SolarVerify.Smt SolarVerify.Vc\n\n"
            + "\n".join(flipped).replace("{ns}", unit["namespace"])
            + "\n"
        )
        path = pathlib.Path(tmp) / "Neg.lean"
        path.write_text(neg)
        r = subprocess.run(
            ["lake", "env", "lean", "-M", "3000", str(path)],
            check=False,
            cwd=HERE,
            capture_output=True,
            text=True,
            stdin=subprocess.DEVNULL,
        )
        out = r.stdout + r.stderr
        if (
            r.returncode == 0
            or "stuck" in out
            or "maximum recursion" in out
            or "heartbeats" in out
        ):
            failures += 1
            print(f"the negative check did not fail by a mismatch:\n{out[:2000]}")
    print(
        f"round trip: {checked} terms agree, {skipped} not reduced by z3, "
        f"{classical_skipped} classical, {len(tests)} units, {failures} failures"
    )
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
