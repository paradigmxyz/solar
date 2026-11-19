"""Bind Morpho's transferFrom call sites and ABI return blocks to emitted PCs."""

import re
from dataclasses import dataclass

from . import regions
from .common import Unsupported


def require(condition, reason):
    if not condition:
        raise Unsupported(f"transferFrom callers: {reason}")


SELECTORS = {
    "supply": 0xA99AAD89,
    "repay": 0x20B76E81,
    "supplyCollateral": 0x238D6579,
    "liquidate": 0xD8EABCB8,
    "flashLoan": 0xE0232B42,
}


@dataclass
class Site:
    name: str
    selector: int
    call_block: str
    return_block: str
    call_block_pc: int
    return_pc: int
    live_words: tuple[int, ...]
    gas: int


def layouts(text, decoded):
    """Match the ordered finalized blocks to instructions and resolved labels."""
    blocks = regions.blocks(text)
    labels = list(blocks)
    emitted = {}
    for index, (label, instructions) in enumerate(blocks.items()):
        next_label = labels[index + 1] if index + 1 < len(labels) else None
        rows = []
        for line in instructions:
            op, _, arg = line.partition(" ")
            if op == "jump" and arg:
                if arg != next_label:
                    rows.extend([("push", arg), ("jump", None)])
            elif op == "jumpi" and arg:
                yes, no = arg.split(", ")
                if next_label == no:
                    rows.extend([("push", yes), ("jumpi", None)])
                elif next_label == yes:
                    rows.extend([("iszero", None), ("push", no), ("jumpi", None)])
                else:
                    rows.extend(
                        [("push", yes), ("jumpi", None), ("push", no), ("jump", None)]
                    )
            elif op == "push":
                rows.append((op, arg if arg.startswith("bb") else int(arg, 0)))
            elif op == "push_immutable":
                # The same compilation emits unlinked deployed-bytecode slots.
                rows.append(("push32", 0))
            elif op in {"dup", "swap"}:
                rows.append((op + arg, None))
            else:
                require(not arg, f"unmodeled finalized EVM IR instruction {line}")
                rows.append((op, None))
        emitted[label] = rows

    referenced = {
        arg
        for rows in emitted.values()
        for op, arg in rows
        if op == "push" and isinstance(arg, str)
    }
    instructions = iter(decoded.items())
    cursor = next(instructions, None)
    pcs, resolved = {}, []
    for label, rows in emitted.items():
        require(cursor is not None, f"missing bytecode at {label}")
        pcs[label] = cursor[0]
        if label in referenced:
            require(cursor[1] == ("jumpdest", None), f"missing JUMPDEST at {label}")
            cursor = next(instructions, None)
        for op, value in rows:
            require(cursor is not None, f"truncated bytecode at {label}")
            pc, (actual, immediate) = cursor
            if op == "push" and isinstance(value, str):
                require(actual.startswith("push"), f"missing PUSH at {pc}")
                resolved.append((value, immediate))
            else:
                require(
                    (actual == op or op == "push" and actual.startswith("push"))
                    and immediate == value,
                    f"final EVM IR disagrees with bytecode at {label}, PC {pc}",
                )
            cursor = next(instructions, None)
    require(
        all(pcs.get(target) == address for target, address in resolved),
        "final bytecode resolves a block to the wrong PC",
    )
    return pcs


def return_plan(decoded, start, scheduled, catalog, fork):
    """Project the concrete caller's STOP or two-word ABI publication."""
    # The scheduled return retains the caller's slots before the last
    # stack cleanup. The final optimizer may remove dead items at the call.
    writes = {}
    stack = list(range(scheduled))
    cost, peak = 0, scheduled
    terminal = None
    for pc, (op, imm) in decoded.items():
        if pc < start:
            continue
        if pc == start:
            require(op == "jumpdest", "caller return is not a JUMPDEST")
        cost += catalog[op]["forks"][fork]["base_gas"]
        if op == "jumpdest" and pc == start:
            continue
        if imm is not None:
            stack.insert(0, ("constant", imm))
        elif op == "pop":
            stack.pop(0)
        elif op.startswith("swap"):
            depth = int(op[4:])
            stack[0], stack[depth] = stack[depth], stack[0]
        elif op == "mstore":
            location, value = stack[:2]
            require(
                location in (("constant", 0), ("constant", 32)),
                "unexpected ABI store offset",
            )
            require(
                type(value) is int and location[1] not in writes,
                "unexpected ABI return value",
            )
            writes[location[1]] = value
            del stack[:2]
        elif op in {"return", "stop"}:
            terminal = op
            break
        else:
            raise Unsupported(f"unmodeled caller publication opcode {op}")
        peak = max(peak, len(stack))
        require(peak <= 1024, "caller return exceeds the stack limit")
    if terminal == "stop":
        require(not writes, "void return stores data")
        return (), cost, peak
    require(
        terminal == "return"
        and stack == [("constant", 0), ("constant", 64)]
        and set(writes) == {0, 32},
        "caller does not publish exactly two ABI words",
    )
    return (writes[0], writes[32]), cost, peak


