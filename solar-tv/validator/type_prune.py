"""Erasure of a named-type presentation table with identical resolved MIR data.

Both modules are independently parsed and validated by the source exporter.
Every supplied body is exported, including unreachable functions and cycles.
The kernel compares their resolved typed graphs and all retained source text.
No execution restriction is relaxed and no opcode receives a default result.
"""

import hashlib
import json
import re
import subprocess
import tempfile
from pathlib import Path

from . import audit, mir_state, native, prune
from .common import Program, Unsupported


def split(text):
    _, functions = prune.declarations(text)
    types, code = [], []
    section = seen = False
    for line in text.splitlines():
        line = prune.unquoted(line)
        if not line:
            continue
        if line == "@types":
            if seen or any(row.startswith("fn @") for row in code):
                raise Unsupported("type pruning requires one canonical type section")
            section = seen = True
            continue
        if section and not line.startswith(("@", "fn @")):
            match = re.fullmatch(r"(struct[0-9]+): (\{.*\})", line)
            if not match or len(types) >= 128:
                raise Unsupported("type pruning requires at most 128 canonical structs")
            types.append((match[1], match[2]))
        else:
            section = False
            code.append(line)
    if len({name for name, _ in types}) != len(types):
        raise Unsupported("type pruning has duplicate named definitions")
    return {"types": types, "code": code, "functions": list(functions)}


def export(text, executable, timeout):
    result = subprocess.run(
        [executable, "--mir-module-model"],
        input=json.dumps({"version": 1, "text": text}),
        capture_output=True,
        text=True,
        check=False,
        timeout=timeout,
    )
    try:
        value = json.loads(result.stdout)
        if (
            result.returncode
            or value.get("version") != 1
            or value.get("status") != "ok"
        ):
            raise Unsupported(
                "module source export: " + value.get("reason", "invalid response")
            )
        model = value["model"]
        if model.get("structural_module") is not True:
            raise Unsupported("module identity needs an explicit structural export")
        return model
    except Unsupported:
        raise
    except (ValueError, KeyError, TypeError) as error:
        raise Unsupported("invalid independent module export") from error


def check(texts, version, timeout, executable):
    from .check import PRELUDE, ROOT, build_model, run

    views = [split(text) for text in texts]
    before, after = views
    if not before["types"] or after["types"]:
        raise Unsupported("type pruning requires erasing a complete type table")
    if before["code"] != after["code"]:
        raise Unsupported("type pruning changed executable or shared declarations")
    models = [export(text, executable, timeout) for text in texts]
    definitions = {
        row["name"]: row for row in native.catalog(executable, timeout)["evmir"]
    }
    for model, view in zip(models, views, strict=True):
        functions = model["functions"]
        if len(functions) != len(view["functions"]) or [
            f["id"] for f in functions
        ] != list(range(len(functions))):
            raise Unsupported(
                "module export must retain every function and numeric identity"
            )
    code = PRELUDE + "set_option maxRecDepth 16384\nset_option maxHeartbeats 2000000\n"
    for side, model, view in zip(("before", "after"), models, views, strict=True):
        functions = ",".join(
            mir_state.function_body(body, definitions) for body in model["functions"]
        )
        code += f"def {side} : MIR.TypeDCE.Program := ⟨{json.dumps(view['code'], ensure_ascii=False)},[{functions}]⟩\n"
    code += """
theorem same : before = after := by rfl
theorem contextual {State Observation : Type}
 (interpret : MIR.TypeDCE.Interpreter State Observation) (fuel : Nat) (state : State) :
 MIR.TypeDCE.execute before interpret fuel state = MIR.TypeDCE.execute after interpret fuel state :=
 MIR.TypeDCE.certificate before after same interpret fuel state
#print axioms contextual
"""
    built = build_model(timeout)
    if built.returncode:
        raise Unsupported(built.stdout + built.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-type-prune-") as folder:
        path = Path(folder) / "Pair.lean"
        path.write_text(code)
        result = run(["lake", "env", "lean", path], ROOT, timeout)
    axioms = re.search(r"'contextual' depends on axioms: \[(.*?)\]", result.stdout)
    if result.returncode or not (
        "'contextual' does not depend on any axioms" in result.stdout
        or axioms
        and not set(axioms[1].split(", "))
        - {"propext", "Quot.sound", "Classical.choice"}
    ):
        raise Unsupported(
            "resolved module identity proof did not close\n"
            + result.stdout
            + result.stderr
        )
    programs = tuple(
        Program(
            ("mir-unused-type-erasure",),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            scope="unused-type-erasure",
            evm_version=version,
            import_evidence={
                "kind": "kernel-checked-resolved-module-identity",
                "source_sha256": hashlib.sha256(text.encode()).hexdigest(),
                "resolved_sha256": hashlib.sha256(
                    json.dumps(model["functions"], sort_keys=True).encode()
                ).hexdigest(),
                "removed_types": [name for name, _ in before["types"]],
                "functions": view["functions"],
                "boundary": "trusted independent compiler parser/schema export and typed constructor renderer; kernel-checked resolved graphs and complete retained text; no instruction-implementation, physical gas or backend layout certificate",
            },
        )
        for text, model, view in zip(texts, models, views, strict=True)
    )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=programs
    )
