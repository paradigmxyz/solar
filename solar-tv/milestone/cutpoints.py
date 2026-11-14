#!/usr/bin/env python3
"""Cut points and relations for the stages of programs that loop.

certify.py proves a stage of loop-free programs by running both to their ends. A program that
loops cannot be run to its end on every call, so its stages are proved through cut points
(SolarTV.CutPoint): this module picks, for a pair of consecutive stages, configurations that cut
every cycle of both programs and the relation between them, from what the compiler kept across
the pass, and writes the segment obligations that sym_seg proves.

The relations are derived, never written by hand:

- Internal calls: a loop in a function that calls enter runs under a call stack. Each cut point
  gets one relation per call string that reaches it: in MIR the returns the calls pushed (the
  caller's block, the rest of it after the call and the caller's values, which the relation
  equates on every value the caller reads after the call), in EVM IR the return labels on the
  stack, found by running the program over stacks of labels and other words (evm_states), and
  below the callee's words the caller's, as the scheduler recorded them at the call
  (Schedule.json "calls"). A recursive call fails closed.
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
    if a_dyn or b_dyn:
        return evm_call_cuts(pass_name, a_blocks, b_blocks)
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


def evm_states(pass_name, blocks, limit=50000):
    """The configurations a program reaches, abstractly, and the edges between them: (block
    index, stack) with the stack top first, each word the block a pushLabel pushed or None for
    any other word. A return is a dynamic jump to the label on top, so a callee's blocks are
    reached once per call string; a dynamic jump to any other word, or a call string that keeps
    growing (recursion), fails closed."""
    start = (0, ())
    edges, work = {start: []}, [start]
    while work:
        state = work.pop()
        idx, stack = state
        stack = list(stack)
        out, end = [], False
        for ins in blocks[idx][1]:
            m = re.fullmatch(
                r"\.op \(\.stack \((\.dup|\.swap|\.exchange) ([0-9 ]+)\)\)", ins
            )
            if m:
                args = [int(t) for t in m[2].split()]
                need = {
                    ".dup": args[0] + 1,
                    ".swap": args[0] + 2,
                    ".exchange": max(args) + 1,
                }[m[1]]
                if len(stack) < need:
                    raise Unsupported(
                        f"{pass_name}: block bb{blocks[idx][0]} underflows its stack"
                    )
                if m[1] == ".dup":
                    stack.insert(0, stack[args[0]])
                elif m[1] == ".swap":
                    j = args[0] + 1
                    stack[0], stack[j] = stack[j], stack[0]
                else:
                    a, b = args
                    stack[a], stack[b] = stack[b], stack[a]
                continue
            m = re.fullmatch(r"\.pushLabel (\d+)", ins)
            if m:
                stack.insert(0, int(m[1]))
                continue
            pop, push, kind, targets = effect(ins)
            if len(stack) < pop:
                raise Unsupported(
                    f"{pass_name}: block bb{blocks[idx][0]} underflows its stack"
                )
            if kind in ("dyn", "dyni"):
                target = stack[0]
                if target is None:
                    raise Unsupported(
                        f"{pass_name}: block bb{blocks[idx][0]} jumps to a word no label pushed"
                    )
                targets = [target]
            stack = stack[pop:]
            out += [(t, tuple(stack)) for t in targets]
            if kind in ("jump", "dyn", "table", "end"):
                end = True
                break
            stack = [None] * push + stack
        if not end:
            out.append((idx + 1, tuple(stack)))
        out = [s for s in out if s[0] < len(blocks)]
        edges[state] = out
        for s in out:
            if s not in edges:
                if len(edges) >= limit or len(s[1]) > 1024:
                    raise Unsupported(
                        f"{pass_name}: more than {limit} call strings (a recursive call?)"
                    )
                edges[s] = []
                work.append(s)
    return edges


def evm_call_cuts(pass_name, a_blocks, b_blocks):
    """Cut points of two EVM IR programs with internal calls, by call string: the cut blocks are
    chosen as in evm_cuts, on the graph of configurations evm_states reaches, so that every cycle
    under every call stack passes one; each cut block gets one relation per stack shape it is
    entered with (its height and where return labels sit, named by the blocks they return to),
    which must be the same in both programs: the same words at the same height, the labels each
    program's own, and the same state. Returns the cut tests of both sides and the relations."""
    a_ids = [i for i, _ in a_blocks]
    b_ids = [i for i, _ in b_blocks]
    common = {i for i in a_ids if a_ids.count(i) == 1 and b_ids.count(i) == 1}
    sides = []
    for blocks, ids in ((a_blocks, a_ids), (b_blocks, b_ids)):
        edges = scoped_states(blocks, evm_states(pass_name, blocks))
        nodes = sorted(edges, key=lambda s: (s[0], len(s[1]), str(s[1])))
        ix = {s: n for n, s in enumerate(nodes)}
        succ = {ix[s]: sorted({ix[t] for t in edges[s]}) for s in nodes}
        sides.append((nodes, succ, [ids[s[0]] for s in nodes], ids))
    both = {sides[0][2][n] for n in on_cycle(sides[0][1])} & {
        sides[1][2][n] for n in on_cycle(sides[1][1])
    }

    def uncovered(chosen):
        for nodes, succ, node_ids, _ in sides:
            removed = {n for n, i in enumerate(node_ids) if i in chosen}
            cycle = cyclic_component(succ, removed)
            if cycle is not None:
                return cycle, node_ids
        return None

    chosen = set()
    while (found := uncovered(chosen)) is not None:
        cycle, node_ids = found
        candidates = [node_ids[n] for n in cycle if node_ids[n] in common]
        if not candidates:
            raise Unsupported(
                f"{pass_name}: a loop through "
                + ", ".join(f"bb{node_ids[n]}" for n in cycle)
                + " has no block both programs keep"
            )
        candidates.sort(key=lambda i: i not in both)
        chosen.add(candidates[0])
    for ident in sorted(chosen):
        if uncovered(chosen - {ident}) is None:
            chosen.discard(ident)
    def shapes_of(ident):
        ia, ib = a_ids.index(ident), b_ids.index(ident)
        shapes = []
        for (nodes, _, _, ids), at in zip(sides, (ia, ib)):
            shape = {}
            for idx, stack in nodes:
                if idx == at:
                    key = tuple(None if w is None else ids[w] for w in stack)
                    shape[key] = stack
            shapes.append(shape)
        return shapes

    def preds_of(ident):
        """The blocks each program enters ident from: a cut both programs reach from the same
        blocks ends their segments together, where one the pass jumps to from a block that did
        not before (a threaded jump to a shared revert) would end one side's segment only."""
        out = []
        for nodes, succ, node_ids, _ in sides:
            out.append({node_ids[n] for n, ts in succ.items() for t in ts if node_ids[t] == ident})
        return out

    def same_entry(ident):
        shapes = shapes_of(ident)
        preds = preds_of(ident)
        return set(shapes[0]) == set(shapes[1]) and preds[0] == preds[1]

    # Join points: a block both programs keep that more than one configuration enters, under
    # the same stack shapes and from the same blocks in both, is cut too, so that every segment
    # is a tree of configurations and no path through the program is evaluated twice. Any other
    # join stays inside its segments.
    for nodes, succ, node_ids, _ in sides if JOINS else ():
        preds = {}
        for n, ts in succ.items():
            for t in set(ts):
                preds[t] = preds.get(t, 0) + 1
        for n, k in preds.items():
            ident = node_ids[n]
            if k >= 2 and ident in common and ident not in chosen:
                if same_entry(ident):
                    chosen.add(ident)
    a_nodes, a_succ_n, a_node_ids, _ = sides[0]
    a_ix = {s: n for n, s in enumerate(a_nodes)}

    def reach(at, stack):
        """The cut configurations of a (block index, stack shape) a segment of a from
        (at, stack) can end at: the relation lemmas its proof needs to try."""
        start = a_ix.get((at, tuple(stack)))
        if start is None or not JOINS:
            return None
        seen, work, found = {start}, [start], set()
        while work:
            for t in a_succ_n[work.pop()]:
                if a_node_ids[t] in chosen:
                    found.add(a_nodes[t])
                elif t not in seen:
                    seen.add(t)
                    work.append(t)
        return found

    if JOINS:
        bound_segments(sides[0], chosen, same_entry, common)
    rels, tests_a, tests_b = [], [], []
    for ident in sorted(chosen):
        ia, ib = a_ids.index(ident), b_ids.index(ident)
        tests_a.append(ia)
        tests_b.append(ib)
        shapes = shapes_of(ident)
        if set(shapes[0]) != set(shapes[1]):
            raise Unsupported(
                f"{pass_name}: bb{ident} is entered with stacks {sorted(map(str, shapes[0]))} "
                f"before and {sorted(map(str, shapes[1]))} after"
            )
        for key in sorted(shapes[0], key=str):
            sa, sb = shapes[0][key], shapes[1][key]
            ws = [(f"w{k}", "EvmRules.Word") for k, w in enumerate(sa) if w is None]

            def term(stack):
                return (
                    "["
                    + ", ".join(
                        f"w{k}" if w is None else f"(BitVec.ofNat 256 {w})"
                        for k, w in enumerate(stack)
                    )
                    + "]"
                )

            rels.append(
                {
                    "src_vars": ws + [("s", "State")],
                    "tgt_vars": [],
                    "src": f"({ia}, {term(sa)}, s)",
                    "tgt": f"({ib}, {term(sb)}, s)",
                    "conds": [],
                    "block": (ia, tuple(sa)),
                    "reach": reach(ia, sa),
                }
            )
    fuel = (len(sides[0][0]) + 1, len(sides[1][0]) + 1)
    return {
        "tests": (tests_a, tests_b),
        "rels": rels,
        "fuel": fuel,
        "entry_reach": reach(0, ()),
    }


