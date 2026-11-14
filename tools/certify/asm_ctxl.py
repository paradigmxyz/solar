"""Certificates of the assembly checker with contexts and gas (checkXL), inferred (untrusted); used by gen_ctx.py and gen_ctxgas.py.

infer_ctx(program_text, code) computes the AssembleCtx.CtxCert that
SolidCore.Refinement.Solar.AssembleCtx.checkX / checkXB need for an exported EVM IR program
whose internal functions are entered at several stack heights: one context per distinct
(entry height, entry types, entry labels) a flow analysis from block 0 reaches, each with the
context every successor enters in. It mirrors AssembleCutSim.blockOkC for heights and types and
AssembleCtxTrack.exits for labels. Nothing here is trusted: the kernel's evaluation of the
checker re-derives every claim.
"""

import asm_cert

DYN_TOP = asm_cert.DYN_TOP


def _swap(xs, i, fill):
    size = max(len(xs), i + 2)
    at = lambda j: xs[j] if j < len(xs) else fill
    return [at(i + 1) if j == 0 else at(0) if j == i + 1 else at(j) for j in range(size)]


def _need(op):
    if op in asm_cert.STACK2:
        return 2
    if op in ("iszero", "not"):
        return 1
    if op == "calldatasize":
        return 0
    return None


# Effects that leave no result (AssembleCtxTrack.pushesE).
NOPUSH = {"mstore", "mstore8", "sstore", "tstore", "calldatacopy", "mcopy", "codecopy",
          "returndatacopy"}


def run_ctx(blocks, turned, b, h, ty, labs):
    """The run of block b from the context (h, ty, labs): its exits (target, h, ty, labs), the
    labels it pushes and whether it reads CALLER/KECCAK, the environment, or calls."""
    n = len(blocks)
    instrs = blocks[b]
    ty, labs = list(ty), list(labs)
    exits, pushed = [], []

    def pad(x, k):
        return x + [None] * (k - len(x)) if len(x) < k else x

    def need(k):
        if h < k:
            raise SystemExit(f"error: block {b} needs {k} words at height {h}")

    for k, i in enumerate(instrs):
        tag = i[0]
        last = k == len(instrs) - 1
        if b in turned and k == len(instrs) - 2:
            need(1)
            exits.append((i[1], h - 1, ty[1:], labs[1:]))
            exits.append((instrs[-1][1], h - 1, ty[1:], labs[1:]))
            return exits, pushed
        if last and ((tag == "jump" and i[1] == b + 1) or (tag == "stop" and b + 1 == n)):
            if tag != "stop":
                exits.append((b + 1, h, ty, labs))
            return exits, pushed
        if tag == "push":
            h, ty, labs = h + 1, [False] + ty, [None] + labs
        elif tag == "dup":
            need(i[1] + 1)
            h, ty, labs = h + 1, [ty[i[1]]] + ty, [pad(labs, i[1] + 1)[i[1]]] + labs
        elif tag == "swap":
            need(i[1] + 2)
            ty, labs = _swap(ty, i[1], None), _swap(pad(labs, h), i[1], None)
        elif tag == "stack" and i[1] == "pop":
            need(1)
            h, ty, labs = h - 1, ty[1:], labs[1:]
        elif tag == "stack" and i[1] == "returndatasize":
            h, ty, labs = h + 1, [False] + ty, [None] + labs
        elif tag == "stack":
            nd = _need(i[1])
            if nd is None:
                raise SystemExit(f"error: block {b}: stack op {i[1]} outside the fragment")
            need(nd)
            h, ty, labs = h - nd + 1, [False] + ty[nd:], [None] + labs[nd:]
        elif tag == "load":
            need(1)
            ty, labs = [False] + ty[1:], [None] + labs[1:]
        elif tag == "store":
            need(2)
            h, ty, labs = h - 2, ty[2:], labs[2:]
        elif tag == "mem":
            raise SystemExit(f"error: block {b}: memory op {i[1]} outside the fragment")
        elif tag in ("effect", "env", "log", "other"):
            if tag == "log":
                name, arity = "log", i[2]
            elif tag == "other":
                if i[1].replace(" ", "") == ".effect(.call.staticcall)6":
                    name, arity = "staticcall", 6
                else:
                    raise SystemExit(f"error: block {b}: {i[1]} outside the fragment")
            elif tag == "env":
                name, arity = "env", i[2]
            else:
                name, arity = i[1], i[2]
            need(arity)
            if name in NOPUSH or name == "log":
                h, ty, labs = h - arity, ty[arity:], labs[arity:]
            else:
                h, ty, labs = h - arity + 1, [False] + ty[arity:], [None] + labs[arity:]
        elif tag == "jump":
            exits.append((i[1], h, ty, labs))
            return exits, pushed
        elif tag == "jumpi":
            need(1)
            h, ty, labs = h - 1, ty[1:], labs[1:]
            exits.append((i[1], h, ty, labs))
        elif tag == "pushLabel":
            pushed.append(i[1])
            h, ty, labs = h + 1, [True] + ty, [[i[1]]] + labs
        elif tag == "jumpDyn":
            need(1)
            ts = labs[0] if labs else None
            if ts is None:
                raise SystemExit(f"error: block {b}: a dynamic jump to an unknown word")
            for t in ts:
                exits.append((t, h - 1, ty[1:], labs[1:]))
            return exits, pushed
        elif tag in ("stop", "ret", "revert"):
            return exits, pushed
        else:
            raise SystemExit(f"error: block {b}: {i} outside the fragment")
    exits.append((b + 1, h, ty, labs))
    return exits, pushed


