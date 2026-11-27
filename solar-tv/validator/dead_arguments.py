"""Check internal parameter projection against both complete typed MIR exports."""

import hashlib
import re
import sys
import tempfile
from functools import cache
from pathlib import Path

from . import audit, dead_ssa, mir_cfg, prune, type_prune
from .common import Program, Unsupported

MODULE = "SolarTV.MIR.Transforms.DeadArguments"
DECLARATION = re.compile(rf"^fn @({prune.NAME})\(([^()]*)\)(.*)$", re.MULTILINE)
OPERANDS = {"arg", "local", "constant", "undefined"}


def candidate(paths):
    if any(p.suffix != ".mir" or p.stat().st_size > 1024 * 1024 for p in paths):
        return False
    before, after = [
        {
            name: (params, suffix)
            for name, params, suffix in DECLARATION.findall(p.read_text())
        }
        for p in paths
    ]
    return before.keys() == after.keys() and any(
        before[name][0] != after[name][0] for name in before
    )


def entry_ids(model):
    roots = {row["id"] for row in model["selectors"]}
    roots.update(model["runtime_entries"])
    roots.update(
        model[key] for key in ("dispatch_entry", "constructor", "fallback", "receive")
    )
    return roots


def argument_uses(node):
    if isinstance(node, list):
        return {index for part in node for index in argument_uses(part)}
    if isinstance(node, dict):
        if node.get("kind") == "arg":
            return {node["index"]}
        return {index for part in node.values() for index in argument_uses(part)}
    return set()


def retained(before, after, used):
    if len(before) > 128 or len(after) >= len(before):
        raise Unsupported("dead argument elimination requires a smaller parameter list")

    @cache
    def match(old, new):
        if old == len(before):
            return () if new == len(after) else None
        if new < len(after) and before[old] == after[new]:
            tail = match(old + 1, new + 1)
            if tail is not None:
                return (old, *tail)
        return match(old + 1, new) if old not in used else None

    result = match(0, 0)
    if result is None:
        raise Unsupported("removed MIR parameter still has a use or changes type")
    return list(result)


def align(before, after):
    if len(before["blocks"]) != len(after["blocks"]):
        raise Unsupported("dead argument elimination changed block identities")
    aliases, seen = {}, set()
    for old, new in zip(before["blocks"], after["blocks"], strict=True):
        for group in ("phis", "instructions"):
            if len(old[group]) != len(new[group]):
                raise Unsupported("dead argument elimination changed the action count")
            for a, b in zip(old[group], new[group], strict=True):
                left, right = a["result"], b["result"]
                if left is None and right is None:
                    continue
                if left is None or right is None or left in seen or right in aliases:
                    raise Unsupported("invalid SSA correspondence for dead arguments")
                aliases[right] = left
                seen.add(left)
    return dead_ssa.rename_locals(after, aliases)


def render(body, functions, keep, symbols, literals):
    def project(node, reads):
        if isinstance(node, list):
            return [project(operand, reads) for operand in node]
        if not isinstance(node, dict):
            return node
        kind = node.get("kind")
        if kind in OPERANDS and "type" in node:
            if kind == "arg":
                i = node["index"]
                if not 0 <= i < len(body["params"]):
                    raise Unsupported("out-of-range MIR argument operand")
                reads.append(3 * (keep[i] if keep is not None else i) + 1)
            elif kind == "local":
                reads.append(3 * node["index"])
            else:
                key = dead_ssa.stable(node)
                reads.append(3 * literals.setdefault(key, len(literals)) + 2)
            return {"type": node["type"]}
        return {key: project(node[key], reads) for key in sorted(node)}

    rows = []
    for kind, data, edges in mir_cfg.import_graph(body, combine=False):
        if kind == "forward":
            rows.append(f".forward {edges[0]}")
            continue
        if kind == "literal":
            rows.append(f".literalBranch {str(data).lower()} {edges[0]} {edges[1]}")
            continue
        reads = []
        operation = data.get("operation", data) if isinstance(data, dict) else {}
        if operation.get("kind") in {"call", "tail_call"}:
            callee = operation["function"]
            args = operation["args"]
            if (
                callee not in functions
                or [a["type"] for a in args] != functions[callee]["params"]
            ):
                raise Unsupported("invalid direct call signature for dead arguments")
            if any(arg["kind"] == "undefined" for arg in args):
                raise Unsupported(
                    "dead argument elimination requires defined direct operands"
                )
            project(args, reads)
            op = {field: value for field, value in operation.items() if field != "args"}
            payload = {**data, "operation": op} if "operation" in data else op
            symbol = dead_ssa.stable((kind, payload))
            signature = symbols.setdefault(symbol, len(symbols))
            tail = str(operation["kind"] == "tail_call").lower()
            key = f"(.directCall {callee} {signature} {tail})"
        else:
            payload = dead_ssa.stable((kind, project(data, reads)))
            key = f"(.opaque {symbols.setdefault(payload, len(symbols))})"
        writes = [3 * local for local in mir_cfg.writes(data)]
        rows.append(f".action ⟨{key},{reads},{writes},{edges}⟩")
    return "#[" + ",".join(rows) + "]", len(rows) + 1


