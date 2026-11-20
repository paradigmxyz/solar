#!/usr/bin/env python3
"""The fixed-fuel gas stage: a resource obligation for a contract with one loop.

  tools/certify/fixed_fuel.py SOURCE.sol [--contract C] [--solar BIN] [--solc BIN] [--out FILE]

The loop checker's bound is `gasBoundL c n = 22100 * (n * bmax) + memory` at a block fuel `n`
where the final EVM IR ends. For straight-line code `n` is a fixed stage fuel. For a loop it is
`E + k * B + X` after `k` iterations (Verify/Gas/FixedFuel/Run.lean), so the stage needs a bound
`M` on `k` over every call. This stage computes the obligation's numbers, never a proof:

* `M` from the source loop's shape (`source_loops`): a counted `for` loop whose counter starts
  at a literal, steps by one and is compared with a literal (`M` is the literal distance) or
  with a word of the call or the state (`M` is the word type's range: a call can make it that
  large). Anything else is outside the shape and unsupported.
* `B`, `E` and `X` from the final EVM IR's control flow (`cfg_loop`): the loop is the one
  strongly connected component, with one entry block, its header. `B` is the longest path from
  the header back to it, `X` the longest from the header to an end, and `E` the rest of the
  longest path from the entry, so the fuel is `F = E + M * B + X` (`fixedFuel`).
* `bmax` and the memory term from the loop checker's certificate (`asm_cert.infer`), as
  `generic.gas_need` computes them.

The obligation is `G = 22100 * (F * bmax) + memory = M * perIter + exit`, where
`perIter = 22100 * (B * bmax)` and `exit = 22100 * ((E + X) * bmax) + memory`
(`Verify.Gas.FixedFuel.gasBoundL_eq`). Its verdict is `verdictOf`'s:

  0  BOUNDED G: G is below 2^64; the obligation is Shapes.Resources at G, with the loop's bridge
  1  REJECTED: resource: out of gas: G is at least 2^64; no Resources holds at it
  3  BLOCKED: <reason>: the loop is outside the counted shape, or the control flow is not one
     single-entry loop; nothing is claimed

A revert is none of these: a run that reverts is a result, which the bound covers. Nothing here
selects by contract, file or hash.
"""

import argparse
import json
import pathlib
import re
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import asm_cert
import generic

GAS_LIMIT = 2**64
ITEM = 22100
STAGE_RE = re.compile(r"^-- solar-lean-stage: kind=(\w+) index=(\d+)", re.MULTILINE)


class Unsupported(Exception):
    """The loop or its control flow is outside what the fixed-fuel stage bounds."""


# --- the source loop's shape -----------------------------------------------------------------


def uint_bits(type_string):
    m = re.fullmatch(r"uint(\d*)", (type_string or "").strip())
    return int(m.group(1) or 256) if m else None


def literal_value(node):
    if (
        node.get("nodeType") != "Literal"
        or node.get("kind") != "number"
        or node.get("subdenomination")
    ):
        return None
    value = node.get("value")
    if not isinstance(value, str):
        return None
    try:
        return int(value.replace("_", ""), 0)
    except ValueError:
        return None


def type_of(node):
    return node.get("typeDescriptions", {}).get("typeString", "")


def is_var(node, vid):
    return (
        node.get("nodeType") == "Identifier"
        and node.get("referencedDeclaration") == vid
    )


def counter_of(init):
    """The counter's declaration id, its bits and its literal start."""
    if init is None:
        raise Unsupported("the loop has no initialization of its counter")
    if init.get("nodeType") == "VariableDeclarationStatement":
        decls = init.get("declarations") or []
        if len(decls) != 1 or decls[0] is None:
            raise Unsupported("the initialization declares other than one counter")
        d, v = decls[0], init.get("initialValue")
        vid, bits = d["id"], uint_bits(type_of(d))
    elif (
        init.get("nodeType") == "ExpressionStatement"
        and init["expression"].get("nodeType") == "Assignment"
        and init["expression"].get("operator") == "="
        and init["expression"]["leftHandSide"].get("nodeType") == "Identifier"
    ):
        lhs = init["expression"]["leftHandSide"]
        v, vid, bits = (
            init["expression"]["rightHandSide"],
            lhs["referencedDeclaration"],
            uint_bits(type_of(lhs)),
        )
    else:
        raise Unsupported("the initialization is not a counter set to a literal")
    start = literal_value(v) if v is not None else None
    if bits is None or start is None:
        raise Unsupported(
            "the counter is not an unsigned integer that starts at a literal"
        )
    return vid, bits, start


