"""Check a semantic-to-lowered phase advance, including executable rewrites."""

import hashlib
import re

from . import audit, mir, mir_symbolic, native, phase_carriers, phi_edges, type_prune
from .common import Program, Unsupported


def check(texts, tables, version, timeout, executable):
    before, after = [text.splitlines() for text in texts]
    # Standalone IR output starts with a banner containing the input filename.
    # Keep the canonical module, including metadata and declaration order.
    for lines in (before, after):
        if lines and lines[0].startswith("; === "):
            lines.pop(0)
    if (
        len(after) < 2
        or after[1] != "@phase lowered"
        or not before
        or not before[0].startswith("@module ")
        or any(line.startswith("@phase ") for line in before[1:])
    ):
        raise Unsupported("phase certificate requires a semantic-to-lowered advance")
    del after[1]
    if before != after:
        return check_rewrites(texts, tables, version, timeout, executable)

    views = [
        {"code": lines, "functions": list(functions)}
        for lines, (_, functions) in zip((before, after), tables, strict=True)
    ]
    programs = type_prune.prove_identity(
        texts,
        views,
        version,
        timeout,
        executable,
        scope="phase-advance",
        evidence={"phases": ["semantic", "lowered"]},
    )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=programs
    )


def check_rewrites(texts, tables, version, timeout, executable):
    (old_header, before), (new_header, after) = tables
    if old_header != [line for line in new_header if line != "@phase lowered"] or list(
        before
    ) != list(after):
        raise Unsupported("phase advance changed executable or shared declarations")
    models = [type_prune.export(text, executable, timeout) for text in texts]
    roots = [
        {key: value for key, value in model.items() if key != "functions"}
        for model in models
    ]
    if roots[0] != roots[1]:
        raise Unsupported("phase advance changed the module interface")

    ids = list(before)
    by_id = [{body["id"]: body for body in model["functions"]} for model in models]
    expected_ids = set(range(len(ids)))
    if any(table.keys() != expected_ids for table in by_id):
        raise Unsupported("phase advance changed the function table")
    changed = {
        i
        for i, name in enumerate(ids)
        if before[name]["text"] != after[name]["text"] or by_id[0][i] != by_id[1][i]
    }
    definitions = {
        row["name"]: row for row in native.catalog(executable, timeout)["evmir"]
    }
    for i, name in enumerate(ids):
        left, right = by_id[0][i], by_id[1][i]
        for field in ("params", "returns"):
            if [erase_pointer(ty) for ty in left[field]] != right[field]:
                raise Unsupported(f"phase advance changed the interface of @{name}")
        left_header, right_header = (
            table[name]["text"][0] for table in (before, after)
        )
        if interface(erase_pointer_text(left_header)) != interface(right_header):
            raise Unsupported(f"phase advance changed the declaration of @{name}")
    try:
        evidence = phase_carriers.check(*by_id, changed, definitions, timeout)
    except Unsupported:
        evidence = prove_locally(
            texts, by_id, ids, changed, definitions, version, timeout, executable
        )

    programs = tuple(
        Program(
            ("mir-phase-rewrite",),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            scope="phase-rewrite",
            evm_version=version,
            import_evidence={
                "kind": "kernel-checked-phase-rewrites",
                "source_sha256": hashlib.sha256(text.encode()).hexdigest(),
                "functions": list(evidence),
                "certificates": evidence,
                "boundary": "trusted compiler parser/schema export and typed constructor renderer; unpriced direct execution, phi-edge correspondence or contextual carrier graph",
            },
        )
        for text in texts
    )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=programs
    )


def prove_locally(
    texts, by_id, ids, changed, definitions, version, timeout, executable
):
    evidence = {}
    for i in sorted(changed):
        left, right = by_id[0][i], by_id[1][i]
        name = ids[i]
        edges = phi_edges.check(left, right, definitions, timeout)
        if edges:
            evidence[name] = {"kind": "phi-edges", "edges": edges}

    # A pure edge split preserves local control and phi state in any caller.
    # Direct execution proofs also check every caller that can reach changed
    # code, including the runtime dispatch entry.
    affected = {i for i in changed if ids[i] not in evidence}
    callees = [
        call_targets(by_id[0][i]) | call_targets(by_id[1][i]) for i in range(len(ids))
    ]
    while additions := {
        i
        for i, targets in enumerate(callees)
        if i not in affected and affected & targets
    }:
        affected.update(additions)
    for i in sorted(affected):
        name = ids[i]
        pair = [
            mir_symbolic.translate(text, name, executable, version, timeout)
            for text in texts
        ]
        result = mir_symbolic.prove_direct(*pair, timeout, preserve_memory_terms=True)
        if not mir_symbolic.proved(result):
            raise Unsupported(
                f"phase advance changed executable MIR in @{name}; direct proof did not close\n"
                + result.stdout
                + result.stderr
            )
        evidence[name] = {"kind": "direct-execution"}

    return evidence


def erase_pointer(ty):
    if ty["kind"] == "pointer":
        return {"kind": "word", "bits": 256, "name": "i256"}
    return ty


def erase_pointer_text(header):
    return re.sub(r"ptr<[a-z]+>", "i256", header)


def interface(header):
    signature, _, attributes = header.removesuffix(" {").partition(" [")
    return signature, mir.interface_attributes(attributes.removesuffix("]"))


def call_targets(body):
    targets = set()
    for block in body["blocks"]:
        term = block["terminator"]
        if term["kind"] == "tail_call":
            targets.add(term["function"])
        for inst in block["instructions"]:
            op = inst["operation"]
            if op["kind"] == "call":
                targets.add(op["function"])
    return targets
