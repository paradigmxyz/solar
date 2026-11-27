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
    stack_layout,
)
from .common import Program, Unsupported

GUARD = re.compile(r"push bb(\d+)")
SIMP = (
    ", EVM.StackSchedule.trace, EVM.StackSchedule.expand, EVM.StackSchedule.Refines, "
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
        if (
            GUARD.fullmatch(line)
            or code_relocations.operation(line)
            or stack_layout.immutable_push(line)
        ):
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


def recovery_suffix(tail, catalog, version):
    """Only forward a GAS sample directly into the funded recovery policy."""
    observers = [
        index
        for index, line in enumerate(tail)
        if line != "sstore" and peephole.observes_resources(line, catalog)
    ]
    if not observers:
        return False
    if (
        version != "cancun"
        or len(observers) != 2
        or observers[0] == 0
        or observers[1] != observers[0] + 1
        or tail[observers[0] - 1 : observers[1] + 1] != ["push 1", "gas", "staticcall"]
    ):
        raise Unsupported("scheduled continuation observes resources or code layout")
    return True


def repeated_partition(common, middles):
    """Keep the earlier write and the complete path to a deleted word store."""
    old, new = middles
    if (
        len(old) != 3
        or new
        or re.fullmatch(r"dup \d+", old[0]) is None
        or re.fullmatch(r"push \d+", old[1]) is None
        or old[2] != "mstore"
    ):
        return None
    for start in reversed(range(len(common) - 2)):
        if common[start + 1 : start + 3] == old[1:] and re.fullmatch(
            r"dup \d+", common[start]
        ):
            return common[:start], [common[start:] + middle for middle in middles]
    return None


def certificate(
    models,
    common,
    tail,
    imported,
    aborts,
    version,
    height,
    entry_limit,
    *,
    repeated=False,
):
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
def {name} (labelAddress : Nat → Word) : List EVM.Effects.Op := [{",".join(ops)}]
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
        "(fun ctx stack _ => matched labelAddress ctx stack)",
    )
    if height is not None:
        relation = "=" if entry_limit is None else "≤"
        condition = f" (fits : stack.length {relation} {height})"
        supported = f"fun stack => stack.length {relation} {height}"
        matched = "(matched labelAddress)"
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
    interpreter = "RepeatedStore" if repeated else "StackSchedule"
    policy = "(policy : EVM.RepeatedStore.Policy handler)" if repeated else ""
    use_policy = "policy " if repeated else ""
    code += f"""
private theorem sharedPrefix : afterCommon = beforeCommon := by decide
private theorem sharedTail : afterTail = beforeTail := by decide
private theorem matched (labelAddress : Nat → Word) (ctx : EVM.Context) (stack : EVM.Stack){condition} :
    EVM.StackSchedule.Refines (EVM.{interpreter}.trace ctx .{version} aborts (after labelAddress) stack)
      (EVM.{interpreter}.trace ctx .{version} aborts (before labelAddress) stack) := by
"""
    for i in range(16):
        code += f"  have index{i} : (({i} : Fin 256) : Nat) = {i} := by decide\n"
    simplifier = evm_effects.stack_proof(
        need,
        peak,
        SIMP
        + (
            ", EVM.RepeatedStore.trace, EVM.RepeatedStore.erase, "
            "EVM.RepeatedStore.update, EVM.RepeatedStore.redundant, EVM.RepeatedStore.written"
            if repeated
            else ""
        ),
        discharger="first | decide | omega",
        height=height if entry_limit is None else None,
        bounded=entry_limit is not None,
        max_steps=600000,
    )
    return (
        code
        + "\n".join("  " + line for line in simplifier.splitlines())
        + f"""

theorem contextual {{World Halt : Type}} (labelAddress : Nat → Word)
    (enter : List String → EVM.Suffix.PrefixResult World Halt)
    (handler : EVM.StackSchedule.Handler World) {policy} (exit : EVM.StackSchedule.Exit) (final : World)
    {binding}
    (ran : EVM.StackSchedule.observe enter handler .{version} beforeCommon aborts (before labelAddress) =
      .done exit final) :
    EVM.StackSchedule.observe enter handler .{version} afterCommon aborts (after labelAddress) = .done exit final ∧
      afterTail = beforeTail := by
  rw [sharedPrefix]{bridge}
  exact ⟨EVM.{interpreter}.contextual enter handler {use_policy}.{version} beforeCommon aborts (before labelAddress) (after labelAddress)
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
        prefix.export(
            text,
            entry,
            version,
            replay,
            timeout,
            code_tail=True,
            immutables=any(
                line.strip().startswith("push_immutable ") for line in text.splitlines()
            ),
        )
        for text in texts
    ]
    common, middles, tail = peephole.partition(models, retain_code_prefix=True)
    redundant_store = repeated_partition(common, middles)
    if redundant_store is not None:
        common, middles = redundant_store
    catalog = {op["name"]: op for op in native.catalog(replay, timeout)["evmir"]}
    recovery = recovery_suffix(tail, catalog, version)
    limit = 512 if recovery else 256
    if len(common) + max(map(len, middles)) + len(tail) > limit:
        raise Unsupported(f"stack schedule window exceeds {limit} instructions")
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
        evm_effects.instructions(
            code, catalog, version, labels, composed=True, label_pushes=True
        )
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
        models,
        common,
        tail,
        imported,
        aborts,
        version,
        cut_height,
        entry_limit,
        repeated=redundant_store is not None,
    )
    if recovery:
        # The shared continuation can sample different words at this one site.
        # For two funded calls the ordinary transition charges a stable debit.
        definitions += "#print axioms EVM.State.CallRefinement.paired_recovery\n"
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
    recovery_axioms = re.search(
        r"'SolarTV.EVM.State.CallRefinement.paired_recovery' depends on axioms: \[(.*?)\]",
        result.stdout,
    )
    if (
        result.returncode
        or not axioms
        or set(axioms[1].split(", ")) - {"propext", "Quot.sound", "Classical.choice"}
        or recovery
        and (
            not recovery_axioms
            or set(recovery_axioms[1].split(", "))
            - {"propext", "Quot.sound", "Classical.choice"}
        )
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
                **({"repeated_store": True} if redundant_store is not None else {}),
                **(
                    {"funded_recovery": {"destination": 1, "child_debit": 3000}}
                    if recovery
                    else {}
                ),
                **(
                    {
                        "shared_immutables": sorted(
                            set(filter(stack_layout.immutable_push, common + tail))
                        )
                    }
                    if any(stack_layout.immutable_push(line) for line in common + tail)
                    else {}
                ),
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
