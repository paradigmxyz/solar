"""Check literal materializations in independently exported EVM CFGs.

Python proposes runs of closed instructions. Lean checks the actual nodes,
the words they produce, their peak stack use and their charge on the selected
fork. The matched graph retains the continuation. The explicit interaction
mode also checks requests under shared replies. Both executions must be
funded through all reached charges and admission checks.
"""

import hashlib
import json
import os
import re
import tempfile
from pathlib import Path

from . import (
    audit,
    cfg_flow,
    continuation_flow,
    correspondence,
    entry_stack,
    native,
    source_identity,
    stack_layout,
)
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


def replacement(model, pc, expected, catalog, version, protected):
    """Find a short closed suffix producing exactly the old PUSH word."""
    stack, ops = [], []
    peak = gas = 0
    for _ in range(RECIPE_LIMIT):
        if pc in protected:
            break
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


def pair(
    models, catalog, version, protected=(frozenset(), frozenset()), *, from_entry=False
):
    points, indices = [], {}
    witnesses = [[], []]
    nodes, pushes, meters = [[], []], [[], []], [[], []]
    local_edges = [[], []]

    limit = 16384 if from_entry else 4096

    def index(point):
        if point not in indices:
            if len(points) >= limit:
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
        before_label = positions[0] in protected[0]
        after_label = positions[1] in protected[1]
        if same and before_label == after_label:
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
        if (
            positions[0] in protected[0]
            or left["kind"] != "operation"
            or not left["operation"].startswith("push ")
        ):
            raise Unsupported("compact PUSH pair changes a reachable nonliteral action")
        name = left["operation"]
        term, old_gas, value = instruction(name, catalog, version)
        recipe, peak, gas, after = replacement(
            models[1], positions[1], value, catalog, version, protected[1]
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
                or before in protected[0]
                or after in protected[1]
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


def check(
    paths,
    solar,
    replay,
    version,
    timeout,
    entry,
    height,
    *,
    interactions=False,
    bound_from_entry=False,
):
    from .check import PRELUDE, ROOT, build_model, run

    from_entry = entry is None
    bound = 0 if from_entry else 1024 if height is None else height
    if not 0 <= bound <= 1024:
        raise Unsupported(
            "compact PUSH proof requires an architectural entry stack height"
        )
    texts = source_identity.parse_pair(paths, solar, version, timeout, entry)
    catalog = {row["name"]: row for row in native.catalog(replay, timeout)["evmir"]}
    roots = None
    if bound_from_entry:
        roots = [
            entry_stack.propose(text, entry, replay, catalog, version, timeout)
            for text in texts
        ]
        bound = max(max(bounds) for _, _, bounds in roots)
    witnesses = None
    sources = None
    if not (from_entry or interactions):
        try:
            sources = [
                correspondence.export(text, entry, version, replay, timeout)
                for text in texts
            ]
        except Unsupported as error:
            if str(error) != "EVM source graph: escaping label in EVM source graph":
                raise
        else:
            graphs = sources
    if sources is None:
        sources = [
            correspondence.export(text, entry, version, replay, timeout, labels=True)
            for text in texts
        ]
        graphs, witnesses = zip(
            *(
                continuation_flow.project(
                    model,
                    catalog,
                    version,
                    interactions=interactions,
                    from_entry=from_entry or bound_from_entry,
                )
                for model in sources
            ),
            strict=True,
        )
    protected = (
        tuple(w["label_pushes"] for w in witnesses)
        if witnesses
        else (frozenset(), frozenset())
    )
    projected, points, meters = pair(
        graphs, catalog, version, protected, from_entry=from_entry or bound_from_entry
    )
    signatures = stack_layout.signatures(projected[0], catalog)
    requests = sorted(source_identity.INTERACTION_OPS & signatures.keys())
    if interactions and not requests and not (from_entry or bound_from_entry):
        raise Unsupported(
            "compact PUSH interaction trace requires a reachable interaction"
        )
    try:
        _, bounds, _ = stack_layout.certificate(
            *projected, signatures, entry_bound=bound
        )
    except Unsupported as error:
        raise RecipeRejected(f"compact PUSH stack certificate: {error}") from error
    prelude = (
        PRELUDE
        + """
open EVM.SourceGraph (Program)
open EVM.ControlFlow (Edges)
open EVM.CompactPushes EVM.StackGraph
noncomputable section
set_option maxRecDepth 16384
set_option maxHeartbeats 3500000
"""
    )
    code = prelude
    proof_names = [
        "matched",
        "SolarTV.EVM.CompactPushes.exact",
        "SolarTV.EVM.CompactPushes.funded",
    ]
    for side, name in enumerate(("before", "after")):
        for label, graph, root in (
            (f"{name}Graph", graphs[side], "bb0" if witnesses else entry),
            (name, projected[side], "bb0"),
        ):
            start, rows = source_identity.render_nodes(
                graph,
                catalog,
                version,
                root,
                max_growth=RECIPE_LIMIT,
                interactions=interactions,
                from_entry=from_entry or bound_from_entry,
            )
            code += cfg_flow.table(f"{label}Nodes", "EVM.SourceGraph.Node", rows)
            code += f"def {label} : Program := ⟨{start},{label}Nodes.rows.toArray⟩\n"
        code += cfg_flow.table(f"{name}Recipes", "Point", points[side])
        guard = "fun _ => false"
        if witnesses:
            code += cfg_flow.continuation_proof(name, witnesses[side], f"{name}Graph")
            guard = f"EVM.ContinuationFlow.pushesLabel {name}SourceNodes.view {name}Points.view"
        prices = ",".join(str(step["gas"]) for step in meters[side])
        code += f"""
def {name}Protected : Nat → Bool := {guard}
def {name}RecipeProjection (source : Program) : Option (Program × List Nat) :=
  project source .{version} {name}Recipes.rows.toArray {name}Protected {str(interactions).lower()}
def {name}Prices : List Nat := [{prices}]
theorem {name}Checked : {name}RecipeProjection {name}Graph = some ({name},{name}Prices) :=
  project_checked {name}Graph .{version} {name}Recipes.rows.toArray
    {name}GraphNodes.view {name}Recipes.view {name}Protected {str(interactions).lower()} {name} {name}Prices
    {name}GraphNodesLookup {name}RecipesLookup (by decide +kernel)
"""
    code += cfg_flow.table("stackBounds", "Nat", list(map(str, bounds)))
    code += cfg_flow.signatures_declaration(signatures, catalog)
    code += f"""
def heights (i : Nat) : Nat := (stackBounds.get? i).getD 0
theorem valid : Valid before after id heights signatures {bound} :=
  valid_views before after beforeNodes.view afterNodes.view id heights signatures {bound}
    beforeNodesLookup afterNodesLookup (by decide +kernel) (by rfl)
    (by decide +kernel) (by decide +kernel)
def observe {{World Observation : Type}} (handlers : Handlers World Observation)
    (fuel : Nat) (state : State World) (p : Program × List Nat) :=
  execute p.1 signatures handlers fuel p.1.entry state
theorem matchedRecipes {{World Observation : Type}} (handlers : Handlers World Observation)
    (fuel : Nat) (state : State World) (fits : state.height ≤ {bound}) :
    (beforeRecipeProjection beforeGraph).map (observe handlers fuel state) =
      (afterRecipeProjection afterGraph).map (observe handlers fuel state) :=
  contextual _ _ before after beforePrices afterPrices signatures heights {bound}
    beforeChecked afterChecked valid handlers fuel state fits
"""
    if witnesses:
        for name in ("before", "after"):
            code += f"""
def {name}Projected : Option (Program × List Nat) :=
  (EVM.ContinuationFlow.project {name}Source {name}Points.rows.toArray
    {name}Edges.rows.toArray).bind {name}RecipeProjection
"""
        match_proof = """EVM.ControlFlow.compose beforeProvenance afterProvenance
    beforeRecipeProjection afterRecipeProjection (observe handlers fuel state)
    (matchedRecipes handlers fuel state fits)"""
    else:
        code += """
def beforeProjected := beforeRecipeProjection beforeGraph
def afterProjected := afterRecipeProjection afterGraph
"""
        match_proof = "matchedRecipes handlers fuel state fits"
    code += f"""
theorem matched {{World Observation : Type}} (handlers : Handlers World Observation)
    (fuel : Nat) (state : State World) (fits : state.height ≤ {bound}) :
    beforeProjected.map (observe handlers fuel state) =
      afterProjected.map (observe handlers fuel state) :=
  {match_proof}
"""
    if requests:
        names = ",".join(map(json.dumps, requests))
        code += f"""
theorem traceSignatures :
    ([{names}] : List String).all (EVM.InteractionTrace.signatureFits signatures) = true :=
  by decide +kernel
theorem matchingRequests {{Payload Halt : Type}} (view : Payload → EVM.InteractionTrace.View)
    (ordinary : Handlers Payload Halt)
    (reply : EVM.InteractionTrace.Request → Payload → Except Halt Payload)
    (fuel : Nat) (state : State (EVM.InteractionTrace.Cursor Payload)) (fits : state.height ≤ {bound}) :
    beforeProjected.map (observe (EVM.InteractionTrace.handlers view ordinary reply) fuel state) =
      afterProjected.map (observe (EVM.InteractionTrace.handlers view ordinary reply) fuel state) :=
  matched (EVM.InteractionTrace.handlers view ordinary reply) fuel state fits
"""
        proof_names += [
            "traceSignatures",
            "matchingRequests",
            "SolarTV.EVM.InteractionTrace.stable_request",
        ]
    if from_entry:
        if requests:
            code += """
theorem matchingEntry {Payload Halt : Type} (view : Payload → EVM.InteractionTrace.View)
    (ordinary : Handlers Payload Halt)
    (reply : EVM.InteractionTrace.Request → Payload → Except Halt Payload)
    (fuel : Nat) (world : EVM.InteractionTrace.Cursor Payload) :
    beforeProjected.map (observe (EVM.InteractionTrace.handlers view ordinary reply) fuel ⟨0,world⟩) =
      afterProjected.map (observe (EVM.InteractionTrace.handlers view ordinary reply) fuel ⟨0,world⟩) :=
  matchingRequests view ordinary reply fuel ⟨0,world⟩ (Nat.le_refl 0)
"""
        else:
            code += """
theorem matchingEntry {World Observation : Type} (handlers : Handlers World Observation)
    (fuel : Nat) (world : World) :
    beforeProjected.map (observe handlers fuel ⟨0,world⟩) =
      afterProjected.map (observe handlers fuel ⟨0,world⟩) :=
  matched handlers fuel ⟨0,world⟩ (Nat.le_refl 0)
"""
        proof_names.append("matchingEntry")
    if roots:
        code += """
theorem matchedFromEntryBound {World Observation : Type}
    (handlers : Handlers World Observation) (fuel : Nat) (state : State World)
    {pc : Nat}
    (reaches : EVM.StackReachability.Reaches BeforeFrame.graph BeforeFrame.signatures
      pc state.height) :
    beforeProjected.map (observe handlers fuel state) =
      afterProjected.map (observe handlers fuel state) :=
  matched handlers fuel state (BeforeFrame.entryFits reaches)
"""
        proof_names += [
            "BeforeFrame.trees",
            "BeforeFrame.entryFits",
            "AfterFrame.trees",
            "AfterFrame.entryFits",
            "matchedFromEntryBound",
        ]
    code += "".join(f"#print axioms {name}\n" for name in proof_names)
    built = build_model(timeout)
    if built.returncode:
        raise Unsupported(built.stdout + built.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-compact-pushes-") as folder:
        directory = Path(folder)
        env = None
        if roots:
            env = os.environ.copy()
            env["LEAN_PATH"] = os.pathsep.join(
                [folder, *filter(None, [env.get("LEAN_PATH")])]
            )
            imports = []
            # Check each caller independently before importing the theorems.
            for name, proposal in zip(
                ("BeforeFrame", "AfterFrame"), roots, strict=True
            ):
                path = directory / f"{name}.lean"
                path.write_text(
                    prelude + entry_stack.certificate(name, proposal, catalog, bound)
                )
                checked = run(
                    [
                        "lake",
                        "env",
                        "lean",
                        "-j1",
                        f"--root={folder}",
                        "-o",
                        path.with_suffix(".olean"),
                        path,
                    ],
                    ROOT,
                    timeout,
                    env=env,
                )
                if checked.returncode:
                    raise RecipeRejected(
                        f"{name} stack certificate did not close\n"
                        + checked.stdout
                        + checked.stderr
                    )
                imports.append(f"import {name}\n")
            code = code.replace(
                "import SolarTV\n", "import SolarTV\n" + "".join(imports), 1
            )
        proof = directory / "Pair.lean"
        proof.write_text(code)
        result = run(["lake", "env", "lean", "-j1", proof], ROOT, timeout, env=env)
    if result.returncode:
        raise RecipeRejected(
            "compact PUSH certificate did not close\n" + result.stdout + result.stderr
        )
    for theorem in proof_names:
        axioms = re.search(rf"'{theorem}' depends on axioms: \[(.*?)\]", result.stdout)
        if not (
            f"'{theorem}' does not depend on any axioms" in result.stdout
            or axioms
            and set(axioms[1].split(", "))
            <= {"propext", "Classical.choice", "Quot.sound"}
        ):
            raise Unsupported("unexpected compact PUSH proof axioms")
    proof_kind = "kernel-checked-compact-pushes"
    if from_entry:
        proof_kind += "-entry-trace"
    elif roots:
        proof_kind += "-entry-bound"
    elif requests:
        proof_kind += "-interaction-trace"
    programs = tuple(
        Program(
            ("evm-compact-pushes", str(bound)),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            selector=f"bb{model['nodes'][model['entry']]['label']}"
            if from_entry
            else entry,
            scope="compact-pushes",
            gas="unpriced-shared-interactions" if requests else "sufficient",
            evm_version=version,
            import_evidence={
                "kind": proof_kind,
                "source_sha256": hashlib.sha256(
                    json.dumps(model, sort_keys=True).encode()
                ).hexdigest(),
                "recipes": recipes,
                "entry_height_bound": bound,
                **(witnesses[side]["evidence"] if witnesses else {}),
                **({"interactions": requests} if requests else {}),
                **(
                    {"caller_stack": entry_stack.evidence(roots[side])} if roots else {}
                ),
                **(
                    {
                        "physical_entry": True,
                        "block_labels": sorted(witnesses[side]["block_labels"]),
                        "max_projected_stack_bound": max(bounds),
                    }
                    if from_entry
                    else {}
                ),
            },
        )
        for side, (text, model, recipes) in enumerate(
            zip(texts, sources, meters, strict=True)
        )
    )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=programs
    )