def steps_by_one(expr, vid):
    e = (expr or {}).get("expression", {})
    if (
        e.get("nodeType") == "UnaryOperation"
        and e.get("operator") == "++"
        and is_var(e.get("subExpression", {}), vid)
    ):
        return True
    return (
        e.get("nodeType") == "Assignment"
        and e.get("operator") == "+="
        and is_var(e["leftHandSide"], vid)
        and literal_value(e["rightHandSide"]) == 1
    )


def bound_of(cond, vid, bits, start, checked):
    """The iteration bound M of the condition, and whether it is a literal distance."""
    if cond is None or cond.get("nodeType") != "BinaryOperation":
        raise Unsupported("the condition is not a comparison of the counter")
    op, lhs, rhs = cond["operator"], cond["leftExpression"], cond["rightExpression"]
    if is_var(rhs, vid) and op in (">", ">="):
        op, lhs, rhs = {">": "<", ">=": "<="}[op], rhs, lhs
    if not is_var(lhs, vid) or op not in ("<", "<=", "!="):
        raise Unsupported(
            "the condition is not the counter compared with a limit (<, <=, !=)"
        )
    if op != "<" and not checked:
        raise Unsupported(
            "an unchecked counter that the condition does not stop below its limit can wrap"
        )
    wrap = 2**bits - start  # checked: the increment past the type's range reverts
    k = literal_value(rhs)
    if k is not None:
        m = {
            "<": k - start,
            "<=": k - start + 1,
            "!=": k - start if k >= start else wrap,
        }[op]
        return max(0, min(m, wrap)), True
    rb = uint_bits(type_of(rhs))
    if rb is None:
        raise Unsupported("the limit is neither a literal nor an unsigned word")
    top = 2**rb - 1
    m = {"<": top - start, "<=": top - start + 1, "!=": wrap}[op]
    return max(0, min(m, wrap)), False


def body_ok(body, vid):
    for n in generic.walk(body):
        t = n.get("nodeType")
        if t in ("ForStatement", "WhileStatement", "DoWhileStatement"):
            raise Unsupported("a nested loop")
        if t in ("Break", "Continue"):
            raise Unsupported("break or continue in the body")
        if t == "InlineAssembly":
            raise Unsupported("assembly in the body")
        if t == "Assignment" and is_var(n["leftHandSide"], vid):
            raise Unsupported("the body writes the counter")
        if (
            t == "UnaryOperation"
            and n.get("operator") in ("++", "--", "delete")
            and is_var(n.get("subExpression", {}), vid)
        ):
            raise Unsupported("the body writes the counter")


def source_loops(ast, contract):
    """Each loop of the contract: its function, its bound M and whether that is a literal distance."""
    defs = [
        d
        for d in ast.get("nodes", [])
        if d.get("nodeType") == "ContractDefinition"
        and (contract is None or d["name"] == contract)
    ]
    out = []
    for d in defs:
        for fn in generic.walk(d):
            if fn.get("nodeType") != "FunctionDefinition" or not fn.get("body"):
                continue
            stack = [(fn["body"], True)]
            while stack:
                n, checked = stack.pop()
                t = n.get("nodeType")
                if t in ("WhileStatement", "DoWhileStatement"):
                    raise Unsupported(
                        f"{fn.get('name') or fn.get('kind')}: a while loop has no counted shape"
                    )
                if t == "ForStatement":
                    vid, bits, start = counter_of(n.get("initializationExpression"))
                    if not steps_by_one(n.get("loopExpression"), vid):
                        raise Unsupported(
                            f"{fn.get('name')}: the update is not the counter stepped by one"
                        )
                    body_ok(n.get("body", {}), vid)
                    m, literal = bound_of(n.get("condition"), vid, bits, start, checked)
                    out.append(
                        {
                            "function": fn.get("name") or fn.get("kind"),
                            "src": n.get("src"),
                            "M": m,
                            "limit": "literal" if literal else "word",
                        }
                    )
                    continue
                for v in n.values():
                    if isinstance(v, dict) and "nodeType" in v:
                        stack.append((v, checked and t != "UncheckedBlock"))
                    elif isinstance(v, list):
                        stack.extend(
                            (x, checked and t != "UncheckedBlock")
                            for x in v
                            if isinstance(x, dict) and "nodeType" in x
                        )
    return out


# --- the final EVM IR's control flow ---------------------------------------------------------