def loops(blocks):
    succ, _, _ = evm_cfg(blocks)
    return cyclic_component(succ, set()) is not None


def words(n):
    return [f"w{i}" for i in range(n)]


def stack_term(ws):
    return "[" + ", ".join(ws) + "]"


def evm_pair(a, b, cuts, fuel_a, fuel_b, heartbeats, recdepth=None):
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
        binder = (
            f"∃ ({' '.join(ws)} : EvmRules.Word) (s : State), "
            if ws
            else "∃ s : State, "
        )
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
        wb = f"{{{' '.join(ws)} : EvmRules.Word}} " if ws else ""
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
            binders += f" ({' '.join(ws)} : EvmRules.Word)"
        if "s" in vars_:
            binders += " (s : State)"
        out.append(
            f"set_option maxHeartbeats {heartbeats} in\n"
            + (f"set_option maxRecDepth {recdepth} in\n" if recdepth else "")
            + f"theorem {name} (o : Oracle) (call : Call){binders} :\n"
            f"    ∃ k, {seg_rel}\n"
            f"      (seg (EVM.Program.block o call {A}) {p}_cutA k ({ia}, {stack_term(wa)}, {sa}))\n"
            f"      (seg (EVM.Program.block o call {B}) {p}_cutB {fuel_b} ({ib}, {stack_term(wb_)}, {sb})) := by\n"
            f"  refine ⟨{fuel_a}, ?_⟩\n"
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
        f"    ∀ P ∈ {p}_rels call, ∀ x x', P x x' → ∃ k, {seg_rel}\n"
        f"      (seg (EVM.Program.block o call {A}) {p}_cutA k x)\n"
        f"      (seg (EVM.Program.block o call {B}) {p}_cutB {fuel_b} x') := by\n"
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
        f"  sim call _ := CutPoint.Sim.of_each fun P m x x' h =>\n"
        f"    match {p}_segments o call P m x x' h with | ⟨k, s⟩ => ⟨k, {fuel_b}, s⟩\n\n"
    )
    out.append(
        f"/-- Every segment of `{B}` takes at most {fuel_b} steps: the bound that carries a loop's\n"
        f"ranking certificate on `{A}` to a step bound on `{B}` (`CutPoint.ends_within_sim_start`). -/\n"
        f"theorem {p}_bounded (o : Oracle) (call : Call) :\n"
        f"    CutPoint.BoundedSim (EVM.Program.block o call {A}) {p}_cutA\n"
        f"      (EVM.Program.block o call {B}) {p}_cutB Agree\n"
        f"      (fun x x' => ∃ P ∈ {p}_rels call, P x x') {fuel_b} :=\n"
        f"  fun x x' ⟨P, m, h⟩ => {p}_segments o call P m x x' h\n\n"
    )
    return "".join(out)


# --- Lowered MIR ---------------------------------------------------------------------------


def parse_mir(text):
    """The functions of an exported lowered MIR module, in order (their numbers): each a dict
    of params and blocks by number, a block (phis, insts, term) with phis
    [(dst, [(pred, operand)])]."""
    body = text[text.index("functions := [") + len("functions := [") :]
    # A function the runtime does not reach keeps its place, with no blocks, so that positions
    # stay the compiler's function numbers that configurations name.
    parts = re.split(r"^\s*-- @([^\s:]+)(?::.*)?$", body, flags=re.MULTILINE)
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
    return cuts


# Whether to cut at join points too (certify.py's --joins): for a program whose paths share
# so many blocks that evaluating every path from a loop's cut point does not end in time.
JOINS = False

# Under JOINS, whether the stages between two MIR modules are cut at their join points too.
MIR_JOINS = True

