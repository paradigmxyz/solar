"""Check that a removed FMP store is private through the last frame return."""

from dataclasses import dataclass

from . import dead_ssa, scalar_reuse, state_read_reuse

MODULUS = state_read_reuse.WORD_MODULUS

AXIOMS = """
#print axioms MIR.PrivateCursor.skip_store
#print axioms MIR.PrivateCursor.access
#print axioms MIR.PrivateCursor.storage_read
#print axioms MIR.PrivateCursor.runs
#print axioms MIR.PrivateCursor.terminal
"""


@dataclass(frozen=True)
class Read:
    block: int
    instruction: int | None
    action: str
    origin: str
    offset: int | None
    size: int | None


def operations(body):
    for block in body["blocks"]:
        yield from (inst["operation"] for inst in block["instructions"])
        yield block["terminator"]


def bindings(body):
    return {
        inst["result"]: inst
        for block in body["blocks"]
        for inst in block["instructions"]
        if inst["result"] is not None
    }


def key(inst):
    return dead_ssa.stable(
        {"kind": "local", "index": inst["result"], "type": inst["type"]}
    )


def removed_store(before, after):
    old, new = before["blocks"], after["blocks"]
    if len(old) != len(new):
        return None
    candidates = [
        i
        for i, (left, right) in enumerate(zip(old, new, strict=True))
        if len(left["instructions"]) != len(right["instructions"])
    ]
    if len(candidates) != 1:
        return None
    (index,) = candidates
    actions = old[index]["instructions"]
    if len(actions) != len(new[index]["instructions"]) + 1:
        return None
    for point, inst in enumerate(actions):
        op = inst["operation"]
        if (
            inst["result"] is not None
            or op["kind"] != "opcode"
            or op["name"] != "mstore"
            or state_read_reuse.constant(op["args"][0]) != 64
            or op["args"][1]["kind"] == "undefined"
        ):
            continue
        trimmed = list(old)
        trimmed[index] = {
            **old[index],
            "instructions": actions[:point] + actions[point + 1 :],
        }
        if dead_ssa.same_definitions({**before, "blocks": trimmed}, after):
            return index, point
    return None


def range_read(block, point, action, offset, size, producers, origin):
    base, delta = state_read_reuse.address(offset, producers)
    extent = size if isinstance(size, int) else state_read_reuse.constant(size)
    if base is None and (delta >= 96 or extent is not None and delta + extent <= 64):
        return Read(block, point, action, "literal", delta, extent)
    if base == origin and (
        delta == 0 or action == "returndata" and delta == MODULUS - 32
    ):
        return Read(
            block, point, action, "floor" if delta == 0 else "header", None, extent
        )
    return None


def inventory(body, origin, definitions, fork, *, caller=False, internal=False):
    """Every surviving memory read is ranged; identical writes can overlap."""
    producers = bindings(body)
    reads = []
    for index, block in enumerate(body["blocks"]):
        for point, inst in enumerate(block["instructions"]):
            op = inst["operation"]
            if (
                caller
                and (index, point) == (0, 0)
                or scalar_reuse.total(inst, definitions, fork)
                or op["kind"] == "layout"
                and op["query"] == "heap_floor"
                and op["function"] == body["return_buffer"]
            ):
                continue
            name = op.get("name")
            if op["kind"] == "memory_zero" or (
                op["kind"] == "opcode" and name in {"sload", "mstore"}
            ):
                continue
            if op["kind"] != "opcode" or name not in {"mload", "keccak256"}:
                return None
            offset, *rest = op["args"]
            read = range_read(
                index, point, name, offset, rest[0] if rest else 32, producers, origin
            )
            if read is None:
                return None
            reads.append(read)
        term = block["terminator"]
        if term["kind"] in {"branch", "jump", "stop", "invalid"}:
            continue
        if (
            term["kind"] == "return"
            and internal
            and len(term["args"]) == 1
            and state_read_reuse.address(term["args"][0], producers) == (origin, 0)
        ):
            continue
        if term["kind"] in {"returndata", "revert"}:
            offset, size = term["args"]
            read = range_read(
                index,
                None,
                term["kind"],
                offset,
                size,
                producers,
                origin,
            )
            if read is not None:
                reads.append(read)
                continue
        return None
    return reads


