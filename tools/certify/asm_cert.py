"""The certificate of the assembly checker for code with loops, inferred.

`infer(program_text, bytecode)` reads an exported EVM IR program (a stage file of
`-Zemit-lean-stages`) and its runtime bytecode and computes what
`SolidCore.Refinement.Solar.AssembleLoop.checkL` needs beside them: each labelled block's offset
(matched against the bytecode, which also decides which final branches solar's assembler turned
around), each block's lowest and highest entry stack height and the types of its entry words (a
flow analysis from block 0 that mirrors the checker's step), each block's budget (the items one
run of it executes), the memory bound, the blocks pushed labels name and whether the program
reads CALLER or hashes. It mirrors solidity-lean's `SolidCore.Refinement.Solar.AssembleInfer`.
Nothing here is trusted: the generated proof states the result as data and the kernel's
evaluation of the checker re-derives every claim, so a wrong inference only fails the proof.
"""

import re

STACK2 = {
    "add",
    "mul",
    "sub",
    "div",
    "mod",
    "lt",
    "gt",
    "slt",
    "sgt",
    "eq",
    "and",
    "or",
    "xor",
    "byte",
    "shl",
    "shr",
}


def split_top(text):
    """Split at commas outside brackets and parentheses."""
    parts, depth, cur = [], 0, []
    for ch in text:
        if ch in "([":
            depth += 1
        elif ch in ")]":
            depth -= 1
        if ch == "," and depth == 0:
            parts.append("".join(cur).strip())
            cur = []
        else:
            cur.append(ch)
    if "".join(cur).strip():
        parts.append("".join(cur).strip())
    return parts


def num(s):
    return int(s, 0)


def parse_instr(s):
    """One instruction as a tuple: ('push', v), ('dup', i), ('swap', i), ('stack', name),
    ('store',), ('load',), ('mem', name), ('effect', name, arity), ('log', n, arity),
    ('jump', t), ('jumpi', t), ('pushLabel', t), ('jumpDyn',), ('stop',), ('ret',),
    ('revert',), ('other', s)."""
    s = " ".join(s.split())
    m = re.fullmatch(r"\.op \(\.stack \(\.push (\S+)\)\)", s)
    if m:
        return ("push", num(m.group(1)))
    m = re.fullmatch(r"\.op \(\.stack \(\.(dup|swap) (\d+)\)\)", s)
    if m:
        return (m.group(1), int(m.group(2)))
    m = re.fullmatch(r"\.op \(\.stack \.(\w+)\)", s)
    if m:
        return ("stack", m.group(1))
    m = re.fullmatch(r"\.op \.(\w+)", s)
    if m:
        return (m.group(1),) if m.group(1) in ("store", "load") else ("mem", m.group(1))
    m = re.fullmatch(r"\.effect \(\.log (\d+)\) (\d+)", s)
    if m:
        return ("log", int(m.group(1)), int(m.group(2)))
    m = re.fullmatch(r'\.effect \(\.env "(\w+)"\) (\d+)', s)
    if m:
        return ("env", m.group(1), int(m.group(2)))
    m = re.fullmatch(r"\.effect \.(\w+) (\d+)", s)
    if m:
        return ("effect", m.group(1), int(m.group(2)))
    m = re.fullmatch(r"\.(jump|jumpi|pushLabel) (\d+)", s)
    if m:
        return (m.group(1), int(m.group(2)))
    m = re.fullmatch(r"\.(jumpDyn|stop|ret|revert)", s)
    if m:
        return (m.group(1),)
    return ("other", s)


def parse_program(text):
    """The blocks of an exported program, each a list of instructions."""
    parts = re.split(r"^\s*-- bb\d+\s*$", text, flags=re.MULTILINE)[1:]
    blocks = []
    for part in parts:
        start, depth = part.index("["), 0
        for end in range(start, len(part)):
            depth += {"[": 1, "]": -1}.get(part[end], 0)
            if depth == 0:
                break
        blocks.append([parse_instr(i) for i in split_top(part[start + 1 : end])])
    return blocks


def decode(code):
    """The code's instructions linearly: (pc, opcode, immediate)."""
    out, pc = [], 0
    while pc < len(code):
        op = code[pc]
        width = op - 0x5F if 0x60 <= op <= 0x7F else 0
        out.append((pc, op, int.from_bytes(code[pc + 1 : pc + 1 + width], "big")))
        pc += 1 + width
    return out


