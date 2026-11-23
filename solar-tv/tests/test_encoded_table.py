"""Check each assembler encoding against the actual deployed byte image."""

import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import native, world
from validator.call_budget import decode_runtime
from validator.common import Program

VERSION = "cancun"
SENTINEL = 71
FIRST_MARKER = 101
SWITCH = ROOT.parent / "tests/ui/codegen/lowering/evm-ir/switch/dense_table.sol"


class EncodedTableTests(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.directory = Path(folder.name)
        self.report = self.directory / "audit.json"
        self.hex_file = self.directory / "runtime.hex"
        self.replay = native.default(SOLAR)
        self.catalog = {
            row["name"]: row for row in native.catalog(self.replay, 30)["evmir"]
        }

    def compile_table(self, count, padding, flags, mask=None):
        labels = range(2, count + 2)
        # Calldata supplies the selector. Each target body identifies its own
        # PC independently of the packed operands or stride stubs.
        text = (
            f"@module Table\nbb0 [stack=0]:\n  push {SENTINEL}\n  push 0\n  calldataload\n"
            + (f"  push {mask}\n  and\n" if mask is not None else "")
            + "  indexed_jump "
            + ", ".join(f"bb{label}" for label in labels)
            + "\n"
        )
        if padding:
            text += (
                "bb1:\n"
                + "  push 0xf000000000000000000000000000000000000000000000000000000000000001\n"
                * padding
                + "  invalid\n"
            )
        text += "".join(
            f"bb{label}:\n  push {FIRST_MARKER + index}\n"
            + ("  invalid\n" if index == 1 else "  stop\n")
            for index, label in enumerate(labels)
        )
        path = self.directory / "table.evmir"
        path.write_text(text)
        output = subprocess.check_output(
            [
                SOLAR,
                path,
                "-j1",
                "-Zevm-ir-pipeline=none",
                "--emit=bin-runtime",
                "--allow=2264",
                f"--evm-version={VERSION}",
                *flags,
            ],
            timeout=30,
            text=True,
        )
        (contract,) = json.loads(output)["contracts"].values()
        image = bytes.fromhex(contract["bin-runtime"])
        decoded = decode_runtime(image, self.catalog)
        self.assertEqual(
            list(decoded.items())[:3],
            [(0, ("push1", SENTINEL)), (2, ("push0", 0)), (3, ("calldataload", None))],
        )
        labels_by_marker = {
            decoded[pc + 1][1]: pc
            for pc, row in decoded.items()
            if row[0] == "jumpdest"
            and decoded.get(pc + 1, (None,))[0] == "push1"
            and decoded[pc + 1][1] in range(FIRST_MARKER, FIRST_MARKER + count)
        }
        return (
            text,
            image,
            decoded,
            [labels_by_marker[FIRST_MARKER + index] for index in range(count)],
        )

    def cli(self, image, targets, jumps, *, entry=4, mask_pc=None, version=VERSION):
        self.hex_file.write_text(image.hex() + "\n")
        result = subprocess.run(
            [
                CLI,
                "encoded-table",
                self.hex_file,
                f"--pc={entry}",
                "--targets=" + ",".join(map(str, targets)),
                f"--jumps={jumps}",
                f"--replay={self.replay}",
                f"--evm-version={version}",
                "--timeout=30",
                f"--report={self.report}",
                *(() if mask_pc is None else (f"--mask-pc={mask_pc}",)),
            ],
            text=True,
            capture_output=True,
            timeout=60,
            check=False,
        )
        return result, json.loads(self.report.read_text())

    def program(self, text, image, selector, gas, *, prefix=b""):
        fixture = self.directory / "world.json"
        fixture.write_text(
            json.dumps(
                {
                    "version": 1,
                    "gas": gas,
                    "calldata": "0x" + (prefix + selector.to_bytes(32, "big")).hex(),
                }
            )
        )
        return Program(
            ("world-entry", "evmir"),
            [],
            "",
            "",
            world=world.with_runtime(world.read(fixture, SOLAR, "exact"), image),
            evm_version=VERSION,
            replay_ir=text,
            gas="exact",
        )

    def compile_with_audit(self, path, *flags, capture=False):
        directory = self.directory / "table-audits"
        env = {
            **os.environ,
            "SOLAR_TV": CLI,
            "SOLAR": SOLAR,
            "SOLAR_TV_REPLAY": self.replay,
        }
        command = [
            SOLAR,
            path,
            "-j1",
            "--emit=bin,bin-runtime",
            "--allow=2264",
            f"--evm-version={VERSION}",
            *flags,
        ]
        plain = subprocess.check_output(command, env=env, text=True, timeout=30)
        audited = subprocess.run(
            [
                *command,
                f"-Zsolar-tv={directory}",
                *(("-Zsolar-tv-capture",) if capture else ()),
            ],
            env=env,
            text=True,
            capture_output=True,
            timeout=180,
            check=False,
        )
        self.assertEqual(audited.returncode, 0, audited.stderr)
        self.assertEqual(plain, audited.stdout)
        (layout,) = list((directory / "indexed-tables").glob("*/layout.json"))
        (contract,) = json.loads(plain)["contracts"].values()
        return layout, bytes.fromhex(contract["bin-runtime"])

    def check_bundle(self, layout):
        result = subprocess.run(
            [
                CLI,
                "indexed-tables",
                layout,
                "--replay",
                self.replay,
                "--report",
                self.report,
            ],
            timeout=90,
            text=True,
            capture_output=True,
            check=False,
        )
        return result, json.loads(self.report.read_text())

    def test_compiler_emitted_encodings_have_total_in_range_routes(self):
        cases = (
            ("byte", 3, 0, ("-Ogas",), 1),
            ("relative", 10, 8, ("-Osize",), 1),
            ("shift", 10, 8, ("-Onone", "-Zpack-wide-jump-tables"), 1),
            ("two-word", 33, 0, ("-Osize", "-Zpack-wide-jump-tables"), 1),
            ("stride", 10, 8, ("-Onone",), 2),
        )
        for name, count, padding, flags, jumps in cases:
            with self.subTest(encoding=name):
                text, image, decoded, targets = self.compile_table(
                    count, padding, flags
                )
                dispatch = list(decoded.values())[3:]
                dispatch = dispatch[
                    : next(i for i, row in enumerate(dispatch) if row[0] == "jump")
                ]
                ops = [op for op, _ in dispatch]
                match name:
                    case "byte":
                        self.assertEqual(ops[-1], "byte")
                    case "relative":
                        self.assertEqual(ops[-3:], ["byte", "push2", "add"])
                    case "shift":
                        self.assertEqual(ops[-3:], ["shr", "push2", "and"])
                    case "two-word":
                        self.assertEqual(ops[:4], ["dup1", "push1", "shr", "dup1"])
                    case "stride":
                        self.assertEqual(ops, ["push1", "mul", "push2", "add"])

                result, record = self.cli(image, targets, jumps)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual(record["evidence"], "kernel-checked-encoded-table")
                self.assertEqual([row["pc"] for row in record["entries"]], targets)
                for index, row in enumerate(record["entries"]):
                    # First PUSH, PUSH0 and CALLDATALOAD cost eight gas.
                    program = self.program(text, image, index, row["gas"] + 8)
                    replay = native.replay(
                        program,
                        program,
                        [],
                        probe_pc=row["pc"],
                        executable=self.replay,
                        timeout=30,
                    )
                    cut = json.loads(replay["observations"][0])
                    self.assertEqual(cut["stack"], [str(SENTINEL)])
                    self.assertEqual(cut["gas"], 0)

                short = self.program(text, image, 0, record["entries"][0]["gas"] + 7)
                replay = native.replay(
                    short,
                    short,
                    [],
                    raw_runtime=True,
                    executable=self.replay,
                    timeout=30,
                )
                self.assertEqual(json.loads(replay["observations"][0])[0], "out-of-gas")

    def test_mutated_byte_reaches_the_wrong_valid_jumpdest(self):
        text, image, decoded, targets = self.compile_table(3, 0, ("-Ogas",))
        push = next(pc for pc, row in decoded.items() if row[0] == "push3")
        data = image[push + 1 : push + 4]
        index = push + 1 + data.index(targets[0])
        broken = image[:index] + bytes([targets[1]]) + image[index + 1 :]
        result, record = self.cli(broken, targets, 1)
        self.assertEqual(result.returncode, 2)
        self.assertEqual(record["status"], "unsupported")
        self.assertIn("did not verify", result.stderr)

        # The corrupted destination is a different real block. Replay each
        # image in its own world so the corrupted runtime is actually used.
        stops = []
        for code in (image, broken):
            program = self.program(text, code, 0, 500)
            replay = native.replay(
                program,
                program,
                [],
                raw_runtime=True,
                executable=self.replay,
                timeout=30,
            )
            stops.append(json.loads(replay["observations"][0])[0])
        self.assertEqual(stops, ["success", "exception"])

        result, record = self.cli(image, targets, 1, entry=index)
        self.assertEqual(result.returncode, 2)
        self.assertEqual(record["status"], "unsupported")

    def test_mask_at_dispatch_proves_all_selector_words(self):
        text, image, decoded, targets = self.compile_table(4, 0, ("-Ogas",), mask=3)
        self.assertEqual(decoded[4], ("push1", 3))
        self.assertEqual(decoded[6], ("and", None))
        result, record = self.cli(image, targets, 1, entry=7, mask_pc=4)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(record["producer"], {"pc": 4, "mask": 3, "gas": 6})
        self.assertEqual(record["entry_coverage"], {"status": "verified", "cuts": [4]})
        for selector in (0, 1, 2, 3, 4, 1 << 255, (1 << 256) - 1):
            entry = record["entries"][selector & 3]
            program = self.program(text, image, selector, entry["gas"] + 14)
            observed = native.replay(
                program,
                program,
                [],
                probe_pc=entry["pc"],
                executable=self.replay,
                timeout=30,
            )
            cut = json.loads(observed["observations"][0])
            self.assertEqual(cut["stack"], [str(SENTINEL)])
            self.assertEqual(cut["gas"], 0)

        broken = image[:5] + b"\x07" + image[6:]
        result, record = self.cli(broken, targets, 1, entry=7, mask_pc=4)
        self.assertEqual(result.returncode, 2)
        self.assertEqual(record["status"], "unsupported")
        stops = []
        for code in (image, broken):
            program = self.program(text, code, 4, 500)
            observed = native.replay(
                program,
                program,
                [],
                raw_runtime=True,
                executable=self.replay,
                timeout=30,
            )
            stops.append(json.loads(observed["observations"][0])[0])
        self.assertEqual(stops, ["success", "exception"])

    def test_live_assembly_binds_repeated_labels_with_opaque_data(self):
        path = self.directory / "sparse.evmir"
        text = """@module Sparse
@data
  literal_0: hex"5b7f"
bb7 [stack=0]:
  push 0
  calldataload
  push 3
  and
  indexed_jump bb42, bb19, bb42, bb88
bb19:
  push_data literal_0
  pop
  stop
bb42:
  invalid
bb88:
  stop
"""
        path.write_text(text)
        layout, runtime = self.compile_with_audit(
            path, "-Ogas", "-Zevm-ir-pipeline=none"
        )
        emission = json.loads(layout.read_text())
        record = json.loads((layout.parent / "result.json").read_text())
        self.assertEqual(record["status"], "verified")
        self.assertEqual(record["evidence"], "kernel-checked-logical-indexed-tables")
        self.assertEqual(record["bytecode_sha256"], hashlib.sha256(runtime).hexdigest())
        self.assertEqual(runtime[emission["code_end"] :], bytes.fromhex("5b7f"))
        table = record["tables"][0]
        self.assertEqual(table["logical_targets"], [2, 1, 2, 3])
        physical = dict(emission["labels"])
        self.assertEqual(
            [e["pc"] for e in table["entries"]], [physical[i] for i in (2, 1, 2, 3)]
        )
        self.assertEqual(table["producer"]["mask"], 3)

        instructions = decode_runtime(runtime[: emission["code_end"]], self.catalog)
        start = next(pc for pc, row in instructions.items() if row[0] == "push4")
        slot = start + 1 + runtime[start + 1 : start + 5].index(physical[2])
        broken = runtime[:slot] + bytes([physical[1]]) + runtime[slot + 1 :]
        (layout.parent / "bytecode.hex").write_text(broken.hex())
        result, record = self.check_bundle(layout)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(record["status"], "counterexample")
        witness = record["tables"][0]["witness"]
        self.assertEqual(
            (witness["selector"], witness["expected_pc"], witness["actual_pc"]),
            (0, physical[2], physical[1]),
        )
        stops = []
        for image in (runtime, broken):
            program = self.program(text, image, witness["selector"], 500)
            replay = native.replay(
                program,
                program,
                [],
                raw_runtime=True,
                executable=self.replay,
                timeout=30,
            )
            stops.append(json.loads(replay["observations"][0])[0])
        self.assertEqual(stops, ["exception", "success"])

    def test_real_solidity_switch_emits_a_replayable_bundle(self):
        layout, runtime = self.compile_with_audit(SWITCH, "-Osize", capture=True)
        self.assertFalse((layout.parent / "result.json").exists())
        image = bytes.fromhex((layout.parent / "bytecode.hex").read_text())
        self.assertEqual(runtime, image)
        result, record = self.check_bundle(layout)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(record["status"], "verified")
        table = record["tables"][0]
        self.assertEqual(len(table["entries"]), 24)
        self.assertEqual(table["selector_scope"], "from-physical-entry")
        (guard,) = table["range_guards"]
        self.assertEqual(guard["limit"], 24)
        self.assertEqual(
            table["entry_coverage"], {"status": "verified", "cuts": [guard["pc"]]}
        )
        text = (layout.parent / "source.evmir").read_text()
        selector = bytes.fromhex("839a19d9")

        def replay(code, key, *, gas=50_000, pc=None, method=selector):
            program = self.program(text, code, key, gas, prefix=method)
            observed = native.replay(
                program,
                program,
                [],
                probe_pc=pc,
                raw_runtime=pc is None,
                executable=self.replay,
                timeout=30,
            )
            return json.loads(observed["observations"][0])

        for key in (9, 10, 33, 34, (1 << 256) - 1):
            with self.subTest(key=key):
                incoming = replay(runtime, key, pc=guard["pc"])
                index = (key - 10) % (1 << 256)
                self.assertEqual(incoming["stack"][0], str(index))
                entry_stack = incoming["stack"]
                if index < guard["limit"]:
                    edge = table["entries"][index]
                    spent = guard["gas"] + guard["marker_gas"] + edge["gas"]
                    pc = edge["pc"]
                    expected_stack = entry_stack[1:]
                else:
                    spent = guard["gas"]
                    pc = guard["miss_pc"]
                    expected_stack = entry_stack
                funded = 50_000 - incoming["gas"] + spent
                cut = replay(runtime, key, gas=funded, pc=pc)
                self.assertEqual(cut["gas"], 0)
                self.assertEqual(cut["stack"], expected_stack)
                self.assertEqual(replay(runtime, key, gas=funded - 1)[0], "out-of-gas")

        start = guard["pc"]
        self.assertEqual(runtime[start : start + 3], bytes.fromhex("806018"))
        wider = runtime[: start + 2] + bytes([25]) + runtime[start + 3 :]
        (layout.parent / "bytecode.hex").write_text(wider[: len(image)].hex())
        result, record = self.check_bundle(layout)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(record["status"], "counterexample")
        self.assertEqual(
            record["tables"][0]["witness"],
            {
                "selector": 24,
                "expected_pc": guard["miss_pc"],
                "actual_pc": table["entry_pc"] - 1,
                "guard_pc": start,
                "gas": guard["gas"],
            },
        )
        self.assertEqual(
            [replay(code, 34)[0] for code in (runtime, wider)], ["success", "exception"]
        )

        marker = table["entry_pc"] - 1
        instructions = list(decode_runtime(runtime, self.catalog).items())
        (dispatch_push,) = [
            push[0]
            for sub, push, jump in zip(instructions, instructions[1:], instructions[2:])
            if jump[0] < start
            and sub[1][0] == "sub"
            and push[1][0] == "push1"
            and jump[1][0] == "jumpi"
        ]
        redirected = (
            runtime[: dispatch_push + 1]
            + bytes([marker])
            + runtime[dispatch_push + 2 :]
        )
        # The final case falls off the end of the emitted instruction stream.
        # Appended bytes must prevent the proof from treating that as a halt.
        extended = runtime + b"\xfe"
        for broken in (redirected, extended):
            (layout.parent / "bytecode.hex").write_text(broken.hex())
            result, record = self.check_bundle(layout)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            table = record["tables"][0]
            self.assertEqual(table["status"], "verified")
            self.assertEqual(table["selector_scope"], "from-guard-cut")
            self.assertEqual(table["entry_coverage"]["status"], "unproven")
        self.assertEqual(
            [replay(code, 0, method=b"\0\0\0\1")[0] for code in (runtime, redirected)],
            ["revert", "success"],
        )
        self.assertEqual(
            [replay(code, 10)[0] for code in (runtime, extended)],
            ["success", "exception"],
        )

    def test_live_outlined_transfer_uses_the_original_targets(self):
        path = (
            ROOT.parent / "tests/ui/codegen/disasm/indexed_jump_two_byte_entries.evmir"
        )
        layout, _ = self.compile_with_audit(path, "-Onone", "-Zevm-ir-pipeline=none")
        artifact = json.loads(layout.read_text())
        self.assertEqual(artifact["sites"][0]["jumps"], 2)
        record = json.loads((layout.parent / "result.json").read_text())
        self.assertEqual(record["status"], "verified")
        table = record["tables"][0]
        self.assertEqual(table["logical_targets"], list(range(2, 12)))
        self.assertEqual(
            [row["pc"] for row in table["entries"]],
            list(dict(artifact["labels"]).values()),
        )
        self.assertTrue(all(row["gas"] > 24 for row in table["entries"]))

    def test_entry_certificate_covers_two_tables_in_a_cycle(self):
        path = self.directory / "cycle.evmir"
        text = """@module Cycle
bb0 [stack=0]:
  jump bb1
bb1:
  push 0
  calldataload
  push 1
  and
  indexed_jump bb2, bb3
bb2:
  push 32
  calldataload
  push 1
  and
  indexed_jump bb1, bb3
bb3:
  stop
"""
        path.write_text(text)
        layout, runtime = self.compile_with_audit(
            path, "-Ogas", "-Zevm-ir-pipeline=none"
        )
        record = json.loads((layout.parent / "result.json").read_text())
        self.assertEqual(record["status"], "verified")
        first, second = record["tables"]
        cuts = [first["producer"]["pc"], second["producer"]["pc"]]
        for table in (first, second):
            self.assertEqual(table["selector_scope"], "from-physical-entry")
            self.assertEqual(
                table["entry_coverage"], {"status": "verified", "cuts": cuts}
            )

        stops = []
        for selector in (0, 1):
            program = self.program(text, runtime, selector, 500)
            replay = native.replay(
                program,
                program,
                [],
                raw_runtime=True,
                executable=self.replay,
                timeout=30,
            )
            stops.append(json.loads(replay["observations"][0])[0])
        self.assertEqual(stops, ["out-of-gas", "success"])

    def test_extended_stack_payload_is_not_certified_as_jumpdest(self):
        # The local cut adds one to selector zero and jumps. Under Amsterdam
        # byte one is an extended DUP immediate, even though it equals 0x5b.
        image = bytes.fromhex("e65b60010156")
        result, record = self.cli(image, [1], 1, entry=2, version="amsterdam")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(record["status"], "unsupported")
