"""Check lower-builtins on typed SSA, including operands outside the cut."""

import json
import re
import subprocess
import tempfile
import unittest
from pathlib import Path

import test_mir_address_calls as calls
import test_mir_state as state
import test_mir_storage_bytes as storage
from test_cli import CLI, SOLAR

STORE = """@module Store

fn @f(arg0: i256, arg1: ptr<memory>) {
  bb0:
    store_storage_bytes arg0, arg1
    ret
}
"""

CALL = """@module Transfer

fn @f(arg0: i160, arg1: ptr<memory>) -> i256 {
  bb0:
    v0 = gas
    v1 = extcall arg0, arg1, gas v0
    v2 = icall returndata_bytes<>
    jumpi v1, bb1, bb2
  bb1:
    v3 = mload v2
    ret v3
  bb2:
    icall @failed
    ret 0
}

fn @failed() {
  bb0:
    revert 0, 0
}
"""


class BuiltinLoweringTests(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.directory = Path(tmp.name)
        self.before = self.directory / "before.mir"
        self.after = self.directory / "after.mir"

    def lower(self, text):
        self.before.write_text(text)
        return subprocess.check_output(
            [
                SOLAR,
                self.before,
                "-j1",
                "-Zmir-pipeline=lower-builtins",
                "-Zvalidate-ir=true",
            ],
            text=True,
            timeout=30,
        )

    def cli(self, lowered, status=0, version="cancun"):
        self.after.write_text(lowered)
        report = self.directory / "report.json"
        result = subprocess.run(
            [
                CLI,
                "mir",
                self.before,
                self.after,
                "--solar",
                SOLAR,
                "--function=f",
                f"--evm-version={version}",
                "--timeout=30",
                "--report",
                report,
            ],
            text=True,
            capture_output=True,
            check=False,
            timeout=90,
        )
        self.assertEqual(result.returncode, status, result.stdout + result.stderr)
        return json.loads(report.read_text())

    def mutated(self, lowered, pattern, replacement):
        changed, count = re.subn(
            pattern, replacement, lowered, count=1, flags=re.MULTILINE
        )
        self.assertEqual(count, 1, lowered)
        self.after.write_text(changed)
        subprocess.run(
            [SOLAR, self.after, "-j1", "-Zmir-pipeline=none", "-Zvalidate-ir=true"],
            capture_output=True,
            check=True,
            timeout=30,
        )
        self.assertEqual(self.cli(changed, status=2)["status"], "unsupported")

    def test_call_and_returndata_with_shared_gas_sample(self):
        lowered = self.lower(CALL)
        record = self.cli(lowered)
        self.assertEqual(
            record["region"]["proof"], "contextual-call-returndata-lowering"
        )
        self.assertFalse(record["whole_pass"])
        self.assertEqual(
            record["import_correspondence"][0]["unchanged_callees"], ["failed"]
        )
        self.assertTrue(
            any("shared sampled gas word" in item for item in record["assumptions"])
        )
        for pattern, replacement in (
            (r"( = call )v\d+,", r"\g<1>0,"),
            (r"(returndatacopy v\d+, v\d+, )v\d+", r"\g<1>0"),
            (r"( = ptradd arg1, )32", r"\g<1>33"),
            (r"^(    jumpi )(v\d+), (bb\d+), (bb\d+)", r"\1\2, \4, \3"),
            (r"(    revert 0, )0$", r"\g<1>1"),
        ):
            with self.subTest(pattern=pattern):
                self.mutated(lowered, pattern, replacement)

    def test_call_requires_a_modeled_fork(self):
        lowered = self.lower(CALL)
        self.assertEqual(
            self.cli(lowered, status=2, version="amsterdam")["status"], "unsupported"
        )

    def test_storage_word_loop_and_clear_phi(self):
        lowered = self.lower(STORE)
        record = self.cli(lowered)
        self.assertEqual(record["region"]["proof"], "contextual-storage-bytes-lowering")
        self.assertFalse(record["whole_pass"])
        self.assertEqual(
            record["import_correspondence"][1]["proved_helpers"],
            ["clear_storage_words"],
        )
        for pattern, replacement in (
            (r"( = div v\d+, )32", r"\g<1>31"),
            (r"( = and v\d+, )31", r"\g<1>15"),
            (r"( = phi \[bb0: arg1\], \[bb2: )v\d+\]", r"\g<1>arg1]"),
            (r"(mstore 32, )65", r"\g<1>17"),
        ):
            with self.subTest(pattern=pattern):
                self.mutated(lowered, pattern, replacement)

    def test_broken_raw_call_replays_independently(self):
        original = calls.source(object_result=True)
        lowered = storage.lower(original)
        broken, count = re.subn(
            r"( = call )(?:100000|0x186a0),", r"\g<1>0,", lowered, count=1
        )
        self.assertEqual(count, 1, lowered)
        _, report = state.MirStateTests.pair(
            self, original, broken, fixture=calls.fixture(), status=1
        )
        self.assertEqual(report["evidence"], "kernel-and-rust-replay")
        self.assertEqual(
            report["witness"]["observations"], report["witness"]["native_observations"]
        )
