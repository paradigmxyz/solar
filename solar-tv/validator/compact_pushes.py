"""Check literal materializations in independently exported EVM CFGs.

Python proposes runs of closed instructions. Lean checks the actual nodes,
the words they produce, their peak stack use and their charge on the selected
fork. The matched graph retains the continuation. The explicit interaction
mode also checks requests under shared replies. Both executions must be
funded through all reached charges and admission checks.
"""

import hashlib
import json
import re
import tempfile
from pathlib import Path

from . import (
    audit,
    cfg_flow,
    code_relocations,
    continuation_flow,
    correspondence,
    entry_stack,
    native,
    source_identity,
    stack_layout,
    stack_words,
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


def address_fold(model, start, protected):
    push = model["nodes"][start]["node"]
    duplicate = push["next"]
    copied = model["nodes"][duplicate]["node"]
    depth = re.fullmatch(r"dup ([0-9]+)", copied.get("operation", ""))
    if depth is None or not 2 <= int(depth[1]) <= 16:
        return None
    addition = copied["next"]
    add = model["nodes"][addition]["node"]
    if add.get("operation") != "add" or protected.intersection(
        (start, duplicate, addition)
    ):
        return None
    return int(depth[1]) - 2, add["next"]


def pair(
    models, catalog, version, protected=(frozenset(), frozenset()), *, from_entry=False
):
    points, indices = [], {}
    witnesses = [[], []]
    nodes, pushes, meters = [[], []], [[], []], [[], []]
    recipe_rows = {}
    local_edges = [[], []]
    carried = [None, None]
    folds, duplicates = [], []

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
            for side, node in enumerate((left, right)):
                nodes[side].append(node)
                witnesses[side].append(
                    f"⟨{positions[side]},[],{edge},none,{len(pushes[side])},none⟩"
                )
            continue
        if (
            positions[0] in protected[0]
            or left["kind"] != "operation"
            or not left["operation"].startswith("push ")
        ):
            raise Unsupported("compact PUSH pair changes a reachable nonliteral action")
        name = left["operation"]
        term, old_gas, value = instruction(name, catalog, version)
        before = left["next"]
        old_recipe, outputs = [term], [name]
        address = "none"
        copy = "none"
        old_peak = 1
        duplicate = re.fullmatch(r"dup (\d+)", right.get("operation", ""))
        if duplicate and positions[1] not in protected[1]:
            index_ = int(duplicate[1])
            if not 1 <= index_ <= 16:
                raise Unsupported("constant copy requires a legacy DUP")
            depth = index_ - 1
            if carried[1] is None:
                carried[1] = stack_words.Facts(models[1], catalog, protected[1])
            carried[1].demand(positions[1], depth, value)
            duplicates.append((positions[1], depth, value))
            copy = f"some ⟨{depth},{value}#256⟩"
            recipe, peak, after = [], 1, right["next"]
            gas = catalog[f"dup{index_}"]["forks"][version]["base_gas"]
        else:
            try:
                recipe, peak, gas, after = replacement(
                    models[1], positions[1], value, catalog, version, protected[1]
                )
            except Unsupported:
                folded = address_fold(models[0], positions[0], protected[0])
                if (
                    folded is None
                    or positions[1] in protected[1]
                    or right["kind"] != "operation"
                    or not right["operation"].startswith("push ")
                ):
                    raise
                depth, before = folded
                new_term, gas, result = instruction(
                    right["operation"], catalog, version
                )
                base = (result - value) & WORD_MASK
                if carried[0] is None:
                    carried[0] = stack_words.Facts(models[0], catalog, protected[0])
                carried[0].demand(positions[0], depth, base)
                folds.append((positions[0], value, depth, base))
                address = f"some ⟨{value}#256,{depth},{base}#256⟩"
                old_recipe, old_peak = [], 2
                old_gas += catalog[f"dup{depth + 2}"]["forks"][version]["base_gas"]
                old_gas += catalog["add"]["forks"][version]["base_gas"]
                recipe, peak, after = [new_term], 1, right["next"]
                outputs = [right["operation"]]
                value = result
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
            old_peak = len(outputs)
            term, charge, _ = instruction(outputs[-1], catalog, version)
            old_recipe.append(term)
            recipe.append(term)
            old_gas += charge
            gas += charge
            before, after = next_left["next"], next_right["next"]
        successor = index((before, after))
        recipe_rows[len(nodes[0])] = (len(pushes[0]), len(outputs), len(meters[0]))
        for side, (steps, growth, charge) in enumerate(
            ((old_recipe, old_peak, old_gas), (recipe, peak, gas))
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
                f"⟨{positions[side]},[{','.join(steps)}],.next {successor},"
                f"{address if side == 0 else 'none'},{guard['next']},"
                f"{copy if side == 1 else 'none'}⟩"
            )
            meters[side].append(
                {
                    "word": str(value),
                    "shared_pushes": len(outputs) - 1,
                    "operations": len(steps) or (3 if side == 0 else 1),
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
    return projected, witnesses, meters, carried, folds, duplicates, recipe_rows


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
    mask = ()
    if bound_from_entry:
        roots = [
            entry_stack.propose(text, entry, replay, catalog, version, timeout)
            for text in texts
        ]
        bound = max(max(root.bounds) for root in roots)
        mask = roots[0].mask()
        if mask != roots[1].mask():
            raise Unsupported(
                "the two physical entries disagree on the region's return slots"
            )
    witnesses = None
    sources = None
    if not (from_entry or interactions or mask):
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
                    mask=mask,
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
    relocated_code = code_relocations.bind(
        sources, texts, solar, version, timeout, catalog
    )
    projected, points, meters, carried, folds, duplicates, recipe_rows = pair(
        graphs, catalog, version, protected, from_entry=from_entry or bound_from_entry
    )
    chunked = from_entry and len(points[0]) > 2048
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
        + f"""
open EVM.SourceGraph (Program)
open EVM.ControlFlow (Edges)
open EVM.CompactPushes EVM.StackGraph
noncomputable section
set_option maxRecDepth {65536 if chunked else 16384}
set_option maxHeartbeats 3500000
"""
    )
    stages = []
    code = ""
    if mask:
        bits = ",".join(str(tag).lower() for tag in mask)
        code += f"\ndef callerMask : List Bool := [{bits}]\n"
    proof_names = [
        "matched",
        "SolarTV.EVM.CompactPushes.exact",
        "SolarTV.EVM.CompactPushes.funded",
    ]
    if relocated_code:
        proof_names += [
            "SolarTV.EVM.CodeRelocations.declared_read",
            "SolarTV.EVM.CodeRelocations.argument_read",
            "SolarTV.EVM.CodeRelocations.argument_length",
        ]
    if folds or duplicates:
        proof_names += [
            "SolarTV.EVM.StackWords.effect_relates",
            "SolarTV.EVM.StackWords.literal_relates",
            "SolarTV.EVM.StackWords.dup_relates",
            "SolarTV.EVM.StackWords.swap_relates",
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
                inherited=bool(mask),
            )
            code += cfg_flow.table(f"{label}Nodes", "EVM.SourceGraph.Node", rows)
            code += f"def {label} : Program := ⟨{start},{label}Nodes.rows.toArray⟩\n"
            if chunked:
                stages.append(code)
                code = ""
        code += cfg_flow.table(f"{name}Recipes", "Point", points[side])
        guard = "fun _ => false"
        if witnesses:
            code += cfg_flow.continuation_proof(
                name,
                witnesses[side],
                f"{name}Graph",
                inherited=bool(mask),
                chunked=chunked,
            )
            guard = f"EVM.ContinuationFlow.pushesLabel {name}SourceNodes.view {name}Points.view"
            if chunked:
                stages.append(code)
                code = ""
        prices = ",".join(str(step["gas"]) for step in meters[side])
        code += f"""
def {name}Protected : Nat → Bool := {guard}
"""
        facts = "(fun _ => [])"
        if carried[side]:
            code += carried[side].declarations(name, catalog, chunked=chunked)
            facts = f"{name}WordFacts.facts"
            proof_names.append(f"{name}WordFacts.constantAt")
        if side == 0:
            for i, (pc, offset, depth, base) in enumerate(folds):
                code += stack_words.fold_proof(i, pc, offset, depth, base, version)
                proof_names.append(f"foldAt{i}")
        else:
            for i, (pc, depth, word) in enumerate(duplicates):
                code += stack_words.duplicate_proof(i, pc, depth, word)
                proof_names.append(f"duplicateAt{i}")
        code += f"""
def {name}RecipeProjection (source : Program) : Option (Program × List Nat) :=
  project source .{version} {name}Recipes.rows.toArray {name}Protected {str(interactions).lower()} {facts}
def {name}Prices : List Nat := [{prices}]
"""
        if chunked:
            size = len(points[side])
            output_rows = []
            for index, node in enumerate(rows[:size]):
                if index in recipe_rows:
                    offset, count, meter = recipe_rows[index]
                    literals = ",".join(rows[size + offset : size + offset + count])
                    price = meters[side][meter]["gas"]
                    output_rows.append(f"⟨{node},[{literals}],some {price}⟩")
                else:
                    output_rows.append(f"⟨{node},[],none⟩")
            result = f"{name}Output"
            arguments = (
                f"{name}Recipes.view "
                f"{name}Protected {str(interactions).lower()} {facts}"
            )
            code += cfg_flow.table(result, "Row", output_rows)
            code += cfg_flow.check_rows(
                f"{name}RecipeRows",
                f"EVM.ProofTable.checkedRow {result}.view "
                f"(projectRow {name}GraphNodes.view .{version} {arguments})",
                size,
            )
            code += f"""
theorem {name}Checked : {name}RecipeProjection {name}Graph = some ({name},{name}Prices) :=
  project_rows {name}Graph .{version} {name}Recipes.rows.toArray {name}GraphNodes.view {arguments}
    {name} {name}Prices {result} {name}GraphNodesLookup {name}RecipesLookup {result}Lookup
    (by decide +kernel) (by rfl) (by rfl)
    (fun i inside => {name}RecipeRows i (Nat.zero_le _) inside)
"""
            stages.append(code)
            code = ""
        else:
            code += f"""
theorem {name}Checked : {name}RecipeProjection {name}Graph = some ({name},{name}Prices) :=
  project_checked {name}Graph .{version} {name}Recipes.rows.toArray
    {name}GraphNodes.view {name}Recipes.view {name}Protected {str(interactions).lower()} {facts} {name} {name}Prices
    {name}GraphNodesLookup {name}RecipesLookup (by decide +kernel)
"""
    code += cfg_flow.table("stackBounds", "Nat", list(map(str, bounds)))
    code += cfg_flow.signatures_declaration(signatures, catalog)
    code += """
def heights (i : Nat) : Nat := (stackBounds.get? i).getD 0
"""
    if chunked:
        code += cfg_flow.check_rows(
            "matchedHeights",
            "checkIndex beforeNodes.view afterNodes.view id heights signatures",
            len(bounds),
        )
        valid_check = "valid_rows"
        checked = "(fun i inside => matchedHeights i (Nat.zero_le _) inside)"
    else:
        valid_check = "valid_views"
        checked = "(by decide +kernel)"
    code += f"""
theorem valid : Valid before after id heights signatures {bound} :=
  {valid_check} before after beforeNodes.view afterNodes.view id heights signatures {bound}
    beforeNodesLookup afterNodesLookup (by decide +kernel) (by rfl)
    (by decide +kernel) {checked}
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
        projection = "projectInherited" if mask else "project"
        argument = " callerMask" if mask else ""
        for name in ("before", "after"):
            code += f"""
def {name}Projected : Option (Program × List Nat) :=
  (EVM.ContinuationFlow.{projection} {name}Source {name}Points.rows.toArray
    {name}Edges.rows.toArray{argument}).bind {name}RecipeProjection
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
        if mask:
            proof_names.append("SolarTV.EVM.ContinuationFlow.inherited_return_word")
            for name, root in zip(("BeforeFrame", "AfterFrame"), roots, strict=True):
                code += entry_stack.mask_proof(name, root)
                proof_names.append(f"{name}.callerLayout")
    code += "".join(f"#print axioms {name}\n" for name in proof_names)
    built = build_model(timeout)
    if built.returncode:
        raise Unsupported(built.stdout + built.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-compact-pushes-") as folder:
        directory = Path(folder)
        if chunked:
            result = cfg_flow.check_stages(directory, prelude, [*stages, code], timeout)
        else:
            env = None
            header = prelude
            if roots:
                try:
                    imports, env = entry_stack.compile_callers(
                        directory, roots, catalog, bound, prelude, timeout
                    )
                except Unsupported as error:
                    raise RecipeRejected(str(error)) from error
                header = header.replace(
                    "import SolarTV\n", "import SolarTV\n" + imports, 1
                )
            proof = directory / "Pair.lean"
            proof.write_text(header + code)
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
                **({"code_layout": relocated_code[side]} if relocated_code else {}),
                **({"address_folds": len(folds)} if folds else {}),
                **({"constant_duplicates": len(duplicates)} if duplicates else {}),
                **(
                    {"caller_return_slots": [i for i, tag in enumerate(mask) if tag]}
                    if mask
                    else {}
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
