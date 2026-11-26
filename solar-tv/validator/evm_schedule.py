"""Validate EVM stack schedules against ordered logical effects."""

import hashlib
import json
import re
import tempfile
from pathlib import Path

from . import (
    audit,
    cfg_flow,
    code_relocations,
    entry_stack,
    evm_effects,
    native,
    peephole,
    prefix,
    regions,
    source_identity,
)
from .common import Program, Unsupported

GUARD = re.compile(r"push bb(\d+)")
SIMP = (
    ", EVM.StackSchedule.trace, EVM.StackSchedule.project, EVM.StackSchedule.Refines, "
    "EVM.StackSchedule.revertExit, EVM.Effects.request, aborts, prices, "
    "Evm.lt, Evm.gt, Evm.slt, Evm.sgt, Evm.and, Evm.or, Evm.xor, Evm.eq, Bool.beq_comm, "
    "BitVec.add_assoc, EVM.StackSchedule.add_left_comm, BitVec.and_comm, BitVec.or_comm, BitVec.xor_comm"
)


class ScheduleMismatch(Unsupported):
    """The imported schedules did not match on the proposed stack domain."""


def abort_path(texts, catalog, version, replay, timeout, entry):
    """Import a shared direct chain that reads only its own operands to revert."""
    seen = set()
    operations = []
    models = []
    while len(seen) < 8:
        if entry in seen:
            return None
        seen.add(entry)
        try:
            left, right = [
                prefix.export(text, entry, version, replay, timeout) for text in texts
            ]
        except Unsupported:
            return None
        if left != right or any(GUARD.fullmatch(line) for line in left["instructions"]):
            return None
        try:
            imported, _, _ = evm_effects.instructions(
                left["instructions"], catalog, version, composed=True
            )
        except Unsupported:
            return None
        operations.extend(imported)
        models.append([entry, left])
        if len(operations) > 64:
            return None
        if left["terminal"] == "Op(253)":
            return operations, models
        target = left.get("jump_target")
        if not isinstance(target, int):
            return None
        operations.append(".reserve 1")
        entry = f"bb{target}"
    return None


def height_after_prefix(height, lines, catalog, version):
    for line in lines:
        if GUARD.fullmatch(line) or code_relocations.operation(line):
            height += 1
            continue
        scalar = prefix.operation(line, catalog, version)
        signature = (
            scalar[1] if scalar is not None else catalog.get(line, {}).get("stack")
        )
        if signature is None or height < signature[0]:
            raise Unsupported("cannot derive the scheduled window's incoming height")
        height += signature[1] - signature[0]
        if height > 1024:
            raise Unsupported("shared prefix exceeds the 1024-word stack limit")
    return height


