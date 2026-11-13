#!/usr/bin/env python3
"""Cut points and relations for the stages of programs that loop.

certify.py proves a stage of loop-free programs by running both to their ends. A program that
loops cannot be run to its end on every call, so its stages are proved through cut points
(SolarTV.CutPoint): this module picks, for a pair of consecutive stages, configurations that cut
every cycle of both programs and the relation between them, from what the compiler kept across
the pass, and writes the segment obligations that sym_seg proves.

The relations are derived, never written by hand:

- EVM IR to EVM IR: a pass keeps its blocks' numbers (the "-- bbN" the exporter writes before
  each block). The cut points are blocks of the same number in both programs, chosen so that
  every cycle of either program passes through one; the relation at a pair is that the stacks
  are the same words, of the height both programs have at that block, and the state is the
  same. A pass that moves a header's words, or a loop with no block both programs keep, fails
  closed here, naming the pass.

Nothing here is trusted: a wrong cut set makes a segment run out of fuel and a wrong relation
makes a segment's end differ, and either fails the Lean proof by mismatch.
"""

import itertools
import json
import re


class Unsupported(Exception):
    """A stage this procedure cannot relate: the reason names the pass and why."""


# Stack effects of EVM IR instructions: words popped and pushed.
BINARY = {
    "eq",
    "sub",
    "add",
    "mul",
    "and",
    "or",
    "xor",
    "div",
    "sdiv",
    "mod",
    "smod",
    "exp",
    "signextend",
    "lt",
    "gt",
    "slt",
    "sgt",
    "byte",
    "shl",
    "shr",
    "sar",
}
UNARY = {"iszero", "not", "clz"}
TERNARY = {"addmod", "mulmod"}
NULLARY = {"calldatasize", "returndatasize"}
# Effects that leave a word.
RESULT_EFFECTS = {
    "mload",
    "msize",
    "sload",
    "tload",
    "calldataload",
    "calldatasize",
    "callvalue",
    "caller",
    "address",
    "returndatasize",
    "keccak256",
    "gas",
    "create",
    "create2",
}
NO_RESULT_EFFECTS = {
    "mstore",
    "mstore8",
    "sstore",
    "tstore",
    "calldatacopy",
    "mcopy",
    "returndatacopy",
    "codecopy",
}


def split_top(s):
    """Split on commas outside brackets."""
    out, depth, cur = [], 0, []
    for ch in s:
        if ch in "([{⟨":
            depth += 1
        elif ch in ")]}⟩":
            depth -= 1
        if ch == "," and depth == 0:
            out.append("".join(cur).strip())
            cur = []
        else:
            cur.append(ch)
    if "".join(cur).strip():
        out.append("".join(cur).strip())
    return out


def parse_evm(text):
    """The blocks of an exported EVM IR program, in order: (compiler block number, [instr])."""
    body = text[text.index("blocks := [") + len("blocks := [") :]
    parts = re.split(r"^\s*-- bb(\d+)\s*$", body, flags=re.MULTILINE)
    blocks = []
    for i in range(1, len(parts), 2):
        ident = int(parts[i])
        chunk = parts[i + 1].strip()
        chunk = re.sub(r"\]\s*\}\s*(end .*)?$", "", chunk, flags=re.DOTALL).strip()
        chunk = chunk.rstrip(",").strip()
        assert chunk.startswith("[") and chunk.endswith("]"), (
            ident,
            chunk[:80],
            chunk[-80:],
        )
        blocks.append((ident, split_top(chunk[1:-1])))
    return blocks


def effect(instr):
    """(popped, pushed, kind, targets) of one instruction; kind is None, "jump", "jumpi",
    "dyn", "table" or "end"."""
    m = re.fullmatch(r"\.op \(\.stack \((\.push|\.dup|\.swap|\.exchange) .*\)\)", instr)
    if m:
        return {".push": (0, 1), ".dup": (0, 1), ".swap": (0, 0), ".exchange": (0, 0)}[
            m[1]
        ] + (None, [])
    m = re.fullmatch(r"\.op \(\.stack \.(\w+)\)", instr)
    if m:
        op = m[1]
        if op in BINARY:
            return (2, 1, None, [])
        if op in UNARY:
            return (1, 1, None, [])
        if op in TERNARY:
            return (3, 1, None, [])
        if op in NULLARY:
            return (0, 1, None, [])
        if op == "pop":
            return (1, 0, None, [])
    m = re.fullmatch(r"\.op \.(\w+)", instr)
    if m:
        return {
            "load": (1, 1),
            "store": (2, 0),
            "storeByte": (2, 0),
            "size": (0, 1),
            "copy": (3, 0),
            "hash": (2, 1),
        }[m[1]] + (None, [])
    m = re.fullmatch(r"\.effect \(?\.(\w+)(?: [^)]*\))? (\d+)", instr)
    if m:
        kind, arity = m[1], int(m[2])
        if kind in RESULT_EFFECTS or kind in ("env", "call", "immutable"):
            return (arity, 1, None, [])
        if kind in NO_RESULT_EFFECTS or kind == "log":
            return (arity, 0, None, [])
    m = re.fullmatch(r"\.(jump|jumpi|pushLabel) (\d+)", instr)
    if m:
        t = int(m[2])
        return {
            "jump": (0, 0, "jump", [t]),
            "jumpi": (1, 0, "jumpi", [t]),
            "pushLabel": (0, 1, None, []),
        }[m[1]]
    if instr == ".jumpDyn":
        return (1, 0, "dyn", [])
    if instr == ".jumpiDyn":
        return (2, 0, "dyni", [])
    m = re.fullmatch(r"\.jumpTable \[(.*)\]", instr)
    if m:
        return (1, 0, "table", [int(t) for t in m[1].split(",") if t.strip()])
    if instr in (".stop", ".ret", ".revert", ".invalid"):
        return (0, 0, "end", [])
    raise Unsupported(f"an instruction the stack heights do not know: {instr}")


