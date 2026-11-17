#!/usr/bin/env python3
"""The rank that bounds a loop-free final EVM IR's run, for the caller route "any".

solidity-lean's Verify/Gas/FixedFuel/Acyclic.lean proves the fuel from the program alone:
`targets b instrs` are the blocks block `b` can continue to (none for a dynamic jump), and when
`rankOk p rk` holds (a rank for every block, every static target ranked below its block, a
target past the program ranked 0) every run of `p` ends within `rankFuel p rk` blocks under every
oracle (`ends_of_rank`). This module computes such a rank from the exported program, the longest
path to a halt; it is untrusted: the kernel evaluates `rankOk` on the exact export, so a wrong
rank or a mismatch with `targets` fails the module. A program with a loop or a dynamic jump has
none (`rank` returns None) and keeps the backend stages' fuel."""

import re

import asm_cert

CONTINUE = {
    "push",
    "dup",
    "swap",
    "stack",
    "store",
    "load",
    "mem",
    "effect",
    "env",
    "log",
    "pushLabel",
}
HALT = {"stop", "ret", "revert"}


def targets(b, instrs):
    """Acyclic.targets: the blocks after block B's INSTRS, or None for a dynamic jump or an
    instruction this reading does not know."""
    for i, ins in enumerate(instrs):
        k = ins[0]
        if k in CONTINUE:
            continue
        if k == "jump":
            return [ins[1]]
        if k == "jumpi":
            rest = targets(b, instrs[i + 1 :])
            return None if rest is None else [ins[1], *rest]
        if k in HALT:
            return []
        if k == "other":
            s = ins[1]
            if s == ".invalid":
                return []
            m = re.fullmatch(r"\.jumpTable \[([\d,\s]*)\]", s)
            if m:
                return [int(t) for t in m.group(1).replace(",", " ").split()]
            if s.startswith((".op ", ".effect ")):
                continue
        return None
    return [b + 1]


def rank(blocks):
    """The longest path from each block to a halt, or None with a loop or a dynamic jump."""
    n = len(blocks)
    succ = []
    for b, instrs in enumerate(blocks):
        ts = targets(b, instrs)
        if ts is None:
            return None
        succ.append(ts)
    rk, state = [0] * n, [0] * n

    def visit(b):
        stack = [(b, 0)]
        while stack:
            v, j = stack.pop()
            if j == 0:
                if state[v] == 2:
                    continue
                if state[v] == 1:
                    return False
                state[v] = 1
            ts = [t for t in succ[v] if t < n]
            if j < len(ts):
                stack.append((v, j + 1))
                t = ts[j]
                if state[t] == 1:
                    return False
                if state[t] == 0:
                    stack.append((t, 0))
                continue
            rk[v] = max((rk[t] + 1 if t < n else 1 for t in succ[v]), default=0)
            state[v] = 2
        return True

    for b in range(n):
        if state[b] == 0 and not visit(b):
            return None
    return rk


def fuel(rk):
    """Acyclic.rankFuel: block 0's rank plus one (1 for an empty program)."""
    return (rk[0] if rk else 0) + 1


def of_text(program_text):
    """The rank of an exported program's text, or None."""
    return rank(asm_cert.parse_program(program_text))