def lower_len(i):
    """The items of an instruction (`Assemble.lowerInstr`)."""
    return 2 if i[0] in ("jump", "jumpi") else 1


def pushed(i):
    """The label an instruction's items push, if any."""
    return i[1] if i[0] in ("jump", "jumpi", "pushLabel") else None


def layout(blocks, code):
    """The turned blocks and each labelled block's offset, matched against the code."""
    n = len(blocks)

    def final_branch(b):
        instrs = blocks[b]
        if (
            len(instrs) >= 2
            and instrs[-1][0] == "jump"
            and instrs[-2] == ("jumpi", b + 1)
        ):
            return instrs[-1][1]
        return None

    def dropped(b, k):
        i, last = blocks[b][k], k == len(blocks[b]) - 1
        return last and (
            (i[0] == "jump" and i[1] == b + 1) or (i[0] == "stop" and b + 1 == n)
        )

    candidates = [b for b in range(n) if final_branch(b) is not None]
    # Labels every layout pushes: all but a candidate's final jumpi (b + 1), which only an
    # unturned branch pushes.
    fixed = set()
    for b in range(n):
        for k, i in enumerate(blocks[b]):
            if dropped(b, k):
                continue
            if b in candidates and k == len(blocks[b]) - 2:
                continue
            t = pushed(i)
            if t is not None:
                fixed.add(t)
    ops = decode(code)
    pos, turned, offsets, labelled = 0, [], [0] * n, set(fixed)

    def take(count):
        nonlocal pos
        if pos + count > len(ops):
            raise SystemExit("error: the bytecode ends before the final EVM IR's items")
        pos += count

    for b in range(n):
        if b in labelled:
            if pos >= len(ops) or ops[pos][1] != 0x5B:
                raise SystemExit(f"error: block {b}'s label is not at a JUMPDEST")
            offsets[b] = ops[pos][0]
            take(1)
        instrs = blocks[b]
        for k, i in enumerate(instrs):
            if b in candidates and k == len(instrs) - 2:
                if pos < len(ops) and ops[pos][1] == 0x15:
                    turned.append(b)
                    take(3)
                else:
                    labelled.add(b + 1)
                    take(2)
                    take(2)
                break
            if dropped(b, k):
                break
            take(lower_len(i))
    return turned, offsets, labelled


def need_of(op):
    if op in STACK2:
        return (2, -1)
    if op in ("iszero", "not"):
        return (1, 0)
    if op == "calldatasize":
        return (0, 1)
    return None


def ty_at(ty, i):
    return ty[i] if i < len(ty) else None


def ty_swap(ty, i):
    size = max(len(ty), i + 2)
    out = []
    for j in range(size):
        if j == 0:
            out.append(ty_at(ty, i + 1))
        elif j == i + 1:
            out.append(ty_at(ty, 0))
        else:
            out.append(ty_at(ty, j))
    return out