def evm_cfg(blocks):
    """Successors by block index and the static stack height at each block's entry."""
    labels = sorted(
        {
            int(t)
            for _, b in blocks
            for i in b
            for t in re.findall(r"^\.pushLabel (\d+)$", i)
        }
    )
    succ, heights = {}, {0: 0}
    exits = {}
    for idx, (_, instrs) in enumerate(blocks):
        out, end = [], False
        for ins in instrs:
            _, _, kind, targets = effect(ins)
            out += targets
            if kind in ("dyn", "dyni"):
                out += labels
            if kind in ("jump", "dyn", "table", "end"):
                end = True
                break
        if not end:
            out.append(idx + 1)
        succ[idx] = sorted({t for t in out if t < len(blocks)})
    # Heights, forward from block 0; a dynamic jump's successors get no height from it.
    work = [0]
    dynamic = False
    # Blocks entered at several heights (such as a shared revert) get none: they cannot be cut
    # points.
    conflict = set()
    while work:
        idx = work.pop()
        h = heights[idx]
        for ins in blocks[idx][1]:
            pop, push, kind, targets = effect(ins)
            if h < pop:
                raise Unsupported(
                    f"block bb{blocks[idx][0]} underflows its static stack"
                )
            h = h - pop
            for t in targets:
                if heights.setdefault(t, h) != h:
                    conflict.add(t)
                if t not in exits:
                    exits[t] = True
                    work.append(t)
            if kind in ("dyn", "dyni"):
                dynamic = True
            if kind in ("jump", "dyn", "table", "end"):
                break
            h += push
        else:
            t = idx + 1
            if t < len(blocks):
                if heights.setdefault(t, h) != h:
                    conflict.add(t)
                if t not in exits:
                    exits[t] = True
                    work.append(t)
    for t in conflict:
        heights.pop(t, None)
    return succ, heights, dynamic


def cyclic_component(succ, removed):
    """A cycle of the graph without the nodes in removed, as a list of nodes, or None; the
    first node is the target of a back edge (a loop header) when there is one."""
    color, stack = {}, []

    def dfs(n):
        color[n] = 1
        stack.append(n)
        for m in succ.get(n, []):
            if m in removed:
                continue
            if color.get(m) == 1:
                return stack[stack.index(m) :]
            if m not in color:
                c = dfs(m)
                if c:
                    return c
        stack.pop()
        color[n] = 2
        return None

    import sys

    sys.setrecursionlimit(100000)
    for n in sorted(succ):
        if n not in removed and n not in color:
            c = dfs(n)
            if c:
                return c
    return None


def on_cycle(succ):
    """The nodes that lie on some cycle."""
    out = set()
    for n in succ:
        seen, work = set(), list(succ.get(n, []))
        while work:
            m = work.pop()
            if m == n:
                out.add(n)
                break
            if m not in seen:
                seen.add(m)
                work += succ.get(m, [])
    return out


def evm_cuts(pass_name, a_blocks, b_blocks):
    """Cut points of two EVM IR programs by their compiler block numbers: a list of
    (index in a, index in b, height), or [] when neither loops. Blocks on cycles of both programs
    come first, so that a loop the pass rotates is cut at a block both still loop through, and a
    cut point no cycle needs is dropped."""
    a_succ, a_h, a_dyn = evm_cfg(a_blocks)
    b_succ, b_h, b_dyn = evm_cfg(b_blocks)
    a_ids = [i for i, _ in a_blocks]
    b_ids = [i for i, _ in b_blocks]
    common = {i for i in a_ids if a_ids.count(i) == 1 and b_ids.count(i) == 1}
    both = {a_ids[n] for n in on_cycle(a_succ)} & {b_ids[n] for n in on_cycle(b_succ)}

    def uncovered(chosen):
        for succ, ids in ((a_succ, a_ids), (b_succ, b_ids)):
            removed = {ix for ix, i in enumerate(ids) if i in chosen}
            cycle = cyclic_component(succ, removed)
            if cycle is not None:
                return cycle, ids
        return None

    chosen = set()
    while (found := uncovered(chosen)) is not None:
        cycle, ids = found
        candidates = [ids[n] for n in cycle if ids[n] in common]
        if not candidates:
            raise Unsupported(
                f"{pass_name}: a loop through "
                + ", ".join(f"bb{ids[n]}" for n in cycle)
                + " has no block both programs keep"
            )
        candidates.sort(key=lambda i: i not in both)
        chosen.add(candidates[0])
    for ident in sorted(chosen):
        if uncovered(chosen - {ident}) is None:
            chosen.discard(ident)
    if chosen and (a_dyn or b_dyn):
        raise Unsupported(
            f"{pass_name}: loops in a program with internal calls (return labels "
            "on the stack at a cut point)"
        )
    cuts = []
    for ident in sorted(chosen):
        ia, ib = a_ids.index(ident), b_ids.index(ident)
        ha, hb = a_h.get(ia), b_h.get(ib)
        if ha is None or hb is None:
            raise Unsupported(f"{pass_name}: bb{ident} has no static stack height")
        if ha != hb:
            raise Unsupported(
                f"{pass_name}: bb{ident} is entered with {ha} words before and "
                f"{hb} after: the pass moved the header's words"
            )
        cuts.append((ia, ib, ha))
    return cuts


def loops(blocks):
    succ, _, _ = evm_cfg(blocks)
    return cyclic_component(succ, set()) is not None


def words(n):
    return [f"w{i}" for i in range(n)]


def stack_term(ws):
    return "[" + ", ".join(ws) + "]"