def direct_callers(models, selected, definitions, fork):
    target = selected["id"]
    names = []
    inventories = {}
    first, second = models
    peers = {fn["id"]: fn for fn in second["functions"]}
    for body in first["functions"]:
        ref = [op for op in operations(body) if op.get("function") == target]
        if not ref:
            continue
        if (
            len(ref) != 1
            or ref[0]["kind"] != "call"
            or not body["blocks"]
            or not body["blocks"][0]["instructions"]
            or body != peers.get(body["id"])
            or body["id"] not in first["runtime_entries"]
            or body["returns"]
        ):
            return None
        call = body["blocks"][0]["instructions"][0]
        if call["operation"] != ref[0] or call["result"] is None:
            return None
        inventory_ = inventory(body, key(call), definitions, fork, caller=True)
        if inventory_ is None:
            return None
        names.append(body["return_buffer"])
        inventories[body["return_buffer"]] = inventory_
    for body in second["functions"]:
        if body["return_buffer"] not in names and any(
            op.get("function") == target for op in operations(body)
        ):
            return None
    return inventories if inventories else None


def project(before, after, models, definitions, fork):
    removed = removed_store(before, after)
    if removed is None:
        return None
    floor = [
        inst
        for inst in bindings(after).values()
        if inst["operation"]
        == {
            "kind": "layout",
            "query": "heap_floor",
            "function": after["return_buffer"],
            "args": [],
        }
    ]
    if len(floor) != 1:
        return None
    internal = bool(before["returns"])
    if internal:
        if any(
            before["id"]
            in (
                *module["runtime_entries"],
                module["constructor"],
                module["dispatch_entry"],
            )
            for module in models
        ):
            return None
        callers = direct_callers(models, before, definitions, fork)
        if callers is None:
            return None
    else:
        callers = {}
    reads = inventory(after, key(floor[0]), definitions, fork, internal=internal)
    if reads is None:
        return None
    return removed, {after["return_buffer"]: reads, **callers}


def certificate(i, inventories):
    source = AXIOMS
    for caller, reads in enumerate(inventories.values()):
        for point, read in enumerate(reads):
            tag = f"private{i}_{caller}_{point}"
            if read.origin == "literal":
                params = ""
                addr = f"{read.offset}#256"
                safe = (
                    "Or.inr (by decide +kernel)"
                    if read.offset >= 96
                    else ("Or.inl (by decide +kernel)")
                )
            else:
                params = "(floor : Word) (minimum : 128 ≤ floor.toNat) "
                addr = (
                    "floor"
                    if read.origin == "floor"
                    else (f"(floor + {(MODULUS - 32)}#256)")
                )
                prop = "floor_read" if read.origin == "floor" else "floor_header"
                safe = f"MIR.PrivateCursor.{prop} floor _ minimum"
            size = "size" if read.size is None else f"{read.size}#256"
            if read.size is None:
                params += "(size : Word) "
            if read.instruction is None:
                source += f"""
theorem {tag} {params}(left right : MIR.State.Frame) (reverted : Bool)
    (related : MIR.PrivateCursor.Frames left right) :
    MIR.PrivateCursor.endBytes {addr} {size} reverted left =
      MIR.PrivateCursor.endBytes {addr} {size} reverted right :=
  MIR.PrivateCursor.terminal left right {addr} {size} reverted related ({safe})
"""
            else:
                action = (
                    f"(.load {addr})"
                    if read.action == "mload"
                    else (f"(.hash {addr} {size})")
                )
                source += f"""
theorem {tag} {params}(cfg : MIR.State.Config) (left right : MIR.State.Frame)
    (related : MIR.PrivateCursor.Frames left right) :
    MIR.PrivateCursor.Results (MIR.PrivateCursor.Access.run {action} cfg left)
      (MIR.PrivateCursor.Access.run {action} cfg right) :=
  MIR.PrivateCursor.access cfg left right {action} related ({safe})
"""
            source += f"#print axioms {tag}\n"
    return source
