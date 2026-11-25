"""Check lower-builtins on typed SSA, including operands outside the cut."""

import json
import os
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

LOAD = """@module Load

fn @f(arg0: i256) -> ptr<memory> {
  bb0:
    v0 = load_storage_bytes arg0
    ret v0
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

    def test_storage_loader_checks_index_phi_and_memory_stride(self):
        lowered = self.lower(LOAD)
        record = self.cli(lowered)
        self.assertEqual(record["region"]["proof"], "contextual-storage-bytes-load")
        self.assertFalse(record["whole_pass"])
        for pattern, replacement in (
            (r"( = phi [^\n]*?: )0(?=[,\]])", r"\g<1>1"),
            (r"( = mul v\d+, )32", r"\g<1>64"),
            (r"(mstore 32, )34", r"\g<1>65"),
        ):
            with self.subTest(pattern=pattern):
                self.mutated(lowered, pattern, replacement)

    def test_storage_loader_wrong_stride_replays_independently(self):
        source = state.module(
            "v0 = load_storage_bytes 3\nv1 = ptradd v0, 64\nv2 = mload v1\nret v2"
        )
        lowered = self.lower(source)
        broken, count = re.subn(r"( = mul v\d+, )32", r"\g<1>64", lowered, count=1)
        self.assertEqual(count, 1)
        world = storage.fixture({3: 67, storage.BASE: 17, storage.BASE + 1: 255})
        state.MirStateTests.pair(self, source, lowered, fixture=world)
        _, report = state.MirStateTests.pair(
            self, source, broken, fixture=world, status=1
        )
        self.assertEqual(report["evidence"], "kernel-and-rust-replay")
        self.assertEqual(
            report["witness"]["observations"], report["witness"]["native_observations"]
        )

    def test_live_storage_module_keeps_the_typed_caller(self):
        source = (
            LOAD.replace("@f(", "@load(")
            + "\n"
            + STORE[STORE.index("fn @f") :].replace("@f(", "@store(")
            + """
fn @caller(arg0: i256) -> ptr<memory> {
  bb0:
    v0 = icall @load, arg0
    icall @store, arg0, v0
    ret v0
}
"""
        )
        lowered = self.lower(source)
        audit = self.directory / "audit"
        checked = subprocess.check_output(
            [
                SOLAR,
                self.before,
                "-j1",
                "-Zmir-pipeline=lower-builtins",
                "-Zvalidate-ir=true",
                f"-Zsolar-tv={audit}",
            ],
            env={**os.environ, "SOLAR_TV": CLI, "SOLAR": SOLAR},
            text=True,
            timeout=120,
        )
        self.assertEqual(lowered, checked)
        records = [
            json.loads(path.read_text()) for path in (audit / "audits").glob("*.json")
        ]
        (module,) = (
            row for row in records if row.get("region", {}).get("kind") == "module"
        )
        self.assertEqual(module["status"], "verified")
        self.assertEqual(module["evidence"], "kernel-proof")
        self.assertEqual(module["region"]["proof"], "contextual-storage-bytes-module")
        self.assertEqual(
            module["import_correspondence"][1]["replaced"],
            {"load": "load", "store": "store"},
        )
        broken = lowered.replace("icall @store, arg0, v0", "icall @store, 4, v0")
        self.assertNotEqual(broken, lowered)
        self.after.write_text(broken)
        report = self.directory / "broken.json"
        result = subprocess.run(
            [
                CLI,
                "mir",
                self.before,
                self.after,
                "--solar",
                SOLAR,
                "--prune",
                "--report",
                report,
            ],
            text=True,
            capture_output=True,
            timeout=120,
            check=False,
        )
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(json.loads(report.read_text())["status"], "unsupported")

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
