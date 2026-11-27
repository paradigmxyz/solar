"""The lifecycle certificate bound to an expected theorem (generic.py --lifecycle-bind): the
binding's checks, and the golden text of the bound Lifecycle module, SourceSame and Composed. No
Lean; testdata/lifecycle_bind/counter-bind.json names solidity-lean's Counter expected theorem
(Verify/LoopSource/Equiv/CounterCtx.lean counter_expected, n = 8).

  python3 tools/certify/test_lifecycle_bind.py
"""

import copy
import json
import pathlib
import re
import sys
import tempfile
import typing
import unittest
import unittest.mock

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import generic

BIND = json.loads(
    (HERE / "testdata" / "lifecycle_bind" / "counter-bind.json").read_text()
)
NAME = "counter_f1c87a33"
SIGS = [{"sig": "bump(uint256)", "sel": 0xB20EB4C4}]


def binding(b, ws=(8,)):
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        json.dump(b, f)
    return generic.lifecycle_binding(f.name, list(ws))


def life(bind):
    return generic.lifecycle_lean(
        f"Certified.{NAME}.Lifecycle",
        f"{NAME}.Source",
        f"Certified.{NAME}.Src.importedSourceUnit",
        "Counter",
        SIGS,
        bytes.fromhex("6080"),
        bytes.fromhex("6001"),
        bytes.fromhex("6001"),
        [8],
        [(1, 8)],
        bind=bind,
    )


class Binding(unittest.TestCase):
    def test_counter_binding_reads(self):
        b = binding(BIND)
        self.assertEqual(b["ws"], [8])
        self.assertEqual(generic.bind_modules(b)[0], "Contracts.VerifyCounter.Source")

    def test_refused(self):
        cases = {}
        for k, v in [
            ("fuel", "2 ^ 256 + 8"),
            ("contract", "x y"),
            ("init", "a\nb"),
            ("unit", ""),
        ]:
            b = copy.deepcopy(BIND)
            b[k] = v
            cases[f"{k} not a name"] = (b, (8,))
        b = copy.deepcopy(BIND)
        del b["runtime"]
        cases["missing key"] = (b, (8,))
        b = copy.deepcopy(BIND)
        b["extra"] = 1
        cases["extra key"] = (b, (8,))
        cases["other words"] = (BIND, (5,))
        for k, v in [
            ("term", "by sorry"),
            ("term", "by native_decide"),
            ("binders", "(h : False)\n"),
            ("prop", "True"),
            ("prop", "SolidCore.Verify.Generic.Expected a b other"),
            ("term", "#exit"),
            ("module", "A B"),
        ]:
            b = copy.deepcopy(BIND)
            b["expected"][k] = v
            cases[f"expected.{k} {v!r}"] = (b, (8,))
        b = copy.deepcopy(BIND)
        b["modules"] = []
        cases["no modules"] = (b, (8,))
        for label, (b, ws) in cases.items():
            with self.subTest(label), self.assertRaises(generic.Failed):
                binding(b, ws)


