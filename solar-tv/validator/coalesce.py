"""Allocation-prefix certificates with exact, unchanged call continuations.

The independent schema export supplies SSA operands. Only uninitialized,
infallible, word-aligned reservations and constant word initializers are admitted
before the cut. Reachable calls and their cyclic bodies remain unchanged.
Unreachable bodies are outside this function-local certificate.
The byte-memory proof assumes the allocation producer's well-formed, nonwrapping
cursor contract; it never chooses a cursor or assumes callees terminate.
"""

import hashlib
import json
import re
import subprocess
import tempfile
from pathlib import Path

from . import audit, prune
from .common import Program, Unsupported, literal


def project(model):
    functions = {body["id"]: body for body in model["functions"]}
    entry = functions[model["entry_id"]]
    if entry["params"] or entry["returns"] or len(entry["blocks"]) != 3:
        raise Unsupported("allocation cut requires a void, guarded constructor")
    guard, body, failure = entry["blocks"]
    if (
        guard["phis"]
        or body["phis"]
        or failure["phis"]
        or failure["instructions"]
        or failure["terminator"]
        != {
            "kind": "revert",
            "args": [
                {
                    "kind": "constant",
                    "type": {"bits": 256, "kind": "word", "name": "i256"},
                    "value": "0",
                },
                {
                    "kind": "constant",
                    "type": {"bits": 256, "kind": "word", "name": "i256"},
                    "value": "0",
                },
            ],
        }
        or body["terminator"] != {"kind": "return", "args": []}
        or guard["terminator"].get("kind") != "branch"
        or guard["terminator"].get("then") != 2
        or guard["terminator"].get("else") != 1
        or [i["operation"]["kind"] for i in guard["instructions"]] != ["opcode", "ne"]
        or guard["instructions"][0]["operation"]
        != {"kind": "opcode", "name": "callvalue", "args": []}
    ):
        raise Unsupported("allocation cut requires an unchanged nonpayable guard")
    # The two guards must also agree exactly across the pair. Their effects run
    # before either prefix, so a failed guard cannot be discarded by the cut.
    locals, plan, calls = {}, [], []
    reserved = 0

    def operand(value):
        if value["kind"] == "constant":
            literal(value["value"])
            return "constant", int(value["value"], 0) if value["value"].startswith(
                "0x"
            ) else int(value["value"])
        if value["kind"] == "local" and value["index"] in locals:
            return locals[value["index"]]
        raise Unsupported("allocation cut has an unknown SSA operand")

    instructions = body["instructions"]
    lowered = 0
    if instructions and instructions[0]["operation"].get("name") == "mload":
        # Match the entire initial bump, not arbitrary loads or stores to 0x40.
        # The kernel bridge requires the same explicit producer contract as alloc.
        size, locals = lowered_reservation(instructions[:4])
        plan.append(("reserve", size))
        reserved = size
        lowered = 1
        instructions = instructions[4:]
    for inst in instructions:
        op = inst["operation"]
        args = [operand(value) for value in op["args"]]
        result = None
        if op["kind"] == "call":
            if inst["result"] is not None or inst["type"] is not None:
                raise Unsupported("allocation cut only admits void call continuations")
            if op["function"] not in functions:
                raise Unsupported("allocation cut has a missing call target")
            calls.append((op["function"], args))
            continue
        if calls:
            raise Unsupported("allocation cut changed an instruction after a call")
        if op["kind"] == "allocate":
            if (
                lowered
                or op["checked"]
                or op["zeroed"]
                or len(args) != 1
                or args[0][0] != "constant"
                or not 0 < args[0][1] <= 65536
                or args[0][1] % 32
            ):
                raise Unsupported(
                    "allocation cut requires aligned infallible reservations"
                )
            result = "pointer", reserved
            reserved += args[0][1]
            plan.append(("reserve", args[0][1]))
        elif (
            op["kind"] == "low_bits"
            and op["bits"] == 256
            and len(args) == 1
            or op["kind"] == "word"
            and len(args) == 1
        ):
            result = args[0]
        elif op["kind"] == "opcode" and op["name"] == "add" and len(args) == 2:
            (kind, offset), (other, increment) = args
            if (
                kind != "pointer"
                or other != "constant"
                or offset + increment > reserved
            ):
                raise Unsupported(
                    "allocation cut pointer arithmetic leaves its reservation"
                )
            result = "pointer", offset + increment
        elif op["kind"] == "opcode" and op["name"] == "mstore" and len(args) == 2:
            (kind, offset), (other, value) = args
            if (
                kind != "pointer"
                or other != "constant"
                or offset % 32
                or offset + 32 > reserved
            ):
                raise Unsupported("allocation cut writes outside an initialized word")
            plan.append(("store", offset, value))
        else:
            raise Unsupported(
                "allocation cut contains an observable or unsupported operation"
            )
        if inst["result"] is not None:
            if result is None:
                raise Unsupported("allocation cut instruction result mismatch")
            locals[inst["result"]] = result
        elif result is not None:
            raise Unsupported("allocation cut instruction lacks its SSA result")
    if not plan or plan[0][0] != "reserve" or not calls or reserved > 65536:
        raise Unsupported(
            "allocation cut requires a reserved prefix and call continuation"
        )
    return {
        "plan": plan,
        "calls": calls,
        "guard": guard,
        "extent": reserved,
        "lowered_reservations": lowered,
    }


