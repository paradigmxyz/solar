"""Certify the search-to-shift region of a captured two-slot storage loop."""

import hashlib
import json
import tempfile
from pathlib import Path

from . import audit, builtin_lowering, dead_ssa, entry_proof, mir_state, native, world
from .common import Program, Unsupported

MODULE = "SolarTV.MIR.Transforms.StorageInduction"


def capture_region(body, *, pointer):
    """Cut the search preheader and common continuation, then alpha-rename."""
    blocks = body["blocks"]
    require = builtin_lowering.require

    def jump(index):
        term = blocks[index]["terminator"]
        require(term["kind"] == "jump", "storage loop expected an unconditional edge")
        return term["target"]

    def branch(index):
        term = blocks[index]["terminator"]
        require(term["kind"] == "branch", "storage loop expected a branch")
        return term["then"], term["else"]

    def predecessor(index, exclude):
        [pred] = [
            i
            for i, block in enumerate(blocks)
            if i != exclude and index in builtin_lowering.targets(block["terminator"])
        ]
        return pred

    def operands(index, instruction, name):
        op = blocks[index]["instructions"][instruction]["operation"]
        require(op.get("name") == name, f"storage loop expected {name}")
        return op["args"]

    [inspect] = [
        i
        for i, block in enumerate(blocks)
        if block["terminator"]["kind"] == "branch"
        and any(
            inst["operation"].get("name") == "sload" for inst in block["instructions"]
        )
        and block["instructions"][-1]["operation"].get("name") == "eq"
    ]
    hit, latch = branch(inspect)
    head = jump(latch)
    check, resume = branch(head)
    require(check == inspect, "storage search branch has a different destination")
    preheader = predecessor(head, latch)
    shift = jump(hit)
    prep, clear = branch(shift)
    panic, move = branch(prep)
    require(
        jump(move) == shift and jump(clear) == resume,
        "storage shift branch has a different destination",
    )
    order = [
        preheader,
        head,
        inspect,
        latch,
        hit,
        shift,
        prep,
        move,
        clear,
        panic,
        resume,
    ]
    require(
        len(set(order)) == len(order) and 0 not in order, "invalid storage loop cut"
    )

    # The only values entering the cut are the mapping base and search key.
    base = (
        operands(preheader, 0, "add")[0] if pointer else operands(inspect, 1, "add")[1]
    )
    needle = operands(inspect, -1, "eq")[1]
    require(
        base["kind"] == needle["kind"] == "local"
        and base["type"] == needle["type"] == builtin_lowering.word()
        and base != needle,
        "storage loop live-in shape changed",
    )
    labels = {old: new for new, old in enumerate(order)}
    locals_ = {
        inst["result"]: index
        for index, inst in enumerate(
            inst
            for i in order[:-1]
            for inst in (*blocks[i]["phis"], *blocks[i]["instructions"])
            if inst["result"] is not None
        )
    }
    require(
        base["index"] not in locals_ and needle["index"] not in locals_,
        "storage region does not start with two live-ins",
    )

    def rename(value):
        if isinstance(value, list):
            return [rename(item) for item in value]
        if not isinstance(value, dict):
            return value
        if value == base:
            return {"kind": "arg", "index": 0, "type": base["type"]}
        if value == needle:
            return {"kind": "arg", "index": 1, "type": needle["type"]}
        result = {key: rename(item) for key, item in value.items()}
        if result.get("kind") == "local":
            result["index"] = locals_[result["index"]]
        if result.get("result") is not None:
            result["result"] = locals_[result["result"]]
        if "incoming" in result:
            result["incoming"] = sorted(
                [labels[pred], value] for pred, value in result["incoming"]
            )
        for key in ("target", "then", "else"):
            if key in result:
                result[key] = labels[result[key]]
        return result

    # The dead temporary on the clear edge still runs. Its layout entry is
    # keyed by an instruction ID that is different in the original two bodies.
    alloc = blocks[clear]["instructions"][0]["operation"]
    require(
        alloc["kind"] == "deferred_allocate", "storage loop lost the clear allocation"
    )
    return [rename(blocks[i]) for i in order[:-1]], alloc["site"]


