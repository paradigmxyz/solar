"""Project the resolved transferFrom continuation onto its stack/state effects."""

from dataclasses import dataclass

from .common import Unsupported


def require(condition, reason):
    if not condition:
        raise Unsupported(f"transferFrom continuation: {reason}")


MASK = 2**256 - 1
HEAP = ("heap", 160)
DATA = ("heap", 192)
COLLECT = (
    ("store", 64, ("add", HEAP, ("and", ("add", "length", 63), MASK - 31))),
    ("store", HEAP, "length"),
    ("copy", DATA, 0, "length"),
)
FAILED = b"transferFrom reverted"
FALSE = b"transferFrom returned false"
ROUTES = ("child-failed", "empty", "short", "malformed", "false", "true")


def operation(op, a, b):
    if op == "add":
        if isinstance(a, int):
            a, b = b, a
        if isinstance(a, tuple) and a[0] == "heap" and isinstance(b, int):
            return "heap", a[1] + b
    if op == "and" and isinstance(a, int):
        a, b = b, a
    if op == "shl" and isinstance(a, int) and isinstance(b, int):
        return (b << a) & MASK
    return op, a, b


def error_stores(reason):
    return (
        ("store", 32, 32),
        ("store", 64, len(reason)),
        ("store", 96, int.from_bytes(reason.ljust(32, b"\0"), "big")),
        ("store", 0, 0x08C379A0),
    )


@dataclass
class Continuation:
    prices: list[int]
    copy_gas: int
    peak_depth: int
    instructions: set[int]
    relocations: dict[int, int]


def paths(decoded, start, stack, catalog, fork):
    """Follow both outcomes of each physical branch and bind its operands."""
    pending = [
        (start, ("success", *stack), (None,) * (len(stack) + 1), (), (), 0, frozenset())
    ]
    terminal = {}
    instructions, relocations = set(), {}
    depth = len(stack) + 1
    while pending:
        pc, stack, origins, guards, effects, price, seen = pending.pop()
        stack, origins = list(stack), list(origins)
        while True:
            require(pc not in seen and len(seen) < 96, "unexpected loop")
            seen = seen | {pc}
            instructions.add(pc)
            op, immediate = decoded[pc]
            info = catalog[op]["forks"][fork]
            require(info["available"], "unavailable opcode")
            price += info["base_gas"]
            next_pc = pc + 1
            if immediate is not None:
                stack.insert(0, immediate)
                origins.insert(0, pc)
                next_pc += int(op[4:])
            elif op.startswith("dup"):
                i = int(op[3:]) - 1
                stack.insert(0, stack[i])
                origins.insert(0, origins[i])
            elif op.startswith("swap"):
                i = int(op[4:])
                stack[i], stack[0] = stack[0], stack[i]
                origins[i], origins[0] = origins[0], origins[i]
            elif op == "pop":
                stack.pop(0)
                origins.pop(0)
            elif op == "returndatasize":
                stack.insert(0, "length")
                origins.insert(0, None)
            elif op in {"iszero", "not"}:
                a = stack.pop(0)
                origins[0] = None
                stack.insert(
                    0, MASK ^ a if op == "not" and isinstance(a, int) else (op, a)
                )
            elif op in {"add", "and", "shl", "lt", "sgt"}:
                stack[:2] = [operation(op, *stack[:2])]
                origins[:2] = [None]
            elif op == "mload":
                address = stack.pop(0)
                origins[0] = None
                effects += (("load", address),)
                stack.insert(0, "length" if address == HEAP else ("load", address))
            elif op == "mstore":
                effects += (("store", *stack[:2]),)
                del stack[:2]
                del origins[:2]
            elif op == "returndatacopy":
                effects += (("copy", *stack[:3]),)
                del stack[:3]
                del origins[:3]
            elif op in {"jump", "jumpi"}:
                target = stack.pop(0)
                producer = origins.pop(0)
                if op == "jump" and target == "return":
                    require(not stack, "success does not restore the caller stack")
                    exit_ = ("return",)
                    break
                require(
                    isinstance(target, int)
                    and decoded.get(target) == ("jumpdest", None),
                    "branch does not target JUMPDEST",
                )
                require(producer is not None, "dynamic local branch destination")
                relocations[producer] = target
                if op == "jumpi":
                    condition = stack.pop(0)
                    origins.pop(0)
                    require(len(guards) < 5, "too many branches")
                    pending.append(
                        (
                            target,
                            tuple(stack),
                            tuple(origins),
                            guards + ((condition, True),),
                            effects,
                            price,
                            seen,
                        )
                    )
                    guards += ((condition, False),)
                else:
                    next_pc = target
            elif op == "revert":
                exit_ = ("revert", *stack[:2])
                break
            else:
                require(op == "jumpdest", f"unsupported opcode {op}")
            depth = max(depth, len(stack))
            pc = next_pc
        require(guards not in terminal, "duplicate branch conditions")
        terminal[guards] = effects, exit_, price
    return terminal, depth, instructions, relocations


