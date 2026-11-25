"""Check shared allocation lowering and replay a changed helper's bytes."""

import json
import os
import re
import subprocess
import tempfile
import unittest
from pathlib import Path

import test_lower_abi_encode as abi_encode
import test_mir_abi as abi
import test_mir_abi_encode_recursive as recursive
import test_mir_state as state
from test_cli import CLI, SOLAR

CHECKED_PAIR = state.module(
    """v0 = calldataload 0
mstore 64, v0
v1 = alloc checked 64
v2 = alloc checked 96
v3 = ptrtoint ptr<memory> v2 to i256
ret v3"""
)


class LowerAllocTests(unittest.TestCase):
    lower = abi.MirAbiTests.lower
    automatic = abi_encode.LowerAbiEncodeTests.automatic
    pair = state.MirStateTests.pair

    def test_checked_helper_and_changed_panic(self):
        before = CHECKED_PAIR
        after = self.lower(before, "lower-alloc")
        module = self.automatic(before, after, 0, function=None)
        self.assertEqual(module["region"]["kind"], "module")
        self.assertEqual(module["evidence"], "kernel-proof")
        self.assertEqual(
            module["region"]["proof"], "kernel-checked-allocation-lowering"
        )
        changed, count = re.subn(r"(?m)^(\s*mstore 32, )65\b", r"\g<1>66", after)
        self.assertEqual(count, 1)
        self.automatic(before, changed, 2, function=None)
        self.pair(before, after, fixture=recursive.calldata((128).to_bytes(32, "big")))
        for cursor in (2**64 - 32, 2**256 - 32):
            with self.subTest(cursor=cursor):
                fixture = recursive.calldata(cursor.to_bytes(32, "big"))
                self.pair(before, after, fixture=fixture)
                _, record = self.pair(after, changed, status=1, fixture=fixture)
                self.assertEqual(
                    record["witness"]["observations"],
                    record["witness"]["native_observations"],
                )

    def test_live_hook_checks_the_added_helper(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, audit = root / "before.mir", root / "audit"
            source.write_text(CHECKED_PAIR)
            command = [SOLAR, source, "-j1", "-Ogas", "-Zmir-pipeline=lower-alloc"]
            env = {**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR}
            plain, checked = (
                subprocess.run(
                    [*command, *flags],
                    env=env,
                    text=True,
                    capture_output=True,
                    timeout=120,
                    check=True,
                )
                for flags in ([], [f"-Zsolar-tv={audit}"])
            )
            self.assertEqual(plain.stdout, checked.stdout)
            records = [
                json.loads(path.read_text())
                for path in (audit / "audits").glob("*.json")
            ]
            (module,) = (
                row for row in records if row.get("region", {}).get("kind") == "module"
            )
            self.assertEqual(module["status"], "verified")
            self.assertEqual(module["evidence"], "kernel-proof")
            self.assertEqual(
                module["region"]["proof"], "kernel-checked-allocation-lowering"
            )