class Text(unittest.TestCase):
    def test_unbound_module_is_unchanged(self):
        t = life(None)
        self.assertIn("def fuel : Nat := 4096\n", t)
        self.assertIn(
            f'TypeCheck.CheckedInput.contract Certified.{NAME}.Src.importedSourceUnit "Counter"',
            t,
        )
        self.assertNotIn("fuel_same", t)

    def test_bound_module_uses_the_theorems_constants(self):
        t = life(binding(BIND))
        self.assertIn(
            "import Contracts.VerifyCounter.Source\nimport Verify.LoopSource.Equiv.CounterCtx\n"
            "import Verify.LoopSource.Equiv.CounterCtor\n"
            "import Verify.LoopSource.Equiv.Counter.gen.Counter.Bytecode\nimport Verify.Generic.Expected\n",
            t,
        )
        self.assertIn("def fuel : Nat := Contracts.VerifyCounter.Source.fuel\n", t)
        self.assertIn("def frameFuel : Nat := 4096\n", t)
        self.assertIn("  (Verify.LoopSource.Equiv.CounterCtx.src 8).checked\n", t)
        for line in (
            "theorem fuel_same : fuel = Contracts.VerifyCounter.Source.fuel := rfl\n",
            "theorem contract_same : contract? = (Verify.LoopSource.Equiv.CounterCtx.src 8).checked := rfl\n",
            "theorem init_same : init = Verify.LoopSource.Equiv.CounterCtor.counterInit := by\n  apply ByteArray.ext; decide +kernel\n",
            "theorem code_same : code = SolidCore.Verify.Generic.runtimeCode SolarTV.Export.Counter.bytecode := by\n  apply ByteArray.ext; decide +kernel\n",
            "theorem ws_same : ws = [8] := rfl\n",
        ):
            self.assertIn(line, t)
        self.assertLess(
            t.index("#print axioms statement"), t.index("theorem fuel_same")
        )
        for bad in ("sorry", "native_decide", "axiom "):
            self.assertNotIn(bad, t.replace("#print axioms", ""))

    def test_source_same(self):
        t = generic.source_same_lean(NAME, binding(BIND))
        self.assertIn(
            f"theorem unit_same : Certified.{NAME}.Src.importedSourceUnit = "
            "SolidCore.Refinement.Source.VerifyCounter.importedSourceUnit := rfl\n",
            t,
        )

    def test_composed(self):
        t = generic.composed_lean(NAME, binding(BIND))
        L = f"Certified.{NAME}.Lifecycle"
        S = "(Verify.LoopSource.Equiv.CounterCtx.src 8)"
        I = "Verify.LoopSource.Equiv.CounterCtor.counterInit"
        CODE = "(SolidCore.Verify.Generic.runtimeCode SolarTV.Export.Counter.bytecode)"
        self.assertIn(
            f"import {NAME}.Lifecycle\nimport {NAME}.SourceSame\n"
            "import Verify.LoopSource.Equiv.CounterCreated\n",
            t,
        )
        self.assertIn(
            "theorem composed {K : Nat} (hK : K ≤ Verify.LoopSource.Equiv.CounterCtx.Kmax) :\n"
            f"    SolidCore.Verify.Generic.Expected {S} "
            "(Verify.LoopSource.Equiv.CounterCtx.spec 8 K) SolarTV.Export.Counter.bytecode ∧\n"
            f"      ∃ c, {S}.checked = .ok c ∧ {S}.CreatedBy c {I} (Verify.Lifecycle.initArgs {I} [8]) ∧\n"
            f"        Verify.Lifecycle.Statement Contracts.VerifyCounter.Source.fuel c {I} {CODE} [8] {L}.storageLit ∧\n"
            f"        SolidCore.Refinement.Deploys (Verify.Lifecycle.initArgs {I} [8]) {CODE} {L}.storageLit := by\n"
            "  obtain ⟨hE, c, hc, hcb, -⟩ := Verify.LoopSource.Equiv.CounterCreated.counter_expected_created "
            "(n := 8) (Or.inr rfl) hK\n"
            f"  obtain ⟨c', hc', hl⟩ := {L}.statement\n"
            f"  rw [{L}.contract_same, hc] at hc'\n"
            "  cases hc'\n"
            f"  rw [{L}.fuel_same, {L}.init_same, {L}.code_same, {L}.ws_same] at hl\n"
            "  exact ⟨hE, c, hc, hcb, hl, hl.deploys⟩\n",
            t,
        )
        self.assertEqual(t.count("∃ c"), 1)
        self.assertNotIn("admitCtor", t)
        self.assertIn("depends on axioms: [propext, Classical.choice, Quot.sound]", t)

    def test_lifecycle_contract_is_the_sources_checked(self):
        t = life(binding(BIND))
        self.assertIn("  (Verify.LoopSource.Equiv.CounterCtx.src 8).checked\n", t)
        self.assertIn(
            "theorem contract_same : contract? = (Verify.LoopSource.Equiv.CounterCtx.src 8).checked := rfl\n",
            t,
        )
        self.assertNotIn("admitCtor", t)


