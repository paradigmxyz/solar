"""Prove a static MIR ABI interface change against the direct interpreter."""

import re
import tempfile
from dataclasses import replace
from pathlib import Path

from . import audit, entry_proof, mir_state, mir_symbolic, prune, type_prune
from .common import Unsupported


def width(ty):
    widths = {"address": 160, "bool": 1, "bytes32": 256}
    if ty in widths:
        return widths[ty]
    if re.fullmatch(r"uint[0-9]{1,3}", ty):
        n = int(ty[4:])
        if 8 <= n <= 256 and n % 8 == 0:
            return n
    raise Unsupported("ABI certificate requires unsigned static words")


def layout(header, attr, typed, *, arguments):
    match = re.search(rf"\b{attr}=\[([^\]]*)\]", header)
    if not match:
        raise Unsupported("implicit ABI layout is missing")
    types = match[1].split(", ") if match[1] else []
    if len(types) != len(typed) or len(types) > 4:
        raise Unsupported("ABI certificate needs one static word per MIR value")
    for ty, value in zip(types, typed, strict=True):
        bits = width(ty)
        if value["kind"] != "word" or (
            value["bits"] < bits if arguments else value["bits"] > bits
        ):
            raise Unsupported("ABI certificate cannot bind the implicit word width")
    return types


