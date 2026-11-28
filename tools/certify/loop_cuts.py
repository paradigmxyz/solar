"""Loop cut points for the generic driver (tools/certify/generic.py).

A source the envelope blocks on a loop goes through the steps below, for each function whose
loop solar-verify's manifest lists. None of them is a reason to pass: the run stays BLOCKED in
every case, and a step that fails leaves the function without a loop_cuts entry.

0. shape: a loop the source erasure (solidity-lean's `eraseLoopStmt`, or for a while loop
   `eraseWhile` through an invariant, step 3w) certainly does not cover is refused here by name,
   before any build (`loop_shapes`: do-while loops, nested loops,
   several loops in one function, break, continue, return, revert, require, logs, decrements,
   computed-slot writes, `++` on a narrow word, a condition other than `counter < bound`, and
   loops outside an external function's own body). The rest is decided by the erasure itself.

1. exporter: `solar-verify SOURCE --emit-loop-heads WORK/loops/loop-heads.json`, with z3 from
   SOLAR_VERIFY_Z3, then `--check-loop-heads` on the file it wrote. The manifest is an untrusted
   hint. The driver runs the pinned solar-verify (solar-verify.pin, heads_exporter), which writes
   the heads of every function and each failing function's status and failures: a loop function
   that solar-verify does not verify is blocked here on its own (failing_functions), and the
   other functions go on.
2. cut: loop_equiv.py writes the function's `Cut` from the manifest and the run's own
   `Stage0.lean`, exported by the run's solar with the run's options. The selector and argument
   count come from solc's AST of the source, never from the manifest. Exit 2 is a refusal.
3. obligations: in the run's clone of the solidity-lean base, a driver module written here runs
   the base's obligation generator (`GEN_MODULE`), which writes Data, V0..Vk and Obl under
   Certified/NAME/Loops/FN/. With no body summary (the default) it takes the erasure route: it
   erases the function's body with `eraseLoopStmt` at the statement fuel `LOOP_FUEL` and refuses
   a body outside the erasure's shape. lake then builds Obl, whose theorems the kernel checks:
   leaves, tree, seqs, valid and obl, and on the erasure route erased (the imported function's
   arity, erasure and tree: `GenObl.Erased`, whose `runs` is `bodyRuns_of_eraseLoop`) and same
   (the erased tree and the sequents' tree observe alike), the source and obligation premises of
   solidity-lean's `EraseLoopJoin.GeneratedTarget`. A base without `Cut.lean` or the generator
   blocks here, before any build.
3w. while route: a function whose one loop is a while loop (`while_functions`) skips GEN_MODULE's
   run: WHILE_MODULE's probe proposes the loop's data (the state at the loop, the body fuel and the
   exit words; the condition, guards and end at the header are the erasure's), the source
   invariant is the loop's @custom:invariant closed with the words the step leaves alone (when
   loop_infer.py's z3 screen accepts it), else loop_infer.py's discovery on the source loop, and
   the MIR invariant is loop_infer.py's discovery on the cut leaves against the tree of the
   source's own eraseWhile erasure. WHILE_MODULE's emit writes the modules (route
   "while-inferred"): Obl adds whileOf_eq, src_seqs and src_valid, the source premises of
   Verify.LoopSource.Invariant.Final.final_of_whileW.
4. record: loop_cuts[FN] = {loop_heads, cut, obligations, measure, route}, each file with its sha256.

Not done here: the published statement does not yet apply the join (`GeneratedTarget`) to the
run's backend stage, loop check and loop resources, so the run stays BLOCKED.
"""

import hashlib
import json
import os
import pathlib
import re
import shutil
import sys
import tempfile

import export_ns

HERE = pathlib.Path(__file__).resolve().parent

FORMAT = "solar-verify loop heads 1"
CUT_FILE = "Verify/LoopSource/Equiv/Cut.lean"
GEN_MODULE = "Verify.LoopSource.Equiv.GenObl"
GEN_FILE = GEN_MODULE.replace(".", "/") + ".lean"
GEN_FORMAT = "loop obligations 1"
# The discovered-invariant route (step 3b): when the generator finds no candidate set and measure
# whose sequents exist, loop_infer.py proposes an invariant for the leaves and the source's own
# erased tree, and this generator states the sequents over Verify.Phase3.Infer.allSeqsW.
INFER_MODULE = "Verify.Phase3.Route.GenInfer"
INFER_FILE = INFER_MODULE.replace(".", "/") + ".lean"
# The generator's refusals that the discovered-invariant route answers: no accepted measure (no
# candidate set of Inv.cands gives sequents with a measure), and a sequent with a traced
# counterexample, which is also what Inv.allSeqs gives an end that returns a loop word (haltGoals
# has no goal for a computed output word, so its sequent must be a contradiction). The route
# re-decides both: GenInfer refuses a candidate whose allSeqsW sequents a traced call refutes, and
# loop_infer.py reports a concrete run that ends differently from the erased tree as a refutation.
INFER_TRIGGERS = ("no accepted measure", "the source tree disagrees with the MIR")
# The while route (step 3w): a function whose one loop is a while loop is erased through an
# invariant (solidity-lean's Verify/LoopSource/Invariant: eraseWhile with the loop's data, checked by
# Invariant.check; the source loop's obligations Invariant.Obl as sequents, whileSeqs). This
# generator proposes the data, exports the source loop and the MIR leaves (with the tree of the
# source's own eraseWhile erasure) for loop_infer.py, and writes the obligation modules; the
# source invariant is the loop's @custom:invariant (closed with the words the step leaves alone)
# when it checks, else a discovered one.
WHILE_MODULE = "Verify.LoopSource.Invariant.GenWhile"
WHILE_FILE = WHILE_MODULE.replace(".", "/") + ".lean"
ROUTES = ("erasure", "summary", "erasure-inferred", "while-inferred")
REFUSED = 2
# The loop cut hook's lake options: -R so the lakefile is elaborated with this -K, at the generated
# library's cap (generic.GEN_LEAN_MEMORY; Counter's Obl fails the kernel at 6000).
LAKE = ["lake", "-R", "-KleanMemory=12000"]
# keep(label, output) -> (log name or None, tail): the driver's keeper of failed builds' logs
# (generic.keep_log), set by hook; None keeps none.
KEEP = None


