"""Tests of the lakefile.lean rule for a run's inputs commit (tools/certify/lakefile_inputs.py) and
of every place that applies it: generic.py's commit_inputs, after_run_problem and the executed-tree
rebuild (executed_problem). checkout.sh's side is in checkout_pin_test.sh.

  python3 tools/certify/test_lakefile_inputs.py
"""

import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import generic
import lakefile_inputs as li
from test_generic import CONTRACT, NAME, lean_fixture

BASE = b"-- lakefile\nlean_lib Base where\n"


def st(name=NAME, contract=CONTRACT):
    return li.lakefile_lib(name, contract).encode()


MALICIOUS = [
    "a_0000abcd\nlean_lib x",
    'a"_0000abcd',
    "a'_0000abcd",
    "a«_0000abcd",
    "a»_0000abcd",
    "a/b_0000abcd",
    "../a_0000abcd",
    "a.._0000abcd",
    "A_0000abcd",
    "a_0000abc",
    "a-b_0000abcd",
    "",
]


class Names(unittest.TestCase):
    def test_malicious_names_refused_before_rendering(self):
        for n in MALICIOUS:
            with self.subTest(name=n):
                with self.assertRaises(li.Refused):
                    li.lakefile_lib(n, "C")
                with self.assertRaises(generic.Failed):
                    generic.lakefile_lib(n, "C")
                self.assertIsNotNone(li.problem(BASE, BASE + b"x", n))

    def test_malicious_contracts_refused(self):
        for c in ["C\nx", 'C"', "C«", "C/D", "C.D", "1C", ""]:
            with self.subTest(contract=c), self.assertRaises(li.Refused):
                li.lakefile_lib(NAME, c)

    def test_inputs_dir_must_be_certified_name(self):
        self.assertEqual(li.name_of(f"Certified/{NAME}"), NAME)
        for d in [
            f"Certified/{NAME}/x",
            "Certified/../x_0000abcd",
            f"Other/{NAME}",
            "Certified/foo",
        ]:
            self.assertIsNone(li.name_of(d), d)

    def test_driver_names_fit_the_grammar(self):
        for n in ["meter_ca0b7786", "flag_4ce5cf9a", "c_1_0000abcd", "c_ab12cd34"]:
            li.check_name(n)


class Rule(unittest.TestCase):
    def test_exact_append_accepted(self):
        self.assertIsNone(li.problem(BASE, BASE + st(), NAME))
        self.assertIsNone(li.problem(BASE, BASE + st(), NAME, CONTRACT))
        self.assertIsNone(li.problem(BASE, BASE, NAME))

    def refused(self, head, needle, base=BASE, name=NAME, contract=None):
        why = li.problem(base, head, name, contract)
        self.assertIsNotNone(why)
        self.assertIn(needle, why)

    def test_changed_stanza(self):
        self.refused(
            BASE + st().replace(b"srcDir", b"srcdir"), "not the library stanza"
        )

    def test_edit_elsewhere_plus_stanza(self):
        self.refused(BASE.replace(b"Base", b"Based") + st(), "edits the base")

    def test_stanza_for_other_name(self):
        self.refused(BASE + st("zz_0000abcd"), "not the library stanza")

    def test_stanza_for_declared_name(self):
        self.refused(BASE + st() + st(), "already declares", base=BASE + st())

    def test_duplicate_stanza(self):
        self.refused(BASE + st() + st(), "not the library stanza")

    def test_extra_line_after_stanza(self):
        self.refused(BASE + st() + b"\n", "not the library stanza")

    def test_mismatched_contract(self):
        self.refused(BASE + st(contract="Other"), "names contract", contract=CONTRACT)

    def test_mismatched_dir_name(self):
        """A stanza for NAME judged as the inputs of another Certified dir."""
        self.refused(BASE + st(), "not the library stanza", name="other_0000abcd")


def commit(repo, msg):
    g = [
        "git",
        "-C",
        str(repo),
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "-c",
        "commit.gpgsign=false",
    ]
    subprocess.run([*g, "add", "-A"], check=True)
    subprocess.run([*g, "commit", "-q", "--allow-empty", "-m", msg], check=True)
    return generic.git(repo, "rev-parse", "HEAD")


class Fixture(unittest.TestCase):
    def setUp(self):
        self.t = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.t.name)
        self.repo, self.base, cache = lean_fixture(self.root)
        (self.repo / ".lake/packages").mkdir(parents=True)
        shutil.move(cache / ".lake/packages/Pkg", self.repo / ".lake/packages/Pkg")
        (self.repo / ".gitignore").write_text(".lake\n")
        self.base = commit(self.repo, "ignore")
        self.base_lf = (self.repo / "lakefile.lean").read_bytes()
        self.pin, _ = generic.pinned_packages(self.repo, self.base)

    def tearDown(self):
        self.t.cleanup()

    def inputs(self, lf):
        (self.repo / "Certified" / NAME).mkdir(parents=True, exist_ok=True)
        (self.repo / "Certified" / NAME / "S.lean").write_text("-- s\n")
        (self.repo / "lakefile.lean").write_bytes(lf)
        return commit(self.repo, "inputs")