class SourceModuleImport(unittest.TestCase):
    """The bound lifecycle module imports the module defining the source term (P2's box run failed
    with Unknown identifier Verify.LoopSource.Equiv.CounterCtx.src: contract? named the source, whose
    module was not imported), and the record binds that module beside the term."""

    def imports(self, t):
        return [
            l.removeprefix("import ") for l in t.splitlines() if l.startswith("import ")
        ]

    def test_counter_imports_exactly(self):
        self.assertEqual(
            self.imports(life(binding(BIND))),
            [
                "Verify.Lifecycle.CreationFrame",
                "Verify.Lifecycle.Life",
                "Verify.Lifecycle.KernelIndex",
                "SolidCore.Refinement.KernelRfl",
                "SolidCore.Refinement.KernelTable",
                f"{NAME}.Source",
                "Contracts.VerifyCounter.Source",
                "Verify.LoopSource.Equiv.CounterCtx",
                "Verify.LoopSource.Equiv.CounterCtor",
                "Verify.LoopSource.Equiv.Counter.gen.Counter.Bytecode",
                "Verify.Generic.Expected",
            ],
        )

    def test_non_default_source_module(self):
        b = copy.deepcopy(BIND)
        b["source"] = "Acme.Specs.Vault.src 8"
        b["source_module"] = "Acme.Specs.Vault"
        b["expected"]["prop"] = (
            "SolidCore.Verify.Generic.Expected (Acme.Specs.Vault.src 8) (Acme.Specs.Vault.spec 8) "
            "SolarTV.Export.Counter.bytecode"
        )
        t = life(binding(b))
        self.assertIn("Acme.Specs.Vault", self.imports(t))
        self.assertNotIn("Verify.LoopSource.Equiv.CounterCtx", self.imports(t))
        self.assertIn("  (Acme.Specs.Vault.src 8).checked\n", t)
        self.assertIn(
            "theorem contract_same : contract? = (Acme.Specs.Vault.src 8).checked := rfl\n",
            t,
        )

    def test_source_module_shared_with_another_is_imported_once(self):
        b = copy.deepcopy(BIND)
        b["source_module"] = "Contracts.VerifyCounter.Source"
        imps = self.imports(life(binding(b)))
        self.assertEqual(imps.count("Contracts.VerifyCounter.Source"), 1)

    def test_missing_and_invalid_source_module_refused(self):
        b = copy.deepcopy(BIND)
        del b["source_module"]
        with self.assertRaises(generic.Failed) as e:
            binding(b)
        self.assertIn("keys must be exactly", str(e.exception))
        for bad in ("", "A B", "A\nB", "A.", 8):
            with self.subTest(bad=bad):
                b = copy.deepcopy(BIND)
                b["source_module"] = bad
                with self.assertRaises(generic.Failed) as e:
                    binding(b)
                self.assertIn("source_module", str(e.exception))

    def test_record_binds_the_source_module(self):
        self.assertIn("source_module", generic.BIND_KEYS)
        src = (HERE / "generic.py").read_text()
        self.assertIn('life["bind"] = {k: bind[k] for k in BIND_KEYS}', src)
        self.assertIn(bind_source := '"source_module"', src)
        del bind_source

    def test_composed_and_source_same_resolve_through_their_imports(self):
        """Composed names source, contract, fuel, init and runtime, and gets them through Lifecycle
        (now importing the source's module) and the expected theorem's module; SourceSame names only
        the unit, from unit_module."""
        b = binding(BIND)
        c = generic.composed_lean(NAME, b)
        self.assertIn(f"import {NAME}.Lifecycle\n", c)
        self.assertIn("import Verify.LoopSource.Equiv.CounterCreated\n", c)
        ss = generic.source_same_lean(NAME, b)
        self.assertIn("import Contracts.VerifyCounter.Source\n", ss)


