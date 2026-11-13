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

    def reach(known, top):
        if len(known) >= 2 and known[1] != 0:
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
            h, known, ty = h + 1, [i[1]] + known[:1], [False] + ty
        elif tag == "dup":
            j = i[1]
            known = [known[j]] + known[:1] if j < len(known) else []
            h, ty = h + 1, [ty_at(ty, j)] + ty
        elif tag == "swap":
            known, ty = [], ty_swap(ty, i[1])
        elif tag == "stack" and i[1] == "pop":
            h, known, ty = h - 1, [], ty[1:]
        elif tag == "stack":
            nd = need_of(i[1])
            if nd is None:
                return edges, items, top
            h, known, ty = max(h + nd[1], 0), [], [False] + ty[nd[0] :]
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
            if not known:
                if bail is not None:
                    bail.append(b)
                return edges, items, top
            top = max(top, known[0] + 32)
            h, known, ty = h - 2, [], ty[2:]
        elif tag == "load":
            if not known:
                if bail is not None:
                    bail.append(b)
                return edges, items, top
            top = max(top, known[0] + 32)
            known, ty = [], [False] + ty[1:]
        elif (tag == "effect" and i[1] in ("callvalue", "calldatasize", "caller", "address")) or (
            tag == "env" and i[1] in ENV_READS and i[2] == 0
        ):
            h, known, ty = h + 1, [], [False] + ty
        elif tag == "effect" and i[1] in ("calldataload", "sload"):
            known, ty = [], [False] + ty[1:]
        elif tag == "effect" and i[1] == "sstore":
            h, known, ty = h - 2, [], ty[2:]
        elif tag == "effect" and i[1] == "keccak256":
            if dyn:
                top = DYN_TOP
            elif len(known) < 2 and bail is not None:
                bail.append(b)
            top = reach(known, top) if not dyn else top
            h, known, ty = h - 1, [], [False] + ty[2:]
        elif tag == "log":
            if dyn:
                top = DYN_TOP
            elif len(known) < 2 and bail is not None:
                bail.append(b)
            top = reach(known, top) if not dyn else top
            h, known, ty = h - (i[1] + 2), [], ty[i[1] + 2 :]
        elif tag == "pushLabel":
            h, known, ty = h + 1, [], [True] + ty
        elif tag == "jumpDyn":
            edges += [(t, h - 1, ty[1:]) for t in rets]
            return edges, items, top
        elif tag in ("ret", "revert"):
            if not dyn and len(known) < 2 and bail is not None:
                bail.append(b)
            return edges, items - 1, top if dyn else reach(known, top)
        else:
            return edges, items - (1 if tag == "stop" else 0), top
        h = max(h, 0)
    edges.append((b + 1, h, ty))
    return edges, items, top


def meet(a, b):
    return [x if x == y else None for x, y in zip(a, b)]


def infer(program_text, code, dyn=False, bail=None):
    """The certificate's fields and the highest entry heights, as Lean literals' data."""
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
    for _ in range(4 * n + 8):
        changed = False
        for b in range(n):
            current = entry[b]
            if current is None:
                continue
            need, high, ty = current
            edges, _, _ = run_block(blocks, turned, rets, b, need, ty[:need], dyn)
            for t, h, tty in edges:
                if t >= n:
                    continue
                new = (h, high + h - need, tty)
                old = entry[t]
                if old is not None:
                    new = (
                        min(old[0], new[0]),
                        max(old[1], new[1]),
                        meet(old[2], new[2]),
                    )
                if new != old:
                    entry[t], changed = new, True
        if not changed:
            break
    else:
        raise SystemExit("error: the stack heights of the final EVM IR do not settle")
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
    }


def lean_ty(t):
    return "none" if t is None else ("some true" if t else "some false")


def lean_cert(c):
    """The certificate as a Lean `Assemble.Cert` literal and the highest heights' list."""
    types = ",\n      ".join(
        "[" + ", ".join(lean_ty(t) for t in ty) + "]" for ty in c["types"]
    )
    cert = (
        "{ offsets := [" + ", ".join(hex(o) for o in c["offsets"]) + "],\n"
        "    needs := " + str(c["needs"]) + ",\n"
        + ("    top := 2 ^ 140,\n" if c["top"] == DYN_TOP else f"    top := {c['top']},\n")
        + "    turned := " + str(c["turned"]) + ",\n"
        "    costs := " + str(c["costs"]) + ",\n"
        "    types := [" + types + "],\n"
        "    rets := " + str(c["rets"]) + ",\n"
        "    env := " + ("true" if c["env"] else "false") + ",\n"
        "    reads := " + ("true" if c["reads"] else "false") + " }"
    )
    return cert, str(c["maxs"])


# The environment reads the checker executes (`Assemble.envRead`): words of EthLib's block and
# transaction environment of the call, under an oracle that reads them so (`Assemble.EnvOk`).
ENV_READS = {"origin", "gasprice", "coinbase", "timestamp", "number", "gaslimit", "chainid"}

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
                or (dyn and tag == "effect" and (i[1], i[2]) in (("gas", 0), ("calldatacopy", 3)))
                or (tag == "log" and i[1] <= 4)
                or (tag == "env" and i[1] in ENV_READS and i[2] == 0)
            )
            if not ok:
                return f"block {b}: {i}"
    return None
