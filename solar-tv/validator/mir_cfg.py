"""Propose path and live-local correspondence for independently exported MIR."""

import hashlib
import tempfile
from collections import deque
from itertools import batched
from pathlib import Path

from . import audit, dead_ssa, entry_proof, phase_carriers, prune, type_prune
from .common import Program, Unsupported


def import_graph(body):
    """Keep predecessor-selected phi inputs as one simultaneous action."""
    blocks = body["blocks"]
    points, nodes = {(0, None, -1): 0}, []
    pending = deque(points)

    def point(block, predecessor, position=-1):
        if not blocks[block]["phis"] or position >= 0:
            predecessor = None
        location = (block, predecessor, position)
        if location not in points:
            if len(points) >= 4096:
                raise Unsupported("MIR control-flow graph exceeds 4096 points")
            points[location] = len(points)
            pending.append(location)
        return points[location]

    while pending:
        index, predecessor, position = pending.popleft()
        block = blocks[index]
        if position == -1:
            phis = []
            for phi in block["phis"]:
                choices = [
                    value for source, value in phi["incoming"] if source == predecessor
                ]
                if len(choices) != 1:
                    raise Unsupported("MIR control flow has an unresolved phi input")
                phis.append({**phi, "incoming": choices[0]})
            next = point(index, None, 0)
            nodes.append(
                ("action", phis, [next]) if phis else ("forward", None, [next])
            )
        elif position < len(block["instructions"]):
            inst = block["instructions"][position]
            nodes.append(("action", inst, [point(index, None, position + 1)]))
        else:
            term = block["terminator"]
            match term["kind"]:
                case "jump":
                    nodes.append(("forward", None, [point(term["target"], index)]))
                case "branch":
                    keys = {k: v for k, v in term.items() if k not in {"then", "else"}}
                    edges = [point(term[k], index) for k in ("then", "else")]
                    (condition,) = term["args"]
                    if (
                        condition["kind"] == "constant"
                        and condition["type"]
                        == {"kind": "word", "bits": 1, "name": "i1"}
                        and condition["value"] in {"0", "1"}
                    ):
                        nodes.append(("literal", condition["value"] == "1", edges))
                    else:
                        nodes.append(("select", keys, edges))
                case "switch":
                    keys = {**term, "cases": [key for key, _ in term["cases"]]}
                    del keys["default"]
                    targets = [
                        point(term["default"], index),
                        *(point(dest, index) for _, dest in term["cases"]),
                    ]
                    nodes.append(("select", keys, targets))
                case _:
                    nodes.append(("terminal", term, []))
    return nodes


def flat_insert(data):
    if (
        not isinstance(data, dict)
        or data.get("operation", {}).get("kind") != "insert_value"
    ):
        return None
    op = data["operation"]
    ty, index = op["aggregate_type"], op["index"]
    base, value = op["args"]
    if (
        ty["kind"] == "product"
        and data["type"] == base["type"] == ty
        and all(field["kind"] == "word" for field in ty["fields"])
        and 0 <= index < len(ty["fields"])
        and value["type"] == ty["fields"][index]
        and base["kind"] in {"undefined", "local"}
    ):
        return base, value, len(ty["fields"]), index
    return None


def reads(data):
    insert = flat_insert(data)
    if insert is not None and insert[0]["kind"] == "undefined":
        return dead_ssa.reads(insert[1])
    return dead_ssa.reads(data)


