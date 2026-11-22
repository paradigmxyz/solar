"""Rooted function-table correspondence and resolved module identity proofs.

The compiler reparses both complete modules. Keep every retained declaration,
including attributes and instruction text, and extract all named references
outside string literals. Displaying FunctionId references by their exact names
is a trusted boundary (crates/mir/src/display.rs:784); numeric function-pointer
dispatchers and backend memory records are rejected. This is a structural call
machine certificate, not a proof of instruction semantics or backend layout.
"""

import hashlib
import json
import re
import tempfile
from pathlib import Path

from . import audit
from .common import Program, Unsupported

NAME = r"[A-Za-z_$][A-Za-z0-9_.$]*"
ROOT_ATTRIBUTE = re.compile(
    r"(?:\[|,\s*)(?:selector\s*=|(?:constructor|entry|receive|fallback)(?=\s*[,\]]))"
)


def unquoted(line, *, strip_strings=False):
    result = []
    quote = escape = False
    for char in line:
        if not quote and char == ";":
            break
        if char == '"' and not escape:
            quote = not quote
        if not strip_strings or not quote and char != '"':
            result.append(char)
        escape = char == "\\" and not escape if quote else False
    if quote:
        raise Unsupported("unterminated canonical MIR string")
    return "".join(result).strip()


def declarations(text, *, allow_empty=False):
    if "backend-memory" in text or "backend-stack" in text:
        raise Unsupported("function pruning does not certify backend memory records")
    header, functions = [], {}
    current = None
    for line in text.splitlines():
        line = unquoted(line)
        if not line:
            continue
        if line.startswith("fn @"):
            match = re.match(rf"fn @({NAME})\(", line)
            if current is not None or not match or match[1] in functions:
                raise Unsupported("ambiguous canonical MIR declaration")
            current = match[1]
            functions[current] = {
                "text": [],
                "targets": [],
                "root": bool(ROOT_ATTRIBUTE.search(line)),
            }
        if current is None:
            header.append(line)
        else:
            functions[current]["text"].append(line)
            if line == "}":
                current = None
    if current is not None or len(functions) > 128 or not (allow_empty or functions):
        raise Unsupported("function pruning requires 1..128 complete declarations")
    for function in functions.values():
        if "function_pointer_dispatcher" in function["text"][0]:
            raise Unsupported(
                "function pruning does not certify numeric function-pointer identities"
            )
        references = []
        for line in function["text"][1:]:
            references.extend(
                re.findall(
                    rf"@({NAME})(?![A-Za-z0-9_.$])", unquoted(line, strip_strings=True)
                )
            )
        function["targets"] = sorted(set(references))
    return header, functions


