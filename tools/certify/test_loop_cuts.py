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
        if cmd[:6] == [*lc.LAKE, "env", "lean", "--run"]:
            if "gen" in self.fail:
                return 2, "", "no measure"
            name = pathlib.Path(cmd[-1]).parent.parent.parent.name
            pre = f"{name}/Loops/bump/"
            mods = [pre + m for m in ("Data", "V0", "Obl")]
            if "nowrite" not in self.fail:
                for m in mods:
                    p = pathlib.Path(cwd) / "Certified" / f"{m}.lean"
                    p.parent.mkdir(parents=True, exist_ok=True)
                    p.write_text(f"-- {m}\n")
            line = self.gen_line or json.dumps(
                {
                    "format": lc.GEN_FORMAT,
                    "route": "erasure",
                    "modules": mods,
                    "measure": "fun s => s.hw 1",
                }
            )
            return 0, line + "\n", ""
        if cmd[:4] == [*lc.LAKE, "build"]:
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

    def go(self, fake, ast=None, z3="/z3", clone=None, shape=None):
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
            **({"shape": shape} if shape else {}),
        )

    def test_all_steps_record_the_cut(self):
        heads, cuts, blocked = self.go(Fake(self.tmp))
        self.assertEqual(blocked, {})
        self.assertEqual(list(cuts), ["bump"])
        e = cuts["bump"]
        self.assertEqual(
            set(e), {"loop_heads", "cut", "obligations", "measure", "route"}
        )
        self.assertEqual(e["route"], "erasure")
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
        self.assertIn("erasure route", r["detail"])
        self.assertIn("GeneratedTarget", r["detail"])

    def test_the_selector_and_arity_come_from_the_ast(self):
        fake = Fake(self.tmp)
        self.go(fake)
        (c,) = [c for c in fake.calls if c[0] == sys.executable]
        self.assertEqual(c[-3:-1], ["0xb20eb4c4", "1"])
        gen = (self.clone / "Certified/bump_00000000/Loops/bump/Gen.lean").read_text()
        self.assertIn("summary? := none", gen)
        self.assertIn("fuel := 2 ^ 256 + 8", gen)
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
            json.dumps(
                {
                    "format": lc.GEN_FORMAT,
                    "route": "guess",
                    "modules": ["bump_00000000/Loops/bump/Obl"],
                    "measure": "m",
                }
            ),
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


class BodySummary(unittest.TestCase):
    """Without a body summary the generator takes the erasure route; the driver passes a summary
    only when given one."""

    SUMMARY = (
        "Verify.LoopSource.Equiv.Counter.Summary",
        "Verify.LoopSource.Equiv.CounterSummary.summary",
    )

    def test_without_a_summary_the_driver_passes_none(self):
        gen = lc.gen_driver("CounterRun", "Counter", "bump", SEL, 1)
        self.assertIn("      fuel := 2 ^ 256 + 8\n", gen)
        self.assertIn("      summary? := none }\n", gen)
        self.assertNotIn("body?", gen)
        self.assertNotIn("Summary", gen.replace("summary?", ""))

    def test_a_summary_is_imported_and_passed(self):
        gen = lc.gen_driver("CounterRun", "Counter", "bump", SEL, 1, self.SUMMARY)
        self.assertIn("import Verify.LoopSource.Equiv.Counter.Summary\n", gen)
        self.assertIn(
            "      summary? := some Verify.LoopSource.Equiv.CounterSummary.summary }\n",
            gen,
        )
        self.assertIn('"Verify.LoopSource.Equiv.Counter.Summary"]', gen)

    def test_a_malformed_summary_is_refused(self):
        for summary in [("A B", "x"), ("A", "x\ny"), ("A", 'x"'), ("", "x")]:
            with self.subTest(summary=summary), self.assertRaises(lc.Stop):
                lc.gen_driver("CounterRun", "Counter", "bump", SEL, 1, summary)

    def test_summaries_come_only_from_the_option(self):
        self.assertEqual(generic.loop_summaries([]), {})
        self.assertEqual(
            generic.loop_summaries(["bump=A.B:A.B.s"]), {"bump": ("A.B", "A.B.s")}
        )
        for bad in ["bump", "bump=A.B", "=A:B", "bump=A:B:C"]:
            with self.subTest(bad=bad), self.assertRaises(generic.Failed):
                generic.loop_summaries([bad])


