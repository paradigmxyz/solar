"""Check real ABI encoding lowerings and independently replay a changed copy."""

import json
import re
import subprocess
import tempfile
import unittest
from pathlib import Path

import test_mir_abi as abi
import test_mir_abi_encode_recursive as recursive
import test_mir_state as state
from test_cli import CLI, SOLAR


class LowerAbiEncodeTests(unittest.TestCase):
    lower = abi.MirAbiTests.lower
    pair = state.MirStateTests.pair

    def automatic(self, before, after, status, *, function="f"):
        with tempfile.TemporaryDirectory() as directory:
            tmp = Path(directory)
            before_path, after_path = [
                tmp / f"{leg}.mir" for leg in ("before", "after")
            ]
            before_path.write_text(before)
            after_path.write_text(after)
            report = tmp / "report.json"
            result = subprocess.run(
                [
                    CLI,
                    "mir",
                    before_path,
                    after_path,
                    "--prune" if function is None else f"--function={function}",
                    "--solar",
                    SOLAR,
                    "--timeout=30",
                    "--report",
                    report,
                ],
                capture_output=True,
                text=True,
                timeout=180,
                check=False,
            )
            self.assertEqual(result.returncode, status, result.stdout + result.stderr)
            return json.loads(report.read_text())

    def test_calldata_callback_and_changed_copy(self):
        selector = 0x11223344 << 224
        before = state.module(
            f"""v0 = make_calldata_slice ptr<calldata> 0, 3
v1 = abi_encode [word, calldata_bytes], scratch, selector {selector}, args 7, v0
v2 = extract_value slice<memory>, v1, 0
v3 = extract_value slice<memory>, v1, 1
returndata v2, v3""",
            "fn @f()",
        )
        after = self.lower(before, "lower-abi-encode")
        proof = self.automatic(before, after, 0)
        self.assertEqual(proof["evidence"], "kernel-proof")
        self.assertEqual(proof["region"]["proof"], "kernel-checked-abi-encode-effects")
        changed, count = re.subn(
            r"(?m)^(\s*calldatacopy [^,]+), [^,]+,",
            r"\1, ptr<calldata> 1,",
            after,
            count=1,
        )
        self.assertEqual(count, 1)
        self.assertEqual(self.automatic(before, changed, 2)["status"], "unsupported")
        fixture = recursive.calldata(b"abcd")
        self.pair(before, after, fixture=fixture)
        _, witness = self.pair(after, changed, fixture=fixture, status=1)
        self.assertEqual(
            witness["witness"]["observations"],
            witness["witness"]["native_observations"],
        )

    def test_zero_copy_bytes_keep_payload_and_clear_padding(self):
        before = state.module(
            """v0 = alloc 64
mstore v0, 3
v1 = ptradd v0, 32
v2 = calldataload 0
mstore v1, v2
jump bb1
bb1:
v3 = abi_encode [memory_bytes], scratch, args v0
v4 = extract_value slice<memory>, v3, 0
v5 = extract_value slice<memory>, v3, 1
returndata v4, v5""",
            "fn @f()",
        )
        after = self.lower(before, "lower-abi-encode")
        proof = self.automatic(before, after, 0)
        self.assertEqual(proof["evidence"], "kernel-proof")
        self.assertEqual(
            proof["region"]["proof"], "kernel-checked-abi-encode-zero-copy"
        )
        fixture = recursive.calldata(b"abc" + bytes([255]) * 29)
        self.pair(before, after, fixture=fixture)
        changed, count = re.subn(
            r"(?m)^(\s*mstore [^,]+), 0$", rf"\1, {1 << 248}", after
        )
        self.assertEqual(count, 1)
        self.automatic(before, changed, 2)
        _, witness = self.pair(after, changed, fixture=fixture, status=1)
        self.assertEqual(
            witness["witness"]["observations"],
            witness["witness"]["native_observations"],
        )