# The calls the cut-point proofs are about, when not every call (certify.py's --exclude): the
# binders each segment proof assumes ("hyps", such as " (hsel0 : ... ≠ 0x...)"), their names
# ("names", " hsel0 ..."), the lines that add their mirror images ("prune"), the projections of
# "h : scope call" onto them ("given", a function of "h"), and the predicate ("term").
SCOPE = None

# The external functions outside the scope (certify.py's --exclude): their selectors and names.
# No cut point is placed in code only their calls reach, so no segment proof is written for
# it: under the scope's assumptions the dispatcher's test of their selector fails, and every
# segment that reaches it ends there.
EXCLUDED_SELECTORS = set()
EXCLUDED_FUNCTIONS = set()


def in_scope_mir(func):
    return func["name"] not in EXCLUDED_FUNCTIONS


def scoped_states(blocks, edges):
    """edges (evm_states') restricted to the configurations a call in scope reaches: an edge a
    dispatcher takes when the selector word equals an excluded selector (push SEL; eq; jumpi T)
    is dropped."""
    if not EXCLUDED_SELECTORS:
        return edges
    dropped = set()
    for idx, (_, instrs) in enumerate(blocks):
        for k in range(len(instrs) - 2):
            m = re.fullmatch(r"\.op \(\.stack \(\.push (0x[0-9a-f]+|\d+)\)\)", instrs[k])
            if (
                m
                and int(m[1], 0) in EXCLUDED_SELECTORS
                and instrs[k + 1] == ".op (.stack .eq)"
                and (j := re.fullmatch(r"\.jumpi (\d+)", instrs[k + 2]))
            ):
                dropped.add((idx, int(j[1])))
    start = (0, ())
    seen, work = {start}, [start]
    while work:
        st = work.pop()
        for t in edges[st]:
            if (st[0], t[0]) in dropped or t in seen:
                continue
            seen.add(t)
            work.append(t)
    return {st: [t for t in edges[st] if t in seen and (st[0], t[0]) not in dropped]
            for st in edges if st in seen}


def scope_parts():
    if SCOPE is None:
        return "", "", "", (lambda h: ""), "anyCall"
    return SCOPE["hyps"], SCOPE["names"], SCOPE["prune"], SCOPE["given"], SCOPE["term"]


# Relations per list, and segment proofs per module, under JOINS.
CHUNK = 16


# Under JOINS, the most configurations one segment evaluates before it is cut again.
SEGMENT = 24


def bound_segments(side, chosen, same_shapes, common):
    """Cuts the configuration trees of side (nodes, succ, node ids) so that no segment
    evaluates more than SEGMENT configurations: a configuration whose subtree is larger
    becomes a cut point when both programs keep its block under the same stack shapes. Cutting
    never changes what is proved, only where the segments end."""
    nodes, succ, node_ids, _ = side
    ok = {}

    def eligible(n):
        i = node_ids[n]
        if i not in ok:
            ok[i] = i in common and same_shapes(i)
        return ok[i]

    roots = [0] + [n for n in range(len(nodes)) if node_ids[n] in chosen]
    size = {}
    for root in roots:
        # Post-order without recursion: a configuration's size is one more than its
        # children's that the segment evaluates.
        stack = [(root, False)]
        while stack:
            n, done = stack.pop()
            if not done:
                stack.append((n, True))
                for c in succ[n]:
                    if node_ids[c] not in chosen and c not in size:
                        stack.append((c, False))
                continue
            s = 1 + sum(size.get(c, 0) for c in succ[n] if node_ids[c] not in chosen)
            if s > SEGMENT:
                # Cut the largest subtrees below first, so that a configuration with many small
                # subtrees (a dispatcher entering every function) is bounded too, roots
                # included; then the configuration itself, when it is still too large.
                for c in sorted(set(succ[n]), key=lambda c: -size.get(c, 0)):
                    if s <= SEGMENT:
                        break
                    if node_ids[c] not in chosen and size.get(c, 0) > 0 and eligible(c):
                        chosen.add(node_ids[c])
                        s -= size[c]
            if s > SEGMENT and n != root and eligible(n):
                chosen.add(node_ids[n])
                s = 0
            size[n] = s


def mir_block_cuts(funcs, cuts):
    """Under JOINS, every block of a function but its entry, besides its loop headers and join
    points, as candidates for the stack scheduling stage, whose one segment from the entry
    otherwise evaluates every function's body at once: the stage is then proved block by block,
    each candidate kept only where the schedule map relates it. Their relations are the join
    points' (the same state, the phis' inputs and the live values the same words)."""
    if not JOINS:
        return []
    have = {(f, h) for f, h, _, _ in cuts}
    out = []
    for f, fa in enumerate(funcs):
        if not in_scope_mir(fa):
            continue
        _, _, sa = mir_headers(fa)
        for h in sorted(fa["blocks"]):
            if (f, h) in have or h == 0:
                continue
            preds = sorted(b for b in sa if h in sa[b])
            if preds:
                out.append((f, h, preds, preds))
    return out


def mir_mir_block_cuts(a_funcs, b_funcs, cuts):
    """Under JOINS, the blocks two MIR modules cut at besides their loop headers: every block of a
    function but its entry that both keep, with the same phis and the same values live into it,
    so that the stage is proved block by block. A block where the pass changed which values are
    live (place-reads moving a read across it) relates nothing the other program has computed,
    and stays inside its segments. Their relations are the join points' (the same state, the
    phis' inputs and the live values the same words)."""
    if not JOINS:
        return []
    have = {(f, h) for f, h, _, _ in cuts}
    out = []
    for f, (fa, fb) in enumerate(zip(a_funcs, b_funcs)):
        if not in_scope_mir(fa):
            continue
        _, _, sa = mir_headers(fa)
        _, _, sb = mir_headers(fb)
        live_a, _ = mir_liveness(fa)
        live_b, _ = mir_liveness(fb)
        for h in sorted(set(fa["blocks"]) & set(fb["blocks"])):
            if (f, h) in have or h == 0:
                continue
            pa = sorted(b for b in sa if h in sa[b])
            pb = sorted(b for b in sb if h in sb[b])
            if not pa or not pb:
                continue
            da = sorted(d for d, _ in fa["blocks"][h][0])
            db = sorted(d for d, _ in fb["blocks"][h][0])
            if da != db or live_a[h] != live_b[h]:
                continue
            out.append((f, h, pa, pb))
    return out


