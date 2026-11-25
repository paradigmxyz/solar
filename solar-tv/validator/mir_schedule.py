"""Validate within-block MIR permutations using the resolved SSA actions."""

import subprocess
import sys
from collections import Counter, deque
from functools import partial

from . import abi_effects, audit, dead_ssa, mir_symbolic, native, prune
from .common import Unsupported


def candidate(paths, function):
    if any(
        path.suffix != ".mir" or path.stat().st_size > 1024 * 1024 for path in paths
    ):
        return False
    declarations = [prune.declarations(path.read_text())[1] for path in paths]
    if any(function not in table for table in declarations):
        return False
    before, after = [table[function]["text"] for table in declarations]
    return before != after and Counter(before) == Counter(after)


def state_independent(inst, definitions, fork):
    operation = inst["operation"]
    if operation["kind"] == "opcode":
        name = operation["name"]
        row = definitions.get(name)
        if row is None or not row["forks"][fork]["available"]:
            return False
        if name in {"calldataload", "calldatasize", "address"}:
            return inst["result"] is not None and row["stack"] == [
                len(operation["args"]),
                1,
            ]
    return dead_ssa.erasable(inst, definitions)


def align(before, after, total):
    """Propose a bijection by matching the operands of retained actions."""
    if any(
        before[key] != after[key]
        for key in ("id", "params", "returns", "return_buffer")
    ):
        raise Unsupported("MIR scheduling changed a function interface")
    if len(before["blocks"]) != len(after["blocks"]):
        raise Unsupported("MIR scheduling changed the block count")

    def definitions(body):
        return {
            inst["result"]: (index, inst)
            for index, block in enumerate(body["blocks"])
            for inst in (*block["phis"], *block["instructions"])
            if inst["result"] is not None
        }

    left, right = map(definitions, (before, after))
    if len(left) != len(right):
        raise Unsupported("MIR scheduling changed the number of definitions")
    known, used, pending = {}, set(), deque()

    def bind(old, new):
        if new in known:
            if known[new] != old:
                raise Unsupported("conflicting MIR schedule local correspondence")
            return
        if old in used or left[old][0] != right[new][0]:
            raise Unsupported(
                "MIR schedule requires an injective within-block correspondence"
            )
        known[new] = old
        used.add(old)
        pending.append((left[old][1], right[new][1]))

    def match(old, new):
        if isinstance(old, list) and isinstance(new, list) and len(old) == len(new):
            for a, b in zip(old, new, strict=True):
                match(a, b)
            return
        if isinstance(old, dict) and isinstance(new, dict) and old.keys() == new.keys():
            local = old.get("kind") == new.get("kind") == "local"
            for field in old:
                a, b = old[field], new[field]
                if (field == "index" and local) or (
                    field == "result" and a is not None and b is not None
                ):
                    bind(a, b)
                else:
                    match(a, b)
            return
        if old != new:
            raise Unsupported("MIR scheduling changed an action or operand type")

    for old, new in zip(before["blocks"], after["blocks"], strict=True):
        match(old["phis"], new["phis"])
        match(old["terminator"], new["terminator"])
        match(
            [step for step in old["instructions"] if not total(step)],
            [step for step in new["instructions"] if not total(step)],
        )

    def close():
        while pending:
            match(*pending.popleft())

    close()
    # An unused result still belongs to the permutation. Propose matching
    # shapes in source order, then validate the dependencies of every pair.
    buckets = {}
    for old, (index, inst) in left.items():
        if old not in used:
            key = (index, dead_ssa.stable(dead_ssa.signature(inst)))
            buckets.setdefault(key, deque()).append(old)
    for new, (index, inst) in right.items():
        if new not in known:
            key = (index, dead_ssa.stable(dead_ssa.signature(inst)))
            options = buckets.get(key, ())
            while options and options[0] in used:
                options.popleft()
            if not options:
                raise Unsupported("MIR schedule has an unmatched unused action")
            bind(options.popleft(), new)
            close()

    def rename(value):
        if isinstance(value, list):
            return [rename(item) for item in value]
        if not isinstance(value, dict):
            return value
        renamed = {key: rename(item) for key, item in value.items()}
        if renamed.get("kind") == "local":
            renamed["index"] = known[renamed["index"]]
        if renamed.get("result") is not None:
            renamed["result"] = known[renamed["result"]]
        return renamed

    return rename(after)