def check(paths, solar, executable, version, timeout, function, *, replay, verbose):
    from .check import build_model

    if function != "remove":
        raise Unsupported("no storage-loop certificate for the selected function")
    texts, models, _ = builtin_lowering.load_pair(
        paths, solar, executable, version, timeout
    )
    try:
        cuts = [
            capture_region(builtin_lowering.functions(model)[function], pointer=pointer)
            for pointer, model in zip((False, True), models, strict=True)
        ]
        recovery = cuts[1][0][4]["instructions"][1]["operation"]
        amount = recovery["args"][0]
        if recovery.get("name") != "shr" or amount["kind"] != "constant":
            raise Unsupported("storage loop has no constant recovery shift")
        recovery_shift = mir_state.literal(amount["value"])
    except Unsupported:
        raise
    except (IndexError, KeyError, ValueError) as error:
        raise Unsupported(
            "not the supported storage search and shift region"
        ) from error
    catalog = {row["name"]: row for row in native.catalog(executable, timeout)["evmir"]}
    regions = []
    for blocks, _ in cuts:
        for block in blocks:
            for inst in block["instructions"]:
                op = inst["operation"]
                if (
                    op["kind"] == "opcode"
                    and not catalog[op["name"]]["forks"][version]["available"]
                ):
                    raise Unsupported("storage loop uses an unavailable operation")
        regions.append(
            "[" + ",".join(mir_state.block(block, catalog) for block in blocks) + "]"
        )
    source = f"""import {MODULE}
open SolarTV EvmRules MIR.StorageInduction
set_option maxRecDepth 8192
def before : List MIR.State.Block := {regions[0]}
def after : List MIR.State.Block := {regions[1]}
theorem captured :
    before = indexBlocks {cuts[0][1]} ∧
    after = pointerBlocks {cuts[1][1]} {recovery_shift} := ⟨rfl,rfl⟩
#print axioms captured
"""
    if recovery_shift == "1#256":
        source += """
theorem contextual (base needle : Word) :
    let left := indexedSearch base needle 6 0
    let right := pointerSearch base needle 6 base
    left = right ∧ completes left :=
  ⟨correspondence base needle, searchCompletes base needle 0 6 (by decide)⟩
#print axioms contextual
"""
    entry_proof.success(build_model(timeout, MODULE), MODULE)
    with tempfile.TemporaryDirectory(prefix="solar-tv-storage-induction-") as folder:
        folder = Path(folder)
        proof = entry_proof.lean(folder, "Pair", source, timeout)
        if proof.returncode:
            raise Unsupported(
                "storage region does not match the typed search and shift templates"
            )
        entry_proof.check_axioms(
            proof.stdout.splitlines(), 2 if recovery_shift == "1#256" else 1
        )
        if recovery_shift != "1#256":
            return replay_handoff(
                folder,
                texts,
                cuts,
                solar,
                executable,
                version,
                timeout,
                replay,
                verbose,
            )
    pair = tuple(
        Program(
            (function, "storage-induction"),
            [],
            "",
            "",
            canonical_ir=text,
            replay_ir=text,
            selector=function,
            scope="storage-induction",
            evm_version=version,
            import_evidence={
                "kind": "kernel-checked-storage-induction",
                "source_sha256": hashlib.sha256(
                    dead_ssa.stable(blocks).encode()
                ).hexdigest(),
                "boundary": (
                    "Unpriced typed CFG projection from the search preheader to the packing "
                    "continuation or common failure. Equal base, needle, world, memory and "
                    "deferred allocation placement. Ordered SLOAD and SSTORE slots and words."
                ),
            },
        )
        for text, (blocks, _) in zip(texts, cuts, strict=True)
    )
    return audit.Result("verified", "verified\n", program=pair[0], pair=pair)


def call_data(key, needle):
    return "0x9dfa4e94" + f"{key:064x}{needle:064x}"


def call_world(folder, cuts, solar, needle):
    # Use the same deferred allocation placement even though the two site IDs
    # differ. The returned pointer is dead in both captures.
    fixture = {
        "version": 1,
        "fuel": 1024,
        "calldata": call_data(7, needle),
        "mir_layout": {
            "allocations": {
                "remove": {
                    str(site): {"kind": "static", "address": 1024, "size": 64}
                    for _, site in cuts
                }
            }
        },
    }
    path = folder / "storage-loop-world.json"
    path.write_text(json.dumps(fixture))
    return world.read(path, solar, None, kind="mir")


def trace_pair(folder, texts, fixture, solar, executable, version, timeout):
    from . import traced_replay

    return traced_replay.trace_pair(
        folder,
        texts,
        fixture,
        solar,
        executable,
        version,
        timeout,
        "remove",
        stores=False,
    )


def replay_handoff(
    folder, texts, cuts, solar, executable, version, timeout, replay, verbose
):
    from . import report

    fixture = call_world(folder, cuts, solar, 3)
    pair, observations, native_observations, traces = trace_pair(
        folder, texts, fixture, solar, executable, version, timeout
    )
    writes = [[event for event in trace if event[0] == 0x55] for trace in traces]
    first = next(
        (
            (index, old, new)
            for index, (old, new) in enumerate(zip(*writes, strict=False))
            if old != new
        ),
        None,
    )
    if first is None or observations[0] == observations[1]:
        raise Unsupported("storage loop certificate did not close")
    index, before, after = first
    return audit.Result(
        "counterexample",
        report.counterexample(
            *pair,
            [],
            *(json.dumps(item) for item in observations),
            replay,
            verbose=verbose,
            native_checked=True,
        )
        + f"First differing SSTORE (#{index + 1}): slot {before[1]} = {before[2]}"
        f" vs slot {after[1]} = {after[2]}\n",
        program=pair[0],
        pair=pair,
        witness={
            "inputs": {},
            "calldata": call_data(7, 3),
            "observations": observations,
            "native_observations": native_observations,
            "first_different_write": {"index": index, "before": before, "after": after},
        },
    )
