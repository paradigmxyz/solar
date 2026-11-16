"""Suffix refinement composed with an exactly shared precompile prefix.

The prefix is retained as the complete independently exported instruction list,
passed identically to an arbitrary shared state/gas executor. Its interpreter
binding remains trusted. Never delete it or replace its returned context with
entry defaults. External calls are restricted to canonical recovery-precompile
targets; code-image observers and relocations remain unsupported.
"""

import hashlib
import json
import re
import tempfile
from pathlib import Path

from . import audit, evm_effects, prefix, regions
from .common import Program, Unsupported

CORRESPONDENCE = "prefix continuation requires code-image/call correspondence"


def admit_common(lines, catalog, version):
    calls = 0
    for index, line in enumerate(lines):
        if prefix.operation(line, catalog, version) is not None:
            continue
        row = catalog.get(line)
        if (
            row is None
            or not row["forks"][version]["available"]
            or row["observes_code"]
            or line in {"jump", "jumpi"}
        ):
            raise Unsupported(CORRESPONDENCE)
        if row["sets_returndata"]:
            # Target is the second word consumed by STATICCALL. A successful
            # GAS read puts one word above the literal, independently of entry
            # stack contents. Underflow/OOG before the suffix remains a halt.
            if (
                line != "staticcall"
                or row["stack"] != [6, 1]
                or lines[max(0, index - 2) : index] != ["push 1", "gas"]
            ):
                raise Unsupported(CORRESPONDENCE)
            calls += 1
    if not calls or len(lines) > 256:
        raise Unsupported(CORRESPONDENCE)
    return calls


def check(texts, models, entry, catalog, version, policy, timeout):
    from .check import PRELUDE, ROOT, build_model, run

    if version == "amsterdam":
        raise Unsupported("shared precompile prefixes do not model two-resource gas")
    if policy == "exact":
        raise Unsupported("post-precompile suffix proves refinement, not exact gas")
    if (
        models[0]["terminal"] != models[1]["terminal"]
        or models[0]["following"] != models[1]["following"]
    ):
        raise Unsupported(CORRESPONDENCE)
    left, right = [model["instructions"] for model in models]
    cut = 0
    for a, b in zip(left, right):
        if a != b:
            break
        cut += 1
    common = left[:cut]
    calls = admit_common(common, catalog, version)
    labels = set(regions.blocks(texts[0]))
    try:
        imported = [
            evm_effects.instructions(code[cut:], catalog, version, labels)
            for code in (left, right)
        ]
    except Unsupported as error:
        raise Unsupported(CORRESPONDENCE) from error
    need = max(row[1] for row in imported)
    peak = max(row[2] for row in imported)
    if need + peak > 32:
        raise Unsupported("combined suffix exceeds the stack proof budget")
    declarations = "def common : List String := " + json.dumps(common) + "\n"
    declarations += "".join(
        f"def {name} (prices : Nat → Nat × Nat) : List EVM.Effects.Op := [{','.join(row[0])}]\n"
        for name, row in zip(("before", "after"), imported, strict=True)
    )
    proof = f"""
theorem interaction (prices : Nat → Nat × Nat) (ctx : EVM.Context) (stack : EVM.Stack) :
 EVM.Effects.Refines (EVM.Effects.expand ctx .{version} (after prices) stack 0)
   (EVM.Effects.expand ctx .{version} (before prices) stack 0) := by
"""
    proof += (
        "\n".join(
            f"  have index{i} : (({i} : Fin 256) : Nat) = {i} := by decide"
            for i in range(16)
        )
        + "\n"
    )
    proof += "\n".join(
        "  " + line
        for line in evm_effects.stack_proof(
            need, peak, ", EVM.eq_iszero_zero, EVM.sub_zero"
        ).splitlines()
    )
    proof += f"""
theorem contextual {{World Halt : Type}}
 (executePrefix : List String → EVM.Suffix.PrefixResult World Halt)
 (handler : EVM.Effects.Handler World) (prices : Nat → Nat × Nat)
 (exit : EVM.Effects.Exit) (gas : Nat) (world : World)
 (success : EVM.Suffix.execute executePrefix handler .{version} common (before prices) =
   .done exit gas world) :
 ∃ remaining, EVM.Suffix.execute executePrefix handler .{version} common (after prices) =
   .done exit remaining world ∧ gas ≤ remaining :=
 EVM.Suffix.resource executePrefix handler .{version} common (before prices) (after prices)
   (interaction prices) exit gas world success
theorem prefix_halt {{World Halt : Type}}
 (executePrefix : List String → EVM.Suffix.PrefixResult World Halt)
 (handler : EVM.Effects.Handler World) (prices : Nat → Nat × Nat) (outcome : Halt)
 (halt : executePrefix common = .stopped outcome) :
 EVM.Suffix.execute executePrefix handler .{version} common (before prices) = .stopped outcome ∧
 EVM.Suffix.execute executePrefix handler .{version} common (after prices) = .stopped outcome :=
 EVM.Suffix.stopped executePrefix handler .{version} common _ _ outcome halt
#print axioms contextual
#print axioms prefix_halt
"""
    build = build_model(timeout)
    if build.returncode:
        raise Unsupported(build.stdout + build.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-suffix-") as folder:
        path = Path(folder) / "Pair.lean"
        path.write_text(
            PRELUDE + "set_option maxHeartbeats 2000000\n" + declarations + proof
        )
        result = run(["lake", "env", "lean", path], ROOT, timeout)
    allowed = {"propext", "Quot.sound", "Classical.choice"}
    axioms = [
        re.search(rf"'{name}' depends on axioms: \[(.*?)\]", result.stdout)
        for name in ("contextual", "prefix_halt")
    ]
    if result.returncode or any(
        not found or set(found[1].split(", ")) - allowed for found in axioms
    ):
        raise Unsupported(
            "universal suffix proof did not close; no reachable counterexample established\n"
            + result.stdout
            + result.stderr
        )
    programs = tuple(
        Program(
            ("evm-suffix", "any"),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            selector=entry,
            scope="shared-prefix-suffix",
            gas="refine",
            evm_version=version,
            import_evidence={
                "kind": "independent-post-precompile-suffix",
                "source_sha256": hashlib.sha256(
                    json.dumps(model, sort_keys=True).encode()
                ).hexdigest(),
                "shared_prefix_sha256": hashlib.sha256(
                    json.dumps(common).encode()
                ).hexdigest(),
                "shared_prefix_operations": cut,
                "suffix_operations": len(code[cut:]),
                "recovery_calls": calls,
                "continuation": "identical terminal and physical successor; logical label context",
                "boundary": "trusted parser/schema/renderer and shared prefix-executor binding; no full-state interpreter or physical code-placement certificate",
            },
        )
        for text, model, code in zip(texts, models, (left, right), strict=True)
    )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=programs
    )
