"""Dynamic EXP, copy and hash charges in Lean and independent Rust execution."""

import json
import subprocess
import tempfile
import unittest
from pathlib import Path

from test_cli import ROOT, SOLAR

REPLAY = str(Path(SOLAR).with_name("solar-tv-replay"))
GAS = 1_000_000
MAXIMUM = 2**256 - 1


class DynamicGasTests(unittest.TestCase):
    def test_kernel_and_native_charges_agree(self):
        cases = [
            (
                "push 33\npush 0\nkeccak256\npop",
                ".instruction (.stack (.push 33)), .instruction (.stack (.push 0)), .instruction .hash, .instruction (.stack .pop)",
                55,
            ),
            (
                "push 33\npush 127\npush 1\nmcopy",
                ".instruction (.stack (.push 33)), .instruction (.stack (.push 127)), .instruction (.stack (.push 1)), .instruction .copy",
                33,
            ),
            (
                f"push 0\npush {MAXIMUM}\npush {MAXIMUM}\nmcopy",
                f".instruction (.stack (.push 0)), .instruction (.stack (.push {MAXIMUM})), .instruction (.stack (.push {MAXIMUM})), .instruction .copy",
                11,
            ),
            (
                f"push 0\npush {MAXIMUM}\nkeccak256\npop",
                f".instruction (.stack (.push 0)), .instruction (.stack (.push {MAXIMUM})), .instruction .hash, .instruction (.stack .pop)",
                37,
            ),
            (
                f"push {MAXIMUM}\npush 1\nexp\npop",
                f".instruction (.stack (.push {MAXIMUM})), .instruction (.stack (.push 1)), .instruction (.stack .exp), .instruction (.stack .pop)",
                1618,
            ),
        ]
        examples = []
        for body, operations, charge in cases:
            with self.subTest(body=body):
                text = f"@module Dynamic\nbb0:\n{body}\nstop\n"
                request = {
                    "version": 1,
                    "kind": "evmir",
                    "before": text,
                    "after": text,
                    "inputs": {"gas": str(GAS)},
                    "memory": False,
                    "gas": True,
                    "evm_version": "cancun",
                }
                result = subprocess.run(
                    [REPLAY],
                    input=json.dumps(request),
                    capture_output=True,
                    text=True,
                    check=True,
                    timeout=30,
                )
                self.assertEqual(
                    json.loads(result.stdout),
                    {
                        "version": 1,
                        "status": "ok",
                        "observations": [f"success(empty), gas={GAS - charge}"] * 2,
                    },
                )
                examples.append(
                    f"example : EVM.Gas.execute ⟨0, 0⟩ [{operations}] "
                    f"(.control .stop) [] (Memory.initial [] 0) {GAS} = "
                    f"⟨.execution (.success []), {GAS - charge}⟩ := by decide +kernel"
                )
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "DynamicGas.lean"
            path.write_text(
                "import SolarTV\nopen SolarTV\nset_option maxRecDepth 4096\n"
                + "\n".join(examples)
            )
            result = subprocess.run(
                ["lake", "env", "lean", path],
                cwd=ROOT,
                capture_output=True,
                text=True,
                check=False,
                timeout=60,
            )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