def mir_join_cuts(a_funcs, b_funcs, cuts):
    """Join points of two MIR modules, cut besides their loop headers: a block of a function
    that more than one of its blocks jumps to, in both modules, with the same phis. Cutting them
    makes every segment a tree of blocks, so that no path is evaluated twice. Their relations
    are the headers' (the same state, the phis' inputs and the live values the same words),
    without the headers' candidate facts."""
    if not JOINS:
        return []
    have = {(f, h) for f, h, _, _ in cuts}
    out = []
    for f, (fa, fb) in enumerate(zip(a_funcs, b_funcs)):
        _, _, sa = mir_headers(fa)
        _, _, sb = mir_headers(fb)

        def preds(succ, h):
            return sorted(b for b in succ if h in succ[b])

        for h in sorted(set(fa["blocks"]) & set(fb["blocks"])):
            if (f, h) in have or h == 0:
                continue
            pa, pb = preds(sa, h), preds(sb, h)
            if len(pa) < 2 or len(pb) < 2:
                continue
            da = sorted(d for d, _ in fa["blocks"][h][0])
            db = sorted(d for d, _ in fb["blocks"][h][0])
            if da != db:
                continue
            out.append((f, h, pa, pb))
    return out


def tokens(s):
    """Split on spaces outside brackets."""
    out, depth, cur = [], 0, []
    for ch in s:
        if ch in "([{⟨":
            depth += 1
        elif ch in ")]}⟩":
            depth -= 1
        if ch == " " and depth == 0:
            if cur:
                out.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
    if cur:
        out.append("".join(cur))
    return out


def mir_calls(func):
    """The internal calls of a function: (block, ordinal in the block, callee, index)."""
    out = []
    for b, (_, insts, _) in sorted(func["blocks"].items()):
        k = 0
        for i, ins in enumerate(insts):
            if ins.startswith(".call "):
                out.append((b, k, int(tokens(ins)[2]), i))
                k += 1
    return out


def mir_contexts(pass_name, funcs, entry):
    """The call stacks each function runs under: tuples of call sites (caller, block, ordinal),
    innermost first. A tail call keeps the caller's; a call string longer than the module has
    functions is a recursion, which fails closed."""
    ctx = {entry: {()}}
    work = [(entry, ())]
    while work:
        f, c = work.pop()
        if f >= len(funcs):
            continue
        nexts = [(callee, ((f, b, k),) + c) for b, k, callee, _ in mir_calls(funcs[f])]
        for _, _, term in funcs[f]["blocks"].values():
            m = re.match(r"\.tailCall (\d+)", term)
            if m:
                nexts.append((int(m[1]), c))
        for g, d in nexts:
            # A call into an excluded function is outside the scope: no context of it, nor of
            # the functions only it calls, is related.
            if g < len(funcs) and not in_scope_mir(funcs[g]):
                continue
            if len(d) > len(funcs):
                raise Unsupported(
                    f"{pass_name}: a recursive call into @{funcs[g]['name']}"
                )
            if d not in ctx.setdefault(g, set()):
                ctx[g].add(d)
                work.append((g, d))
    return ctx


def scoped_blocks(funcs, entry, ctxs):
    """The blocks (function, block) a call in the scope reaches: those of the functions it runs
    (ctxs, from mir_contexts), where the entry's dispatch skips the cases of excluded selectors.
    A cut anywhere else starts a segment no call in the scope runs."""
    out = set()
    for f in ctxs:
        if f >= len(funcs):
            continue
        blocks = funcs[f]["blocks"]
        seen, work = {0}, [0]
        while work:
            b = work.pop()
            term = blocks[b][2]
            succ = set(mir_succ(term))
            if f == entry and EXCLUDED_SELECTORS and term.startswith(".switch"):
                cases = re.findall(r"\(\.const (0x[0-9a-f]+|\d+), (\d+)\)", term)
                taken = {int(t) for v, t in cases if int(v, 0) not in EXCLUDED_SELECTORS}
                dead = {int(t) for v, t in cases if int(v, 0) in EXCLUDED_SELECTORS}
                succ -= dead - taken
            for t in succ:
                if t in blocks and t not in seen:
                    seen.add(t)
                    work.append(t)
        out |= {(f, b) for b in seen}
    return out


def mir_entry(text):
    return int(re.search(r"entry := (\d+)", text)[1])


def call_site(func, b, k):
    """The k-th call of block b: its index and its tokens."""
    calls = [
        (i, ins)
        for i, ins in enumerate(func["blocks"][b][1])
        if ins.startswith(".call ")
    ]
    i, ins = calls[k]
    return i, tokens(ins)


def frame_term(funcs, site, env):
    """The return a call site pushes, with the caller's values env."""
    f, b, k = site
    _, insts, term = funcs[f]["blocks"][b]
    i, t = call_site(funcs[f], b, k)
    extras = t[4] if len(t) > 4 else "[]"
    publish = t[5] if len(t) > 5 else "none"
    rest = "[" + ", ".join(insts[i + 1 :]) + "]"
    return f"⟨{f}, {b}, {t[1]}, {rest}, {term}, {env}, {extras}, {publish}⟩"


def live_after_call(func, site):
    """The caller's values the code after a call reads, but for those the call binds."""
    _, b, k = site
    live, edge = mir_liveness(func)
    _, insts, term = func["blocks"][b]
    i, _ = call_site(func, b, k)
    out = set()
    for s in mir_succ(term):
        if s in func["blocks"]:
            out |= live[s] | edge(b, s)
    out |= mir_uses(term)
    for ins in reversed(insts[i + 1 :]):
        out -= mir_defs(ins)
        out |= mir_uses(ins)
    out -= mir_defs(insts[i])
    return out


def frame_rel(fa_list, fb_list, ctx):
    """The stacks of returns of a call string in both modules, the caller values' variables and
    the conditions that relate them: every value the caller reads after the call the same."""
    src, tgt, sv, tv, conds = [], [], [], [], []
    for n, site in enumerate(ctx):
        c, c2 = f"c{n}", f"c{n}'"
        src.append(frame_term(fa_list, site, c))
        tgt.append(frame_term(fb_list, site, c2))
        sv.append((c, "MIR.Lowered.Env"))
        tv.append((c2, "MIR.Lowered.Env"))
        f = site[0]
        live = live_after_call(fa_list[f], site) | live_after_call(fb_list[f], site)
        conds += [f"{c2} {v} = {c} {v}" for v in sorted(live)]
    return "[" + ", ".join(src) + "]", "[" + ", ".join(tgt) + "]", sv, tv, conds


def phi_input(func, h, pred, dst):
    """The operand phi dst of block h reads from pred."""
    for d, incoming in func["blocks"][h][0]:
        if d == dst:
            for p, op in incoming:
                if p == pred:
                    return op
    raise Unsupported(f"@{func['name']} bb{h}: phi {dst} has no input from bb{pred}")


def word_lit(op):
    return f"({op.split()[1]} : EvmRules.Word)"


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


