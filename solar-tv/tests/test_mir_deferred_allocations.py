"""Deferred allocations use checked placement inputs, without backend execution."""

import unittest

import test_mir_abi as abi
import test_mir_state as state
import test_mir_storage_bytes as storage


def fixture(placement, site="0", function="f"):
    return {
        **storage.fixture(),
        "mir_layout": {"allocations": {function: {site: placement}}},
    }


class MirDeferredAllocationTests(unittest.TestCase):
    pair = state.MirStateTests.pair
    lower = abi.MirAbiTests.lower

    def test_static_placement_preserves_memory_and_fmp(self):
        before = state.module(
            "v0 = alloc 32 !metadata(deferred_alloc)\nv1 = ptrtoint ptr<memory> v0 to i256\nret v1"
        )
        initial = fixture({"kind": "static", "address": 256, "size": 32})
        self.pair(before, state.module("ret 256"), fixture=initial)
        self.pair(
            before,
            before.replace(" !metadata(deferred_alloc)", ""),
            fixture=initial,
            status=1,
        )

    def test_real_cfg_pair_and_wrong_store(self):
        before = state.module(
            "v0 = alloc 32 !metadata(deferred_alloc)\njump bb1\nbb1:\nmstore v0, 7\nv1 = mload v0\nret v1"
        )
        initial = fixture({"kind": "static", "address": 256, "size": 32})
        self.pair(before, self.lower(before, "cfg-simplify"), fixture=initial)
        self.pair(before, before.replace("v0, 7", "v0, 8"), fixture=initial, status=1)

    def test_heap_placement_bumps_on_every_iteration(self):
        before = state.module(
            "jump bb1\nbb1:\nv0 = phi [bb0: 0], [bb1: v2]\n"
            "v1 = alloc 32 !metadata(deferred_alloc)\nv2 = add v0, 1\n"
            "v3: i1 = lt v2, 2\njumpi v3, bb1, bb2\nbb2:\nv4 = mload 64\nret v4"
        )
        initial = fixture({"kind": "heap", "size": 32}, site="1")
        expected = state.module("mstore 64, 192\nret 192")
        self.pair(before, expected, fixture=initial)

    def test_zero_initialization_and_size_obligations(self):
        before = state.module(
            "v0 = alloc zeroed checked 32 !metadata(deferred_alloc)\nv1 = mload v0\nret v1"
        )
        for kind in ("static", "heap"):
            placement = {"kind": kind, "size": 32}
            address = 128
            if kind == "static":
                placement["address"] = address = 256
            initial = fixture(placement)
            data = bytes.fromhex(initial["memory"]["data"].removeprefix("0x"))
            initial["memory"] = {
                "data": "0x" + data.ljust(320, b"\xff").hex(),
                "size": 320,
            }
            expected = state.module(
                ("mstore 64, 160\n" if kind == "heap" else "")
                + f"mstore {address}, 0\nret 0"
            )
            with self.subTest(kind=kind):
                self.pair(before, expected, fixture=initial)
        initial = fixture({"kind": "static", "address": 256, "size": 64})
        result, _ = self.pair(before, before, fixture=initial, status=2)
        self.assertIn("extent disagrees", result.stderr)

    def test_missing_and_invalid_sites_are_inconclusive(self):
        before = state.module("v0 = alloc 32 !metadata(deferred_alloc)\nret 0")
        result, _ = self.pair(before, before, status=2)
        self.assertIn("missing deferred allocation", result.stderr)
        for placement, site in (
            ({"kind": "static", "address": 129, "size": 32}, "0"),
            ({"kind": "static", "address": 256, "size": 65537}, "0"),
            ({"kind": "heap", "size": 32}, "00"),
            ({"kind": "heap", "size": 32}, "4096"),
        ):
            with self.subTest(placement=placement, site=site):
                self.pair(before, before, fixture=fixture(placement, site), status=2)


if __name__ == "__main__":
    unittest.main()