class CommitInputs(Fixture):
    def test_exact_append_commits(self):
        generic.fresh_certified(self.repo, NAME, CONTRACT)
        (self.repo / "Certified" / NAME).mkdir(parents=True)
        (self.repo / "Certified" / NAME / "S.lean").write_text("-- s\n")
        _, changed = generic.commit_inputs(self.repo, self.base, NAME, CONTRACT)
        self.assertEqual(
            sorted(changed), sorted([f"Certified/{NAME}/S.lean", "lakefile.lean"])
        )

    def test_contract_mismatch_refused(self):
        generic.fresh_certified(self.repo, NAME, "Other")
        (self.repo / "Certified" / NAME).mkdir(parents=True)
        (self.repo / "Certified" / NAME / "S.lean").write_text("-- s\n")
        with self.assertRaises(generic.Failed) as e:
            generic.commit_inputs(self.repo, self.base, NAME, CONTRACT)
        self.assertIn("names contract", str(e.exception))

    def test_edited_lakefile_refused(self):
        (self.repo / "Certified" / NAME).mkdir(parents=True)
        (self.repo / "Certified" / NAME / "S.lean").write_text("-- s\n")
        (self.repo / "lakefile.lean").write_bytes(self.base_lf + st() + b"-- more\n")
        with self.assertRaises(generic.Failed) as e:
            generic.commit_inputs(self.repo, self.base, NAME, CONTRACT)
        self.assertIn("touches paths outside", str(e.exception))


class AfterRun(Fixture):
    def problem(self, inputs):
        return generic.after_run_problem(
            self.repo, NAME, inputs, {"solidity_lean": inputs}, self.pin
        )

    def test_exact_append_passes(self):
        self.assertIsNone(self.problem(self.inputs(self.base_lf + st())))

    def test_appended_bytes_altered_in_checkout(self):
        ins = self.inputs(self.base_lf + st())
        (self.repo / "lakefile.lean").write_bytes(
            self.base_lf + st().replace(b"roots", b"Roots")
        )
        why = self.problem(ins)
        self.assertTrue(why.startswith("executed tree: "), why)
        self.assertIn("lakefile.lean", why)

    def test_appended_bytes_altered_in_commit(self):
        """A copied checkout whose input commit was remade with altered stanza bytes."""
        ins = self.inputs(self.base_lf + st().replace(b"roots", b"Roots"))
        why = self.problem(ins)
        self.assertIn("the input commit's lakefile.lean", why)


class Reconstruction(Fixture):
    def record(self, lf, extra=()):
        d = self.root / "rec"
        paths = [f"Certified/{NAME}/S.lean", "lakefile.lean", *extra]
        for q in paths:
            (d / "inputs" / q).parent.mkdir(parents=True, exist_ok=True)
            (d / "inputs" / q).write_bytes(lf if q == "lakefile.lean" else b"-- s\n")
        want = generic.inputs_tree(self.repo, self.base, d / "inputs", paths)
        rec = {
            "name": NAME,
            "contract": CONTRACT,
            "solidity_lean": {
                "executed_tree": want,
                "base": self.base,
                "base_tree": generic.git(self.repo, "rev-parse", self.base + "^{tree}"),
                "inputs": paths,
            },
        }
        return rec, d

    def test_exact_append_rebuilds(self):
        rec, d = self.record(self.base_lf + st())
        self.assertIsNone(generic.executed_problem(rec, d, self.repo))

    def test_altered_append_refused(self):
        """The archived lakefile's appended bytes altered, with the executed tree rehashed to match."""
        rec, d = self.record(self.base_lf + st().replace(b"roots", b"Roots"))
        why = generic.executed_problem(rec, d, self.repo)
        self.assertIn("the listed generated input lakefile.lean", why)

    def test_mismatched_name_refused(self):
        rec, d = self.record(self.base_lf + st("zz_0000abcd"))
        self.assertIn(
            "not the library stanza", generic.executed_problem(rec, d, self.repo)
        )

    def test_stray_path_refused(self):
        rec, d = self.record(self.base_lf + st(), extra=["SolidCore/A.lean"])
        self.assertIn("outside Certified/", generic.executed_problem(rec, d, self.repo))


if __name__ == "__main__":
    unittest.main()
