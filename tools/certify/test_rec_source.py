"""Tests of rec_source.py: the recursion source stage binds only its own source."""

import os
import pathlib
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
# A solidity-lean checkout with the pinned exports, when SOLIDITY_LEAN_DIR names one.
P2 = pathlib.Path(os.environ.get("SOLIDITY_LEAN_DIR") or HERE / "missing-solidity-lean")
SRC = HERE / "testdata/rec_cuts/RecSum.sol"
SOLC = pathlib.Path.home() / ".cache/solc/solc-0.8.35"


def run(*args):
    return subprocess.run([sys.executable, str(HERE / "rec_source.py"), *map(str, args)],
                          capture_output=True, text=True)


@unittest.skipUnless((P2 / "Verify/Phase3/RecSource/Ast.lean").is_file() and SOLC.is_file(),
                     "needs solidity-lean's RecSource and solc 0.8.35")
class Binding(unittest.TestCase):
    def test_own_source_binds(self):
        r = run(SRC, "--solidity-lean", P2, "--solc", SOLC)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("bound", r.stdout)

    def test_changed_body_is_refused(self):
        with tempfile.TemporaryDirectory() as d:
            mutant = pathlib.Path(d) / "RecSum.sol"
            mutant.write_text(SRC.read_text().replace("return n + sum(n - 1);", "return n + sum(n - 1) + 1;"))
            r = run(mutant, "--solidity-lean", P2, "--solc", SOLC)
            self.assertEqual(r.returncode, 1)
            self.assertIn("REFUSED", r.stderr)

    def test_comment_only_change_is_refused(self):
        # The digest is part of the module (its header), so even a source whose AST is the same is a
        # different source until the module is regenerated from it.
        with tempfile.TemporaryDirectory() as d:
            other = pathlib.Path(d) / "RecSum.sol"
            other.write_text(SRC.read_text() + "\n// trailing\n")
            r = run(other, "--solidity-lean", P2, "--solc", SOLC)
            self.assertEqual(r.returncode, 1)


if __name__ == "__main__":
    unittest.main()
