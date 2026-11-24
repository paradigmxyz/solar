"""Prove jump-chain contraction and block coalescing in exported EVM graphs."""

import hashlib
import json
import os
import re
import subprocess
import tempfile
from itertools import batched
from pathlib import Path

from . import audit, branch_layout, regions, stack_layout
from .common import Program, Unsupported

ROW_STAGE_BREAK = "\n-- Separate kernel row-check stage.\n"


def projections(models, *, limit=2048):
    """Propose a reachable pairing. Lean checks all paths and edges separately."""
    indices = {}
    points = []
    edges = []
    projections = [{"entry": 0, "nodes": []}, {"entry": 0, "nodes": []}]

    def index(pair):
        # Index pairs, so one source point can match several on the other side.
        if pair not in indices:
            if len(points) >= limit:
                raise Unsupported(f"paired control-flow graph exceeds {limit} points")
            indices[pair] = len(points)
            points.append(pair)
        return indices[pair]

    def follow(model, position):
        growth = hops = 0
        while hops < len(model["nodes"]):
            node = model["nodes"][position]["node"]
            if node["kind"] != "jump":
                return growth, hops, dict(node)
            position = node["next"]
            growth = max(growth, node["growth"])
            hops += 1
        raise Unsupported("jump-only cycle has no control-flow projection")

    index(tuple(model["entry"] for model in models))
    jump_limit = 1
    for position, pair in enumerate(points):
        left, right = [follow(m, pc) for m, pc in zip(models, pair, strict=True)]
        jump_limit = max(jump_limit, left[1] + 1, right[1] + 1)
        a, b = left[2], right[2]
        fields = tuple(field for field in ("next", "yes", "no") if field in a)
        if {k: v for k, v in a.items() if k not in fields} != {
            k: v for k, v in b.items() if k not in fields
        }:
            raise Unsupported("control-flow projection changed an action or terminal")
        successors = [index((a[field], b[field])) for field in fields]
        match successors:
            case [target]:
                edges.append(f".next {target}")
            case [yes, no]:
                edges.append(f".branch {yes} {no}")
            case []:
                edges.append(".terminal")
        for field, successor in zip(fields, successors, strict=True):
            a[field] = b[field] = 2 * successor
        for model, (growth, _, node) in zip(projections, (left, right), strict=True):
            offset = 2 * position
            model["nodes"].extend(
                (
                    {
                        "id": offset,
                        "label": 0,
                        "node": {
                            "kind": "jump",
                            "growth": growth,
                            "next": offset + 1,
                        },
                    },
                    {"id": offset + 1, "label": 0, "node": node},
                )
            )
    left, right = zip(*points, strict=True)
    return (list(left), list(right)), projections, edges, jump_limit


def tree(rows):
    if len(rows) <= 12:
        return f"(.leaf [{','.join(rows)}])"
    middle = len(rows) // 2
    return f"(.branch {len(rows)} {tree(rows[:middle])} {tree(rows[middle:])})"


def table(name, row_type, rows, *, lookup=True):
    definition = f"""
def {name} : EVM.ProofTable.Tree {row_type} := {tree(rows)}
"""
    if not lookup:
        return definition
    return (
        definition
        + f"""
theorem {name}Lookup : {name}.view = EVM.ProofTable.ofArray {name}.rows.toArray :=
  {name}.view_eq (by decide +kernel)
"""
    )


