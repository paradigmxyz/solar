"""Deletion-only, contextual SSA simulation across arbitrary CFG cycles.

Independent source exports retain every phi, transfer and effect. An injective
local-ID correspondence aligns retained definitions; the kernel checks exact
action erasure and that no retained read can observe an erased result. Only
total scalar definitions and typed slice construction/projection may disappear.
This is an unpriced action-machine proof, not direct interpreter or gas
correspondence.
"""

import copy
import hashlib
import json
import re
import subprocess
import tempfile
from pathlib import Path

from . import audit, mir_effects, mir_state, native, prune
from .common import Program, Unsupported


def stable(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def reads(value):
    if isinstance(value, list):
        return [index for item in value for index in reads(item)]
    if not isinstance(value, dict):
        return []
    if value.get("kind") == "undefined":
        raise Unsupported("dead SSA excludes undefined operands")
    if value.get("kind") == "local":
        index = value["index"]
        if type(index) is not int or not 0 <= index < 2**32:
            raise Unsupported("invalid dead SSA local index")
        return [index]
    return [index for item in value.values() for index in reads(item)]


def signature(value):
    """Alignment hints alone are never evidence; the complete renamed data follows."""
    if isinstance(value, list):
        return [signature(item) for item in value]
    if not isinstance(value, dict):
        return value
    return {
        key: signature(item)
        for key, item in value.items()
        if key != "result" and not (key == "index" and value.get("kind") == "local")
    }


def align(before, after):
    if len(before["blocks"]) != len(after["blocks"]):
        raise Unsupported("dead SSA requires unchanged block identities")
    mapping = {}
    mapped = set()
    removed = []

    def define(old, new):
        if old is None or new is None:
            if old != new:
                raise Unsupported("dead SSA changes a result representation")
            return
        if new in mapping or old in mapped:
            raise Unsupported("dead SSA local correspondence is not injective")
        mapping[new] = old
        mapped.add(old)

    for left, right in zip(before["blocks"], after["blocks"], strict=True):
        if len(left["phis"]) != len(right["phis"]):
            raise Unsupported("dead SSA cannot erase or change phi groups")
        for old, new in zip(left["phis"], right["phis"], strict=True):
            define(old["result"], new["result"])
        a, b = left["instructions"], right["instructions"]
        wanted = [stable(signature(inst)) for inst in b]
        position = 0
        # A deletion-only subsequence is linear in the instruction count. When
        # repeated signatures are ambiguous, exact data validation below may
        # conservatively reject; an alignment guess is never a certificate.
        for inst in a:
            if position < len(b) and stable(signature(inst)) == wanted[position]:
                define(inst["result"], b[position]["result"])
                position += 1
            else:
                removed.append(inst)
        if position != len(b):
            raise Unsupported("dead SSA only admits instruction deletion")
    if not removed or any(inst["result"] is None for inst in removed):
        raise Unsupported("dead SSA requires erased result definitions")
    dead = [inst["result"] for inst in removed]
    if len(dead) != len(set(dead)) or set(dead) & set(mapping.values()):
        raise Unsupported("dead SSA result correspondence overlaps erased definitions")

    def rename(value):
        if isinstance(value, list):
            return [rename(item) for item in value]
        if not isinstance(value, dict):
            return value
        result = {key: rename(item) for key, item in value.items()}
        if result.get("kind") == "local":
            if result["index"] not in mapping:
                raise Unsupported("dead SSA has an unmapped local use")
            result["index"] = mapping[result["index"]]
        if result.get("result") is not None:
            if result["result"] not in mapping:
                raise Unsupported("dead SSA has an unmapped definition")
            result["result"] = mapping[result["result"]]
        return result

    aligned = rename(after)
    expected = copy.deepcopy(before)
    for block in expected["blocks"]:
        block["instructions"] = [
            inst for inst in block["instructions"] if inst["result"] not in dead
        ]
    if expected != aligned:
        raise Unsupported("dead SSA changed a retained action, phi or transfer")
    return aligned, dead, mapping


def erasable(inst, definitions):
    if inst["result"] is None or inst["type"] is None:
        return False
    op = inst["operation"]
    result = inst["type"]
    if op["kind"] in {"make_slice", "extract_value"}:
        # These signatures match MIR.DeadSlice.make and MIR.DeadSlice.extract.
        # The module check rejects explicit undef across the supplied bodies.
        make = op["kind"] == "make_slice"
        slice_ty = result if make else op["aggregate_type"]
        if slice_ty["kind"] != "slice" or slice_ty["location"] not in {
            "memory",
            "calldata",
            "returndata",
        }:
            return False
        fields = list(map(mir_state.value_type, mir_state.leaf_types(slice_ty)))
        args = [mir_state.value_type(arg["type"]) for arg in op["args"]]
        if make:
            return op["location"] == slice_ty["location"] and args == fields
        index = op["index"]
        return (
            args == [mir_state.value_type(slice_ty)]
            and type(index) is int
            and 0 <= index < len(fields)
            and mir_state.value_type(result) == fields[index]
        )

    # A cast between a word and a pointer reads scalar bits. It never accesses
    # that address space. Keep the pointer type in the action's identity.
    def scalar_width(ty):
        if op["kind"] == "word" and ty["kind"] == "pointer" and ty["bits"] == 256:
            mir_state.value_type(ty)
            return 256
        return mir_effects.scalar_width(ty)

    try:
        bits = scalar_width(result)
        args = [("unused", scalar_width(arg["type"])) for arg in op["args"]]
        return mir_effects.pure(op, args, bits, definitions) is not None
    except Unsupported:
        return False


def render(body, definitions):
    result = []
    for block in body["blocks"]:
        actions = []
        if block["phis"]:
            phi = block["phis"]
            actions.append(
                f"⟨{json.dumps(stable(phi))},{reads(phi)},{[p['result'] for p in phi]},false⟩"
            )
        for inst in block["instructions"]:
            writes = [] if inst["result"] is None else [inst["result"]]
            actions.append(
                f"⟨{json.dumps(stable(inst))},{reads(inst)},{writes},{str(erasable(inst, definitions)).lower()}⟩"
            )
        term = block["terminator"]
        result.append(
            f"⟨[{','.join(actions)}],{json.dumps(stable(term))},{reads(term)}⟩"
        )
    code = "[" + ",".join(result) + "]"
    if len(code.encode()) > 262144:
        raise Unsupported("dead SSA action program exceeds 256 KiB")
    return code


def export(text, function, executable, timeout):
    result = subprocess.run(
        [executable, "--mir-model"],
        input=json.dumps({"version": 1, "text": text, "function": function}),
        capture_output=True,
        text=True,
        check=False,
        timeout=timeout,
    )
    try:
        value = json.loads(result.stdout)
        if result.returncode or value.get("status") != "ok":
            raise Unsupported(
                "dead SSA source: " + value.get("reason", "invalid export")
            )
        return value["model"]
    except Unsupported:
        raise
    except (ValueError, KeyError, TypeError) as error:
        raise Unsupported("invalid dead SSA source export") from error


def check(paths, solar, executable, version, timeout, function):
    from .check import PRELUDE, ROOT, build_model, run

    texts, models, declarations = [], [], []
    definitions = {
        row["name"]: row for row in native.catalog(executable, timeout)["evmir"]
    }
    for path in paths:
        if path.suffix != ".mir" or path.stat().st_size > 1024 * 1024:
            raise Unsupported("dead SSA requires MIR files of at most 1 MiB")
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
        texts.append(parsed.stdout)
        models.append(export(parsed.stdout, function, executable, timeout))
        declarations.append(prune.declarations(parsed.stdout))
    (header, old), (other_header, new) = declarations
    if (
        header != other_header
        or list(old) != list(new)
        or function not in old
        or any(old[name]["text"][0] != new[name]["text"][0] for name in old)
    ):
        raise Unsupported("dead SSA changed module declarations or interfaces")
    # An opaque internal-call action is sound only with the same closed supplied
    # implementation. Re-entering this changed body needs a call simulation.
    pending, callees = list(old[function]["targets"]), set()
    while pending:
        name = pending.pop()
        if name == function:
            raise Unsupported("dead SSA excludes callees re-entering the changed body")
        if name in callees:
            continue
        if name not in old or old[name] != new[name]:
            raise Unsupported("dead SSA changed a supplied callee")
        callees.add(name)
        pending.extend(old[name]["targets"])
    entries = []
    for model in models:
        bodies = {body["id"]: body for body in model["functions"]}
        if len(bodies) != len(model["functions"]) or model["entry_id"] not in bodies:
            raise Unsupported("invalid dead SSA function table")
        for body in bodies.values():
            reads(body)
            for block in body["blocks"]:
                if block["terminator"]["kind"] in {"tail_call", "selfdestruct"}:
                    raise Unsupported("dead SSA excludes tail calls and SELFDESTRUCT")
                for inst in block["instructions"]:
                    op = inst["operation"]
                    mir_state.operation(op, definitions)
                    if op["kind"] in {
                        "extcall",
                        "layout",
                        "internal_frame_load",
                        "internal_frame_store",
                    }:
                        raise Unsupported(
                            "dead SSA excludes messages and placement queries"
                        )
                    if op["kind"] == "opcode":
                        row = definitions[op["name"]]
                        if (
                            not row["forks"][version]["available"]
                            or row["observes_code"]
                            or row["sets_returndata"]
                            or row["observes_gas"]
                            and row["gas"] != "SStore"
                        ):
                            raise Unsupported(
                                "dead SSA excludes code/gas observers and external calls"
                            )
        entries.append(bodies[model["entry_id"]])
    if models[0]["entry_id"] != models[1]["entry_id"]:
        raise Unsupported("dead SSA changed the selected function identity")
    aligned, dead, mapping = align(*entries)
    if any(
        not erasable(inst, definitions)
        for block in entries[0]["blocks"]
        for inst in block["instructions"]
        if inst["result"] in dead
    ):
        raise Unsupported("dead SSA cannot erase a trapping or stateful operation")
    for block in aligned["blocks"]:
        if set(reads(block)) & set(dead):
            raise Unsupported(
                "dead SSA result remains live in an action, phi or transfer"
            )
    expressions = [render(body, definitions) for body in (entries[0], aligned)]
    code = PRELUDE + "set_option maxRecDepth 8192\nset_option maxHeartbeats 1000000\n"
    code += f"def before : List MIR.DeadSSA.Block := {expressions[0]}\n"
    code += f"def after : List MIR.DeadSSA.Block := {expressions[1]}\n"
    code += f"def dead : List Nat := {dead}\n"
    code += """
theorem valid : before.all (MIR.DeadSSA.safe dead) = true := by decide
theorem erased : after = before.map (MIR.DeadSSA.eraseBlock dead) := by rfl
theorem contextual {Value State Observation : Type}
 (handlers : MIR.DeadSSA.Handlers Value State Observation) (fuel : Nat)
 (registers : MIR.DeadSSA.Registers Value) (state : State) :
 MIR.DeadSSA.run handlers before fuel 0 none registers state =
 MIR.DeadSSA.run handlers after fuel 0 none registers state := by
 rw [erased]
 exact MIR.DeadSSA.simulation handlers dead before valid fuel 0 none registers registers state
   (fun _ _ => rfl)
#print axioms contextual
"""
    built = build_model(timeout)
    if built.returncode:
        raise Unsupported(built.stdout + built.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-dead-ssa-") as folder:
        proof = Path(folder) / "Pair.lean"
        proof.write_text(code)
        result = run(["lake", "env", "lean", proof], ROOT, timeout)
    axioms = re.search(r"'contextual' depends on axioms: \[(.*?)\]", result.stdout)
    if result.returncode or not (
        "'contextual' does not depend on any axioms" in result.stdout
        or axioms
        and not set(axioms[1].split(", "))
        - {"propext", "Quot.sound", "Classical.choice"}
    ):
        raise Unsupported(
            "dead SSA simulation did not close\n" + result.stdout + result.stderr
        )
    pair = tuple(
        Program(
            (function, "dead-ssa"),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            selector=function,
            scope="dead-ssa",
            evm_version=version,
            import_evidence={
                "kind": "kernel-checked-cyclic-dead-ssa",
                "source_sha256": hashlib.sha256(stable(model).encode()).hexdigest(),
                "action_sha256": hashlib.sha256(expression.encode()).hexdigest(),
                "erased_results": dead,
                "blocks": len(entries[0]["blocks"]),
                "local_correspondence": sorted(mapping.items()),
                "unchanged_callees": sorted(callees),
                "boundary": "trusted compiler parser/schema export, injective local-ID normalization and action renderer; total pure actions only; arbitrary shared stateful handlers; no direct-interpreter, physical gas or backend-layout claim",
            },
        )
        for text, model, expression in zip(texts, models, expressions, strict=True)
    )
    return audit.Result("verified", stdout="verified\n", program=pair[0], pair=pair)
