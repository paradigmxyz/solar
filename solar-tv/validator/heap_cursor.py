"""Track the reserved FMP word from a valid logical MIR function entry."""

from collections import defaultdict, deque
from dataclasses import dataclass, replace

from . import phi_edges, scalar_reuse
from . import state_read_reuse as reuse

AXIOMS = """
#print axioms MIR.HeapCursor.entry_inhabited
#print axioms MIR.HeapCursor.floor_separates
#print axioms MIR.HeapCursor.offsets_separate
#print axioms MIR.HeapCursor.reservation_at
#print axioms MIR.HeapCursor.observe_at
#print axioms MIR.HeapCursor.observe_after_store
#print axioms MIR.HeapCursor.preserve_expansion
#print axioms MIR.HeapCursor.preserve_write
#print axioms MIR.HeapCursor.compose_offset
#print axioms MIR.HeapCursor.callee_entry
#print axioms MIR.HeapCursor.callee_return
#print axioms MIR.HeapCursor.raw_call_preserves_word
#print axioms MIR.HeapCursor.raw_load_at
#print axioms MIR.HeapCursor.raw_store_at
"""


@dataclass(frozen=True)
class Flow:
    addresses: dict
    cursors: list
    transfers: list
    observations: list
    dependencies: set
    summary: tuple | None
    raw_loads: list
    raw_stores: list
    call_points: dict


def ordered_blocks(body):
    successors = [phi_edges.targets(b["terminator"]) for b in body["blocks"]]
    if not successors:
        return [], {}
    reachable = scalar_reuse.reachable(successors)
    parents = defaultdict(set)
    for index in sorted(reachable):
        for other in successors[index]:
            parents[other].add(index)
    counts = {index: len(parents[index]) for index in sorted(reachable)}
    ready = deque(index for index, count in counts.items() if not count)
    order = []
    while ready:
        current = ready.popleft()
        order.append(current)
        for other in sorted(set(successors[current])):
            counts[other] -= 1
            if not counts[other]:
                ready.append(other)
    return (order, parents) if len(order) == len(reachable) else ([], {})


def output_write(op):
    if op["kind"] != "opcode":
        return None
    if op["name"] in reuse.COPIES and len(op["args"]) == 3:
        return op["args"][0], reuse.constant(op["args"][2])
    if op["name"] in {"mstore", "mstore8"} and len(op["args"]) == 2:
        return op["args"][0], 32 if op["name"] == "mstore" else 1
    if result := reuse.message_output(op):
        destination, size = result
        return destination, reuse.constant(size)
    return None


def advance(inst, bindings, addresses, definitions, fork, read_only, lookup):
    """Return a proved cursor bump, a result offset and the callee dependency."""
    op = inst["operation"]
    kind = op["kind"]
    if kind == "call" and op["function"] not in read_only:
        target = op["function"]
        known = lookup(target)
        if known is not None:
            bump, result = known
            return bump, result if inst["result"] is not None else None, target
        return None, None, None
    if (
        kind == "allocate"
        and not op["zeroed"]
        and len(op["args"]) == 1
        and inst["result"] is not None
        and (size := reuse.constant(op["args"][0])) is not None
    ):
        return size, 0, None
    if kind == "free_pointer":
        return 0, 0, None
    if (
        reuse.preserves_all(inst, definitions, fork, read_only)
        or kind in {"mapping_slot", "array_data_slot", "array_element_slot"}
        or kind == "opcode"
        and op["name"] in reuse.STORAGE_WRITES
    ):
        return 0, None, None
    if write := output_write(op):
        addr, size = write
        if reuse.disjoint_words(
            (None, 64), reuse.address(addr, bindings, addresses), size, 32
        ):
            return 0, None, None
    return None, None, None


def raw_cursor(inst, current, bindings, addresses, definitions, fork):
    if current is None:
        return None
    op = inst["operation"]
    if op["kind"] != "opcode" or op["name"] not in {"mload", "mstore"}:
        return None
    if reuse.identify(inst, bindings, definitions, fork, None) is None:
        return None
    target, *args = op["args"]
    if reuse.constant(target) != 64:
        return None
    if op["name"] == "mload":
        return 0, 0
    base, offset = reuse.address(args[0], bindings, addresses)
    if base == reuse.HEAP_BASE and current <= offset <= reuse.MEMORY_LIMIT:
        return offset - current, None
    return None