def pair_module(a, b, cuts_a, cuts_b, rels, fuel_a, fuel_b, heartbeats, entry_reach=None, recdepth=None):
    """Lean (after the header) proving the stage from a["def"] to b["def"] through cut points:
    cuts_a/cuts_b are the cut tests' blocks, rels the relations at pairs of cut points, each a
    dict of src_vars, tgt_vars ([(name, type)]), src and tgt (configuration terms) and conds
    (propositions over the variables)."""
    A, B, p = a["def"], b["def"], b["def"]
    H, N, prune, given, term = scope_parts()
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
        "reach": entry_reach,
    }
    allrels = [entry] + rels
    # With JOINS, a stage has hundreds of relations: they are kept in lists of CHUNK, appended,
    # so that a relation's membership proof and each case split stay small.
    chunked = JOINS and len(allrels) > CHUNK
    groups = [allrels[i : i + CHUNK] for i in range(0, len(allrels), CHUNK)]

    def mem_of(n):
        if not chunked:
            return mem_proof(n)
        k, i = divmod(n, CHUNK)
        t = mem_proof(i)
        if k < len(groups) - 1:
            t = f"List.mem_append_left _ ({t})"
        for _ in range(k):
            t = f"List.mem_append_right _ ({t})"
        return t

    def body(r):
        vs = r["src_vars"] + r["tgt_vars"]
        parts = [f"x = {r['src']}", f"x' = {r['tgt']}"] + r["conds"]
        inner = " ∧ ".join(parts)
        return f"∃ {binder_text(vs)}, {inner}" if vs else inner

    if chunked:
        for k, g in enumerate(groups):
            out.append(
                f"/-- Relations {k * CHUNK} to {k * CHUNK + len(g) - 1}. -/\n"
                f"def {p}_rels{k} (call : Call) : List ({ca} → {cb} → Prop) :=\n  ["
                + ",\n   ".join(f"fun x x' => {body(r)}" for r in g)
                + "]\n\n"
            )
        joined = f"{p}_rels{len(groups) - 1} call"
        for k in reversed(range(len(groups) - 1)):
            joined = f"{p}_rels{k} call ++ ({joined})"
        out.append(
            f"/-- The relations at the first configurations and at each pair of cut points, from "
            f"what the compiler kept across the pass. -/\n"
            f"def {p}_rels (call : Call) : List ({ca} → {cb} → Prop) :=\n  {joined}\n\n"
        )
    else:
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
            proof = f"by\n  obtain {pat} := h\n  exact rel_mem ({mem_of(n)}) {wit}"
        else:
            if k == 1:
                wit = "⟨" + ", ".join([*names, "rfl", "h"]) + "⟩"
                proof = f"\n  rel_mem ({mem_of(n)}) {wit}"
            else:
                pat = "⟨" + ", ".join(f"h{i}" for i in range(k)) + "⟩"
                wit = (
                    "⟨" + ", ".join(names + ["rfl"] + [f"h{i}" for i in range(k)]) + "⟩"
                )
                proof = (
                    f"by\n  obtain {pat} := h\n  exact rel_mem ({mem_of(n)}) {wit}"
                )
        out.append(
            f"theorem {name} {{call : Call}} {src_vars} {{y : {cb}}}\n"
            f"    (h : {h}) :\n"
            f"    ∃ Q ∈ {p}_rels call, Q {r['src']} y := {proof}\n\n"
        )
    seg_rel = f"SegRel Agree (fun y y' => ∃ Q ∈ {p}_rels call, Q y y')"
    unf = ", ".join(unf_a + unf_b)
    segs = []
    def seg_mks(r):
        # The relation lemmas of the cut points the segment can reach, when the exporter's
        # configuration graph names them: a leaf tries only those.
        reach = r.get("reach")
        if reach is None:
            return mks
        return [m for m, q in zip(mks, rels) if q.get("block") in reach]

    if chunked:
        out.append("-- @@defs-end\n")
    seg_from = len(out)
    for n, r in enumerate(allrels):
        name = f"{p}_seg{n}"
        segs.append(name)
        vs = r["src_vars"] + r["tgt_vars"]
        hyps = " ".join(f"(h{i} : {c})" for i, c in enumerate(r["conds"]))
        args = " ".join(x for x in [binder_text(vs), hyps] if x)
        out.append(
            f"set_option maxHeartbeats {heartbeats} in\n"
            + (f"set_option maxRecDepth {recdepth} in\n" if recdepth else "")
            + f"theorem {name} (o : Oracle) (call : Call){H} {args} :\n"
            f"    ∃ k, {seg_rel}\n"
            f"      (seg ({step_term(a['kind'], A)}) {cut_a} k {r['src']})\n"
            f"      (seg ({step_term(b['kind'], B)}) {cut_b} {fuel_b} {r['tgt']}) := by\n"
            f"  refine ⟨{fuel_a}, ?_⟩\n"
            + prune
            + f"  sym_seg [{unf}] [{', '.join(seg_mks(r))}]\n\n"
            + ("\x00" if chunked else "")
        )
    def case_lines(start, group):
        lines = []
        for n, r in enumerate(group, start=start):
            vs = [v for v, _ in r["src_vars"] + r["tgt_vars"]]
            hs = [f"h{i}" for i in range(len(r["conds"]))]
            pat = "⟨" + ", ".join(vs + ["rfl", "rfl"] + hs) + "⟩"
            lines.append(
                f"  · obtain {pat} := holds\n"
                f"    exact {segs[n]} o call{N} {' '.join(vs + hs)}".rstrip()
            )
        return lines

    if chunked:
        # The segment proofs, then per list the lemma that covers it, each list's part of the
        # output marked for certify.py to put in a module of its own.
        seg_texts = "".join(out[seg_from:]).split("\x00")
        del out[seg_from:]
        for k, g in enumerate(groups):
            out.append("-- @@chunk\n")
            out.append("".join(seg_texts[k * CHUNK : k * CHUNK + len(g)]))
            out.append(
                f"theorem {p}_segments{k} (o : Oracle) (call : Call){H} :\n"
                f"    ∀ P ∈ {p}_rels{k} call, ∀ x x', P x x' → ∃ k, {seg_rel}\n"
                f"      (seg ({step_term(a['kind'], A)}) {cut_a} k x)\n"
                f"      (seg ({step_term(b['kind'], B)}) {cut_b} {fuel_b} x') := by\n"
                f"  intro P mem x x' holds\n"
                f"  simp only [{p}_rels{k}, List.mem_cons, List.not_mem_nil, or_false] at mem\n"
                f"  rcases mem with {' | '.join(['rfl'] * len(g))}\n"
                + "\n".join(case_lines(k * CHUNK, g))
                + "\n\n"
            )
        out.append("-- @@main\n")
        steps = []
        for k in range(len(groups) - 1):
            steps.append(
                f"  rcases List.mem_append.1 mem with mem | mem\n"
                f"  · exact {p}_segments{k} o call{N} P mem x x' holds\n"
            )
        out.append(
            f"theorem {p}_segments (o : Oracle) (call : Call){H} :\n"
            f"    ∀ P ∈ {p}_rels call, ∀ x x', P x x' → ∃ k, {seg_rel}\n"
            f"      (seg ({step_term(a['kind'], A)}) {cut_a} k x)\n"
            f"      (seg ({step_term(b['kind'], B)}) {cut_b} {fuel_b} x') := by\n"
            f"  intro P mem x x' holds\n"
            f"  unfold {p}_rels at mem\n"
            + "".join(steps)
            + f"  exact {p}_segments{len(groups) - 1} o call{N} P mem x x' holds\n\n"
        )
    else:
        out.append(
            f"theorem {p}_segments (o : Oracle) (call : Call){H} :\n"
            f"    ∀ P ∈ {p}_rels call, ∀ x x', P x x' → ∃ k, {seg_rel}\n"
            f"      (seg ({step_term(a['kind'], A)}) {cut_a} k x)\n"
            f"      (seg ({step_term(b['kind'], B)}) {cut_b} {fuel_b} x') := by\n"
            f"  intro P mem x x' holds\n"
            f"  simp only [{p}_rels, List.mem_cons, List.not_mem_nil, or_false] at mem\n"
            f"  rcases mem with {' | '.join(['rfl'] * len(allrels))}\n"
            + "\n".join(case_lines(0, allrels))
            + "\n\n"
        )
    out.append(
        f"/-- `{A}` and `{B}` simulated through their cut points on every call. -/\n"
        f"def {p}_sim (o : Oracle) :\n"
        f"    CutPoint.LayerSim {layer_term(a['kind'])} {layer_term(b['kind'])} o {A} {B} "
        f"{term} where\n"
        f"  σ := {ca}\n  τ := {cb}\n"
        f"  step call := {step_term(a['kind'], A)}\n"
        f"  step' call := {step_term(b['kind'], B)}\n"
        f"  init call := {init_term(a['kind'], A)}\n"
        f"  init' call := {init_term(b['kind'], B)}\n"
        f"  runs _ _ := rfl\n  runs' _ _ := rfl\n"
        f"  cut := {cut_a}\n  cut' := {cut_b}\n"
        f"  Rel call := fun x x' => ∃ P ∈ {p}_rels call, P x x'\n"
        f"  start _ _ := ⟨_, {mem_of(0)}, rfl, rfl⟩\n"
        f"  sim call {'h' if SCOPE else '_'} := CutPoint.Sim.of_each fun P m x x' hP =>\n"
        f"    match {p}_segments o call{given('h')} P m x x' hP with | ⟨k, s⟩ => ⟨k, {fuel_b}, s⟩\n\n"
    )
    out.append(
        f"/-- Every segment of `{B}` takes at most {fuel_b} steps: the bound that carries a loop's\n"
        f"ranking certificate on `{A}` to a step bound on `{B}` (`CutPoint.ends_within_sim_start`). -/\n"
        f"theorem {p}_bounded (o : Oracle) (call : Call){H} :\n"
        f"    CutPoint.BoundedSim ({step_term(a['kind'], A)}) {cut_a}\n"
        f"      ({step_term(b['kind'], B)}) {cut_b} Agree\n"
        f"      (fun x x' => ∃ P ∈ {p}_rels call, P x x') {fuel_b} :=\n"
        f"  fun x x' ⟨P, m, h⟩ => {p}_segments o call{N} P m x x' h\n\n"
    )
    return "".join(out)


