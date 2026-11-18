"""The loop route's published statement with resources open (final.py open_premises): the
statement is Generic.Expected with the driver-fixed hypothesis ResourcesOpen as its one assumption,
the record says Resources OPEN (hypothesis) and not closure, and no record with an open premise is
ever CERTIFIED (final.py's strongest, and scripts/certify's own guard). Runs final.py as the driver
does, with no Lean; the Lean text is golden, not built.

  python3 tools/certify/test_loop_final.py
"""

import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from test_generic import expected_shape

NS = "Certified.c_1.Final"
G = "_root_.SolidCore.Verify.Generic"
CERTIFY = (HERE.parents[1] / "scripts" / "certify").read_text()


def run(shape):
    with tempfile.TemporaryDirectory() as d:
        d = pathlib.Path(d)
        (d / "shape.json").write_text(json.dumps(shape))
        p = subprocess.run(
            [
                sys.executable,
                "-I",
                str(HERE / "final.py"),
                str(d / "shape.json"),
                "--name",
                "c_1",
                "--contract",
                "C",
                "--hex",
                "6080604052",
                "--out",
                str(d / "Final.lean"),
                "--statement-out",
                str(d / "Statement.lean"),
                "--record",
                str(d / "record.json"),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        out = {"rc": p.returncode, "err": p.stderr}
        for f in ("Final.lean", "Statement.lean", "record.json"):
            if (d / f).exists():
                out[f] = (d / f).read_text()
        return out


def loop_shape(**over):
    return expected_shape(
        **{"kind": "functional", "open_premises": ["resources"], **over}
    )


RES_DEF = (
    "def ResourcesOpen : Prop :=\n"
    f"  ∀ c, _root_.{NS}.src.checked = .ok c →\n"
    f"    {G}.ResourcesAt (_root_.{NS}.src.semAt {G}.Context.default) c {G}.Context.default\n"
    f"      ({G}.runtimeCode _root_.{NS}.runtimeBytes) (V) (fun _ => 7)\n"
)


class OpenResources(unittest.TestCase):
    def test_statement_takes_resources_as_its_assumption(self):
        o = run(loop_shape())
        self.assertEqual(o["rc"], 0, o["err"])
        st = o["Statement.lean"]
        self.assertIn(RES_DEF, st)
        self.assertIn(f"  assumptions := [_root_.{NS}.ResourcesOpen]\n", st)
        self.assertIn(
            f"{G}.Expected _root_.{NS}.src _root_.{NS}.spec _root_.{NS}.runtimeBytes",
            st,
        )
        # Named hypothesis, never a hole or an axiom.
        for text in (st, o["Final.lean"].replace("#print axioms", "")):
            for bad in ("sorry", "axiom", "admit", "instance", "native_decide"):
                self.assertNotIn(bad, text)

    def test_final_uses_the_assumption_not_a_certified_proof(self):
        f = run(loop_shape())["Final.lean"]
        self.assertIn("  · rintro hA _ rfl\n", f)
        self.assertIn(
            f"    have hr := hA _root_.{NS}.ResourcesOpen (List.mem_singleton_self _) c hc\n",
            f,
        )
        self.assertIn("            resources := hr }", f)
        self.assertNotIn("certifiedAt_default", f)

    def test_record_says_open_and_not_closure(self):
        rec = json.loads(run(loop_shape())["record.json"])
        self.assertIs(rec["strongest"], False)
        self.assertEqual(rec["open_premises"], ["resources"])
        self.assertTrue(
            rec["statuses"]["resources"].startswith("Resources OPEN (hypothesis")
        )
        self.assertEqual(rec["statuses"]["lifecycle"], "OPEN")
        self.assertEqual(rec["statuses"]["closure"], "not closure")
        self.assertIn(f"{NS}.ResourcesOpen (OPEN)", rec["assumptions"])
        self.assertEqual(rec["kind"], "functional")

    def test_no_certified_record_with_the_premise(self):
        """The negative: an open premise never yields CERTIFIED, at either gate."""
        rec = json.loads(run(loop_shape())["record.json"])
        self.assertFalse(rec["strongest"])
        # scripts/certify's gate: every status must start "proved"/"not applicable" ...
        self.assertFalse(
            all(
                v.startswith(("proved", "not applicable"))
                for v in rec["statuses"].values()
            )
        )
        # ... and a record naming an open premise is refused outright.
        self.assertIn('and not final.get("open_premises") and not provenance', CERTIFY)
        self.assertIn('if [[ "$strongest" == "True" ]]; then', CERTIFY)

    def test_the_fixed_route_is_unchanged(self):
        o = run(expected_shape())
        self.assertEqual(o["rc"], 0, o["err"])
        self.assertIn("  assumptions := []\n", o["Statement.lean"])
        self.assertNotIn("ResourcesOpen", o["Statement.lean"] + o["Final.lean"])
        self.assertIn("certifiedAt_default h", o["Final.lean"])
        self.assertNotIn("open_premises", json.loads(o["record.json"])["statuses"])
        self.assertEqual(json.loads(o["record.json"])["open_premises"], [])

    def test_refused(self):
        cases = {
            "certified kind with the premise": expected_shape(
                open_premises=["resources"]
            ),
            "unknown premise": loop_shape(open_premises=["termination"]),
            "two premises": loop_shape(open_premises=["resources", "resources"]),
            "not expected": {
                k: v for k, v in loop_shape().items() if k not in ("statement",)
            },
            "functional without the premise": expected_shape(kind="functional"),
        }
        for label, shape in cases.items():
            with self.subTest(label):
                self.assertNotEqual(run(shape)["rc"], 0, label)


if __name__ == "__main__":
    unittest.main()