def kept(label, out):
    return KEEP(label, out)[0] if KEEP else None


# The statement fuel the erasure route erases the body at: eraseLoopStmt needs 2^256 + 1 more for a
# loop than for its body, and Counter's body (three blocks around one loop) erases at 2^256 + 8. The
# published statement must name the same fuel.
LOOP_FUEL = "2 ^ 256 + 8"
# The generator splits the sequents' proofs over at most this many V modules.
V_FILES = 8
# A dotted Lean name: the module and constant a body summary names (GenObl.Summary).
LEAN_NAME = r"[A-Za-z_][A-Za-z0-9_']*(\.[A-Za-z_][A-Za-z0-9_']*)*"


class Stop(Exception):
    """A step that does not complete, and why: the function stays BLOCKED with no loop_cuts entry."""

    def __init__(self, step, why, log=None):
        super().__init__(f"{step}: {why}")
        self.step = step
        self.why = why
        self.log = log


def file_entry(p):
    p = pathlib.Path(p)
    return {"path": str(p), "sha256": hashlib.sha256(p.read_bytes()).hexdigest()}


def tail(text, n=300):
    return (text or "").strip()[-n:]


def loop_functions(manifest):
    """The names of the functions the manifest gives a loop, in its order. A name listed twice
    (an overload) is refused: loop_equiv.py and the ABI lookup both need one function per name."""
    if not isinstance(manifest, dict) or manifest.get("format") != FORMAT:
        fmt = manifest.get("format") if isinstance(manifest, dict) else None
        raise Stop("exporter", f"unknown manifest format {fmt!r}")
    names = [
        f["function"].split(".")[-1]
        for f in manifest.get("functions", [])
        if f.get("loops")
    ]
    dup = sorted({n for n in names if names.count(n) > 1})
    if dup:
        raise Stop("exporter", f"more than one function named {', '.join(dup)}")
    return names


def abi(ast, contract, fname):
    """The selector and argument count of CONTRACT's external function FNAME, read from solc's AST
    of the source unit (functionSelector and the parameter list), as generic.solc_ast returns it.
    It must name exactly one such function."""
    hits = [
        f
        for c in ast.get("nodes", [])
        if c.get("nodeType") == "ContractDefinition" and c.get("name") == contract
        for f in c.get("nodes", [])
        if f.get("nodeType") == "FunctionDefinition"
        and f.get("kind") == "function"
        and f.get("name") == fname
        and f.get("functionSelector")
    ]
    if len(hits) != 1:
        raise Stop(
            "abi",
            f"{fname}: {len(hits)} external functions of {contract} have this name, not one",
        )
    f = hits[0]
    return int(f["functionSelector"], 16), len(f["parameters"]["parameters"])


# loop_equiv.py's namespace of the certificate it writes, which the hook gives each function its own
# (cut_namespace): a contract with several loop functions imports every function's Cut module in one
# environment (loop_final.py's parts statement), so one fixed name would collide.
CUT_NS = "Verify.LoopSource.Equiv.Generated"


def cut_namespace(name, fname):
    """The namespace of FNAME's cut certificate in the run NAME."""
    return f"Certified.{name}.Loops.{fname}.CutCert"


def retarget_cut(path, name, fname):
    """Move loop_equiv.py's certificate at PATH into FNAME's own namespace (cut_namespace). The
    certificate is untrusted data; only its name changes."""
    text = pathlib.Path(path).read_text()
    old = (f"namespace {CUT_NS}\n", f"end {CUT_NS}\n")
    if any(text.count(o) != 1 for o in old):
        raise Stop("cut", f"loop_equiv.py's certificate is not in namespace {CUT_NS}")
    ns = cut_namespace(name, fname)
    text = text.replace(old[0], f"namespace {ns}\n").replace(old[1], f"end {ns}\n")
    pathlib.Path(path).write_text(text)


def gen_driver(name, contract, fname, sel, nargs, summary=None):
    """The module that runs the base's obligation generator for FNAME in the run's clone.

    The generator's interface (GEN_MODULE): `run (cfg : Config) : IO UInt32` writes the modules
    `<pre>Data`, `<pre>V0`..`<pre>Vk` and `<pre>Obl` under `cfg.root`. It exits 0 after printing
    one JSON line `{"format": GEN_FORMAT, "route": ..., "modules": [...], "measure": "..."}`,
    and exits REFUSED, with the reason on stderr, when cut.check, the source tree or allSeqs is
    none or no measure is found, or when the body is outside the erasure's shape (or a given
    summary does not match the imported function).

    With no SUMMARY (the default) the driver passes `summary? := none` and the generator takes
    the erasure route: `eraseLoopStmt` of the imported function's body at `fuel := LOOP_FUEL`.
    SUMMARY, when given, is (module, constant): a `GenObl.Summary` of FNAME's body (its `Br` and
    the storage-field and function shape it was proved for). The generator checks its shape
    against this run's imported contract and the generated modules prove that by `kernel_rfl`, so
    a summary for another function or source is refused."""
    pre = f"{name}/Loops/{fname}/"
    imports = gen_imports(name, contract, fname, summary)
    checked = (
        "SolidCore.Solidity.TypeCheck.CheckedInput.contract "
        f'Certified.{name}.Src.importedSourceUnit "{contract}"'
    )
    lines = [f"import {GEN_MODULE}", *(f"import {m}" for m in imports), ""]
    lines += [
        f"/-! Generated by solar's tools/certify/loop_cuts.py for `{fname}`; do not edit. -/",
        "",
        "def main : IO UInt32 :=",
        f"  {GEN_MODULE}.run",
        f"    {{ stage0 := {export_ns.of_run(name, contract)}.stage0",
        f'      stage0Name := "{export_ns.of_run(name, contract)}.stage0"',
        f"      cut := {cut_namespace(name, fname)}.cut",
        f'      cutName := "{cut_namespace(name, fname)}.cut"',
        f"      contract? := {checked}",
        f"      contractName := {json.dumps(checked)}",
        f"      sel := {sel:#010x}",
        f"      nargs := {nargs}",
        f"      imports := [{', '.join(json.dumps(m) for m in imports)}]",
        f"      pre := {json.dumps(pre)}",
        f'      ns := "Certified.{name}.Loops.{fname}"',
        '      root := "Certified"',
        f"      files := {V_FILES}",
        f"      fuel := {LOOP_FUEL}",
        f"      summary? := {'none' if summary is None else 'some ' + summary[1]} }}",
    ]
    return "\n".join(lines) + "\n"