def evm_pair(a, b, cuts, fuel_a, fuel_b, heartbeats):
    """Lean source (after the header) proving the stage from EVM IR program a["def"] to
    b["def"] through the cut points cuts, as LayerSim and Stage."""
    A, B, p = a["def"], b["def"], b["def"]
    cfg = "Nat × EVM.Stack × State"
    cut_a = " || ".join(f"x.1 == {ia}" for ia, _, _ in cuts)
    cut_b = " || ".join(f"x.1 == {ib}" for _, ib, _ in cuts)
    out = []
    out.append(
        f"/-- The cut points of `{A}`: the blocks that cut its loops. -/\n"
        f"def {p}_cutA : {cfg} → Bool := fun x => {cut_a}\n\n"
        f"/-- The same blocks of `{B}`. -/\n"
        f"def {p}_cutB : {cfg} → Bool := fun x => {cut_b}\n\n"
    )
    # The cut tests of a configuration the code has built, so that evaluation decides them
    # only once the block is known.
    for side, test in (("A", cut_a), ("B", cut_b)):
        out.append(
            f"theorem {p}_cut{side}_mk (b : Nat) (st : EVM.Stack) (s : State) :\n"
            f"    {p}_cut{side} (b, st, s) = ({test.replace('x.1', 'b')}) := rfl\n\n"
        )
    rels = [
        "fun x x' => x = (0, [], State.initial call) ∧ x' = (0, [], State.initial call)"
    ]
    for ia, ib, h in cuts:
        ws = words(h)
        binder = f"∃ ({' '.join(ws)} : Word) (s : State), " if ws else "∃ s : State, "
        rels.append(
            f"fun x x' => {binder}x = ({ia}, {stack_term(ws)}, s) ∧ "
            f"x' = ({ib}, {stack_term(ws)}, s)"
        )
    out.append(
        f"/-- The relations at the first configurations and at each pair of cut points: "
        f"the same words at the same height, and the same state. -/\n"
        f"def {p}_rels (call : Call) : List ({cfg} → {cfg} → Prop) :=\n  ["
        + ",\n   ".join(rels)
        + "]\n\n"
    )

    def mem(i):
        t = "List.Mem.head _"
        for _ in range(i):
            t = f"List.Mem.tail _ ({t})"
        return t

    mks = []
    for n, (ia, ib, h) in enumerate(cuts, start=1):
        ws = words(h)
        wb = f"{{{' '.join(ws)} : Word}} " if ws else ""
        name = f"{p}_mk{n}"
        mks.append(name)
        out.append(
            f"theorem {name} {{call : Call}} {wb}{{s : State}} {{y : {cfg}}}\n"
            f"    (h : y = ({ib}, {stack_term(ws)}, s)) :\n"
            f"    ∃ Q ∈ {p}_rels call, Q ({ia}, {stack_term(ws)}, s) y :=\n"
            f"  rel_mem ({mem(n)}) ⟨{', '.join(ws + ['s'])}, rfl, h⟩\n\n"
        )
    seg_rel = f"SegRel Agree (fun y y' => ∃ Q ∈ {p}_rels call, Q y y')"
    unf = f"{p}_cutA_mk, {p}_cutB_mk"
    segs = []
    starts = [((0, [], "State.initial call"), (0, [], "State.initial call"), [])] + [
        ((ia, words(h), "s"), (ib, words(h), "s"), words(h) + ["s"])
        for ia, ib, h in cuts
    ]
    for n, ((ia, wa, sa), (ib, wb_, sb), vars_) in enumerate(starts):
        name = f"{p}_seg{n}"
        segs.append(name)
        binders = ""
        ws = [v for v in vars_ if v != "s"]
        if ws:
            binders += f" ({' '.join(ws)} : Word)"
        if "s" in vars_:
            binders += " (s : State)"
        out.append(
            f"set_option maxHeartbeats {heartbeats} in\n"
            f"theorem {name} (o : Oracle) (call : Call){binders} :\n"
            f"    ∃ k k', {seg_rel}\n"
            f"      (seg (EVM.Program.block o call {A}) {p}_cutA k ({ia}, {stack_term(wa)}, {sa}))\n"
            f"      (seg (EVM.Program.block o call {B}) {p}_cutB k' ({ib}, {stack_term(wb_)}, {sb})) := by\n"
            f"  refine ⟨{fuel_a}, {fuel_b}, ?_⟩\n"
            f"  sym_seg [{unf}] [{', '.join(mks)}]\n\n"
        )
    cases = []
    for n, (_, _, vars_) in enumerate(starts):
        pat = "⟨rfl, rfl⟩" if not vars_ else "⟨" + ", ".join(vars_) + ", rfl, rfl⟩"
        args = " ".join(vars_)
        cases.append(
            f"  · obtain {pat} := holds\n    exact {segs[n]} o call {args}".rstrip()
        )
    out.append(
        f"theorem {p}_segments (o : Oracle) (call : Call) :\n"
        f"    ∀ P ∈ {p}_rels call, ∀ x x', P x x' → ∃ k k', {seg_rel}\n"
        f"      (seg (EVM.Program.block o call {A}) {p}_cutA k x)\n"
        f"      (seg (EVM.Program.block o call {B}) {p}_cutB k' x') := by\n"
        f"  intro P mem x x' holds\n"
        f"  simp only [{p}_rels, List.mem_cons, List.not_mem_nil, or_false] at mem\n"
        f"  rcases mem with {' | '.join(['rfl'] * len(starts))}\n"
        + "\n".join(cases)
        + "\n\n"
    )
    out.append(
        f"/-- `{A}` and `{B}` simulated through their cut points on every call. -/\n"
        f"def {p}_sim (o : Oracle) :\n"
        f"    CutPoint.LayerSim EVM.Program.layer EVM.Program.layer o {A} {B} anyCall where\n"
        f"  σ := {cfg}\n  τ := {cfg}\n"
        f"  step call := EVM.Program.block o call {A}\n"
        f"  step' call := EVM.Program.block o call {B}\n"
        f"  init call := (0, [], State.initial call)\n"
        f"  init' call := (0, [], State.initial call)\n"
        f"  runs _ _ := rfl\n  runs' _ _ := rfl\n"
        f"  cut := {p}_cutA\n  cut' := {p}_cutB\n"
        f"  Rel call := fun x x' => ∃ P ∈ {p}_rels call, P x x'\n"
        f"  start _ _ := ⟨_, List.Mem.head _, rfl, rfl⟩\n"
        f"  sim call _ := CutPoint.Sim.of_each ({p}_segments o call)\n\n"
    )
    return "".join(out)