def env_word(op, env):
    """An operand's word in env: a value's word or a constant."""
    if op.startswith(".value"):
        return f"{env} {op.split()[1]}"
    return word_lit(op)


def header_facts(func, h, pred, live, env):
    """The facts (assume) the code checks from header h, entered from pred, before it writes
    memory or storage, as equations of the configuration at the cut point: a phi as its input
    from pred, a load of a constant slot or offset as the state's word there, a value live into
    the header as env's. They are candidates the compiler's own facts give, not invariants
    written by hand: the cut-point proof checks that every segment establishes them again, and a
    fact that does not hold fails it."""
    blocks = func["blocks"]

    def word(op, terms):
        if op.startswith(".value"):
            v = int(op.split()[1])
            if v in terms:
                return terms[v]
            if v in live:
                return f"{env} {v}"
            return None
        return word_lit(op)

    # A phi's input from pred is a word of the configuration entering from pred.
    terms = {}
    for d, _ in blocks[h][0]:
        op = phi_input(func, h, pred, d)
        terms[d] = f"{env} {op.split()[1]}" if op.startswith(".value") else word_lit(op)
    facts = []

    def scan(b, terms, depth):
        # A block with phis joins paths whose words differ: stop there.
        if depth > 2 or (b != h and blocks[b][0]):
            return
        terms = dict(terms)
        for ins in blocks[b][1]:
            m = re.fullmatch(
                r"\.effect \(some (\d+)\) \.(sload|mload) \[\.const ([0-9a-fx]+)\]", ins
            )
            if m:
                c = int(m[3], 0)
                terms[int(m[1])] = (
                    f"s.storage ({c:#x} : EvmRules.Word)"
                    if m[2] == "sload"
                    else f"s.memory.load {c}"
                )
                continue
            m = re.fullmatch(
                r"\.assume false \.eq \[(\.\w+ [0-9a-fx]+), (\.\w+ [0-9a-fx]+)\]", ins
            )
            if m:
                x, y = word(m[1], terms), word(m[2], terms)
                if x is not None and y is not None and x != y:
                    # The load or the value on the left, which simp rewrites; never a constant.
                    if x.startswith("(") or (
                        y.startswith("s.") and not x.startswith("s.")
                    ):
                        x, y = y, x
                    fact = f"{x} = {y}"
                    if not x.startswith("(") and fact not in facts:
                        facts.append(fact)
                continue
            if re.match(r"\.(effect none \.(sstore|mstore|mstore8|tstore)|call)", ins):
                return
            for d in mir_defs(ins):
                terms.pop(d, None)
        for nxt in mir_succ(blocks[b][2]):
            if nxt in blocks and nxt != h:
                scan(nxt, terms, depth + 1)

    scan(h, terms, 0)
    return facts