def gen_imports(name, contract, fname, summary=None):
    """The modules Gen.lean imports besides the generator: the run's Stage0, the cut, the source,
    and the summary's module when one is given."""
    imports = [
        f"{name}.gen.{contract}.Stage0",
        f"{name}.Loops.{fname}.Cut",
        f"{name}.Source",
    ]
    if summary is not None:
        mod, const = summary
        if not (re.fullmatch(LEAN_NAME, mod) and re.fullmatch(LEAN_NAME, const)):
            raise Stop(
                "obligations",
                f"the body summary {summary!r} is not a Lean module and constant",
            )
        imports.append(mod)
    return imports


def gen_result(out):
    """The generator's JSON line: its modules (stems relative to Certified/), its measure and its
    route ("erasure" or "summary"; a generator that prints none is the summary-only one)."""
    lines = [l for l in out.splitlines() if l.strip().startswith("{")]
    if not lines:
        raise Stop("obligations", "the generator printed no result line")
    try:
        j = json.loads(lines[-1])
    except ValueError as e:
        raise Stop(
            "obligations", f"the generator's result line is not JSON: {e}"
        ) from None
    mods, measure = j.get("modules"), j.get("measure")
    if (
        j.get("format") != GEN_FORMAT
        or not isinstance(mods, list)
        or not all(isinstance(m, str) for m in mods)
        or not isinstance(measure, str)
        or not measure
    ):
        raise Stop(
            "obligations",
            f"the generator's result line is malformed: {lines[-1][:200]}",
        )
    route = j.get("route", "summary")
    if route not in ROUTES:
        raise Stop("obligations", f"the generator names an unknown route {route!r}")
    return mods, measure, route


def heads_exporter(verify):
    """The solar-verify whose --emit-loop-heads writes the manifest, for the driver: the pinned one
    (solar-verify.pin's binary, when it is the pinned SHA-256), which writes the heads of every
    function it verified and reports each function that fails in the manifest (its status and
    failures) rather than exiting 1; the run's VERIFY() when the pin is unusable, with which any
    failing spec in the file stops step 1. The manifest is an untrusted hint either way
    (loop_equiv.py reads only how many loops a function has)."""
    try:
        pin = json.loads((HERE / "solar-verify.pin").read_text())
        exe = HERE.parent.parent / (
            os.environ.get("CERTIFY_SPEC_VERIFY_BINARY") or pin["binary"]
        )
        if (
            exe.is_file()
            and hashlib.sha256(exe.read_bytes()).hexdigest() == pin["sha256"]
        ):
            return exe
    except OSError, ValueError, KeyError, TypeError:
        pass
    return verify()


def failing_functions(manifest):
    """{function name: why} for the manifest's functions whose verification did not succeed (status
    other than verified), with the failures solar-verify reported for each."""
    out = {}
    for f in manifest.get("functions", []):
        if f.get("status") != "verified":
            fails = "; ".join(str(x) for x in f.get("failures") or [])
            out[f["function"].split(".")[-1]] = (
                f"solar-verify reports {f.get('status')}"
                + (f": {fails}" if fails else "")
            )
    return out


def exporter(run, verify, z3, src, out):
    """Step 1: the manifest, written and then re-derived by solar-verify."""
    if not z3:
        raise Stop("exporter", "SOLAR_VERIFY_Z3 is not set")
    env = dict(os.environ, SOLAR_VERIFY_Z3=z3)
    rc, _, err = run([str(verify), str(src), "--emit-loop-heads", str(out)], env=env)
    if rc != 0 or not out.is_file():
        raise Stop(
            "exporter", f"solar-verify --emit-loop-heads exits {rc}: {tail(err)}"
        )
    rc, _, err = run([str(verify), str(src), "--check-loop-heads", str(out)], env=env)
    if rc != 0:
        raise Stop(
            "exporter",
            f"solar-verify --check-loop-heads rejects the manifest: {tail(err)}",
        )
    try:
        return json.loads(out.read_text())
    except ValueError as e:
        raise Stop("exporter", f"the manifest is not JSON: {e}") from None


def cut(run, heads, stage0, fname, sel, nargs, out):
    """Step 2: loop_equiv.py's certificate for FNAME at OUT."""
    rc, _, err = run(
        [
            sys.executable,
            "-I",
            str(HERE / "loop_equiv.py"),
            str(heads),
            str(stage0),
            fname,
            f"{sel:#010x}",
            str(nargs),
            str(out),
        ]
    )
    if rc == REFUSED:
        raise Stop("cut", f"loop_equiv.py refuses: {tail(err)}")
    if rc != 0 or not out.is_file():
        raise Stop("cut", f"loop_equiv.py exits {rc}: {tail(err)}")


