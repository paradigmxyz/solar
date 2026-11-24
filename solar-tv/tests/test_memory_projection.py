"""Bind projected MLOAD to emitted bytes and replay changed reads independently."""

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
        path = self.folder / "load.evmir"
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
        decoded = decode_runtime(image, definitions)
        (pc,) = [pc for pc, (name, _) in decoded.items() if name == "mload"]
        return text, image, decoded, pc

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

    def observe(self, text, image):
        fixture = self.folder / "world.json"
        fixture.write_text(json.dumps({"version": 1, "gas": 100000}))
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