class Shapes(unittest.TestCase):
    """loop_shapes refuses, by name, the loops eraseLoopStmt certainly does not cover, and leaves
    every counted for loop to the generator."""

    @staticmethod
    def ident(name, ty="uint256"):
        return {
            "nodeType": "Identifier",
            "name": name,
            "typeDescriptions": {"typeString": ty},
        }

    def inc(self, name, op="++", ty="uint256", sub=None):
        return {
            "nodeType": "ExpressionStatement",
            "expression": {
                "nodeType": "UnaryOperation",
                "operator": op,
                "subExpression": sub or self.ident(name, ty),
                "typeDescriptions": {"typeString": ty},
            },
        }

    def counted(self, body=None, cond_op="<", post=None, left=None):
        return {
            "nodeType": "ForStatement",
            "initializationExpression": {"nodeType": "VariableDeclarationStatement"},
            "condition": {
                "nodeType": "BinaryOperation",
                "operator": cond_op,
                "leftExpression": left or self.ident("i"),
                "rightExpression": self.ident("n"),
            },
            "loopExpression": post or self.inc("i"),
            "body": {"nodeType": "Block", "statements": body or [self.inc("count")]},
        }

    @staticmethod
    def cdef(*stmts, kind="function", visibility="external", node="FunctionDefinition"):
        fn = {
            "nodeType": node,
            "kind": kind,
            "name": "bump",
            "visibility": visibility,
            "body": {"nodeType": "Block", "statements": list(stmts)},
        }
        return {"nodeType": "ContractDefinition", "name": "C", "nodes": [fn]}

    def why(self, cdef):
        return [w for _, _, w in lc.loop_shapes(cdef)]

    def test_a_counted_loop_is_left_to_the_generator(self):
        self.assertEqual(self.why(self.cdef(self.counted())), [None])
        self.assertEqual(lc.shape_refusals(self.cdef(self.counted())), {})

    def test_refusals_name_the_reason(self):
        req = {
            "nodeType": "ExpressionStatement",
            "expression": {
                "nodeType": "FunctionCall",
                "expression": self.ident("require"),
                "arguments": [],
            },
        }
        idx = {
            "nodeType": "IndexAccess",
            "baseExpression": self.ident("m"),
            "indexExpression": self.ident("i"),
        }
        write = {
            "nodeType": "ExpressionStatement",
            "expression": {
                "nodeType": "Assignment",
                "operator": "=",
                "leftHandSide": idx,
            },
        }
        cases = [
            (
                {
                    "nodeType": "WhileStatement",
                    "body": {
                        "nodeType": "Block",
                        "statements": [{"nodeType": "Break"}],
                    },
                },
                "break in the loop",
            ),
            ({"nodeType": "DoWhileStatement", "body": {}}, "do-while loop"),
            (self.counted(body=[{"nodeType": "Break"}]), "break in the loop"),
            (self.counted(body=[{"nodeType": "Continue"}]), "continue in the loop"),
            (self.counted(body=[{"nodeType": "Return"}]), "return in the loop"),
            (self.counted(body=[{"nodeType": "EmitStatement"}]), "emits a log"),
            (
                self.counted(body=[{"nodeType": "RevertStatement"}]),
                "revert in the loop",
            ),
            (self.counted(body=[req]), "require in the loop"),
            (self.counted(body=[self.inc("count", "--")]), "decrement"),
            (self.counted(body=[self.inc("x", ty="uint8")]), "on a uint8"),
            (self.counted(body=[self.inc("m", sub=idx)]), "indexed or member"),
            (self.counted(body=[write]), "mapping or array element"),
            (self.counted(cond_op="<="), "is `<=`, not `counter < bound`"),
            (self.counted(left={"nodeType": "Literal"}), "not a variable"),
            (dict(self.counted(), condition=None), "without a condition"),
            (self.counted(body=[self.counted()]), "nested loop"),
        ]
        for loop, part in cases:
            with self.subTest(part=part):
                (w,) = self.why(self.cdef(loop))[:1]
                self.assertIsNotNone(w)
                self.assertIn(part, w)
                self.assertIn("bump", lc.shape_refusals(self.cdef(loop)))

    def test_two_loops_in_one_function(self):
        ws = self.why(self.cdef(self.counted(), self.counted()))
        self.assertEqual(len(ws), 2)
        for w in ws:
            self.assertIn("2 loops in bump", w)

    def test_loops_outside_an_external_body(self):
        for kw, part in [
            ({"visibility": "internal"}, "internal function bump"),
            ({"visibility": "private"}, "private function bump"),
            ({"kind": "constructor"}, "in the constructor"),
            ({"kind": "fallback"}, "in the fallback"),
            ({"node": "ModifierDefinition"}, "modifier bump"),
        ]:
            with self.subTest(part=part):
                (w,) = self.why(self.cdef(self.counted(), **kw))
                self.assertIn(part, w)

    def test_a_shape_refusal_blocks_before_any_build(self):
        tmp = pathlib.Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, tmp)
        h = Hook()
        h.tmp, h.clone = tmp, tmp / "clone"
        fake = Fake(tmp)
        _heads, cuts, blocked = h.go(fake, shape={"bump": "a while loop: x"}.get)
        self.assertEqual(cuts, {})
        self.assertEqual(blocked, {"bump": {"step": "shape", "why": "a while loop: x"}})
        self.assertTrue(
            all(c[0].endswith("solar-verify") for c in fake.calls), fake.calls
        )
        (r,) = lc.reasons(cuts, blocked)
        self.assertEqual(r["detail"], "loop cut: shape: a while loop: x")