def obligations(
    run,
    lake_env,
    lean_dir,
    name,
    contract,
    fname,
    sel,
    nargs,
    cut_file,
    stage0,
    summary=None,
    annotations=None,
):
    """Step 3 for FNAME in the prepared clone LEAN_DIR: the obligation files, the measure and the
    generator's route. ANNOTATIONS, when not None, puts FNAME on the while route (step 3w), with
    its loop's @custom:invariant texts."""
    for f in (CUT_FILE, GEN_FILE):
        if not (lean_dir / f).is_file():
            raise Stop("obligations", f"the solidity-lean base has no {f}")
    d = lean_dir / "Certified" / name
    g = d / "gen" / contract
    g.mkdir(parents=True, exist_ok=True)
    if not (g / "Stage0.lean").exists():
        shutil.copy(stage0, g / "Stage0.lean")
    fd = d / "Loops" / fname
    fd.mkdir(parents=True, exist_ok=True)
    shutil.copy(cut_file, fd / "Cut.lean")
    (fd / "Gen.lean").write_text(gen_driver(name, contract, fname, sel, nargs, summary))
    # lake env lean runs Gen.lean against built imports only, so build them first.
    deps = [GEN_MODULE, *gen_imports(name, contract, fname, summary)]
    rc, out, err = run([*LAKE, "build", *deps], cwd=lean_dir, env=lake_env)
    if rc != 0:
        raise Stop(
            "obligations",
            f"lake cannot build the generator's imports: {tail(out + err)}",
            kept(f"{name}.Loops.{fname}.GenImports", out + err),
        )
    if annotations is not None and summary is None:
        if not (lean_dir / WHILE_FILE).is_file():
            raise Stop(
                "obligations",
                f"the solidity-lean base has no {WHILE_FILE} for the while route",
            )
        rc2, out2, err2 = run(
            [*LAKE, "build", WHILE_MODULE], cwd=lean_dir, env=lake_env
        )
        if rc2 != 0:
            raise Stop(
                "obligations",
                f"lake cannot build {WHILE_MODULE}: {tail(out2 + err2)}",
                kept(f"{name}.Loops.{fname}.GenWhile", out2 + err2),
            )
        w = while_route(
            run,
            lambda f: run(
                [*LAKE, "env", "lean", "--run", str(f)], cwd=lean_dir, env=lake_env
            ),
            fd,
            name,
            contract,
            fname,
            sel,
            nargs,
            z3=lake_env.get("SOLAR_VERIFY_Z3") if lake_env else None,
            annotations=annotations,
        )
        rc, out, err = 0, w["out"], ""
        (fd / "while-route.json").write_text(
            json.dumps({k: v for k, v in w.items() if k != "out"})
        )
    else:
        rc, out, err = run(
            [*LAKE, "env", "lean", "--run", str(fd / "Gen.lean")],
            cwd=lean_dir,
            env=lake_env,
        )
    inferred = None
    if rc == REFUSED and summary is None and any(t in err for t in INFER_TRIGGERS):
        if not (lean_dir / INFER_FILE).is_file():
            raise Stop(
                "obligations",
                f"the generator refuses: {tail(err)}; the solidity-lean base has no {INFER_FILE} "
                "for the discovered-invariant route",
                kept(f"{name}.Loops.{fname}.Gen", out + err),
            )
        rc2, out2, err2 = run(
            [*LAKE, "build", INFER_MODULE], cwd=lean_dir, env=lake_env
        )
        if rc2 != 0:
            raise Stop(
                "obligations",
                f"lake cannot build {INFER_MODULE}: {tail(out2 + err2)}",
                kept(f"{name}.Loops.{fname}.GenInfer", out2 + err2),
            )
        inferred = infer(
            run,
            lambda f: run(
                [*LAKE, "env", "lean", "--run", str(f)], cwd=lean_dir, env=lake_env
            ),
            fd,
            name,
            contract,
            fname,
            sel,
            nargs,
            z3=lake_env.get("SOLAR_VERIFY_Z3") if lake_env else None,
            why=tail(err),
        )
        out, rc = inferred["out"], 0
    if rc == REFUSED:
        raise Stop(
            "obligations",
            f"the generator refuses: {tail(err)}",
            kept(f"{name}.Loops.{fname}.Gen", out + err),
        )
    if rc != 0:
        raise Stop(
            "obligations",
            f"the generator exits {rc}: {tail(err or out)}",
            kept(f"{name}.Loops.{fname}.Gen", out + err),
        )
    mods, measure, route = gen_result(out)
    obl = f"{name}/Loops/{fname}/Obl"
    if obl not in mods:
        raise Stop("obligations", f"the generator wrote no {obl}")
    files = [lean_dir / "Certified" / f"{m}.lean" for m in mods]
    missing = [str(f) for f in files if not f.is_file()]
    if missing:
        raise Stop("obligations", f"the generator did not write {', '.join(missing)}")
    rc, out, err = run(
        [*LAKE, "build", obl.replace("/", ".")],
        cwd=lean_dir,
        env=lake_env,
    )
    if rc != 0:
        raise Stop(
            "obligations",
            f"lake cannot build {obl}: {tail(out + err)}",
            kept(f"{name}.Loops.{fname}.Obl", out + err),
        )
    return files, measure, route