def product_certificate(name, nodes):
    """Bind the checked field masks to inserts and every other local read."""
    seeds, children = [], {}
    for kind, data, _ in nodes:
        insert = flat_insert(data) if kind == "action" else None
        if insert is not None:
            base = insert[0]
            if base["kind"] == "undefined":
                seeds.append((data, insert))
            else:
                children.setdefault(base["index"], []).append((data, insert))
    if not seeds:
        return ""
    inserts, constructing, pending = [], set(), deque(seeds)
    while pending:
        data, (base, _, arity, index) = pending.popleft()
        result = data["result"]
        if result in constructing:
            raise Unsupported("MIR product construction repeats a result")
        constructing.add(result)
        source = "none" if base["kind"] == "undefined" else f"some {base['index']}"
        inserts.append(f"⟨{result},{source},{arity},{index}⟩")
        pending.extend(children.get(result, ()))
    observed = set()
    for kind, data, _ in nodes:
        if kind in {"forward", "literal"}:
            continue
        if (
            kind == "action"
            and isinstance(data, dict)
            and data["result"] in constructing
        ):
            # An insert copies its base without observing undefined fields.
            # Its scalar operand must be initialized.
            observed.update(dead_ssa.reads(data["operation"]["args"][1]))
        else:
            observed.update(reads(data))
    return f"""
theorem {name} : MIR.ProductInit.safe [{",".join(inserts)}] {sorted(observed)} = true :=
  by decide +kernel
#print axioms {name}
"""


def writes(data):
    if isinstance(data, list):
        return [phi["result"] for phi in data]
    result = data.get("result")
    return [] if result is None else [result]


def word_key(data):
    if not isinstance(data, dict) or "operation" not in data:
        return None
    operation = data["operation"]
    op = operation.get("name")
    if operation["kind"] != "opcode" or op not in {"add", "sub"}:
        return None
    operands = operation["args"]
    word = {"kind": "word", "bits": 256, "name": "i256"}
    if (
        len(operands) != 2
        or operands[0]["kind"] != "local"
        or operands[1]["kind"] != "constant"
        or any(value["type"] != word for value in operands)
        or data["type"] != word
        or data["result"] is None
    ):
        return None
    value = int(operands[1]["value"])
    if not 0 <= value < 1 << 256:
        raise Unsupported("MIR word constant is outside i256")
    return op, value


def action_signature(data):
    key = word_key(data)
    if key is None:
        return dead_ssa.signature(data)
    op, value = key
    return ("word-add", value if op == "add" else -value % (1 << 256))


def pairing(old, new):
    points = [(0, 0)]
    locations = {(0, 0): 0}
    targets, live, definitions = [], [], []
    jump_limit = 1
    word_rewrites = 0

    def index(pair):
        if pair not in locations:
            if len(points) >= 4096:
                raise Unsupported("paired MIR control flow exceeds 4096 points")
            locations[pair] = len(points)
            points.append(pair)
        return locations[pair]

    def follow(nodes, point):
        hops = 1
        while nodes[point][0] in {"forward", "literal"}:
            kind, condition, edges = nodes[point]
            point = edges[int(kind == "literal" and not condition)]
            hops += 1
            if hops > len(nodes):
                raise Unsupported(
                    "MIR CFG contains a cycle of only unconditional transfers"
                )
        return hops, nodes[point]

    for before, after in points:
        left_hops, (left_kind, left, left_edges) = follow(old, before)
        right_hops, (right_kind, right, right_edges) = follow(new, after)
        jump_limit = max(jump_limit, left_hops, right_hops)
        a, b = writes(left), writes(right)
        if (
            left_kind != right_kind
            or len(a) != len(b)
            or len(left_edges) != len(right_edges)
            or action_signature(left) != action_signature(right)
        ):
            raise Unsupported("MIR control-flow comparison changed a typed action")
        word_rewrites += word_key(left) != word_key(right)
        targets.append(
            [index(edge) for edge in zip(left_edges, right_edges, strict=True)]
        )
        live.append(set(zip(reads(left), reads(right), strict=True)))
        definitions.append(
            tuple({local: i for i, local in enumerate(side)} for side in (a, b))
        )

    # At a matched use, either a dominating action writes both values at the
    # same result position or the pair must agree at the previous cursor too.
    predecessors = [[] for _ in points]
    for source, edges in enumerate(targets):
        for dest in set(edges):
            predecessors[dest].append(source)
    pending = deque((i, pair) for i, row in enumerate(live) for pair in row)
    facts = len(pending)
    while pending:
        if facts > 65536:
            raise Unsupported("MIR CFG local correspondence exceeds 65536 pairs")
        dest, pair = pending.popleft()
        for source in predecessors[dest]:
            left, right = definitions[source]
            a, b = left.get(pair[0]), right.get(pair[1])
            if a != b:
                raise Unsupported("MIR CFG writes disagree on a live local pair")
            if a is None and pair not in live[source]:
                facts += 1
                live[source].add(pair)
                pending.append((source, pair))
    if live[0]:
        raise Unsupported(
            "MIR CFG entry depends on a local without a matched definition"
        )
    return points, targets, live, jump_limit, word_rewrites