def proof(old, new, definitions, version, prelude):
    tables = [{body["id"]: body for body in side["functions"]} for side in (old, new)]
    left, right = tables
    if old["entry_id"] != new["entry_id"] or left.keys() != right.keys():
        raise Unsupported("MIR scheduling changed the reachable call graph")

    total = partial(state_independent, fork=version)
    descriptions = {}

    def action_key(action):
        # Intern exact descriptors from both exports. The kernel compares
        # their short identifiers without reducing complete JSON strings.
        descriptor = dead_ssa.stable(action)
        return f'"{descriptions.setdefault(descriptor, len(descriptions))}"'

    code = prelude + "set_option maxRecDepth 8192\nset_option maxHeartbeats 1000000\n"
    code += """
#print axioms MIR.Schedule.calldata_load
#print axioms MIR.Schedule.calldata_size
#print axioms MIR.Schedule.address
"""
    changed, identical = [], []
    for id_, before in left.items():
        after = right[id_]
        if before == after:
            identical.append(id_)
            continue
        renamed = align(before, after, lambda inst: total(inst, definitions))
        a, b = [
            dead_ssa.render(body, definitions, total=total, key=action_key)
            for body in (before, renamed)
        ]
        changed.append(id_)
        code += f"""
namespace ScheduleBody{id_}
def before : List MIR.DeadSSA.Block := {a}
def after : List MIR.DeadSSA.Block := {b}
private theorem admitted : MIR.Schedule.programPair before after = true := by decide
theorem contextual {{Value State Observation : Type}}
    (handlers : MIR.DeadSSA.Handlers Value State Observation) (fuel index : Nat)
    (predecessor : Option Nat) (registers : MIR.DeadSSA.Registers Value) (state : State) :
    MIR.DeadSSA.run handlers before fuel index predecessor registers state =
      MIR.DeadSSA.run handlers after fuel index predecessor registers state :=
  MIR.Schedule.run_eq handlers before after admitted fuel index predecessor registers state
#print axioms contextual
end ScheduleBody{id_}
"""
    if identical:
        code += abi_effects.retained_callee_code(
            [[table[id_] for id_ in identical] for table in tables], definitions
        )
    if not changed:
        raise Unsupported("no changed MIR schedules")
    return code, changed, identical


def prove_storage_reads(texts, old, new, replay, version, timeout, function):
    if (
        len(old["functions"]) != 1
        or len(new["functions"]) != 1
        or not any(
            inst["operation"].get("name") == "sload"
            for block in old["functions"][0]["blocks"]
            for inst in block["instructions"]
        )
    ):
        raise Unsupported("MIR storage schedule needs a single body with storage reads")
    programs = tuple(
        mir_symbolic.translate(text, function, replay, version, timeout)
        for text in texts
    )
    before, after = programs
    if before.interface != after.interface:
        raise Unsupported("MIR storage schedule changed the function interface")
    result = mir_symbolic.prove_direct(*programs, timeout, preserve_memory_terms=True)
    if not mir_symbolic.proved(result):
        raise Unsupported(
            "direct MIR storage schedule proof did not close\n" + result.stdout[-2000:]
        )
    for program in programs:
        program.import_evidence["storage_read_schedule"] = True
    return audit.Result("verified", stdout="verified\n", program=before, pair=programs)


def check(paths, solar, replay, version, timeout, function, *, verbose=False):
    from .check import PRELUDE

    try:
        if not candidate(paths, function):
            return None
        texts, _, modules = abi_effects.typed_pair(
            paths, solar, replay, version, timeout
        )
        left, right = [
            {key: value for key, value in module.items() if key != "functions"}
            for module in modules
        ]
        if left != right:
            raise Unsupported("MIR scheduling changed the module entry bindings")
        old, new = [
            abi_effects.selected_body(module, function, tail_calls=True)[0]
            for module in modules
        ]
        definitions = {
            row["name"]: row for row in native.catalog(replay, timeout)["evmir"]
        }
        try:
            code, changed, identical = proof(old, new, definitions, version, PRELUDE)
        except Unsupported:
            return prove_storage_reads(
                texts, old, new, replay, version, timeout, function
            )
        return abi_effects.certificate(
            code,
            texts,
            function,
            version,
            timeout,
            {
                "kind": "kernel-checked-mir-schedule",
                "rescheduled_functions": changed,
                "unchanged_functions": identical,
            },
            scope="mir-schedule",
        )
    except (Unsupported, OSError, subprocess.TimeoutExpired) as error:
        if verbose:
            print(f"MIR schedule: {error}", file=sys.stderr)
        return None
