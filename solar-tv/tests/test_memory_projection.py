"""Bind projected memory operations to emitted bytes and independently replay them."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import check, entry_proof, native, world
from validator.call_budget import decode_runtime
from validator.common import Program

MODULE = "SolarTV.EVMIR.MemoryProjection"
WRITES = "SolarTV.EVMIR.MemoryWrites"
SPILL = "SolarTV.EVMIR.Transforms.SpilledReturn"
VERSION = "cancun"
VALUE = int.from_bytes(bytes(range(1, 33)), "big")


class MemoryProjectionTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.folder = Path(temporary.name)
        self.replay = native.default(SOLAR)
        self.catalog = native.catalog(self.replay, 30)

    def compile(self, offset):
        text = (
            "@module ProjectedLoad\nbb0 [stack=0]:\n"
            "  push 165\n  push 1024\n  mstore8\n"
            f"  push {VALUE}\n  push {offset}\n  mstore\n"
            f"  push {offset}\n  mload\n"
            "  push 0\n  mstore\n  msize\n  push 32\n  mstore\n"
            "  push 64\n  push 0\n  return\n"
        )
        image, decoded = self.compile_runtime(text)
        (pc,) = [pc for pc, (name, _) in decoded.items() if name == "mload"]
        return text, image, decoded, pc

    def compile_runtime(self, text):
        path = self.folder / "memory.evmir"
        path.write_text(text)
        result = subprocess.check_output(
            [
                SOLAR,
                path,
                "-j1",
                "-Zevm-ir-pipeline=none",
                f"--evm-version={VERSION}",
                "--emit=bin-runtime",
            ],
            text=True,
            timeout=30,
        )
        (contract,) = json.loads(result)["contracts"].values()
        image = bytes.fromhex(contract["bin-runtime"])
        definitions = {row["name"]: row for row in self.catalog["evmir"]}
        return image, decode_runtime(image, definitions)

    def proof_source(self, image, pc, offset, limit):
        return f"""import {MODULE}
open SolarTV EvmRules
set_option autoImplicit false
set_option Elab.async false
def image : List Execution.Byte := {list(image)}
def cfg : EVM.State.Config := ⟨.{VERSION}, {world.opcode_rows(self.catalog, VERSION)}, 0, {{}}, none⟩
def offset : Word := {offset}
def limit : Nat := {limit}
theorem inRange : offset.toNat + 32 ≤ limit := by decide +kernel
theorem price : EVM.State.available cfg 0x51 = ⟨true,3,0,1,1⟩ := by decide +kernel
theorem fetched : image[{pc}]? = some (0x51 : Execution.Byte) := by decide +kernel
theorem emittedLoad (journal : Execution.Journal) (frame : EVM.State.Frame) (stack : EVM.Stack)
    (same : frame.code = image) (positioned : frame.pc = {pc})
    (words : frame.stack = offset :: stack) (fits : (offset :: stack).length ≤ 1024) :
    EVM.State.instruction cfg journal frame =
      EVM.State.withMemory journal frame 3 offset.toNat 32 (fun grown =>
        EVM.State.next journal grown
          (Memory.load (Memory.Projection.project limit grown.memory) offset.toNat :: stack)) := by
  exact Memory.Projection.load_instruction limit cfg journal frame offset stack 3 0 price
    (by simpa only [same, positioned] using fetched) words inRange fits
