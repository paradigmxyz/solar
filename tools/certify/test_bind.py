"""The expected statement's export-manifest binding (final.py --manifest-json) and the run's binding
manifest (bind.py): golden text and fail-closed checks, with no Lean and no solar.

  python3 tools/certify/test_bind.py
"""

import copy
import hashlib
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import bind
from test_generic import expected_shape

NAME, C, HEX = "c_0000abcd", "C", "6080604052fe"
SRC = f"Certified/{NAME}/{NAME}.sol"
OPTS = ["-Ogas", "--evm-version", "osaka"]
ARGS = [SRC, *OPTS, "--emit=bin-runtime", f"-Zemit-lean-stages=Certified/{NAME}/gen"]
DIGEST = hashlib.sha256(bytes.fromhex(HEX)).hexdigest()
SOURCE = b"contract C {}\n"
SRC_SHA = hashlib.sha256(SOURCE).hexdigest()
NS = f"Certified.{NAME}.Final"
M = f"_root_.SolarTV.Export.{C}.Manifest"


def manifest(**over):
    m = {
        "arguments": ARGS,
        "sources": [{"path": SRC, "sha256": SRC_SHA}],
        "contract": C,
        "bytecode": HEX,
        "bytecode_sha256": DIGEST,
    }
    m.update(over)
    return m


