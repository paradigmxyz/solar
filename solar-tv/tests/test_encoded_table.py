"""Check each assembler encoding against the actual deployed byte image."""

import json
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

    def cli(self, image, targets, jumps, *, entry=4, mask_pc=None):
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

    def program(self, text, image, selector, gas):
        fixture = self.directory / "world.json"
        fixture.write_text(
            json.dumps(
                {
                    "version": 1,
                    "gas": gas,
                    "calldata": "0x" + selector.to_bytes(32, "big").hex(),
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