def hook(
    *,
    run,
    solar,
    verify,
    z3,
    src,
    ast,
    contract,
    name,
    work,
    stages,
    lean_dir,
    lake_env,
    log,
    summary=lambda fname: None,
    shape=lambda fname: None,
    keep=None,
    heads_verify=None,
):
    """Steps 1 to 4 for every loop function of SRC. SOLAR and VERIFY return the run's solar and
    solar-verify (HEADS_VERIFY, when given, the one step 1 runs: heads_exporter), STAGES(solar, tmp) exports the run's stages to tmp/gen, LEAN_DIR() returns the
    prepared clone and LAKE_ENV(clone) its build environment; each may raise Stop. Returns
    (loop_heads, loop_cuts, blocked): loop_cuts only for the functions every step completed for,
    blocked for the rest, keyed by function ("*" when no function was reached), each
    {"step", "why"}. SUMMARY(fname) gives FNAME's body summary (module, constant) for gen_driver,
    or None: the erasure route. SHAPE(fname) gives the reason FNAME's loop is outside the
    erasure's shape (`loop_shapes`), or None; a reason blocks FNAME at step "shape" before any
    export or build."""
    global KEEP
    KEEP = keep
    loops = pathlib.Path(work) / "loops"
    loops.mkdir(parents=True, exist_ok=True)
    heads_file = loops / "loop-heads.json"
    cuts, blocked, heads = {}, {}, None
    try:
        manifest = exporter(run, (heads_verify or verify)(), z3, src, heads_file)
        heads = dict(file_entry(heads_file), checked=True)
        names = loop_functions(manifest)
        failing = failing_functions(manifest)
        if failing:
            heads["failing"] = failing
        if not names:
            raise Stop("exporter", "the manifest gives no function a loop")
    except Stop as s:
        return heads, cuts, {"*": {"step": s.step, "why": s.why}}
    except (OSError, ValueError, KeyError, TypeError) as e:
        return (
            heads,
            cuts,
            {"*": {"step": "internal", "why": f"{type(e).__name__}: {e}"}},
        )
    log(f"loop heads: {heads['sha256']}; loop functions {', '.join(names)}")
    whiles = while_functions(ast, contract)
    try:
        src_bytes = pathlib.Path(src).read_bytes()
    except OSError:
        src_bytes = b""
    stage0, clone = None, None
    for fname, why in failing.items():
        if fname not in names:
            log(f"loop heads: {fname} (no loop): {why}")
    for fname in names:
        try:
            if fname in failing:
                # Each function's verification is reported on its own: one that fails blocks only itself.
                raise Stop("exporter", failing[fname])
            if (why := shape(fname)) is not None:
                raise Stop("shape", why)
            sel, nargs = abi(ast, contract, fname)
            if stage0 is None:
                tmp = pathlib.Path(tempfile.mkdtemp(dir=loops, prefix="export-"))
                _, err = stages(solar(), tmp)
                if not (tmp / "gen" / contract / "Stage0.lean").is_file():
                    raise Stop(
                        "cut",
                        f"solar exported no Stage0.lean for {contract}: {tail(err)}",
                    )
                stage0 = tmp / "gen" / contract / "Stage0.lean"
            cut_file = loops / f"LoopCut.{fname}.lean"
            cut(run, heads_file, stage0, fname, sel, nargs, cut_file)
            retarget_cut(cut_file, name, fname)
            if clone is None:
                clone = lean_dir()
            files, measure, route = obligations(
                run,
                lake_env(clone),
                clone,
                name,
                contract,
                fname,
                sel,
                nargs,
                cut_file,
                stage0,
                summary(fname),
                annotations=(
                    loop_annotations(src_bytes, whiles[fname])
                    if fname in whiles
                    else None
                ),
            )
            cuts[fname] = {
                "loop_heads": {"path": heads["path"], "sha256": heads["sha256"]},
                "cut": file_entry(cut_file),
                "obligations": [file_entry(f) for f in files],
                "measure": measure,
                "route": route,
            }
            wr = clone / "Certified" / name / "Loops" / fname / "while-route.json"
            if route == "while-inferred" and wr.is_file():
                cuts[fname]["while"] = json.loads(wr.read_text())
            log(
                f"loop cut {fname}: obligations built ({route} route); measure {measure}"
            )
        except Stop as s:
            blocked[fname] = {"step": s.step, "why": s.why}
            if s.log:
                blocked[fname]["log"] = s.log
            log(f"loop cut {fname}: BLOCKED at {s.step}: {s.why}")
        except (OSError, ValueError, KeyError, TypeError) as e:
            blocked[fname] = {"step": "internal", "why": f"{type(e).__name__}: {e}"}
            log(f"loop cut {fname}: BLOCKED: {type(e).__name__}: {e}")
    return heads, cuts, blocked


def reasons(cuts, blocked):
    """The run's extra BLOCKED reasons, one per function, whatever its steps reached."""
    out = [
        {
            "feature": "loop",
            "where": fname,
            "detail": (
                "loop cut and obligations checked on the erasure route (the source premises of "
                "EraseLoopJoin.GeneratedTarget: eraseLoopStmt, its tree, bodyRuns_of_eraseLoop)"
                if cuts[fname].get("route") == "erasure"
                else "loop cut and obligations checked on the erasure route with a discovered "
                "invariant (loop_infer.py over the erased tree; sequents Verify.Phase3.Infer.allSeqsW)"
                if cuts[fname].get("route") == "erasure-inferred"
                else "loop cut and obligations checked on the while route (eraseWhile with the loop's "
                "data, the source invariant's sequents whileSeqs, loop_infer.py's MIR invariant over "
                "the while erasure's tree; Verify.LoopSource.Invariant.Final.final_of_whileW's premises)"
                if cuts[fname].get("route") == "while-inferred"
                else "loop cut and obligations checked from a body summary"
            )
            + "; the published statement does not apply the join yet: the loop's statement is OPEN",
        }
        for fname in cuts
    ]
    out += [
        {
            "feature": "loop",
            "where": "" if fname == "*" else fname,
            "detail": f"loop cut: {b['step']}: {b['why']}",
        }
        for fname, b in blocked.items()
    ]
    return out


# --- the erasure's shape, read from solc's AST ------------------------------------------------

LOOPS = ("ForStatement", "WhileStatement", "DoWhileStatement")


def _walk(node):
    if isinstance(node, dict):
        yield node
        for v in node.values():
            yield from _walk(v)
    elif isinstance(node, list):
        for v in node:
            yield from _walk(v)


def _type(node):
    return (node.get("typeDescriptions") or {}).get("typeString") or ""


def _callee(node):
    e = node.get("expression") or {}
    return e.get("name") if e.get("nodeType") == "Identifier" else None


