"""Kernel-checked structural correspondence with an independent compiler CFG.

The compiler's Rust parser exports operation identities and control edges. Its
normalizer and the opcode-to-Lean mapping remain trusted. A certificate checks
the separate Python tree against that graph before any pair can be verified.
"""

import hashlib
import json
import re
import subprocess
import tempfile
from pathlib import Path

from . import evmir, memory, native, regions
from .common import Inconclusive, Unsupported


def export(text, entry_label, version, executable, timeout, *, labels=False):
    """Obtain the independent reachable graph without assigning opcode semantics."""
    response = subprocess.run(
        [executable, "--evm-model"],
        input=json.dumps(
            {
                "version": 1,
                "text": text,
                "entry": int(entry_label.removeprefix("bb")),
                "evm_version": version,
                "labels": labels,
            }
        ),
        text=True,
        capture_output=True,
        check=False,
        timeout=timeout,
    )
    try:
        value = json.loads(response.stdout)
    except ValueError:
        raise Unsupported("invalid independent EVM source export") from None
    if response.returncode or value.get("version") != 1 or value.get("status") != "ok":
        raise Unsupported("EVM source graph: " + value.get("reason", "invalid export"))
    model = value["model"]
    rows = model["nodes"]
    if (
        not rows
        or len(rows) > 4096
        or [row["id"] for row in rows] != list(range(len(rows)))
    ):
        raise Unsupported("invalid independent EVM graph indices")
    return model


