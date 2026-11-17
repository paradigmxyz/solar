"""Tests of the loop cut-point hook (tools/certify/loop_cuts.py) and its call in generic.py.

  python3 tools/certify/test_loop_cuts.py

They need neither Lean, solar nor z3: a fake command runner stands in for solar-verify, the
generator and lake, and loop_equiv.py runs for real on testdata/loop_equiv. They pin that every
step that does not complete leaves the function without a loop_cuts entry and the run BLOCKED.
"""

import json
import pathlib
import shutil
import sys
import tempfile
import unittest
import unittest.mock

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import generic
import loop_cuts as lc

DATA = HERE / "testdata" / "loop_equiv"
SEL = 0xB20EB4C4


def bump_ast(name="bump", n=1, contract="Bump"):
    params = [{"name": f"a{i}"} for i in range(n)]
    fn = {
        "nodeType": "FunctionDefinition",
        "kind": "function",
        "name": name,
        "functionSelector": f"{SEL:08x}",
        "parameters": {"parameters": params},
    }
    c = {
        "nodeType": "ContractDefinition",
        "contractKind": "contract",
        "name": contract,
        "nodes": [fn],
    }
    return {"nodeType": "SourceUnit", "nodes": [c]}


class Fake:
    """A command runner: solar-verify, the generator (lake env lean --run) and lake build, each
    answering per FAIL; loop_equiv.py runs for real."""

    def __init__(self, root, fail=(), manifest=None, gen_line=None):
        self.root, self.fail, self.calls = root, set(fail), []
        self.manifest = manifest or json.loads(
            (DATA / "Bump.loop-heads.json").read_text()
        )
        self.gen_line = gen_line

    def __call__(self, cmd, cwd=None, env=None):
        self.calls.append(cmd)
        if cmd[0].endswith("solar-verify"):
            if "--emit-loop-heads" in cmd:
                if "emit" in self.fail:
                    return 1, "", "z3 died"
                pathlib.Path(cmd[-1]).write_text(json.dumps(self.manifest))
                return 0, "", ""
            return (1, "", "differs") if "check" in self.fail else (0, "", "")
        if cmd[0] == sys.executable:
            if "cut" in self.fail:
                return 2, "", "loop_equiv: refused"
            return generic.run(cmd)
        if cmd[:4] == ["lake", "env", "lean", "--run"]:
            if "gen" in self.fail:
                return 2, "", "no measure"
            name = pathlib.Path(cmd[4]).parent.parent.parent.name
            pre = f"{name}/Loops/bump/"
            mods = [pre + m for m in ("Data", "V0", "Obl")]
            if "nowrite" not in self.fail:
                for m in mods:
                    p = pathlib.Path(cwd) / "Certified" / f"{m}.lean"
                    p.parent.mkdir(parents=True, exist_ok=True)
                    p.write_text(f"-- {m}\n")
            line = self.gen_line or json.dumps(
                {"format": lc.GEN_FORMAT, "modules": mods, "measure": "fun s => s.hw 1"}
            )
            return 0, line + "\n", ""
        if cmd[:2] == ["lake", "build"]:
            return (1, "error: V0", "") if "build" in self.fail else (0, "", "")
        raise AssertionError(f"unexpected command {cmd}")