# --- Lowered MIR ---------------------------------------------------------------------------


def parse_mir(text):
    """The functions of an exported lowered MIR module, in order (their numbers): each a dict
    of params and blocks by number, a block (phis, insts, term) with phis
    [(dst, [(pred, operand)])]."""
    body = text[text.index("functions := [") + len("functions := [") :]
    parts = re.split(r"^\s*-- @(\S+)\s*$", body, flags=re.MULTILINE)
    funcs = []
    for i in range(1, len(parts), 2):
        chunk = parts[i + 1]
        params = [int(n) for n in re.findall(r"\d+", chunk[: chunk.index("blocks :=")])]
        blocks = {}
        bparts = re.split(r"^\s*-- bb(\d+)\s*$", chunk, flags=re.MULTILINE)
        for j in range(1, len(bparts), 2):
            b = bparts[j + 1].strip()
            start, end = b.index("⟨"), b.rindex("⟩")
            phis_s, insts_s, term = split_top(b[start + 1 : end])
            phis = []
            for dst, inc in re.findall(
                r"\((\d+), \[((?:\(\d+, [^()]*\)(?:, )?)*)\]\)", phis_s
            ):
                incoming = [
                    (int(p), op.strip())
                    for p, op in re.findall(
                        r"\((\d+), (\.(?:value|const) [0-9a-fx]+)\)", inc
                    )
                ]
                phis.append((int(dst), incoming))
            insts = split_top(insts_s.strip()[1:-1]) if insts_s.strip() != "[]" else []
            blocks[int(bparts[j])] = (phis, insts, term.strip())
        funcs.append({"name": parts[i], "params": params, "blocks": blocks})
    return funcs


def mir_defs(inst):
    m = re.match(r"\.(?:pure|ne|word|low|select|heapFloor|alloc) (\d+)", inst)
    if m:
        return {int(m[1])}
    m = re.match(r"\.(?:effect|call) \(some (\d+)\)", inst)
    out = {int(m[1])} if m else set()
    if inst.startswith(".call"):
        out |= {int(n) for n in re.findall(r"some (\d+)", inst)}
    return out


def mir_uses(text):
    return {int(n) for n in re.findall(r"\.value (\d+)", text)}


def mir_succ(term):
    m = re.fullmatch(r"\.jump (\d+)", term)
    if m:
        return [int(m[1])]
    m = re.fullmatch(r"\.branch \(.*\) (\d+) (\d+)", term)
    if m:
        return [int(m[1]), int(m[2])]
    if term.startswith(".switch"):
        head, cases = term.split("[", 1)
        return [int(head.split()[-1])] + [
            int(t) for t in re.findall(r", (\d+)\)", cases)
        ]
    return []


def mir_liveness(func):
    """Values live on entry to each block, before its phis bind (phi destinations excluded),
    and the values each edge's phis read."""
    blocks = func["blocks"]
    upward, defs = {}, {}
    for b, (phis, insts, term) in blocks.items():
        seen, up = {d for d, _ in phis}, set()
        for ins in insts:
            up |= mir_uses(ins) - seen
            seen |= mir_defs(ins)
        up |= mir_uses(term) - seen
        upward[b], defs[b] = up, seen

    def edge(p, s):
        return {
            int(op.split()[1])
            for _, inc in blocks[s][0]
            for q, op in inc
            if q == p and op.startswith(".value")
        }

    live = {b: set() for b in blocks}
    changed = True
    while changed:
        changed = False
        for b, (_, _, term) in blocks.items():
            out = set()
            for s in mir_succ(term):
                if s in blocks:
                    out |= live[s] | edge(b, s)
            new = upward[b] | (out - defs[b])
            if new != live[b]:
                live[b], changed = new, True
    return live, edge


def mir_headers(func):
    """Loop headers of a function: targets of back edges of a depth-first walk from block 0."""
    blocks = func["blocks"]
    succ = {
        b: [s for s in mir_succ(t) if s in blocks] for b, (_, _, t) in blocks.items()
    }
    color, heads = {}, set()

    def dfs(n):
        color[n] = 1
        for m in succ[n]:
            if color.get(m) == 1:
                heads.add(m)
            elif m not in color:
                dfs(m)
        color[n] = 2

    if 0 in blocks:
        dfs(0)
    preds = {h: sorted(b for b in blocks if h in succ[b]) for h in heads}
    return heads, preds, succ


def mir_cuts(pass_name, a_funcs, b_funcs):
    """Cut points of two MIR modules: (function, header, source predecessors, target
    predecessors), the headers the same in both. A pass may put blocks on a header's edges, so
    its predecessors may differ: the relation pairs every predecessor of one with every
    predecessor of the other, relating the words their phis bind."""
    if len(a_funcs) != len(b_funcs):
        raise Unsupported(f"{pass_name}: the modules have different functions")
    cuts = []
    for f, (fa, fb) in enumerate(zip(a_funcs, b_funcs)):
        ha, pa, _ = mir_headers(fa)
        hb, pb, _ = mir_headers(fb)
        if ha != hb:
            raise Unsupported(
                f"{pass_name}: @{fa['name']} has loop headers {sorted(ha)} before "
                f"and {sorted(hb)} after"
            )
        for h in sorted(ha):
            da = sorted(d for d, _ in fa["blocks"][h][0])
            db = sorted(d for d, _ in fb["blocks"][h][0])
            if da != db:
                raise Unsupported(
                    f"{pass_name}: @{fa['name']} bb{h} binds phis {da} before and {db} after"
                )
            cuts.append((f, h, pa[h], pb[h]))
    called = {
        int(n)
        for fs in (a_funcs, b_funcs)
        for fn in fs
        for _, insts, _ in fn["blocks"].values()
        for i in insts
        for n in re.findall(r"^\.call \S+ (\d+)", i)
    }
    for f, _, _, _ in cuts:
        if f in called:
            raise Unsupported(
                f"{pass_name}: a loop in @{a_funcs[f]['name']}, which an internal "
                "call enters (a call stack at the cut point)"
            )
    return cuts