def _iteration_refusal(nodes, while_loop=False):
    """Why an iteration made of NODES (the body and the update) is certainly outside
    `eraseLoopStmt`'s counted loop, or None. Each refusal is a statement the erasure has no case
    for inside a loop: the iteration must end normally or with Panic(0x11), add one to the words it
    changes and write only constant slots. WHILE_LOOP: the body of a while loop (eraseWhile's step,
    which also takes `x--` of a local word and writes no storage)."""
    for top in nodes:
        for n in _walk(top):
            t = n.get("nodeType")
            if t in LOOPS:
                return "a nested loop: the erasure covers one loop, whose body has none"
            if t in ("Break", "Continue"):
                return f"{t.lower()} in the loop: an iteration must run to its end"
            if t == "Return":
                return "return in the loop: an iteration must run to its end"
            if t == "EmitStatement":
                return "the loop emits a log: an iteration may only add one to words"
            if t == "RevertStatement":
                return "revert in the loop: an iteration may only fail with Panic(0x11)"
            if t == "FunctionCall" and _callee(n) in ("require", "revert"):
                return f"{_callee(n)} in the loop: an iteration may only fail with Panic(0x11)"
            if t == "FunctionCall" and _callee(n) == "assert":
                return "assert in the loop: it fails with Panic(0x01), and an iteration may only fail with Panic(0x11)"
            if t == "UnaryOperation" and n.get("operator") == "delete":
                return "delete in the loop: an iteration may only add one to words"
            if t == "UnaryOperation" and n.get("operator") == "--" and not while_loop:
                return "a decrement in the loop: an iteration may only add one to words"
            if t == "UnaryOperation" and n.get("operator") == "--":
                sub = n.get("subExpression") or {}
                if sub.get("nodeType") != "Identifier" or _type(n) != "uint256":
                    return "`--` on something other than a uint256 local in the while loop: eraseWhile steps local words only"
            if t == "UnaryOperation" and n.get("operator") == "++":
                sub = n.get("subExpression") or {}
                if sub.get("nodeType") in ("IndexAccess", "MemberAccess"):
                    return "`++` on an indexed or member expression in the loop: the erasure steps locals and storage fields only"
                if _type(n) != "uint256":
                    return f"`++` on a {_type(n) or 'non-word'} in the loop: the erasure steps uint256 words only"
            if (
                t == "Assignment"
                and (n.get("leftHandSide") or {}).get("nodeType") == "IndexAccess"
            ):
                return "the loop writes a mapping or array element: an iteration may write constant slots only"
    return None


def _loop_refusal(loop, fn, count):
    """Why LOOP, in the function or modifier FN with COUNT loops, is certainly outside the erasure."""
    t = loop.get("nodeType")
    name = fn.get("name") or fn.get("kind") or "?"
    if fn.get("nodeType") == "ModifierDefinition":
        return f"a loop in modifier {name}: the erasure covers a loop in an external function's own body"
    kind = fn.get("kind")
    if kind in ("constructor", "fallback", "receive"):
        return f"a loop in the {kind}: the erasure covers a loop in an external function's own body"
    if fn.get("visibility") in ("internal", "private"):
        return f"a loop in {fn.get('visibility')} function {name}: the erasure covers a loop in an external function's own body, not in a callee"
    if t == "WhileStatement":
        # The while route (step 3w): eraseWhile takes x++, x-- and assignments of local words; the
        # body must end normally or with Panic(0x11) and write no storage.
        why = _iteration_refusal([loop.get("body")], while_loop=True)
        if why:
            return why
        if count > 1:
            return f"{count} loops in {name}: the cut certificate covers one loop per function"
        return None
    if t == "DoWhileStatement":
        return "a do-while loop: the erasure covers counted for loops only"
    nested = _iteration_refusal([loop.get("body"), loop.get("loopExpression")])
    if nested:
        return nested
    if count > 1:
        return (
            f"{count} loops in {name}: the cut certificate covers one loop per function"
        )
    c = loop.get("condition")
    if c is None:
        return "a for loop without a condition: the erasure needs `counter < bound`"
    if c.get("nodeType") != "BinaryOperation" or c.get("operator") != "<":
        op = (
            c.get("operator")
            if c.get("nodeType") == "BinaryOperation"
            else c.get("nodeType")
        )
        return f"the loop's condition is `{op}`, not `counter < bound`"
    if (c.get("leftExpression") or {}).get("nodeType") != "Identifier":
        return "the left side of the loop's condition is not a variable: the erasure needs `counter < bound`"
    return None


def loop_shapes(cdef):
    """Every loop of the contract definition CDEF (solc's AST) as (node, function, reason), where
    function is the name of the function or modifier holding it and reason why it is certainly
    outside the erasure's counted loop, or None when only the erasure (the generator, in the
    kernel) can tell. Never a reason to pass: a loop with no reason here can still be refused by
    the generator."""
    out = []
    for fn in cdef.get("nodes", []):
        if fn.get("nodeType") not in ("FunctionDefinition", "ModifierDefinition"):
            continue
        loops = [n for n in _walk(fn.get("body")) if n.get("nodeType") in LOOPS]
        for loop in loops:
            out.append(
                (
                    loop,
                    fn.get("name") or fn.get("kind") or "",
                    _loop_refusal(loop, fn, len(loops)),
                )
            )
    return out


def shape_refusals(cdef):
    """The functions of CDEF with a loop outside the erasure's shape, each with its first reason."""
    out = {}
    for _, fname, why in loop_shapes(cdef):
        if why is not None and fname not in out:
            out[fname] = why
    return out


