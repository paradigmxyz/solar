"""Timeouts terminate the whole proof invocation, including Lake's Lean child."""

import os
import sys
import tempfile
import time
import unittest
from pathlib import Path
from subprocess import TimeoutExpired

from test_cli import ROOT

sys.path.insert(0, str(ROOT))
from validator.check import run


class ProcessTests(unittest.TestCase):
    @unittest.skipUnless(os.name == "posix", "POSIX process-group regression")
    def test_timeout_kills_prover_descendants(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            gate, marker, ready = (
                folder / name for name in ("gate", "marker", "ready")
            )
            gate.touch()
            child = (
                "from pathlib import Path; import sys,time\n"
                "gate,marker,ready=map(Path,sys.argv[1:]); ready.touch()\n"
                "while gate.exists(): time.sleep(0.01)\n"
                "marker.touch()\n"
            )
            parent = (
                "import subprocess,sys,time; from pathlib import Path\n"
                f"subprocess.Popen([sys.executable,'-c',{child!r},*sys.argv[1:]])\n"
                "while not Path(sys.argv[3]).exists(): time.sleep(0.01)\n"
                "print('prover started',flush=True); time.sleep(30)\n"
            )
            with self.assertRaises(TimeoutExpired) as error:
                run([sys.executable, "-c", parent, gate, marker, ready], folder, 2)
            self.assertIn("prover started", error.exception.output)
            gate.unlink()
            deadline = time.monotonic() + 0.3
            while time.monotonic() < deadline and not marker.exists():
                time.sleep(0.01)
            self.assertFalse(marker.exists(), "timed-out child survived its invocation")