SOLC = pathlib.Path.home() / ".cache/solc" / f"solc-{generic.SOLC_VERSION}"


@unittest.skipUnless(SOLC.exists(), "needs the pinned solc")
class EnvelopeLoops(unittest.TestCase):
    """The envelope's loop reason on solc's real AST: a counted loop names the erasure route, and a
    loop outside it names why; both still block."""

    SRC = """// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract L {
    uint256 count;

    function bump(uint256 n) external {
        uint256 i = 0;
        for (; i < n; i++) {
            count++;
        }
    }

    function spin(uint256 n) external {
        uint256 i = 0;
        while (i < n) {
            i++;
        }
    }

    function guarded(uint256 n) external {
        for (uint256 i = 0; i < n; i++) {
            require(count < 10, "big");
            count++;
        }
    }
}
"""

    def test_reasons(self):
        tmp = pathlib.Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, tmp)
        src = tmp / "L.sol"
        src.write_text(self.SRC)
        ast, contracts = generic.solc_ast(str(SOLC), src)
        with self.assertRaises(generic.Blocked) as e:
            generic.envelope(self.SRC, ast, contracts, None, None)
        loops = {
            r["detail"].split(":")[0]: r["detail"]
            for r in e.exception.reasons
            if r["feature"] == "loop"
        }
        self.assertEqual(set(loops), {"bump", "spin", "guarded"})
        self.assertIn("the source erasure's shape (eraseLoopStmt", loops["bump"])
        self.assertIn("does not apply them yet", loops["bump"])
        self.assertIn("a while loop", loops["spin"])
        self.assertIn("require in the loop", loops["guarded"])
        cdef = next(
            n for n in ast["nodes"] if n.get("nodeType") == "ContractDefinition"
        )
        # spin's while loop is the while route's (step 3w): the shape scan admits it.
        self.assertEqual(set(lc.shape_refusals(cdef)), {"guarded"})
        self.assertEqual(set(lc.while_functions(ast, "L")), {"spin"})


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
                unittest.mock.Mock(contract=None, loop_summary=None),
                pathlib.Path("/nonexistent"),
                "bump_0",
                "0" * 64,
            )
        self.assertEqual(rec["loop_cuts"], {})
        self.assertEqual(out[0]["feature"], "loop")
        self.assertIn("exporter", out[0]["detail"])

    def test_the_hook_gets_the_shape_refusals(self):
        ast = bump_ast()
        fn = ast["nodes"][0]["nodes"][0]
        fn["visibility"] = "external"
        fn["body"] = {
            "nodeType": "Block",
            "statements": [
                {"nodeType": "WhileStatement", "body": {"nodeType": "Continue"}}
            ],
        }
        with unittest.mock.patch.object(
            lc, "hook", return_value=(None, {}, {})
        ) as hook:
            generic.loop_cuts(
                {},
                DATA / "Bump.sol",
                ast,
                unittest.mock.Mock(contract=None, loop_summary=None),
                pathlib.Path("/nonexistent"),
                "bump_0",
                "0" * 64,
            )
        shape = hook.call_args.kwargs["shape"]
        self.assertIn("continue in the loop", shape("bump"))
        self.assertIsNone(shape("other"))

    def test_the_hook_runs_only_on_a_loop_block(self):
        src = (HERE / "generic.py").read_text()
        i = src.index('if any(r["feature"] == "loop" for r in b.reasons):')
        # The branch ends at its BLOCKED (the loop route holds an out of gas until loop_final ran).
        block = src[i : src.index("raise Blocked(reasons) from b", i) + 40]
        cuts = block.index("loop_cuts(record, src, ast, a, work, name, src_sha)")
        self.assertLess(
            cuts, block.index("loop_resources(record, src, ast, a.contract)")
        )
        # The loop's final theorem (loop_final.py) runs only once every loop function's
        # obligations are built, and the run stays BLOCKED whatever it gives.
        fin = block.index(
            "reasons = loop_final(record, reasons, src, ast, a, work, name)"
        )
        self.assertLess(cuts, fin)
        self.assertIn(
            'if record.get("loop_cuts") and not record.get("loop_cuts_blocked"):', block
        )
        self.assertIn("raise Blocked(reasons) from b", block[fin:])
        self.assertLess(fin, block.index("raise out_of_gas"))