def exports(paths, solar, replay, version, timeout):
    from .check import ROOT, run

    texts, models = [], []
    for path in paths:
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
        models.append(type_prune.export(parsed.stdout, replay, timeout))
    left, right = (prune.declarations(text) for text in texts)
    if left[0] != right[0] or list(left[1]) != list(right[1]):
        raise Unsupported("dead argument elimination changed module bindings")
    headers = [
        {name: suffix for name, _, suffix in DECLARATION.findall(text)}
        for text in texts
    ]
    if headers[0] != headers[1] or headers[0].keys() != left[1].keys():
        raise Unsupported("dead argument elimination changed function attributes")
    bindings = [
        {key: value for key, value in model.items() if key != "functions"}
        for model in models
    ]
    if bindings[0] != bindings[1]:
        raise Unsupported("dead argument elimination changed exported entry bindings")
    return texts, models, left[1]


def certificate(models, declarations, function):
    before, after = (
        {body["id"]: body for body in model["functions"]} for model in models
    )
    if before.keys() != after.keys() or any(
        len(table) != len(model["functions"])
        for table, model in zip((before, after), models)
    ):
        raise Unsupported("dead argument elimination changed function identities")
    roots = entry_ids(models[0])
    kept, changes = {}, {}
    for index, old in before.items():
        new = after[index]
        name = old["return_buffer"]
        if name != new["return_buffer"] or old["returns"] != new["returns"]:
            raise Unsupported(
                "dead argument elimination changed a callee return interface"
            )
        if old["params"] != new["params"]:
            if index in roots or name not in declarations or declarations[name]["root"]:
                raise Unsupported(
                    "dead argument elimination changed an entry interface"
                )
            kept[index] = retained(
                old["params"], new["params"], argument_uses(old["blocks"])
            )
            changes[name] = {"before_arity": len(old["params"]), "kept": kept[index]}
    if not kept or function not in {body["return_buffer"] for body in before.values()}:
        raise Unsupported("no internal parameter projection for the selected module")

    cases = "".join(f"\n  | {index} => some {args}" for index, args in kept.items())
    source = [
        f"import {MODULE}",
        "open SolarTV.MIR CfgFlow DeadArguments",
        "set_option maxRecDepth 16384",
        "set_option maxHeartbeats 8000000",
        "set_option Elab.async false",
        f"def slots : DeadArguments.Slots{cases}\n  | _ => none",
    ]
    symbols, literals = {}, {}
    for index, old in before.items():
        old_graph, limit = render(old, before, None, symbols, literals)
        new_graph, after_limit = render(
            align(old, after[index]), after, kept.get(index), symbols, literals
        )
        source.append(f"""
def before{index} : Graph := {old_graph}
def after{index} : Graph := {new_graph}
def pair{index} : DeadArguments.Pair slots :=
  ⟨{index}, ⟨before{index}, {limit}⟩, ⟨after{index}, {after_limit}⟩,
    by rw [← Array.all_toList]; rfl,
    by apply Array.toList_inj.mp; rw [Array.toList_map]; rfl, rfl⟩
""")
    source.append(
        "def pairs : List (DeadArguments.Pair slots) := ["
        + ",".join(f"pair{index}" for index in before)
        + "]"
    )
    source.append("""
def contextual {Value World Observation : Type} :=
  DeadArguments.simulation (Value := Value) (World := World) (Observation := Observation) slots pairs
#print axioms contextual
""")
    return "\n".join(source), changes


def check(paths, solar, replay, version, timeout, function, *, verbose=False):
    from . import entry_proof
    from .check import build_model

    if not candidate(paths):
        return None
    try:
        texts, models, declarations = exports(paths, solar, replay, version, timeout)
        source, changes = certificate(models, declarations, function)
        entry_proof.success(build_model(timeout, MODULE), MODULE)
        with tempfile.TemporaryDirectory(prefix="solar-tv-dead-arguments-") as folder:
            result = entry_proof.lean(Path(folder), "DeadArguments", source, timeout)
        output = entry_proof.success(result, "dead argument elimination")
        entry_proof.check_axioms(
            [line for line in output.splitlines() if line.startswith("'contextual' ")],
            1,
        )
    except Unsupported as error:
        if verbose:
            print(f"MIR dead arguments: {error}", file=sys.stderr)
        return None

    pair = tuple(
        Program(
            (function, "mir-dead-arguments"),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            selector=function,
            scope="mir-dead-arguments",
            evm_version=version,
            import_evidence={
                "kind": "kernel-checked-mir-argument-projection",
                "source_sha256": hashlib.sha256(
                    dead_ssa.stable(model).encode()
                ).hexdigest(),
                "functions": len(model["functions"]),
                "argument_projection": changes,
            },
        )
        for text, model in zip(texts, models, strict=True)
    )
    return audit.Result("verified", stdout="verified\n", program=pair[0], pair=pair)
