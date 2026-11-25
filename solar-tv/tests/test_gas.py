"""Gas-aware proofs and replayed out-of-gas / GAS-observer counterexamples."""

import os
import shlex
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR, check


class GasTests(unittest.TestCase):
    def audit(self, before, after, *, mode="refine", layer="evmir", version="cancun"):
        return subprocess.run(
            [
                CLI,
                layer,
                before,
                after,
                "--solar",
                SOLAR,
                f"--gas={mode}",
                f"--evm-version={version}",
            ],
            text=True,
            capture_output=True,
            check=False,
            timeout=120,
        )

    def pair(self, before, after, *, mode="refine", height=0, status=1):
        with tempfile.TemporaryDirectory() as tmp:
            paths = []
            for name, body in [("before", before), ("after", after)]:
                path = Path(tmp) / f"{name}.evmir"
                path.write_text(f"@module Gas\nbb0 [stack={height}]:\n{body}\n")
                paths.append(path)
            result = self.audit(*paths, mode=mode)
        self.assertEqual(result.returncode, status, result.stderr + result.stdout)
        return result

    def test_real_peephole_is_resource_refinement(self):
        folder = ROOT / "examples/evmir/gas"
        for version in ("cancun", "osaka"):
            with self.subTest(version=version):
                result = self.audit(
                    folder / "before.evmir", folder / "after.evmir", version=version
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout, "verified\n")

    def test_savings_do_not_imply_exact_gas_equality(self):
        folder = ROOT / "examples/evmir/gas"
        result = self.audit(
            folder / "before.evmir", folder / "after.evmir", mode="exact"
        )
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("gas = 17\n", result.stdout)
        self.assertIn("Remaining gas changed from 0 to 3", result.stdout)

    def test_added_cost_trips_out_of_gas(self):
        folder = ROOT / "examples/evmir/gas"
        result = self.audit(folder / "before.evmir", folder / "broken.evmir")
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("gas = 17\n", result.stdout)
        self.assertIn("after  = out-of-gas, gas=0", result.stdout)
        self.assertIn("Out-of-gas behavior changed", result.stdout)

    def test_gas_reads_after_paying_its_own_charge(self):
        folder = ROOT / "examples/evmir/gas-observer"
        result = self.audit(folder / "before.evmir", folder / "broken.evmir")
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("gas = 18\n", result.stdout)
        self.assertIn("before = bb1, stack=[16]", result.stdout)
        self.assertIn("after  = bb1, stack=[11]", result.stdout)

    def test_memory_expansion_can_be_the_only_behavior_difference(self):
        folder = ROOT / "examples/evmir/gas-memory"
        before, after = folder / "before.evmir", folder / "broken.evmir"
        sufficient = check("evmir", before, after)
        self.assertEqual(sufficient.returncode, 0, sufficient.stderr)
        limited = self.audit(before, after)
        self.assertEqual(limited.returncode, 1, limited.stderr)
        self.assertIn("gas = 10\n", limited.stdout)
        self.assertIn("before = success(empty), gas=0", limited.stdout)
        self.assertIn("after  = out-of-gas, gas=0", limited.stdout)

    def test_push0_and_mul_have_distinct_prices(self):
        result = self.pair("push 0\npop\nstop", "push 1\npop\nstop")
        self.assertIn("gas = 4\n", result.stdout)
        result = self.pair(
            "push 0\ndup 1\nadd\npop\nstop", "push 0\ndup 1\nmul\npop\nstop"
        )
        self.assertIn("gas = 10\n", result.stdout)

    def test_exact_identity_includes_oog_revert_and_exception(self):
        for code in (
            "stop",
            "invalid",
            "push 0\npush 0\nrevert",
            "push 0\nmload\npop\nstop",
        ):
            with self.subTest(code=code):
                self.pair(code, code, mode="exact", status=0)

    def test_revert_cannot_turn_into_success(self):
        result = self.pair("push 0\npush 0\nrevert", "push 0\npush 0\nreturn")
        self.assertIn("before = revert(empty), gas=0", result.stdout)
        self.assertIn("after  = success(empty), gas=0", result.stdout)

    def test_gas_never_gets_a_fake_mir_price(self):
        folder = ROOT / "examples/mir/dce"
        result = self.audit(folder / "before.mir", folder / "after.mir", layer="mir")
        self.assertEqual(result.returncode, 2)
        self.assertIn("gas auditing requires scheduled EVM IR, not MIR", result.stderr)

    def test_unsupported_version_and_opcodes_are_inconclusive(self):
        path = ROOT / "examples/evmir/gas/before.evmir"
        result = self.audit(path, path, version="unknown")
        self.assertEqual(result.returncode, 2)
        self.assertIn("unknown gas-audit EVM version", result.stderr)
        self.pair("push 0\nsload\npop\nstop", "stop", status=2)

    @unittest.skipIf(sys.platform == "win32", "POSIX shell replay")
    def test_replay_retains_gas_policy_and_version(self):
        folder = ROOT / "examples/evmir/gas"
        result = self.audit(
            folder / "before.evmir", folder / "broken.evmir", version="prague"
        )
        self.assertEqual(result.returncode, 1, result.stderr)
        argv = shlex.split(result.stdout.rsplit("\nReplay\n  ", 1)[1].strip())
        start = argv.index("--gas")
        self.assertEqual(
            argv[start : start + 4], ["--gas", "refine", "--evm-version", "prague"]
        )
        replay = subprocess.run(
            argv, capture_output=True, text=True, timeout=120, check=False
        )
        self.assertEqual(replay.returncode, 1, replay.stderr)
        self.assertEqual(replay.stdout, result.stdout)

    def test_host_environment_can_enable_gas(self):
        folder = ROOT / "examples/evmir/gas"
        result = subprocess.run(
            [
                CLI,
                "evmir",
                folder / "before.evmir",
                folder / "broken.evmir",
                "--solar",
                SOLAR,
            ],
            env={**os.environ, "SOLAR_TV_GAS": "refine"},
            capture_output=True,
            text=True,
            timeout=120,
            check=False,
        )
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("gas=refine, evm=cancun", result.stdout)