def check_rows(name, check, size, *, start=0):
    """Give each kernel reduction a separate declaration and join the ranges."""
    declarations = [f"def {name}At : Nat → Bool := {check}\n"]

    def visit(start, count):
        if count <= 128:
            part = f"{name}At{start}"
            declarations.append(f"""
theorem {part} : EVM.StackReachability.CheckedRange {name}At {start} {count} :=
  EVM.StackReachability.checkedRange {name}At {start} {count} (by decide +kernel)
""")
            return part
        middle = count // 2
        left = visit(start, middle)
        right = visit(start + middle, count - middle)
        return f"(EVM.StackReachability.joinRanges {left} {right})"

    expression = visit(start, size)
    declarations.append(f"""
theorem {name} : EVM.StackReachability.CheckedRange {name}At {start} {size} :=
  {expression}
""")
    if len(declarations) <= 48:
        return "".join(declarations)
    # Large reachable graphs need hundreds of independent reductions. Release
    # each batch's elaboration state and keep the theorem that joins the ranges.
    batches = ["".join(batch) for batch in batched(declarations[:-1], 48)]
    return ROW_STAGE_BREAK.join(["", *batches, declarations[-1]])


def check_stages(directory, prelude, stages, timeout):
    """Release each large certificate's elaboration state before the next."""
    from .check import ROOT, run

    env = os.environ.copy()
    env["LEAN_PATH"] = os.pathsep.join(
        [str(directory), *filter(None, [env.get("LEAN_PATH")])]
    )
    previous = None
    stdout = []
    modules = (part for stage in stages for part in stage.split(ROW_STAGE_BREAK))
    for index, stage in enumerate(modules):
        if not stage.strip():
            continue
        path = directory / f"CfgStage{index}.lean"
        header = (
            prelude.replace(
                "import SolarTV\n", f"import SolarTV\nimport {previous}\n", 1
            )
            if previous
            else prelude
        )
        path.write_text(header + stage)
        result = run(
            [
                "lake",
                "env",
                "lean",
                "-j1",
                f"--root={directory}",
                "-o",
                path.with_suffix(".olean"),
                path,
            ],
            ROOT,
            timeout,
            env=env,
        )
        stdout.append(result.stdout)
        if result.returncode:
            return subprocess.CompletedProcess(
                result.args, result.returncode, "".join(stdout), result.stderr
            )
        previous = path.stem
    return subprocess.CompletedProcess([], 0, "".join(stdout), "")


def projection_proof(
    name, module, program, nodes, points, edges, result, limit="", *, size=None
):
    prefix = f"{program} {points}.rows.toArray {edges}.rows.toArray {limit}"
    if size is not None:
        check = (
            f"{module}.checkRow {nodes}.view {points}.view {edges}.view "
            f"{result}Nodes.view {limit}"
        )
        return (
            check_rows(f"{name}Rows", check, size)
            + f"""
theorem {name} : {module}.project {prefix} = some {result} :=
  {module}.project_rows {prefix}
    {nodes}.view {points}.view {edges}.view {result} {result}Nodes.view
    {nodes}Lookup {points}Lookup {edges}Lookup {result}NodesLookup
    (by decide +kernel) (by rfl) (by rfl)
    (fun i inside => {name}Rows i (Nat.zero_le _) inside)
"""
        )
    return f"""
theorem {name} : {module}.project {prefix} = some {result} :=
  {module}.project_checked {program} {points}.rows.toArray {edges}.rows.toArray
    {nodes}.view {points}.view {edges}.view {limit} {result}
    {nodes}Lookup {points}Lookup {edges}Lookup (by decide +kernel)
"""


