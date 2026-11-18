"""Loop cut points for the generic driver (tools/certify/generic.py).

A source the envelope blocks on a loop goes through four steps here, for each function whose
loop solar-verify's manifest lists. None of them is a reason to pass: the run stays BLOCKED in
every case, and a step that fails leaves the function without a loop_cuts entry.

1. exporter: `solar-verify SOURCE --emit-loop-heads WORK/loops/loop-heads.json`, with z3 from
   SOLAR_VERIFY_Z3, then `--check-loop-heads` on the file it wrote. The manifest is an untrusted
   hint.
2. cut: loop_equiv.py writes the function's `Cut` from the manifest and the run's own
   `Stage0.lean`, exported by the run's solar with the run's options. The selector and argument
   count come from solc's AST of the source, never from the manifest. Exit 2 is a refusal.
3. obligations: in the run's clone of the solidity-lean base, a driver module written here runs
   the base's obligation generator (`GEN_MODULE`), which writes Data, V0..Vk and Obl under
   Certified/NAME/Loops/FN/. lake then builds Obl, whose theorems (leaves, tree, seqs, valid,
   obl) the kernel checks. A base without `Cut.lean` or the generator blocks here, before any
   build.
4. record: loop_cuts[FN] = {loop_heads, cut, obligations, measure}, each file with its sha256.

Not done here: the published statement does not yet apply `loop_equiv` to the run's backend
stage and loop check, and the loop's resources stay OPEN, since no iteration-bound premise exists
yet.
"""

import hashlib
import json
import os
import pathlib
import re
import shutil
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent

FORMAT = "solar-verify loop heads 1"
CUT_FILE = "Verify/LoopSource/Equiv/Cut.lean"
GEN_MODULE = "Verify.LoopSource.Equiv.GenObl"
GEN_FILE = GEN_MODULE.replace(".", "/") + ".lean"
GEN_FORMAT = "loop obligations 1"
REFUSED = 2
# keep(label, output) -> (log name or None, tail): the driver's keeper of failed builds' logs
# (generic.keep_log), set by hook; None keeps none.
KEEP = None


def kept(label, out):
    return KEEP(label, out)[0] if KEEP else None


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


def gen_driver(name, contract, fname, sel, nargs, summary=None):
    """The module that runs the base's obligation generator for FNAME in the run's clone.

    The generator's interface (GEN_MODULE): `run (cfg : Config) : IO UInt32` writes the modules
    `<pre>Data`, `<pre>V0`..`<pre>Vk` and `<pre>Obl` under `cfg.root`. It exits 0 after printing
    one JSON line `{"format": GEN_FORMAT, "modules": [...], "measure": "..."}`, and exits
    REFUSED, with the reason on stderr, when cut.check, brTree or allSeqs is none or no measure
    is found, or when no body summary is given or it does not match the imported function.

    SUMMARY is (module, constant): a `GenObl.Summary` of FNAME's body (its `Br` and the
    storage-field and function shape it was proved for), which the generator needs because the
    source erasure has no loop case. The generator checks its shape against this run's imported
    contract and the generated modules prove that by `kernel_rfl`, so a summary for another
    function or source is refused. With none, the driver passes `summary? := none` and the
    generator refuses."""
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
        f"    {{ stage0 := SolarTV.Export.{contract}.stage0",
        f'      stage0Name := "SolarTV.Export.{contract}.stage0"',
        "      cut := Verify.LoopSource.Equiv.Generated.cut",
        '      cutName := "Verify.LoopSource.Equiv.Generated.cut"',
        f"      contract? := {checked}",
        f"      contractName := {json.dumps(checked)}",
        f"      sel := {sel:#010x}",
        f"      nargs := {nargs}",
        f"      imports := [{', '.join(json.dumps(m) for m in imports)}]",
        f"      pre := {json.dumps(pre)}",
        f'      ns := "Certified.{name}.Loops.{fname}"',
        '      root := "Certified"',
        f"      files := {V_FILES}",
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
    """The generator's JSON line: its modules (stems relative to Certified/) and its measure."""
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
    return mods, measure


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
):
    """Step 3 for FNAME in the prepared clone LEAN_DIR: the obligation files and the measure."""
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
    rc, out, err = run(
        ["lake", "build", "-KleanMemory=6000", *deps], cwd=lean_dir, env=lake_env
    )
    if rc != 0:
        raise Stop(
            "obligations",
            f"lake cannot build the generator's imports: {tail(out + err)}",
            kept(f"{name}.Loops.{fname}.GenImports", out + err),
        )
    rc, out, err = run(
        ["lake", "env", "lean", "--run", str(fd / "Gen.lean")],
        cwd=lean_dir,
        env=lake_env,
    )
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
    mods, measure = gen_result(out)
    obl = f"{name}/Loops/{fname}/Obl"
    if obl not in mods:
        raise Stop("obligations", f"the generator wrote no {obl}")
    files = [lean_dir / "Certified" / f"{m}.lean" for m in mods]
    missing = [str(f) for f in files if not f.is_file()]
    if missing:
        raise Stop("obligations", f"the generator did not write {', '.join(missing)}")
    rc, out, err = run(
        ["lake", "build", "-KleanMemory=6000", obl.replace("/", ".")],
        cwd=lean_dir,
        env=lake_env,
    )
    if rc != 0:
        raise Stop(
            "obligations",
            f"lake cannot build {obl}: {tail(out + err)}",
            kept(f"{name}.Loops.{fname}.Obl", out + err),
        )
    return files, measure


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
    keep=None,
):
    """Steps 1 to 4 for every loop function of SRC. SOLAR and VERIFY return the run's solar and
    solar-verify, STAGES(solar, tmp) exports the run's stages to tmp/gen, LEAN_DIR() returns the
    prepared clone and LAKE_ENV(clone) its build environment; each may raise Stop. Returns
    (loop_heads, loop_cuts, blocked): loop_cuts only for the functions every step completed for,
    blocked for the rest, keyed by function ("*" when no function was reached), each
    {"step", "why"}. BODY(fname)
    gives FNAME's body summary (module, constant) for gen_driver, or None, which the
    generator refuses."""
    global KEEP
    KEEP = keep
    loops = pathlib.Path(work) / "loops"
    loops.mkdir(parents=True, exist_ok=True)
    heads_file = loops / "loop-heads.json"
    cuts, blocked, heads = {}, {}, None
    try:
        manifest = exporter(run, verify(), z3, src, heads_file)
        heads = dict(file_entry(heads_file), checked=True)
        names = loop_functions(manifest)
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
    stage0, clone = None, None
    for fname in names:
        try:
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
            if clone is None:
                clone = lean_dir()
            files, measure = obligations(
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
            )
            cuts[fname] = {
                "loop_heads": {"path": heads["path"], "sha256": heads["sha256"]},
                "cut": file_entry(cut_file),
                "obligations": [file_entry(f) for f in files],
                "measure": measure,
            }
            log(f"loop cut {fname}: obligations built; measure {measure}")
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
            "detail": "loop cut and obligations checked; the published statement does not apply "
            "loop_equiv yet, and the loop's resources are OPEN",
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
