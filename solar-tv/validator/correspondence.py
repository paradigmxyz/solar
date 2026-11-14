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


def certify(program, executable, timeout, *, entry_label, stack_height):
    native.catalog(executable, timeout)
    response = subprocess.run(
        [executable, "--evm-model"],
        input=json.dumps(
            {
                "version": 1,
                "text": program.replay_ir,
                "entry": int(entry_label.removeprefix("bb")),
                "evm_version": program.evm_version,
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
                    [node["operation"]], None, memory=True
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
    stack = [f"stack{i}" for i in range(stack_height)]
    inputs = ["calldatasize", "returndatasize", *stack, *memory.INPUTS]
    widths = (*(256 for _ in range(2 + stack_height)), *memory.WIDTHS)
    if (
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
    binders = " ".join(
        f"({name} : BitVec {width})" for name, width in zip(inputs, widths, strict=True)
    )
    program.import_obligation = f"""
theorem __IMPORT___entry_checked {binders} :
  Correspondence.evalGraph (__IMPORT___graph ⟨calldatasize,returndatasize⟩) {fuel} {entry}
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
    # Start with straight-line full-word imports. Wider structural/type coverage
    # remains explicitly outside this per-import certificate, not assumed proved.
    if (
        program.memory
        or program.result_type != "Outcome (BitVec 256)"
        or any(width != 256 for width in program.input_widths)
        or re.search(
            r"\b(?:phi|jump|jumpi|switch|checked_\w+|wrapping_div|icall)\b",
            program.canonical_ir,
        )
    ):
        return
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
    config = (
        source.world["configuration"]
        .replace("mirExternalConfig", "__IMPORT___external_config")
        .replace("mirWorldConfig", "__IMPORT___world_config")
    )
    body = source.body.replace("mirWorldConfig", "__IMPORT___world_config")
    args = " ".join(program.inputs)
    binders = " ".join(f"({arg} : BitVec 256)" for arg in program.inputs)
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
    )
    # Closed executions need no algebraic normalization. Kernel evaluation
    # avoids saturating the simplifier on large word-operation test bodies.
    proof = (
        "  decide +kernel\n"
        if not program.inputs
        else f"  simp [{unfold}, MIR.State.scalarOutcome, MIR.eval, MIR.wordBinary, MIR.unary, MIR.shift, MIR.cast, MIR.compare, Outcome.bind, BitVec.add_comm]\n"
        "  all_goals first | rfl | omega\n"
    )
    program.import_obligation = f"""
theorem __IMPORT___import_checked {binders} :
  MIR.State.scalarOutcome 256 (__IMPORT___source {args}) = some (__IMPORT__ {args}) := by
{proof}
"""
    program.import_evidence = {
        "kind": "kernel-checked-scalar-interpreter",
        "fragment": "straight-line i256 without memory or effects",
        "source_sha256": hashlib.sha256(program.canonical_ir.encode()).hexdigest(),
        "boundary": "trusted compiler parser/schema exporter and operation mapping; independent typed SSA interpreter",
    }
