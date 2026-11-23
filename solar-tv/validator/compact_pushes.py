"""Check literal materializations in independently exported EVM CFGs.

Python proposes runs of closed instructions. Lean checks the actual nodes,
the words they produce, their peak stack use and their charge on the selected
fork. The matched graph preserves the entire continuation and excludes gas
and code observers and external messages. It assumes both executions can
pay every reached charge, including the unchanged effects' sentries.
"""

import hashlib
import json
import re
import tempfile
from pathlib import Path

from . import audit, correspondence, native, source_identity, stack_layout
from .common import Program, Unsupported, literal

RECIPE_OPS = {"not", "shl", "shr", "div", "mul"}
RECIPE_LIMIT = 16
WORD_MASK = 2**256 - 1


class RecipeRejected(Unsupported):
    """A matched recipe that could not satisfy the proof obligations."""


def instruction(name, catalog, version):
    if name.startswith("push "):
        word = literal(name[5:])
        value = int(word.removesuffix("#256"))
        opcode = (
            "push0"
            if value == 0 and catalog["push0"]["forks"][version]["available"]
            else "push1"
        )
        return f".push {word}", catalog[opcode]["forks"][version]["base_gas"], value
    if name not in RECIPE_OPS:
        raise Unsupported("compact PUSH recipe contains a nonconstant operation")
    price = catalog[name]["forks"][version]
    if not price["available"] or price["dynamic_gas"]:
        raise Unsupported(
            f"compact PUSH opcode `{name}` is unavailable or has dynamic gas"
        )
    return f".{name}", price["base_gas"], None


def replacement(model, pc, expected, catalog, version):
    """Find a short closed suffix producing exactly the old PUSH word."""
    stack, ops = [], []
    peak = gas = 0
    for _ in range(RECIPE_LIMIT):
        node = model["nodes"][pc]["node"]
        if node["kind"] != "operation":
            break
        name = node["operation"]
        term, cost, value = instruction(name, catalog, version)
        if value is None:
            if len(stack) < (1 if name == "not" else 2):
                break
            top = stack.pop()
            if name == "not":
                value = top ^ WORD_MASK
            else:
                second = stack.pop()
                match name:
                    case "shl":
                        value = (second << top) & WORD_MASK if top < 256 else 0
                    case "shr":
                        value = second >> top if top < 256 else 0
                    case "div":
                        value = top // second if second else 0
                    case "mul":
                        value = (top * second) & WORD_MASK
        stack.append(value)
        ops.append(term)
        peak = max(peak, len(stack))
        gas += cost
        pc = node["next"]
        if stack == [expected]:
            return ops, peak, gas, pc
    raise Unsupported("compact PUSH recipe does not produce the literal's word")


def pair(models, catalog, version):
    points, indices = [], {}
    witnesses = [[], []]
    nodes, pushes, meters = [[], []], [[], []], [[], []]
    local_edges = [[], []]

    def index(point):
        if point not in indices:
            if len(points) >= 4096:
                raise Unsupported("compact PUSH proof exceeds the CFG point budget")
            indices[point] = len(points)
            points.append(point)
        return indices[point]

    index(tuple(model["entry"] for model in models))
    for positions in points:
        left, right = [
            dict(model["nodes"][pc]["node"])
            for model, pc in zip(models, positions, strict=True)
        ]
        fields = tuple(field for field in ("next", "yes", "no") if field in left)
        same = {k: v for k, v in left.items() if k not in fields} == {
            k: v for k, v in right.items() if k not in fields
        }
        if same:
            successors = [index((left[field], right[field])) for field in fields]
            match successors:
                case [successor]:
                    edge = f".next {successor}"
                case [yes, no]:
                    edge = f".branch {yes} {no}"
                case []:
                    edge = ".terminal"
            for field, successor in zip(fields, successors, strict=True):
                left[field] = right[field] = successor
            for pc, rows, terms, node in zip(
                positions, nodes, witnesses, (left, right), strict=True
            ):
                rows.append(node)
                terms.append(f"⟨{pc},[],{edge}⟩")
            continue
        if left["kind"] != "operation" or not left["operation"].startswith("push "):
            raise Unsupported("compact PUSH pair changes a reachable nonliteral action")
        name = left["operation"]
        term, old_gas, value = instruction(name, catalog, version)
        recipe, peak, gas, after = replacement(
            models[1], positions[1], value, catalog, version
        )
        before = left["next"]
        old_recipe, outputs = [term], [name]
        # Include common literal pushes if they can reach the same stack peak.
        # There is no observation between these pushes and the recipe.
        while len(outputs) < peak and len(recipe) < RECIPE_LIMIT:
            next_left = models[0]["nodes"][before]["node"]
            next_right = models[1]["nodes"][after]["node"]
            if (
                next_left["kind"] != "operation"
                or next_right["kind"] != "operation"
                or next_left["operation"] != next_right["operation"]
                or not next_left["operation"].startswith("push ")
            ):
                break
            outputs.append(next_left["operation"])
            term, charge, _ = instruction(outputs[-1], catalog, version)
            old_recipe.append(term)
            recipe.append(term)
            old_gas += charge
            gas += charge
            before, after = next_left["next"], next_right["next"]
        successor = index((before, after))
        for side, (steps, growth, charge) in enumerate(
            ((old_recipe, len(outputs), old_gas), (recipe, peak, gas))
        ):
            guard = {"kind": "jump", "growth": growth, "next": len(pushes[side])}
            nodes[side].append(guard)
            local_edges[side].append(guard)
            for output_index, operation in enumerate(outputs, start=1):
                last = output_index == len(outputs)
                push = {
                    "kind": "operation",
                    "operation": operation,
                    "next": successor if last else len(pushes[side]) + 1,
                }
                pushes[side].append(push)
                if not last:
                    local_edges[side].append(push)
            witnesses[side].append(
                f"⟨{positions[side]},[{','.join(steps)}],.next {successor}⟩"
            )
            meters[side].append(
                {
                    "word": str(value),
                    "shared_pushes": len(outputs) - 1,
                    "operations": len(steps),
                    "peak": growth,
                    "gas": charge,
                }
            )
    if not meters[0]:
        raise Unsupported("no reachable compact PUSH recipe changed")
    for edges in local_edges:
        for node in edges:
            node["next"] += len(points)
    projected = [
        {
            "entry": 0,
            "nodes": [
                {"id": index, "label": 0, "node": node}
                for index, node in enumerate(rows + literals)
            ],
        }
        for rows, literals in zip(nodes, pushes, strict=True)
    ]
    return projected, witnesses, meters