def certify(program, executable, timeout, *, entry_label, stack_height):
    catalog = {row["name"]: row for row in native.catalog(executable, timeout)["evmir"]}
    model = export(
        program.replay_ir, entry_label, program.evm_version, executable, timeout
    )
    rows = model["nodes"]

    def target(node, name):
        value = node[name]
        if type(value) is not int or not 0 <= value < len(rows):
            raise Unsupported("invalid independent EVM graph edge")
        return value

    nodes = []
    for row in rows:
        node = row["node"]
        match node["kind"]:
            case "operation":
                operations, _, _, _ = evmir.instructions(
                    [node["operation"]],
                    None,
                    memory=True,
                    context=program.scope == "universal-region",
                )
                if len(operations) != 1:
                    raise Unsupported("EVM graph operation was not preserved")
                if program.scope == "cyclic-region" and not operations[0].startswith(
                    ".stack ("
                ):
                    raise Unsupported(
                        "cyclic memory/state operations require an invariant"
                    )
                rendered = f".step (EVM.Region.effect ctx ({operations[0]})) {target(node, 'next')}"
            case "jump":
                if node["growth"] not in {0, 1}:
                    raise Unsupported("invalid source transfer growth")
                rendered = f".step (EVM.Region.jumpEffect {node['growth']}) {target(node, 'next')}"
            case "branch":
                rendered = f".branch EVM.Region.conditionEffect {target(node, 'no')} {target(node, 'yes')}"
            case "terminal":
                operation = node["operation"]
                if program.scope == "cyclic-region" and operation not in {
                    "stop",
                    "invalid",
                }:
                    raise Unsupported(
                        "cyclic output requires a memory correspondence invariant"
                    )
                if operation in {"stop", "invalid"}:
                    terminal = f".control .{operation}"
                elif operation in {"return", "revert"}:
                    terminal = f".output {'true' if operation == 'revert' else 'false'}"
                else:
                    raise Unsupported("unmodeled source graph terminal")
                rendered = f".terminal (fun state => EVM.Memory.finish ({terminal}) state.1 state.2)"
            case _:
                raise Unsupported("unknown independent EVM source node")
        nodes.append(rendered)
    entry = target(model, "entry")
    fuel = len(rows) + 1
    graph = f"""
def __IMPORT___graph (ctx : EVM.Context) : Correspondence.Graph EVM.Region.State EVM.Memory.Execution :=
  fun index => ((#[{",".join(nodes)}] : Array (Correspondence.Node EVM.Region.State EVM.Memory.Execution))[index]?)
"""
    if program.scope == "cyclic-region":
        program.interface += (str(entry),)
        program.import_certificate = graph
        program.import_evidence = {
            "kind": "independent-source-graph",
            "nodes": len(rows),
            "boundary": "trusted Rust parser/source normalizer and opcode mapping",
        }
        reachable = {f"bb{row['label']}" for row in rows}
        program.canonical_ir = (
            "@module Region\n"
            + "\n".join(
                f"{label}:\n" + "\n".join(body)
                for label, body in regions.blocks(program.replay_ir).items()
                if label in reachable
            )
            + "\n"
        )
        return
    # Bind the checked tree to the actual function used by the pair proof, not
    # merely to an unused code declaration. The requested entry and input shape
    # come from the validated source selection, independently of the importer.
    universal = stack_height is None
    if universal and (
        program.interface != ("evm-universal-region", "memory")
        or program.inputs
        or program.input_widths
        or program.body != "EVM.Region.execute ctx __IMPORT___code stack memory"
        or program.result_type != "EVM.Memory.Execution"
        or not program.memory
        or program.scope != "universal-region"
    ):
        raise Unsupported("EVM import changed the universal entry interface")
    stack = [f"stack{i}" for i in range(stack_height or 0)]
    inputs = ["calldatasize", "returndatasize", *stack, *memory.INPUTS]
    widths = (*(256 for _ in range(2 + (stack_height or 0))), *memory.WIDTHS)
    if not universal and (
        program.inputs != inputs
        or program.input_widths != widths
        or program.interface != ("evm-region", str(stack_height), "memory")
        or program.result_type != "EVM.Memory.Execution"
        or not program.memory
        or program.scope != "region"
    ):
        raise Unsupported("EVM import changed the selected entry interface")
    program.import_certificate = (
        graph
        + f"""
def __IMPORT___code : EVM.Region.Code := {program.region_code}
theorem __IMPORT___import_checked (ctx : EVM.Context) :
  Correspondence.expand (__IMPORT___graph ctx) {fuel} {entry} =
    some (EVM.Region.toTree ctx __IMPORT___code) := by rfl
theorem __IMPORT___source_correct (ctx : EVM.Context) (state : EVM.Region.State) :
  Correspondence.evalGraph (__IMPORT___graph ctx) {fuel} {entry} state =
    some (EVM.Region.execute ctx __IMPORT___code state.1 state.2) := by
  rw [Correspondence.expansion_correct _ _ _ _ (__IMPORT___import_checked ctx)]
  exact congrArg some (EVM.Region.toTree_correct ctx __IMPORT___code state)
"""
    )
    program.import_evidence = {
        "kind": "kernel-checked-cfg-expansion",
        "nodes": len(rows),
        "graph_sha256": hashlib.sha256(
            json.dumps(model, sort_keys=True).encode()
        ).hexdigest(),
        "boundary": "trusted compiler parser/source normalizer and opcode mapping; projected EVM stack/memory semantics",
    }
    if universal:
        program.stack_required, program.stack_peak = stack_bounds(rows, entry, catalog)
        program.import_obligation = f"""
theorem __IMPORT___entry_checked (ctx : EVM.Context) (stack : EVM.Stack) (memory : Memory.State) :
  Correspondence.evalGraph (__IMPORT___graph ctx) {fuel} {entry} (stack,memory) =
    some (__IMPORT__ ctx stack memory) := by
  rw [__IMPORT___source_correct]
  rfl
"""
        return
    binders = " ".join(
        f"({name} : BitVec {width})" for name, width in zip(inputs, widths, strict=True)
    )
    program.import_obligation = f"""
theorem __IMPORT___entry_checked {binders} :
  Correspondence.evalGraph (__IMPORT___graph ⟨calldatasize,returndatasize,0,[], 0⟩) {fuel} {entry}
    ([{",".join(stack)}],({memory.INITIAL})) = some (__IMPORT__ {" ".join(inputs)}) := by
  rw [__IMPORT___source_correct]
  rfl
"""


