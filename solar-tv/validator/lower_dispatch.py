"""Prove dispatch materialization from independently parsed module data."""

import hashlib
import re
import tempfile
from pathlib import Path

from . import audit, mir_state, native, routed_dispatch, type_prune
from .common import Program, Unsupported


def rejecting_path(body):
    """Construct a Lean block-path proof from the independently resolved CFG."""
    blocks = body["blocks"]
    block = 0
    jumps = []
    visited = set()
    while 0 <= block < len(blocks) and block not in visited:
        visited.add(block)
        data = blocks[block]
        term = data["terminator"]
        if data["phis"] or data["instructions"]:
            break
        if term["kind"] == "revert" and all(
            arg["kind"] == "constant" and arg["value"] == "0" for arg in term["args"]
        ):
            proof = f".revert {block} rfl"
            steps = 2
            for source, target in reversed(jumps):
                proof = f".jump {source} {target} {steps} rfl ({proof})"
                steps += 2
            return steps, proof
        if term["kind"] != "jump":
            break
        jumps.append((block, term["target"]))
        block = term["target"]
    raise Unsupported(
        "dispatcher must reach an empty revert through only unconditional jumps"
    )


def check(texts, before, after, version, timeout, executable, *, debug_reverts=False):
    from .check import PRELUDE, ROOT, build_model, run

    if len(after) != len(before) + 1:
        raise Unsupported("dispatch materialization must add exactly one runtime entry")

    source, target = models = [
        type_prune.export(text, executable, timeout) for text in texts
    ]
    if source["dispatch_entry"] is not None:
        raise Unsupported("dispatch certificate requires an implicit source entry")
    entry = target["dispatch_entry"]
    if entry is None or not 0 <= entry < len(after):
        raise Unsupported("dispatch certificate requires a resolved runtime entry")
    retained = list(after.items())
    dispatcher, _ = retained.pop(entry)
    constructor = source["constructor"]
    if constructor is not None and constructor >= entry:
        constructor += 1
    if target["constructor"] != constructor:
        raise Unsupported("dispatch materialization changed the constructor root")
    for model, table in zip(models, (before, after), strict=True):
        if [body["id"] for body in model["functions"]] != list(range(len(table))):
            raise Unsupported("module export changed the function ID table")

    definitions = {
        row["name"]: row for row in native.catalog(executable, timeout)["evmir"]
    }
    code = PRELUDE + "set_option maxRecDepth 16384\nset_option maxHeartbeats 2000000\n"
    for side, model in zip(("before", "after"), models, strict=True):
        functions = ",".join(
            mir_state.function_body(body, definitions) for body in model["functions"]
        )
        code += f"def {side} : MIR.State.Functions := [{functions}]\n"
    code += f"""
def entry : Nat := {entry}
def dispatcher : MIR.State.Function := (after[entry]'(by decide)).2
def reference : MIR.State.Functions := MIR.LowerDispatch.insertDispatcher before entry dispatcher
"""
    if source["runtime_entries"] or debug_reverts:
        body, evidence = routed_dispatch.proof(
            source,
            target,
            entry,
            definitions,
            version,
            before,
            retained,
            debug_reverts=debug_reverts,
        )
        scope = "routed-dispatch"
        code += body
    else:
        if target["runtime_entries"] or any(
            target[role] is not None for role in ("receive", "fallback")
        ):
            raise Unsupported(
                "dispatch certificate requires a single rejecting runtime root"
            )
        if list(before.items()) != retained:
            raise Unsupported("dispatch materialization changed retained functions")
        steps, path = rejecting_path(target["functions"][entry])
        scope = "empty-dispatch"
        evidence = {"work_steps": steps}
        code += f"""
theorem retained : after = reference := by rfl
def sourceRoutes : List Nat := []
theorem path : MIR.LowerDispatch.RejectPath dispatcher 0 {steps} := {path}
theorem dispatch (cfg : MIR.State.Config) (fuel : Nat) (frame : MIR.State.Frame)
    (depth : cfg.callDepth ≠ 0) (args : frame.arguments = [])
    (bounded : frame.memory.length ≤ 65536) :
    MIR.LowerDispatch.implicitRuntime sourceRoutes frame =
      MIR.State.execute (fuel + {steps}) cfg after entry frame :=
  MIR.LowerDispatch.certificate sourceRoutes rfl after entry dispatcher rfl rfl
    {steps} path cfg fuel frame depth args bounded
#print axioms dispatch
"""
    code += "\n#print axioms retained\n"
    built = build_model(timeout)
    if built.returncode:
        raise Unsupported(built.stderr or built.stdout)
    with tempfile.TemporaryDirectory(prefix="solar-tv-lower-dispatch-") as folder:
        certificate = Path(folder) / "Pair.lean"
        certificate.write_text(code)
        result = run(["lake", "env", "lean", certificate], ROOT, timeout)
    allowed = {"propext", "Quot.sound", "Classical.choice"}
    theorems = ["retained", "dispatch"]
    if evidence.get("legacy_selector_shift"):
        theorems.append("selector_arithmetic")
    for theorem in theorems:
        axioms = re.search(rf"'{theorem}' depends on axioms: \[(.*?)\]", result.stdout)
        if result.returncode or not (
            f"'{theorem}' does not depend on any axioms" in result.stdout
            or axioms
            and set(axioms[1].split(", ")) <= allowed
        ):
            raise Unsupported(
                "dispatch proof did not close\n" + result.stdout + result.stderr
            )

    programs = tuple(
        Program(
            ("mir-lower-dispatch",),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            scope=scope,
            evm_version=version,
            import_evidence={
                "kind": f"kernel-checked-{scope}",
                "source_sha256": hashlib.sha256(text.encode()).hexdigest(),
                "retained": list(before),
                "dispatcher": dispatcher,
                **evidence,
            },
        )
        for text in texts
    )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=programs
    )