def final_py(d, shape, man, hexcode=HEX):
    (d / "shape.json").write_text(json.dumps(shape))
    (d / "manifest.json").write_text(json.dumps(man))
    p = subprocess.run(
        [
            sys.executable,
            "-I",
            str(HERE / "final.py"),
            str(d / "shape.json"),
            "--name",
            NAME,
            "--contract",
            C,
            "--hex",
            hexcode,
            "--out",
            str(d / "Final.lean"),
            "--statement-out",
            str(d / "Statement.lean"),
            "--record",
            str(d / "final.json"),
            "--manifest-json",
            str(d / "manifest.json"),
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    return p


class ManifestBinding(unittest.TestCase):
    def setUp(self):
        self.t = tempfile.TemporaryDirectory()
        self.d = pathlib.Path(self.t.name)

    def tearDown(self):
        self.t.cleanup()

    def test_statement_binds_the_manifest(self):
        p = final_py(self.d, expected_shape(), manifest())
        self.assertEqual(p.returncode, 0, p.stderr)
        st = (self.d / "Statement.lean").read_text()
        self.assertIn(f"import {NAME}.gen.{C}.Manifest\n", st)
        self.assertIn(
            "def Statement : Prop :=\n"
            f'  {M}.sources = [("{SRC}", "{SRC_SHA}")] ∧\n'
            f"    {M}.arguments = {json.dumps(ARGS).replace(', ', ', ')} ∧\n"
            f'    {M}.bytecodeSha256 = "{DIGEST}" ∧\n'
            f"    {M}.bytecodeHex = _root_.{NS}.runtimeHex ∧\n"
            f"    _root_.{NS}.code.data.toList.map",
            st,
        )
        f = (self.d / "Final.lean").read_text()
        self.assertIn(f"refine ⟨rfl, rfl, rfl, rfl, _root_.{NS}.bound, ", f)
        rec = json.loads((self.d / "final.json").read_text())
        self.assertEqual(rec["manifest"]["arguments"], ARGS)
        self.assertEqual(rec["manifest"]["options"], OPTS)
        self.assertEqual(rec["manifest"]["bytecode_sha256"], DIGEST)
        self.assertEqual(rec["proof"], "P.published")
        self.assertIn(f"SolarTV.Export.{C}.Manifest.arguments", rec["statement"])

    def test_without_the_option_the_text_is_unchanged(self):
        (self.d / "shape.json").write_text(json.dumps(expected_shape()))
        p = subprocess.run(
            [
                sys.executable,
                "-I",
                str(HERE / "final.py"),
                str(self.d / "shape.json"),
                "--name",
                NAME,
                "--contract",
                C,
                "--hex",
                HEX,
                "--out",
                str(self.d / "F.lean"),
                "--statement-out",
                str(self.d / "S.lean"),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertNotIn("Manifest", (self.d / "S.lean").read_text())

    def test_refused(self):
        cases = {
            "temporary directory": manifest(
                arguments=[
                    SRC,
                    *OPTS,
                    "--emit=bin-runtime",
                    "-Zemit-lean-stages=/tmp/x/gen",
                ]
            ),
            "absolute source": manifest(
                arguments=[
                    f"/w/{SRC}",
                    *OPTS,
                    "--emit=bin-runtime",
                    f"-Zemit-lean-stages=Certified/{NAME}/gen",
                ]
            ),
            "path option": manifest(
                arguments=[
                    SRC,
                    "--base-path=/tmp",
                    "--emit=bin-runtime",
                    f"-Zemit-lean-stages=Certified/{NAME}/gen",
                ]
            ),
            "dotdot option": manifest(
                arguments=[
                    SRC,
                    "-I..",
                    "--emit=bin-runtime",
                    f"-Zemit-lean-stages=Certified/{NAME}/gen",
                ]
            ),
            "other source": manifest(sources=[{"path": "x.sol", "sha256": SRC_SHA}]),
            "two sources": manifest(sources=[{"path": SRC, "sha256": SRC_SHA}] * 2),
            "other contract": manifest(contract="D"),
            "other bytecode": manifest(bytecode="6080"),
            "wrong digest": manifest(bytecode_sha256="0" * 64),
        }
        for label, man in cases.items():
            with self.subTest(label):
                self.assertNotEqual(
                    final_py(self.d, expected_shape(), man).returncode, 0, label
                )
        with self.subTest("uppercase hex"):
            self.assertNotEqual(
                final_py(self.d, expected_shape(), manifest(), HEX.upper()).returncode,
                0,
            )
        with self.subTest("not the expected statement"):
            shape = {k: v for k, v in expected_shape().items() if k != "statement"}
            self.assertNotEqual(final_py(self.d, shape, manifest()).returncode, 0)


class ManifestWithOpenResources(unittest.TestCase):
    """A loop function's statement with the Resources premise and the manifest binding together:
    the Statement binds Manifest.* and carries ResourcesOpen, and the record is not CERTIFIED."""

    def test_both(self):
        with tempfile.TemporaryDirectory() as t:
            d = pathlib.Path(t)
            shape = expected_shape(kind="functional", open_premises=["resources"])
            p = final_py(d, shape, manifest())
            self.assertEqual(p.returncode, 0, p.stderr)
            st = (d / "Statement.lean").read_text()
            f = (d / "Final.lean").read_text()
            self.assertIn(f"import {NAME}.gen.{C}.Manifest\n", st)
            self.assertIn(f"  {M}.sources = [", st)
            self.assertIn(f"    {M}.arguments = ", st)
            self.assertIn(f'    {M}.bytecodeSha256 = "{DIGEST}" ∧\n', st)
            self.assertIn("def ResourcesOpen : Prop :=\n", st)
            self.assertIn(f"  assumptions := [_root_.{NS}.ResourcesOpen]\n", st)
            self.assertIn(f"refine ⟨rfl, rfl, rfl, rfl, _root_.{NS}.bound, ", f)
            self.assertIn(
                f"    have hr := hA _root_.{NS}.ResourcesOpen (List.mem_singleton_self _) c hc\n",
                f,
            )
            self.assertNotIn("certifiedAt_default", f)
            rec = json.loads((d / "final.json").read_text())
            self.assertIs(rec["strongest"], False)
            self.assertEqual(rec["open_premises"], ["resources"])
            self.assertEqual(rec["manifest"]["arguments"], ARGS)
            self.assertTrue(
                rec["statuses"]["resources"].startswith("Resources OPEN (hypothesis")
            )
            self.assertEqual(rec["statuses"]["closure"], "not closure")
            certify = (HERE.parents[1] / "scripts" / "certify").read_text()
            self.assertIn(
                'and not final.get("open_premises") and not provenance', certify
            )
            self.assertIn("manifest_args=(--manifest-json", certify)


def run_fixture(root):
    d = root / "Certified" / NAME
    (d / "gen" / C).mkdir(parents=True)
    (d / f"{NAME}.sol").write_bytes(SOURCE)
    (d / "bytecode.hex").write_text(HEX + "\n")
    (d / "gen" / C / "manifest.json").write_text(json.dumps(manifest()))
    stmt = f"{M}.arguments = [...] ∧ ..."
    record = {
        "name": NAME,
        "contract": C,
        "caller": "any",
        "source": {"file": "/x", "sha256": SRC_SHA},
        "spec": {"kind": "natspec", "sha256": SRC_SHA},
        "options": OPTS,
        "solar": {
            "commit": "a" * 40,
            "binary_commit": "a" * 40,
            "dirty": False,
            "binary_sha256": "b" * 64,
            "version": "solar",
        },
        "bytecode_sha256": DIGEST,
        "final": {
            "theorem": f"{NS}.final",
            "shape": "SolidCore.Verify.Generic.Expected",
            "statement": stmt,
            "proof": "P.published",
            "manifest": {
                "arguments": ARGS,
                "sources": [[SRC, SRC_SHA]],
                "bytecode_sha256": DIGEST,
            },
        },
    }
    consts = [
        [f"SolarTV.Export.{C}.Manifest.arguments", "def"],
        [f"{NS}.Statement", "def"],
        ["P.published", "theorem"],
        [f"{NS}.final", "theorem"],
        ["Nat", "inductive"],
    ]
    receipt = {
        "theorem": f"{NS}.final",
        "statement": f"{NS}.Statement",
        "closure": 5,
        "replayed_count": 5,
        "trusted_count": 0,
        "trusted": {},
        "unaccounted": [],
        "axioms": ["propext"],
        "sorry_present": False,
        "constants": consts,
        "closure_sha256": "c" * 64,
    }
    return d, record, receipt


class Bind(unittest.TestCase):
    def setUp(self):
        self.t = tempfile.TemporaryDirectory()
        self.d, self.rec, self.rr = run_fixture(pathlib.Path(self.t.name))

    def tearDown(self):
        self.t.cleanup()

    def go(self, rec=None, rr=None, solar="b" * 64, drv="a" * 40, env=None):
        return bind.build(rec or self.rec, self.d, rr or self.rr, solar, drv, env)

    def test_binds(self):
        b = self.go()
        self.assertEqual(
            b["producer"]["embedded_commit"], b["producer"]["driver_commit"]
        )
        self.assertEqual(b["runtime"]["bytecode_sha256"], DIGEST)
        self.assertEqual(b["inputs"]["options"], OPTS)
        self.assertEqual(b["replay"]["trusted"], 0)
        self.assertEqual(
            set(b["replay"]["modules"]),
            {f"{NAME}.gen.{C}.Manifest", f"{NAME}.Statement", "proof", f"{NAME}.Final"},
        )
        self.assertTrue(all(n == 1 for n in b["replay"]["modules"].values()))

    def refused(self, needle, **kw):
        with self.assertRaises(bind.Unbound) as e:
            self.go(**kw)
        self.assertIn(needle, str(e.exception))

    def mut(self, path, value, which="rec"):
        o = copy.deepcopy(self.rec if which == "rec" else self.rr)
        cur = o
        for k in path[:-1]:
            cur = cur[k]
        cur[path[-1]] = value
        return o

    def test_producer(self):
        self.refused("not the record's", solar="c" * 64)
        self.refused("CERTIFY_SOLAR_SHA256", env="d" * 64)
        self.refused("embedded commit", drv="e" * 40)
        self.refused(
            "embedded commit", rec=self.mut(["solar", "binary_commit"], "f" * 40)
        )
        self.refused("dirty", rec=self.mut(["solar", "dirty"], True))

    def test_inputs_and_runtime(self):
        self.refused("canonical", rec=self.mut(["options"], ["-Osize"]))
        self.refused("source", rec=self.mut(["source", "sha256"], "0" * 64))
        self.refused("bytecode", rec=self.mut(["bytecode_sha256"], "0" * 64))
        self.refused(
            "binds another bytecode",
            rec=self.mut(["final", "manifest", "bytecode_sha256"], "0" * 64),
        )
        (self.d / "gen" / C / "manifest.json").write_text(
            json.dumps(
                manifest(
                    arguments=[
                        SRC,
                        *OPTS,
                        "--emit=bin-runtime",
                        "-Zemit-lean-stages=/tmp/g",
                    ]
                )
            )
        )
        self.refused("canonical")

    def test_theorem(self):
        self.refused("any-caller", rec=self.mut(["caller"], "0x2000"))
        self.refused(
            "Generic.Expected", rec=self.mut(["final", "shape"], "Shapes.Certified")
        )
        self.refused(
            "does not bind the manifest", rec=self.mut(["final", "statement"], "True")
        )

    def test_replay(self):
        self.refused("trusts", rr=self.mut(["trusted_count"], 1, "rr"))
        self.refused("unaccounted", rr=self.mut(["unaccounted"], ["X"], "rr"))
        self.refused("cover the closure", rr=self.mut(["replayed_count"], 4, "rr"))
        self.refused("axiom", rr=self.mut(["axioms"], ["sorryAx"], "rr"))
        self.refused("sorry", rr=self.mut(["sorry_present"], True, "rr"))
        self.refused("another theorem", rr=self.mut(["theorem"], "X.final", "rr"))
        for i, mod in enumerate(["Manifest", "Statement", "proof", "Final"]):
            with self.subTest(mod):
                rr = copy.deepcopy(self.rr)
                del rr["constants"][i]
                rr["constants"].append(["Other", "def"])
                self.refused("has no constant of", rr=rr)


if __name__ == "__main__":
    unittest.main()