def phi_input(func, h, pred, dst):
    """The operand phi dst of block h reads from pred."""
    for d, incoming in func["blocks"][h][0]:
        if d == dst:
            for p, op in incoming:
                if p == pred:
                    return op
    raise Unsupported(f"@{func['name']} bb{h}: phi {dst} has no input from bb{pred}")


def word_lit(op):
    return f"({op.split()[1]} : Word)"


# --- Lean ------------------------------------------------------------------------------------

MIR_CFG = "MIR.Lowered.Frame"
EVM_CFG = "Nat × EVM.Stack × State"


def mem_proof(i):
    t = "List.Mem.head _"
    for _ in range(i):
        t = f"List.Mem.tail _ ({t})"
    return t


def binders(vs):
    """Lean binders for [(name, type)], grouped by type."""
    out, cur, ty = [], [], None
    for n, t in vs:
        if t != ty and cur:
            out.append((cur, ty))
            cur = []
        cur.append(n)
        ty = t
    if cur:
        out.append((cur, ty))
    return out


def binder_text(vs, brackets="()"):
    lo, hi = brackets
    return " ".join(f"{lo}{' '.join(ns)} : {t}{hi}" for ns, t in binders(vs))


def cut_defs(p, side, cfg, tests):
    """The cut test of one side and its equations on configurations the code has built."""
    name = f"{p}_cut{side}"
    if cfg == EVM_CFG:
        test = " || ".join(f"b == {i}" for i in tests) or "false"
        return (
            name,
            [f"{name}_mk"],
            (
                f"def {name} : {cfg} → Bool := fun x => {test.replace('b ==', 'x.1 ==')}\n\n"
                f"theorem {name}_mk (b : Nat) (st : EVM.Stack) (s : State) :\n"
                f"    {name} (b, st, s) = ({test}) := rfl\n\n"
            ),
        )
    test = " || ".join(f"(f == {fn} && b == {h})" for fn, h in tests) or "false"
    return (
        name,
        [f"{name}_enter", f"{name}_resume"],
        (
            f"def {name} : {cfg} → Bool := fun x =>\n"
            f"  match x.2.1 with\n"
            f"  | .enter f _ b => {test}\n"
            f"  | .resume .. => false\n\n"
            f"theorem {name}_enter (rs : List MIR.Lowered.Return) (f p b : Nat) "
            f"(env : MIR.Lowered.Env) (s : State) :\n"
            f"    {name} (rs, .enter f p b, env, s) = ({test}) := rfl\n\n"
            f"theorem {name}_resume (rs : List MIR.Lowered.Return) (f b : Nat) "
            f"(is : List MIR.Lowered.Inst) (t : MIR.Lowered.Terminator) (env : MIR.Lowered.Env) "
            f"(s : State) :\n"
            f"    {name} (rs, .resume f b is t, env, s) = false := rfl\n\n"
        ),
    )


def step_term(kind, prog):
    if kind == "mir":
        return f"MIR.Lowered.moduleBlock o call {prog}"
    return f"EVM.Program.block o call {prog}"


def init_term(kind, prog):
    if kind == "mir":
        return (
            f"([], .enter {prog}.entry 0 0, fun _ => 0, "
            f"MIR.Lowered.Module.startState {prog} (State.initial call))"
        )
    return "(0, [], State.initial call)"


def layer_term(kind):
    return "MIR.Lowered.moduleLayer" if kind == "mir" else "EVM.Program.layer"


