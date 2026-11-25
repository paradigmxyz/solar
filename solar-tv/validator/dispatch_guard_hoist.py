"""Bind a hoisted ABI head-size guard to the original typed wrappers."""

import re

from . import mir_state
from .common import Unsupported


def guards(source, entry, selectors, before, after):
    names = list(before)
    if (
        len(selectors) < 2
        or source["receive_route"] is not None
        or source["fallback_route"] is not None
        or names != list(after)
    ):
        raise Unsupported("calldata guard hoist requires selector-only dispatch")

    sizes = set()
    shapes = []
    edited = set()
    for route in selectors:
        wrapper = route["id"]
        head = source["functions"][wrapper]["blocks"][0]
        instructions = head["instructions"]
        branch = head["terminator"]
        if (
            len(instructions) != 2
            or branch["kind"] != "branch"
            or instructions[0]["operation"].get("name") != "calldatasize"
            or instructions[1]["operation"].get("name") != "lt"
        ):
            raise Unsupported("wrapper does not start with a plain calldata-size guard")
        bound = instructions[1]["operation"]["args"][1]
        if bound["kind"] != "constant":
            raise Unsupported("wrapper calldata-size bound must be constant")
        minimum = int(bound["value"], 0)
        if not 4 <= minimum < 2**64:
            raise Unsupported(
                "wrapper calldata-size bound is outside the ABI head range"
            )
        sizes.add(minimum)

        name = names[wrapper]
        old, new = before[name], after[name]
        if len(old) != len(new):
            raise Unsupported("calldata guard hoist changed a retained declaration")
        changes = [(a, b) for a, b in zip(old, new, strict=True) if a != b]
        if (
            len(changes) != 1
            or not (jump := re.fullmatch(r"jumpi [^,]+, bb\d+, (bb\d+)", changes[0][0]))
            or changes[0][1] != f"jump {jump[1]}"
        ):
            raise Unsupported(
                "calldata guard hoist changed code beyond the wrapper guard"
            )
        edited.add(name)
        shape = (
            wrapper + (wrapper >= entry),
            instructions[0]["result"],
            instructions[1]["result"],
            branch["then"],
            branch["else"],
        )
        shapes.append("⟨" + ",".join(map(mir_state.number, shape)) + "⟩")
    if len(sizes) != 1 or any(
        before[name] != after[name] for name in before.keys() - edited
    ):
        raise Unsupported(
            "calldata guard hoist requires identical bounds and retained bodies"
        )
    return sizes.pop(), shapes


def retained_proof(minimum, shapes):
    guards = ",".join(f"guard{index}" for index in range(len(shapes)))
    declarations = "\n".join(
        f"def guard{index} : MIR.DispatchGuardHoist.Guard := {shape}"
        for index, shape in enumerate(shapes)
    )
    cases = " | ".join("rfl" for _ in shapes)
    obligations = "\n".join(
        f"  · exact ⟨guard{index},rfl,_,_,rfl,rfl,rfl,rfl,rfl⟩"
        for index in range(len(shapes))
    )
    return f"""
{declarations}
theorem retained : after = MIR.DispatchGuardHoist.lower [{guards}] reference := by rfl
theorem wrappers : ∀ id ∈ routes.selectors.map Prod.snd,
    ∃ guard : MIR.DispatchGuardHoist.Guard, guard.wrapper = id ∧
      MIR.DispatchGuardHoist.Matches reference after {minimum} guard := by
  intro id member
  simp [routes] at member
  rcases member with {cases}
{obligations}
"""