def run_block(blocks, turned, rets, b, h, ty, dyn=False, bail=None):
    """The run of block b from height h and entry types ty (`AssembleInfer.go`): its edges
    (target, height, types), its items and the memory it reaches.

    With `dyn` it mirrors `AssembleCut.checkC` instead: memory operations at any offset and size,
    GAS and CALLDATACOPY, which run at the frame's own gas and need a memory bound of `DYN_TOP`.
    A static run that stops at memory it cannot bound appends the block to `bail`."""
    n = len(blocks)
    instrs = blocks[b]
    edges, items, top, known = [], 0, 0, []

    def kat(known, j):
        return known[j] if j < len(known) else None

    def two(known):
        return kat(known, 0) is not None and kat(known, 1) is not None

    def reach(known, top):
        if two(known) and known[1] != 0:
            return max(top, known[0] + known[1])
        return top

    for k, i in enumerate(instrs):
        last = k == len(instrs) - 1
        if b in turned and k == len(instrs) - 2:
            e = instrs[-1][1]
            edges += [(e, h - 1, ty[1:]), (b + 1, h - 1, ty[1:])]
            return edges, items + 3, top
        if last and (
            (i[0] == "jump" and i[1] == b + 1) or (i[0] == "stop" and b + 1 == n)
        ):
            if i[0] != "stop":
                edges.append((b + 1, h, ty))
            return edges, items, top
        tag = i[0]
        if tag in ("jump",):
            edges.append((i[1], h, ty))
            return edges, items + 2, top
        if tag == "jumpi":
            edges.append((i[1], h - 1, ty[1:]))
            items += 2
            h, known, ty = h - 1, [], ty[1:]
            continue
        items += 1
        if tag == "push":
            h, known, ty = h + 1, [i[1]] + known, [False] + ty
        elif tag == "dup":
            j = i[1]
            known = [kat(known, j)] + known
            h, ty = h + 1, [ty_at(ty, j)] + ty
        elif tag == "swap":
            known, ty = [], ty_swap(ty, i[1])
        elif tag == "stack" and i[1] == "pop":
            h, known, ty = h - 1, [], ty[1:]
        elif tag == "stack":
            nd = need_of(i[1])
            if nd is None:
                return edges, items, top
            h, known, ty = (
                max(h + nd[1], 0),
                [None] + known[nd[0] :],
                [False] + ty[nd[0] :],
            )
        elif tag == "store" and dyn:
            top = DYN_TOP
            h, known, ty = h - 2, [], ty[2:]
        elif tag == "load" and dyn:
            top = DYN_TOP
            known, ty = [], [False] + ty[1:]
        elif tag == "effect" and dyn and i[1] == "gas" and i[2] == 0:
            h, known, ty = h + 1, [], [False] + ty
        elif tag == "effect" and dyn and i[1] == "calldatacopy" and i[2] == 3:
            top = DYN_TOP
            h, known, ty = h - 3, [], ty[3:]
        elif tag == "store":
            if kat(known, 0) is None:
                if bail is not None:
                    bail.append(b)
                return edges, items, top
            top = max(top, known[0] + 32)  # ty: ignore[unsupported-operator]
            h, known, ty = h - 2, known[2:], ty[2:]
        elif tag == "load":
            if kat(known, 0) is None:
                if bail is not None:
                    bail.append(b)
                return edges, items, top
            top = max(top, known[0] + 32)  # ty: ignore[unsupported-operator]
            known, ty = [], [False] + ty[1:]
        elif tag == "effect" and i[1] == "caller":
            h, known, ty = h + 1, [None] + known, [False] + ty
        elif (tag == "effect" and i[1] in ("callvalue", "calldatasize", "address")) or (
            tag == "env" and i[1] in ENV_READS and i[2] == 0
        ):
            h, known, ty = h + 1, [], [False] + ty
        elif tag == "effect" and i[1] == "calldataload":
            known, ty = [None] + known[1:], [False] + ty[1:]
        elif tag == "effect" and i[1] == "sload":
            known, ty = [], [False] + ty[1:]
        elif tag == "effect" and i[1] == "sstore":
            h, known, ty = h - 2, [], ty[2:]
        elif tag == "effect" and i[1] == "keccak256":
            if dyn:
                top = DYN_TOP
            elif not two(known) and bail is not None:
                bail.append(b)
            top = reach(known, top) if not dyn else top
            h, known, ty = h - 1, [], [False] + ty[2:]
        elif tag == "log":
            if dyn:
                top = DYN_TOP
            elif not two(known) and bail is not None:
                bail.append(b)
            top = reach(known, top) if not dyn else top
            h, known, ty = h - (i[1] + 2), [], ty[i[1] + 2 :]
        elif tag == "pushLabel":
            h, known, ty = h + 1, [], [True] + ty
        elif tag == "jumpDyn":
            edges += [(t, h - 1, ty[1:]) for t in rets]
            return edges, items, top
        elif tag in ("ret", "revert"):
            if not dyn and not two(known) and bail is not None:
                bail.append(b)
            return edges, items - 1, top if dyn else reach(known, top)
        else:
            return edges, items - (1 if tag == "stop" else 0), top
        h = max(h, 0)
    edges.append((b + 1, h, ty))
    return edges, items, top


def meet(a, b):
    return [x if x == y else None for x, y in zip(a, b)]


class Unbounded(SystemExit):
    """The inference's refusal: the stack heights have no bound the checker accepts (a cycle that
    gains words). A SystemExit, so the command line exits 1 with the message as before; callers in
    the same process catch it by its type and record a refusal instead of exiting."""