#print axioms emittedLoad
"""

    def observe(self, text, image, *, gas=100000, probe_pc=None):
        fixture = self.folder / "world.json"
        fixture.write_text(json.dumps({"version": 1, "gas": gas}))
        program = Program(
            ("world-entry", "evmir"),
            [],
            "",
            "",
            world=world.with_runtime(world.read(fixture, SOLAR, "exact"), image),
            evm_version=VERSION,
            replay_ir=text,
            gas="exact",
        )
        replay = native.replay(
            program,
            program,
            [],
            raw_runtime=True,
            probe_pc=probe_pc,
            executable=self.replay,
            timeout=30,
        )
        return json.loads(replay["observations"][0])

    def test_emitted_load_projection_and_changed_address(self):
        entry_proof.success(check.build_model(60, MODULE), "memory projection build")
        # Offset, prefix, active bytes, gas used, changed active bytes, extra gas.
        cases = (
            (0, 256, 1056, 141, 1056, 1),
            (1, 256, 1056, 143, 1056, 0),
            (224, 256, 1056, 143, 1056, 0),
            (1024, 1056, 1056, 143, 1088, 3),
            (65504, 65536, 65536, 14378, 65568, 11),
        )
        for offset, limit, active, charge, changed_active, extra in cases:
            with self.subTest(offset=offset):
                text, image, decoded, pc = self.compile(offset)
                source = self.proof_source(image, pc, offset, limit)
                proved = entry_proof.lean(self.folder, "EmittedLoad", source, 30)
                entry_proof.success(proved, "emitted MLOAD correspondence")
                entry_proof.check_axioms(proved.stdout.splitlines(), 1)

                # The prefix grows full memory beyond the local projection.
                actual = self.observe(text, image)
                self.assertEqual(actual[0], "success")
                self.assertEqual(
                    bytes(actual[1]),
                    VALUE.to_bytes(32, "big") + active.to_bytes(32, "big"),
                )
                # Expansion to 33 words costs 101 gas, and to 2048 costs 14336.
                # Other instructions cost 40 with PUSH0 at both offsets, or 42.
                self.assertEqual(actual[2], 100000 - charge)

                push_pc = next(reversed([start for start in decoded if start < pc]))
                name, _ = decoded[push_pc]
                if name == "push0":
                    changed = image[:push_pc] + bytes([0x60, 1]) + image[push_pc + 1 :]
                else:
                    width = pc - push_pc - 1
                    changed = (
                        image[: push_pc + 1]
                        + (offset + 1).to_bytes(width, "big")
                        + image[pc:]
                    )
                observed = self.observe(text, changed)
                self.assertEqual(observed[0], "success")
                staged = bytearray(changed_active)
                staged[1024] = 165
                staged[offset : offset + 32] = VALUE.to_bytes(32, "big")
                self.assertEqual(
                    bytes(observed[1]),
                    staged[offset + 1 : offset + 33]
                    + changed_active.to_bytes(32, "big"),
                )
                self.assertNotEqual(observed[1], actual[1])
                self.assertEqual(observed[2], actual[2] - extra)
                if offset + 33 > limit:
                    outside = entry_proof.lean(
                        self.folder,
                        "OutsideProjection",
                        self.proof_source(changed, pc, offset + 1, limit),
                        30,
                    )
                    self.assertNotEqual(outside.returncode, 0)

                wrong_opcode = image[:pc] + bytes([0x50]) + image[pc + 1 :]
                rejected = entry_proof.lean(
                    self.folder,
                    "WrongLoad",
                    self.proof_source(wrong_opcode, pc, offset, limit),
                    30,
                )
                self.assertNotEqual(rejected.returncode, 0)
                self.assertEqual(self.observe(text, wrong_opcode)[0], "exception")

    def write_source(self, image, points):
        store, byte, copy = points
        return f"""import {WRITES}
open SolarTV EvmRules
set_option autoImplicit false
set_option Elab.async false
def image : List Execution.Byte := {list(image)}
def cfg : EVM.State.Config := ⟨.{VERSION}, {world.opcode_rows(self.catalog, VERSION)}, 0, {{}}, none⟩

set_option linter.defProp false in
def emittedStore (isByte : Bool) (journal : Execution.Journal) (frame : EVM.State.Frame)
    (grown : List Execution.Byte) (offset value : Word) (stack : EVM.Stack)
    (same : frame.code = image) (positioned : frame.pc = if isByte then {byte} else {store})
    (words : frame.stack = offset :: value :: stack) (fits : (offset :: value :: stack).length ≤ 1024)
    (aligned : frame.memory.length % 32 = 0) (bounded : frame.memory.length ≤ 65536)
    (within : offset.toNat + (if isByte then 1 else 32) ≤ 65)
    (expanded : Execution.expand frame.memory offset.toNat (if isByte then 1 else 32) = .ok grown) :=
  Memory.Projection.store_frame 65 cfg journal frame grown offset value stack isByte 3 0
    (by cases isByte <;> decide +kernel) (by rw [same, positioned]; cases isByte <;> decide +kernel)
    words fits aligned bounded within expanded

set_option linter.defProp false in
def emittedCopy (journal : Execution.Journal) (frame : EVM.State.Frame)
    (first grown : List Execution.Byte) (destination source count : Word) (stack : EVM.Stack)
    (same : frame.code = image) (positioned : frame.pc = {copy})
    (words : frame.stack = destination :: source :: count :: stack)
    (fits : (destination :: source :: count :: stack).length ≤ 1024)
    (aligned : frame.memory.length % 32 = 0) (bounded : frame.memory.length ≤ 65536)
    (within : count.toNat = 0 ∨ max destination.toNat source.toNat + count.toNat ≤ 65)
    (firstGrowth : Execution.expand frame.memory destination.toNat count.toNat = .ok first)
    (lastGrowth : Execution.expand first source.toNat count.toNat = .ok grown) :=
  Memory.Projection.copy_frame 65 cfg journal frame first grown destination source count stack 3 3
    (by decide +kernel) (by rw [same, positioned]; decide +kernel) words fits aligned bounded
    within firstGrowth lastGrowth