def _norm(ty, labs, h):
    ty = (list(ty) + [None] * h)[:h]
    labs = (list(labs) + [None] * h)[:h]
    while labs and labs[-1] is None:
        labs.pop()
    return tuple(ty), tuple(tuple(sorted(l)) if l is not None else None for l in labs)


def _merge(a, b):
    need = min(a[0], b[0])
    mx = max(a[1], b[1])
    ty = tuple(a[2][i] if a[2][i] == b[2][i] else None for i in range(need))
    return (need, mx, ty, a[3])


def infer_ctx(program_text, code):
    blocks = asm_cert.parse_program(program_text)
    n = len(blocks)
    turned, offsets, labelled = asm_cert.layout(blocks, code)
    ctxs = [[] for _ in range(n)]  # per block: list of (need, max, ty, labs)
    ctxs[0].append((0, 0, (), ()))
    succ, pushes = {}, {}
    work = [(0, 0)]
    steps = 0
    while work:
        steps += 1
        if steps > 100000:
            raise SystemExit("error: the contexts do not settle")
        b, j = work.pop()
        need, mx, ty, labs = ctxs[b][j]
        exits, pushed = run_ctx(blocks, turned, b, need, list(ty), [list(l) if l else None for l in labs])
        pushes[(b, j)] = pushed
        m = {}
        for t, th, tty, tlabs in exits:
            if t >= n:
                continue
            nty, nlabs = _norm(tty, tlabs, th)
            new = (th, th + (mx - need), nty, nlabs)
            if t == b:
                old = ctxs[b][j]
                if _merge(old, new) != old:
                    raise SystemExit(f"error: block {b} jumps to itself in another context")
                continue
            idx = next((k for k, x in enumerate(ctxs[t]) if x[3] == nlabs), None)
            if idx is None:
                ctxs[t].append(new)
                idx = len(ctxs[t]) - 1
                work.append((t, idx))
            else:
                merged = _merge(ctxs[t][idx], new)
                if merged != ctxs[t][idx]:
                    ctxs[t][idx] = merged
                    work.append((t, idx))
            if t in m and m[t] != idx:
                raise SystemExit(f"error: block {b} enters block {t} in two contexts")
            m[t] = idx
        succ[(b, j)] = m
    costs = []
    for b in range(n):
        items = 0
        instrs = blocks[b]
        for k, i in enumerate(instrs):
            if b in turned and k == len(instrs) - 2:
                items += 3
                break
            items += 2 if i[0] in ("jump", "jumpi") else 1
        costs.append((1 if b in labelled else 0) + items)
    rets = sorted({t for bl in blocks for i in bl if i[0] == "pushLabel" for t in [i[1]]})
    flat = [i for bl in blocks for i in bl]
    env = any(i[0] == "effect" and i[1] in ("caller", "keccak256") for i in flat)
    reads = any(i[0] == "env" for i in flat)
    calls = any(i[0] == "other" and "staticcall" in i[1] for i in flat) or any(
        i[0] == "stack" and i[1] == "returndatasize" for i in flat) or any(
        i[0] == "effect" and i[1] in ("returndatasize", "returndatacopy") for i in flat)
    out = []
    for b in range(n):
        row = []
        for j, (h, hmax, ty, labs) in enumerate(ctxs[b]):
            r = set(pushes.get((b, j), []))
            for l in labs:
                if l is not None:
                    r |= set(l)
            row.append({"need": h, "max": hmax, "types": list(ty), "labs": list(labs),
                        "succ": sorted(succ.get((b, j), {}).items()), "rets": sorted(r)})
        out.append(row)
    return {"offsets": offsets, "turned": turned, "costs": costs, "rets": rets, "env": env,
            "reads": reads, "calls": calls, "ctxs": out}


def _ty(t):
    return "none" if t is None else ("some true" if t else "some false")


def _lab(l):
    return "none" if l is None else "some [" + ", ".join(str(x) for x in l) + "]"


def lean_ctx_cert(c):
    base = ("{ offsets := [" + ", ".join(hex(o) for o in c["offsets"]) + "],\n"
            "      needs := [], top := 2 ^ 140, turned := " + str(c["turned"]) + ",\n"
            "      costs := " + str(c["costs"]) + ",\n"
            "      rets := " + str(c["rets"]) + ", env := " + ("true" if c["env"] else "false")
            + ", reads := " + ("true" if c["reads"] else "false")
            + ", calls := " + ("true" if c["calls"] else "false") + " }")
    rows = []
    for row in c["ctxs"]:
        items = []
        for x in row:
            items.append("{ need := %d, max := %d, types := [%s], labs := [%s], succ := [%s], rets := %s }" % (
                x["need"], x["max"], ", ".join(_ty(t) for t in x["types"]),
                ", ".join(_lab(l) for l in x["labs"]),
                ", ".join("(%d, %d)" % (t, j) for t, j in x["succ"]), str(x["rets"])))
        rows.append("[" + ",\n       ".join(items) + "]")
    return "{ base :=\n    " + base + ",\n    ctxs := [\n    " + ",\n    ".join(rows) + "] }"