def continuation_proof(name, witness, result, *, inherited=False, chunked=False):
    """Bind the continuation projection to the independently exported graph."""
    points = witness["points"]
    shapes = ""
    if chunked:
        ids = {}
        points = []
        for pc, shape in witness["point_data"]:
            type_name = f"{name}Shape{len(ids)}"
            shape = tuple(shape)
            if shape not in ids:
                ids[shape] = type_name
                tags = ",".join(
                    "none" if tag is None else f"some {tag}" for tag in shape
                )
                shapes += f"def {type_name} : EVM.ContinuationFlow.Shape := [{tags}]\n"
            points.append(f"⟨{pc},{ids[shape]}⟩")
    tables = (
        shapes
        + table(f"{name}SourceNodes", "EVM.ContinuationFlow.Node", witness["source"])
        + table(f"{name}Points", "EVM.ContinuationFlow.Point", points)
        + table(f"{name}Edges", "Edges", witness["edges"])
        + f"""
def {name}Source : EVM.ContinuationFlow.Program :=
  ⟨{witness["entry"]},{name}SourceNodes.rows.toArray⟩
"""
    )
    if inherited:
        return (
            tables
            + f"""
theorem {name}Provenance :
    EVM.ContinuationFlow.projectInherited {name}Source {name}Points.rows.toArray
      {name}Edges.rows.toArray callerMask = some {result} :=
  EVM.ContinuationFlow.inherited_checked {name}Source {name}Points.rows.toArray
    {name}Edges.rows.toArray callerMask {name}SourceNodes.view {name}Points.view {name}Edges.view
    {result} {name}SourceNodesLookup {name}PointsLookup {name}EdgesLookup (by decide +kernel)
"""
        )
    if chunked:
        check = (
            f"EVM.ProofTable.checkedRow {result}Nodes.view "
            f"(fun i => EVM.ContinuationFlow.projectRow {name}SourceNodes.view "
            f"{name}Points.view {name}Edges.view i)"
        )
        return (
            tables
            + check_rows(f"{name}ProvenanceRows", check, len(points))
            + f"""
theorem {name}Provenance :
    EVM.ContinuationFlow.project {name}Source {name}Points.rows.toArray
      {name}Edges.rows.toArray = some {result} :=
  EVM.ContinuationFlow.project_rows {name}Source {name}Points.rows.toArray
    {name}Edges.rows.toArray {name}SourceNodes.view {name}Points.view {name}Edges.view
    {result} {result}Nodes.view
    {name}SourceNodesLookup {name}PointsLookup {name}EdgesLookup {result}NodesLookup
    (by decide +kernel) (by rfl) (by rfl)
    (fun i inside => {name}ProvenanceRows i (Nat.zero_le _) inside)
"""
        )
    return tables + projection_proof(
        f"{name}Provenance",
        "EVM.ContinuationFlow",
        f"{name}Source",
        f"{name}SourceNodes",
        f"{name}Points",
        f"{name}Edges",
        result,
    )


def signatures_declaration(signature_table, catalog):
    # Keep literal operands in the graph without searching through them for arities.
    push = catalog["push1"]["stack"]
    rows = ",".join(
        f"({json.dumps(name)},({inputs},{outputs}))"
        for name, (inputs, outputs) in signature_table.items()
        if not name.startswith("push ")
    )
    return f"""
def signatures (name : String) : Option (Nat × Nat) :=
  if name.startsWith "push " then some ({push[0]},{push[1]}) else
    ([{rows}] : List (String × (Nat × Nat))).lookup name
"""