#print axioms emittedStore
#print axioms emittedCopy
"""

    def test_emitted_stores_and_copy_at_exact_gas(self):
        entry_proof.success(check.build_model(60, WRITES), "memory writes build")
        maximum = 2**256 - 1
        for destination, source, size in (
            (30, 28, 35),
            (28, 30, 35),
            (maximum, maximum, 0),
        ):
            with self.subTest(destination=destination, source=source, size=size):
                text = (
                    "@module ProjectedWrites\nbb0 [stack=0]:\n"
                    f"  push {VALUE}\n  push 28\n  mstore\n"
                    "  push 427\n  push 31\n  mstore8\n"
                    f"  push {size}\n  push {source}\n  push {destination}\n  mcopy\n"
                    "  push 0\n  pop\n  stop\n"
                )
                image, decoded = self.compile_runtime(text)
                points = tuple(
                    next(pc for pc, (name, _) in decoded.items() if name == operation)
                    for operation in ("mstore", "mstore8", "mcopy")
                )
                proof = entry_proof.lean(
                    self.folder, "EmittedWrites", self.write_source(image, points), 30
                )
                entry_proof.success(proof, "emitted memory write correspondence")
                entry_proof.check_axioms(proof.stdout.splitlines(), 2)

                expected = bytearray(96 if size else 64)
                expected[28:60] = VALUE.to_bytes(32, "big")
                expected[31] = 0xAB
                if size:
                    expected[destination : destination + size] = expected[
                        source : source + size
                    ]
                # The stores and pushes cost 33 gas (32 for a zero count).
                # The nonempty copy costs 3 base, 6 for two copied words and
                # 3 for expanding the longer of the two ranges by one word.
                charge = 45 if size else 35
                copy = points[-1]
                paid = self.observe(text, image, gas=charge, probe_pc=copy + 1)
                self.assertEqual(
                    (paid["stack"], paid["gas"], bytes(paid["memory"])),
                    ([], 0, expected),
                )
                self.assertEqual(
                    self.observe(text, image, gas=charge - 1)[0], "out-of-gas"
                )

                # Preserve the code length and copy operands, then change
                # only the opcode so the original binding must fail.
                altered = image[:copy] + b"\x50" + image[copy + 1 :]
                rejected = entry_proof.lean(
                    self.folder, "WrongCopy", self.write_source(altered, points), 30
                )
                self.assertNotEqual(rejected.returncode, 0)
                if size:
                    changed = self.observe(text, altered, gas=charge, probe_pc=copy + 1)
                    self.assertEqual(changed["stack"], [str(source), str(size)])
                    self.assertEqual(len(changed["memory"]), 64)

    def test_emitted_spilled_return(self):
        text = """@module Spill
bb0 [stack=0]:
  push bb1
  push 128
  mstore
  push 65535
  push 0
  mstore
  jump bb2
bb1 [stack=0]:
  push 195
  push 0
  mstore
  push 32
  push 0
  return
bb2 [stack=0]:
  push 128
  mload
  jump
"""
        image, decoded = self.compile_runtime(text)
        (load,) = [pc for pc, (op, _) in decoded.items() if op == "mload"]
        store = next(pc for pc, (op, _) in decoded.items() if op == "mstore")
        destination = decoded[0][1]
        count = 100_000
        before = self.observe(text, image, gas=count, probe_pc=load)
        after = self.observe(text, image, gas=count, probe_pc=destination)
        prefix = count - after["gas"]
        self.assertEqual(before["stack"], ["128"])
        self.assertEqual(before["gas"] - after["gas"], 11)
        self.assertEqual(after["stack"], [])
        self.assertEqual(int.from_bytes(bytes(before["memory"][128:160])), destination)
        self.assertEqual(
            self.observe(text, image, gas=prefix, probe_pc=destination)["gas"], 0
        )
        self.assertEqual(self.observe(text, image, gas=prefix - 1)[0], "out-of-gas")
        self.assertEqual(self.observe(text, image)[:2], ["success", [0] * 31 + [195]])

        entry_proof.success(check.build_model(60, SPILL), "spill model build")
        changed = image[: load - 1] + b"\0" + image[load:]
        for suffix, artifact in (("Original", image), ("WrongSlot", changed)):
            source = f"""import {SPILL}
