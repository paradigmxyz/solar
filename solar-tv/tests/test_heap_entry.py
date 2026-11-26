"""Certify heap-pointer writes from constructor and ABI entry."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import evaluate, heap_entry, native, world


class HeapEntryTests(unittest.TestCase):
    def test_address_word_guard_checks_the_declared_width(self):
        image = bytes.fromhex("60043560a01c600f576080604052005bfe")
        selector = 0x12345678
        with tempfile.TemporaryDirectory() as folder:
            directory = Path(folder)
            path = directory / "runtime.hex"
            path.write_text(image.hex())
            args = [
                CLI,
                "heap-entry",
                path,
                "--solar",
                SOLAR,
                "--floor",
                "128",
                "--extent",
                "0",
                "--selector",
                hex(selector),
                "--cut",
                "14",
                "--min-calldata",
                "36",
                "--word-bound",
                "4:160",
            ]
            verified = subprocess.run(
                args, check=True, capture_output=True, text=True, timeout=60
            )
            report = json.loads(verified.stdout)
            self.assertEqual(report["word_bounds"], {"4": 160})
            self.assertEqual(report["status"], "verified")
            self.assertEqual(report["replay"]["underfunded"], ["out-of-gas", [], 0])

            changed = image[:4] + b"\x9f" + image[5:]
            path.write_text(changed.hex())
            rejected = subprocess.run(
                args, check=False, capture_output=True, text=True, timeout=60
            )
            self.assertEqual(rejected.returncode, 2, rejected.stderr)

            fixture_path = directory / "world.json"
            fixture_path.write_text(
                json.dumps(
                    {
                        "version": 1,
                        "calldata": "0x"
                        + selector.to_bytes(4, "big").hex()
                        + (1 << 159).to_bytes(32, "big").hex(),
                    }
                )
            )
            fixture = world.read(fixture_path, SOLAR, "exact")
            executable = native.default(SOLAR)
            original = heap_entry.observe(
                image, fixture, "cancun", executable, 10, pc=14
            )
            self.assertEqual(
                int.from_bytes(bytes(original["memory"][64:96]), "big"), 128
            )
            altered = heap_entry.observe(changed, fixture, "cancun", executable, 10)
            self.assertEqual(altered[0], "exception")

    def test_abi_heap_route_checks_the_store_and_length_guard(self):
        image = bytes.fromhex(
            "60043610601557"  # Short calldata branches to the shared revert.
            "5f3560e01c631234567814601957"  # Select the public handler.
            "5b5f5ffd"
            "5b60a43610601557"  # The handler requires a 164-byte head.
            "61012060405200"  # Publish the FMP, then stop.
        )
        with tempfile.TemporaryDirectory() as folder:
            directory = Path(folder)
            path = directory / "runtime.hex"
            path.write_text(image.hex())
            args = [
                CLI,
                "heap-entry",
                path,
                "--solar",
                SOLAR,
                "--floor",
                "288",
                "--extent",
                "192",
                "--selector",
                "0x12345678",
                "--cut",
                "39",
                "--min-calldata",
            ]
            result = subprocess.run(
                [*args, "164"], check=True, capture_output=True, text=True, timeout=60
            )
            report = json.loads(result.stdout)
            self.assertEqual(report["evidence"], "kernel-proof")
            self.assertEqual((report["cut"], report["gas_cost"]), (39, 91))
            self.assertEqual(report["replay"]["underfunded"], ["out-of-gas", [], 0])
            self.assertEqual(report["replay"]["funded"]["memory"], [0] * 94 + [1, 32])

            too_short = subprocess.run(
                [*args, "163"], check=False, capture_output=True, text=True, timeout=60
            )
            self.assertEqual(
                too_short.returncode, 2, too_short.stdout + too_short.stderr
            )

            changed = image[:35] + b"\x00" + image[36:]
            path.write_text(changed.hex())
            wrong_floor = subprocess.run(
                [*args, "164"], check=False, capture_output=True, text=True, timeout=60
            )
            self.assertEqual(
                wrong_floor.returncode, 2, wrong_floor.stdout + wrong_floor.stderr
            )
            fixture_path = directory / "world.json"
            fixture_path.write_text(
                json.dumps(
                    {"version": 1, "gas": 91, "calldata": "0x12345678" + "00" * 160}
                )
            )
            fixture = world.read(fixture_path, SOLAR, "exact")
            reached = heap_entry.observe(
                changed, fixture, "cancun", native.default(SOLAR), 10, pc=39
            )
            self.assertEqual(reached["memory"], [0] * 94 + [1, 0])

    def test_openzeppelin_creation_bytecode_heap_floor(self):
        manifest = json.loads((ROOT / "evaluation.json").read_text())
        (case,) = (
            case
            for case in manifest["cases"]
            if case["name"] == "openzeppelin-erc20-mock"
        )
        with tempfile.TemporaryDirectory() as folder:
            directory = Path(folder)
            source = evaluate.inputs(case)
            baseline, _ = evaluate.compile_input(SOLAR, source, directory)
            captured, _ = evaluate.compile_input(SOLAR, source, directory, capture=True)
            self.assertEqual(baseline, captured)
            outputs = json.loads((directory / "baseline.json").read_text())
            contract = outputs["contracts"][case["source"]][case["contract"]]
            image = bytes.fromhex(contract["evm"]["bytecode"]["object"])
            path = directory / "constructor.hex"
            path.write_text(image.hex())
            args = [
                CLI,
                "heap-entry",
                path,
                "--solar",
                SOLAR,
                "--floor=0xc0",
                "--extent=96",
            ]
            result = subprocess.run(
                args, check=True, capture_output=True, text=True, timeout=60
            )
            report = json.loads(result.stdout)
            self.assertEqual(report["evidence"], "kernel-proof")
            self.assertEqual((report["cut"], report["gas_cost"]), (5, 18))
            self.assertEqual(report["replay"]["underfunded"], ["out-of-gas", [], 0])
            self.assertEqual(
                report["replay"]["funded"]["memory"][-32:], [0] * 31 + [192]
            )

            changed = image[:1] + b"\x80" + image[2:]
            path.write_text(changed.hex())
            result = subprocess.run(
                args, check=False, capture_output=True, text=True, timeout=60
            )
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            fixture_path = directory / "world.json"
            fixture_path.write_text('{"version":1,"gas":18}')
            fixture = world.read(fixture_path, SOLAR, "exact")
            reached = heap_entry.observe(
                changed, fixture, "cancun", native.default(SOLAR), 10, pc=5
            )
            self.assertEqual(reached["memory"][-32:], [0] * 31 + [128])