def successors(blocks):
    """Each block's successors; the number past the last block is the one end node."""
    n = len(blocks)
    rets = sorted({i[1] for bl in blocks for i in bl if i[0] == "pushLabel"})
    succ = []
    for b, instrs in enumerate(blocks):
        out, falls = set(), True
        for i in instrs:
            k = i[0]
            if k == "jump":
                out.add(i[1])
                falls = False
                break
            if k == "jumpi":
                out.add(i[1])
            elif k == "jumpDyn":
                out.update(rets)
                falls = False
                break
            elif k in ("stop", "ret", "revert"):
                falls = False
                break
            elif k == "other":
                s = i[1]
                if s == ".invalid":
                    falls = False
                    break
                if s == ".jumpiDyn":
                    out.update(rets)
                elif s.startswith(".jumpTable"):
                    out.update(int(x, 0) for x in re.findall(r"0x[0-9a-fA-F]+|\d+", s))
                    falls = False
                    break
                else:
                    raise Unsupported(
                        f"bb{b}: an instruction the stage does not follow: {s[:60]}"
                    )
        if falls:
            out.add(b + 1)
        succ.append(sorted({min(t, n) for t in out}))
    succ.append([])
    return succ


def sccs(succ):
    index, low, on, stack, comps, counter = {}, {}, set(), [], [], [0]

    def visit(v):
        work = [(v, iter(succ[v]))]
        index[v] = low[v] = counter[0]
        counter[0] += 1
        stack.append(v)
        on.add(v)
        while work:
            u, it = work[-1]
            w = next(it, None)
            if w is None:
                work.pop()
                if work:
                    low[work[-1][0]] = min(low[work[-1][0]], low[u])
                if low[u] == index[u]:
                    comp = set()
                    while True:
                        x = stack.pop()
                        on.discard(x)
                        comp.add(x)
                        if x == u:
                            break
                    comps.append(comp)
            elif w not in index:
                index[w] = low[w] = counter[0]
                counter[0] += 1
                stack.append(w)
                on.add(w)
                work.append((w, iter(succ[w])))
            elif w in on:
                low[u] = min(low[u], index[w])

    for v in range(len(succ)):
        if v not in index:
            visit(v)
    return comps


def longest(succ, src, weight, nodes=None):
    """Longest path weights from src in an acyclic graph (None on a cycle), over nodes."""
    order, state = [], {}

    def dfs(v):
        stack = [(v, iter(succ[v]))]
        state[v] = 1
        while stack:
            u, it = stack[-1]
            w = next(it, None)
            if w is None:
                stack.pop()
                state[u] = 2
                order.append(u)
            elif nodes is not None and w not in nodes:
                continue
            elif state.get(w) == 1:
                raise Unsupported(
                    "a cycle besides the loop's back edges (a nested or irreducible loop)"
                )
            elif w not in state:
                state[w] = 1
                stack.append((w, iter(succ[w])))

    dfs(src)
    best = {}
    for v in reversed(order):
        base = best.get(v)
        if v == src:
            base = weight(v)
        if base is None:
            continue
        best[v] = base
        for w in succ[v]:
            if (nodes is None or w in nodes) and w in state:
                best[w] = max(best.get(w, 0), base + weight(w))
    return best


def cfg_loop(blocks):
    """The one loop of the reachable control flow: header, B, and the graph without back edges."""
    succ = successors(blocks)
    reach, todo = {0}, [0]
    while todo:
        for w in succ[todo.pop()]:
            if w not in reach:
                reach.add(w)
                todo.append(w)
    loops = [
        c
        for c in sccs(succ)
        if c & reach and (len(c) > 1 or any(v in succ[v] for v in c))
    ]
    if len(loops) != 1:
        raise Unsupported(
            f"the final EVM IR has {len(loops)} loops; the fixed-fuel stage takes exactly one"
        )
    comp = loops[0]
    heads = sorted(v for v in comp if v == 0 or any(v in succ[u] for u in reach - comp))
    if len(heads) != 1:
        raise Unsupported(
            f"the loop has {len(heads)} entry blocks; the stage takes one header"
        )
    h = heads[0]
    inner = [[w for w in ws if not (w == h and u in comp)] for u, ws in enumerate(succ)]
    latches = [u for u in comp if h in succ[u]]
    body = longest(inner, h, lambda v: 1, nodes=comp)
    B = max(body[u] for u in latches)
    return h, B, inner


def fuel(blocks, M):
    """F = E + M * B + X over the control flow, and its parts."""
    h, B, dag = cfg_loop(blocks)
    w = lambda v: 1 + (M * B if v == h else 0)
    total = max(longest(dag, 0, w).values())
    X = max(longest(dag, h, lambda v: 1).values())
    return {"header": h, "B": B, "X": X, "E": total - M * B - X, "F": total}


