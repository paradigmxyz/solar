"""Real Morpho signing expansions and independent replay at the message cut."""

import json
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import test_mir_abi as abi
import test_mir_state as state
from test_cli import CLI, ROOT, SOLAR

sys.path.insert(0, str(ROOT))
from validator import builtin_lowering, call_budget, native
from validator.common import Unsupported


class AuthorizationLoweringTests(unittest.TestCase):
    pair = state.MirStateTests.pair
    lower = abi.MirAbiTests.lower

    def test_captured_morpho_pair(self):
        with tempfile.TemporaryDirectory() as work:
            directory = Path(work)
            *_, pair = call_budget.capture(SOLAR, directory, 45)
            proof = directory / "Authorization.lean"
            name = "setAuthorizationWithSig"
            replay = native.default(SOLAR)
            result = builtin_lowering.check(
                pair, SOLAR, replay, "cancun", 45, name, proof_path=proof
            )
            self.assertEqual(result.status, "verified")
            self.assertEqual(
                result.program.import_evidence["kind"], "contextual-signing-expansion"
            )
            self.assertTrue(proof.is_file())
            report = directory / "authorization.json"
            cli = subprocess.run(
                [
                    CLI,
                    "mir",
                    *pair,
                    "--solar",
                    SOLAR,
                    "--replay",
                    replay,
                    "--evm-version",
                    "cancun",
                    "--function",
                    name,
                    "--report",
                    report,
                    "--timeout",
                    "45",
                ],
                text=True,
                capture_output=True,
                check=False,
                timeout=100,
            )
            self.assertEqual(cli.returncode, 0, cli.stdout + cli.stderr)
            audit = json.loads(report.read_text())
            self.assertEqual(audit["region"]["proof"], "contextual-signing-expansion")
            self.assertTrue(
                any("paid GAS sample" in row for row in audit["assumptions"])
            )

            after = pair[1].read_text()
            start = after.index(f"fn @{name}(")
            end = after.index("\n}", start)
            function = after[start:end]
            broken = directory / "broken.mir"
            for pattern, replacement in (
                (r"(ptradd v\d+, )34", r"\g<1>35"),
                (r"mstore 0, 0", "mstore 0, 7"),
            ):
                with self.subTest(pattern=pattern):
                    altered, count = re.subn(pattern, replacement, function, count=1)
                    self.assertEqual(count, 1)
                    broken.write_text(after[:start] + altered + after[end:])
                    with self.assertRaisesRegex(Unsupported, "expansion instruction"):
                        builtin_lowering.check(
                            (pair[0], broken), SOLAR, replay, "cancun", 45, name
                        )

    def test_signing_envelope_and_recovery_boundary_replay(self):
        before = state.module(
            'v0 = abi_encode_packed (data hex"1901", bytes32 17, bytes32 19)\n'
            "v7 = ptradd v0, 34\n"
            "v8 = mload v7\n"
            "jump bb1\n"
            "bb1:\n"
            "v1 = inttoptr i256 128 to ptr<memory>\n"
            "v2 = make_memory_slice v1, 128\n"
            "v3 = extcall static word i160 1, v2\n"
            "v4 = zext i1 v3 to i256\n"
            "v5 = mload 0\n"
            "v6 = or v4, v5\n"
            "v9 = add v6, v8\n"
            "ret v9"
        )
        after = self.lower(before, "lower-builtins")
        self.assertRegex(after, r"(?m)^    v\d+ = gas$")
        memory = bytearray(384)
        memory[31] = 42
        memory[64:96] = (256).to_bytes(32, "big")
        memory[256:] = bytes([255]) * 128
        # The replay starts with a paid GAS sample supplied at the STATICCALL
        # boundary. Native MIR exposes GAS only in isolated opcode windows.
        for budget in (99, 3100, 3200):
            with self.subTest(budget=budget):
                raw = re.sub(r"(?m) = gas$", f" = add {budget}, 0", after)
                fixture = {
                    "memory": {"data": "0x" + memory.hex()},
                    "message_gas": [budget],
                }
                self.pair(before, raw, fixture=fixture)

        raw = re.sub(r"(?m) = gas$", " = add 3200, 0", after)
        fixture = {"memory": {"data": "0x" + memory.hex()}, "message_gas": [3200]}
        mutations = {
            "request": re.sub(r"(?m) = gas$", " = add 0, 0", after),
            "scratch": raw.replace("mstore 0, 0", "mstore 0, 42", 1),
            "offset": re.sub(r"(ptradd v\d+, )34", r"\g<1>35", raw, count=1),
        }
        for name, broken in mutations.items():
            with self.subTest(name=name):
                self.assertNotEqual(raw, broken)
                _, record = self.pair(before, broken, fixture=fixture, status=1)
                witness = record["witness"]
                observed = witness["observations"]
                self.assertEqual(observed, witness["native_observations"])
                self.assertEqual(
                    [row["values"] for row in observed],
                    [["18"], [{"request": "17", "scratch": "60", "offset": "1"}[name]]],
                )