class ComposedPinnedApi(unittest.TestCase):
    """Every externally qualified constant the Composed template itself writes (not the binding's
    names) is a declaration of the pinned solidity-lean (tools/certify/solidity-lean.rev), at the
    file and namespace below; the smoke box at 822e0c6c failed on Verify.Lifecycle.Deploys, which
    does not exist (Deploys is SolidCore.Refinement's)."""

    # name -> where it is declared at solidity-lean 822e0c6c
    PINNED: typing.ClassVar[dict[str, str]] = {
        "SolidCore.Verify.Generic.runtimeCode": "Verify/Generic/Expected.lean:190 (namespace SolidCore.Verify.Generic)",
        "Verify.Lifecycle.Statement": "Verify/Lifecycle/Route.lean:101 (namespace Verify.Lifecycle)",
        "Verify.Lifecycle.initArgs": "Verify/Lifecycle/Route.lean:44 (namespace Verify.Lifecycle)",
        "SolidCore.Refinement.Deploys": "SolidCore/Refinement/Deploy.lean:25 (namespace SolidCore.Refinement)",
    }

    def test_no_stale_deploys(self):
        t = generic.composed_lean(NAME, binding(BIND))
        self.assertNotIn("Verify.Lifecycle.Deploys", t)
        self.assertIn(
            "        SolidCore.Refinement.Deploys (Verify.Lifecycle.initArgs "
            "Verify.LoopSource.Equiv.CounterCtor.counterInit [8]) "
            "(SolidCore.Verify.Generic.runtimeCode SolarTV.Export.Counter.bytecode) "
            f"Certified.{NAME}.Lifecycle.storageLit := by\n",
            t,
        )

    def test_template_constants_are_pinned(self):
        b = binding(BIND)
        t = generic.composed_lean(NAME, b)
        bound = set()
        for k in ("unit", "fuel", "init", "runtime", "source"):
            bound.add(b[k].split(" ")[0])
        for x in (
            (
                b["expected"]["binders"]
                + " "
                + b["expected"]["prop"]
                + " "
                + b["expected"]["term"]
            )
            .replace("(", " ")
            .replace(")", " ")
            .split()
        ):
            bound.add(x)
        names = set(
            re.findall(
                r"\b(?:SolidCore|Verify|Contracts|SolarTV)(?:\.[A-Za-z_][A-Za-z0-9_']*)+",
                "\n".join(
                    line for line in t.splitlines() if not line.startswith("import ")
                ),
            )
        )
        template = names - bound
        self.assertEqual(template, set(self.PINNED), sorted(template))
        self.assertEqual(
            (HERE / "solidity-lean.rev").read_text().strip(),
            "822e0c6c363bfc9fa686c3338c57743dc73cfd2e",
        )

    def test_reachable_through_imports(self):
        """Composed imports NAME.Lifecycle, which imports Verify.Lifecycle.Life, which imports
        Verify.Lifecycle.Route and through it SolidCore.Refinement.Deploy; Expected comes through
        the binding's expected.module."""
        t = generic.composed_lean(NAME, binding(BIND))
        self.assertIn(f"import {NAME}.Lifecycle\n", t)
        self.assertIn("import Verify.Lifecycle.Life\n", life(binding(BIND)))