def analyze(body, definitions, fork, read_only, lookup):
    order, parents = ordered_blocks(body)
    if not order:
        return None
    bindings = {
        inst["result"]: inst
        for block in body["blocks"]
        for inst in block["instructions"]
        if inst["result"] is not None
    }
    addresses, outgoing = {}, {}
    dependencies, returns, cursors, transfers = set(), [], [0], []
    observations = []
    raw_loads, raw_stores = [], []
    call_points = {}

    def point(cursor):
        cursors.append(cursor)
        return len(cursors) - 1

    tailed = False
    for index in order:
        incoming = [outgoing[parent] for parent in sorted(parents[index])]
        values = {cursors[node] for node in incoming} if index else {0}
        current = values.pop() if len(values) == 1 else None
        start = point(current)
        for node in incoming if index else [0]:
            transfers.append((node, start, 0))
        for position, inst in enumerate(body["blocks"][index]["instructions"]):
            op = inst["operation"]
            if op["kind"] == "call":
                call_points[index, position] = start
            raw = raw_cursor(inst, current, bindings, addresses, definitions, fork)
            if raw is None:
                bump, result, callee = advance(
                    inst, bindings, addresses, definitions, fork, read_only, lookup
                )
            else:
                bump, result = raw
                callee = None
                if result is None:
                    raw_stores.append((start, current, bump))
                else:
                    raw_loads.append((start, current))
            if (
                current is not None
                and result is not None
                and inst["result"] is not None
                and current + result <= reuse.MEMORY_LIMIT
            ):
                addresses[inst["result"]] = current + result
                observations.append((start, result, current + result))
            updated = (
                current + bump if current is not None and bump is not None else None
            )
            if updated is not None and updated > reuse.MEMORY_LIMIT:
                updated = None
                bump = None
            if callee is not None and updated is not None:
                dependencies.add(callee)
            end = point(updated)
            transfers.append((start, end, bump))
            start, current = end, updated
        outgoing[index] = start
        term = body["blocks"][index]["terminator"]
        if term["kind"] == "tail_call":
            tailed = True
        if term["kind"] == "return":
            result = None
            if len(term["args"]) == 1:
                base, offset = reuse.address(term["args"][0], bindings, addresses)
                if base == reuse.HEAP_BASE:
                    result = offset
            returns.append((current, result))
    summary = None
    if (
        not tailed
        and not raw_stores
        and returns
        and returns[0][0] is not None
        and all(item == returns[0] for item in returns)
        and len(body["returns"]) <= 1
        and all(ty["kind"] in {"word", "pointer"} for ty in body["returns"])
    ):
        summary = returns[0]
    return Flow(
        addresses,
        cursors,
        transfers,
        observations,
        dependencies,
        summary,
        raw_loads,
        raw_stores,
        call_points,
    )


def with_callees(body, functions, definitions, fork, read_only):
    flows = {}

    def lookup(index):
        if index not in flows:
            # An in-progress summary is unusable on recursive paths.
            flows[index] = None
            if index in functions:
                flows[index] = analyze(
                    functions[index], definitions, fork, read_only, lookup
                )
        known = flows[index]
        return known.summary if known is not None else None

    root = body["id"]
    flows[root] = None
    selected = analyze(body, definitions, fork, read_only, lookup)
    flows[root] = selected
    if selected is None:
        return {}
    closure = {}
    pending = [root]
    while pending:
        index = pending.pop()
        if index in closure:
            continue
        closure[index] = flows[index]
        pending.extend(sorted(flows[index].dependencies))
    return closure


def callers(target, model, definitions, fork, read_only):
    """Bind every direct call from a public wrapper to its checked cursor."""
    functions = {body["id"]: body for body in model["functions"]}
    selectors = {route["id"]: route["selector"] for route in model["selectors"]}
    if target in selectors or target in {
        *model["runtime_entries"],
        model["constructor"],
        model["dispatch_entry"],
    }:
        return {}
    proved = {}
    for identity, body in functions.items():
        sites = []
        for index, block in enumerate(body["blocks"]):
            if block["terminator"].get("function") == target:
                return {}
            sites.extend(
                (index, position)
                for position, inst in enumerate(block["instructions"])
                if inst["operation"].get("function") == target
            )
        if not sites:
            continue
        if identity not in selectors or identity not in model["runtime_entries"]:
            return {}
        flows = with_callees(body, functions, definitions, fork, read_only)
        flow = flows.get(identity)
        if flow is None or flow.raw_stores:
            return {}
        calls = []
        for site in sites:
            point = flow.call_points.get(site)
            if point is None or flow.cursors[point] is None:
                return {}
            calls.append((*site, point, flow.cursors[point]))
        proved[body["return_buffer"]] = (identity, selectors[identity], calls, flows)
    return proved