def render(nodes, symbols):
    def row(kind, data, edges):
        if kind == "forward":
            return f".forward {edges[0]}"
        if kind == "literal":
            return f".literalBranch {str(data).lower()} {edges[0]} {edges[1]}"
        word = word_key(data)
        if word:
            op, value = word
            key = f"(.{op}Constant {value}#256)"
        else:
            symbol = symbols.setdefault(
                (kind, dead_ssa.stable(dead_ssa.signature(data))), len(symbols)
            )
            operation = data.get("operation", data) if isinstance(data, dict) else {}
            if operation.get("kind") in {"call", "tail_call"}:
                tail = str(operation["kind"] == "tail_call").lower()
                key = f"(.directCall {operation['function']} {symbol} {tail})"
            else:
                key = f"(.opaque {symbol})"
        return f".action ⟨{key},{reads(data)},{writes(data)},{edges}⟩"

    return "#[" + ",".join(row(*node) for node in nodes) + "]"


def render_point(point, targets, live):
    pairs = ",".join(f"({a},{b})" for a, b in sorted(live))
    return f"⟨{point[0]},{point[1]},[{pairs}],{targets}⟩"


def graph_source(i, old, new):
    symbols = {}
    return f"""
def before{i} : Graph := {render(old, symbols)}
def after{i} : Graph := {render(new, symbols)}
"""


def certificate(i, old, new, witnesses, limit):
    code = graph_source(i, old, new)
    parts = []
    checks = []
    proof = ""
    predicate = f"(checkAt before{i} after{i} points{i} {limit})"
    # Reduce bounded chunks in the kernel. Checking a large array all at once
    # retains the reductions for every action and its live-locals check.
    for index, part in enumerate(batched(witnesses, 12)):
        name = f"points{i}_{index}"
        theorem = f"valid{i}_{index}"
        code += f"def {name} : Array Point := #[{','.join(part)}]\n"
        parts.append(name)
        checks.append(
            f"theorem {theorem} : {name}.all {predicate} = true := by decide +kernel"
        )
        proof = f"(append_checked {proof} {theorem})" if proof else theorem
    # Keep random access to the full table independent of proof partitioning.
    code += f"def points{i} : Array Point := #[{','.join(witnesses)}]\n"
    code += "\n".join(checks) + "\n"
    return (
        code
        + f"""
theorem valid{i} : Valid before{i} after{i} points{i} {limit} := by
  have partition : points{i} = {" ++ ".join(parts)} := by rfl
  exact Eq.mp (congrArg (fun rows => rows.all {predicate} = true) partition.symm)
    {proof}
theorem preserved{i} {{V S T : Type}} (handle : Handler V S T) (work : Nat)
    (left right : Locals V) (state : S) :
    MIR.CfgFlow.run before{i} {limit} handle work 0 left state =
    MIR.CfgFlow.run after{i} {limit} handle work 0 right state :=
  simulation before{i} after{i} points{i} {limit} valid{i} handle work 0
    {witnesses[0]} (by rfl) left right state (by simp [agrees])
#print axioms valid{i}
#print axioms preserved{i}
"""
    )