class CreatedByControls(unittest.TestCase):
    """Driver-level negative controls for the CreatedBy conjunct, each with its registered stage
    and reason. Stages: "constructor parameters" (constructor_words: Blocked, feature constructor
    parameters), "bind" (lifecycle_binding: Failed, --lifecycle-bind …) and "lifecycle kernel" (the
    generated module's init_same, code_same, ws_same and contract_same,
    which Lean checks: a mismatch does not build, verdict outside, its log kept)."""

    TYPES = ("uint256",)

    def blocked(self, text, needle):
        with self.assertRaises(generic.Blocked) as e:
            generic.constructor_words(list(self.TYPES), text)
        (r,) = e.exception.reasons
        self.assertEqual(r["feature"], "constructor parameters")
        self.assertIn(needle, r["detail"])

    def test_args(self):
        self.blocked(None, "takes 1 argument(s)")  # missing
        self.blocked("8,9", "takes 1 argument(s)")  # extra
        self.blocked("x", "is not")  # type-invalid
        self.blocked(str(2**256), "is outside uint256")  # type-invalid range
        self.assertEqual(generic.constructor_words(list(self.TYPES), "8"), [8])

    def refused(self, b, ws, needle):
        with self.assertRaises(generic.Failed) as e:
            binding(b, ws)
        self.assertIn(needle, str(e.exception))
        self.assertTrue(
            str(e.exception).startswith("--lifecycle-bind: "), str(e.exception)
        )

    def edit(self, **kw):
        b = copy.deepcopy(BIND)
        for k, v in kw.items():
            if v is None:
                del b[k]
            else:
                b[k] = v
        return b

    def test_changed_args(self):
        self.refused(BIND, (9,), "is applied to ['8'], not this run's words [9]")
        self.refused(
            self.edit(ws=[9]), (8,), "ws [9] are not this run's constructor words [8]"
        )

    def test_missing_and_extra_binding(self):
        self.refused(self.edit(source=None), (8,), "keys must be exactly")
        self.refused(self.edit(other="x"), (8,), "keys must be exactly")

    def test_substituted_contract_refused(self):
        """A contract? other than the source's checked (the old "contract" key, or a different c)
        cannot be given: the binding has no such key (stage bind), contract? is the source's checked
        by definition, and Composed takes c from the theorem and proves the certificate's equal
        (stage lifecycle kernel: a different witness does not build)."""
        self.refused(
            self.edit(contract="Contracts.VerifyCounter.Source.contract?"),
            (8,),
            "keys must be exactly",
        )
        t = generic.composed_lean(NAME, binding(BIND))
        self.assertIn(
            f"  rw [Certified.{NAME}.Lifecycle.contract_same, hc] at hc'\n  cases hc'\n",
            t,
        )

    def test_type_invalid_binding(self):
        for k in ("source",):
            for bad in ("src (8)", "src 8 x", "src\n8", "src 0x8", "", 8):
                with self.subTest(k=k, bad=bad):
                    self.refused(self.edit(**{k: bad}), (8,), f"{k} ")

    def test_mismatched_source(self):
        self.refused(
            self.edit(source="Verify.LoopSource.Equiv.CounterCtx.src 5"),
            (8,),
            "is applied to ['5']",
        )
        self.refused(
            self.edit(source="Other.src 8"),
            (8,),
            "expected.prop is not about the bound source",
        )

    def test_mismatched_runtime(self):
        self.refused(
            self.edit(runtime="Other.bytecode"),
            (8,),
            "expected.prop is not about the bound runtime",
        )

    def test_kernel_gates_creation_runtime_source(self):
        """An init, runtime or source binding that names other bytes passes the Python checks; the
        generated modules state each against the run's own, so Lean refuses to build them and the
        kernel stage records outside with the Lifecycle log."""
        t = life(binding(BIND))
        for line in (
            "theorem init_same : init = Verify.LoopSource.Equiv.CounterCtor.counterInit := by\n",
            "theorem code_same : code = SolidCore.Verify.Generic.runtimeCode SolarTV.Export.Counter.bytecode := by\n",
            "theorem ws_same : ws = [8] := rfl\n",
        ):
            self.assertIn(line, t)
        old = generic.BUILD_LOG_DIR
        with tempfile.TemporaryDirectory() as d:
            generic.BUILD_LOG_DIR = pathlib.Path(d)
            try:
                kq = generic.lifecycle_kernel(
                    lambda mod, text: (False, "error: init_same: decide failed"),
                    f"Certified.{NAME}.Lifecycle",
                    f"{NAME}.Source",
                    f"Certified.{NAME}.Src.importedSourceUnit",
                    "Counter",
                    SIGS,
                    bytes.fromhex("6080"),
                    bytes.fromhex("6001"),
                    bytes.fromhex("6001"),
                    [8],
                    [(1, 8)],
                    [],
                    bind=binding(BIND),
                )
            finally:
                generic.BUILD_LOG_DIR = old
        self.assertEqual(
            (kq["stage"], kq["ok"], kq["verdict"]), ("kernel", False, "outside")
        )
        self.assertIn("init_same", kq["log_tail"])