def while_functions(ast, contract):
    """{function name: its while loop's AST node} for the functions of CONTRACT (solc's AST) whose
    one loop is a while loop the shape scan admits: the while route's (step 3w)."""
    cdefs = [
        n
        for n in (ast or {}).get("nodes", [])
        if n.get("nodeType") == "ContractDefinition" and n.get("name") == contract
    ]
    if len(cdefs) != 1:
        return {}
    shapes = loop_shapes(cdefs[0])
    count = {}
    for _, fname, _ in shapes:
        count[fname] = count.get(fname, 0) + 1
    return {
        fname: node
        for node, fname, why in shapes
        if why is None
        and count[fname] == 1
        and node.get("nodeType") == "WhileStatement"
    }


# --- step 3b: the discovered-invariant route -------------------------------------------------


def infer_driver(name, contract, fname, sel, nargs, call):
    """A module that runs one GenInfer step (CALL: `exportLeaves cfg "PATH"` or `emit cfg I μ`) on
    the same GenObl.Config gen_driver gives GenObl.run (the erasure route: no summary)."""
    text = gen_driver(name, contract, fname, sel, nargs)
    head, rest = text.split("def main : IO UInt32 :=\n", 1)
    first, cfg = rest.split("\n", 1)
    assert first.strip() == f"{GEN_MODULE}.run", first
    head = head.replace(
        f"import {GEN_MODULE}\n", f"import {GEN_MODULE}\nimport {INFER_MODULE}\n", 1
    )
    return (
        head
        + "open SolidCore.Refinement.Generic Verify.LoopSource.Equiv.Inv\n\n"
        + "def cfg : Verify.LoopSource.Equiv.GenObl.Config :=\n"
        + cfg.rstrip("\n")
        .replace("\n    ", "\n  ")
        .replace("    { stage0", "  { stage0", 1)
        + "\n\n"
        + f"def main : IO UInt32 :=\n  {INFER_MODULE}.{call}\n"
    )


def infer(run, lean_run, fd, name, contract, fname, sel, nargs, z3=None, why=""):
    """Step 3b in the function's directory FD: GenInfer exports the cut check's leaves with the tree
    of the source's own erasure, loop_infer.py proposes an invariant and a measure for them (Spacer,
    then interval bounds; untrusted), and GenInfer writes the obligation modules over allSeqsW.
    LEAN_RUN(file) runs a module's main. Returns {"out" (GenInfer's JSON line), "candidate",
    "report"}; raises Stop("infer", ...) with loop_infer.py's diagnosis otherwise."""
    fd = pathlib.Path(fd)
    leaves = fd / "leaves.json"
    (fd / "InferExport.lean").write_text(
        infer_driver(
            name,
            contract,
            fname,
            sel,
            nargs,
            f"exportLeaves cfg {json.dumps(str(leaves))}",
        )
    )
    rc, out, err = lean_run(fd / "InferExport.lean")
    if rc != 0 or not leaves.is_file():
        raise Stop("infer", f"the leaves export exits {rc}: {tail(err or out)}")
    cand = fd / "candidate.json"
    env = dict(os.environ, **({"Z3": z3} if z3 else {}))
    rc, out, err = run(
        [
            sys.executable,
            "-I",
            str(HERE / "loop_infer.py"),
            str(leaves),
            "--emit-candidate",
            str(cand),
        ],
        env=env,
    )
    report = {}
    for line in reversed((out or "").splitlines()):
        if line.startswith("{"):
            try:
                report = json.loads(line)
            except ValueError:
                pass
            break
    if rc != 0 or not cand.is_file():
        raise Stop(
            "infer",
            f"the generator refuses ({why}) and loop_infer.py finds no invariant "
            f"({report.get('discovery', 'exit ' + str(rc))}, engines {report.get('engines', {})}): {tail(err, 400)}",
        )
    c = json.loads(cand.read_text())
    (fd / "InferEmit.lean").write_text(
        infer_driver(
            name, contract, fname, sel, nargs, f"emit cfg {c['I_lean']} {c['mu_lean']}"
        )
    )
    rc, out, err = lean_run(fd / "InferEmit.lean")
    if rc != 0:
        raise Stop(
            "infer",
            f"GenInfer refuses loop_infer.py's {c.get('source')} candidate "
            f"(I = {c.get('I_math')}, mu = {c.get('mu_math')}): {tail(err or out)}",
        )
    return {"out": out, "candidate": c, "report": report}


# --- step 3w: the while route ------------------------------------------------------------------

ANNOTATION = re.compile(r"@custom:invariant\s+(.+?)\s*$")
ATOM = re.compile(
    r"^\s*([A-Za-z_][A-Za-z0-9_]*|\d+)\s*(<=|>=|==|<|>)\s*([A-Za-z_][A-Za-z0-9_]*|\d+)\s*$"
)


def loop_annotations(src_bytes, loop):
    """The @custom:invariant texts of the /// lines right above the loop node LOOP (solc's AST, its
    "src" byte offset into SRC_BYTES)."""
    try:
        start = int(str(loop.get("src", "")).split(":")[0])
    except ValueError:
        return []
    lines = src_bytes[:start].decode("utf-8", "replace").split("\n")[:-1]
    out = []
    for line in reversed(lines):
        s = line.strip()
        if not s.startswith("///"):
            break
        m = ANNOTATION.search(s[3:])
        if m:
            out.append(m.group(1))
    return list(reversed(out))


def annotated_candidate(texts, probe):
    """The source invariant of the annotations TEXTS over the source header's words (PROBE: the
    while probe's source system: base n, header names, steps and initial words), as loop_infer.py's
    formulas: each conjunct `a op b` of locals or literals, then, for every header word the step
    leaves alone, its equality with its word before the loop (the frame the annotation omits).
    None when a conjunct is outside this grammar or names no header word."""
    base, names = probe["n"], probe["names"]

    def term(t):
        if t.isdigit():
            return ["lit", int(t)]
        if t in names:
            return ["arg", base + names.index(t)]
        return None

    I = []
    for text in texts:
        for atom in text.split("&&"):
            m = ATOM.match(atom)
            if not m:
                return None
            a, op, b = term(m.group(1)), m.group(2), term(m.group(3))
            if a is None or b is None:
                return None
            I.append(
                {
                    "<=": ["le", a, b],
                    "<": ["lt", a, b],
                    ">=": ["le", b, a],
                    ">": ["lt", b, a],
                    "==": ["eq", a, b],
                }[op]
            )
    for j, (st, w0) in enumerate(zip(probe["steps"], probe["inits"])):
        if st == ["arg", base + j]:
            I.append(["eq", ["arg", base + j], w0])
    return I or None