def recursive(calls):
    remaining = set(calls)
    while remaining:
        leaves = {i for i in remaining if not calls[i] & remaining}
        if not leaves:
            return True
        remaining -= leaves
    return False


def call_proof(pairs, entry):
    return f"""
open MIR.RecursiveCalls (ProcedurePair runRecursive procedures)
def relatedProcedures : List ProcedurePair := [{",".join(pairs)}]
theorem matchedCalls {{Value World Observation : Type}}
    (handlers : MIR.RecursiveCalls.Handlers Value World Observation)
    (argumentLocals : Nat → List Value → Locals Value) (work callLimit : Nat)
    (args : List Value) (state : World) :
    runRecursive (procedures relatedProcedures false) handlers argumentLocals work callLimit
        {entry} args state =
      runRecursive (procedures relatedProcedures true) handlers argumentLocals work callLimit
        {entry} args state :=
  MIR.RecursiveCalls.simulation relatedProcedures handlers argumentLocals work callLimit
    {entry} args state
#print axioms matchedCalls
"""


def check(paths, solar, executable, version, timeout, function):
    from .check import PRELUDE, ROOT, build_model, run

    texts, models, interfaces = [], [], []
    for path in paths:
        if path.suffix != ".mir" or path.stat().st_size > 1024 * 1024:
            raise Unsupported("MIR CFG proof requires MIR files of at most 1 MiB")
        parsed = run(
            [
                solar,
                path.resolve(),
                "-Zmir-pipeline=none",
                "-Zvalidate-ir=true",
                f"--evm-version={version}",
            ],
            ROOT,
            timeout,
        )
        if parsed.returncode:
            raise Unsupported(parsed.stderr)
        texts.append(parsed.stdout)
        models.append(type_prune.export(parsed.stdout, executable, timeout))
        header, entries = prune.declarations(parsed.stdout)
        interfaces.append((header, [entry["text"][0] for entry in entries.values()]))
    if interfaces[0] != interfaces[1] or {
        k: v for k, v in models[0].items() if k != "functions"
    } != {k: v for k, v in models[1].items() if k != "functions"}:
        raise Unsupported("MIR CFG proof changed shared declarations or interfaces")
    tables = [
        {body["return_buffer"]: body for body in model["functions"]} for model in models
    ]
    if any(
        len(table) != len(model["functions"])
        for table, model in zip(tables, models, strict=True)
    ):
        raise Unsupported("MIR CFG proof requires unambiguous names")
    code = (
        PRELUDE
        + """
open MIR.CfgFlow
set_option maxRecDepth 16384
set_option maxHeartbeats 2000000
#print axioms MIR.CfgFlow.typed_jump
#print axioms MIR.CfgFlow.typed_enter_empty
#print axioms MIR.CfgFlow.typed_literal_branch
#print axioms MIR.CfgFlow.normalized_word_result
#print axioms MIR.PhaseShaping.pointer_carrier
#print axioms MIR.PhaseShaping.full_word_casts
#print axioms MIR.ProductInit.construct_initialized
#print axioms MIR.ProductInit.observed_initialized
"""
    )
    work = [function]
    checked = {}
    calls = {}
    procedure_pairs = []
    axioms_checks = 8
    while work:
        name = work.pop()
        if name in checked:
            continue
        try:
            a, b = [table[name] for table in tables]
        except KeyError as error:
            raise Unsupported("MIR CFG proof has an unknown callee") from error
        if {k: v for k, v in a.items() if k != "blocks"} != {
            k: v for k, v in b.items() if k != "blocks"
        }:
            raise Unsupported("MIR CFG proof changed a function binding")
        if not a["blocks"] or not b["blocks"]:
            raise Unsupported("MIR CFG proof requires defined function bodies")
        i = len(checked)
        old, new = map(import_graph, (a, b))
        calls[a["id"]] = set()
        for kind, data, _ in old:
            if kind == "action" and isinstance(data, dict):
                op = data["operation"]
            elif kind == "terminal":
                op = data
            else:
                continue
            if op["kind"] in {"call", "tail_call"}:
                index = op["function"]
                calls[a["id"]].add(index)
                targets = [
                    name for name, body in tables[0].items() if body["id"] == index
                ]
                if len(targets) != 1:
                    raise Unsupported("MIR CFG proof has an ambiguous call binding")
                work.extend(targets)
        product_initialization = []
        for side, graph in enumerate((old, new)):
            products = product_certificate(f"products{i}_{side}", graph)
            product_initialization.append(bool(products))
            if products:
                code += products
                axioms_checks += 1
        literal_branches = [
            sum(node[0] == "literal" for node in graph) for graph in (old, new)
        ]
        if old == new:
            code += graph_source(i, old, new)
            code += f"""
theorem identical{i} : before{i} = after{i} := by rfl
#print axioms identical{i}
"""
            axioms_checks += 1
            checked[name] = {
                "proof": "identity",
                "source_points": len(old),
                "literal_branches": literal_branches,
            }
            limit = len(old) + 1
            matched = (
                f"fun handle work locals state => congrArg "
                f"(fun graph => run graph {limit} handle work 0 locals state) identical{i}"
            )
        else:
            try:
                points, edges, live, limit, word_rewrites = pairing(old, new)
            except Unsupported as error:
                try:
                    proof, copies = phase_carriers.paired_proof(i, a, b)
                except Unsupported:
                    raise error from None
                code += proof
                axioms_checks += 1
                checked[name] = {
                    "proof": "carrier-copies",
                    "copies": copies,
                    "product_initialization": product_initialization,
                }
                continue
            witnesses = [
                render_point(point, targets, locals_)
                for point, targets, locals_ in zip(points, edges, live, strict=True)
            ]
            code += certificate(i, old, new, witnesses, limit)
            matched = f"fun handle work locals state => preserved{i} handle work locals locals state"
            axioms_checks += 2
            checked[name] = {
                "proof": "path-correspondence",
                "paired_points": len(points),
                "reused_points": [
                    len(points) - len({point[side] for point in points})
                    for side in (0, 1)
                ],
                "live_pairs": sum(map(len, live)),
                "word_rewrites": word_rewrites,
                "literal_branches": literal_branches,
            }
        checked[name]["product_initialization"] = product_initialization
        procedure_pairs.append(
            f"⟨{a['id']},⟨before{i},{limit}⟩,⟨after{i},{limit}⟩,{matched}⟩"
        )
    nested = any(calls.values()) and len(procedure_pairs) == len(checked)
    recursive_calls = recursive(calls)
    if recursive_calls and not nested:
        raise Unsupported(
            "recursive MIR requires path certificates for all direct callees"
        )
    if nested:
        code += call_proof(procedure_pairs, tables[0][function]["id"])
        axioms_checks += 1
    entry_proof.success(build_model(timeout), "MIR CFG proof model")
    with tempfile.TemporaryDirectory(prefix="solar-tv-mir-cfg-") as folder:
        output = entry_proof.success(
            entry_proof.lean(Path(folder), "Cfg", code, timeout), "MIR CFG certificate"
        )
    entry_proof.check_axioms(output.splitlines(), axioms_checks)
    evidence = {
        "kind": "kernel-checked-typed-jump-chain",
        "functions": sorted(checked),
        "correspondence": checked,
        "proof_sha256": hashlib.sha256(code.encode()).hexdigest(),
        "boundary": "trusted structural exporter and action-constructor binding; unpriced shared effect handlers including calls and GAS; independent caller/ABI and backend correspondence remain separate",
        **(
            {"nested_calls": {"recursive": recursive_calls, "functions": sorted(calls)}}
            if nested
            else {}
        ),
    }
    programs = tuple(
        Program(
            ("mir-cfg-flow", function),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            selector=function,
            scope="mir-cfg-flow",
            evm_version=version,
            import_evidence=evidence,
        )
        for text in texts
    )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=programs
    )
