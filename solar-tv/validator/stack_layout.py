"""Reindexed source graphs with kernel-checked architectural stack bounds.

No instruction or transfer check is erased. Opcode arities come from the shared
compiler schema; the Lean interpreter derives height changes from those arities
and leaves values, memory and effects to arbitrary shared handlers. The finite
certificate proves local bounds and simulation for every execution bound. It
does not price physical transfers or prove the opcode-handler embedding.
"""

import hashlib
import json
import re
import tempfile
from collections import deque
from pathlib import Path

from . import audit, regions
from .common import Program, Unsupported


def immutable_push(instruction):
    """A fixed-width PUSH filled from a deployment slot, not a literal."""
    return re.fullmatch(r"push_immutable \d+, (?:[1-9]|[12]\d|3[0-2])", instruction)


def signatures(model, catalog):
    table = {}
    for row in model["nodes"]:
        node = row["node"]
        if node["kind"] != "operation":
            continue
        instruction = name = node["operation"]
        if instruction.startswith("push ") or immutable_push(instruction):
            name = "push1"
        elif match := re.fullmatch(r"(dup|swap) (\d+)", instruction):
            name = match[1] + match[2]
        signature = catalog[name]["stack"]
        if not signature or any(
            type(n) is not int or not 0 <= n <= 1024 for n in signature
        ):
            raise Unsupported("layout requires fixed architectural opcode arities")
        table[instruction] = tuple(signature)
    return table


def certificate(before, after, table, *, entry_bound=1024):
    def keys(model):
        counts, result = {}, []
        for row in model["nodes"]:
            label = row["label"]
            offset = counts.get(label, 0)
            result.append((label, offset))
            counts[label] = offset + 1
        return result

    old, new = keys(before), keys(after)
    positions = {key: i for i, key in enumerate(new)}
    if set(old) != set(new):
        raise Unsupported("layout changed source nodes or labels")
    rename = [positions[key] for key in old]
    if rename[before["entry"]] != after["entry"]:
        raise Unsupported("layout changed the source entry")
    for row in before["nodes"]:
        node = dict(row["node"])
        for field in ("next", "yes", "no"):
            if field in node:
                node[field] = rename[node[field]]
        target = after["nodes"][rename[row["id"]]]["node"]
        if node["kind"] == "jump" and target["kind"] == "jump":
            node["growth"] = target["growth"]
        if node != target:
            raise Unsupported("layout changed an operation, operand, terminal or edge")
    bounds = [0] * len(old)
    bounds[before["entry"]] = entry_bound
    pending = deque(range(len(old)))
    updates = 0
    while pending:
        index = pending.popleft()
        node, upper = before["nodes"][index]["node"], bounds[index]
        match node["kind"]:
            case "operation":
                inputs, outputs = table[node["operation"]]
                edges = (
                    [
                        (
                            node["next"],
                            min(upper, 1024 + inputs - outputs) - inputs + outputs,
                        )
                    ]
                    if upper >= inputs
                    else []
                )
            case "jump":
                edges = [(node["next"], min(upper, 1024 - node["growth"]))]
            case "branch":
                edges = [
                    (node[field], max(0, min(upper, 1023) - 1))
                    for field in ("yes", "no")
                ]
            case _:
                edges = []
        for child, height in edges:
            if height > bounds[child]:
                bounds[child] = height
                pending.append(child)
                updates += 1
                if updates > 65536:
                    raise Unsupported("layout stack analysis exceeds 65536 updates")
    changed = []
    for row in before["nodes"]:
        node = row["node"]
        target = after["nodes"][rename[row["id"]]]["node"]
        if node["kind"] == "jump" and node["growth"] != target["growth"]:
            if bounds[row["id"]] + max(node["growth"], target["growth"]) > 1024:
                raise Unsupported("changed transfer may overflow the 1024-word stack")
            changed.append({"node": row["id"], "height_bound": bounds[row["id"]]})
    return rename, bounds, changed


def check_data(texts, models, expressions, entry, catalog, version, timeout):
    from .check import PRELUDE, ROOT, build_model, run

    table = signatures(models[0], catalog)
    rename, bounds, changed = certificate(*models, table)
    exits = [
        row["id"]
        for row in models[0]["nodes"]
        if row["node"] == {"kind": "terminal", "operation": "jump"}
    ]
    signature_rows = ",".join(
        f"({json.dumps(name)},({n},{m}))" for name, (n, m) in table.items()
    )
    code = (
        PRELUDE
        + f"""
def before : EVM.SourceGraph.Program := {expressions[0]}
def after : EVM.SourceGraph.Program := {expressions[1]}
def rename (i : Nat) : Nat := (#[{",".join(map(str, rename))}] : Array Nat)[i]?.getD 0
def bounds (i : Nat) : Nat := (#[{",".join(map(str, bounds))}] : Array Nat)[i]?.getD 0
def signatures (name : String) : Option (Nat × Nat) :=
  ([{signature_rows}] : List (String × (Nat × Nat))).lookup name
theorem valid : EVM.StackGraph.Valid before after rename bounds signatures := by decide +kernel
theorem contextual {{World Observation : Type}}
 (handlers : EVM.StackGraph.Handlers World Observation) (fuel : Nat) (state : EVM.StackGraph.State World) :
 EVM.StackGraph.execute before signatures handlers fuel before.entry state =
 EVM.StackGraph.execute after signatures handlers fuel after.entry state :=
 EVM.StackGraph.contextual before after rename bounds signatures valid handlers fuel state
#print axioms contextual
"""
    )
    built = build_model(timeout)
    if built.returncode:
        raise Unsupported(built.stdout + built.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-stack-layout-") as folder:
        path = Path(folder) / "Pair.lean"
        path.write_text(code)
        result = run(["lake", "env", "lean", path], ROOT, timeout)
    axioms = re.search(r"'contextual' depends on axioms: \[(.*?)\]", result.stdout)
    if (
        result.returncode
        or not axioms
        or set(axioms[1].split(", ")) - {"propext", "Quot.sound", "Classical.choice"}
    ):
        raise Unsupported(
            "layout stack certificate did not close\n" + result.stdout + result.stderr
        )
    programs = []
    for text, model in zip(texts, models, strict=True):
        labels = {f"bb{row['label']}" for row in model["nodes"]}
        projection = (
            "@module StackLayout\n"
            + "\n".join(
                f"{label}:\n" + "\n".join(body)
                for label, body in regions.blocks(text).items()
                if label in labels
            )
            + "\n"
        )
        programs.append(
            Program(
                ("evm-stack-layout", entry),
                [],
                "",
                "",
                canonical_ir=projection,
                replay_ir=text,
                selector=entry,
                scope="stack-layout",
                evm_version=version,
                import_evidence={
                    "kind": "independent-schema-stack-layout",
                    "graph_sha256": hashlib.sha256(
                        json.dumps(model, sort_keys=True).encode()
                    ).hexdigest(),
                    "nodes": len(model["nodes"]),
                    "source_to_target_nodes": rename,
                    "source_height_bounds": bounds,
                    "changed_transfers": changed,
                    "dynamic_exits": exits,
                    "signatures": table,
                    "boundary": "trusted parser/CFG normalizer, schema arities and opcode-handler/height binding; kernel-checked local bounds and all-execution-bounds simulation; no physical gas or backend layout certificate",
                },
            )
        )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=tuple(programs)
    )