# --- Step 3b: the discovered-invariant route (agent 179) -------------------------------------


class Infer(unittest.TestCase):
    def test_the_driver_module_runs_one_geninfer_step_on_the_gen_config(self):
        t = lc.infer_driver(
            "n", "Bump", "bump", SEL, 1, 'exportLeaves cfg "/x/leaves.json"'
        )
        self.assertIn("import Verify.Phase3.Route.GenInfer\n", t)
        self.assertIn("def cfg : Verify.LoopSource.Equiv.GenObl.Config :=", t)
        self.assertIn("summary? := none", t)
        self.assertTrue(
            t.rstrip().endswith(
                'Verify.Phase3.Route.GenInfer.exportLeaves cfg "/x/leaves.json"'
            )
        )
        self.assertNotIn("GenObl.run", t)

    def fake(self, d, export_rc=0, infer_rc=0, emit_rc=0):
        calls = []

        def lean_run(f):
            calls.append(("lean", pathlib.Path(f).name))
            if pathlib.Path(f).name == "InferExport.lean":
                if export_rc == 0:
                    (pathlib.Path(d) / "leaves.json").write_text("{}")
                return export_rc, "", "export refused" if export_rc else ""
            if emit_rc:
                return (
                    emit_rc,
                    "",
                    "loop obligations: refused: sequent 3 has a traced counterexample",
                )
            return (
                0,
                json.dumps(
                    {
                        "format": lc.GEN_FORMAT,
                        "route": "erasure-inferred",
                        "modules": ["n/Loops/bump/Obl"],
                        "measure": "m",
                    }
                ),
                "",
            )

        def run(cmd, env=None):
            calls.append(("run", pathlib.Path(cmd[2]).name))
            if infer_rc:
                return (
                    infer_rc,
                    json.dumps(
                        {
                            "discovery": "undecided",
                            "engines": {"spacer": {"verdict": "undecided"}},
                        }
                    ),
                    "no invariant",
                )
            out = cmd[cmd.index("--emit-candidate") + 1]
            pathlib.Path(out).write_text(
                json.dumps(
                    {"I_lean": "[]", "mu_lean": "(WExpr.arg 1)", "source": "spacer"}
                )
            )
            return 0, "{}", ""

        return calls, run, lean_run

    def test_export_infer_emit(self):
        with tempfile.TemporaryDirectory() as d:
            calls, run, lean_run = self.fake(d)
            r = lc.infer(run, lean_run, d, "n", "Bump", "bump", SEL, 1)
            self.assertEqual(
                calls,
                [
                    ("lean", "InferExport.lean"),
                    ("run", "loop_infer.py"),
                    ("lean", "InferEmit.lean"),
                ],
            )
            self.assertEqual(lc.gen_result(r["out"])[2], "erasure-inferred")
            self.assertIn(
                "emit cfg [] (WExpr.arg 1)",
                (pathlib.Path(d) / "InferEmit.lean").read_text(),
            )

    def test_each_failure_stops_with_the_diagnosis(self):
        for kw, frag in (
            ({"export_rc": 2}, "leaves export"),
            ({"infer_rc": 2}, "undecided"),
            ({"emit_rc": 2}, "GenInfer refuses"),
        ):
            with tempfile.TemporaryDirectory() as d:
                _, run, lean_run = self.fake(d, **kw)
                with self.assertRaises(lc.Stop) as e:
                    lc.infer(run, lean_run, d, "n", "Bump", "bump", SEL, 1)
                self.assertEqual(e.exception.step, "infer")
                self.assertIn(frag, e.exception.why)

    def test_the_triggers_are_the_generators_sequent_refusals(self):
        self.assertEqual(
            set(lc.INFER_TRIGGERS),
            {"no accepted measure", "the source tree disagrees with the MIR"},
        )
        # A source outside the erasure, or a cut that does not match the MIR, never reaches the route.
        self.assertFalse(
            any(
                t in "eraseLoopStmt is none" or t in "cut.check is none"
                for t in lc.INFER_TRIGGERS
            )
        )
        self.assertIn("erasure-inferred", lc.ROUTES)