def check_cyclic(a, b, timeout):
    from . import audit
    from .check import PRELUDE, ROOT, build_model, definition, run

    if a.scope != b.scope or a.interface != b.interface:
        raise Unsupported(
            "cyclic regions require a source-graph correspondence invariant"
        )
    build = build_model(timeout)
    if build.returncode:
        raise Unsupported(build.stdout + build.stderr)
    theorem = """
theorem graphs_equal (ctx : EVM.Context) : before_graph ctx = after_graph ctx := by rfl
theorem all_finite_executions (ctx : EVM.Context) (fuel entry : Nat) (state : EVM.Region.State) :
    Correspondence.evalGraph (before_graph ctx) fuel entry state =
      Correspondence.evalGraph (after_graph ctx) fuel entry state := by rw [graphs_equal]
#print axioms all_finite_executions
"""
    with tempfile.TemporaryDirectory(prefix="solar-tv-loop-proof-") as folder:
        path = Path(folder) / "Loop.lean"
        path.write_text(
            PRELUDE + definition("before", a) + definition("after", b) + theorem
        )
        result = run(["lake", "env", "lean", path], ROOT, timeout)
    if result.returncode:
        raise Unsupported(
            "cyclic region needs an invariant beyond graph isomorphism\n"
            + result.stdout
            + result.stderr
        )
    axioms = re.search(
        r"'all_finite_executions' depends on axioms: \[(.*?)\]", result.stdout
    )
    if (
        "'all_finite_executions' does not depend on any axioms" not in result.stdout
        and (
            not axioms
            or set(axioms[1].split(", "))
            - {"propext", "Quot.sound", "Classical.choice"}
        )
    ):
        raise Inconclusive("unexpected cyclic correspondence axioms")
    return audit.Result(
        "verified",
        "verified (unbounded graph correspondence)\n",
        program=a,
        pair=(a, b),
    )


def certify_scalar_mir(program, solar, executable, timeout):
    result = re.fullmatch(r"Outcome \(BitVec (\d+)\)", program.result_type)
    # The scalar importer already rejects cyclic CFGs. Memory, actual calls and
    # symbolic checked powers still require additional correspondence lemmas.
    if (
        program.memory
        or not result
        or re.search(r"^\s*(?:v\d+ = )?icall\b", program.canonical_ir, re.MULTILINE)
        or (
            program.inputs
            and re.search(
                r"^\s*v\d+ = checked_pow\b", program.canonical_ir, re.MULTILINE
            )
        )
    ):
        return
    result_width = int(result[1])
    from . import mir_state, world
    from .world_check import MIR_UNFOLD

    native.catalog(executable, timeout)
    with tempfile.TemporaryDirectory(prefix="solar-tv-scalar-import-") as folder:
        path = Path(folder) / "world.json"
        path.write_text('{"version":1}')
        fixture = world.read(path, solar, None, kind="mir")
    source = mir_state.translate(
        program.replay_ir or program.canonical_ir,
        solar,
        program.evm_version,
        fixture,
        timeout,
        program.selector or program.interface[0],
        executable,
    )
    if source.world["entry"]:
        return
    boundary = source.world["boundary"]
    if (
        source.inputs != program.inputs
        or source.input_widths != program.input_widths
        or any(ty.get("kind", "word") != "word" for ty in boundary["params"])
        or len(boundary["returns"]) != 1
        or boundary["returns"][0].get("kind", "word") != "word"
        or boundary["returns"][0]["bits"] != result_width
    ):
        raise Unsupported("MIR import changed the typed scalar interface")
    config = (
        source.world["configuration"]
        .replace("mirExternalConfig", "__IMPORT___external_config")
        .replace("mirWorldConfig", "__IMPORT___world_config")
    )
    body = source.body.replace("mirWorldConfig", "__IMPORT___world_config")
    args = " ".join(program.inputs)
    binders = " ".join(
        f"({arg} : BitVec {width})"
        for arg, width in zip(program.inputs, program.input_widths, strict=True)
    )
    program.import_certificate = (
        config
        + f"def __IMPORT___source {binders} : MIR.State.Run :=\n"
        + "\n".join("  " + line for line in body.splitlines())
        + "\n"
    )
    unfold = (
        MIR_UNFOLD.replace("before, after,", "__IMPORT___source, __IMPORT__,")
        .replace("mirWorldConfig", "__IMPORT___world_config")
        .replace("tvWorldConfig, ", "")
        + ", MIR.State.scalarOutcome, MIR.choose, MIR.phi, MIR.branch, MIR.cast, MIR.checked, MIR.guard, MIR.select, MIR.ternary, Outcome.bind, BitVec.add_comm"
    )
    # Closed executions need no algebraic normalization. Kernel evaluation
    # avoids saturating the simplifier on large word-operation test bodies.
    proof = (
        "  decide +kernel\n"
        if not program.inputs
        else f"  simp [{unfold}]\n"
        "  all_goals\n"
        "    repeat first\n"
        "      | (solve | rfl | (apply BitVec.eq_of_toNat_eq; simp [BitVec.toNat_setWidth, BitVec.toNat_add, Nat.mod_mod_of_dvd] <;> omega))\n"
        f"      | (split_mir_guard <;> simp_all [{unfold}])\n"
    )
    program.import_obligation = f"""
set_option maxHeartbeats 2000000 in
set_option maxRecDepth 16384 in
theorem __IMPORT___import_checked {binders} :
  MIR.State.scalarOutcome {result_width} (__IMPORT___source {args}) = some (__IMPORT__ {args}) := by
{proof}
"""
    program.import_evidence = {
        "kind": "kernel-checked-scalar-interpreter",
        "fragment": "acyclic scalar i1..i256 without memory, calls or symbolic checked power",
        "source_sha256": hashlib.sha256(program.canonical_ir.encode()).hexdigest(),
        "boundary": "trusted compiler parser/schema exporter and operation mapping; independent typed SSA interpreter",
    }


