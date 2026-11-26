"""Check reordered or removed stack operations at their actual consumer."""

import hashlib
import json
import re
import tempfile
from pathlib import Path

from . import (
    audit,
    code_relocations,
    evm_effects,
    interior,
    native,
    prefix,
    regions,
    source_identity,
    suffix,
)
from .common import Program, Unsupported


class UnpairedContinuation(Unsupported):
    """A changed charge reaches a resource-sensitive continuation."""


def partition(models, *, retain_code_prefix=False):
    if any(models[0][key] != models[1][key] for key in ("terminal", "following")):
        raise Unsupported("peephole continuation changed")
    left, right = (model["instructions"] for model in models)
    start = interior.common_length(left, right)
    if left == right:
        raise Unsupported("no changed stack operations")
    end = interior.common_length(left[start:][::-1], right[start:][::-1])
    # Keep known literal inputs and take the actual conditional consumer when
    # its condition is the changed value. An earlier or subsequent branch
    # stays in the shared context with its untouched successor.
    while start and re.fullmatch(r"(?:push|dup) \d+", left[start - 1]):
        start -= 1
    if not retain_code_prefix and "size-init-tail" in left[:start]:
        start = 0
    suffix = left[len(left) - end :]
    # A shared POP can discard slots reordered differently on the two sides.
    end -= next((i for i, op in enumerate(suffix) if op != "pop"), end)
    suffix = left[len(left) - end :]
    if (
        len(suffix) >= 2
        and re.fullmatch(r"push bb\d+", suffix[0])
        and suffix[1] == "jumpi"
    ):
        end -= 2
    middles = [code[start : len(code) - end] for code in (left, right)]
    return left[:start], middles, left[len(left) - end :]


def low_mask_edits(left, right):
    """Propose matched instruction runs and exact low-mask replacements."""
    edits, common = [], []
    a = b = 0
    while a < len(left) and b < len(right):
        window = left[a : a + 5]
        if (
            len(window) == 5
            and window[:2] == ["push 0", "not"]
            and (count := re.fullmatch(r"push (\d+)", window[2]))
            and window[3:] == ["shr", "and"]
            and right[b : b + 4] == [window[2], "shl", window[2], "shr"]
        ):
            edits.extend((common, int(count[1])))
            common = []
            a += 5
            b += 4
        elif left[a] == right[b]:
            common.append(left[a])
            a += 1
            b += 1
        else:
            return None
    if edits and a == len(left) and b == len(right):
        return [*edits, common]
    return None


def observes_resources(line, catalog):
    row = catalog.get(line, {})
    return any(
        row.get(key) for key in ("observes_gas", "sets_returndata", "observes_code")
    )


def low_mask_tail(tail, catalog):
    """Retain supported suffix instructions, including storage admission."""
    for index, line in enumerate(tail):
        if (
            line != "sstore"
            and observes_resources(line, catalog)
            or line.startswith("push bb")
            and tail[index + 1 : index + 2] != ["jumpi"]
        ):
            return tail[:index], tail[index:]
    return tail, []


def certificate(
    common,
    tail,
    models,
    imported,
    version,
    prelude,
    *,
    edits=None,
    preserve_stack_faults=False,
):
    code = prelude + "set_option maxHeartbeats 3000000\n"
    for name, model, ops in zip(("before", "after"), models, imported, strict=True):
        following = (
            "none" if model["following"] is None else f"some {model['following']}"
        )
        instructions = model["instructions"]
        suffix = instructions[len(instructions) - len(tail) :]
        code += f"""
def {name} (prices : Nat → Nat × Nat) : List EVM.Effects.Op := [{",".join(ops[0])}]
def {name}Common : List String := {json.dumps(instructions[: len(common)])}
def {name}Tail : EVM.Suffix.Tail := ⟨{json.dumps(suffix)}, {json.dumps(model["terminal"])}, {following}⟩
"""
    stateful = any(op.startswith(".effect ") for ops, _, _ in imported for op in ops)
    # The pure prefix contract includes stack faults. Stateful and low-mask
    # certificates retain their original-success refinement policy.
    strict = preserve_stack_faults and edits is None and not stateful
    allow_source_fault = "false" if strict else "true"
    code += f"""
private theorem sharedPrefix : afterCommon = beforeCommon := by decide
private theorem sharedTail : afterTail = beforeTail := by decide

theorem middle (prices : Nat → Nat × Nat) (ctx : EVM.Context) (stack : EVM.Stack) :
    EVM.Effects.Refines (EVM.Effects.expand ctx .{version} (after prices) stack 0)
      (EVM.Effects.expand ctx .{version} (before prices) stack 0) {allow_source_fault} := by
"""
    if edits is None:
        need = max(row[1] for row in imported)
        peak = max(row[2] for row in imported)
        if need + peak > 32:
            raise Unsupported("peephole island exceeds the 32-word stack proof budget")
        for i in range(16):
            code += f"  have index{i} : (({i} : Fin 256) : Nat) = {i} := by decide\n"
        arithmetic = (
            ", EVM.eq_iszero_zero, EVM.sub_zero, EVM.ContextualPeephole.double_iszero_zero, "
            "EVM.ContextualPeephole.invert_lt, EVM.ContextualPeephole.odd_word, "
            "EVM.ContextualPeephole.add_literal"
        )
        proof = evm_effects.stack_proof(
            need, peak, arithmetic, discharger="first | decide | omega"
        )
        code += "\n".join("  " + line for line in proof.splitlines())
    else:
        code += f"""  let edits : List EVM.LateWord.Edit := [{",".join(edits)}]
  have source : before prices = EVM.LateWord.instructions false edits := rfl
  have target : after prices = EVM.LateWord.instructions true edits := rfl
  rw [source, target]
  exact EVM.LateWord.refines ctx .{version} edits stack 0 0 (Nat.le_refl 0)
"""
    code += f"""

theorem contextual {{World Halt : Type}}
    (enter : List String → EVM.Suffix.PrefixResult World Halt)
    (handler : EVM.Effects.Handler World) (prices : Nat → Nat × Nat)
    (exit : EVM.Effects.Exit) (gas : Nat) (world : World)
    (ran : EVM.Suffix.execute enter handler .{version} beforeCommon (before prices) = .done exit gas world) :
    ∃ left, EVM.Suffix.execute enter handler .{version} afterCommon (after prices) = .done exit left world ∧
      gas ≤ left ∧ afterTail = beforeTail := by
  rw [sharedPrefix]
  obtain ⟨left, next, enough⟩ := EVM.Suffix.resource enter handler .{version} beforeCommon
    (before prices) (after prices) (middle prices) exit gas world ran
  exact ⟨left, next, enough, sharedTail⟩
#print axioms contextual
"""
    return code


