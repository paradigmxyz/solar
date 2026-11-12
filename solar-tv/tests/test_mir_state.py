"""Direct typed SSA state proofs, original-MIR replay and unsupported boundaries."""

import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

from test_cli import CLI, ROOT, SOLAR

REPLAY = str(Path(SOLAR).with_name("solar-tv-replay"))


def module(body, signature="fn @f() -> i256"):
    return f"@module State\n{signature} {{\nbb0:\n{body}\n}}\n"


class MirStateTests(unittest.TestCase):
    def pair(
        self, before, after, *, fixture=None, status=0, version="cancun", solar=SOLAR
    ):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            paths = [folder / f"{name}.mir" for name in ("before", "after")]
            for path, text in zip(paths, (before, after), strict=True):
                path.write_text(text)
            world = folder / "world.json"
            world.write_text(json.dumps({"version": 1, **(fixture or {})}))
            report = folder / "audit.json"
            result = subprocess.run(
                [
                    CLI,
                    "mir",
                    *paths,
                    "--solar",
                    solar,
                    "--replay",
                    REPLAY,
                    "--function=f",
                    "--world",
                    world,
                    f"--evm-version={version}",
                    "--report",
                    report,
                    "--verbose",
                    "--timeout=45",
                ],
                env={
                    key: value
                    for key, value in os.environ.items()
                    if key != "SOLAR_TV_REPLAY"
                },
                text=True,
                capture_output=True,
                check=False,
                timeout=100,
            )
            record = json.loads(report.read_text())
        self.assertEqual(result.returncode, status, result.stdout + result.stderr)
        if status in {0, 1}:
            self.assertEqual(record["gas_policy"], "semantic-effects")
            self.assertEqual(record["world"]["backend_images"], [])
            self.assertTrue(record["world"]["fixture"]["direct_mir"])
            self.assertNotIn(
                "compiled full entry; trusted compiler backend lowering",
                record["assumptions"],
            )
        return result, record

    def test_real_storage_passes_and_broken_write(self):
        folder = ROOT / "examples/mir/storage-passes"
        before = (folder / "before.mir").read_text()
        fixture = json.loads((folder / "world.json").read_text())
        for transform in ("storage-cse", "storage-dse", "storage-cse,storage-dse"):
            with self.subTest(transform=transform):
                after = subprocess.check_output(
                    [
                        SOLAR,
                        folder / "before.mir",
                        "-Ogas",
                        f"-Zmir-pipeline={transform}",
                        "-Zvalidate-ir=true",
                    ],
                    text=True,
                )
                self.assertNotEqual(before.splitlines()[1:], after.splitlines()[1:])
                if "," in transform:
                    self.assertEqual(
                        after.splitlines()[1:],
                        (folder / "after.mir").read_text().splitlines()[1:],
                    )
                self.pair(before, after, fixture=fixture)
        result, record = self.pair(
            before, (folder / "broken.mir").read_text(), fixture=fixture, status=1
        )
        self.assertIn("Storage slot 1 changed", result.stdout)
        self.assertEqual(
            record["witness"]["observations"], record["witness"]["native_observations"]
        )

    def test_initial_memory_has_no_implicit_fmp(self):
        before = module("v0 = mload 64\nret v0")
        self.pair(before, module("mstore 64, 0\nret 0"))
        self.pair(
            before,
            module("ret 42"),
            fixture={"memory": {"size": 96, "data": "0x" + (bytes(95) + b"*").hex()}},
        )
        self.pair(module("v0 = msize\nret v0"), module("ret 0"))

    def test_live_memory_and_transient_are_observable(self):
        result, _ = self.pair(
            module("mstore8 0, 7\nret 0"), module("mstore8 0, 8\nret 0"), status=1
        )
        self.assertIn("Live memory", result.stdout)
        result, _ = self.pair(module("tstore 0, 7\nret 0"), module("ret 0"), status=1)
        self.assertIn("Transient storage slot 0", result.stdout)
        self.pair(
            module("v0 = tload 0\nret v0"),
            module("ret 7"),
            fixture={"accounts": [{"address": 65536, "transient": {"0": 7}}]},
        )

    def test_revert_exception_and_transaction_boundaries(self):
        self.pair(
            module("sstore 0, 7\ntstore 0, 9\nrevert 0, 0"), module("revert 0, 0")
        )
        self.pair(module("sstore 0, 7\ninvalid"), module("invalid"))
        self.pair(
            module("sstore 0, 7\nret 0"), module("invalid"), fixture={"static": True}
        )
        self.pair(module("tstore 0, 7\nstop"), module("stop"))
        self.pair(module("sstore 0, 7\nstop"), module("stop"), status=1)

    def test_memory_copy_is_overlap_safe_and_calldata_is_explicit(self):
        fixture = {"memory": {"data": "0x01020304"}}
        self.pair(
            module("mcopy 1, 0, 3\nv0 = mload 0\nret v0"),
            module(
                "mstore 0, "
                + str(int.from_bytes(b"\x01\x01\x02\x03" + bytes(28), "big"))
                + "\nv0 = mload 0\nret v0"
            ),
            fixture=fixture,
        )
        self.pair(
            module("calldatacopy 0, 0, 1\nv0 = mload 0\nret v0"),
            module(f"mstore 0, {42 << 248}\nv0 = mload 0\nret v0"),
            fixture={"calldata": "0x2a"},
        )

    def test_typed_arguments_casts_and_checked_arithmetic(self):
        before = module(
            "v0: i8 = add arg0, i8 0\nv1 = zext i8 v0 to i256\nret v1",
            "fn @f(arg0: i8) -> i256",
        )
        after = module("v0 = zext i8 arg0 to i256\nret v0", "fn @f(arg0: i8) -> i256")
        self.pair(before, after)
        self.pair(
            module("v0: i8 = add i8 255, i8 1\nv1 = zext i8 v0 to i256\nret v1"),
            module("ret 0"),
        )

    def test_pointer_arithmetic_wraps_without_provenance_assumptions(self):
        before = module(
            f"v0 = inttoptr i256 {2**256 - 1} to ptr<memory>\n"
            "v1 = ptradd v0, 1\nv2 = ptrtoint ptr<memory> v1 to i256\nret v2"
        )
        self.pair(before, module("ret 0"))

    def test_checked_failure_rolls_back_and_preserves_payload(self):
        before = module(
            "sstore 0, 7\nv0 = checked_add uint8, i8 255, i8 1\n"
            "v1 = zext i8 v0 to i256\nret v1"
        )
        after = module(f"mstore 0, {0x4E487B71 << 224}\nmstore 4, 17\nrevert 0, 36")
        self.pair(before, after)
        self.pair(before, module("ret 0"), status=1)

    def test_environment_and_zero_length_memory_boundaries(self):
        self.pair(
            module("v0 = timestamp\nret v0"),
            module("ret 42"),
            fixture={"block": {"timestamp": 42}},
        )
        self.pair(
            module(f"mcopy {2**256 - 1}, {2**256 - 1}, 0\nret 0"), module("ret 0")
        )
        self.pair(module("mstore 0, 7\nstop"), module("stop"))

    def test_loop_phis_are_simultaneous(self):
        before = module(
            "jump bb1\nbb1:\nv0 = phi [bb0: 7], [bb1: v1]\nv1 = phi [bb0: 9], [bb1: v0]\nv2 = phi [bb0: 0], [bb1: v3]\nv3 = add v2, 1\nv4: i1 = lt v3, 4\njumpi v4, bb1, bb2\nbb2:\nret v0"
        )
        self.pair(before, module("ret 9"))
        self.pair(before, module("ret 7"), status=1)

    def test_model_bounds_are_never_equivalence(self):
        loop = module("jump bb1\nbb1:\njump bb1")
        for body in (loop, module("v0 = mload 65536\nret v0")):
            with self.subTest(body=body):
                result, record = self.pair(body, body, fixture={"fuel": 16}, status=2)
                self.assertEqual(record["status"], "unsupported")
                self.assertIn("limit", result.stderr)

    def test_calls_and_gas_are_explicitly_unsupported(self):
        for before, fixture in (
            (module("v0 = call 10000, 2, 0, 0, 0, 0, 0\nret v0"), {}),
            (module("ret 0"), {"gas": 100000}),
        ):
            with self.subTest(before=before, fixture=fixture):
                self.pair(before, before, fixture=fixture, status=2)

    def test_direct_mir_never_invokes_backend(self):
        with tempfile.TemporaryDirectory() as directory:
            wrapper = Path(directory) / "solar"
            wrapper.write_text(
                "#!/usr/bin/env python3\nimport os,sys\nif any(arg.startswith('--emit') for arg in sys.argv[1:]):\n sys.exit('backend must not run')\nos.execv("
                + repr(SOLAR)
                + ", ["
                + repr(SOLAR)
                + ", *sys.argv[1:]])\n"
            )
            wrapper.chmod(0o755)
            self.pair(
                module("sstore 0, 7\nret 0"),
                module("sstore 0, 7\nret 0"),
                solar=str(wrapper),
            )
