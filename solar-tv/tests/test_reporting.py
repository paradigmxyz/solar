"""Failure reports distinguish concrete counterexamples from checker limits."""

import shlex
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from test_cli import CLI, ROOT, SOLAR, check

sys.path.insert(0, str(ROOT))
from validator import report
from validator.common import Program, Unsupported


class ReportingTests(unittest.TestCase):
    def test_counterexample_report(self):
        before = Program(
            ("f",), ["arg0"], "MIR.Execution", "", canonical_ir="bb0:\n  ret 1\n"
        )
        after = Program(
            ("f",), ["arg0"], "MIR.Execution", "", canonical_ir="bb0:\n  ret arg0\n"
        )
        self.assertEqual(
            report.counterexample(
                before, after, [0], "1", "0", "checker mir before after", verbose=False
            ),
            "counterexample\n"
            "  MIR function @f\n"
            "  arg0 = 0\n"
            "  before = 1\n"
            "  after  = 0\n"
            "\nDifference\n"
            "  Returned value changed\n"
            "\nChanged IR\n"
            "--- before\n+++ after\n@@ -1,2 +1,2 @@\n"
            " bb0:\n-  ret 1\n+  ret arg0\n"
            "\nReplay\n  checker mir before after\n",
        )

    def test_first_differing_observable(self):
        for before, after, expected in [
            ("bb1, stack=[1]", "bb2, stack=[2]", "Outgoing edge changed"),
            (
                "bb1, stack=[1]",
                "bb1, stack=[]",
                "Outgoing stack height changed from 1 to 0",
            ),
            (
                "bb1, stack=[1, 2, 3]",
                "bb1, stack=[1, 4, 5]",
                "Outgoing stack word 1 changed (word 0 is the top)",
            ),
            ("revert Panic(17)", "0", "Revert changed to a return"),
            ("revert(empty)", "success(empty)", "Revert changed to success(empty)"),
            ("revert Panic(17)", "revert Panic(18)", "Revert payload changed"),
            ("0", "revert Panic(17)", "Execution changed to a revert"),
            ("bb1, stack=[]", "success(empty)", "Execution outcome changed"),
            (
                "exception(invalid-instruction)",
                "success(empty)",
                "Exceptional halt behavior changed",
            ),
        ]:
            with self.subTest(before=before, after=after):
                self.assertEqual(report.difference(before, after), expected)
        program = Program(("evm-window", "2", "bb1"), [], "EVM.Execution", "")
        self.assertEqual(
            report.region(program), "EVM IR window bb0, entry stack height 2"
        )

    def test_diff_ignores_dump_paths_and_derived_comments(self):
        self.assertEqual(
            report.ir_diff(
                "; === /tmp/before.mir ===\nbb0: ; preds bb1\n  ret 1\n",
                "; === /tmp/after.mir ===\nbb0: ; preds bb2\n  ret 1\n",
                verbose=False,
            ),
            "",
        )

    def test_long_diff_has_an_explicit_limit(self):
        before = "\n".join(f"  push {i}" for i in range(100))
        after = "\n".join(f"  push {i + 100}" for i in range(100))
        full = report.ir_diff(before, after, verbose=True).splitlines()
        compact = report.ir_diff(before, after, verbose=False).splitlines()
        self.assertEqual(compact[:60], full[:60])
        self.assertEqual(
            compact[60:],
            [f"... {len(full) - 60} more diff lines. Use --verbose to show all"],
        )

    def test_inconclusive_details_and_timeouts(self):
        error = Unsupported("compiler rejected input\nfull compiler diagnostic")
        self.assertEqual(
            report.inconclusive(error, "checker", verbose=False),
            "inconclusive: compiler rejected input\n"
            "  Use --verbose for full compiler and prover diagnostics\n"
            "\nReplay\n  checker\n",
        )
        self.assertEqual(
            report.inconclusive(error, "checker", verbose=True),
            "inconclusive: compiler rejected input\n"
            "\nDiagnostics\ncompiler rejected input\nfull compiler diagnostic\n"
            "\nReplay\n  checker\n",
        )
        for command, stage in [
            (["solar", "-Zvalidate-ir=true"], "compiler validation"),
            (["lake", "env", "lean"], "Lean validation"),
        ]:
            self.assertEqual(
                report.inconclusive(
                    subprocess.TimeoutExpired(command, 3), "checker", verbose=False
                ),
                f"inconclusive: {stage} timed out after 3 seconds\n"
                "  Increase --timeout to allow more time\n\nReplay\n  checker\n",
            )

    def test_powershell_replay_quotes_arguments(self):
        before = Path("before's.mir").resolve()
        after = Path("after.mir").resolve()
        with patch.object(report.sys, "platform", "win32"):
            command = report.replay_command("mir", before, after, "solar path", 17)
        argv = [
            sys.executable,
            CLI,
            "mir",
            str(before),
            str(after),
            "--solar",
            "solar path",
            "--timeout",
            "17",
        ]
        self.assertEqual(
            command, "& " + " ".join("'" + arg.replace("'", "''") + "'" for arg in argv)
        )

    @unittest.skipIf(sys.platform == "win32", "POSIX shell replay")
    def test_reported_command_replays_paths_with_shell_characters(self):
        with tempfile.TemporaryDirectory(prefix="tv's files ") as tmp:
            folder = Path(tmp)
            paths = []
            for name in ("before", "broken"):
                path = folder / f"{name} $(never-run);.mir"
                path.write_text((ROOT / f"examples/mir/dce/{name}.mir").read_text())
                paths.append(path)
            result = check("mir", *paths)
            self.assertEqual(result.returncode, 1, result.stderr)
            command = result.stdout.rsplit("\nReplay\n  ", 1)[1].strip()
            argv = shlex.split(command)
            self.assertEqual(argv[2:4], [str(path.resolve()) for path in paths])
            replay = subprocess.run(
                argv, capture_output=True, text=True, timeout=120, check=False
            )
            self.assertEqual(replay.returncode, 1, replay.stderr)
            self.assertEqual(replay.stdout, result.stdout)

    def test_verbose_preserves_compiler_diagnostics(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "invalid.evmir"
            path.write_text("@module Invalid\nbb0:\n  pop\n  stop\n")
            result = check("evmir", path, path)
            verbose = subprocess.run(
                [CLI, "evmir", path, path, "--solar", SOLAR, "--verbose"],
                capture_output=True,
                text=True,
                timeout=120,
                check=False,
            )
        self.assertEqual(result.returncode, 2)
        self.assertEqual(verbose.returncode, 2)
        self.assertEqual(result.stdout, "")
        self.assertEqual(result.stderr.splitlines()[0], verbose.stderr.splitlines()[0])
        self.assertIn("Use --verbose", result.stderr)
        self.assertNotIn("\nDiagnostics\n", result.stderr)
        self.assertIn("\nDiagnostics\n", verbose.stderr)
        self.assertIn("empty stack", verbose.stderr)
