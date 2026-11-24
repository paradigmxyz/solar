"""Kernel and evm2 agreement at the transaction boundary."""

import json
import subprocess
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

EXAMPLES = ROOT / "examples/evmir/transactions"
CURVE_ORDER = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141

# The same public fixture signer authorizes clearing at nonce 1 for any chain.
CLEAR = {
    "chainId": "0x0",
    "address": "0x0000000000000000000000000000000000000000",
    "nonce": "0x1",
    "yParity": "0x1",
    "r": "0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
    "s": "0x410fbe615676151886af9bd4c4397e2e0502f9fece21f46f30362b38502d424b",
}


class TransactionTests(unittest.TestCase):
    def check(self, fixture):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "tx.json"
            path.write_text(json.dumps(fixture))
            process = subprocess.run(
                [CLI, "transaction", path, "--solar", SOLAR],
                text=True,
                capture_output=True,
                timeout=120,
                check=False,
            )
        self.assertEqual(process.returncode, 0, process.stdout + process.stderr)
        report = json.loads(process.stdout)
        self.assertEqual(report["evidence"], "kernel-and-rust-replay")
        return report["observation"]

    def test_revert_keeps_signed_authorization_and_refund(self):
        request = json.loads((EXAMPLES / "set-code-revert.json").read_text())
        report = self.check(request)
        self.assertTrue(report["admitted"])
        self.assertFalse(report["success"])
        self.assertEqual(report["accounts"][0], [1, []])
        nonce, code = report["accounts"][1]
        self.assertEqual(nonce, 1)
        self.assertEqual(code, [239, 1, 0, *int("0x10002", 16).to_bytes(20, "big")])
        self.assertEqual((report["spent"], report["refund"]), (46004, 9200))

    def test_clear_uses_updated_nonce_and_wildcard_chain(self):
        request = json.loads((EXAMPLES / "set-code-revert.json").read_text())
        request["fork"] = "osaka"
        request["tx"]["gas"] = hex(80000)
        request["tx"]["authorizationList"].append(CLEAR)
        report = self.check(request)
        self.assertEqual(report["accounts"], [[1, []], [2, []]])
        self.assertEqual(report["spent"], 71004)

    def test_high_s_and_wrong_chain_do_not_apply(self):
        for key in ("chainId", "s"):
            with self.subTest(key=key):
                request = json.loads((EXAMPLES / "set-code-revert.json").read_text())
                auth = request["tx"]["authorizationList"][0]
                if key == "s":
                    auth["s"] = hex(CURVE_ORDER - int(auth["s"], 16))
                    auth["yParity"] = hex(1 - int(auth["yParity"], 16))
                else:
                    auth["chainId"] = "0x2"
                report = self.check(request)
                self.assertEqual(report["accounts"], [[1, []], [0, []]])
                self.assertEqual(report["refund"], 0)

    def test_admission_rejects_unpaid_intrinsic_gas(self):
        request = json.loads((EXAMPLES / "set-code-revert.json").read_text())
        request["tx"]["gas"] = hex(45999)
        self.assertFalse(self.check(request)["admitted"])

    def test_self_authorization_sees_sender_nonce_increment(self):
        request = json.loads((EXAMPLES / "set-code-revert.json").read_text())
        request["sender"] = request["inspect"][1]
        request["tx"]["authorizationList"] = [CLEAR]
        report = self.check(request)
        self.assertTrue(report["admitted"])
        self.assertEqual(report["accounts"], [[0, []], [2, []]])

    def test_amsterdam_state_gas_spills_after_reservoir(self):
        request = json.loads((EXAMPLES / "amsterdam-reservoir.json").read_text())
        self.assertEqual(
            self.check(request),
            [
                [16756216, 150, 0, 0],
                [16756216, 50, 100, 0],
                [16756213, 50, 100, 0],
                [16756209, 0, 154, 4],
                [16756204, 0, 154, 4],
                [16756208, 150, 0, 0],
            ],
        )
        request["charges"] = [{"execution": 16756217}]
        self.assertEqual(self.check(request), [[16756216, 150, 0, 0], None])
        request["charges"] = [{"state": 16756367}]
        self.assertEqual(self.check(request), [[16756216, 150, 0, 0], None])

    def test_amsterdam_refills_spill_before_reservoir(self):
        request = json.loads((EXAMPLES / "amsterdam-reservoir.json").read_text())
        request["charges"] = [
            {"state": 154},
            {"refill": 2},
            {"refill": 7},
            {"refill": 155},
            {"execution": 3},
            {"rollback": None},
        ]
        self.assertEqual(
            self.check(request),
            [
                [16756216, 150, 0, 0],
                [16756212, 0, 154, 4],
                [16756214, 0, 152, 2],
                [16756216, 5, 145, 0],
                [16756216, 160, -10, 0],
                [16756213, 160, -10, 0],
                [16756213, 150, 0, 0],
            ],
        )