class Hook(unittest.TestCase):
    def setUp(self):
        self.tmp = pathlib.Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp)
        self.clone = self.tmp / "clone"
        for f in (lc.CUT_FILE, lc.GEN_FILE):
            (self.clone / f).parent.mkdir(parents=True, exist_ok=True)
            (self.clone / f).write_text("-- stand-in\n")

    def stages(self, solar, tmp):
        g = tmp / "gen" / "Bump"
        g.mkdir(parents=True)
        shutil.copy(DATA / "Bump.Stage0.lean", g / "Stage0.lean")
        return "", ""

    def go(self, fake, ast=None, z3="/z3", clone=None):
        return lc.hook(
            run=fake,
            solar=lambda: "solar",
            verify=lambda: "/bin/solar-verify",
            z3=z3,
            src=DATA / "Bump.sol",
            ast=ast or bump_ast(),
            contract="Bump",
            name="bump_00000000",
            work=self.tmp / "work",
            stages=self.stages,
            lean_dir=clone or (lambda: self.clone),
            lake_env=lambda d: {},
            log=lambda m: None,
        )

    def test_all_steps_record_the_cut(self):
        heads, cuts, blocked = self.go(Fake(self.tmp))
        self.assertEqual(blocked, {})
        self.assertEqual(list(cuts), ["bump"])
        e = cuts["bump"]
        self.assertEqual(set(e), {"loop_heads", "cut", "obligations", "measure"})
        self.assertEqual(e["loop_heads"]["sha256"], heads["sha256"])
        self.assertTrue(heads["checked"])
        self.assertEqual(
            [pathlib.Path(o["path"]).name for o in e["obligations"]],
            ["Data.lean", "V0.lean", "Obl.lean"],
        )
        self.assertEqual(e["measure"], "fun s => s.hw 1")
        self.assertIn("def cut : Cut", pathlib.Path(e["cut"]["path"]).read_text())
        # The run stays BLOCKED even so, naming the open premises.
        (r,) = lc.reasons(cuts, blocked)
        self.assertEqual(r["feature"], "loop")
        self.assertIn("OPEN", r["detail"])

    def test_the_selector_and_arity_come_from_the_ast(self):
        fake = Fake(self.tmp)
        self.go(fake)
        (c,) = [c for c in fake.calls if c[0] == sys.executable]
        self.assertEqual(c[-3:-1], ["0xb20eb4c4", "1"])
        gen = (self.clone / "Certified/bump_00000000/Loops/bump/Gen.lean").read_text()
        self.assertIn("sel := 0xb20eb4c4", gen)
        self.assertIn("nargs := 1", gen)
        self.assertIn("import Verify.LoopSource.Equiv.GenObl", gen)

    def assert_blocked(self, fake, step, **kw):
        heads, cuts, blocked = self.go(fake, **kw)
        self.assertEqual(cuts, {})
        ((k, b),) = blocked.items()
        self.assertEqual(b["step"], step, b)
        for r in lc.reasons(cuts, blocked):
            self.assertEqual(r["feature"], "loop")
        return heads, k, b

    def test_each_failure_blocks_with_no_entry(self):
        for fail, step in [
            ("emit", "exporter"),
            ("check", "exporter"),
            ("cut", "cut"),
            ("gen", "obligations"),
            ("nowrite", "obligations"),
            ("build", "obligations"),
        ]:
            with self.subTest(fail=fail):
                shutil.rmtree(self.tmp / "work", ignore_errors=True)
                self.assert_blocked(Fake(self.tmp, fail={fail}), step)

    def test_no_z3_blocks_before_running(self):
        fake = Fake(self.tmp)
        _, k, _b = self.assert_blocked(fake, "exporter", z3=None)
        self.assertEqual(k, "*")
        self.assertEqual(fake.calls, [])

    def test_a_base_without_the_generator_blocks_before_any_build(self):
        (self.clone / lc.GEN_FILE).unlink()
        fake = Fake(self.tmp)
        _, _, b = self.assert_blocked(fake, "obligations")
        self.assertIn("GenObl", b["why"])
        self.assertFalse([c for c in fake.calls if c[0] == "lake"])

    def test_the_abi_must_name_one_function(self):
        self.assert_blocked(Fake(self.tmp), "abi", ast=bump_ast(name="other"))

    def test_a_malformed_generator_result_blocks(self):
        for line in [
            "no json here",
            json.dumps({"format": "other", "modules": [], "measure": "m"}),
            json.dumps(
                {"format": lc.GEN_FORMAT, "modules": ["x/Data"], "measure": "m"}
            ),
            json.dumps({"format": lc.GEN_FORMAT, "modules": [], "measure": ""}),
        ]:
            with self.subTest(line=line):
                shutil.rmtree(self.tmp / "work", ignore_errors=True)
                self.assert_blocked(Fake(self.tmp, gen_line=line), "obligations")

    def test_manifests_without_one_loop_function_per_name(self):
        m = json.loads((DATA / "Bump.loop-heads.json").read_text())
        none = dict(m, functions=[dict(f, loops=[]) for f in m["functions"]])
        self.assert_blocked(Fake(self.tmp, manifest=none), "exporter")
        two = dict(m, functions=m["functions"] * 2)
        self.assert_blocked(Fake(self.tmp, manifest=two), "exporter")
        self.assert_blocked(Fake(self.tmp, manifest=dict(m, format="v0")), "exporter")

    def test_a_clone_that_cannot_be_made_blocks(self):
        def nope():
            raise lc.Stop("obligations", "cannot clone the base")

        self.assert_blocked(Fake(self.tmp), "obligations", clone=nope)


class DriverCall(unittest.TestCase):
    """generic.py adds the hook's reasons to a loop BLOCKED and never turns it into a pass."""

    def test_reasons_extend_the_loop_block(self):
        rec = {}
        with unittest.mock.patch.object(
            lc, "hook", return_value=(None, {}, {"*": {"step": "exporter", "why": "x"}})
        ):
            out = generic.loop_cuts(
                rec,
                DATA / "Bump.sol",
                bump_ast(),
                unittest.mock.Mock(contract=None),
                pathlib.Path("/nonexistent"),
                "bump_0",
                "0" * 64,
            )
        self.assertEqual(rec["loop_cuts"], {})
        self.assertEqual(out[0]["feature"], "loop")
        self.assertIn("exporter", out[0]["detail"])

    def test_the_hook_runs_only_on_a_loop_block(self):
        src = (HERE / "generic.py").read_text()
        i = src.index("loop_resources(record, src, ast, a.contract)\n")
        self.assertIn(
            "loop_cuts(record, src, ast, a, work, name, src_sha)", src[i : i + 300]
        )


if __name__ == "__main__":
    unittest.main()