def check(paths, solar, replay, version, timeout, entry, height):
    from .check import PRELUDE, ROOT, build_model, run

    bound = 1024 if height is None else height
    if not 0 <= bound <= 1024:
        raise Unsupported(
            "compact PUSH proof requires an architectural entry stack height"
        )
    texts = source_identity.parse_pair(paths, solar, version, timeout, entry)
    catalog = {row["name"]: row for row in native.catalog(replay, timeout)["evmir"]}
    sources = [
        correspondence.export(text, entry, version, replay, timeout) for text in texts
    ]
    raw = [source_identity.render(model, catalog, version, entry) for model in sources]
    projected, witnesses, meters = pair(sources, catalog, version)
    table = stack_layout.signatures(projected[0], catalog)
    try:
        _, bounds, _ = stack_layout.certificate(*projected, table, entry_bound=bound)
    except Unsupported as error:
        raise RecipeRejected(f"compact PUSH stack certificate: {error}") from error
    signatures = ",".join(
        f"({json.dumps(name)},({inputs},{outputs}))"
        for name, (inputs, outputs) in table.items()
    )
    code = (
        PRELUDE
        + """
open EVM.SourceGraph (Program)
open EVM.CompactPushes
open EVM.StackGraph
set_option maxRecDepth 16384
set_option maxHeartbeats 3500000
"""
    )
    for name, source, model, points, meter in zip(
        ("before", "after"), raw, projected, witnesses, meters, strict=True
    ):
        terms = source_identity.render(
            model, catalog, version, "bb0", max_growth=RECIPE_LIMIT
        )
        prices = ",".join(str(step["gas"]) for step in meter)
        code += f"""
def {name}Source : Program := {source}
def {name}Points : Array Point := #[{",".join(points)}]
def {name} : Program := {terms}
theorem {name}Checked : project {name}Source .{version} {name}Points =
    some ({name},#[{prices}]) := by decide +kernel
"""
    code += f"""
def heights (i : Nat) : Nat := (#{bounds} : Array Nat)[i]?.getD 0
def signatures (name : String) : Option (Nat × Nat) :=
  ([{signatures}] : List (String × (Nat × Nat))).lookup name
theorem valid : Valid before after id heights signatures {bound} := by decide +kernel
theorem matched {{World Observation : Type}} (handlers : Handlers World Observation)
    (fuel : Nat) (state : State World) (fits : state.height ≤ {bound}) :
    (project beforeSource .{version} beforePoints).map
      (fun p => execute p.1 signatures handlers fuel p.1.entry state) =
    (project afterSource .{version} afterPoints).map
      (fun p => execute p.1 signatures handlers fuel p.1.entry state) := by
  rw [beforeChecked, afterChecked]
  change some (execute before signatures handlers fuel before.entry state) =
    some (execute after signatures handlers fuel after.entry state)
  rw [← valid.2.1]
  exact congrArg some (bounded_simulation before after id heights signatures valid
    handlers fuel before.entry state valid.1 (Nat.le_trans fits valid.2.2.1))
#print axioms matched
#print axioms EVM.CompactPushes.exact
#print axioms EVM.CompactPushes.funded
"""
    built = build_model(timeout)
    if built.returncode:
        raise Unsupported(built.stdout + built.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-compact-pushes-") as folder:
        proof = Path(folder) / "Pair.lean"
        proof.write_text(code)
        result = run(["lake", "env", "lean", proof], ROOT, timeout)
    if result.returncode:
        raise RecipeRejected(
            "compact PUSH certificate did not close\n" + result.stdout + result.stderr
        )
    for theorem in (
        "matched",
        "SolarTV.EVM.CompactPushes.exact",
        "SolarTV.EVM.CompactPushes.funded",
    ):
        axioms = re.search(rf"'{theorem}' depends on axioms: \[(.*?)\]", result.stdout)
        if not (
            f"'{theorem}' does not depend on any axioms" in result.stdout
            or axioms
            and set(axioms[1].split(", "))
            <= {"propext", "Classical.choice", "Quot.sound"}
        ):
            raise Unsupported("unexpected compact PUSH proof axioms")
    programs = tuple(
        Program(
            ("evm-compact-pushes", str(bound)),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            selector=entry,
            scope="compact-pushes",
            gas="sufficient",
            evm_version=version,
            import_evidence={
                "kind": "kernel-checked-compact-pushes",
                "source_sha256": hashlib.sha256(
                    json.dumps(model, sort_keys=True).encode()
                ).hexdigest(),
                "recipes": recipes,
                "entry_height_bound": bound,
            },
        )
        for text, model, recipes in zip(texts, sources, meters, strict=True)
    )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=programs
    )
