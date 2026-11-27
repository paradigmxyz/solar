#!/usr/bin/env python3
"""The published final theorem of a self-recursive contract (Phase 3, slice 5; RECURSION.md).

The generic source stage erases a function's body to a finite tree and inlines the internal calls
it erases, so a function that calls itself has no erasure (the "erasure Lowering" refuses it), and
its backend stages are cut-point stages (rec_cuts.py), which shape_gen.py's plain stage chain and the
loop assembly checker (checkL) do not take. This route replaces both for a self-recursive function
whose measure is written (@custom:decreases):

* the source stage: solidity-lean's Verify/Phase3/RecSource proves the source's behaviour by induction
  on the measure in SolidCore's own interpreter (Runs.sum_runs, Total.total_runs, Join.source_returns),
  about the SolidCore AST in RecSource/Ast.lean, which rec_source.py binds to this run's source byte for
  byte (the front end's AST of the file, its digest in the module's header);
* the backend stages: the pinned export Verify/Phase3/RecSum/gen/RecSum with its cut-point stage proofs
  (Proofs/Stages.lean), which this run's own -Zemit-lean-stages export must equal file for file;
* the assembly stage: AssembleRec's checker with frames (checkF), forwards from the checker's gas bound
  (forward_of_checkF) and backwards at every gas (AssembleRecWithin.within_of_checkF, failures below the
  bound being Shapes.Inert), with the block count bounded by the measure (RecSource/Gas.lean's
  ends_within: 2 * a + 4 blocks, so the gas bound 618800 * (2 * a + 4) + 9 < 2 ^ 64);
* the join: RecSource/Certified.lean's shape_ok, Shapes.Certified of the checked source and the exact
  bytecode, with the statement's inputs in RecSource/CertSpec.lean (valid calls, allowed statuses,
  postcondition, non-vacuity);
* final.py's statement (kind certified, the pinned bytes bound to this run's by runtime_is_fresh),
  lake build NAME.Final, check_final.lean with this run's bytes, and the kernel replay of the final's
  closure (loop_final.replay_closure).

What is generic and what is not: the checker, the backward and forward assembly theorems, final.py and
check_final are generic. The recursion proofs (sum_runs, fn_steps) are RecSum's own: PROGRAMS lists
the programs that have them, and any other self-recursive function is refused at step "program" with
the reason, never proved. The postcondition is a Lean predicate written for the function's ensures,
not solar-verify's export, so the statement is never the strongest status.

Refusals (Refused, with the step): "recursion": not exactly one function calling itself with a
measure; "program": no source proof for this function; "source": rec_source.py refuses the binding;
"export": this run's stage export is not the pinned one; "statement": final.py refuses; "build": the
final does not build; "check": check_final refuses; "replay": the closure's kernel replay fails.
"""

import json
import pathlib
import re
import shutil
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

# The programs whose recursion proofs exist, keyed by (contract, recursive function).
PROGRAMS = {
    ("RecSum", "sum"): {
        "ast_module": "Verify.Phase3.RecSource.Ast",
        "export_dir": "Verify/Phase3/RecSum/gen/RecSum",
        "pinned_bytes": "SolarTV.Export.RecSum.bytecode",
        "pinned_module": "Verify.Phase3.RecSum.gen.RecSum.Bytecode",
        "source_modules": [
            "Verify.Phase3.RecSource.Ast",
            "Verify.Phase3.RecSource.Source",
            "Verify.Phase3.RecSource.Runs",
            "Verify.Phase3.RecSource.Total",
            "Verify.Phase3.RecSource.Join",
        ],
        "stage_modules": [
            "Verify.Phase3.RecSum.Proofs.Stages",
            "Verify.Phase3.RecSum.Proofs.Execution",
            "SolidCore.Refinement.Solar.AssembleRecWithin",
            "Verify.Phase3.RecSource.Gas",
        ],
        "spec_modules": ["Verify.Phase3.RecSource.CertSpec"],
        "proof_module": "Verify.Phase3.RecSource.Certified",
        "shape": {
            "kind": "certified",
            "contract": "Verify.Phase3.RecSource.contract?",
            "fuel": "Verify.Phase3.RecSource.fuel",
            "source_sem": "SolidCore.Refinement.Generic.Loop.solidCoreRuntime",
            "valid": "Verify.Phase3.RecSource.Cert.Valid",
            "allowed": "Verify.Phase3.RecSource.Cert.Allowed",
            "post": "Verify.Phase3.RecSource.Cert.Post",
            "nonvacuous": "Verify.Phase3.RecSource.Cert.nonvacuous",
            "gas": "fun call => 618800 * (2 * (SolarTV.Machine.calldataWord call 4).toNat + 4) + 9",
            "env": [],
            "spec": "a Lean specification (Verify.Phase3.RecSource.Cert.Post) written for total's "
            "@custom:ensures n <= 64 ==> result == n * (n + 1) / 2, not solar-verify's export; the valid "
            "calls are its @custom:succeeds-if domain (msg.value == 0 && n <= 64) and the allowed status "
            "is success",
            "proof": "Verify.Phase3.RecSource.Cert.shape_ok",
            "excluded": [
                "calls: total(a) with a <= 64, no value, any storage; calls with a > 64 (the source "
                "reverts) and other call data are outside the statement",
                "gas: the bound is 618800 * (2 * a + 4) + 9 (at most 81681609); below it the bytecode may "
                "run out of gas, which equivalence admits as a failure that changes nothing",
                "frame: one caller (Target.message's), the runtime code only; deployment is not covered",
                "source model trusted: SolidCore's checked RecSum (Verify.Phase3.RecSource.contract?, the "
                "front end's AST of the source, bound by rec_source.py) is the meaning of RecSum.sol "
                "(frontend trust boundary)",
                "proof route: the recursion proofs (RecSource.Runs.sum_runs, RecSource.Gas.fn_steps) are "
                "RecSum's own; the route refuses any other recursive body",
            ],
        },
    },
}


