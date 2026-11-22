"""Certify the first heap-pointer write in emitted constructor bytecode."""

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