# --- the obligation ----------------------------------------------------------------------------


def last_evm(gen):
    best = None
    for f in sorted(gen.glob("*.lean")):
        m = STAGE_RE.search(f.read_text()[:400])
        if m and m.group(1) == "evm" and (best is None or int(m.group(2)) > best[0]):
            best = (int(m.group(2)), f)
    if best is None:
        raise Unsupported("the stage export has no EVM IR stage")
    return best[1]


def obligation(solar, src, contract, ast, tmp):
    """The resource obligation's numbers and verdict for SOURCE's contract (one loop)."""
    loops = source_loops(ast, contract)
    if len(loops) != 1:
        raise Unsupported(
            f"the source has {len(loops)} loops; the fixed-fuel stage takes exactly one"
        )
    M = loops[0]["M"]
    gen = pathlib.Path(tmp) / "gen"
    plain, err = generic.export_stages(solar, src, contract, tmp)
    if plain is None:
        raise Unsupported(
            "the stage exporter rejects the contract: " + (err or "").strip()[:300]
        )
    final = last_evm(gen / contract)
    text = final.read_text()
    outside = asm_cert.outside(text)
    if outside:
        raise Unsupported(
            f"the final EVM IR is outside the loop checker's fragment: {outside}"
        )
    blocks = asm_cert.parse_program(text)
    f = fuel(blocks, M)
    inferred = asm_cert.infer(text, bytes.fromhex(plain))
    bmax = max(inferred["costs"]) if inferred["costs"] else 0
    words = inferred["top"] // 32
    mem = 3 * words + words * words // 512
    per_iter = ITEM * (f["B"] * bmax)
    exit_cost = ITEM * ((f["E"] + f["X"]) * bmax) + mem
    G = ITEM * (f["F"] * bmax) + mem
    assert G == M * per_iter + exit_cost
    return {
        "kind": "fixed-fuel",
        "loop": loops[0],
        "final_evm_ir": final.stem,
        **f,
        "M": M,
        "bmax": bmax,
        "memory": mem,
        "per_iteration_gas": per_iter,
        "exit_gas": exit_cost,
        "G": G,
        "verdict": "bounded" if G < GAS_LIMIT else "out-of-gas",
        "lean": f"Verify.Gas.FixedFuel.verdictOf (some {M}) {per_iter} {exit_cost}",
    }


def contract_of(ast, contract):
    if contract:
        return contract
    names = [
        d["name"]
        for d in ast.get("nodes", [])
        if d.get("nodeType") == "ContractDefinition"
        and d.get("contractKind") == "contract"
        and not d.get("abstract")
    ]
    if len(names) != 1:
        raise Unsupported(f"{len(names)} concrete contracts; name one with --contract")
    return names[0]


def verdict_line(ob):
    if ob["verdict"] == "bounded":
        return (
            0,
            f"BOUNDED {ob['G']}: {ob['M']} iterations of at most {ob['B']} blocks, fuel {ob['F']}",
        )
    return 1, (
        f"REJECTED: resource: out of gas: the fixed bound {ob['G']} is at least 2^64 "
        f"({ob['M']} iterations, {ob['loop']['limit']} limit, of at most {ob['B']} blocks)"
    )


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("source")
    ap.add_argument("--contract")
    ap.add_argument("--solar", default=str(generic.ROOT / "target" / "debug" / "solar"))
    ap.add_argument(
        "--solc",
        default=str(
            pathlib.Path.home() / ".cache/solc" / f"solc-{generic.SOLC_VERSION}"
        ),
    )
    ap.add_argument("--out")
    a = ap.parse_args()
    src = pathlib.Path(a.source).resolve()
    try:
        ast, _ = generic.solc_ast(a.solc, src)
        with tempfile.TemporaryDirectory() as tmp:
            ob = obligation(a.solar, src, contract_of(ast, a.contract), ast, tmp)
        code, line = verdict_line(ob)
    except Unsupported as e:
        ob, code, line = (
            {"kind": "fixed-fuel", "verdict": "unsupported", "why": str(e)},
            3,
            f"BLOCKED: loop: {e}",
        )
    except (generic.Failed, generic.Blocked) as e:
        ob, code, line = (
            {"kind": "fixed-fuel", "verdict": "unsupported", "why": str(e)},
            3,
            f"BLOCKED: {e}",
        )
    if a.out:
        pathlib.Path(a.out).write_text(json.dumps(ob, indent=2) + "\n")
    print(json.dumps(ob, indent=2))
    print(line)
    sys.exit(code)


if __name__ == "__main__":
    main()
