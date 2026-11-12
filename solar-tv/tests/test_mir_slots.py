"""Slot hashes retain scratch effects; account reads use live independent state."""

import json
import subprocess
import unittest

import test_mir_state as state
from test_cli import ROOT, SOLAR


class MirSlotTests(unittest.TestCase):
    pair = state.MirStateTests.pair

    def test_real_slot_lowering_and_wrong_parent(self):
        folder = ROOT / "examples/mir/slots"
        before = (folder / "before.mir").read_text()
        after = subprocess.check_output(
            [
                SOLAR,
                folder / "before.mir",
                "-Ogas",
                "-Zmir-pipeline=lower-mapping-slots",
                "-Zvalidate-ir=true",
            ],
            text=True,
        )
        self.assertEqual(
            after.splitlines()[1:], (folder / "after.mir").read_text().splitlines()[1:]
        )
        self.pair(before, after)
        _, record = self.pair(before, (folder / "broken.mir").read_text(), status=1)
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )

    def test_bytes_keys_memory_and_calldata(self):
        for name in ("slots-bytes-memory", "slots-bytes-calldata"):
            with self.subTest(name=name):
                folder = ROOT / "examples/mir" / name
                before = (folder / "before.mir").read_text()
                after = subprocess.check_output(
                    [
                        SOLAR,
                        folder / "before.mir",
                        "-Ogas",
                        "-Zmir-pipeline=lower-mapping-slots,lower-slices",
                        "-Zvalidate-ir=true",
                    ],
                    text=True,
                )
                self.assertEqual(
                    after.splitlines()[1:],
                    (folder / "after.mir").read_text().splitlines()[1:],
                )
                fixture = json.loads((folder / "world.json").read_text())
                self.pair(before, after, fixture=fixture)
                self.pair(
                    before,
                    before.replace(
                        "mapping_slot_bytes v0, 9", "mapping_slot_bytes v0, 8"
                    ),
                    fixture=fixture,
                    status=1,
                )

    def test_empty_key_overlap_and_wrapping_element(self):
        self.pair(
            state.module(
                "v0 = storage_array_element_slot 3, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2\nret v0"
            ),
            state.module("v0 = storage_array_data_slot 3\nv1 = sub v0, 2\nret v1"),
        )
        # FMP points into the key: the bytes are copied before the parent is written.
        prefix = "mstore 64, 160\nmstore 128, 3\nmstore 160, 0x0102030000000000000000000000000000000000000000000000000000000000\nv0 = inttoptr i256 128 to ptr<memory>\n"
        self.pair(
            state.module(prefix + "v1 = mapping_slot_bytes v0, 9\nret v1"),
            state.module(
                prefix
                + "mcopy 160, 160, 3\nmstore 163, 9\nv1 = keccak256 160, 35\nret v1"
            ),
        )
        prefix = "mstore 64, 128\nv9 = inttoptr i256 0 to ptr<calldata>\nv0 = make_calldata_slice v9, 0\n"
        self.pair(
            state.module(prefix + "v1 = mapping_slot_bytes v0, 9\nret v1"),
            state.module(prefix + "mstore 128, 9\nv1 = keccak256 128, 32\nret v1"),
        )

    def test_external_accounts_and_padding(self):
        fixture = {"accounts": [{"address": 42, "balance": 11, "code": "0x600100"}]}
        for version in ("istanbul", "berlin", "prague"):
            with self.subTest(version=version):
                before = state.module(
                    "v0 = balance 42\nv1 = extcodesize 42\nextcodecopy 42, 0, 1, 4\nv2 = add v0, v1\nret v2"
                )
                after = state.module(
                    "v0 = balance 42\nv1 = extcodesize 42\nmstore 0, 0x0100000000000000000000000000000000000000000000000000000000000000\nret 14"
                )
                self.pair(before, after, fixture=fixture, version=version)
        # Hash distinguishes an existing empty account from an absent one.
        prefix = "v0 = extcodehash 42\nv1 = extcodehash 43\n"
        self.pair(
            state.module(prefix + "ret v1"),
            state.module(prefix + "ret 0"),
            fixture={"accounts": [{"address": 42, "nonce": 1}]},
        )
        self.pair(
            state.module("v0 = balance 42\nret v0"),
            state.module("v0 = balance 42\nret 12"),
            fixture=fixture,
            status=1,
        )

    def test_empty_hash_address_truncation_and_extreme_offsets(self):
        empty_hash = 0xC5D2460186F7233C927E7DB2DCC703C0E500B653CA82273B7BFAD8045D85A470
        prefix = "v0 = extcodehash 42\n"
        self.pair(
            state.module(prefix + "ret v0"),
            state.module(prefix + f"ret {empty_hash}"),
            fixture={"accounts": [{"address": 42, "nonce": 1}]},
        )
        self.pair(
            state.module(f"v0 = balance {2**160 + 42}\nret v0"),
            state.module("v0 = balance 42\nret v0"),
            fixture={"accounts": [{"address": 42, "balance": 11}]},
        )
        maximum = 2**256 - 1
        self.pair(
            state.module(f"extcodecopy 42, 0, {maximum}, 2\nret 0"),
            state.module("v0 = extcodesize 42\nmstore 0, 0\nret 0"),
            fixture={"accounts": [{"address": 42, "code": "0x616263"}]},
        )
        self.pair(
            state.module(f"extcodecopy 43, {maximum}, {maximum}, 0\nret 0"),
            state.module("v0 = extcodesize 43\nret 0"),
        )

    def test_delegation_introspection_is_not_execution_resolution(self):
        fixture = {
            "accounts": [
                {
                    "address": 42,
                    "code": "0xef0100000000000000000000000000000000000000002b",
                },
                {"address": 43, "code": "0x600100"},
            ]
        }
        prefix = "v0 = extcodesize 42\n"
        self.pair(
            state.module(prefix + "ret v0"),
            state.module(prefix + "ret 23"),
            fixture=fixture,
            version="prague",
        )

    def test_root_code_stays_unknown_and_limits_stay_errors(self):
        for body in (
            "v0 = extcodesize 65536\nret v0",
            "v0 = extcodehash 65536\nret v0",
            "extcodecopy 65536, 0, 0, 0\nret 0",
        ):
            with self.subTest(body=body):
                result, _ = self.pair(state.module(body), state.module(body), status=2)
                self.assertIn("root runtime code is unknown", result.stderr)
        body = "extcodecopy 42, 65536, 0, 1\nret 0"
        result, _ = self.pair(state.module(body), state.module(body), status=2)
        self.assertIn("memory limit", result.stderr)
