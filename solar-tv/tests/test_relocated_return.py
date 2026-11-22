"""Check a real ERC20Mock layout change and a misdirected mint return."""

import json
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import evaluate, mint_layout, native, world


class RelocatedReturnTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        temporary = tempfile.TemporaryDirectory()
        cls.addClassCleanup(temporary.cleanup)
        cls.folder = Path(temporary.name)
        cases = json.loads((ROOT / "evaluation.json").read_text())["cases"]
        case = next(row for row in cases if row["name"] == "openzeppelin-erc20-mock")
        inputs = evaluate.inputs(case)
        cls.baseline, _ = evaluate.compile_input(SOLAR, inputs, cls.folder)
        cls.captured, _ = evaluate.compile_input(
            SOLAR, inputs, cls.folder, capture=True
        )
        pairs = [
            row["paths"]
            for row in evaluate.captures(cls.folder)
            if row["module"] == "ERC20Mock_runtime"
            and row["pass"] == "block-layout"
            and (row["region"] or {}).get("name") == "bb67"
        ]
        (cls.paths,) = pairs
        cls.world = cls.folder / "world.json"
        cls.world.write_text(
            json.dumps(
                {
                    "version": 1,
                    "calldata": "0x40c10f19" + f"{0x2222:064x}{7:064x}",
                    "accounts": [
                        {"address": 65536, "storage": {"2": "9", "4": "99"}},
                        {"address": 99, "balance": 123, "storage": {"1": "9"}},
                    ],
                }
            )
        )

    def check_pair(self, stem, paths, status):
        report = self.folder / f"{stem}.json"
        result = subprocess.run(
            [
                CLI,
                "evmir",
                *paths,
                "--solar",
                SOLAR,
                "--return-path=0x40c10f19",
                "--world",
                self.world,
                "--report",
                report,
                "--timeout=90",
                "--verbose",
            ],
            capture_output=True,
            text=True,
            check=False,
            timeout=300,
        )
        self.assertEqual(result.returncode, status, result.stdout + result.stderr)
        return result, json.loads(report.read_text())

    def test_mint_returns_to_the_relocated_continuation(self):
        self.assertEqual(self.baseline, self.captured)
        result, record = self.check_pair("verified", self.paths, 0)
        self.assertIn("continuation PC", result.stdout)
        self.assertEqual(record["region"]["proof"], "kernel-checked-bytecode-return")
        self.assertEqual(record["gas_policy"], "unpriced-structural")
        self.assertEqual(record["evidence"], "kernel-proof")
        self.assertFalse(record["whole_pass"])
        old, new = record["import_correspondence"]
        self.assertNotEqual(old["continuation_pc"], new["continuation_pc"])
        for side in (old, new):
            self.assertNotEqual(side["caller_push_pc"], side["indirect_jump_pc"])
        self.assertEqual(len(set(record["world"]["backend_images"])), 2)

    def test_mint_returning_to_the_bool_stub_is_a_counterexample(self):
        rewrite = self.paths[1].read_text()
        changed, count = re.subn(
            r"(?m)^(  push )bb18(?=\n  jump bb63$)", r"\g<1>bb55", rewrite
        )
        self.assertEqual(count, 1)
        broken = self.folder / "wrong-target.evmir"
        broken.write_text(changed)
        result, record = self.check_pair("counterexample", [self.paths[0], broken], 1)
        self.assertIn("--return-path 0x40c10f19", result.stderr)
        self.assertEqual(record["evidence"], "kernel-and-rust-replay")
        old, new = record["witness"]["observations"]
        self.assertEqual(old[:2], ["success", []])
        self.assertEqual(new[:2], ["success", [0] * 31 + [1]])
        self.assertEqual(old[4:], new[4:])
        (_, _, _, _, slots) = old[4][1]
        self.assertEqual(dict(slots)["2"], "16")
        self.assertEqual(old[5][0][2], [0] * 31 + [7])
        before, after = record["witness"]["cuts"]
        self.assertEqual(before["stack"], [str(0x40C10F19)])
        self.assertEqual(before["stack"], after["stack"])
        self.assertEqual(before["memory"], after["memory"])
        self.assertNotEqual(before["pc"], after["pc"])

    def test_symbolic_mint_replays_the_gas_boundaries(self):
        self.assertEqual(self.baseline, self.captured)
        solar = str(Path(SOLAR).resolve())
        executable = native.default(solar)
        images = [
            world.runtime(path.read_text(), "evmir", solar, "cancun", 90)
            for path in self.paths
        ]
        proof = self.folder / "mint-proof"
        proof.mkdir()
        initial = world.read(self.world, solar, None)
        paths = mint_layout.certify(proof, images, initial, solar, executable, 90)
        self.assertEqual(
            set(paths),
            {"Nonpayable", "Malformed", "ZeroAddress", "Overflow", "Success"},
        )

        root = json.loads(self.world.read_text())
        selector = "0x40c10f19"
        balance_slot = "83862486449319017530411873853010819072563423120971894174048127046291118262171"
        scenarios = [
            ("cold", root["calldata"], 9, [], "success", ""),
            ("short", selector, 9, [], "revert", ""),
            (
                "dirty",
                selector + f"{(1 << 160) | 0x2222:064x}{7:064x}",
                9,
                [],
                "revert",
                "",
            ),
            (
                "zero",
                selector + f"{0:064x}{7:064x}",
                9,
                [],
                "revert",
                "ec442f05" + "00" * 32,
            ),
            (
                "overflow",
                root["calldata"],
                2**256 - 1,
                [],
                "revert",
                "4e487b71" + f"{17:064x}",
            ),
            (
                "sentry",
                selector + f"{0x2222:064x}{0:064x}",
                9,
                [[65536, "2"], [65536, balance_slot]],
                "success",
                "",
            ),
        ]
        for name, calldata, supply, warm_slots, halt, data in scenarios:
            with self.subTest(name=name):
                root["calldata"] = calldata
                root["accounts"][0]["storage"]["2"] = str(supply)
                root["warm_slots"] = warm_slots
                fixture = self.folder / "mint-replay.json"
                fixture.write_text(json.dumps(root))
                result = mint_layout.replay(
                    proof, images, world.read(fixture, solar, None), executable, 90
                )
                before = result["minimum_gas_before"]
                after = result["minimum_gas_after"]
                self.assertLessEqual(after, before)
                samples = {r["gas"]: r["observations"] for r in result["samples"]}
                low = samples[after - 1]
                self.assertEqual([row[:2] for row in low], [["out-of-gas", []]] * 2)
                for observation in samples[before]:
                    self.assertEqual(observation[0], halt)
                    self.assertEqual(bytes(observation[1]).hex(), data)
                if name in {"cold", "overflow"}:
                    self.assertEqual(before - after, 11)
                    self.assertEqual(samples[after][0][0], "out-of-gas")
                    self.assertEqual(samples[after][1][0], halt)
                if name == "cold":
                    # Exhaustion can occur after the first SSTORE, which must roll back.
                    self.assertEqual(dict(low[0][4][1][4])["2"], "9")
                    self.assertEqual(dict(low[1][4][1][4])["2"], "9")
                    self.assertEqual(low[0][5], [])
                    self.assertEqual(low[1][5], [])
                if name == "sentry":
                    self.assertEqual(before, after)
