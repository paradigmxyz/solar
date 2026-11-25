"""Check that every path from a dominating producer preserves a state read."""

from collections import defaultdict
from dataclasses import dataclass

from . import dead_ssa, phi_edges, scalar_reuse
from .scalar_checks import WORD, local, substitute

WORD_MODULUS = 2**256
MEMORY_LIMIT = 65536
HEAP_BASE = "entry_heap"
STORAGE_READS = {"sload", "tload"}
STORAGE_WRITES = {"sstore", "tstore"}
MEMORY_READS = {"mload", "msize", "keccak256", "returndatasize", "fmp"}
CONTEXT_READS = {
    "address",
    "origin",
    "caller",
    "callvalue",
    "calldataload",
    "calldatasize",
    "gasprice",
    "gas",
    "codesize",
    "extcodesize",
    "extcodehash",
    "balance",
    "selfbalance",
    "blockhash",
    "blobhash",
    "coinbase",
    "timestamp",
    "number",
    "prevrandao",
    "difficulty",
    "gaslimit",
    "chainid",
    "basefee",
    "blobbasefee",
}
LOGS = {"log0", "log1", "log2", "log3", "log4"}
COPIES = {"mcopy", "calldatacopy", "returndatacopy", "codecopy"}

AXIOMS = """
#print axioms MIR.StateReadReuse.warmSlot_contains
#print axioms MIR.StateReadReuse.warmSlot_preserved
#print axioms MIR.StateReadReuse.storage_read
#print axioms MIR.StateReadReuse.storage_written
#print axioms MIR.StateReadReuse.storage_write_other
#print axioms MIR.StateReadReuse.expanded_read
#print axioms MIR.StateReadReuse.load_word
#print axioms MIR.StateReadReuse.store_word
#print axioms MIR.StateReadReuse.word_write_disjoint
#print axioms MIR.StateReadReuse.relative_disjoint
#print axioms MIR.StateReadReuse.mapping_reuse
#print axioms MIR.StateReadReuse.mapping_sets
#print axioms MIR.StateReadReuse.distinct_offsets
#print axioms MIR.StateReadReuse.require_continues
#print axioms MIR.StateReadReuse.publish_scalar
"""


@dataclass(frozen=True)
class Site:
    producer: int
    consumer: int
    slice: int


def constant(operand):
    if operand["kind"] == "constant" and operand["type"].get("bits") == 256:
        return int(operand["value"])
    return None


def address(operand, bindings, heap=None):
    """Resolve value-preserving word casts and constant modular offsets."""
    delta = 0
    for _ in range(64):
        if (literal := constant(operand)) is not None:
            return None, (literal + delta) % WORD_MODULUS
        if heap is not None and operand["kind"] == "local" and operand["index"] in heap:
            offset = heap[operand["index"]] + delta
            if 0 <= offset <= MEMORY_LIMIT:
                return HEAP_BASE, offset
            break
        inst = bindings.get(operand["index"]) if operand["kind"] == "local" else None
        if inst is None or inst["type"].get("bits") != 256:
            break
        op = inst["operation"]
        name = op.get("name")
        args = op["args"]
        if (
            (op["kind"] == "word" or op["kind"] == "low_bits" and op["bits"] == 256)
            and len(args) == 1
            and args[0]["type"].get("bits") == 256
        ):
            (operand,) = args
            continue
        if (
            op["kind"] == "pointer_add"
            or op["kind"] == "opcode"
            and name in {"add", "sub"}
        ):
            left, right = args
            if name == "add" and constant(left) is not None:
                left, right = right, left
            if (offset := constant(right)) is not None:
                operand = left
                delta += -offset if name == "sub" else offset
                continue
        break
    return dead_ssa.stable(operand), delta % WORD_MODULUS


def disjoint_words(read, written, count, span):
    read_base, start = read
    write_base, destination = written
    if count == 0:
        return True
    if read_base == write_base and read_base in {None, HEAP_BASE}:
        return (
            start + span <= destination
            or count is not None
            and destination + count <= start
        )
    if read_base is None and write_base == HEAP_BASE:
        return start + span <= 128 + destination
    if read_base == HEAP_BASE and write_base is None:
        return count is not None and destination + count <= 128 + start
    if read_base == write_base:
        delta = (destination - start) % WORD_MODULUS
        return span <= delta <= MEMORY_LIMIT
    return False


