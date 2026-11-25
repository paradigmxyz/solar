"""Compose a captured ABI module from its checked public entry certificates."""

import hashlib
import json
import tempfile
from pathlib import Path

from . import audit, entry_proof, prune, type_prune
from .common import Unsupported

ENTRY_PROOFS = {
    "kernel-checked-static-abi-entry",
    "kernel-checked-bytes-abi-entry",
    "kernel-checked-empty-constructor-abi-entry",
    "kernel-checked-void-abi-effect-tree",
    "kernel-checked-abi-contextual-effects",
    "kernel-checked-abi-bytes-effects",
    "kernel-checked-abi-loop-entry",
    "kernel-checked-abi-constructor-effects",
}


def roots(module):
    functions = {f["id"]: f["return_buffer"] for f in module["functions"]}
    runtime_ids = {entry["id"] for entry in module["selectors"]}
    result = [
        (f"selector:{entry['selector']}", functions[entry["id"]], entry["mutability"])
        for entry in module["selectors"]
    ]
    for role in ("receive", "fallback"):
        if entry := module[f"{role}_route"]:
            runtime_ids.add(entry["id"])
            result.append((role, functions[entry["id"]], entry["mutability"]))
    if runtime_ids != set(module["runtime_entries"]):
        raise Unsupported("ABI module composition has unclassified runtime entries")
    if (constructor := module["constructor"]) is not None:
        result.append(("constructor", functions[constructor], ""))
    names = {row[1] for row in result}
    if (
        len(names) != len(result)
        or not names
        or len(set(functions.values())) != len(functions)
    ):
        raise Unsupported("ABI module composition needs distinct public entry names")
    return sorted(result), set(functions.values())


def compose(paths, reports, executable, version, timeout):
    from .check import PRELUDE, build_model

    if not reports:
        raise Unsupported("ABI module composition needs checked public entries")
    capture_hashes = [hashlib.sha256(path.read_bytes()).hexdigest() for path in paths]
    model = audit.semantics()
    verified = {}
    for path in reports:
        report = json.loads(path.read_text())
        region = report.get("region", {})
        if (
            report.get("status") == "verified"
            and report.get("evidence") == "kernel-proof"
            and report.get("target") == version
            and report.get("semantics") == model
            and region.get("kind") == "function"
            and region.get("proof") in ENTRY_PROOFS
            and [source.get("capture_sha256") for source in report["inputs"]]
            == capture_hashes
            and [source["path"] for source in report["inputs"]]
            == [str(capture.resolve()) for capture in paths]
        ):
            verified[region["name"]] = path
    texts = [path.read_text() for path in paths]
    old_declarations, new_declarations = [prune.declarations(text)[0] for text in texts]
    if old_declarations != new_declarations:
        raise Unsupported("ABI module composition needs unchanged module declarations")
    before, after = [type_prune.export(text, executable, timeout) for text in texts]
    if any(
        module["library"] or module["dispatch_entry"] is not None
        for module in (before, after)
    ):
        raise Unsupported("ABI module composition requires undispatched contract roots")
    (old_roots, old_names), (new_roots, new_names) = map(roots, (before, after))
    additions = sorted(new_names - old_names)
    if not additions or old_names - new_names:
        raise Unsupported(
            "ABI module composition requires retained declarations and new helpers"
        )
    checked = [row for row in old_roots if row[1] in verified]
    if len(checked) != len(old_roots):
        raise Unsupported("ABI module composition needs a proof at every public entry")

    def list_source(rows):
        return (
            "["
            + ",".join(
                "("
                + ",".join(json.dumps(value, ensure_ascii=False) for value in row)
                + ")"
                for row in rows
            )
            + "]"
        )

    code = (
        PRELUDE
        + f"""
abbrev Key := String × String × String
def sourceRoots : List Key := {list_source(old_roots)}
def targetRoots : List Key := {list_source(new_roots)}
def checkedRoots : List Key := {list_source(checked)}
theorem sameRoots : sourceRoots = targetRoots := by decide +kernel
theorem covered : sourceRoots = checkedRoots := by decide +kernel
theorem composed {{Observation : Type}} (source target : Key → Observation)
    (entryProof : ∀ key ∈ checkedRoots, source key = target key) :
    sourceRoots.map source = targetRoots.map target := by
  rw [← sameRoots, covered]
  apply List.map_congr_left
  exact entryProof
#print axioms sameRoots
#print axioms covered
#print axioms composed
"""
    )
    entry_proof.success(build_model(timeout), "ABI module proof model")
    with tempfile.TemporaryDirectory(prefix="solar-tv-abi-group-") as directory:
        output = entry_proof.success(
            entry_proof.lean(Path(directory), "Roots", code, timeout),
            "ABI module root certificate",
        )
    entry_proof.check_axioms(output.splitlines(), 3)
    if audit.semantics() != model:
        raise Unsupported("checker changed during ABI module composition")
    return {
        "version": 1,
        "status": "verified",
        "evidence": "kernel-proof",
        "layer": "mir",
        "target": version,
        "semantics": model,
        "whole_pass": False,
        "region": {
            "kind": "module",
            "proof": "kernel-checked-abi-root-composition",
            "added_helpers": additions,
        },
        "assumptions": [
            "compiler-validated root descriptors from the independent typed exporter",
            "each root observation follows its linked entry certificate and that certificate's premises",
            "new private helpers enter only through the proved public root executions",
        ],
        "inputs": [
            {"path": str(path.resolve()), "capture_sha256": sha}
            for path, sha in zip(paths, capture_hashes, strict=True)
        ],
        "entry_reports": {
            name: {
                "path": str(verified[name]),
                "sha256": hashlib.sha256(verified[name].read_bytes()).hexdigest(),
            }
            for _, name, _ in checked
        },
    }