class WhileRoute(Shapes):
    """The while route (step 3w): the shape scan admits a while loop whose body eraseWhile may
    cover, while_functions names it, and the loop's @custom:invariant becomes loop_infer.py
    formulas over the source header's words, closed with the words the step leaves alone."""

    def whl(self, body):
        return {
            "nodeType": "WhileStatement",
            "condition": {"nodeType": "BinaryOperation", "operator": ">"},
            "body": {"nodeType": "Block", "statements": body},
        }

    def test_a_while_loop_is_left_to_the_while_route(self):
        loop = self.whl([self.inc("a", "--"), self.inc("b")])
        cdef = self.cdef(loop)
        self.assertEqual(self.why(cdef), [None])
        ast = {"nodes": [cdef]}
        self.assertEqual(list(lc.while_functions(ast, "C")), ["bump"])
        self.assertEqual(lc.while_functions(ast, "D"), {})

    def test_while_refusals(self):
        idx = {"nodeType": "IndexAccess"}
        for body, part in [
            ([{"nodeType": "Break"}], "break in the loop"),
            (
                [self.inc("m", "--", sub=idx)],
                "`--` on something other than a uint256 local",
            ),
            (
                [self.inc("x", "--", ty="uint8")],
                "`--` on something other than a uint256 local",
            ),
            ([self.counted()], "nested loop"),
        ]:
            with self.subTest(part=part):
                w = self.why(self.cdef(self.whl(body)))[0]
                self.assertIn(part, w)
        ws = self.why(self.cdef(self.whl([self.inc("i")]), self.whl([self.inc("i")])))
        self.assertTrue(all("2 loops in bump" in w for w in ws))
        self.assertEqual(
            lc.while_functions({"nodes": [self.cdef(self.whl([]), self.whl([]))]}, "C"),
            {},
        )

    SRC = b"""contract C {
    function f(uint256 n) public pure returns (uint256) {
        uint256 i = 0;
        /// @custom:invariant i <= n
        /// @custom:decreases n - i
        while (i < n) { i++; }
        return i;
    }
}
"""

    def test_annotations_and_candidate(self):
        start = self.SRC.index(b"while")
        texts = lc.loop_annotations(self.SRC, {"src": f"{start}:20:0"})
        self.assertEqual(texts, ["i <= n"])
        # The source header of f: i, then n and _ret0 (words 1, 2, 3 over the call's word 0).
        probe = {
            "n": 1,
            "names": ["i", "n", "_ret0"],
            "steps": [["bin", "add", ["arg", 1], ["lit", 1]], ["arg", 2], ["arg", 3]],
            "inits": [["lit", 0], ["arg", 0], ["lit", 0]],
        }
        self.assertEqual(
            lc.annotated_candidate(texts, probe),
            [
                ["le", ["arg", 1], ["arg", 2]],
                ["eq", ["arg", 2], ["arg", 0]],
                ["eq", ["arg", 3], ["lit", 0]],
            ],
        )
        self.assertEqual(
            lc.annotated_candidate(["i > 3 && 2 == n"], probe)[:2],
            [["lt", ["lit", 3], ["arg", 1]], ["eq", ["lit", 2], ["arg", 2]]],
        )
        for bad in ["i <= m", "i + 1 <= n", "i <= n || i == 0"]:
            with self.subTest(bad=bad):
                self.assertIsNone(lc.annotated_candidate([bad], probe))
        self.assertEqual(lc.loop_annotations(self.SRC, {"src": "0:1:0"}), [])

    def test_while_driver(self):
        text = lc.while_driver(
            "H", "Horn", "ok_transfer", 0xFD0011A5, 1, 'probe cfg "s" "m"'
        )
        self.assertIn(f"import {lc.WHILE_MODULE}\n", text)
        self.assertIn(f"def main : IO UInt32 :=\n  {lc.WHILE_MODULE}.probe cfg", text)
        self.assertIn("summary? := none", text)


if __name__ == "__main__":
    unittest.main()