def pair_module(a, b, cuts_a, cuts_b, rels, fuel_a, fuel_b, heartbeats):
    """Lean (after the header) proving the stage from a["def"] to b["def"] through cut points:
    cuts_a/cuts_b are the cut tests' blocks, rels the relations at pairs of cut points, each a
    dict of src_vars, tgt_vars ([(name, type)]), src and tgt (configuration terms) and conds
    (propositions over the variables)."""
    A, B, p = a["def"], b["def"], b["def"]
    ca = MIR_CFG if a["kind"] == "mir" else EVM_CFG
    cb = MIR_CFG if b["kind"] == "mir" else EVM_CFG
    out = []
    cut_a, unf_a, text_a = cut_defs(p, "A", ca, cuts_a)
    cut_b, unf_b, text_b = cut_defs(p, "B", cb, cuts_b)
    out += [text_a, text_b]
    entry = {
        "src_vars": [],
        "tgt_vars": [],
        "src": init_term(a["kind"], A),
        "tgt": init_term(b["kind"], B),
        "conds": [],
    }
    allrels = [entry] + rels

    def body(r):
        vs = r["src_vars"] + r["tgt_vars"]
        parts = [f"x = {r['src']}", f"x' = {r['tgt']}"] + r["conds"]
        inner = " ∧ ".join(parts)
        return f"∃ {binder_text(vs)}, {inner}" if vs else inner

    out.append(
        f"/-- The relations at the first configurations and at each pair of cut points, from what "
        f"the compiler kept across the pass. -/\n"
        f"def {p}_rels (call : Call) : List ({ca} → {cb} → Prop) :=\n  ["
        + ",\n   ".join(f"fun x x' => {body(r)}" for r in allrels)
        + "]\n\n"
    )
    mks = []
    for n, r in enumerate(rels, start=1):
        name = f"{p}_mk{n}"
        mks.append(name)
        tv = r["tgt_vars"]
        concl = " ∧ ".join([f"y = {r['tgt']}"] + r["conds"])
        h = f"∃ {binder_text(tv)}, {concl}" if tv else concl
        src_vars = binder_text(r["src_vars"], "{}")
        names = [v for v, _ in r["src_vars"]]
        k = len(r["conds"]) + 1
        if tv:
            tnames = [v for v, _ in tv]
            pat = "⟨" + ", ".join(tnames + [f"h{i}" for i in range(k)]) + "⟩"
            wit = (
                "⟨"
                + ", ".join(names + tnames + ["rfl"] + [f"h{i}" for i in range(k)])
                + "⟩"
            )
            proof = f"by\n  obtain {pat} := h\n  exact rel_mem ({mem_proof(n)}) {wit}"
        else:
            if k == 1:
                wit = "⟨" + ", ".join([*names, "rfl", "h"]) + "⟩"
                proof = f"\n  rel_mem ({mem_proof(n)}) {wit}"
            else:
                pat = "⟨" + ", ".join(f"h{i}" for i in range(k)) + "⟩"
                wit = (
                    "⟨" + ", ".join(names + ["rfl"] + [f"h{i}" for i in range(k)]) + "⟩"
                )
                proof = (
                    f"by\n  obtain {pat} := h\n  exact rel_mem ({mem_proof(n)}) {wit}"
                )
        out.append(
            f"theorem {name} {{call : Call}} {src_vars} {{y : {cb}}}\n"
            f"    (h : {h}) :\n"
            f"    ∃ Q ∈ {p}_rels call, Q {r['src']} y := {proof}\n\n"
        )
    seg_rel = f"SegRel Agree (fun y y' => ∃ Q ∈ {p}_rels call, Q y y')"
    unf = ", ".join(unf_a + unf_b)
    segs = []
    for n, r in enumerate(allrels):
        name = f"{p}_seg{n}"
        segs.append(name)
        vs = r["src_vars"] + r["tgt_vars"]
        hyps = " ".join(f"(h{i} : {c})" for i, c in enumerate(r["conds"]))
        args = " ".join(x for x in [binder_text(vs), hyps] if x)
        out.append(
            f"set_option maxHeartbeats {heartbeats} in\n"
            f"theorem {name} (o : Oracle) (call : Call) {args} :\n"
            f"    ∃ k k', {seg_rel}\n"
            f"      (seg ({step_term(a['kind'], A)}) {cut_a} k {r['src']})\n"
            f"      (seg ({step_term(b['kind'], B)}) {cut_b} k' {r['tgt']}) := by\n"
            f"  refine ⟨{fuel_a}, {fuel_b}, ?_⟩\n"
            f"  sym_seg [{unf}] [{', '.join(mks)}]\n\n"
        )
    cases = []
    for n, r in enumerate(allrels):
        vs = [v for v, _ in r["src_vars"] + r["tgt_vars"]]
        hs = [f"h{i}" for i in range(len(r["conds"]))]
        pat = "⟨" + ", ".join(vs + ["rfl", "rfl"] + hs) + "⟩"
        cases.append(
            f"  · obtain {pat} := holds\n"
            f"    exact {segs[n]} o call {' '.join(vs + hs)}".rstrip()
        )
    out.append(
        f"theorem {p}_segments (o : Oracle) (call : Call) :\n"
        f"    ∀ P ∈ {p}_rels call, ∀ x x', P x x' → ∃ k k', {seg_rel}\n"
        f"      (seg ({step_term(a['kind'], A)}) {cut_a} k x)\n"
        f"      (seg ({step_term(b['kind'], B)}) {cut_b} k' x') := by\n"
        f"  intro P mem x x' holds\n"
        f"  simp only [{p}_rels, List.mem_cons, List.not_mem_nil, or_false] at mem\n"
        f"  rcases mem with {' | '.join(['rfl'] * len(allrels))}\n"
        + "\n".join(cases)
        + "\n\n"
    )
    out.append(
        f"/-- `{A}` and `{B}` simulated through their cut points on every call. -/\n"
        f"def {p}_sim (o : Oracle) :\n"
        f"    CutPoint.LayerSim {layer_term(a['kind'])} {layer_term(b['kind'])} o {A} {B} "
        f"anyCall where\n"
        f"  σ := {ca}\n  τ := {cb}\n"
        f"  step call := {step_term(a['kind'], A)}\n"
        f"  step' call := {step_term(b['kind'], B)}\n"
        f"  init call := {init_term(a['kind'], A)}\n"
        f"  init' call := {init_term(b['kind'], B)}\n"
        f"  runs _ _ := rfl\n  runs' _ _ := rfl\n"
        f"  cut := {cut_a}\n  cut' := {cut_b}\n"
        f"  Rel call := fun x x' => ∃ P ∈ {p}_rels call, P x x'\n"
        f"  start _ _ := ⟨_, List.Mem.head _, rfl, rfl⟩\n"
        f"  sim call _ := CutPoint.Sim.of_each ({p}_segments o call)\n\n"
    )
    return "".join(out)


def env_word(op, env):
    """An operand's word in env: a value's word or a constant."""
    if op.startswith(".value"):
        return f"{env} {op.split()[1]}"
    return word_lit(op)