def extract(decoded, start, stack, catalog, fork):
    rows, depth, instructions, relocations = paths(decoded, start, stack, catalog, fork)
    failed = (("iszero", "success"), True)
    sent = (("iszero", "success"), False)
    empty = ("length", False)
    nonempty = ("length", True)
    short = (("sgt", 32, "length"), True)
    full = (("sgt", 32, "length"), False)
    boolean = ("load", DATA)
    invalid = (("lt", 1, boolean), True)
    canonical = (("lt", 1, boolean), False)
    result = ("iszero", ("lt", 0, boolean))
    read_head = (("load", HEAP),)
    read_value = (*read_head, ("load", DATA))
    reference = (
        ((failed,), error_stores(FAILED), ("revert", 28, 100)),
        ((sent, empty), (), ("return",)),
        ((sent, nonempty, short), read_head, ("revert", 0, 0)),
        ((sent, nonempty, full, invalid), read_value, ("revert", 0, 0)),
        (
            (sent, nonempty, full, canonical, (result, True)),
            (*read_value, *error_stores(FALSE)),
            ("revert", 28, 100),
        ),
        ((sent, nonempty, full, canonical, (result, False)), read_value, ("return",)),
    )
    require(len(rows) == len(reference), "unexpected exit count")
    prices = []
    for route, (guards, effects, exit_) in zip(ROUTES, reference, strict=True):
        row = rows.get(guards)
        require(
            row is not None and row[:2] == (COLLECT + effects, exit_),
            f"different return-buffer effect or exit on {route}",
        )
        prices.append(row[2])
    return Continuation(
        prices,
        catalog["returndatacopy"]["forks"][fork]["dynamic_gas"],
        depth,
        instructions,
        relocations,
    )


def replay_image(code, decoded, location, tail):
    """Keep the decoded helper instructions and relocate only local destinations."""
    entry, call, branch, failure, _ = location
    points = sorted({pc for pc in decoded if entry <= pc <= call} | tail.instructions)
    # A return stub exposes caller-visible active memory after the dynamic jump.
    # The final local stub stands in for the guard's unused no-code target.
    prologue = bytes.fromhex("60806040526001620200016202000062040000")
    return_pc = len(prologue) + 7
    stubs = bytes.fromhex("5b595ff35b5f5ffd")
    addresses, pc = {failure: return_pc + 4}, return_pc + len(stubs)
    for old in points:
        op, immediate = decoded[old]
        addresses[old] = pc
        pc += 1 + (int(op[4:]) if immediate is not None else 0)
    prologue += b"\x61" + return_pc.to_bytes(2, "big")
    prologue += b"\x61" + addresses[entry].to_bytes(2, "big") + b"\x56"
    image = bytearray(prologue + stubs)
    relocations = {**tail.relocations, branch: failure}
    for old in points:
        op, immediate = decoded[old]
        width = int(op[4:]) if immediate is not None else 0
        image.extend(code[old : old + 1])
        if old in relocations:
            image.extend(addresses[relocations[old]].to_bytes(width, "big"))
        else:
            image.extend(code[old + 1 : old + 1 + width])
    return bytes(image), addresses[call]