def caller_certificate(name, flow, target, calls):
    # Points are numbered in topological order. Retain every edge into the
    # checked prefix, including all paths into each selected call.
    end = max(point for _, _, point, _ in calls)
    prefix = replace(
        flow,
        cursors=flow.cursors[: end + 1],
        transfers=[edge for edge in flow.transfers if edge[1] <= end],
        observations=[
            observation for observation in flow.observations if observation[0] <= end
        ],
        raw_loads=[load for load in flow.raw_loads if load[0] <= end],
    )
    code = certificate(name, prefix)
    for _, _, point, offset in calls:
        tag = f"{name}Entry{point}"
        code += f"""
theorem {tag} (step : MIR.HeapCursor.Transfer → MIR.State.Frame → MIR.State.Frame → Prop)
    (functions : MIR.State.Functions) (args : List MIR.State.Value)
    (floor : Word) (frame child : MIR.State.Frame)
    (start : ∀ state, MIR.HeapCursor.Runs {name}Edges step 0 state →
      MIR.HeapCursor.Entry state floor)
    (keeps : ∀ edge before after head delta, edge ∈ {name}Edges →
      MIR.HeapCursor.atPoint {name}Offsets edge.source = some head →
      edge.advance = some delta → step edge before after →
      MIR.HeapCursor.At before floor head → MIR.HeapCursor.At after floor (head + delta))
    (run : MIR.HeapCursor.Runs {name}Edges step {point} frame)
    (entered : MIR.State.start functions {target} args frame = .ok child) :
    MIR.HeapCursor.Entry child (floor + BitVec.ofNat 256 {offset}) :=
  MIR.HeapCursor.started_entry functions {target} args frame child floor {offset}
    ({name}Cursor step floor start keeps run (by decide +kernel)) entered
#print axioms {tag}
"""
    return code


def certificate(name, flow):
    def option(number):
        return "none" if number is None else f"(some {number})"

    offsets = ",".join(map(option, flow.cursors))
    edges = ",".join(
        f"⟨{source},{target},{option(advance)}⟩"
        for source, target, advance in flow.transfers
    )
    reads = ",".join(
        f"⟨{point},{displacement},{offset}⟩"
        for point, displacement, offset in flow.observations
    )
    predicate = "CursorWord" if flow.raw_stores else "At"
    invariant = (
        f"MIR.HeapCursor.invariant_for {name}Offsets {name}Edges (by decide +kernel) "
        "(by decide +kernel) step (fun frame pos => MIR.HeapCursor.CursorWord frame floor pos)"
        if flow.raw_stores
        else f"MIR.HeapCursor.invariant {name}Offsets {name}Edges (by decide +kernel) "
        "(by decide +kernel) step floor"
    )
    start = (
        "(fun frame run => MIR.HeapCursor.At.cursorWord (start frame run))"
        if flow.raw_stores
        else "start"
    )
    code = f"""
def {name}Offsets : MIR.HeapCursor.Offsets := #[{offsets}]
def {name}Edges : List MIR.HeapCursor.Transfer := [{edges}]
def {name}Reads : List MIR.HeapCursor.Observation := [{reads}]
theorem {name}Cursor (step : MIR.HeapCursor.Transfer → MIR.State.Frame → MIR.State.Frame → Prop)
    (floor : Word)
    (start : ∀ frame, MIR.HeapCursor.Runs {name}Edges step 0 frame → MIR.HeapCursor.Entry frame floor)
    (keeps : ∀ edge before after head delta, edge ∈ {name}Edges →
      MIR.HeapCursor.atPoint {name}Offsets edge.source = some head → edge.advance = some delta →
      step edge before after → MIR.HeapCursor.{predicate} before floor head →
        MIR.HeapCursor.{predicate} after floor (head + delta))
    {{point frame offset}} (run : MIR.HeapCursor.Runs {name}Edges step point frame)
    (known : MIR.HeapCursor.atPoint {name}Offsets point = some offset) :
    MIR.HeapCursor.{predicate} frame floor offset :=
  {invariant} {start} keeps run known
theorem {name}Origins (read : MIR.HeapCursor.Observation) (member : read ∈ {name}Reads) :
    ∃ origin, MIR.HeapCursor.atPoint {name}Offsets read.point = some origin ∧
      origin + read.displacement = read.offset :=
  MIR.HeapCursor.observation {name}Offsets read
    (List.all_eq_true.mp (show {name}Reads.all (MIR.HeapCursor.checkObservation {name}Offsets) = true from
      by decide +kernel) read member)
#print axioms {name}Cursor
#print axioms {name}Origins
"""
    for point, offset in flow.raw_loads:
        known = "live" if flow.raw_stores else "live.cursorWord"
        code += f"""
theorem {name}Load{point} (cfg : MIR.State.Config) (frame : MIR.State.Frame) (floor : Word)
    (live : MIR.HeapCursor.{predicate} frame floor {offset}) :
    MIR.State.memoryOperation cfg frame .load [64] =
      MIR.State.value frame (floor + BitVec.ofNat 256 {offset}) :=
  MIR.HeapCursor.raw_load_at cfg frame floor {offset} {known}
#print axioms {name}Load{point}
"""
    for point, offset, delta in flow.raw_stores:
        code += f"""
theorem {name}Store{point} (cfg : MIR.State.Config) (frame next : MIR.State.Frame) (floor : Word)
    (live : MIR.HeapCursor.CursorWord frame floor {offset})
    (written : MIR.State.memoryOperation cfg frame .store
      [64, floor + BitVec.ofNat 256 {offset} + BitVec.ofNat 256 {delta}] = MIR.State.effect next) :
    MIR.HeapCursor.CursorWord next floor {offset + delta} :=
  MIR.HeapCursor.raw_store_at cfg frame next floor {offset} {delta} live written
#print axioms {name}Store{point}
"""
    return code