def check(paths, solar, replay, version, timeout, function):
    from .check import PRELUDE, ROOT, build_model, run

    texts, tables, models, projections = [], [], [], []
    for path in paths:
        if path.suffix != ".mir" or path.stat().st_size > 1024 * 1024:
            raise Unsupported("allocation cut requires MIR files of at most 1 MiB")
        parsed = run(
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
        if parsed.returncode:
            raise Unsupported(parsed.stderr)
        exported = subprocess.run(
            [replay, "--mir-model"],
            input=json.dumps(
                {"version": 1, "text": parsed.stdout, "function": function}
            ),
            capture_output=True,
            text=True,
            check=False,
            timeout=timeout,
        )
        value = json.loads(exported.stdout)
        if exported.returncode or value.get("status") != "ok":
            raise Unsupported("allocation cut source export failed")
        models.append(value["model"])
        projections.append(project(value["model"]))
        texts.append(parsed.stdout)
        tables.append(prune.declarations(parsed.stdout))
    (header, before), (other_header, after) = tables
    if (
        header != other_header
        or before.keys() != after.keys()
        or function not in before
    ):
        raise Unsupported("allocation cut changed module declarations")
    if not any(before[name]["root"] for name in before):
        raise Unsupported("allocation cut requires explicit external entries")
    if before[function]["text"][0] != after[function]["text"][0]:
        raise Unsupported("allocation cut changed the selected interface")
    if not re.search(r"(?:\[|,\s*)constructor(?:,|\])", before[function]["text"][0]):
        raise Unsupported("allocation cut requires a constructor entry")
    if any(before[name]["text"][0] != after[name]["text"][0] for name in before):
        raise Unsupported("allocation cut changed a supplied function interface")
    if list(before) != list(after):
        raise Unsupported("allocation cut changed numeric call identities")
    pending = list(before[function]["targets"])
    visited = set()
    while pending:
        name = pending.pop()
        if name == function:
            raise Unsupported(
                "allocation cut continuation re-enters the changed constructor"
            )
        if name in visited:
            continue
        if name not in before:
            raise Unsupported("allocation cut continuation has an unknown target")
        if before[name] != after[name]:
            raise Unsupported("allocation cut changed a reachable callee")
        visited.add(name)
        pending.extend(before[name]["targets"])
    # Only this constructor is certified. Other runtime bodies may be lowered
    # by the same pass, but may not enter its closed, unchanged call suffix.
    unrelated_changes = [
        name
        for name in before
        if name != function and name not in visited and before[name] != after[name]
    ]
    a, b = projections
    if a["guard"] != b["guard"] or a["calls"] != b["calls"]:
        raise Unsupported("allocation cut changed its guard or call continuation")
    if (
        a["plan"] == b["plan"]
        and a["lowered_reservations"] == b["lowered_reservations"]
    ):
        raise Unsupported("allocation cut requires a changed reservation prefix")

    def plan(value):
        return (
            "["
            + ",".join(
                f".reserve {step[1]}"
                if step[0] == "reserve"
                else f".store {step[1]} ({literal(str(step[2]))})"
                for step in value
            )
            + "]"
        )

    code = PRELUDE + "set_option maxHeartbeats 1000000\n"
    code += f"def before : List MIR.CoalesceAlloc.Step := {plan(a['plan'])}\n"
    code += f"def after : List MIR.CoalesceAlloc.Step := {plan(b['plan'])}\n"
    for side, projection in zip(("before", "after"), projections, strict=True):
        if projection["lowered_reservations"]:
            size = projection["plan"][0][1]
            tail = plan(projection["plan"][1:])
            source = f"MIR.CoalesceAlloc.run base {tail} (MIR.LowerAllocPrefix.loweredReserve {size} state)"
            bridge = f"by\n  unfold {side}_source\n  rw [MIR.LowerAllocPrefix.reservation base {size} state (by simpa [{side}, MIR.CoalesceAlloc.extent] using valid)]\n  rfl"
        else:
            source = f"MIR.CoalesceAlloc.run base {side} state"
            bridge = "by rfl"
        code += f"""
def {side}_source (base : Nat) (state : MIR.CoalesceAlloc.Result) := {source}
theorem {side}_bridge (base : Nat) (state : MIR.CoalesceAlloc.Result)
 (valid : MIR.CoalesceAlloc.ProducerContract base (MIR.CoalesceAlloc.extent {side}) state) :
 {side}_source base state = MIR.CoalesceAlloc.run base {side} state := {bridge}
"""
    code += """
theorem extent : MIR.CoalesceAlloc.extent before = MIR.CoalesceAlloc.extent after := by decide
theorem data : MIR.CoalesceAlloc.stores before = MIR.CoalesceAlloc.stores after := by decide
theorem left_valid : MIR.CoalesceAlloc.wellFormed 0 before = true := by decide
theorem right_valid : MIR.CoalesceAlloc.wellFormed 0 after = true := by decide
theorem contextual {α : Sort _} (base : Nat) (state : MIR.CoalesceAlloc.Result)
 (valid : MIR.CoalesceAlloc.ProducerContract base (MIR.CoalesceAlloc.extent before) state)
 (continuation : MIR.CoalesceAlloc.Result → α) :
 continuation (before_source base state) = continuation (after_source base state) := by
 rw [before_bridge base state valid,
     after_bridge base state (by simpa [← extent] using valid)]
 exact MIR.CoalesceAlloc.contextual before after base state valid.1 valid.2.2.2.1 (by decide) (by decide) extent data continuation
#print axioms contextual
"""
    build = build_model(timeout)
    if build.returncode:
        raise Unsupported(build.stdout + build.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-allocation-cut-") as folder:
        proof = Path(folder) / "Pair.lean"
        proof.write_text(code)
        result = run(["lake", "env", "lean", proof], ROOT, timeout)
    axioms = re.search(r"'contextual' depends on axioms: \[(.*?)\]", result.stdout)
    if (
        result.returncode
        or not axioms
        or set(axioms[1].split(", ")) - {"propext", "Quot.sound", "Classical.choice"}
    ):
        raise Unsupported(
            "allocation prefix proof did not close\n" + result.stdout + result.stderr
        )
    programs = tuple(
        Program(
            (function, "allocation-prefix"),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            selector=function,
            scope="allocation-prefix",
            evm_version=version,
            import_evidence={
                "kind": "allocation-prefix-with-shared-continuation",
                "source_sha256": hashlib.sha256(
                    json.dumps(model, sort_keys=True).encode()
                ).hexdigest(),
                "reserved_bytes": projection["extent"],
                "plan": projection["plan"],
                "calls": projection["calls"],
                "lowered_reservations": projection["lowered_reservations"],
                "unchanged_callees": sorted(visited),
                "unrelated_changed_bodies": unrelated_changes,
                "boundary": "trusted compiler parser/schema export and SSA-to-byte-memory projection; primitive allocation producer contract and continuation binding remain trusted; no physical gas or backend layout claim",
            },
        )
        for text, model, projection in zip(texts, models, projections, strict=True)
    )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=programs
    )


def lowered_reservation(instructions):
    """Recognize one initial, uninterrupted FMP reservation at full word width."""
    if len(instructions) != 4:
        raise Unsupported("allocation cut requires a complete initial FMP bump")
    load, cast, add, store = instructions
    word = {"bits": 256, "kind": "word", "name": "i256"}
    pointer = {
        "bits": 256,
        "kind": "pointer",
        "location": "memory",
        "name": "ptr<memory>",
    }

    def constant(value):
        return {"kind": "constant", "type": word, "value": str(value)}

    def local(inst):
        return {"kind": "local", "type": inst["type"], "index": inst["result"]}

    args = add["operation"].get("args", [])
    if len(args) != 2 or args[1].get("kind") != "constant":
        raise Unsupported("allocation cut requires a constant FMP increment")
    size = int(args[1]["value"], 0)
    if (
        not 0 < size <= 65536
        or size % 32
        or [inst["type"] for inst in instructions] != [word, pointer, word, None]
        or any(inst["result"] is None for inst in instructions[:3])
        or store["result"] is not None
        or load["operation"]
        != {"kind": "opcode", "name": "mload", "args": [constant(64)]}
        or cast["operation"] != {"kind": "word", "args": [local(load)]}
        or add["operation"]
        != {"kind": "opcode", "name": "add", "args": [local(load), constant(size)]}
        or store["operation"]
        != {"kind": "opcode", "name": "mstore", "args": [constant(64), local(add)]}
    ):
        raise Unsupported("allocation cut requires an exact initial FMP bump")
    return size, {
        load["result"]: ("pointer", 0),
        cast["result"]: ("pointer", 0),
        add["result"]: ("pointer", size),
    }
