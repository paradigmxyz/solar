"""Exact scalar rewrites between independently exported, unchanged contexts.

The common prefix may leave the block early or change storage, gas and memory.
Its complete instruction list is retained, not interpreted with entry defaults.
The middle must preserve checked stacks and exact gas for every input. The full
suffix, terminal and physical successor are shared. Code-image observers and
external calls remain excluded; context-executor binding remains trusted.
"""

import hashlib
import json
import re
import tempfile
from pathlib import Path

from . import audit, prefix, regions
from .common import Program, Unsupported


def common_length(left, right):
    count = 0
    for a, b in zip(left, right):
        if a != b:
            break
        count += 1
    return count


def context(lines, labels, catalog, version):
    """Keep static guards indivisible and reject code/call correspondence gaps."""
    index = 0
    while index < len(lines):
        line = lines[index]
        if match := re.fullmatch(r"push (bb\d+)", line):
            if (
                match[1] not in labels
                or index + 1 == len(lines)
                or lines[index + 1] != "jumpi"
            ):
                raise Unsupported("interior context requires an intact static guard")
            index += 2
            continue
        if prefix.operation(line, catalog, version) is None:
            row = catalog.get(line)
            if (
                row is None
                or not row["forks"][version]["available"]
                or row["observes_code"]
                or row["sets_returndata"]
                or row["stack"] is None
                or line in {"jump", "jumpi"}
            ):
                raise Unsupported(
                    "interior context requires code-image/call correspondence"
                )
        index += 1


def check(texts, models, entry, catalog, version, timeout):
    from .check import PRELUDE, ROOT, build_model, run

    if version == "amsterdam":
        raise Unsupported("interior contexts do not model two-resource gas")
    if (
        models[0]["terminal"] != models[1]["terminal"]
        or models[0]["following"] != models[1]["following"]
    ):
        raise Unsupported("interior continuation changed")
    left, right = [model["instructions"] for model in models]
    if max(len(left), len(right)) > 256:
        raise Unsupported("interior context exceeds 256 instructions")
    start = common_length(left, right)
    if start == min(len(left), len(right)) and len(left) == len(right):
        raise Unsupported("interior context requires a changed scalar island")
    end = common_length(list(reversed(left[start:])), list(reversed(right[start:])))
    # Keep adjacent scalar producers in the proof, so a repeated PUSH can use
    # the word established immediately before it. Stop at the last real effect.
    while start and prefix.operation(left[start - 1], catalog, version) is not None:
        start -= 1
    common = left[:start]
    tails = [code[len(code) - end :] if end else [] for code in (left, right)]
    middles = [
        code[start : len(code) - end if end else len(code)] for code in (left, right)
    ]
    labels = set(regions.blocks(texts[0]))
    context(common, labels, catalog, version)
    context(tails[0], labels, catalog, version)
    imported = []
    for code in middles:
        imported.append(prefix.split({"instructions": code}, catalog, version))
        if imported[-1][1]["instructions"]:
            raise Unsupported("interior change contains an effect or transfer")
    if imported[0][4] != imported[1][4]:
        raise Unsupported(
            "interior rewrite has a gas-sensitive continuation and requires exact gas"
        )
    need = max(row[2] for row in imported)
    peak = max(row[3] for row in imported)
    if need + peak > 32:
        raise Unsupported("interior island exceeds the 32-word proof budget")

    code = PRELUDE
    for name, model, middle, tail in zip(
        ("before", "after"), models, imported, tails, strict=True
    ):
        following = (
            "none" if model["following"] is None else f"some {model['following']}"
        )
        code += f"def {name} : List EVM.Op := [{','.join(middle[0])}]\n"
        code += f"def {name}_common : List String := {json.dumps(model['instructions'][:start])}\n"
        code += f"def {name}_tail : EVM.Suffix.Tail := ⟨{json.dumps(tail)},{json.dumps(model['terminal'])},{following}⟩\n"
    code += (
        "theorem shared_prefix : before_common = after_common := by decide\n"
        "theorem shared_tail : before_tail = after_tail := by decide\n"
        "theorem stacks (ctx : EVM.Context) (stack : EVM.Stack) :\n"
        " EVM.Prefix.run ctx before stack = EVM.Prefix.run ctx after stack := by\n"
        + "\n".join("  " + line for line in prefix.stack_proof(need, peak).splitlines())
        + f"\ntheorem prices : EVM.Prefix.cost .{version} before = EVM.Prefix.cost .{version} after := by decide\n"
        + f"""
theorem contextual {{World Halt : Type}}
 (enter : List String → EVM.Suffix.PrefixResult World Halt)
 (leave : EVM.Suffix.Tail → EVM.Context → EVM.Stack → Nat → World → Halt)
 (fault : Halt) :
 EVM.Suffix.scalar enter leave fault .{version} before_common before_tail before =
 EVM.Suffix.scalar enter leave fault .{version} after_common after_tail after := by
 rw [← shared_prefix, ← shared_tail]
 exact EVM.Suffix.scalar_contextual enter leave fault .{version} before_common before_tail before after stacks prices
#print axioms contextual
"""
    )
    build = build_model(timeout)
    if build.returncode:
        raise Unsupported(build.stdout + build.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-interior-") as folder:
        path = Path(folder) / "Pair.lean"
        path.write_text(code)
        result = run(["lake", "env", "lean", path], ROOT, timeout)
    axioms = re.search(r"'contextual' depends on axioms: \[(.*?)\]", result.stdout)
    if (
        result.returncode
        or not axioms
        or set(axioms[1].split(", ")) - {"propext", "Quot.sound", "Classical.choice"}
    ):
        raise Unsupported(
            "interior stack proof did not close; no reachable counterexample established\n"
            + result.stdout
            + result.stderr
        )
    programs = tuple(
        Program(
            ("evm-interior", "any"),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            selector=entry,
            scope="interior-window",
            gas="exact",
            evm_version=version,
            import_evidence={
                "kind": "independent-shared-context-scalar-island",
                "source_sha256": hashlib.sha256(
                    json.dumps(model, sort_keys=True).encode()
                ).hexdigest(),
                "shared_prefix_operations": start,
                "scalar_operations": len(middle[0]),
                "shared_suffix_operations": end,
                "continuation": "identical complete suffix, terminal and physical successor",
                "boundary": "trusted Rust parser/schema export, opcode mapping and shared context-executor bindings; no full-state interpreter or physical code-placement certificate",
            },
        )
        for text, model, middle in zip(texts, models, imported, strict=True)
    )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=programs
    )
