"""Propose path and live-local correspondence for independently exported MIR."""

import hashlib
import tempfile
from collections import deque
from itertools import batched
from pathlib import Path

from . import audit, dead_ssa, entry_proof, prune, type_prune
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
                    nodes.append(
                        (
                            "select",
                            keys,
                            [point(term[k], index) for k in ("then", "else")],
                        )
                    )
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


def writes(data):
    if isinstance(data, list):
        return [phi["result"] for phi in data]
    result = data.get("result")
    return [] if result is None else [result]


def pairing(old, new):
    points = [(0, 0)]
    locations = {(0, 0): 0}
    targets, live, definitions = [], [], []
    jump_limit = 1

    def index(pair):
        if pair not in locations:
            if len(points) >= 4096:
                raise Unsupported("paired MIR control flow exceeds 4096 points")
            locations[pair] = len(points)
            points.append(pair)
        return locations[pair]

    def follow(nodes, point):
        hops = 1
        while nodes[point][0] == "forward":
            point = nodes[point][2][0]
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
            or dead_ssa.signature(left) != dead_ssa.signature(right)
        ):
            raise Unsupported("MIR control-flow comparison changed a typed action")
        targets.append(
            [index(edge) for edge in zip(left_edges, right_edges, strict=True)]
        )
        live.append(set(zip(dead_ssa.reads(left), dead_ssa.reads(right), strict=True)))
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
    return points, targets, live, jump_limit


def render(nodes, symbols):
    def row(kind, data, edges):
        if kind == "forward":
            return f".forward {edges[0]}"
        key = symbols.setdefault(
            (kind, dead_ssa.stable(dead_ssa.signature(data))), len(symbols)
        )
        return f".action ⟨{key},{dead_ssa.reads(data)},{writes(data)},{edges}⟩"

    return "#[" + ",".join(row(*node) for node in nodes) + "]"


def render_point(point, targets, live):
    pairs = ",".join(f"({a},{b})" for a, b in sorted(live))
    return f"⟨{point[0]},{point[1]},[{pairs}],{targets}⟩"


def certificate(i, old, new, witnesses, limit):
    symbols = {}
    code = f"""
def before{i} : Graph := {render(old, symbols)}
def after{i} : Graph := {render(new, symbols)}
"""
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
    code += f"def points{i} : Array Point := {' ++ '.join(parts)}\n"
    code += "\n".join(checks) + "\n"
    return (
        code
        + f"""
theorem valid{i} : Valid before{i} after{i} points{i} {limit} :=
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
"""
    )
    work = [function]
    checked = {}
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
        points, edges, live, limit = pairing(old, new)
        for kind, data, _ in old:
            if kind == "action" and isinstance(data, dict):
                op = data["operation"]
            elif kind == "terminal":
                op = data
            else:
                continue
            if op["kind"] in {"call", "tail_call"}:
                index = op["function"]
                targets = [
                    name for name, body in tables[0].items() if body["id"] == index
                ]
                if len(targets) != 1:
                    raise Unsupported("MIR CFG proof has an ambiguous call binding")
                work.extend(targets)
        witnesses = [
            render_point(point, targets, locals_)
            for point, targets, locals_ in zip(points, edges, live, strict=True)
        ]
        code += certificate(i, old, new, witnesses, limit)
        checked[name] = {
            "paired_points": len(points),
            "reused_points": [
                len(points) - len({point[side] for point in points}) for side in (0, 1)
            ],
            "live_pairs": sum(map(len, live)),
        }
    entry_proof.success(build_model(timeout), "MIR CFG proof model")
    with tempfile.TemporaryDirectory(prefix="solar-tv-mir-cfg-") as folder:
        output = entry_proof.success(
            entry_proof.lean(Path(folder), "Cfg", code, timeout), "MIR CFG certificate"
        )
    entry_proof.check_axioms(output.splitlines(), 2 + 2 * len(checked))
    evidence = {
        "kind": "kernel-checked-typed-jump-chain",
        "functions": sorted(checked),
        "correspondence": checked,
        "proof_sha256": hashlib.sha256(code.encode()).hexdigest(),
        "boundary": "trusted structural exporter and action-constructor binding; unpriced shared effect handlers including calls and GAS; independent caller/ABI and backend correspondence remain separate",
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