class Wiring(unittest.TestCase):
    def test_driver_checks_source_then_composes(self):
        src = (HERE / "generic.py").read_text()
        i = src.index("def lifecycle_run(")
        body = src[
            i : src.index("\n# What the lifecycle tier's loop subedge leaves open", i)
        ]
        self.assertLess(
            body.index('"SourceSame"'), body.index("kq = lifecycle_kernel(")
        )
        self.assertLess(body.index("kq = lifecycle_kernel("), body.index('"Composed"'))
        self.assertIn("this run's source unit is not the expected theorem's", body)
        self.assertIn("the composed theorem does not build", body)
        self.assertEqual(body.count("build_failure("), 2)
        # The loop-resource module is built after Composed, and only when the run has a bind.
        self.assertLess(
            body.index('"Composed"'), body.index("lifecycle_loop_resources(")
        )
        self.assertIn("if opened and bind:", body)

    def test_loop_resources_build_after_composed(self):
        src = (HERE / "generic.py").read_text()
        i = src.index("def lifecycle_loop_resources(")
        body = src[i : src.index("\ndef admit_lake_jobs", i)]
        self.assertIn('"LoopResources"', body)
        self.assertEqual(body.count("build_failure("), 1)
        self.assertIn('"subedge" if ok else "failed"', body)
        self.assertIn("loop_resources.Unsupported", body)

    def test_build_failure_keeps_the_whole_log(self):
        out = "\n".join(
            [f"✔ [{i}/9] Built M{i}" for i in range(80)]
            + [
                "✖ [9/9] Building Contracts.VerifyCounter.Source",
                "INTERNAL PANIC: lean::memory_exception: excessive memory consumption detected at 'interpreter'",
                "error: Lean exited with code 134",
            ]
        )
        with tempfile.TemporaryDirectory() as d:
            log_name, tail = generic.build_failure(
                pathlib.Path(d), NAME, "SourceSame", out
            )
            self.assertEqual(log_name, f"{NAME}.SourceSame.build.log")
            self.assertEqual((pathlib.Path(d) / log_name).read_text(), out)
        self.assertEqual(len(tail.splitlines()), 60)
        self.assertIn("lean::memory_exception", tail)
        self.assertTrue(tail.endswith("error: Lean exited with code 134"))