open SolarTV EvmRules Execution EVM.State EVM.SpilledReturn
set_option autoImplicit false
set_option Elab.async false
def image : List Byte := {list(artifact)}
def cfg : Config := ⟨.{VERSION}, {world.opcode_rows(self.catalog, VERSION)}, 0, {{}}, none⟩
def destination : Word := {destination}
theorem emittedSave (world : Journal) (frame : Frame) (grown : List Byte) (tail : EVM.Stack)
    (imageEq : frame.code = image) (pc : frame.pc = {store})
    (stack : frame.stack = 128 :: destination :: tail) (height : tail.length + 2 ≤ 1024)
    (arena : expand frame.memory (128 : Word).toNat 32 = .ok grown) :
    let charge := 3 + memoryCharge frame.memory (128 : Word).toNat 32
    let memory := writeBytes grown (128 : Word).toNat (wordBytes destination)
    instruction cfg world frame =
      (if frame.gas < charge then stepExhausted world
       else .ok (.next world {{frame with pc := frame.pc+1, stack := tail, memory, gas := frame.gas-charge}})) ∧
      saved memory 128 destination := by
  exact EVM.SpilledReturn.store cfg world frame grown 128 destination tail 3 0
    (by decide +kernel) (by rw [imageEq, pc]; decide +kernel) stack (by simp; omega) arena
theorem emittedResume (world : Journal) (frame : Frame) (memory written grown : List Byte)
    (tail : EVM.Stack) (value : Word)
    (imageEq : frame.code = image) (pc : frame.pc = {load - 2})
    (stack : frame.stack = tail) (height : tail.length + 1 ≤ 1024)
    (aligned : frame.memory.length % 32 = 0)
    (writeArena : expand memory 0 32 = .ok written)
    (write : frame.memory = writeBytes written 0 (wordBytes value))
    (readArena : expand frame.memory 128 32 = .ok grown)
    (stored : saved memory 128 destination) :
    MeteredPrefix cfg world frame world {{frame with pc := destination.toNat, stack := tail, memory := grown}}
      3 (3 + (3 + memoryCharge frame.memory 128 32 + 8)) := by
  let loadedSlot : Frame := {{frame with pc := {load}, stack := (128 : Word) :: tail}}
  have pushOpcode : (image[{load - 2}]?.getD 0 : Byte) = 0x60 := by decide +kernel
  have pushValueEq : pushValue image {load - 2} 1 = 128 := by decide +kernel
  have inImage : ¬image.length ≤ {load - 2} := by decide +kernel
  have pushSchema : available cfg 0x60 = ⟨true,3,0,0,1⟩ := by decide +kernel
  have stackRoom : ¬tail.length + 1 > 1024 := by omega
  have pushStep (gas : Nat) : instruction cfg world {{frame with gas}} =
      if gas < 3 then stepExhausted world
      else .ok (.next world {{loadedSlot with gas := gas-3}}) := by
    change instructionWithImage cfg world {{frame with gas}} frame.code.length
      (frame.code[frame.pc]?.getD 0) (pushValue frame.code frame.pc) (validJump frame.code) = _
    rw [imageEq, pc, pushOpcode]
    simp [instructionWithImage, loadedSlot, stack, inImage, pushSchema, pushValueEq, pureOp, stackRoom]
  have restored : MeteredPrefix cfg world loadedSlot world
      {{frame with pc := destination.toNat, stack := tail, memory := grown}}
      2 (3 + memoryCharge frame.memory 128 32 + 8) := by
    apply EVM.SpilledReturn.resume cfg world loadedSlot grown 128 destination tail 3 8 0 0
    · decide +kernel
    · decide +kernel
    · change frame.code[{load}]? = some 0x51
      rw [imageEq]; decide +kernel
    · change frame.code[{load + 1}]? = some 0x56
      rw [imageEq]; decide +kernel
    · change validJump frame.code destination = true
      rw [imageEq]; decide +kernel
    · rfl
    · simpa using height
    · exact aligned
    · exact readArena
    · change saved frame.memory 128 destination
      rw [write]
      exact EVM.SpilledReturn.preserve memory written (wordBytes value) 128 destination 0 32
        writeArena (by simp) stored
  exact MeteredPrefix.step pushStep restored
#print axioms emittedSave
#print axioms emittedResume
"""
            proof = entry_proof.lean(self.folder, "Spilled" + suffix, source, 30)
            if artifact == image:
                entry_proof.success(proof, "emitted spilled return")
                entry_proof.check_axioms(proof.stdout.splitlines(), 2)
            else:
                self.assertNotEqual(proof.returncode, 0)
                self.assertEqual(self.observe(text, changed)[0], "exception")