def gaining_cycle(blocks, raised, rising, n):
    """The error for heights that do not settle: a cycle of edges (following `raised` back from a
    block the last round raised, one of RISING) along which the stack gains words, so no highest entry heights (`maxs`)
    satisfy the checker's bound on every edge. Through a dynamic jump this is the checker's
    abstraction of returns: it sends every `jumpDyn` to every return label, so a return can
    re-enter an earlier call's return site with the later call's words still on the stack."""
    b = rising[-1] if rising else None
    for _ in range(n):
        if b is None or raised[b] is None:
            break
        b = raised[b]
    cycle, seen = [], set()
    while b is not None and b not in seen:
        seen.add(b)
        cycle.append(b)
        b = raised[b]
    if b in cycle:
        cycle = cycle[cycle.index(b) :]
    cycle.reverse()
    path = " -> ".join(f"bb{c}" for c in [*cycle, *cycle[:1]])
    dyn = [c for c in cycle if blocks[c] and blocks[c][-1][0] == "jumpDyn"]
    why = (
        f"; {', '.join(f'bb{c}' for c in dyn)} returns by a dynamic jump, which the checker sends "
        "to every return label, so a return re-enters an earlier call's return site higher"
        if dyn
        else ""
    )
    return (
        "error: the stack heights of the final EVM IR have no bound the assembly checker accepts: "
        f"the cycle {path} gains stack words on every round{why}"
    )


def infer(program_text, code, dyn=False, bail=None, heads=None):
    """The certificate's fields and the highest entry heights, as Lean literals' data.

    HEADS maps head blocks to (highest entry height, entry height from below, entry types), which
    the run gives rather than the flow (`AssembleRec.checkR`): a recursive function's cut points,
    whose height grows with the depth of recursion, so no static height of theirs survives the
    recursive call. Only the highest height is the run's in the checker: the height from below and
    the types stay static, so every jump into a head must leave at least the head's height typed as
    the head's entry needs, or this raises SystemExit naming the first jump that does not."""
    heads = heads or {}
    blocks = parse_program(program_text)
    n = len(blocks)
    turned, offsets, labelled = layout(blocks, code)
    rets = []
    for instrs in blocks:
        for i in instrs:
            if i[0] == "pushLabel" and i[1] not in rets:
                rets.append(i[1])
    env = any(
        i[0] == "effect" and i[1] in ("caller", "keccak256")
        for bl in blocks
        for i in bl
    )
    reads = any(i[0] == "env" for bl in blocks for i in bl)
    entry = [None] * n
    entry[0] = (0, 0, [])
    for t, (high, need, ty) in heads.items():
        entry[t] = (need, high, ty)
    # raised[t]: the block whose edge last raised t's highest entry, to name a cycle that gains.
    raised = [None] * n
    # The needs and the types only fall and the highest entries are longest paths, so without a
    # cycle that gains height the rounds settle within n + 2 or so; 4 n + 8 is ample.
    for _ in range(4 * n + 8):
        changed, rising = False, []
        for b in range(n):
            current = entry[b]
            if current is None:
                continue
            need, high, ty = current
            edges, _, _ = run_block(blocks, turned, rets, b, need, ty[:need], dyn)
            for t, h, tty in edges:
                if t >= n:
                    continue
                if t in heads:
                    # A head's entry is fixed; the jump must meet it (checkR's jumpOkR).
                    hn, hty = heads[t][1], heads[t][2]
                    if h < hn or not ty_le(tty, hty):
                        raise SystemExit(
                            f"error: block {b} enters the recursive function's cut point at block {t} "
                            f"with {h} words typed {lean_tys(tty[:h])}, where it needs {hn} typed "
                            f"{lean_tys(hty)}: the frames below a recursive call are below every "
                            "static entry type, so the caller's return label is untyped where it "
                            "resumes (the checker has no stack relation over frames)"
                        )
                    continue
                new = (h, high + h - need, tty)
                old = entry[t]
                if old is not None:
                    new = (
                        min(old[0], new[0]),
                        max(old[1], new[1]),
                        meet(old[2], new[2]),
                    )
                if old is None or new[1] > old[1]:
                    raised[t] = b
                    rising.append(t)
                if new != old:
                    entry[t], changed = new, True
        if not changed:
            break
    else:
        raise Unbounded(gaining_cycle(blocks, raised, rising, n))
    entry = [e if e is not None else (0, 0, []) for e in entry]
    needs = [e[0] for e in entry]
    types = [(e[2][: e[0]] + [None] * e[0])[: e[0]] for e in entry]
    costs, top = [], 0
    for b in range(n):
        _, items, t = run_block(blocks, turned, rets, b, needs[b], types[b], dyn, bail)
        costs.append((1 if b in labelled else 0) + items)
        top = max(top, t)
    return {
        "offsets": offsets,
        "needs": needs,
        "top": top if top == DYN_TOP else (top + 31) // 32 * 32,
        "turned": turned,
        "costs": costs,
        "types": types,
        "rets": rets,
        "env": env,
        "reads": reads,
        "maxs": [e[1] for e in entry],
        "heads": sorted(heads),
    }