def mir_mir_rels(pass_name, fa_list, fb_list, cuts, ctx_a=None, ctx_b=None, joins=()):
    """Relations between two MIR modules at their headers: the same state, the phis' inputs
    and every value live into the header the same words; for a header under calls, one relation
    per call string, with the returns each module pushes and the caller's values read after the
    call the same."""
    rels, tests = [], []
    ctx_a = ctx_a or {}
    ctx_b = ctx_b or {}
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
                feasible = True
                for d in dsts:
                    ia, ib = phi_input(fa, h, pa, d), phi_input(fb, h, pb, d)
                    if not ia.startswith(".value") and not ib.startswith(".value"):
                        # Two constants: the pair of edges either always binds the phi alike
                        # (nothing to relate) or never runs together (no relation at all).
                        if int(ia.split()[1], 0) != int(ib.split()[1], 0):
                            feasible = False
                        continue
                    conds.append(f"{env_word(ib, "env'")} = {env_word(ia, 'env')}")
                if not feasible:
                    continue
                if (f, h) not in joins:
                    conds += header_facts(fa, h, pa, set(live_a[h]), "env")
                    conds += header_facts(fb, h, pb, set(live_b[h]), "env'")
                ca, cb = ctx_a.get(f, {()}), ctx_b.get(f, {()})
                if ca != cb:
                    raise Unsupported(
                        f"{pass_name}: @{fa['name']} is called from {sorted(ca)} before and "
                        f"{sorted(cb)} after"
                    )
                for ctx in sorted(ca):
                    sa, sb, sv, tv, fconds = frame_rel(fa_list, fb_list, ctx)
                    rels.append(
                        {
                            "src_vars": [("env", "MIR.Lowered.Env")]
                            + sv
                            + [("s", "State")],
                            "tgt_vars": [("env'", "MIR.Lowered.Env")] + tv,
                            "src": f"({sa}, MIR.Lowered.Pos.enter {f} {pa} {h}, env, s)",
                            "tgt": f"({sb}, MIR.Lowered.Pos.enter {f} {pb} {h}, env', s)",
                            "conds": conds + fconds,
                        }
                    )
    return tests, tests, rels


def call_record(pass_name, funcs, sched_blocks, fmaps, site):
    """The scheduler's record of a call site (the k-th call of a MIR block): the label of the
    block its return address names, and the caller's words below the call's arguments."""
    f, b, k = site
    ids = [i for i, _ in sched_blocks]
    calls = [c for c in fmaps[f].get("calls", []) if c.get("block") == b]
    if k >= len(calls) or calls[k]["return"] not in ids:
        raise Unsupported(
            f"{pass_name}: the schedule map records no return of the call in "
            f"@{funcs[f]['name']} bb{b}"
        )
    return ids.index(calls[k]["return"]), calls[k]["below"]


def mir_evm_rels(pass_name, funcs, sched_blocks, schedule, cuts, ctxs=None):
    """Relations between the last MIR module and the scheduled EVM IR at its loop headers: the
    MIR header entered from a predecessor, and the scheduled block where its code starts, with
    the stack the scheduler recorded there (each value's word, a phi's word as its input from
    that predecessor), any words below the function's own, and the same state."""
    ids = [i for i, _ in sched_blocks]
    _, heights, dynamic = evm_cfg(sched_blocks)
    sched_states = list(evm_states(pass_name, sched_blocks)) if dynamic else []
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
        layout = bm["layout"]
        height = heights.get(e)
        if (ctxs or {}).get(f, {()}) != {()}:
            height = len(layout)
        hlist = [height]
        if height is None and dynamic and (ctxs or {}).get(f, {()}) == {()}:
            # Under dynamic jumps the CFG has no static heights: the block is related at each
            # height a configuration of the scheduled program enters it at, with the words
            # under its layout free (paths that join there may leave different junk below).
            hlist = sorted({len(st) for (i, st) in sched_states if i == e}) or [None]
        if any(x is None or x < len(layout) for x in hlist):
            raise Unsupported(
                f"{pass_name}: scheduled block bb{bm['start'][0]} has no static "
                "height that holds its layout"
            )
        contexts = sorted((ctxs or {}).get(f, {()}))
        if "ret" in layout and () in contexts:
            raise Unsupported(
                f"{pass_name}: a return address at the loop header of {f}"
            )
        tests_a.append((f, h))
        tests_b.append(e)
        dsts = {d for d, _ in funcs[f]["blocks"][h][0]}
        for ctx, pa, height in itertools.product(contexts, preds, hlist):
            frames, fvars = "[]", []
            labels, belows = [], []
            below = height - len(layout)
            for n, site in enumerate(ctx):
                label, below_n = call_record(
                    pass_name, funcs, sched_blocks, fmaps, site
                )
                labels.append(label)
                belows.append(below_n)
            if ctx:
                frames = (
                    "["
                    + ", ".join(
                        frame_term(funcs, site, f"c{n}") for n, site in enumerate(ctx)
                    )
                    + "]"
                )
                fvars = [(f"c{n}", "MIR.Lowered.Env") for n in range(len(ctx))]
                if "ret" not in layout:
                    raise Unsupported(
                        f"{pass_name}: no return address at the loop header of {f}"
                    )
            junk, words = [], []
            for loc in reversed(layout):
                if loc == "junk":
                    junk.append(f"j{len(junk)}")
                    words.append(junk[-1])
                elif loc == "ret":
                    words.append(f"(BitVec.ofNat 256 {labels[0]})")
                else:
                    v = int(loc[1:])
                    words.append(
                        env_word(phi_input(funcs[f], h, pa, v), "env")
                        if v in dsts
                        else f"env {v}"
                    )
            # Each caller's words below the call's arguments, as the scheduler recorded them: its
            # values as they were at the call, its own return address the next call's label.
            shape = [labels[0] if loc == "ret" else None for loc in reversed(layout)]
            for n, below_n in enumerate(belows):
                for loc in reversed(below_n):
                    if loc == "junk":
                        junk.append(f"j{len(junk)}")
                        words.append(junk[-1])
                        shape.append(None)
                    elif loc == "ret":
                        if n + 1 >= len(labels):
                            raise Unsupported(
                                f"{pass_name}: a return address below the outermost caller of {f}"
                            )
                        words.append(f"(BitVec.ofNat 256 {labels[n + 1]})")
                        shape.append(labels[n + 1])
                    else:
                        words.append(f"c{n} {int(loc[1:])}")
                        shape.append(None)
            if ctx:
                fits = [
                    st
                    for (i, st) in sched_states
                    if i == e
                    and len(st) >= len(shape)
                    and all(w == x for w, x in zip(st, shape) if x is not None)
                    and all(w is None for w in st[len(shape) :])
                ]
                if len(fits) != 1:
                    raise Unsupported(
                        f"{pass_name}: scheduled block bb{bm['start'][0]} is entered under the "
                        f"call string {ctx} with {len(fits)} stack shapes"
                    )
                below = len(fits[0]) - len(shape)
            for _ in range(below):
                junk.append(f"j{len(junk)}")
                words.append(junk[-1])
            rels.append(
                {
                    "src_vars": [("env", "MIR.Lowered.Env")] + fvars + [("s", "State")],
                    "tgt_vars": [(j, "EvmRules.Word") for j in junk],
                    "src": f"({frames}, MIR.Lowered.Pos.enter {f} {pa} {h}, env, s)",
                    "tgt": f"({e}, [{', '.join(words)}], s)",
                    "conds": [],
                }
            )
    if tests_b and dynamic:
        edges = evm_states(pass_name, sched_blocks)
        nodes = sorted(edges, key=lambda s: (s[0], len(s[1]), str(s[1])))
        ix = {s: n for n, s in enumerate(nodes)}
        succ = {ix[s]: sorted({ix[t] for t in edges[s]}) for s in nodes}
        cyc = cyclic_component(
            succ, {n for n, s in enumerate(nodes) if s[0] in tests_b}
        )
        if cyc:
            raise Unsupported(
                f"{pass_name}: a scheduled loop through "
                + ", ".join(f"bb{ids[nodes[n][0]]}" for n in cyc)
                + " passes no MIR header's block"
            )
    elif tests_b:
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
        ws = [(f"w{i}", "EvmRules.Word") for i in range(h)]
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


def stage_body(gen, a, b, heartbeats=2000000, recdepth=None):
    """The cut-point proof of the stage from a to b (manifest stages with def, file, kind and
    pass), after the module header, or raise Unsupported."""
    ta = (gen / a["file"]).read_text()
    tb = (gen / b["file"]).read_text()
    entry_reach = None

    def mir_fuel(funcs):
        # A segment may enter a callee's blocks once per call site.
        calls = sum(len(mir_calls(f)) for f in funcs)
        return sum(len(f["blocks"]) for f in funcs) * (calls + 1) + calls + 1

    if a["kind"] == "mir" and b["kind"] == "mir":
        fa, fb = parse_mir(ta), parse_mir(tb)
        ctx_a = mir_contexts(b["pass"], fa, mir_entry(ta))
        ctx_b = mir_contexts(b["pass"], fb, mir_entry(tb))
        live = scoped_blocks(fa, mir_entry(ta), ctx_a) & scoped_blocks(fb, mir_entry(tb), ctx_b)
        cuts = [c for c in mir_cuts(b["pass"], fa, fb) if c[:2] in live]
        joins = mir_mir_block_cuts(fa, fb, cuts) if MIR_JOINS else []
        joins = [j for j in joins if j[:2] in live]

        def callers_agree(j):
            # Under every call string, each caller reads the same values after the call in both
            # modules: a pass that changed which value the rest of a caller reads (place-reads
            # reusing an earlier load) relates nothing at a cut inside the callee, whose
            # relation equates the callers' values by number.
            for ctx in ctx_a.get(j[0], {()}) | ctx_b.get(j[0], {()}):
                for site in ctx:
                    if live_after_call(fa[site[0]], site) != live_after_call(fb[site[0]], site):
                        return False
            return True

        joins = [j for j in joins if callers_agree(j)]
        tests_a, tests_b, rels = mir_mir_rels(
            b["pass"], fa, fb, cuts + joins, ctx_a, ctx_b, {(f, h) for f, h, _, _ in joins}
        )
        fuel_a, fuel_b = mir_fuel(fa), mir_fuel(fb)
    elif a["kind"] == "mir":
        fa, sched = parse_mir(ta), parse_evm(tb)
        ctxs = mir_contexts(b["pass"], fa, mir_entry(ta))
        live = scoped_blocks(fa, mir_entry(ta), ctxs)
        cuts = [c for c in mir_cuts(b["pass"], fa, fa) if c[:2] in live]
        schedule_file = gen / "Schedule.json"
        if cuts and not schedule_file.exists():
            raise Unsupported(f"{b['pass']}: no Schedule.json from the exporter")
        # Join points are cut too under JOINS, which needs the schedule map as loop headers do.
        schedule = (
            json.loads(schedule_file.read_text())
            if cuts or (JOINS and schedule_file.exists())
            else {"functions": []}
        )
        # A join point is cut where the scheduler records the block's stack, alone it relates.
        fmaps = {fm["function"]: fm for fm in schedule["functions"]}

        def covered(j):
            # The scheduled stack at the block holds every value live into it: a value the
            # scheduler recomputes there instead (a CALLER read, a constant) is related by
            # nothing at the cut, so the block stays inside its segments.
            f, h = j[0], j[1]
            bm = next((x for x in fmaps.get(f, {}).get("blocks", []) if x["block"] == h), None)
            if bm is None:
                return False
            held = {int(x[1:]) for x in bm["layout"] if x.startswith("v")}
            live, _ = mir_liveness(fa[f])
            dsts = {d for d, _ in fa[f]["blocks"][h][0]}
            if not (live[h] | dsts) <= held:
                return False
            # And under every call string, each caller's words below the call hold every value
            # it reads after the call: one the scheduler recomputes after the return (a CALLER
            # read) is related by nothing at a cut inside the callee.
            for ctx in ctxs.get(f, {()}):
                for site in ctx:
                    try:
                        _, below = call_record(b["pass"], fa, sched, fmaps, site)
                    except Unsupported:
                        return False
                    kept = {int(x[1:]) for x in below if x.startswith("v")}
                    if not live_after_call(fa[site[0]], site) <= kept:
                        return False
            return True

        for j in mir_join_cuts(fa, fa, cuts) + mir_block_cuts(fa, cuts):
            if any(c[:2] == j[:2] for c in cuts) or j[:2] not in live or not covered(j):
                continue
            try:
                mir_evm_rels(b["pass"], fa, sched, schedule, cuts + [j], ctxs)
            except Unsupported:
                continue
            cuts.append(j)
        tests_a, tests_b, rels = mir_evm_rels(
            b["pass"], fa, sched, schedule, cuts, ctxs
        )
        fuel_a = mir_fuel(fa)
        _, _, dynamic = evm_cfg(sched)
        fuel_b = (len(evm_states(b["pass"], sched)) if dynamic else len(sched)) + 1
    else:
        ea, eb = parse_evm(ta), parse_evm(tb)
        cuts = evm_cuts(b["pass"], ea, eb)
        if isinstance(cuts, dict):
            (tests_a, tests_b), rels = cuts["tests"], cuts["rels"]
            fuel_a, fuel_b = cuts["fuel"]
            entry_reach = cuts.get("entry_reach")
        else:
            tests_a, tests_b, rels = evm_evm_rels(cuts)
            fuel_a, fuel_b = len(ea) + 1, len(eb) + 1
    return pair_module(a, b, tests_a, tests_b, rels, fuel_a, fuel_b, heartbeats, entry_reach, recdepth)


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
            "import W1Proofs.Segment\nimport W1Proofs.Stage\nimport SolarTV.Termination\n"
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