def check(
    texts,
    models,
    entry,
    catalog,
    version,
    timeout,
    *,
    continuations=None,
    interactions=False,
    from_entry=False,
    code_layout=None,
    callers=None,
    mask=(),
):
    from . import entry_stack
    from .check import PRELUDE, ROOT, build_model, run
    from .source_identity import INTERACTION_OPS, render_nodes

    points, normalized, edges, jump_limit = projections(
        models, limit=16384 if from_entry else 2048
    )
    chunked = len(points[0]) > 2048
    signature_table = stack_layout.signatures(normalized[0], catalog)
    entry_bound = (
        0
        if from_entry
        else max(max(caller.bounds) for caller in callers)
        if callers
        else 1024
    )
    bounded = from_entry or callers is not None
    state_parameter = "(payload : {type})" if from_entry else "(state : State {type})"
    if callers:
        state_parameter += f" (fits : state.height ≤ {entry_bound})"
    state_argument = state_parameter.format(type="World")
    state_value = "⟨0,payload⟩" if from_entry else "state"
    argument = "payload" if from_entry else "state fits" if callers else "state"
    identical = normalized[0] == normalized[1]
    if identical:
        bounds, changed = [], []
    else:
        _, bounds, changed = stack_layout.certificate(
            *normalized, signature_table, entry_bound=entry_bound
        )
    programs_code = []
    for name, model in zip(
        ("before", "after", "old", "new"), (*models, *normalized), strict=True
    ):
        if name == "new" and identical:
            declaration = "def new : Program := old\n"
            if chunked:
                declaration += """
def newNodes := oldNodes
theorem newNodesLookup : newNodes.view = EVM.ProofTable.ofArray new.nodes := oldNodesLookup
"""
            programs_code.append(declaration)
            continue
        root, rows = render_nodes(
            model,
            catalog,
            version,
            "bb0" if continuations or name in {"old", "new"} else entry,
            dynamic_exit=True,
            interactions=interactions,
            from_entry=bounded,
            max_growth=4 if bounded else 1,
            inherited=bool(mask),
        )
        declaration = table(f"{name}Nodes", "EVM.SourceGraph.Node", rows)
        declaration += f"def {name} : Program := ⟨{root},{name}Nodes.rows.toArray⟩\n"
        programs_code.append(declaration)
    provenance = ""
    import_code = []
    provenance_theorems = []
    contextual = ""
    continuation_project = "projectInherited" if mask else "project"
    mask_argument = " callerMask" if mask else ""
    if mask:
        bits = ",".join(str(tag).lower() for tag in mask)
        provenance += f"def callerMask : List Bool := [{bits}]\n"
        theorem = "SolarTV.EVM.ContinuationFlow.inherited_return_word"
        provenance += f"#print axioms {theorem}\n"
        provenance_theorems.append(theorem)
    if continuations is not None:
        checked_tables = False
        for side, (model, (_, witness)) in enumerate(
            zip(models, continuations, strict=True)
        ):
            tables = witness["indexed_tables"]
            if not tables:
                continue
            signatures = stack_layout.signatures(model, catalog)
            _, heights, _ = stack_layout.certificate(
                model, model, signatures, entry_bound=0
            )
            if any(
                heights[pc] + model["nodes"][pc]["node"]["growth"] > 1024
                for pc in tables
            ):
                raise Unsupported("literal indexed jump may exceed the stack limit")
            witness["evidence"]["literal_table_entry_bounds"] = [
                heights[pc] for pc in tables
            ]
            name = f"IndexedTables{side}"
            scope = entry_stack.Caller(witness, signatures, heights, 0)
            theorem = f"{name}.indexedFits"
            import_code.append(
                entry_stack.certificate(name, scope, catalog, 1024, indexed=True)
                + f"#print axioms {theorem}\n"
            )
            provenance_theorems.append(theorem)
            checked_tables = True
        if checked_tables:
            theorem = "SolarTV.EVM.IndexedJump.literal_transfer"
            provenance += f"#print axioms {theorem}\n"
            provenance_theorems.append(theorem)
        imports = []
        import_theorems = []
        for lemma in (
            "transfer_tags",
            "checked_operands",
            "swap_encoding",
            "return_word",
            "conditional_words",
            "conditional_admission",
        ):
            theorem = f"SolarTV.EVM.ContinuationFlow.{lemma}"
            provenance += f"#print axioms {theorem}\n"
            provenance_theorems.append(theorem)
        for name, (_, witness) in zip(("before", "after"), continuations, strict=True):
            theorem = f"{name}Provenance"
            provenance_theorems.append(theorem)
            proof = continuation_proof(
                name, witness, name, inherited=bool(mask), chunked=chunked
            )
            proof += f"#print axioms {theorem}\n"
            import_code.append(proof)
            comparison = witness.get("comparison")
            projection = (
                f"EVM.ContinuationFlow.{continuation_project} "
                f"{{source}} {name}Points.rows.toArray {name}Edges.rows.toArray{mask_argument}"
            )
            if comparison:
                proof = branch_layout.certificate(name, *comparison)
                provenance_theorems.append(f"{name}Comparisons")
                proof += f"#print axioms {name}Comparisons\n"
                import_code.append(proof)
                source = f"EVM.BranchLayout.normalize {name}Original {name}Sites"
                project = f"fun source => {projection.format(source='source')}"
                imported = f"(({source}).bind ({project}))"
                import_code.append(f"""
theorem {name}Import : {imported} = some {name} :=
  (Option.bind_eq_some_iff (x := {source}) (f := {project}) (b := {name})).mpr
    ⟨{name}Source, {name}Comparisons, {theorem}⟩
""")
                imports.append(imported)
                import_theorems.append(f"{name}Import")
            else:
                imports.append(f"({projection.format(source=f'{name}Source')})")
                import_theorems.append(theorem)
        before_import, after_import = imports
        before_theorem, after_theorem = import_theorems
        if any(witness.get("comparison") for _, witness in continuations):
            theorem = "SolarTV.EVM.BranchLayout.eq_sub_swapped"
            provenance += f"#print axioms {theorem}\n"
            provenance_theorems.append(theorem)
        provenance_theorems.append("matchedContinuations")
        contextual = f"""
theorem matchedContinuations {{World Observation : Type}} (handlers : Handlers World Observation)
    (work : Nat) {state_argument} :
    ({before_import}.bind (fun p => project p sources edges {jump_limit})).map
        (fun p => execute p signatures handlers work p.entry {state_value}) =
      ({after_import}.bind (fun p => project p targets edges {jump_limit})).map
        (fun p => execute p signatures handlers work p.entry {state_value}) :=
  EVM.ControlFlow.compose {before_theorem} {after_theorem}
    (fun p => project p sources edges {jump_limit}) (fun p => project p targets edges {jump_limit})
    (fun p => execute p signatures handlers work p.entry {state_value})
    (matched handlers work {argument})
#print axioms matchedContinuations
"""
    if interactions:
        provenance_theorems.extend(
            (
                "traceSignatures",
                "matchingRequests",
                "SolarTV.EVM.InteractionTrace.stable_request",
            )
        )
        names = ",".join(
            json.dumps(name)
            for name in sorted(INTERACTION_OPS & signature_table.keys())
        )
        trace_argument = state_parameter.format(
            type="(EVM.InteractionTrace.Cursor Payload)"
        )
        contextual += f"""
theorem traceSignatures :
    ([{names}] : List String).all (EVM.InteractionTrace.signatureFits signatures) = true :=
  by decide +kernel
theorem matchingRequests {{Payload Halt : Type}} (view : Payload → EVM.InteractionTrace.View)
    (ordinary : Handlers Payload Halt)
    (reply : EVM.InteractionTrace.Request → Payload → Except Halt Payload)
    (work : Nat) {trace_argument} :
    ({before_import}.bind (fun p => project p sources edges {jump_limit})).map
        (fun p => execute p signatures (EVM.InteractionTrace.handlers view ordinary reply)
          work p.entry {state_value}) =
      ({after_import}.bind (fun p => project p targets edges {jump_limit})).map
        (fun p => execute p signatures (EVM.InteractionTrace.handlers view ordinary reply)
          work p.entry {state_value}) :=
  matchedContinuations (EVM.InteractionTrace.handlers view ordinary reply) work {argument}
#print axioms traceSignatures
#print axioms matchingRequests
#print axioms EVM.InteractionTrace.stable_request
"""
    tables = (
        table("sourcePoints", "Nat", list(map(str, points[0])))
        + table("targetPoints", "Nat", list(map(str, points[1])))
        + table("transferEdges", "Edges", edges)
    )
    projections_code = [
        projection_proof(
            f"{name}Projection",
            "EVM.ControlFlow",
            source,
            f"{source}Nodes",
            point_table,
            "transferEdges",
            name,
            str(jump_limit),
            size=len(points[0]) if chunked else None,
        )
        for name, source, point_table in (
            ("old", "before", "sourcePoints"),
            ("new", "after", "targetPoints"),
        )
    ]
    if identical:
        validity = ""
        match_proof = f"""congrArg (Option.map (fun p : Program => execute p signatures handlers work p.entry {state_value}))
    (oldProjection.trans newProjection.symm)"""
    else:
        validity = (
            table("heightBounds", "Nat", list(map(str, bounds)))
            + """
def bounds (i : Nat) : Nat := (heightBounds.get? i).getD 0
"""
        )
        if chunked:
            validity += check_rows(
                "matchedHeights",
                "checkIndex oldNodes.view newNodes.view id bounds signatures",
                len(bounds),
            )
            check = "(fun i inside => matchedHeights i (Nat.zero_le _) inside)"
            validator = "valid_rows"
        else:
            check = "(by decide +kernel)"
            validator = "valid_views"
        validity += f"""
theorem valid : Valid old new id bounds signatures {entry_bound} :=
  {validator} old new oldNodes.view newNodes.view id bounds signatures {entry_bound}
    oldNodesLookup newNodesLookup (by decide +kernel) (by rfl) (by decide +kernel)
    {check}
"""
        lemma = "contextual_bounded" if bounded else "contextual"
        admission = " (Nat.le_refl 0)" if from_entry else " fits" if callers else ""
        match_proof = f"""EVM.ControlFlow.{lemma} before after old new sources targets edges {jump_limit}
    signatures bounds oldProjection newProjection valid handlers work {state_value}{admission}"""
    prelude = (
        PRELUDE
        + f"""
open EVM.SourceGraph (Program)
open EVM.StackGraph EVM.ControlFlow
noncomputable section
set_option maxRecDepth {65536 if chunked else 16384}
set_option maxHeartbeats 3500000
"""
    )
    common = f"""
{tables}
def sources : Array Nat := sourcePoints.rows.toArray
def targets : Array Nat := targetPoints.rows.toArray
def edges : Array Edges := transferEdges.rows.toArray
{signatures_declaration(signature_table, catalog)}
"""
    final = f"""
theorem matched {{World Observation : Type}} (handlers : Handlers World Observation)
    (work : Nat) {state_argument} :
    (project before sources edges {jump_limit}).map
        (fun p => execute p signatures handlers work p.entry {state_value}) =
      (project after targets edges {jump_limit}).map
        (fun p => execute p signatures handlers work p.entry {state_value}) :=
  {match_proof}
#print axioms matched
#print axioms EVM.ControlFlow.follow_execute
#print axioms EVM.ControlFlow.resolve_edge
{contextual}
"""
    if callers:
        final += f"""
theorem matchedAtCaller {{World Observation : Type}} (handlers : Handlers World Observation)
    (work : Nat) (state : State World) {{pc : Nat}}
    (reaches : EVM.StackReachability.Reaches BeforeFrame.graph BeforeFrame.signatures
      pc state.height) :
    ({before_import}.bind (fun p => project p sources edges {jump_limit})).map
        (fun p => execute p signatures handlers work p.entry state) =
      ({after_import}.bind (fun p => project p targets edges {jump_limit})).map
        (fun p => execute p signatures handlers work p.entry state) :=
  matchedContinuations handlers work state (BeforeFrame.entryFits reaches)
#print axioms matchedAtCaller
#print axioms BeforeFrame.entryFits
#print axioms AfterFrame.entryFits
"""
        provenance_theorems += [
            "matchedAtCaller",
            "BeforeFrame.entryFits",
            "AfterFrame.entryFits",
        ]
        if mask:
            for name, caller in zip(
                ("BeforeFrame", "AfterFrame"), callers, strict=True
            ):
                final += entry_stack.mask_proof(name, caller)
                final += f"#print axioms {name}.callerLayout\n"
                provenance_theorems.append(f"{name}.callerLayout")
    built = build_model(timeout)
    if built.returncode:
        raise Unsupported(built.stdout + built.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-cfg-flow-") as folder:
        stages = [
            *programs_code,
            provenance,
            *import_code,
            common,
            *projections_code,
            validity,
            final,
        ]
        if chunked:
            result = check_stages(Path(folder), prelude, stages, timeout)
        else:
            code = prelude + "\n".join(stages)
            proof = Path(folder) / "Pair.lean"
            env = None
            if callers:
                imports, env = entry_stack.compile_callers(
                    proof.parent, callers, catalog, entry_bound, prelude, timeout
                )
                code = code.replace("import SolarTV\n", "import SolarTV\n" + imports, 1)
            proof.write_text(code)
            result = run(["lake", "env", "lean", "-j1", proof], ROOT, timeout, env=env)
    if result.returncode:
        raise Unsupported(
            "control-flow path certificate did not close\n"
            + result.stdout
            + result.stderr
        )
    for theorem in (
        "matched",
        "SolarTV.EVM.ControlFlow.follow_execute",
        "SolarTV.EVM.ControlFlow.resolve_edge",
        *provenance_theorems,
    ):
        axioms = re.search(rf"'{theorem}' depends on axioms: \[(.*?)\]", result.stdout)
        if not (
            f"'{theorem}' does not depend on any axioms" in result.stdout
            or axioms
            and set(axioms[1].split(", "))
            <= {"propext", "Classical.choice", "Quot.sound"}
        ):
            raise Unsupported("unexpected control-flow proof axioms")
    programs = []
    for index, (text, model, paired) in enumerate(
        zip(texts, models, points, strict=True)
    ):
        source, witness = continuations[index] if continuations else (model, None)
        labels = {f"bb{row['label']}" for row in source["nodes"]}
        projection = (
            "@module ControlFlow\n"
            + "\n".join(
                f"{label}:\n" + "\n".join(body)
                for label, body in regions.blocks(text).items()
                if label in labels
            )
            + "\n"
        )
        programs.append(
            Program(
                ("evm-cfg-flow", entry),
                [],
                "",
                "",
                canonical_ir=projection,
                replay_ir=text,
                selector=entry,
                scope=(
                    "layout-flow"
                    if bounded
                    else "interaction-flow"
                    if interactions
                    else "continuation-flow"
                    if witness
                    else "cfg-flow"
                ),
                evm_version=version,
                import_evidence={
                    "kind": "kernel-checked-entry-layout"
                    if from_entry
                    else "kernel-checked-entry-bound-layout"
                    if callers
                    else "kernel-checked-interaction-trace"
                    if interactions
                    else "kernel-checked-continuation-flow"
                    if witness
                    else "kernel-checked-jump-chain-projection",
                    "graph_sha256": hashlib.sha256(
                        json.dumps(model, sort_keys=True).encode()
                    ).hexdigest(),
                    "logical_actions": len(points[0]),
                    "source_points": len(model["nodes"]),
                    "reused_points": len(paired) - len(set(paired)),
                    "stack_height_bounds": bounds,
                    "changed_growth_checks": changed,
                    "boundary": "compiler source graph and schema; position-independent opcode handlers; all logical steps with temporary stack growth; no physical gas claim",
                    **(witness["evidence"] if witness else {}),
                    **(
                        {
                            "physical_entry": True,
                            "entry_height_bound": entry_bound,
                            "block_labels": sorted(witness["block_labels"]),
                        }
                        if from_entry
                        else {}
                    ),
                    **({"code_layout": code_layout[index]} if code_layout else {}),
                    **(
                        {
                            "caller_stack": entry_stack.evidence(callers[index]),
                            "entry_height_bound": entry_bound,
                            "caller_return_slots": [
                                i for i, tag in enumerate(mask) if tag
                            ],
                        }
                        if callers
                        else {}
                    ),
                },
            )
        )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=tuple(programs)
    )