def identify(inst, bindings, definitions, fork, heap):
    op = inst["operation"]
    name, args = op.get("name"), op["args"]
    if (
        op["kind"] == "mapping_slot"
        and inst["type"] == WORD
        and inst["result"] is not None
        and len(args) == 2
        and all(arg["type"] == WORD and arg["kind"] != "undefined" for arg in args)
    ):
        return (
            ("mapping", *(dead_ssa.stable(arg) for arg in args)),
            local(inst["result"], WORD),
            False,
        )
    if (
        op["kind"] != "opcode"
        or name not in (STORAGE_READS | STORAGE_WRITES | {"mload", "mstore"})
        or not definitions[name]["forks"][fork]["available"]
        or any(arg["kind"] == "undefined" for arg in args)
    ):
        return None
    write = name in STORAGE_WRITES or name == "mstore"
    if write:
        if inst["result"] is not None or len(args) != 2 or args[1]["type"] != WORD:
            return None
        stored = args[1]
    else:
        if inst["type"] != WORD or inst["result"] is None or len(args) != 1:
            return None
        stored = local(inst["result"], WORD)
    if args[0]["type"].get("bits") != 256:
        return None
    space = "memory" if name[0] == "m" else name[0] + "load"
    return (
        (space, address(args[0], bindings, None if space in STORAGE_READS else heap)),
        stored,
        write,
    )


def preserves_all(inst, definitions, fork, read_only):
    if inst is None or scalar_reuse.total(inst, definitions, fork):
        return True
    op = inst["operation"]
    kind = op["kind"]
    if kind in {
        "check",
        "checked",
        "require_literal",
        "require_short",
        "require_encode",
        "pointer_add",
        "word",
        "low_bits",
        "sign_extend",
        "ne",
        "free_pointer",
    }:
        return True
    if kind == "call":
        return op["function"] in read_only
    if kind == "layout":
        return op["query"] in {"immutable_read", "heap_floor", "frame_address"}
    return (
        kind == "opcode"
        and op["name"] in STORAGE_READS | MEMORY_READS | CONTEXT_READS | LOGS
    )


def message_output(op):
    if op["kind"] == "opcode":
        length = {"call": 7, "callcode": 7, "delegatecall": 6, "staticcall": 6}.get(
            op["name"]
        )
        if length == len(op["args"]):
            return op["args"][-2:]
    return None


def preserves(inst, fact, bindings, definitions, fork, read_only, heap):
    if preserves_all(inst, definitions, fork, read_only):
        return True
    op = inst["operation"]
    kind, args = op["kind"], op["args"]
    space = fact[0]
    storage = space in STORAGE_READS
    if kind in {"mapping_slot", "array_data_slot", "array_element_slot"}:
        return (
            storage or space == "memory" and disjoint_words(fact[1], (None, 0), 64, 32)
        )
    if kind == "allocate":
        return (
            storage
            or not op["zeroed"]
            and (space == "mapping" or disjoint_words(fact[1], (None, 64), 32, 32))
        )
    if kind != "opcode":
        return False
    name = op["name"]
    if name in STORAGE_WRITES:
        if not storage or space[0] != name[0]:
            return True
        written = address(args[0], bindings)
        old_base, old_offset = fact[1]
        new_base, new_offset = written
        return old_base == new_base and old_offset != new_offset
    if name in COPIES | {"mstore", "mstore8"}:
        if storage:
            return True
        count = constant(args[2]) if name in COPIES else 32 if name == "mstore" else 1
        read, width = ((None, 0), 64) if space == "mapping" else (fact[1], 32)
        return disjoint_words(read, address(args[0], bindings, heap), count, width)
    if heap is not None and not storage and (output := message_output(op)):
        destination, size = output
        read, width = ((None, 0), 64) if space == "mapping" else (fact[1], 32)
        return disjoint_words(
            read, address(destination, bindings, heap), constant(size), width
        )
    return False


def returning_actions(body):
    if (
        not body["blocks"]
        or len(body["returns"]) > 1
        or any(ty["kind"] not in {"word", "pointer"} for ty in body["returns"])
    ):
        return None
    successors = [phi_edges.targets(block["terminator"]) for block in body["blocks"]]
    reachable = scalar_reuse.reachable(successors)
    incoming = defaultdict(list)
    returns = []
    for index in reachable:
        kind = body["blocks"][index]["terminator"]["kind"]
        if kind == "tail_call":
            return None
        if kind == "return":
            returns.append(index)
        for target in successors[index]:
            incoming[target].append(index)
    can_return = set(returns)
    pending = list(returns)
    while pending:
        for parent in incoming[pending.pop()]:
            if parent not in can_return:
                can_return.add(parent)
                pending.append(parent)
    return (
        [
            inst
            for index in sorted(can_return)
            for inst in body["blocks"][index]["instructions"]
        ]
        if returns
        else None
    )