class Refused(Exception):
    """Why the run publishes no recursion final theorem, at which step."""

    def __init__(self, step, why):
        super().__init__(f"{step}: {why}")
        self.step, self.why = step, why


def reason(e, where=""):
    """The run's BLOCKED reason for a refusal."""
    return {"feature": "recursion", "where": where, "detail": f"recursion final: {e.step}: {e.why}"}


def _doc(n):
    d = n.get("documentation") or {}
    return d.get("text", "") if isinstance(d, dict) else str(d)


def route(ast, contract):
    """The recursion route's function of CONTRACT in AST, or None when the source has no recursion:
    {"function", "measure"}. Raises Refused("recursion", why) for recursion the route does not take
    (mutual recursion, a function value, a function without @custom:decreases, or more than one)."""
    import generic

    cdef = next(
        (n for n in ast.get("nodes", []) if n.get("nodeType") == "ContractDefinition" and n.get("name") == contract),
        None,
    )
    if cdef is None:
        return None
    variables = {
        n["id"] for n in generic.walk(ast) if n.get("nodeType") == "VariableDeclaration" and "id" in n
    }
    # The graph the envelope reads (generic.envelope): CDEF's linearization and the sibling library
    # and free functions it runs inline.
    lin, _ = generic.linearization(ast, cdef)
    lin = lin or [cdef]
    inline, _ = generic.sibling_code(ast, cdef, lin)
    cycles, values = generic.recursion_cycles(
        cdef, variables, lin, [node for node, _ in inline]
    )
    if not cycles and not values:
        return None
    if values:
        raise Refused("recursion", "a call through an internal function value")
    if len(cycles) != 1 or len(cycles[0][0]) != 1:
        raise Refused("recursion", "not exactly one function calling itself: " + "; ".join(" and ".join(n) for n, _ in cycles))
    fname = cycles[0][0][0]
    fns = [n for n in generic.walk(ast) if n.get("nodeType") == "FunctionDefinition" and n.get("name") == fname]
    m = re.search(r"@custom:decreases\s+(\S[^\n]*)", _doc(fns[0])) if len(fns) == 1 else None
    if m is None:
        raise Refused("recursion", f"{fname} calls itself without a @custom:decreases measure")
    return {"function": fname, "measure": m.group(1).strip()}


def program(contract, fname):
    """The recursion proofs of CONTRACT's FNAME, or Refused("program")."""
    p = PROGRAMS.get((contract, fname))
    if p is None:
        have = ", ".join(f"{c}.{f}" for c, f in PROGRAMS)
        raise Refused(
            "program",
            f"no source proof for {contract}.{fname}: the recursion route has the measure-induction "
            f"proofs of {have} only (Verify/Phase3/RecSource); a generator for other bodies is open",
        )
    return p


def bind_source(src, lean_dir, solc, run):
    """rec_source.py's binding of the program's Ast module to SRC (its front end's AST, byte for
    byte), or Refused("source")."""
    rc, out, err = run([sys.executable, str(HERE / "rec_source.py"), str(src), "--solidity-lean", str(lean_dir), "--solc", str(solc)])
    if rc != 0:
        raise Refused("source", (err or out).strip()[-300:])
    return out.strip()


def _manifest_key(data):
    """A manifest's bytes with its solar argument list in sorted order: the driver passes the source
    before the options, the pinned export was made with it after them, and the manifest records the
    order; every other line (sources and their digests, compiler, each stage's digest, the bytecode)
    is compared byte for byte."""
    text = data.decode()
    m = re.search(r"^def arguments : List String :=\n  \[(.*?)\]\n", text, re.M | re.S)
    if m is None:
        return data
    args = sorted(re.findall(r'"((?:[^"\\]|\\.)*)"', m.group(1)))
    return (text[: m.start()] + "def arguments (sorted) : " + json.dumps(args) + "\n" + text[m.end():]).encode()