def stack_bounds(rows, entry, catalog):
    """Choose finite proof splits from the independent graph and opcode schema.

    These bounds guide tactics only: the resulting theorem still quantifies
    every stack, and an inaccurate bound cannot establish execution equality.
    """
    cached, active = {}, set()

    pending = [(entry, False)]
    while pending:
        index, expanded = pending.pop()
        if index in cached:
            continue
        node = rows[index]["node"]
        if not expanded:
            if index in active:
                raise Unsupported("cyclic stack bounds require an invariant")
            active.add(index)
            match node["kind"]:
                case "operation" | "jump":
                    successors = [node["next"]]
                case "branch":
                    successors = [node["yes"], node["no"]]
                case "terminal":
                    successors = []
                case _:
                    raise Unsupported("unknown source stack effect")
            pending.append((index, True))
            pending.extend((target, False) for target in successors)
            continue
        match node["kind"]:
            case "operation":
                name = node["operation"]
                if name.startswith("push "):
                    name = "push1"
                elif name.startswith(("dup ", "swap ")):
                    name = name.replace(" ", "")
                definition = catalog.get(name)
                if not definition or definition["stack"] is None:
                    raise Unsupported("region needs a concrete schema stack effect")
                inputs, outputs = definition["stack"]
                need, peak = cached[node["next"]]
                delta = outputs - inputs
                result = max(inputs, need - delta), max(0, delta, delta + peak)
            case "jump":
                need, peak = cached[node["next"]]
                result = need, max(peak, node["growth"])
            case "branch":
                yes, no = cached[node["yes"]], cached[node["no"]]
                result = max(1, yes[0] + 1, no[0] + 1), max(1, yes[1] - 1, no[1] - 1)
            case "terminal":
                result = (2 if node["operation"] in {"return", "revert"} else 0), 0
            case _:
                raise Unsupported("unknown source stack effect")
        active.remove(index)
        cached[index] = result
    return cached[entry]