def read_only_functions(bodies, definitions, fork):
    remaining = {
        body["id"]: actions
        for body in bodies
        if (actions := returning_actions(body)) is not None
    }
    checked = set()
    while remaining:
        newly_checked = {
            id
            for id, actions in remaining.items()
            if all(preserves_all(inst, definitions, fork, checked) for inst in actions)
        }
        if not newly_checked:
            break
        checked.update(newly_checked)
        for id in newly_checked:
            del remaining[id]
    return checked


def checked_slice(producer, consumer, incoming, preserves_fact):
    """Extend backwards through all relevant instructions and CFG edges."""
    pending, seen = [consumer], {consumer}
    while pending:
        for before in incoming[pending.pop()]:
            if before == producer or before in seen:
                continue
            if before == 0 or len(seen) >= 4096 or not preserves_fact(before):
                return None
            seen.add(before)
            pending.append(before)
    # A repeated read preserves its own fact when encountered on a loop.
    return Site(producer, consumer, sum(1 << point for point in seen))


def normalize(body, definitions, fork, *, read_only=(), heap=None):
    blocks = body["blocks"]
    successors = [phi_edges.targets(block["terminator"]) for block in blocks]
    dom, _ = scalar_reuse.dominance(successors)
    entries, actions, points = [], {}, 0
    for block in blocks:
        points += 1  # Simultaneous phi assignment before the first instruction.
        entries.append(points - 1)
        for inst in block["instructions"]:
            actions[points] = inst
            points += 1
        points += 1  # The terminator carries every outgoing edge.
    edges, incoming = [], defaultdict(list)
    for index in dom:
        first = entries[index]
        finish = first + len(blocks[index]["instructions"]) + 1
        edges.extend((node, node + 1) for node in range(first, finish))
        edges.extend((finish, entries[target]) for target in successors[index])
    for before, after in edges:
        incoming[after].append(before)

    bindings = {
        inst["result"]: inst for inst in actions.values() if inst["result"] is not None
    }
    producers, aliases, reduced = defaultdict(list), {}, dict(actions)
    sites = []
    for index in sorted(dom, key=lambda block: (len(dom[block]), block)):
        start = entries[index] + 1
        for point in range(start, start + len(blocks[index]["instructions"])):
            inst = substitute(actions[point], aliases)
            reduced[point] = inst
            if inst["result"] is not None:
                bindings[inst["result"]] = inst
            identified = identify(inst, bindings, definitions, fork, heap)
            if identified is None:
                continue
            key, value, write = identified
            if not write:
                for owner, producer, prior in reversed(producers[key]):
                    if owner not in dom[index]:
                        continue
                    site = checked_slice(
                        producer,
                        point,
                        incoming,
                        lambda node, fact=key: preserves(
                            reduced.get(node),
                            fact,
                            bindings,
                            definitions,
                            fork,
                            read_only,
                            heap,
                        ),
                    )
                    if site is not None:
                        aliases[inst["result"]] = prior
                        sites.append((key[0], site))
                        del reduced[point]
                        break
                else:
                    producers[key].append((index, point, value))
            else:
                producers[key].append((index, point, value))
    result = [
        {
            **block,
            "instructions": [
                reduced[point]
                for point in range(
                    entries[index] + 1, entries[index] + 1 + len(block["instructions"])
                )
                if point in reduced
            ],
        }
        for index, block in enumerate(blocks)
    ]
    return substitute({**body, "blocks": result}, aliases), edges, sites


def certificate(name, edges, sites):
    if not sites:
        return ""
    graph = ",".join(f"({source},{target})" for source, target in edges)
    rows = ",".join(
        f"⟨{site.producer},{site.consumer},{site.slice:#x}⟩" for _, site in sites
    )
    return f"""
def {name}Edges : MIR.StateReadReuse.Edges := [{graph}]
def {name}Sites : List MIR.StateReadReuse.Site := [{rows}]
theorem {name}Checked :
    {name}Sites.all (MIR.StateReadReuse.check {name}Edges) = true := by decide +kernel
theorem {name}Sound {{α : Type}} (site : MIR.StateReadReuse.Site)
    (member : site ∈ {name}Sites) (step : Nat → Nat → α → α → Prop) (holds : α → Prop)
    (establish : ∀ {{target before after}}, step site.producer target before after → holds after)
    (preserve : ∀ {{source target before after}}, site.contains source = true →
      step source target before after → holds before → holds after)
    {{state : α}} (run : MIR.StateReadReuse.Runs {name}Edges step site.consumer state) :
    holds state :=
  MIR.StateReadReuse.invariant {name}Edges site
    (List.all_eq_true.mp {name}Checked _ member) step holds establish preserve run
#print axioms {name}Sound
"""