def export_problem(gen_dir, lean_dir, prog):
    """Why this run's stage export GEN_DIR is not the pinned one the proofs are about, or None: the
    same files, each the same bytes (the manifest's argument list up to order)."""
    pinned = pathlib.Path(lean_dir) / prog["export_dir"]

    def files(d):
        return {
            p.name: (_manifest_key(p.read_bytes()) if p.name == "Manifest.lean" else p.read_bytes())
            for p in pathlib.Path(d).glob("*.lean")
        }

    mine, theirs = files(gen_dir), files(pinned)
    if set(mine) != set(theirs):
        return f"stage files differ: run {sorted(mine)}, pinned {sorted(theirs)}"
    for k in sorted(mine):
        if mine[k] != theirs[k]:
            return f"{k} differs from the pinned {prog['export_dir']}/{k}"
    return None


def shape(prog):
    """shape.json for final.py."""
    return dict(
        prog["shape"],
        spec_modules=prog["spec_modules"],
        pinned_bytes=prog["pinned_bytes"],
        pinned_module=prog["pinned_module"],
        proof_module=prog["proof_module"],
    )


def write_statement(d, name, contract, prog, plain, run):
    """shape.json, Statement.lean and Final.lean in D (Certified/NAME), by final.py with this run's
    bytes PLAIN; Refused("statement") when final.py refuses."""
    d = pathlib.Path(d)
    d.mkdir(parents=True, exist_ok=True)
    (d / "shape.json").write_text(json.dumps(shape(prog), indent=2) + "\n")
    rc, out, err = run([
        sys.executable, str(HERE / "final.py"), str(d / "shape.json"), "--name", name, "--contract", contract,
        "--hex", plain, "--out", str(d / "Final.lean"), "--statement-out", str(d / "Statement.lean"),
        "--record", str(d / "final.json"),
    ])
    if rc != 0:
        raise Refused("statement", f"final.py refuses: {(err or out).strip()[-300:]}")
    return json.loads((d / "final.json").read_text())


# --- the driver's hook ---------------------------------------------------------------------------


def attach(record, *, run, name, contract, ast, solar, src, work, lean_dir, lake_env, plain, solc, log):
    """The recursion route in the driver's clone LEAN_DIR. Records record["rec_final"]; returns []
    when the final theorem checked, else the run's BLOCKED reason (a refusal names its step)."""
    import generic
    import loop_cuts
    import loop_final

    lean_dir = pathlib.Path(lean_dir)
    d = lean_dir / "Certified" / name
    try:
        r = route(ast, contract)
        if r is None:
            raise Refused("recursion", "the source has no recursion")
        prog = program(contract, r["function"])
        bound = bind_source(src, lean_dir, solc, run)
        tmp = pathlib.Path(work) / "rec-final"
        shutil.rmtree(tmp, ignore_errors=True)
        tmp.mkdir(parents=True)
        fresh, err = generic.export_stages(solar, src, contract, tmp)
        if fresh != plain:
            raise Refused("export", "the stage export's runtime bytes are not this run's")
        why = export_problem(tmp / "gen" / contract, lean_dir, prog)
        if why:
            raise Refused("export", why)
        fin = write_statement(d, name, contract, prog, plain, run)
        rc, out, err = run([*loop_cuts.LAKE, "build", f"{name}.Final"], cwd=lean_dir, env=lake_env)
        if rc != 0:
            raise Refused("build", f"lake cannot build {name}.Final: {(out + err).strip()[-300:]}")
        rc, out, err = run(
            [*loop_cuts.LAKE, "env", "lean", "--run", str(HERE / "check_final.lean"), name, prog["proof_module"], plain],
            cwd=lean_dir, env=lake_env,
        )
        if rc != 0:
            raise Refused("check", f"check_final refuses: {(out + err).strip()[-300:]}")
        axioms = re.search(r"axioms \[([^\]]*)\]", out)
        try:
            replayed = loop_final.replay_closure(run, lean_dir, name, lake_env)
        except loop_final.Refused as e:
            raise Refused("replay", e.why) from None
        record["rec_final"] = {
            "function": r["function"],
            "measure": r["measure"],
            "source_binding": bound,
            "proof_module": prog["proof_module"],
            "final": fin,
            "axioms": axioms.group(1).split(", ") if axioms else None,
            "replay": replayed,
        }
        log(f"recursion final {r['function']}: Certified.{name}.Final.final checked")
        return []
    except Refused as e:
        record["rec_final"] = {"refused": {"step": e.step, "why": e.why}}
        log(f"recursion final: REFUSED at {e.step}: {e.why}")
        return [reason(e)]
    except (OSError, ValueError, KeyError, StopIteration) as e:
        record["rec_final"] = {"refused": {"step": "internal", "why": f"{type(e).__name__}: {e}"}}
        return [reason(Refused("internal", f"{type(e).__name__}: {e}"))]
