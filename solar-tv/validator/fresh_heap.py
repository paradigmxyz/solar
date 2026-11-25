"""Direct SSA proofs with an explicit fresh-heap producer obligation.

The source allocation determines the contract's extent. Neither an arbitrary
heap floor nor agreement with the live FMP is inferred. The complete imported
functions execute in Lean: the suffix is not summarized, and two model errors
cannot establish equivalence. This route is conditional on the named placement
and FMP relation, not proof that every caller or backend establishes it.
"""

import hashlib
import json
import tempfile
from dataclasses import replace
from pathlib import Path

from . import audit, native
from .common import Unsupported


def extent(a, b):
    models = [p.symbolic_mir["model"] for p in (a, b)]
    bodies = []
    for model in models:
        if model["params"] or len(model["functions"]) != 1:
            raise Unsupported("fresh-heap proof requires one parameterless body")
        body = model["functions"][0]
        if (
            body["id"] != model["entry_id"]
            or len(body["blocks"]) != 1
            or body["blocks"][0]["phis"]
            or len(body["blocks"][0]["instructions"]) > 64
        ):
            raise Unsupported("fresh-heap proof requires one bounded, phi-free block")
        bodies.append(body["blocks"][0]["instructions"])
    before, after = bodies
    if not before or len(after) < 4:
        raise Unsupported("fresh-heap proof requires an initial reservation")
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

    op = before[0]["operation"]
    args = op.get("args", [])
    if (
        len(args) != 1
        or args[0].get("kind") != "constant"
        or args[0].get("type") != word
    ):
        raise Unsupported("fresh-heap proof requires a constant source extent")
    size = int(args[0]["value"], 0)
    # A 128-byte floor must leave at least one supported frame: do not accept a
    # candidate by generating a contract with an impossible memory-work range.
    if (
        not 0 < size <= 65536 - 128
        or size % 32
        or before[0]["type"] != pointer
        or before[0]["result"] is None
        or op
        != {
            "kind": "allocate",
            "zeroed": False,
            "checked": False,
            "args": [constant(size)],
        }
    ):
        raise Unsupported(
            "fresh-heap proof requires an infallible uninitialized allocation"
        )
    query, cast, add, store = after[:4]
    if (
        [i["type"] for i in after[:4]] != [word, pointer, word, None]
        or any(i["result"] is None for i in after[:3])
        or store["result"] is not None
        or query["operation"]
        != {
            "kind": "layout",
            "query": "heap_floor",
            "function": models[0]["name"],
            "args": [],
        }
        or cast["operation"] != {"kind": "word", "args": [local(query)]}
        or add["operation"]
        != {"kind": "opcode", "name": "add", "args": [local(query), constant(size)]}
        or store["operation"]
        != {"kind": "opcode", "name": "mstore", "args": [constant(64), local(add)]}
    ):
        raise Unsupported("fresh-heap proof requires the exact named floor reservation")
    return size


