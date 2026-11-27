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
        self.assertEqual(report["gas_used"], 36_804)

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

    def test_receipt_floor_settles_balances_after_execution(self):
        request = json.loads((EXAMPLES / "set-code-revert.json").read_text())
        request["base_fee"] = "0x3"
        request["tx"].update(
            {
                "gas": hex(100_000),
                "maxFeePerGas": "0x8",
                "maxPriorityFeePerGas": "0x2",
                "value": "0x11",
                "input": "0x" + "01" * 600,
            }
        )
        other = "0x0000000000000000000000000000000000010003"
        for beneficiary, balance, reward in (
            (other, 775_000, 90_000),
            (request["sender"], 865_000, 865_000),
        ):
            with self.subTest(beneficiary=beneficiary):
                request["beneficiary"] = beneficiary
                report = self.check(request)
                self.assertTrue(report["admitted"])
                self.assertFalse(report["success"])
                self.assertEqual((report["spent"], report["refund"]), (55_604, 11_120))
                self.assertEqual(
                    (report["floor_gas"], report["gas_used"]), (45_000, 45_000)
                )
                self.assertEqual(int(report["sender_balance"], 0), balance)
                self.assertEqual(int(report["beneficiary_balance"], 0), reward)

        request["beneficiary"] = other
        request["accounts"][0]["code"] = "0x00"
        report = self.check(request)
        self.assertTrue(report["success"])
        self.assertEqual(report["gas_used"], 45_000)
        self.assertEqual(int(report["sender_balance"], 0), 774_983)
        self.assertEqual(int(report["beneficiary_balance"], 0), 90_000)

    def test_self_authorization_sees_sender_nonce_increment(self):
        request = json.loads((EXAMPLES / "set-code-revert.json").read_text())
        request["sender"] = request["inspect"][1]
        request["tx"]["authorizationList"] = [CLEAR]
        report = self.check(request)
        self.assertTrue(report["admitted"])
        self.assertEqual(report["accounts"], [[0, []], [2, []]])

    def test_amsterdam_dynamic_fee_commits_storage_and_both_gas_components(self):
        request = json.loads((EXAMPLES / "amsterdam-dynamic-fee.json").read_text())
        report = self.check(request)
        self.assertTrue(report["admitted"])
        self.assertEqual(report["stop"], "success")
        self.assertEqual(report["floor_gas"], 15_000)
        self.assertEqual(report["execution_gas"], 27_105)
        self.assertEqual(report["state_gas"], 97_920)
        self.assertEqual(report["gas_used"], 125_025)
        self.assertEqual(
            [(int(key, 0), int(value, 0)) for key, value in report["accounts"][1][3]],
            [(0, 1)],
        )
        self.assertEqual(int(report["sender_balance"], 0), 10**12 - 5 * 125_025)
        self.assertEqual(int(report["beneficiary_balance"], 0), 2 * 125_025)

    def test_amsterdam_dynamic_fee_rejects_bad_chain_nonce_and_fees(self):
        for field, value in (
            ("chainId", "0x2"),
            ("nonce", "0x1"),
            ("maxFeePerGas", "0x2"),
            ("maxPriorityFeePerGas", "0x9"),
        ):
            with self.subTest(field=field):
                request = json.loads(
                    (EXAMPLES / "amsterdam-dynamic-fee.json").read_text()
                )
                request["tx"][field] = value
                self.assertEqual(self.check(request), {"admitted": False})

    def test_amsterdam_dynamic_fee_caps_refund_and_enforces_floor(self):
        request = json.loads((EXAMPLES / "amsterdam-dynamic-fee.json").read_text())
        request["accounts"][0]["code"] = "0x60015f555f5f5500"
        request["tx"]["input"] = "0x" + "00" * 600
        report = self.check(request)
        self.assertEqual(report["stop"], "success")
        self.assertEqual(report["spent"], 29_609)
        self.assertEqual(report["refund"], 5_921)
        self.assertEqual(report["gas_used"], 53_400)
        self.assertEqual((report["execution_gas"], report["state_gas"]), (53_400, 0))
        self.assertEqual(report["accounts"][1][3], [])
        self.assertEqual(int(report["sender_balance"], 0), 10**12 - 5 * 53_400)

    def test_amsterdam_dynamic_fee_keeps_fees_but_reverts_transfer_and_storage(self):
        request = json.loads((EXAMPLES / "amsterdam-dynamic-fee.json").read_text())
        request["accounts"][0]["code"] = "0x60015f555f5ffd"
        request["tx"]["value"] = "0x11"
        report = self.check(request)
        self.assertEqual(report["stop"], "revert")
        self.assertEqual((report["gas_used"], report["state_gas"]), (33_109, 0))
        self.assertEqual(report["accounts"][0][0], 1)
        self.assertEqual(int(report["accounts"][1][1], 0), 0)
        self.assertEqual(report["accounts"][1][3], [])
        self.assertEqual(report["logs"], [])
        self.assertEqual(int(report["sender_balance"], 0), 10**12 - 5 * 33_109)

    def test_amsterdam_dynamic_fee_new_recipient_state_reservation(self):
        request = json.loads((EXAMPLES / "amsterdam-dynamic-fee.json").read_text())
        request["accounts"].pop(0)
        request["tx"].update(gas=hex(204_600), value="0x11")
        report = self.check(request)
        self.assertEqual(report["stop"], "success")
        self.assertEqual(
            (report["execution_gas"], report["state_gas"]), (21_000, 183_600)
        )
        self.assertEqual(int(report["accounts"][1][1], 0), 17)
        self.assertEqual(len(report["logs"]), 1)
        self.assertEqual(int(report["sender_balance"], 0), 10**12 - 5 * 204_600 - 17)

        request["tx"]["gas"] = hex(204_599)
        report = self.check(request)
        self.assertTrue(report["admitted"])
        self.assertEqual(report["stop"], "out-of-gas")
        self.assertEqual(report["execution_gas"], 204_599)
        self.assertEqual(report["state_gas"], 0)
        self.assertIsNone(report["accounts"][1])
        self.assertEqual(report["logs"], [])

    def test_amsterdam_dynamic_fee_delegation_resolves_warm_and_cold_targets(self):
        request = json.loads((EXAMPLES / "amsterdam-dynamic-fee.json").read_text())
        library = "0x0000000000000000000000000000000000010004"
        request["accounts"][0]["code"] = "0xef0100" + library[2:]
        request["accounts"].append(
            {"address": library, "nonce": 1, "balance": "0x0", "code": "0x60015f5500"}
        )
        for access_list, spent in (
            ([], 128_025),
            ([{"address": library, "storageKeys": []}], 129_305),
        ):
            with self.subTest(access_list=access_list):
                request["tx"]["accessList"] = access_list
                report = self.check(request)
                self.assertEqual(report["stop"], "success")
                self.assertEqual(report["spent"], spent)
                self.assertEqual(report["state_gas"], 97_920)
                self.assertEqual(int(report["accounts"][1][3][0][1], 0), 1)

    def test_amsterdam_block_separates_header_and_receipt_gas(self):
        request = json.loads(
            (EXAMPLES / "amsterdam-dynamic-fee-block.json").read_text()
        )
        report = self.check(request)
        self.assertTrue(report["admitted"])
        self.assertTrue(report["header_matches"])
        self.assertEqual(report["header_gas_used"], 97_920)
        self.assertEqual(report["execution_gas"], 44_210)
        self.assertEqual(report["state_gas"], 97_920)
        self.assertEqual(report["cumulative_gas"], 142_130)
        self.assertEqual(
            [row["cumulative_gas"] for row in report["receipts"]], [125_025, 142_130]
        )
        self.assertEqual(report["accounts"][0][0], 2)
        self.assertEqual(int(report["accounts"][0][1], 0), 10**12 - 5 * 142_130)

        request["header_gas_used"] = 142_130
        report = self.check(request)
        self.assertTrue(report["admitted"])
        self.assertFalse(report["header_matches"])

    def test_amsterdam_block_checks_both_inclusion_limits(self):
        request = json.loads(
            (EXAMPLES / "amsterdam-dynamic-fee-block.json").read_text()
        )
        request["transactions"][0]["tx"]["gas"] = hex(130_000)
        request["block_gas_limit"] = 217_920
        self.assertTrue(self.check(request)["admitted"])
        # Transaction two has room in execution but is short one unit of state gas.
        request["block_gas_limit"] -= 1
        self.assertFalse(self.check(request)["admitted"])

        request["accounts"][0]["storage"] = [["0x0", "0x1"]]
        # A cold no-op SSTORE uses 17,105 execution gas and zero state gas.
        request["block_gas_limit"] = 137_105
        request["header_gas_used"] = 34_210
        self.assertTrue(self.check(request)["header_matches"])
        request["block_gas_limit"] -= 1
        self.assertFalse(self.check(request)["admitted"])

    def test_amsterdam_block_keeps_fees_and_nonce_after_message_revert(self):
        request = json.loads(
            (EXAMPLES / "amsterdam-dynamic-fee-block.json").read_text()
        )
        request["accounts"][0]["code"] = "0x60015f555f5ffd"
        request["transactions"][0]["tx"]["value"] = "0x11"
        request["transactions"][1]["tx"]["to"] = (
            "0x0000000000000000000000000000000000010004"
        )
        request["header_gas_used"] = 48_109
        report = self.check(request)
        self.assertTrue(report["header_matches"])
        self.assertEqual(
            [row["stop"] for row in report["receipts"]], ["revert", "success"]
        )
        self.assertEqual([row["logs"] for row in report["receipts"]], [[], []])
        self.assertEqual(report["state_gas"], 0)
        self.assertEqual(report["accounts"][0][0], 2)
        self.assertEqual(int(report["accounts"][0][1], 0), 10**12 - 5 * 48_109)
        self.assertEqual(int(report["accounts"][1][1], 0), 0)
        self.assertEqual(report["accounts"][1][3], [])

    def test_amsterdam_dynamic_fee_precompile_uses_only_execution_gas(self):
        request = json.loads((EXAMPLES / "amsterdam-dynamic-fee.json").read_text())
        request["tx"].update(
            to="0x0000000000000000000000000000000000000004",
            gas=hex(15_192),
            input="0x010203",
        )
        report = self.check(request)
        self.assertEqual(report["output"], [1, 2, 3])
        self.assertEqual((report["spent"], report["gas_used"]), (15_066, 15_192))
        self.assertEqual(report["state_gas"], 0)

        request["tx"].update(input="0x", gas=hex(15_015))
        self.assertEqual(self.check(request)["stop"], "success")
        request["tx"]["gas"] = hex(15_014)
        report = self.check(request)
        self.assertTrue(report["admitted"])
        self.assertEqual(report["stop"], "out-of-gas")
        self.assertEqual(report["state_gas"], 0)

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

    def test_amsterdam_child_success_merges_negative_ledger_after_grandchild_revert(
        self,
    ):
        request = json.loads((EXAMPLES / "amsterdam-nested-reservoir.json").read_text())
        self.assertEqual(
            self.check(request),
            [
                [16756216, 150, 0, 0, 0, 0],
                [16756212, 0, 154, 4, 0, 0],
                [16756212, 0, 154, 4, 19, 0],
                [100, 0, 0, 0, 0, 1],
                [100, 6, -6, 0, 0, 1],
                [100, 6, -6, 0, -3, 1],
                [30, 0, 0, 0, 0, 2],
                [27, 0, 0, 0, 0, 2],
                [18, 0, 9, 9, 0, 2],
                [18, 0, 9, 9, 7, 2],
                [97, 6, -6, 0, -3, 1],
                [16756209, 6, 148, 4, 16, 0],
                [16756213, 150, 0, 0, 16, 0],
            ],
        )

    def test_amsterdam_child_halt_consumes_forwarded_gas_and_releases_reservation(self):
        request = json.loads((EXAMPLES / "amsterdam-nested-reservoir.json").read_text())
        request["actions"] = [
            {"enter": {"forward": 30, "stipend": 0, "reserved": 154}},
            {"refill": 10},
            {"refund": 5},
            {"finish": "halt"},
            {"finish": "revert"},
        ]
        self.assertEqual(
            self.check(request),
            [
                [16756216, 150, 0, 0, 0, 0],
                [30, 0, 0, 0, 0, 1],
                [30, 10, -10, 0, 0, 1],
                [30, 10, -10, 0, 5, 1],
                [16756186, 150, 0, 0, 0, 0],
                None,
            ],
        )

    def test_amsterdam_storage_refills_reservoir_and_spill(self):
        request = json.loads((EXAMPLES / "amsterdam-storage-refill.json").read_text())
        for gas, reservoir in ((20000, 100000), (120000, 0)):
            with self.subTest(gas=gas, reservoir=reservoir):
                request["world"]["gas"] = gas
                request["reservoir"] = reservoir
                report = self.check(request)
                self.assertEqual(report[:4], ["success", [0] * 32, gas - 12324, 10000])
                self.assertEqual(report[-1], [gas - 12324, reservoir, 0, 0])
                contract = next(
                    account for account in report[4] if account[0] == "65536"
                )
                self.assertEqual(contract[4], [])

    def test_amsterdam_revert_returns_spill_and_restores_storage(self):
        request = json.loads((EXAMPLES / "amsterdam-storage-refill.json").read_text())
        request.update(runtime="0x60015f555f5ffd", reservoir=0)
        request["world"]["gas"] = 120000
        report = self.check(request)
        self.assertEqual(report[:4], ["revert", [], 107891, 0])
        self.assertEqual(report[-1], [107891, 0, 0, 0])
        contract = next(account for account in report[4] if account[0] == "65536")
        self.assertEqual(contract[4], [])

    def test_amsterdam_storage_sentry_uses_execution_gas(self):
        request = json.loads((EXAMPLES / "amsterdam-storage-refill.json").read_text())
        request["runtime"] = "0x5f5f5500"
        request["world"]["gas"] = 2304
        report = self.check(request)
        self.assertEqual(report[:4], ["out-of-gas", [], 0, 0])
        self.assertEqual(report[-1], [0, 100000, 0, 0])

    def test_amsterdam_clearing_uses_warm_and_cold_execution_prices(self):
        request = json.loads((EXAMPLES / "amsterdam-storage-refill.json").read_text())
        request["runtime"] = "0x5f5f5500"
        request["world"]["gas"] = 15000
        request["world"]["accounts"] = [
            {"address": 65536, "nonce": 1, "storage": {"0": "7"}}
        ]
        for warm, cost in ((False, 12100), (True, 10100)):
            with self.subTest(warm=warm):
                request["world"]["warm_slots"] = [[65536, 0]] if warm else []
                report = self.check(request)
                self.assertEqual(report[:4], ["success", [], 15000 - 4 - cost, 11616])
                self.assertEqual(report[-1], [15000 - 4 - cost, 100000, 0, 0])
                self.assertEqual(report[4][0][4], [])