def mir_mir_rels(pass_name, fa_list, fb_list, cuts):
    """Relations between two MIR modules at their headers: the same state, the phis' inputs
    and every value live into the header the same words."""
    rels, tests = [], []
    for f, h, preds_a, preds_b in cuts:
        fa, fb = fa_list[f], fb_list[f]
        tests.append((f, h))
        live_a, _ = mir_liveness(fa)
        live_b, _ = mir_liveness(fb)
        dsts = sorted(d for d, _ in fa["blocks"][h][0])
        live = sorted((live_a[h] | live_b[h]) - set(dsts))
        for pa in preds_a:
            for pb in preds_b:
                conds = [f"env' {v} = env {v}" for v in live]
                for d in dsts:
                    ia, ib = phi_input(fa, h, pa, d), phi_input(fb, h, pb, d)
                    conds.append(f"{env_word(ib, "env'")} = {env_word(ia, 'env')}")
                rels.append(
                    {
                        "src_vars": [("env", "MIR.Lowered.Env"), ("s", "State")],
                        "tgt_vars": [("env'", "MIR.Lowered.Env")],
                        "src": f"([], MIR.Lowered.Pos.enter {f} {pa} {h}, env, s)",
                        "tgt": f"([], MIR.Lowered.Pos.enter {f} {pb} {h}, env', s)",
                        "conds": conds,
                    }
                )
    return tests, tests, rels


def mir_evm_rels(pass_name, funcs, sched_blocks, schedule, cuts):
    """Relations between the last MIR module and the scheduled EVM IR at its loop headers: the
    MIR header entered from a predecessor, and the scheduled block where its code starts, with
    the stack the scheduler recorded there (each value's word, a phi's word as its input from
    that predecessor), any words below the function's own, and the same state."""
    ids = [i for i, _ in sched_blocks]
    _, heights, _ = evm_cfg(sched_blocks)
    fmaps = {fm["function"]: fm for fm in schedule["functions"]}
    rels, tests_a, tests_b = [], [], []
    for f, h, preds, _ in cuts:
        fm = fmaps.get(f)
        if fm is None or fm["unsupported"] or fm["rebuilt"]:
            raise Unsupported(
                f"{pass_name}: no schedule map of function {f} the relation can use"
            )
        bm = next((b for b in fm["blocks"] if b["block"] == h), None)
        if bm is None or bm["start"] is None or bm["start"][1] != 0:
            raise Unsupported(
                f"{pass_name}: MIR header bb{h} of function {f} does not start a "
                "scheduled block"
            )
        e = ids.index(bm["start"][0])
        height = heights.get(e)
        layout = bm["layout"]
        if height is None or height < len(layout):
            raise Unsupported(
                f"{pass_name}: scheduled block bb{bm['start'][0]} has no static "
                "height that holds its layout"
            )
        if "ret" in layout:
            raise Unsupported(
                f"{pass_name}: a return address at the loop header of {f}"
            )
        tests_a.append((f, h))
        tests_b.append(e)
        dsts = {d for d, _ in funcs[f]["blocks"][h][0]}
        for pa in preds:
            junk, words = [], []
            for loc in reversed(layout):
                if loc == "junk":
                    junk.append(f"j{len(junk)}")
                    words.append(junk[-1])
                else:
                    v = int(loc[1:])
                    words.append(
                        env_word(phi_input(funcs[f], h, pa, v), "env")
                        if v in dsts
                        else f"env {v}"
                    )
            for _ in range(height - len(layout)):
                junk.append(f"j{len(junk)}")
                words.append(junk[-1])
            rels.append(
                {
                    "src_vars": [("env", "MIR.Lowered.Env"), ("s", "State")],
                    "tgt_vars": [(j, "Word") for j in junk],
                    "src": f"([], MIR.Lowered.Pos.enter {f} {pa} {h}, env, s)",
                    "tgt": f"({e}, [{', '.join(words)}], s)",
                    "conds": [],
                }
            )
    if tests_b:
        succ, _, _ = evm_cfg(sched_blocks)
        cyc = cyclic_component(succ, set(tests_b))
        if cyc:
            raise Unsupported(
                f"{pass_name}: a scheduled loop through "
                + ", ".join(f"bb{ids[n]}" for n in cyc)
                + " passes no MIR header's block"
            )
    return tests_a, tests_b, rels


def evm_evm_rels(cuts):
    rels = []
    for ia, ib, h in cuts:
        ws = [(f"w{i}", "Word") for i in range(h)]
        st = "[" + ", ".join(w for w, _ in ws) + "]"
        rels.append(
            {
                "src_vars": ws + [("s", "State")],
                "tgt_vars": [],
                "src": f"({ia}, {st}, s)",
                "tgt": f"({ib}, {st}, s)",
                "conds": [],
            }
        )
    return [ia for ia, _, _ in cuts], [ib for _, ib, _ in cuts], rels


def stage_body(gen, a, b, heartbeats=2000000):
    """The cut-point proof of the stage from a to b (manifest stages with def, file, kind and
    pass), after the module header, or raise Unsupported."""
    ta = (gen / a["file"]).read_text()
    tb = (gen / b["file"]).read_text()
    if a["kind"] == "mir" and b["kind"] == "mir":
        fa, fb = parse_mir(ta), parse_mir(tb)
        cuts = mir_cuts(b["pass"], fa, fb)
        tests_a, tests_b, rels = mir_mir_rels(b["pass"], fa, fb, cuts)
        fuel_a = sum(len(f["blocks"]) for f in fa) + 1
        fuel_b = sum(len(f["blocks"]) for f in fb) + 1
    elif a["kind"] == "mir":
        fa, sched = parse_mir(ta), parse_evm(tb)
        cuts = mir_cuts(b["pass"], fa, fa)
        schedule_file = gen / "Schedule.json"
        if cuts and not schedule_file.exists():
            raise Unsupported(f"{b['pass']}: no Schedule.json from the exporter")
        schedule = json.loads(schedule_file.read_text()) if cuts else {"functions": []}
        tests_a, tests_b, rels = mir_evm_rels(b["pass"], fa, sched, schedule, cuts)
        fuel_a = sum(len(f["blocks"]) for f in fa) + 1
        fuel_b = len(sched) + 1
    else:
        ea, eb = parse_evm(ta), parse_evm(tb)
        cuts = evm_cuts(b["pass"], ea, eb)
        tests_a, tests_b, rels = evm_evm_rels(cuts)
        fuel_a, fuel_b = len(ea) + 1, len(eb) + 1
    return pair_module(a, b, tests_a, tests_b, rels, fuel_a, fuel_b, heartbeats)


# --- Driver ----------------------------------------------------------------------------------

LAYER = {
    "mir": "MIR.Lowered.moduleLayer",
    "sched": "EVM.Program.layer",
    "evm": "EVM.Program.layer",
}
AXIOMS = "[propext, Classical.choice, Quot.sound]"


def write_proofs(directory, name, contract, heartbeats=2000000):
    """Writes milestone/NAME/Proofs from the exported stages in NAME/gen/CONTRACT: per pair of
    consecutive stages a cut-point simulation (Runs/<Stage>.lean), and Stages.lean chaining them
    from the first lowered MIR to the final EVM IR. Returns the final theorem's name, or raises
    Unsupported naming the pass and why."""
    gen = directory / name / "gen" / contract
    manifest = json.loads((gen / "manifest.json").read_text())
    stages = [s for s in manifest["stages"] if s["kind"] != "bytecode"]
    for s in stages:
        s["def"] = s["definition"]
    cap = "".join(p[:1].upper() + p[1:] for p in name.split("_"))
    ns = f"SolarTV.Milestone.{cap}"
    runs = directory / name / "Proofs" / "Runs"
    runs.mkdir(parents=True, exist_ok=True)
    opens = f"open EvmRules Machine SolarTV.Export.{contract} SolarTV.Milestone.W1 SolarTV.CutPoint"
    for a, b in itertools.pairwise(stages):
        ka = dict(a, kind="mir" if a["kind"] == "mir" else "evm")
        kb = dict(b, kind="mir" if b["kind"] == "mir" else "evm")
        body = stage_body(gen, ka, kb, heartbeats)
        module = b["file"][: -len(".lean")]
        (runs / f"{module}.lean").write_text(
            "-- Generated by solar-tv/milestone/cutpoints.py; do not edit.\n"
            "import W1Proofs.Segment\nimport W1Proofs.Stage\n"
            f"import {name}.gen.{contract}.{a['file'][: -len('.lean')]}\n"
            f"import {name}.gen.{contract}.{module}\n\n"
            f"/-! `{b['pass']}`: {a['def']} and {b['def']} simulated through cut points. -/\n\n"
            f"namespace {ns}\n\n{opens}\n\n{body}end {ns}\n"
        )
    mirs = [s for s in stages if s["kind"] == "mir"]
    sched = next(s for s in stages if s["kind"] == "sched")
    evms = [s for s in stages if s["kind"] == "evm"]

    def nest(xs):
        if not xs:
            return "trivial"
        return (
            "⟨" + ", ".join(f"({x['def']}_sim o).certified" for x in xs) + ", trivial⟩"
        )

    final = f"{name}_mir_to_evmir"
    last = (evms or [sched])[-1]["def"]
    out = ["-- Generated by solar-tv/milestone/cutpoints.py; do not edit."]
    out += [
        f"import {name}.Proofs.Runs.{s['file'][: -len('.lean')]}" for s in stages[1:]
    ]
    out += ["import W1Proofs.Stage", "import SolarTV.Composition", ""]
    out.append(
        f"/-! The backend stages of `{contract}`, each simulated through the cut points and "
        "relations the compiler's records give (`LayerSim`), on every call and every oracle, "
        "however many times its loops run, and chained from the first lowered MIR to the final "
        "EVM IR. -/\n"
    )
    out.append(f"namespace {ns}\n\n{opens}\n")
    out.append(
        "/-- The compilation from the first lowered MIR to the final EVM IR. -/\n"
        f"def compilation : Compilation :=\n  ⟨{mirs[0]['def']}, "
        f"[{', '.join(m['def'] for m in mirs[1:])}], {sched['def']}, "
        f"[{', '.join(e['def'] for e in evms)}]⟩\n"
    )
    out.append(
        "/-- Every end of the first lowered MIR on any call is an agreeing end of the final EVM "
        "IR. -/\n"
        f"theorem {final} (o : Oracle) :\n"
        f"    Stage (mirSem o) (evmirSem o) {mirs[0]['def']} {last} anyCall :=\n"
        f"  mir_to_evmir_certified o anyCall compilation {nest(mirs[1:])}\n"
        f"    ({sched['def']}_sim o).certified\n    {nest(evms)}\n"
    )
    names = [final]
    for a, b in itertools.pairwise(stages):
        d = b["def"]
        names.append(f"{d}_back")
        out.append(
            f"/-- `{b['pass']}` backwards: every end of `{d}` is an agreeing end of "
            f"`{a['def']}`. -/\n"
            f"theorem {d}_back (o : Oracle) :\n"
            f"    Certified {LAYER[b['kind']]} {LAYER[a['kind']]} o {d} {a['def']} anyCall :=\n"
            f"  ({d}_sim o).certified_back\n"
        )
    for n in names:
        out.append(
            f"/-- info: '{ns}.{n}' depends on axioms: {AXIOMS} -/\n"
            f"#guard_msgs in\n#print axioms {n}\n"
        )
    out.append(f"end {ns}\n")
    (directory / name / "Proofs" / "Stages.lean").write_text("\n".join(out))
    (directory / name / "Proofs.lean").write_text(
        "-- Generated by solar-tv/milestone/cutpoints.py; do not edit.\n"
        f"import {name}.Proofs.Stages\n"
    )
    return f"{ns}.{final}"


def main():
    import argparse
    import pathlib
    import sys

    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("name")
    ap.add_argument("contract")
    ap.add_argument("--heartbeats", type=int, default=2000000)
    args = ap.parse_args()
    here = pathlib.Path(__file__).resolve().parent
    try:
        print(write_proofs(here, args.name, args.contract, args.heartbeats))
    except Unsupported as e:
        sys.exit(f"error: unsupported: {e}")


if __name__ == "__main__":
    main()