def check(a, b, timeout, executable):
    from . import mir_symbolic
    from .check import PRELUDE, ROOT, build_model, run

    size = extent(a, b)
    available = [
        row["opcode"]
        for row in native.catalog(executable, timeout)["evmir"]
        if row["forks"][a.evm_version]["available"]
    ]
    name = json.dumps(a.selector)
    # These are the same target and internal-entry mode as the direct importer.
    # All other execution context, including the supplied layout, stays arbitrary.
    code = PRELUDE + "set_option maxRecDepth 16384\nset_option maxHeartbeats 2000000\n"
    code += f"""
def config (cfg : MIR.State.Config) : MIR.State.Config :=
 {{cfg with fork := .{a.evm_version},available := {available},callDepth := 128,entry := false}}
"""
    for side, program in zip(("before", "after"), (a, b), strict=True):
        model = program.symbolic_mir
        code += f"""
def {side}Code : MIR.State.Functions := {model["code"]}
def {side} (cfg : MIR.State.Config) (frame : MIR.State.Frame) : MIR.State.Run :=
 MIR.State.execute {model["fuel"]} (config cfg) {side}Code {model["model"]["entry_id"]} {{frame with arguments := [],locals := []}}
"""
    unfold = mir_symbolic.step_definitions()
    for term in ("MIR.State.allocate", "MIR.State.placementBinding"):
        unfold = unfold.replace(term + ", ", "")
    # Preserve byte-memory expressions across the two executions. The range
    # laws prove that expansion succeeds without unfolding the memory bytes.
    unfold = unfold.replace(
        "Execution.expand, Execution.requiredWords, Execution.memoryWords, ", ""
    ).replace("↓MIR.State.reduceRun,", "↓MIR.State.reduceShapedRun,")
    unfold += (
        ", config, MIR.State.conclude, BitVec.ofNat_add, reserve, query, slot, floorOffset, floorMod"
        ", MIR.ContinuedCounter.expandAligned"
        ", ↓MIR.State.reduceOpcode"
    )
    # Failed side-condition proofs leave the operation opaque for later splits.
    discharge = (
        "try (first | omega | (simp (disch := try omega) only "
        "[Execution.writeBytes_length, MIR.ContinuedCounter.padLength,"
        " MIR.ContinuedCounter.maxAligned, Nat.max_le] <;> omega))"
    )
    code += f"""
attribute [local irreducible] Execution.readWord Execution.readBytes
  Execution.writeBytes Execution.wordBytes Execution.expand

theorem pair_correct (cfg : MIR.State.Config) (frame : MIR.State.Frame) (floor : Word)
 (valid : MIR.FreshHeap.Contract frame floor {size})
 (binding : MIR.FreshHeap.Binding cfg.layout {name} floor) :
 MIR.State.equivalent (before cfg frame) (after cfg frame) := by
 have reserve (args locals) : MIR.State.allocate {{frame with arguments := args,locals := locals}} ({size}#256) false false =
   .ok (.returned (some floor,{{frame with arguments := args,locals := locals,memory := Execution.writeBytes frame.memory 64 (Execution.wordBytes (floor+{size}))}})) :=
   MIR.FreshHeap.reservation _ floor {size} valid
 have query (other : MIR.State.Frame) := MIR.FreshHeap.query (config cfg) other {name} floor binding
 simp only [config] at query
 have slot := MIR.FreshHeap.active_slot frame.memory valid.1.1 valid.1.2 valid.2.1
 rcases valid with ⟨⟨bounded,aligned⟩,active,cursor,lower,alignment,range⟩
 have floorOffset (offset : Word) (small : offset.toNat ≤ 65536) :
     (floor + offset).toNat = floor.toNat + offset.toNat := by
   simp [BitVec.toNat_add, Nat.mod_eq_of_lt (show floor.toNat + offset.toNat < 2^256 by omega)]
 have floorMod (offset : Nat) (small : offset ≤ 65536) :
     (floor.toNat + offset) % 2^256 = floor.toNat + offset :=
   Nat.mod_eq_of_lt (by omega)
 have reservedBounded : (Execution.writeBytes frame.memory 64 (Execution.wordBytes (floor+{size}))).length ≤ 65536 := by
   simpa using bounded
 have reservedAligned : (Execution.writeBytes frame.memory 64 (Execution.wordBytes (floor+{size}))).length % 32 = 0 := by
   simpa using aligned
 unfold before after
 conv => arg 1; simp (disch := {discharge}) [{unfold}]
 conv => arg 2; simp (disch := {discharge}) [{unfold}]
 repeat (split_mir_guard <;>
   (try simp_all [{unfold}, Nat.add_comm]) <;>
   (try simp (disch := {discharge}) [*, {unfold}, Nat.add_comm]) <;> try omega)
 all_goals first | rfl | (solve | simp [MIR.State.equivalent, MIR.State.observe, BitVec.ofNat_add])
#print axioms pair_correct
"""
    built = build_model(timeout)
    if built.returncode:
        raise Unsupported(built.stdout + built.stderr)
    with tempfile.TemporaryDirectory(prefix="solar-tv-fresh-heap-") as folder:
        path = Path(folder) / "Pair.lean"
        path.write_text(code)
        result = run(["lake", "env", "lean", path], ROOT, timeout)
    if not mir_symbolic.proved(result):
        raise Unsupported(
            "fresh-heap SSA proof did not close\n" + result.stdout + result.stderr
        )
    programs = tuple(
        replace(
            program,
            scope="fresh-heap-function",
            import_evidence={
                **program.import_evidence,
                "kind": "independent-typed-fresh-heap",
                "reserved_bytes": size,
                "contract": "named heap floor equals live FMP; active aligned memory; aligned floor >= 128; source extent fits the 64 KiB work domain",
                "proof_sha256": hashlib.sha256(code.encode()).hexdigest(),
                "boundary": "trusted compiler parser/schema export and typed SSA constructor renderer; direct complete-body execution under an explicit placement/FMP producer condition; no proof that callers or backend establish that condition; no physical gas claim",
            },
        )
        for program in (a, b)
    )
    return audit.Result(
        "verified", stdout="verified\n", program=programs[0], pair=programs
    )