def check(paths, solar, version, timeout, *, executable=None, debug_reverts=False):
    from .check import PRELUDE, ROOT, build_model, run

    texts, tables = [], []
    for path in paths:
        if path.suffix != ".mir" or path.stat().st_size > 1024 * 1024:
            raise Unsupported("function pruning requires MIR files of at most 1 MiB")
        result = run(
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
        if result.returncode:
            raise Unsupported(result.stderr)
        texts.append(result.stdout)
        tables.append(declarations(result.stdout, allow_empty=True))
    (old_header, before), (new_header, after) = tables
    if (
        old_header == new_header
        and "@phase lowered" not in old_header
        and len(after) > len(before)
    ):
        from . import lower_dispatch, native

        return lower_dispatch.check(
            texts,
            before,
            after,
            version,
            timeout,
            executable or native.default(solar),
            debug_reverts=debug_reverts,
        )
    if debug_reverts:
        raise Unsupported("--debug-reverts requires a dispatch materialization pair")
    if old_header != new_header:
        from . import native, phase_advance, type_prune

        replay = executable or native.default(solar)
        if "@phase lowered" in new_header and "@phase lowered" not in old_header:
            return phase_advance.check(texts, tables, version, timeout, replay)
        if "@types" not in old_header:
            raise Unsupported("function pruning changed shared declarations or phase")

        # A different module declaration table needs a separate proof. Never
        # drop an arbitrary header, phase change or changed executable body.
        return type_prune.check(texts, version, timeout, replay)
    removed = sorted(before.keys() - after.keys())
    if not removed or after.keys() - before.keys():
        raise Unsupported("function pruning requires deletion-only table changes")
    if not any(body["root"] for body in before.values()):
        raise Unsupported("function pruning requires explicit module entry points")
    if any(before[name] != body for name, body in after.items()):
        raise Unsupported("function pruning changed a retained body or interface")
    if any(body["root"] for name, body in before.items() if name in removed):
        raise Unsupported("function pruning removed an external entry point")
    if any(set(body["targets"]) - after.keys() for body in after.values()):
        raise Unsupported("function pruning removed a retained call target")
    if any(set(re.findall(rf"@({NAME})", line)) & set(removed) for line in old_header):
        raise Unsupported("function pruning removed a shared declaration reference")

    def table(name, bodies):
        entries = []
        for function, body in bodies.items():
            text = json.dumps(body["text"], ensure_ascii=False)
            targets = json.dumps(body["targets"])
            entries.append(
                f"({json.dumps(function)},⟨{text},{targets},{str(body['root']).lower()}⟩)"
            )
        return f"def {name} : MIR.FunctionDCE.Program := [{','.join(entries)}]\n"

    code = PRELUDE + "set_option maxRecDepth 16384\nset_option maxHeartbeats 2000000\n"
    code += table("before", before) + table("after", after)
    code += """
theorem bodies : MIR.FunctionDCE.unchanged before after = true := by decide
theorem closure : MIR.FunctionDCE.closed after = true := by decide
theorem roots : MIR.FunctionDCE.rootsKept before after = true := by decide
theorem contextual {State Saved Observation : Type}
 (step : (body : MIR.FunctionDCE.Body) → State → MIR.FunctionDCE.Action body.targets State Saved Observation)
 (resume : Saved → State → State) (finish : State → Observation)
 (fuel : Nat) (machine : MIR.FunctionDCE.Machine State Saved)
 (valid : MIR.FunctionDCE.Valid (after.map Prod.fst) machine) :
 MIR.FunctionDCE.run before step resume finish fuel machine =
   MIR.FunctionDCE.run after step resume finish fuel machine :=
 MIR.FunctionDCE.certificate before after bodies closure step resume finish fuel machine valid
theorem entry_kept (name : String) (body : MIR.FunctionDCE.Body)
 (present : (name,body) ∈ before) (root : body.root = true) : name ∈ after.map Prod.fst :=
 MIR.FunctionDCE.roots_preserved before after roots name body present root
#print axioms contextual
#print axioms entry_kept
"""
    build = build_model(timeout)
    if build.returncode:
        raise Unsupported(build.stdout + build.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-function-prune-") as folder:
        path = Path(folder) / "Pair.lean"
        path.write_text(code)
        result = run(["lake", "env", "lean", path], ROOT, timeout)
    allowed = {"propext", "Quot.sound", "Classical.choice"}
    for theorem in ("contextual", "entry_kept"):
        found = re.search(rf"'{theorem}' depends on axioms: \[(.*?)\]", result.stdout)
        if result.returncode or not found or set(found[1].split(", ")) - allowed:
            raise Unsupported(
                "rooted function-table proof did not close\n"
                + result.stdout
                + result.stderr
            )
    programs = tuple(
        Program(
            ("mir-function-pruning",),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            scope="function-pruning",
            evm_version=version,
            import_evidence={
                "kind": "rooted-named-function-table",
                "source_sha256": hashlib.sha256(text.encode()).hexdigest(),
                "functions": len(functions),
                "retained": list(after),
                "removed": removed,
                "external_entries": [
                    name for name, body in before.items() if body["root"]
                ],
                "boundary": "trusted compiler parser/display, named-reference extraction and call-machine binding; no instruction-semantics, numeric function-pointer, gas or layout certificate",
            },
        )
        for text, (_, functions) in zip(texts, tables, strict=True)
    )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=programs
    )