def ty_le(known, want):
    """`Assemble.tyLe`: each word WANT types, KNOWN types the same."""
    return all(
        w is None or (known[i] if i < len(known) else None) == w for i, w in enumerate(want)
    )


def lean_tys(ty):
    return "[" + ", ".join(lean_ty(t) for t in ty) + "]"


def lean_ty(t):
    return "none" if t is None else ("some true" if t else "some false")


def lean_cert(c):
    """The certificate as a Lean `Assemble.Cert` literal and the highest heights' list."""
    types = ",\n      ".join(
        "[" + ", ".join(lean_ty(t) for t in ty) + "]" for ty in c["types"]
    )
    cert = (
        "{ offsets := [" + ", ".join(hex(o) for o in c["offsets"]) + "],\n"
        "    needs := "
        + str(c["needs"])
        + ",\n"
        + (
            "    top := 2 ^ 140,\n"
            if c["top"] == DYN_TOP
            else f"    top := {c['top']},\n"
        )
        + "    turned := "
        + str(c["turned"])
        + ",\n"
        "    costs := " + str(c["costs"]) + ",\n"
        "    types := [" + types + "],\n"
        "    rets := " + str(c["rets"]) + ",\n"
        "    env := " + ("true" if c["env"] else "false") + ",\n"
        "    reads := " + ("true" if c["reads"] else "false") + " }"
    )
    return cert, str(c["maxs"])


# The environment reads the checker executes (`Assemble.envRead`): words of EthLib's block and
# transaction environment of the call, under an oracle that reads them so (`Assemble.EnvOk`).
ENV_READS = {
    "origin",
    "gasprice",
    "coinbase",
    "timestamp",
    "number",
    "gaslimit",
    "chainid",
}

FRAGMENT_EFFECTS = {
    "callvalue",
    "calldatasize",
    "caller",
    "address",
    "calldataload",
    "sload",
    "sstore",
    "keccak256",
}


# The memory bound of a certificate for memory at offsets the checker does not know: every memory a
# run below 2 ^ 256 gas pays for is at most 2 ^ 140 bytes (AssembleLoop.size_le_of_cost).
DYN_TOP = 2**140


def static_ok(program_text, code):
    """Whether the checker for constant offsets (checkL) can bound every memory access."""
    bail = []
    infer(program_text, code, bail=bail)
    return not bail


def outside(program_text, dyn=False):
    """The first instruction outside the fragment the assembly checker executes, or None; with
    `dyn`, outside the fragment of `AssembleCut.checkC` (GAS and CALLDATACOPY besides)."""
    for b, instrs in enumerate(parse_program(program_text)):
        for i in instrs:
            tag = i[0]
            ok = (
                tag
                in (
                    "push",
                    "store",
                    "load",
                    "jump",
                    "jumpi",
                    "pushLabel",
                    "jumpDyn",
                    "stop",
                    "ret",
                    "revert",
                )
                or (tag in ("dup", "swap") and i[1] < 16)
                or (tag == "stack" and (i[1] == "pop" or need_of(i[1]) is not None))
                or (tag == "effect" and i[1] in FRAGMENT_EFFECTS)
                or (
                    dyn
                    and tag == "effect"
                    and (i[1], i[2]) in (("gas", 0), ("calldatacopy", 3))
                )
                or (tag == "log" and i[1] <= 4)
                or (tag == "env" and i[1] in ENV_READS and i[2] == 0)
            )
            if not ok:
                return f"block {b}: {i}"
    return None