def check(
    paths,
    solar,
    replay,
    version,
    timeout,
    entry,
    *,
    paired_observers=False,
    preserve_stack_faults=False,
):
    from .check import PRELUDE, ROOT, build_model, run

    if version == "amsterdam":
        raise Unsupported("stack peepholes do not model Amsterdam two-resource gas")
    texts = source_identity.parse_pair(paths, solar, version, timeout, entry)
    block_labels = [list(regions.blocks(text)) for text in texts]
    if block_labels[0] != block_labels[1]:
        raise Unsupported("peephole block identities or order changed")
    models = [
        prefix.export(text, entry, version, replay, timeout, code_tail=True)
        for text in texts
    ]
    common, middles, tail = partition(models)
    if len(common) + max(map(len, middles)) + len(tail) > 256:
        raise Unsupported("peephole context exceeds 256 instructions")
    catalog = {row["name"]: row for row in native.catalog(replay, timeout)["evmir"]}
    if preserve_stack_faults and any(
        catalog.get(line.split()[0], {}).get(key)
        for line in common
        for key in ("observes_code", "sets_returndata")
    ):
        # An explicit prefix proof must bind the shared preamble across images.
        suffix.admit_common(common, catalog, version)
    edits = low_mask_edits(*middles)
    if edits is not None:
        include, tail = low_mask_tail(tail, catalog)
        edits[-1].extend(include)
        middles = [middle + include for middle in middles]
    observers = sorted({line for line in tail if observes_resources(line, catalog)})
    if observers and not paired_observers:
        if edits is not None:
            raise UnpairedContinuation(
                "low-mask continuation requires paired observations for "
                + ", ".join(observers)
            )
        return interior.check(texts, models, entry, catalog, version, timeout)
    layout = code_relocations.bind(models, texts, solar, version, timeout, catalog)
    labels = set(block_labels[0])
    imported = [
        evm_effects.instructions(
            code,
            catalog,
            version,
            labels,
            code_tail=layout is not None,
            composed=edits is not None,
        )
        for code in middles
    ]
    edits_proof = None
    if edits is not None:
        edits_proof = []
        for edit in edits:
            if isinstance(edit, int):
                edits_proof.append(f".lowMask {edit}#256")
            else:
                ops, _, _ = evm_effects.instructions(
                    edit, catalog, version, labels, composed=True
                )
                edits_proof.extend(f".same ({op})" for op in ops)
    code = certificate(
        common,
        tail,
        models,
        imported,
        version,
        PRELUDE,
        edits=edits_proof,
        preserve_stack_faults=preserve_stack_faults,
    )
    build = build_model(timeout)
    if build.returncode:
        raise Unsupported(build.stdout + build.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-peephole-") as folder:
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
            "stack peephole proof did not close\n" + result.stdout + result.stderr
        )

    programs = tuple(
        Program(
            ("evm-peephole", "any"),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            selector=entry,
            scope="peephole-window",
            gas="refine",
            evm_version=version,
            import_evidence={
                "kind": "kernel-checked-contextual-low-mask"
                if edits
                else "kernel-checked-contextual-peephole",
                "source_sha256": hashlib.sha256(
                    json.dumps(model, sort_keys=True).encode()
                ).hexdigest(),
                "prefix_operations": len(common),
                "changed_window_operations": len(middle),
                "continuation_operations": len(tail),
                "continuation_observers": observers,
                **(
                    {
                        "low_mask_counts": [e for e in edits if isinstance(e, int)],
                        "storage_admissions": middle.count("sstore"),
                    }
                    if edits
                    else {}
                ),
                **({"constructor_code_tail": relocation} if relocation else {}),
            },
        )
        for text, model, middle, relocation in zip(
            texts, models, middles, layout or (None, None), strict=True
        )
    )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=programs
    )
