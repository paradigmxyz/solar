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


def evm_cuts(pass_name, a_blocks, b_blocks):
    """Cut points of two EVM IR programs by their compiler block numbers: a list of
    (index in a, index in b, height), or [] when neither loops."""
    a_succ, a_h, a_dyn = evm_cfg(a_blocks)
    b_succ, b_h, b_dyn = evm_cfg(b_blocks)
    a_ids = [i for i, _ in a_blocks]
    b_ids = [i for i, _ in b_blocks]
    common = {i for i in a_ids if a_ids.count(i) == 1 and b_ids.count(i) == 1}
    chosen = set()
    for _ in range(len(a_blocks) + len(b_blocks) + 1):
        progress = False
        for succ, ids in ((a_succ, a_ids), (b_succ, b_ids)):
            removed = {ix for ix, i in enumerate(ids) if i in chosen}
            cycle = cyclic_component(succ, removed)
            if cycle is None:
                continue
            candidates = [ids[n] for n in cycle if ids[n] in common]
            if not candidates:
                raise Unsupported(
                    f"{pass_name}: a loop through "
                    + ", ".join(f"bb{ids[n]}" for n in cycle)
                    + " has no block both programs keep"
                )
            chosen.add(candidates[0])
            progress = True
            break
        if not progress:
            break
    else:
        raise Unsupported(f"{pass_name}: no cut set found")
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