class BuildLogs(unittest.TestCase):
    """Every failed lake build the driver runs keeps its whole log in the record directory."""

    OUT = "\n".join(
        [f"✔ [{i}/9] Built M{i}" for i in range(80)]
        + [
            "✖ [9/9] Building counter_f1c87a33.Lifecycle",
            "INTERNAL PANIC: lean::memory_exception: excessive memory consumption detected at 'interpreter'",
            "error: Lean exited with code 134",
        ]
    )

    def setUp(self):
        self.t = tempfile.TemporaryDirectory()
        self.d = pathlib.Path(self.t.name)
        self.old = generic.BUILD_LOG_DIR
        generic.BUILD_LOG_DIR = self.d

    def tearDown(self):
        generic.BUILD_LOG_DIR = self.old
        self.t.cleanup()

    def test_failing_kernel_module_keeps_its_log(self):
        """The kernel's Lifecycle and LifecycleRefute builds both fail (Lean exit 134): the stage is
        outside, and both full logs are kept with the Lifecycle tail in the result."""

        def build(mod, text):
            generic.keep_log(f"{NAME}.{mod}", self.OUT)  # lifecycle_build's own keep
            return False, self.OUT

        kq = generic.lifecycle_kernel(
            build,
            f"Certified.{NAME}.Lifecycle",
            f"{NAME}.Source",
            f"Certified.{NAME}.Src.importedSourceUnit",
            "Counter",
            SIGS,
            bytes.fromhex("6080"),
            bytes.fromhex("6001"),
            bytes.fromhex("6001"),
            [8],
            [(1, 8)],
            [],
        )
        self.assertEqual(kq["verdict"], "outside")
        self.assertEqual(
            kq["logs"],
            {
                "Lifecycle": f"{NAME}.Lifecycle.build.log",
                "LifecycleRefute": f"{NAME}.LifecycleRefute.build.log",
            },
        )
        for f in kq["logs"].values():
            self.assertEqual((self.d / f).read_text(), self.OUT)
        self.assertEqual(len(kq["log_tail"].splitlines()), 60)
        self.assertTrue(kq["log_tail"].endswith("error: Lean exited with code 134"))
        self.assertIn("lean::memory_exception", kq["refute_log_tail"])

    def test_lifecycle_build_keeps_every_failed_module(self):
        calls = []

        def fake(cmd, cwd=None, env=None):
            calls.append(cmd)
            return (
                (1, self.OUT, "")
                if cmd[:4] == [*generic.LAKE, "build"]
                else (0, "", "")
            )

        lean = self.d / "lean"
        (lean / "Certified").mkdir(parents=True)
        (lean / "lakefile.lean").write_text("")
        with (
            unittest.mock.patch.object(generic, "run", fake),
            unittest.mock.patch.object(
                generic, "declare_lean_lib", lambda *a, **k: None
            ),
        ):
            ok, _out = generic.lifecycle_build(
                lean, NAME, "-- src", "LifecycleAll", "-- t", None
            )
        self.assertFalse(ok)
        self.assertEqual(
            (self.d / f"{NAME}.LifecycleAll.build.log").read_text(), self.OUT
        )

    def test_no_dir_keeps_nothing(self):
        generic.BUILD_LOG_DIR = None
        name, tail = generic.keep_log("x", self.OUT)
        self.assertIsNone(name)
        self.assertEqual(len(tail.splitlines()), 60)

    def test_loop_cuts_failures_keep_their_logs(self):
        import loop_cuts

        self.assertIs("keep=keep_log," in (HERE / "generic.py").read_text(), True)
        loop_cuts.KEEP = generic.keep_log
        try:
            self.assertEqual(
                loop_cuts.kept(f"{NAME}.Loops.bump.Obl", self.OUT),
                f"{NAME}.Loops.bump.Obl.build.log",
            )
        finally:
            loop_cuts.KEEP = None
        self.assertEqual(
            (self.d / f"{NAME}.Loops.bump.Obl.build.log").read_text(), self.OUT
        )
        src = (HERE / "loop_cuts.py").read_text()
        for label in ("GenImports", "Gen", "Obl"):
            self.assertIn(f'kept(f"{{name}}.Loops.{{fname}}.{label}", out + err)', src)

    def test_lake_argv_pins_memory(self):
        """Every lake call passes -R and -KleanMemory first, so the lakefile's leanMemory reaches the
        generated modules whatever configuration an earlier call cached: the generated library's
        builds (lifecycle modules, the probes' first configure, lake env) at GEN_LEAN_MEMORY 12000,
        and the loop cut hook's calls at the same cap."""
        import loop_cuts

        self.assertEqual(generic.GEN_LEAN_MEMORY, 12000)
        self.assertEqual(generic.LAKE, ["lake", "-R", "-KleanMemory=12000"])
        self.assertEqual(loop_cuts.LAKE, generic.LAKE)
        calls = []

        def fake(cmd, cwd=None, env=None):
            calls.append(list(cmd))
            return 0, "", ""

        lean = self.d / "lean"
        (lean / "Certified").mkdir(parents=True)
        (lean / "lakefile.lean").write_text("")
        with (
            unittest.mock.patch.object(generic, "run", fake),
            unittest.mock.patch.object(
                generic, "declare_lean_lib", lambda *a, **k: None
            ),
        ):
            for mod in (
                "Lifecycle",
                "LifecycleRefute",
                "LifecycleAll",
                "SourceSame",
                "Composed",
            ):
                generic.lifecycle_build(lean, NAME, "-- src", mod, "-- t", None)
        self.assertEqual(
            calls,
            [
                ["lake", "-R", "-KleanMemory=12000", "build", f"{NAME}.{m}"]
                for m in (
                    "Lifecycle",
                    "LifecycleRefute",
                    "LifecycleAll",
                    "SourceSame",
                    "Composed",
                )
            ],
        )
        src = (HERE / "generic.py").read_text()
        # The first configure of each probe clone, and every other call, goes through LAKE.
        self.assertIn(
            '[*LAKE, "build", f"{name}.Source", "SolidCore.Refinement.Wide.Refines"]',
            src,
        )
        self.assertIn(
            '            *LAKE,\n            "build",\n            f"{name}.Source",',
            src,
        )
        for text in (src, (HERE / "loop_cuts.py").read_text()):
            lines = [l for l in text.splitlines() if '"lake"' in l]
            self.assertEqual(len(lines), 1, lines)
            self.assertIn("LAKE = [", lines[0])

    def test_every_lake_failure_site_keeps(self):
        src = (HERE / "generic.py").read_text()
        for label in (
            "Probe",
            "Probe.run",
            "LifeProbe",
            "LifeProbe.run",
            "LifecycleAll",
        ):
            self.assertIn(f'keep_log(f"{{name}}.{label}"', src)
        self.assertIn('keep_log(f"{name}.{mod}", out + err)', src)
        self.assertIn("BUILD_LOG_DIR = rec_dir", src)


if __name__ == "__main__":
    unittest.main()