def check(programs, function, replay, version, timeout):
    from .check import PRELUDE, ROOT, build_model, run

    texts = [program.replay_ir for program in programs]
    old, new = [
        mir_symbolic.translate(text, function, replay, version, timeout)
        for text in texts
    ]
    before, after = [program.symbolic_mir["model"] for program in (old, new)]
    if after["params"] or after["returns"]:
        raise Unsupported("static ABI certificate requires an argument-free wrapper")
    header = prune.declarations(texts[0])[1][function]["text"][0]
    params = layout(header, "abi_params", before["params"], arguments=True)
    leaves = [leaf for ty in before["returns"] for leaf in mir_state.leaf_types(ty)]
    types = layout(header, "abi_return_params", leaves, arguments=False)
    source, target = [type_prune.export(text, replay, timeout) for text in texts]
    if any(
        module["dispatch_entry"] is not None or module["library"]
        for module in (source, target)
    ):
        raise Unsupported("ABI certificate requires undispatched contract entries")

    def route(module, body):
        return next(
            (r for r in module["selectors"] if r["id"] == body["entry_id"]), None
        )

    left, right = route(source, before), route(target, after)
    if (
        not left
        or not right
        or left["wrapper"]
        or right != {**left, "wrapper": True, "id": right["id"]}
    ):
        raise Unsupported("ABI certificate requires the same selected function route")
    routes = source["selectors"] + [
        source[role] for role in ("receive_route", "fallback_route") if source[role]
    ]
    # All-nonpayable modules check value at dispatch, outside this wrapper.
    local_guard = not left["payable"] and any(r["payable"] for r in routes)
    build = build_model(timeout)
    if build.returncode:
        raise Unsupported(build.stderr or build.stdout)
    definition = mir_symbolic.declaration("before", old) + mir_symbolic.declaration(
        "after", new
    )
    guard = "environment.value ≠ 0#256" if local_guard else "False"
    words = [
        f"Execution.readWord environment.input {4 + 32 * i}" for i in range(len(params))
    ]
    arguments = " ".join(
        f"(({word}).setWidth {bits})"
        for word, bits in zip(words, old.input_widths, strict=True)
    )
    short = f"environment.input.length < {4 + 32 * len(params)}" if params else "False"
    invalid = [
        f"{(1 << width(ty)) - 1} < ({word}).toNat"
        for ty, word in zip(params, words, strict=True)
        if width(ty) < 256
    ]
    splits = (["  all_goals by_cases short : " + short] if params else []) + [
        f"  all_goals by_cases bad{i} : {condition}"
        for i, condition in enumerate(invalid)
    ]
    hypotheses = ", ".join(
        (["short"] if params else []) + [f"bad{i}" for i in range(len(invalid))]
    )
    canonical = [
        f"  all_goals try (have fits{i} : ({word}).toNat < {1 << width(ty)} := by omega)"
        for i, (ty, word) in enumerate(zip(params, words, strict=True))
        if width(ty) < 256
    ]
    modulus = ", ".join(
        f"Nat.mod_eq_of_lt fits{i}" for i, ty in enumerate(params) if width(ty) < 256
    )
    rejection = " ∨ ".join([guard, short, *invalid])
    # This boundary changes the terminal observation, so reduce the two
    # result constructors directly after unfolding their respective runs.
    unfold = (
        mir_symbolic.step_definitions()
        .replace("↓MIR.State.reduceConclude, ", "")
        .replace(", ↓MIR.State.reduceEquivalent", "")
    )
    splits_by_length = "\n".join(
        f"""private theorem read{size} (memory : List World.Byte) (offset : Nat) :
    Execution.readBytes memory offset {size} = Execution.readBytes memory offset 32 ++
      Execution.readBytes memory (offset + 32) {size - 32} :=
  readSplit memory offset 32 {size - 32}"""
        for size in (64, 96, 128)
    )
    code = (
        PRELUDE
        + "set_option Elab.async false\nset_option maxHeartbeats 2000000\n"
        + definition
        + f"""
private theorem readWrittenWord (memory : List World.Byte) (offset : Nat) (word : Word)
    (within : offset + 32 ≤ memory.length) :
    Execution.readBytes (Execution.writeBytes memory offset (Execution.wordBytes word)) offset 32 =
      Execution.wordBytes word := by
  simpa using Execution.readBytes_writeBytes_same memory (Execution.wordBytes word) offset
    (by simpa using within)
private theorem readOtherWord (memory : List World.Byte) (dst src : Nat) (word : Word)
    (disjoint : dst + 32 ≤ src ∨ src + 32 ≤ dst) :
    Execution.readBytes (Execution.writeBytes memory dst (Execution.wordBytes word)) src 32 =
      Execution.readBytes memory src 32 := by
  exact Execution.readBytes_writeBytes_disjoint memory (Execution.wordBytes word) dst src 32
    (by simpa using disjoint)
private theorem readAppendedWord (memory suffix : List World.Byte) (offset : Nat)
    (within : offset + 32 ≤ memory.length) :
    Execution.readBytes (memory ++ suffix) offset 32 = Execution.readBytes memory offset 32 := by
  unfold Execution.readBytes
  apply List.map_congr_left
  intro i h
  have hi := List.mem_range.mp h
  have present : offset + i < memory.length := by omega
  simp [List.getElem?_append_left present]
private theorem widenWord {{bits : Nat}} (word : BitVec bits) :
    word.setWidth 256 = BitVec.ofNat 256 word.toNat := by
  apply BitVec.eq_of_toNat_eq
  simp [BitVec.toNat_setWidth]
private theorem wordOfMod (value : Nat) :
    BitVec.ofNat 256 (value % {1 << 256}) = BitVec.ofNat 256 value := by
  apply BitVec.eq_of_toNat_eq
  simpa only [BitVec.toNat_ofNat] using Nat.mod_mod value (2^256)
private theorem readSplit (memory : List World.Byte) (offset first rest : Nat) :
    Execution.readBytes memory offset (first + rest) =
      Execution.readBytes memory offset first ++ Execution.readBytes memory (offset + first) rest := by
  simp [Execution.readBytes, List.range_add, List.map_append, List.map_map, Function.comp_def,
    Nat.add_assoc]
{splits_by_length}
def initial (journal : Execution.Journal) : MIR.State.Frame :=
  {{world := journal, memory := [], arguments := []}}
def encoded (execution : MIR.State.Run) : MIR.State.Run := do
  let result ← execution
  match result.completion with
  | .returned values => return {{result with
      completion := .success (values.flatMap Execution.wordBytes)
      world := EVM.State.finalize result.world}}
  | _ => return result
def implicit (environment : World.Environment) (context : Execution.Block) (journal : Execution.Journal) : MIR.State.Run :=
  if {rejection} then .ok (MIR.State.finish (.reverted []) (initial journal))
  else encoded (before environment context false (initial journal) {arguments})
theorem wrapper (environment : World.Environment) (context : Execution.Block) (journal : Execution.Journal)
    (bounded : environment.input.length < 2^256) :
    MIR.State.equivalent (implicit environment context journal)
      (after environment context false (initial journal)) := by
  by_cases zero : environment.value = 0#256
{chr(10).join(splits)}
{chr(10).join(canonical)}
  all_goals simp [{unfold}, before, after, beforeCode, afterCode, implicit, encoded, initial, zero,
    {hypotheses + "," if hypotheses else ""}
    BitVec.toNat_ofNat, Nat.mod_eq_of_lt bounded,
    MIR.RoutedDispatch.callvalue, MIR.State.fixedSlot, MIR.State.conclude,
    MIR.State.equivalent, MIR.State.observe,
    Except.bind, EVM.State.finalize]
  all_goals try simp only [widenWord, BitVec.toNat_setWidth, wordOfMod,
    {modulus + "," if modulus else ""} Nat.mod_mod]
  all_goals try simp only [BitVec.ofNat_toNat, BitVec.setWidth_eq]
  all_goals try simp (disch := solve | simp | decide) [read64, read96, read128,
    readAppendedWord, readWrittenWord, readOtherWord, List.append_assoc]
  all_goals try decide +kernel
#print axioms wrapper
"""
    )
    with tempfile.TemporaryDirectory(prefix="solar-tv-abi-") as folder:
        path = Path(folder) / "Pair.lean"
        path.write_text(code)
        proof = run(["lake", "env", "lean", "-j1", path], ROOT, timeout)
    if proof.returncode:
        raise Unsupported(
            "static ABI proof did not close\n" + proof.stdout + proof.stderr
        )
    entry_proof.check_axioms(proof.stdout.splitlines(), 1)
    pair = tuple(
        replace(
            program,
            scope="static-abi-entry",
            import_evidence={
                **program.import_evidence,
                "kind": "kernel-checked-static-abi-entry",
                "selector": left["selector"],
                "unsigned_params": params,
                "unsigned_returns": types,
                "rejects_value_in_wrapper": local_guard,
            },
        )
        for program in (old, new)
    )
    return audit.Result("verified", stdout="verified\n", program=pair[0], pair=pair)