# The instructions a block of a recursive function may run besides its jumps and its call
# (AssembleRec.stackOnly): stack operations and ends.
FRAME_TAGS = ("push", "dup", "swap", "stack", "jump", "jumpi", "jumpDyn", "stop", "ret", "revert")


def call_of(instrs):
    """The call ending a block, pushLabel t; jump f, as (t, f) (AssembleRec.callOf), or None."""
    if len(instrs) >= 2 and instrs[-2][0] == "pushLabel" and instrs[-1][0] == "jump":
        return instrs[-2][1], instrs[-1][1]
    return None


def ty_pad(ty, n):
    """AssembleRec.tyPad: the types of the top n words, exactly n of them."""
    return [ty_at(ty, i) for i in range(n)]


def infer_frames(program_text, code, heads, entry):
    """The certificate of AssembleRec.checkF for a program with one recursive function, whose
    entry is block ENTRY: infer's fields with HEADS (the function's cut points, as rec_heads gives
    them) and the frames: the function's blocks and, for each call site's return label, the types
    of the words the call keeps below the callee and where the caller's own return label is among
    them. A return in the function's blocks goes to every call site's label, with the words that
    site kept below the results; a block of the function is entered only by a call, a return or a
    jump from the same frame at the height it needs. Raises SystemExit naming the first place the
    program leaves that shape. As infer, nothing here is trusted: the kernel's evaluation of
    checkF re-derives every claim."""
    blocks = parse_program(program_text)
    n = len(blocks)
    turned, offsets, labelled = layout(blocks, code)
    rets = []
    for instrs in blocks:
        for i in instrs:
            if i[0] == "pushLabel" and i[1] not in rets:
                rets.append(i[1])
    env = any(i[0] == "effect" and i[1] in ("caller", "keccak256") for bl in blocks for i in bl)
    reads = any(i[0] == "env" for bl in blocks for i in bl)

    def ends_dyn(b):
        return bool(blocks[b]) and blocks[b][-1][0] == "jumpDyn"

    def calls_entry(b):
        co = call_of(blocks[b])
        return co is not None and co[1] == entry

    def succ(b):
        edges, _, _ = run_block(blocks, turned, rets, b, 1000, [])
        if ends_dyn(b):
            edges = edges[: len(edges) - len(rets)]
        if call_of(blocks[b]) is not None:
            edges = edges[:-1]
        return [t for t, _, _ in edges if t < n]

    reach, todo = set(heads), list(heads)
    while todo:
        b = todo.pop()
        for t in succ(b):
            if t not in reach:
                reach.add(t)
                todo.append(t)
    frame = {b for b in reach if ends_dyn(b) or calls_entry(b)}
    changed = True
    while changed:
        changed = False
        for b in reach - frame:
            if any(t in frame for t in succ(b)):
                frame.add(b)
                changed = True
    for b in sorted(frame):
        for i in blocks[b][:-2] if call_of(blocks[b]) else blocks[b]:
            if i[0] not in FRAME_TAGS:
                raise SystemExit(
                    f"error: block {b} of the recursive function runs {i}, outside the stack "
                    "operations and jumps a block with a frame may run"
                )

    ent = [None] * n
    ent[0] = (0, 0, [])
    for t, (high, need, ty) in heads.items():
        ent[t] = (need, high, ty)
    hneed, hty = heads[entry][1], heads[entry][2]
    calls = {}
    for _ in range(4 * n + 8):
        changed = False
        # The call sites' kept words, from the types at each call.
        for b in range(n):
            if ent[b] is None or not calls_entry(b):
                continue
            need, high, ty = ent[b]
            edges, _, _ = run_block(blocks, turned, rets, b, need, ty[:need])
            f, h1, ty1 = edges[-1]
            t = call_of(blocks[b])[0]
            if h1 < hneed or not ty_le(ty1, hty):
                raise SystemExit(
                    f"error: block {b} calls the recursive function with {h1} words typed "
                    f"{lean_tys(ty1[:h1])}, where its entry needs {hneed} typed {lean_tys(hty)}"
                )
            keep = ty_pad(ty1[hneed:], h1 - hneed)
            pos = None
            if b in frame:
                labs = [j for j, x in enumerate(keep) if x is True]
                if len(labs) != 1:
                    raise SystemExit(
                        f"error: the recursive call in block {b} keeps {len(labs)} return labels "
                        "below its callee, where its frame has one"
                    )
                pos = labs[0]
            old = calls.get(t)
            new = (keep if old is None else meet(old[0], keep), pos)
            if old is not None and (len(old[0]) != len(keep) or old[1] != pos):
                raise SystemExit(f"error: two calls push the return label of block {t}")
            if new != old:
                calls[t], changed = new, True
        for b in range(n):
            current = ent[b]
            if current is None:
                continue
            need, high, ty = current
            edges, _, _ = run_block(blocks, turned, rets, b, need, ty[:need])
            if call_of(blocks[b]) is not None and call_of(blocks[b])[1] == entry:
                edges = edges[:-1]
            out = []
            if ends_dyn(b) and b in frame:
                h_dyn, ty_dyn = edges[-1][1], edges[-1][2]
                edges = edges[: len(edges) - len(rets)]
                for t, (keep, _) in calls.items():
                    out.append((t, h_dyn + len(keep), high + h_dyn - need,
                                ty_pad(ty_dyn, h_dyn) + keep))
            elif ends_dyn(b):
                for t in rets:
                    if t in frame:
                        raise SystemExit(
                            f"error: block {b}, outside the recursive function, returns to its "
                            f"block {t}"
                        )
            for t, h, tty in edges:
                if t < n and t in frame and b not in frame:
                    raise SystemExit(
                        f"error: block {b} enters the recursive function's block {t} other than "
                        "by a call"
                    )
                out.append((t, h, high + h - need, tty))
            for t, h, hi, tty in out:
                if t >= n:
                    continue
                if t in heads:
                    hn, hty2 = heads[t][1], heads[t][2]
                    if h < hn or not ty_le(tty, hty2):
                        raise SystemExit(
                            f"error: block {b} enters the recursive function's cut point at block "
                            f"{t} with {h} words typed {lean_tys(tty[:h])}, where it needs {hn} "
                            f"typed {lean_tys(hty2)}"
                        )
                    continue
                new = (h, hi, tty)
                old = ent[t]
                if old is not None:
                    new = (min(old[0], new[0]), max(old[1], new[1]), meet(old[2], new[2]))
                if new != old:
                    ent[t], changed = new, True
        if not changed:
            break
    else:
        raise SystemExit("error: the stack heights of the final EVM IR do not settle")
    ent = [e if e is not None else (0, 0, []) for e in ent]
    needs = [e[0] for e in ent]
    types = [(e[2][: e[0]] + [None] * e[0])[: e[0]] for e in ent]
    costs, top = [], 0
    for b in range(n):
        _, items, t = run_block(blocks, turned, rets, b, needs[b], types[b])
        costs.append((1 if b in labelled else 0) + items)
        top = max(top, t)
    for b in range(n):
        if ent[b][1] + costs[b] > 1000:
            raise SystemExit(
                f"error: block {b}'s highest entry ({ent[b][1]} words) and its budget "
                f"({costs[b]} items) exceed 1000 words: the stack bound of the recursion's cut "
                "points (rec_cuts.BOUND) leaves the block too little room"
            )
    return {
        "offsets": offsets,
        "needs": needs,
        "top": (top + 31) // 32 * 32,
        "turned": turned,
        "costs": costs,
        "types": types,
        "rets": rets,
        "env": env,
        "reads": reads,
        "maxs": [e[1] for e in ent],
        "heads": sorted(heads),
        "frames": {
            "blocks": sorted(frame),
            "calls": [(t, calls[t][0], calls[t][1]) for t in sorted(calls)],
        },
    }


def lean_frames(c):
    """The frames of infer_frames as a Lean AssembleRec.Frames literal."""
    fr = c["frames"]
    calls = ", ".join(
        f"({t}, {lean_tys(keep)}, {'none' if pos is None else f'some {pos}'})"
        for t, keep, pos in fr["calls"]
    )
    return "{ blocks := " + str(fr["blocks"]) + ", calls := [" + calls + "] }"