def check_carried_amount(block, slot):
    """Track word identity from the final call preparation, ignoring fresh values."""
    stack = [object() for _ in range(16)]
    for line in block:
        op, _, operand = line.partition(" ")
        if op == "dup":
            stack.insert(0, stack[int(operand) - 1])
        elif op == "swap":
            index = int(operand)
            stack[0], stack[index] = stack[index], stack[0]
        elif op in {"push", "address", "caller"}:
            stack.insert(0, object())
        elif op == "mload":
            stack[0] = object()
        elif op in {"shl", "shr"}:
            stack[:2] = [object()]
        else:
            raise Unsupported(f"unmodeled carried-amount instruction {line}")
    require(
        stack[3] is stack[4 + slot],
        "caller does not retain the transfer amount in its ABI return slot",
    )


def extract(scheduled, finalized, decoded, helper, catalog, fork):
    pcs = layouts(finalized, decoded)
    reference = regions.blocks(scheduled)
    final = regions.blocks(finalized)

    def ownership(text):
        owners, entries = {}, {}
        current = None
        for line in text.splitlines():
            if match := re.match(r"^(bb\d+) \[entry\]:\s*; function (\w+)\(", line):
                label, current = match.groups()
                require(current not in entries, f"duplicate ABI entry {current}")
                entries[current] = label
            if match := re.match(r"^(bb\d+)(?: |:)", line):
                owners[match[1]] = current
        return owners, entries

    owners, _ = ownership(scheduled)
    final_owners, abi_entries = ownership(finalized)
    for name, selector in SELECTORS.items():
        # The actual dispatch branch must target the named optimized entry.
        route = [f"push {hex(selector)}", "eq", f"push {abi_entries[name]}", "jumpi"]
        require(
            any(
                route == block[i : i + 4]
                for block in final.values()
                for i in range(len(block) - 3)
            ),
            f"no matching dispatcher edge for {name}",
        )
    final_sites = {
        name: [
            label
            for label, block in final.items()
            if final_owners[label] == name and block[-1:] == [f"jump {helper}"]
        ]
        for name in SELECTORS
    }

    found = {}
    for label, block in reference.items():
        if block[-1:] != [f"jump {helper}"]:
            continue
        owner = owners[label]
        require(
            owner in SELECTORS and owner not in found,
            "unexpected scheduled helper caller",
        )
        require(len(final_sites[owner]) == 1, f"ambiguous final call site in {owner}")
        final_label = final_sites[owner][0]
        entry = final[final_label]
        require(
            entry[-1:] == [f"jump {helper}"]
            and len(entry) >= 2
            and entry[-2].startswith("push bb"),
            f"final call link changed in {owner}",
        )
        continuation = entry[-2][5:]
        # Named finalized blocks are preserved through return-block sharing.
        # The tuple publishers both consume five live words. A STOP is blind
        # to the residual caller stack, so its values need not be modeled.
        height = 0 if owner in {"supplyCollateral", "flashLoan"} else 6
        words, gas, _ = return_plan(decoded, pcs[continuation], height, catalog, fork)
        require(bool(words) == bool(height), f"return arity changed in {owner}")
        if words:
            check_carried_amount(entry[:-2], words[1 if owner == "liquidate" else 0])
        found[owner] = Site(
            owner,
            SELECTORS[owner],
            final_label,
            continuation,
            pcs[final_label],
            pcs[continuation],
            words,
            gas,
        )
    require(found.keys() == SELECTORS.keys(), "missing transferFrom caller")
    return [found[name] for name in SELECTORS]


def lean_suffixes(sites):
    return "\n".join(
        f"  | .{site.name} => ⟨"
        + (
            f".pair {site.live_words[0]} {site.live_words[1]}"
            if site.live_words
            else ".stop"
        )
        + f",{site.gas}⟩"
        for site in sites
    )


def lean_addresses(sites):
    return "\n".join(f"  | .{site.name} => {site.return_pc}" for site in sites)