def certificate(models, common, tail, imported, aborts, version, height, entry_limit):
    code = (
        "set_option maxHeartbeats 3500000\ndef prices (_ : Nat) : Nat × Nat := (0,0)\n"
    )
    code += "def aborts : Nat → Option (List EVM.Effects.Op)\n"
    for label, body in aborts.items():
        code += f" | {label} => some [{','.join(body)}]\n"
    code += " | _ => none\n"
    for name, model, (ops, _, _) in zip(
        ("before", "after"), models, imported, strict=True
    ):
        following = (
            "none" if model["following"] is None else f"some {model['following']}"
        )
        items = model["instructions"]
        code += f"""
def {name} : List EVM.Effects.Op := [{",".join(ops)}]
def {name}Common : List String := {json.dumps(items[: len(common)])}
def {name}Tail : EVM.Suffix.Tail := ⟨{json.dumps(items[len(items) - len(tail) :])}, {json.dumps(model["terminal"])}, {following}⟩
"""
    need = max(row[1] for row in imported)
    peak = max(row[2] for row in imported) + max(
        (len(body) + 2 for body in aborts.values()), default=0
    )
    if need + peak > 64:
        raise Unsupported("stack schedule exceeds the proof bound")
    if entry_limit is not None and height + peak > 1024:
        raise Unsupported(
            "physical entry leaves too little stack room for the schedule"
        )
    condition = binding = bridge = ""
    supported, witness, matched = (
        "fun _ => True",
        "(fun _ _ _ _ _ => trivial)",
        "(fun ctx stack _ => matched ctx stack)",
    )
    if height is not None:
        relation = "=" if entry_limit is None else "≤"
        condition = f" (fits : stack.length {relation} {height})"
        supported = f"fun stack => stack.length {relation} {height}"
        matched = "matched"
        if entry_limit is None:
            binding = f"(binding : ∀ ctx stack gas world, enter beforeCommon = .next ctx stack gas world → stack.length = {height})"
            witness = "binding"
        else:
            delta = height - entry_limit
            binding = f"""
    (pc incoming : Nat)
    (reached : EVM.StackReachability.Reaches SourceFrame.graph SourceFrame.signatures pc incoming)
    (binding : ∀ ctx stack gas world, enter beforeCommon = .next ctx stack gas world →
      stack.length + {max(0, -delta)} = incoming + {max(0, delta)})"""
            bridge = f"""
  have room := SourceFrame.entryFits reached
  have supported : ∀ ctx stack gas world, enter beforeCommon = .next ctx stack gas world →
      stack.length ≤ {height} := by
    intro ctx stack gas world resumes
    have shape := binding ctx stack gas world resumes
    omega"""
            witness = "supported"
    code += f"""
private theorem sharedPrefix : afterCommon = beforeCommon := by decide
private theorem sharedTail : afterTail = beforeTail := by decide
private theorem matched (ctx : EVM.Context) (stack : EVM.Stack){condition} :
    EVM.StackSchedule.Refines (EVM.StackSchedule.trace ctx .{version} aborts after stack)
      (EVM.StackSchedule.trace ctx .{version} aborts before stack) := by
"""
    for i in range(16):
        code += f"  have index{i} : (({i} : Fin 256) : Nat) = {i} := by decide\n"
    simplifier = evm_effects.stack_proof(
        need,
        peak,
        SIMP,
        discharger="first | decide | omega",
        height=height if entry_limit is None else None,
        bounded=entry_limit is not None,
        max_steps=600000,
    )
    return (
        code
        + "\n".join("  " + line for line in simplifier.splitlines())
        + f"""

theorem contextual {{World Halt : Type}} (enter : List String → EVM.Suffix.PrefixResult World Halt)
    (handler : EVM.StackSchedule.Handler World) (exit : EVM.StackSchedule.Exit) (final : World)
    {binding}
    (ran : EVM.StackSchedule.observe enter handler .{version} beforeCommon aborts before =
      .done exit final) :
    EVM.StackSchedule.observe enter handler .{version} afterCommon aborts after = .done exit final ∧
      afterTail = beforeTail := by
  rw [sharedPrefix]{bridge}
  exact ⟨EVM.StackSchedule.contextual enter handler .{version} beforeCommon aborts before after
    ({supported}) {witness} {matched}
    exit final ran, sharedTail⟩
#print axioms contextual
"""
    )


