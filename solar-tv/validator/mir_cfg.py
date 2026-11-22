"""Kernel-checked MIR CFG coalescing over typed effects and live state."""

import hashlib
import tempfile
from collections import deque
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


def pairing(old, new):
    locations = [{0: 0}, {0: 0}]
    points = [[0], [0]]
    binders = {}
    inverse = {}
    jump_limit = 1

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

    def bind(result, target):
        if result is None or target is None:
            if result != target:
                raise Unsupported("MIR CFG changed a result binder")
            return
        if (
            binders.setdefault(target, result) != result
            or inverse.setdefault(result, target) != target
        ):
            raise Unsupported("MIR CFG binder correspondence is not injective")

    for before, after in zip(*points):
        l, left = follow(old, before)
        r, right = follow(new, after)
        jump_limit = max(jump_limit, l, r)
        if (
            left[0] != right[0]
            or len(left[2]) != len(right[2])
            or dead_ssa.signature(left[1]) != dead_ssa.signature(right[1])
        ):
            raise Unsupported("MIR control-flow comparison changed a typed action")
        if left[0] == "action":
            left_insts = left[1] if isinstance(left[1], list) else [left[1]]
            right_insts = right[1] if isinstance(right[1], list) else [right[1]]
            for a, b in zip(left_insts, right_insts, strict=True):
                bind(a["result"], b["result"])
        for source, target in zip(left[2], right[2], strict=True):
            use, other = locations[0].get(source), locations[1].get(target)
            if use != other:
                raise Unsupported("MIR control-flow comparison changed node sharing")
            if use is None:
                locations[0][source] = locations[1][target] = len(points[0])
                points[0].append(source)
                points[1].append(target)
    return points, jump_limit, binders


def render(nodes, symbols, mapping=None):
    def rename(value):
        if isinstance(value, list):
            return [rename(item) for item in value]
        if not isinstance(value, dict):
            return value
        out = {key: rename(item) for key, item in value.items()}
        if out.get("kind") == "local":
            if out["index"] not in mapping:
                raise Unsupported("MIR CFG proof has an unmapped local operand")
            out["index"] = mapping[out["index"]]
        if out.get("result") is not None:
            out["result"] = mapping.get(out["result"], out["result"])
        return out

    def row(kind, data, edges):
        if kind == "forward":
            return f".forward {edges[0]}"
        dead_ssa.reads(data)
        data = rename(data) if mapping is not None else data
        key = symbols.setdefault(dead_ssa.stable(data), len(symbols))
        if kind == "action":
            return f".action {key} {edges[0]}"
        if kind == "select":
            return f".select {key} {edges}"
        return f".terminal {key}"

    return "#[" + ",".join(row(*node) for node in nodes) + "]"


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
    checked = set()
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
        points, limit, binders = pairing(old, new)
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
        symbols = {}
        code += f"""
def before{i} : Graph := {render(old, symbols)}
def after{i} : Graph := {render(new, symbols, binders)}
def left{i} : List Nat := {points[0]}
def right{i} : List Nat := {points[1]}
def target{i} : Option Graph := project before{i} left{i} {limit}
theorem exists{i} : target{i}.isSome := by decide
theorem same{i} : target{i} = project after{i} right{i} {limit} := by decide
theorem preserved{i} {{S T : Type}} (handlers : Handlers S T) (work : Nat) (state : S) :
    (project before{i} left{i} {limit}).map (fun g => MIR.CfgFlow.run g handlers work 0 state) =
    (project after{i} right{i} {limit}).map (fun g => MIR.CfgFlow.run g handlers work 0 state) :=
  contextual before{i} after{i} left{i} right{i} {limit} same{i} handlers work state
#print axioms exists{i}
#print axioms preserved{i}
"""
        checked.add(name)
    entry_proof.success(build_model(timeout), "MIR CFG proof model")
    with tempfile.TemporaryDirectory(prefix="solar-tv-mir-cfg-") as folder:
        output = entry_proof.success(
            entry_proof.lean(Path(folder), "Cfg", code, timeout), "MIR CFG certificate"
        )
    entry_proof.check_axioms(output.splitlines(), 2 + 2 * len(checked))
    evidence = {
        "kind": "kernel-checked-typed-jump-chain",
        "functions": sorted(checked),
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