def while_driver(name, contract, fname, sel, nargs, call):
    """A module that runs one WHILE_MODULE step (CALL: `probe cfg "SRC" "MIR"` or
    `emit cfg Is μs I μ`) on the GenObl.Config gen_driver gives GenObl.run."""
    text = gen_driver(name, contract, fname, sel, nargs)
    head, rest = text.split("def main : IO UInt32 :=\n", 1)
    first, cfg = rest.split("\n", 1)
    assert first.strip() == f"{GEN_MODULE}.run", first
    head = head.replace(
        f"import {GEN_MODULE}\n", f"import {GEN_MODULE}\nimport {WHILE_MODULE}\n", 1
    )
    return (
        head
        + "open SolidCore.Refinement.Generic Verify.LoopSource.Equiv.Inv\n\n"
        + "def cfg : Verify.LoopSource.Equiv.GenObl.Config :=\n"
        + cfg.rstrip("\n")
        .replace("\n    ", "\n  ")
        .replace("    { stage0", "  { stage0", 1)
        + "\n\n"
        + f"def main : IO UInt32 :=\n  {WHILE_MODULE}.{call}\n"
    )


def _infer(run, leaves, cand, z3, candidate=None):
    """loop_infer.py on LEAVES, writing CAND; with CANDIDATE (a JSON file), that invariant is
    screened (and a measure found) instead of discovered. Returns (rc, report, stderr)."""
    env = dict(os.environ, **({"Z3": z3} if z3 else {}))
    cmd = [
        sys.executable,
        "-I",
        str(HERE / "loop_infer.py"),
        str(leaves),
        "--emit-candidate",
        str(cand),
    ]
    if candidate is not None:
        cmd += ["--candidate", str(candidate)]
    rc, out, err = run(cmd, env=env)
    report = {}
    for line in reversed((out or "").splitlines()):
        if line.startswith("{"):
            try:
                report = json.loads(line)
            except ValueError:
                pass
            break
    return rc, report, err


def while_route(
    run, lean_run, fd, name, contract, fname, sel, nargs, z3=None, annotations=()
):
    """Step 3w in the function's directory FD. Returns {"out" (the emit step's JSON line),
    "source_invariant", "invariant", "probe"}; raises Stop("while", ...) with the step's reason."""
    fd = pathlib.Path(fd)
    src_leaves, mir_leaves = fd / "while-source.json", fd / "while-leaves.json"
    (fd / "WhileProbe.lean").write_text(
        while_driver(
            name,
            contract,
            fname,
            sel,
            nargs,
            f"probe cfg {json.dumps(str(src_leaves))} {json.dumps(str(mir_leaves))}",
        )
    )
    rc, out, err = lean_run(fd / "WhileProbe.lean")
    if rc != 0 or not src_leaves.is_file() or not mir_leaves.is_file():
        raise Stop("while", f"the while probe exits {rc}: {tail(err or out)}")
    probe = json.loads(src_leaves.read_text())
    src_cand = fd / "source-candidate.json"
    source = None
    ann = annotated_candidate(list(annotations), probe) if annotations else None
    if ann is not None:
        given = fd / "source-annotation.json"
        given.write_text(json.dumps({"I": ann}))
        rc, rep, _ = _infer(run, src_leaves, src_cand, z3, candidate=given)
        if rc == 0 and src_cand.is_file():
            source = "annotation"
    if source is None:
        src_cand.unlink(missing_ok=True)
        rc, rep, err = _infer(run, src_leaves, src_cand, z3)
        if rc != 0 or not src_cand.is_file():
            raise Stop(
                "while",
                "no source invariant: "
                + ("the annotation does not check and " if ann is not None else "")
                + f"loop_infer.py finds none ({rep.get('discovery', 'exit ' + str(rc))}, "
                f"engines {rep.get('engines', {})}): {tail(err, 300)}",
            )
        source = rep.get("source") or "discovered"
    sc = json.loads(src_cand.read_text())
    mir_cand = fd / "candidate.json"
    rc, rep, err = _infer(run, mir_leaves, mir_cand, z3)
    if rc != 0 or not mir_cand.is_file():
        raise Stop(
            "while",
            f"no MIR invariant for the while erasure's tree: loop_infer.py finds none "
            f"({rep.get('discovery', 'exit ' + str(rc))}, engines {rep.get('engines', {})}): {tail(err, 300)}",
        )
    mc = json.loads(mir_cand.read_text())
    (fd / "WhileEmit.lean").write_text(
        while_driver(
            name,
            contract,
            fname,
            sel,
            nargs,
            f"emit cfg {sc['I_lean']} {sc['mu_lean']} {mc['I_lean']} {mc['mu_lean']}",
        )
    )
    rc, out, err = lean_run(fd / "WhileEmit.lean")
    if rc != 0:
        raise Stop(
            "while",
            f"GenWhile refuses the candidates (source I = {sc.get('I_math')}, mu = {sc.get('mu_math')}; "
            f"MIR I = {mc.get('I_math')}, mu = {mc.get('mu_math')}): {tail(err or out)}",
        )
    return {
        "out": out,
        "source_invariant": {
            "source": source,
            "I": sc.get("I_math"),
            "mu": sc.get("mu_math"),
        },
        "invariant": {
            "source": mc.get("source"),
            "I": mc.get("I_math"),
            "mu": mc.get("mu_math"),
        },
        "probe": {"exit": probe.get("exit"), "names": probe.get("names")},
    }