def check(
    paths, solar, replay, version, timeout, entry, height=None, *, from_entry=False
):
    from .check import PRELUDE, ROOT, build_model, run

    texts = source_identity.parse_pair(paths, solar, version, timeout, entry)
    labels = list(regions.blocks(texts[0]))
    if labels != list(regions.blocks(texts[1])):
        raise Unsupported("stack schedule changed block identities or order")
    models = [
        prefix.export(text, entry, version, replay, timeout, code_tail=True)
        for text in texts
    ]
    common, middles, tail = peephole.partition(models, retain_code_prefix=True)
    if len(common) + max(map(len, middles)) + len(tail) > 256:
        raise Unsupported("stack schedule window exceeds 256 instructions")
    catalog = {op["name"]: op for op in native.catalog(replay, timeout)["evmir"]}
    if any(
        line != "sstore" and peephole.observes_resources(line, catalog) for line in tail
    ):
        raise Unsupported("scheduled continuation observes resources or code layout")
    layout = code_relocations.bind(models, texts, solar, version, timeout, catalog)
    if tail and catalog.get(tail[0], {}).get("pure"):
        # A common consumer may accept operands presented in a different order.
        middles = [middle + tail[:1] for middle in middles]
        tail = tail[1:]
    cut_height = entry_limit = None
    caller = None
    if height is not None:
        if not 0 <= height <= 1024:
            raise Unsupported("invalid stack schedule entry height")
        cut_height = height_after_prefix(height, common, catalog, version)
        if cut_height > 32:
            raise Unsupported(
                "stack schedule fixed height exceeds the 32-word proof budget"
            )
    elif from_entry:
        caller = entry_stack.propose(texts[0], entry, replay, catalog, version, timeout)
        entry_limit = max(caller.bounds)
        cut_height = height_after_prefix(entry_limit, common, catalog, version)
    imported = [
        evm_effects.instructions(code, catalog, version, labels, composed=True)
        for code in middles
    ]
    aborts = {}
    abort_evidence = {}
    guards = {
        int(m[1]) for code in middles for line in code if (m := GUARD.fullmatch(line))
    }
    for label in sorted(guards):
        path = abort_path(texts, catalog, version, replay, timeout, f"bb{label}")
        if path is not None:
            aborts[label], abort_evidence[label] = path
    definitions = certificate(
        models, common, tail, imported, aborts, version, cut_height, entry_limit
    )
    build = build_model(timeout)
    if build.returncode:
        raise Unsupported(build.stdout + build.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-stack-schedule-") as folder:
        directory = Path(folder)
        if caller is None:
            code = directory / "Pair.lean"
            code.write_text(PRELUDE + definitions)
            result = run(["lake", "env", "lean", "-j1", code], ROOT, timeout)
        else:
            bound = entry_stack.certificate(
                "SourceFrame", caller, catalog, entry_limit, chunked=True
            )
            result = cfg_flow.check_stages(
                directory,
                PRELUDE
                + "noncomputable section\nset_option maxRecDepth 65536\nset_option maxHeartbeats 3500000\n",
                [bound, definitions],
                timeout,
            )
    axioms = re.search(r"'contextual' depends on axioms: \[(.*?)\]", result.stdout)
    if (
        result.returncode
        or not axioms
        or set(axioms[1].split(", ")) - {"propext", "Quot.sound", "Classical.choice"}
    ):
        raise ScheduleMismatch(
            "stack schedule proof did not close\n" + result.stdout + result.stderr
        )
    programs = tuple(
        Program(
            ("evm-stack-schedule", "any"),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            selector=entry,
            scope="stack-schedule",
            gas="unpriced-shared-interactions",
            evm_version=version,
            import_evidence={
                "kind": "kernel-checked-stack-schedule",
                "source_sha256": hashlib.sha256(
                    json.dumps(model, sort_keys=True).encode()
                ).hexdigest(),
                "prefix_operations": len(common),
                "schedule_operations": len(code),
                "continuation_operations": len(tail),
                "entry_height": height,
                "schedule_height": cut_height,
                **(
                    {"source_stack_bound": entry_stack.evidence(caller)}
                    if caller
                    else {}
                ),
                "abort_path_sha256": hashlib.sha256(
                    json.dumps(abort_evidence, sort_keys=True).encode()
                ).hexdigest(),
                "abort_targets": sorted(aborts),
                "context_observers": sorted(
                    {
                        line
                        for line in [*common, *tail]
                        if peephole.observes_resources(line, catalog)
                    }
                ),
                **({"constructor_code_tail": relocation} if relocation else {}),
            },
        )
        for text, model, code, relocation in zip(
            texts, models, middles, layout or (None, None), strict=True
        )
    )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=programs
    )
