#!/usr/bin/env python3
"""Writes the kernel proofs of one exported contract's backend stages.

certify.sh runs this on a directory solar wrote with -Zemit-lean-stages (its manifest.json and
stage files). For every pair of consecutive stages it writes a module that proves, with
sym_exec, that both runs end in the same result on every call (the first MIR stage also gets
the proof that it ends), then a Stages module that turns each into a Stage of the fuel-free
semantics and chains them with the library's composition, and an Assembly module that checks
the bytecode against the final EVM IR with the assembly walk. Nothing in the proofs is written
per contract: the fuels are the programs' block counts, which bound a loop-free run (times the
square of one more than the call sites, each a MIR `.call` or an EVM IR `.pushLabel`, when
the program makes internal calls, whose callee blocks run again on every call, and times one
more than --loops, a bound on the iterations of a program whose loops run a fixed number of
times, which the first stage's end proof checks), and the
layout is the bytecode's JUMPDESTs in order. A contract outside that scope (one that loops,
or whose stages differ) fails its proof: the generated files are only claims until Lean checks
them.

    certify.py GEN_DIR --gen-module M --proof-dir D --proof-module P --namespace N --label L
"""

import argparse
import itertools
import json
import pathlib
import re
import sys

sys.path.insert(
    0, str(pathlib.Path(__file__).resolve().parents[2] / "solar-tv" / "milestone")
)
import cutpoints

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import asm_cert
import export_ns
import rec_cuts

AXIOMS = "[propext, Classical.choice, Quot.sound]"


def blocks(text):
    """The number of blocks of an exported program: one "-- bbN" comment each."""
    return len(re.findall(r"^\s*-- bb\d+$", text, re.MULTILINE))


def jump_targets(text):
    """The blocks an exported program's static jumps and pushed labels target, leaving out a
    block's final transfer to the block after it, which falls through (`blockItems`): a final
    `jump next`, and of a final `jumpi yes, jump no` the one of the two that follows."""
    parts = re.split(r"^\s*-- bb\d+$", text, flags=re.MULTILINE)[1:]
    jump = re.compile(r"\.(jumpi|jump|pushLabel) (\d+)")
    final = re.compile(r"\.jumpi (\d+),\s*\.jump (\d+)\]|\.jump (\d+)\]")
    targets = set()
    for index, part in enumerate(parts):
        nxt = index + 1 if index + 1 < len(parts) else None
        tail = final.search(part)
        end = tail.start() if tail else len(part)
        targets |= {int(m.group(2)) for m in jump.finditer(part[:end])}
        if tail is None:
            continue
        if tail.group(3) is not None:
            if int(tail.group(3)) != nxt:
                targets.add(int(tail.group(3)))
        else:
            yes, no = int(tail.group(1)), int(tail.group(2))
            if no != nxt:
                # ISZERO; push no; JUMPI, or both jumps.
                targets.add(no)
                if yes != nxt:
                    targets.add(yes)
            else:
                targets.add(yes)
    return targets


def calls(text):
    """The internal call sites of an exported program: a MIR call or a pushed return label."""
    return len(re.findall(r"\.call |\.pushLabel ", text))


def fuel(text, loops=0):
    """A bound on the blocks a run visits: its block count when it makes no call and does not
    loop, times one more than the iterations its loops run (the --loops claim)."""
    return blocks(text) * (1 + calls(text)) ** 2 * (1 + loops)


def stage_list(gen, loops=0):
    manifest = json.loads((gen / "manifest.json").read_text())
    stages = []
    for s in manifest["stages"]:
        if s["kind"] == "bytecode":
            continue
        text = (gen / s["file"]).read_text()
        stages.append(
            {
                "def": s["definition"],
                "module": s["file"][: -len(".lean")],
                "kind": s["kind"],
                "pass": s["pass"],
                "fuel": fuel(text, loops),
                "calls": calls(text),
            }
        )
    kinds = [s["kind"] for s in stages]
    if not kinds or kinds[0] != "mir" or kinds.count("sched") != 1:
        raise SystemExit(f"error: unexpected stages {kinds}")
    for s in stages:
        if s["fuel"] == 0:
            raise SystemExit(f"error: {s['module']} has no blocks")
    return manifest, stages


def run_fn(stage):
    return "MIR.Lowered.Module.run" if stage["kind"] == "mir" else "EVM.Program.run"


def layer(stage):
    return "MIR.Lowered.moduleLayer" if stage["kind"] == "mir" else "EVM.Program.layer"


def sem(stage):
    return "mirSem" if stage["kind"] == "mir" else "evmirSem"


def axioms_check(namespace, name, axioms=AXIOMS):
    return (
        f"/-- info: '{namespace}.{name}' depends on axioms: {axioms} -/\n"
        f"#guard_msgs (whitespace := lax) in\n#print axioms {name}\n"
    )


# The option every generated module of a large contract opens with, set by main: the proofs
# unfold and rewrite the stages' programs, whose lists nest deeper than Lean's default recursion
# depth allows once they have hundreds of blocks, as the exporter's own modules do.
DEPTH = ""


def header(imports, doc, namespace, opens):
    return (
        "-- Generated by solar's tools/certify/certify.py; do not edit.\n"
        + "".join(f"import {i}\n" for i in imports)
        + f"\n/-! {doc} -/\n\nnamespace {namespace}\n\nopen {opens}\n\n"
        + DEPTH
    )


def loops_somewhere(gen, stages):
    """Whether a stage's program has a cycle: a MIR loop header or an EVM IR cycle."""
    for s in stages:
        text = (gen / f"{s['module']}.lean").read_text()
        if s["kind"] == "mir":
            if any(cutpoints.mir_headers(f)[0] for f in cutpoints.parse_mir(text)):
                return True
        elif cutpoints.loops(cutpoints.parse_evm(text)):
            return True
    return False


# Segment proofs per module, when the programs are cut at their join points (cutpoints.py marks
# each list of relations' part): each module checks in its own Lean process, within its memory,
# and the modules check in parallel.
def split_segments(out, pm, module, p, body, imports, ns, opens, what):
    """With --joins, moves the stage's definitions to Runs/MODULE/Defs and its segment proofs
    to Runs/MODULE/SegN, one list of relations each, and returns the rest of the body and the imports of the
    stage's module; else returns them as they are."""
    if not cutpoints.JOINS or "-- @@defs-end\n" not in body:
        return body, imports
    defs, body = body.split("-- @@defs-end\n", 1)
    body, rest = body.split("-- @@main\n", 1)
    segs = [c for c in body.split("-- @@chunk\n") if c.strip()]
    sub = out / "Runs" / module
    sub.mkdir(parents=True, exist_ok=True)
    opener = "open SolarTV.CutPoint (seg SegRel)\n\n"
    (sub / "Defs.lean").write_text(
        header(
            imports,
            f"{what}: the cut points, relations and relation lemmas.",
            ns,
            opens,
        )
        + opener
        + defs
        + f"end {ns}\n"
    )
    mods = []
    for n, chunk in enumerate(segs):
        name = f"Seg{n}"
        mods.append(f"{pm}.Runs.{module}.{name}")
        (sub / f"{name}.lean").write_text(
            header(
                [f"{pm}.Runs.{module}.Defs"],
                f"{what}: the segments from the relations of list {n}.",
                ns,
                opens,
            )
            + opener
            + chunk
            + f"end {ns}\n"
        )
    return rest, mods


def rec_heads(gen, last):
    """rec_frame_heads' heads."""
    return rec_frame_heads(gen, last)[0]


def rec_frame_heads(gen, last):
    """The heads of the assembly checker for a recursive function (AssembleRec.checkR), and the
    function's entry block: the cut
    points of the final EVM IR, the function's entry and the blocks it resumes at after a
    recursive call (rec_cuts.EvmView), each with its highest entry height, entry height from below
    and entry types. The stage relations bound the stack below a frame by rec_cuts.BOUND, so the
    entry is at most the frame's words over that; the frame's words are its return label (a label)
    and the caller's words (data), and a resume point has the call's result on top."""
    mir = cutpoints.parse_mir((gen / "Stage0.lean").read_text())
    F, sites, outer = rec_cuts.shape(last["pass"], mir)
    sf = gen / "Schedule.json"
    if not sf.exists():
        raise rec_cuts.Unsupported("no Schedule.json from the exporter")
    view = rec_cuts.EvmView(
        last["pass"],
        cutpoints.parse_evm((gen / f"{last['module']}.lean").read_text()),
        json.loads(sf.read_text()),
        F,
        sites,
        outer,
    )

    def types(layout):
        return [x == "ret" for x in layout]

    heads = {
        view.entry: (
            len(view.entry_layout) + rec_cuts.BOUND,
            len(view.entry_layout),
            types(view.entry_layout),
        )
    }
    for s in sites:
        ty = [False] + types(view.below[s])
        heads[view.ret[s]] = (len(ty) + rec_cuts.BOUND, len(ty), ty)
    return heads, view.entry


def rel_patterns(runs_text, d):
    """For each relation of the last stage's cut points (rec_cuts' D_rels), the rcases pattern
    that names the second stack's bound: the first configurations' pair, then for each cut point
    its existential words, both configurations, the two bounds and the frames."""
    m = re.search(
        rf"def {d}_rels \(call : Call\)[^\n]*:=\n  \[(.*?)\]\n\n", runs_text, re.DOTALL
    )
    if m is None:
        raise rec_cuts.Unsupported(f"the last stage's module defines no {d}_rels")
    pats = []
    for rel in re.split(r",\n   (?=fun x x' =>)", m.group(1)):
        q = re.match(r"fun x x' => ∃ ((?:\([^()]*\) ?)+),", rel.strip())
        if q is None:
            pats.append(None)
            continue
        names = [n for g in re.findall(r"\(([^():]*):", q.group(1)) for n in g.split()]
        pats.append(
            "⟨" + ", ".join(["_"] * len(names) + ["-", "rfl", "-", "h1", "-"]) + "⟩"
        )
    return pats


def rec_execution(args, ns, exported, label, contract, stages, last_text, code, gm, pm):
    """The Execution module of a program with one recursive function: AssembleRec.checkF on the
    certificate and frames asm_cert.infer_frames gives, the heads' bound read off the last stage's
    cut-point simulation (reach_cut), and the bytecode run as the final EVM IR and the first
    lowered MIR end. Returns the module's text, or raises SystemExit or rec_cuts.Unsupported with
    the reason it cannot."""
    first, prev, last = stages[0], stages[-2], stages[-1]
    heads, entry = rec_frame_heads(args.gen, last)
    c = asm_cert.infer_frames(last_text, code, heads, entry)
    cert, maxs = asm_cert.lean_cert(c)
    frames = asm_cert.lean_frames(c)
    d, p = last["def"], prev["def"]
    runs = (args.proof_dir / "Runs" / f"{last['module']}.lean").read_text()
    pats = rel_patterns(runs, d)
    hl = sorted(heads)
    alts = " ∨ ".join(f"x.1 = {t}" for t in hl)
    rf = " | ".join(["rfl"] * len(hl))
    cases = []
    for pat in pats:
        if pat is None:
            cases.append("  · obtain ⟨-, rfl⟩ := hPy\n    simp at hx\n")
        else:
            cases.append(
                f"  · obtain {pat} := hPy\n"
                "    simp only [SolidCore.Refinement.Solar.AssembleLoop.maxH, maxs, List.length_cons]\n"
                "    simp\n    omega\n"
            )
    A = "SolidCore.Refinement.Solar.Assemble"
    R = "SolidCore.Refinement.Solar.AssembleRec"
    ex = header(
        [f"{pm}.Stages", f"{gm}.Bytecode", R],
        f"The bytecode of `{contract}` runs as its final EVM IR does, by the assembly checker with "
        "frames (AssembleRec.checkF) on an inferred certificate the kernel's evaluation of the checker "
        "re-derives: the recursive function's dynamic jumps return through the frames its call sites "
        "push, and its cut points' heights come from the last stage's relation, which bounds the "
        "stack below a frame by the recursion's measure. With the backend stages read forwards, "
        "every end of the first lowered MIR is the bytecode's, at a gas the checker bounds.",
        ns,
        f"SolarTV {exported} {R}",
    )
    ex += (
        "/-- The runtime bytecode, as bytes. -/\n"
        "def code : ByteArray := ⟨(bytecode.map UInt8.ofNat).toArray⟩\n\n"
        "/-- The checker's certificate (tools/certify/asm_cert.py, infer_frames). -/\n"
        f"def cert : {A}.Cert :=\n  {cert}\n\n"
        "/-- The highest stack each block is entered with; at the heads, the run's bound. -/\n"
        f"def maxs : List Nat := {maxs}\n\n"
        "/-- The recursive function's cut points: its entry and where it resumes after its calls. -/\n"
        f"def heads : List Nat := {hl}\n\n"
        "/-- The recursive function's blocks and its call sites: each return label with the types of "
        "the words its call\nkeeps below the callee and where the caller's own label is among them. -/\n"
        f"def frames : Frames := {frames}\n\n"
        "set_option maxRecDepth 100000 in\n"
        "/-- The checker with frames accepts the bytecode for the final EVM IR: one kernel evaluation "
        "of its fast form. -/\n"
        f"theorem check_ok : checkF {d} code cert maxs heads frames = true :=\n"
        "  checkF_of_checkFS (by decide +kernel)\n\n"
        "/-- The run's bound at the heads, from the last stage's relation: every cut point the final "
        "EVM IR reaches is\nrelated to one of the stage before it (reach_cut), whose relation bounds "
        "the stack below the frame. -/\n"
        f"theorem heads_ok (o : Machine.Oracle) (call : Call) : HeadsOk o call {d} maxs heads := by\n"
        "  intro k x hk hlt hhd\n"
        f"  have hS : SolarTV.CutPoint.Sim (EVM.Program.block o call {p}) {d}_cutA\n"
        f"      (EVM.Program.block o call {d}) {d}_cutB Machine.Agree\n"
        f"      (fun x x' => ∃ P ∈ {d}_rels call, P x x') := ({d}_sim o).sim call trivial\n"
        f"  have hx : {alts} := by simp [heads] at hhd; omega\n"
        "  rcases Nat.eq_zero_or_pos k with rfl | hk1\n"
        "  · simp only [SolarTV.CutPoint.steps, Option.some.injEq, Sum.inr.injEq] at hk\n"
        "    subst hk\n    simp at hx\n"
        f"  have hcut : {d}_cutB x = true := by\n"
        "    obtain ⟨b, st, s⟩ := x\n    simp only at hx\n"
        f"    rcases hx with {rf} <;> rfl\n"
        "  obtain ⟨y, P, hP, hPy⟩ := reach_cut hS k (0, [], Machine.State.initial call)\n"
        f"    (0, [], Machine.State.initial call) (({d}_sim o).start call trivial) x hk1 hk hcut\n"
        f"  simp only [{d}_rels, List.mem_cons, List.not_mem_nil, or_false] at hP\n"
        f"  rcases hP with {' | '.join(['rfl'] * len(pats))}\n"
        + "".join(cases)
        + "\n/-- **The assembly stage**: whenever the final EVM IR's run ends within `n` blocks, the "
        "bytecode at any gas from the\nchecker's bound for `n` blocks ends as it does. -/\n"
        f"theorem {label}_bytecode (o : Machine.Oracle) (ho : cert.env = true → {A}.OracleOk o)\n"
        f"    (he : cert.reads = true → {A}.EnvOk o) (call : Call) (n : Nat)\n"
        f"    (ends : ∃ r, Machine.iterate (EVM.Program.block o call {d}) n (0, [], Machine.State.initial call) =\n"
        "      some r)\n"
        "    {gas : Nat} (hg : SolidCore.Refinement.Solar.AssembleLoop.gasBoundL cert n ≤ gas)\n"
        f"    (obs : Observable) (hrun : (evmirSem o).run {d} call = some obs) :\n"
        "    SolidCore.Refinement.Final.Target.runWith gas code call = some obs :=\n"
        "  stage_of_checkF_gas check_ok o ho he call (heads_ok o call) n ends hg obs hrun\n\n"
        "/-- **The first lowered MIR to the exact bytecode**: every end of the first lowered MIR on a "
        "call is matched by an\nagreeing end of the final EVM IR within some number of blocks, which "
        "the bytecode ends with at every gas from\nthe checker's bound for them. -/\n"
        f"theorem {label}_bytecode_of_mir (o : Machine.Oracle) (ho : cert.env = true → {A}.OracleOk o)\n"
        f"    (he : cert.reads = true → {A}.EnvOk o) (call : Call) (r : Machine.Halt × Machine.State)\n"
        f"    (hends : MIR.Lowered.moduleLayer.Ends o {first['def']} call r) :\n"
        "    ∃ r' n, Machine.Agree r r' ∧ ∀ gas, SolidCore.Refinement.Solar.AssembleLoop.gasBoundL cert n ≤ gas →\n"
        "      SolidCore.Refinement.Final.Target.runWith gas code call = some (Machine.observe call r'.2 r'.1) := by\n"
        f"  obtain ⟨r', ⟨n, hn⟩, hag⟩ := {label}_certified o call trivial r hends\n"
        f"  refine ⟨r', n, hag, fun gas hg => {label}_bytecode o ho he call n ⟨r', hn⟩ hg _ ?_⟩\n"
        f"  change (EVM.Program.layer.result o {d} call).map _ = _\n"
        "  rw [EVM.Program.layer.result_of_ends ⟨n, hn⟩]\n  rfl\n\n"
        + axioms_check(ns, "check_ok", "[propext, Quot.sound]")
        + "\n"
        + axioms_check(ns, f"{label}_bytecode")
        + "\n"
        + axioms_check(ns, f"{label}_bytecode_of_mir")
        + f"\nend {ns}\n"
    )
    return ex


def write_cut_points(
    args, manifest, stages, ns, opens, label, every_doc, scope="anyCall", scope_def=""
):
    """The proofs of a contract whose programs loop: per pair of consecutive stages a LayerSim
    through the cut points cutpoints.py derives from what the compiler kept across the pass,
    and the Stages module that chains their certificates on every call, both ways."""
    gm, pm, out = args.gen_module, args.proof_module, args.proof_dir
    contract = manifest["contract"]
    (out / "Runs").mkdir(parents=True, exist_ok=True)
    pairs = list(itertools.pairwise(stages))
    first, last = stages[0], stages[-1]
    for a, b in pairs:
        p = b["def"]
        try:
            body = cutpoints.stage_body(
                args.gen,
                a | {"file": a["module"] + ".lean"},
                b | {"file": b["module"] + ".lean"},
                int(args.heartbeats),
                int(args.recdepth) if args.recdepth else None,
            )
        except cutpoints.Unsupported as e:
            raise SystemExit(f"error: no cut points for {a['def']} to {b['def']}: {e}")
        what = (
            "Stack scheduling: the last lowered MIR and the scheduled EVM IR"
            if b["kind"] == "sched"
            else f"`{b['pass']}`: its input and output"
        )
        body += (
            f"/-- Every end of `{a['def']}` on a call is matched by an agreeing end of "
            f"`{p}`. -/\n"
            f"theorem {p}_certified (o : Oracle) :\n"
            f"    Certified {layer(a)} {layer(b)} o {a['def']} {p} {cutpoints.scope_parts()[4]} :=\n"
            f"  ({p}_sim o).certified\n\n"
            f"/-- Every end of `{p}` on a call is matched by an agreeing end of "
            f"`{a['def']}`. -/\n"
            f"theorem {p}_certified_back (o : Oracle) :\n"
            f"    Certified {layer(b)} {layer(a)} o {p} {a['def']} {cutpoints.scope_parts()[4]} :=\n"
            f"  ({p}_sim o).certified_back\n\n"
        )
        base = [
            "W1Proofs.Segment",
            "W1Proofs.Stage",
            "W1Proofs.Axioms",
            "SolarTV.Termination",
            # A recursive function's stages cut at its call frames (rec_cuts.py) close their
            # segments with rec_seg.
            *(["Verify.Phase3.RecSeg"] if rec_cuts.is_recursive(args.gen) else []),
            f"{gm}.{b['module']}",
            f"{gm}.{a['module']}",
        ]
        body, imports = split_segments(
            out, pm, b["module"], p, body, base, ns, opens, what
        )
        (out / "Runs" / f"{b['module']}.lean").write_text(
            header(
                imports,
                f"{what} simulated through cut points on every call: from every pair of "
                "configurations the relations derived from the compiler relate, the two "
                "loop-free segments end alike (`sym_seg`), so their runs end in agreeing "
                "results for any number of loop iterations, both ways.",
                ns,
                opens,
            )
            + "open SolarTV.CutPoint (seg SegRel)\n\n"
            + body
            + "".join(
                f"/-- info: '{ns}.{n}' depends on axioms within {AXIOMS} -/\n"
                f"#guard_msgs in\n#axioms_within {n}\n\n"
                for n in (f"{p}_certified", f"{p}_certified_back")
            )
            + f"end {ns}\n"
        )
    mirs = [s for s in stages if s["kind"] == "mir"]
    sched = next(s for s in stages if s["kind"] == "sched")
    evms = [s for s in stages if s["kind"] == "evm"]

    def steps(xs):
        return (
            "trivial"
            if not xs
            else "⟨" + ", ".join(f"{x['def']}_certified o" for x in xs) + ", trivial⟩"
        )

    final = f"{label}_mir_to_evmir"
    body = ([scope_def] if scope_def else []) + [
        (
            f"/-- The compilation from the first lowered MIR to the final EVM IR. -/\n"
            f"def compilation : Compilation :=\n"
            f"  ⟨{first['def']}, [{', '.join(m['def'] for m in mirs[1:])}], {sched['def']}, "
            f"[{', '.join(e['def'] for e in evms)}]⟩\n"
        ),
        (
            f"/-- Every stage from the first lowered MIR to the final EVM IR, each through cut "
            f"points, chained by the library's composition, on every call. -/\n"
            f"theorem {final} (o : Oracle) : Stage (mirSem o) (evmirSem o) {first['def']} "
            f"{last['def']} {scope} :=\n"
            f"  mir_to_evmir_certified o {scope} compilation {steps(mirs[1:])} "
            f"({sched['def']}_certified o)\n    {steps(evms)}\n"
        ),
    ]
    fwd = f"{stages[1]['def']}_certified o"
    for s in stages[2:]:
        fwd = f"({fwd}).trans ({s['def']}_certified o)"
    back = f"{stages[-1]['def']}_certified_back o"
    for s in reversed(stages[1:-1]):
        back = f"({back}).trans ({s['def']}_certified_back o)"
    body.append(
        f"/-- Every end of the first lowered MIR is matched by an agreeing end of the final "
        f"EVM IR, on every call. -/\n"
        f"theorem {label}_certified (o : Oracle) :\n"
        f"    Certified MIR.Lowered.moduleLayer EVM.Program.layer o {first['def']} "
        f"{last['def']} {scope} :=\n  {fwd}\n"
    )
    body.append(
        f"/-- Every end of the final EVM IR is matched by an agreeing end of the first "
        f"lowered MIR, on every call: one ends exactly when the other does. -/\n"
        f"theorem {label}_certified_back (o : Oracle) :\n"
        f"    Certified EVM.Program.layer MIR.Lowered.moduleLayer o {last['def']} "
        f"{first['def']} {scope} :=\n  {back}\n"
    )
    checks = "".join(
        "\n" + axioms_check(ns, n)
        for n in (final, f"{label}_certified", f"{label}_certified_back")
    )
    imports = [f"{pm}.Runs.{s['module']}" for s in stages[1:]] + [
        "W1Proofs.Stage",
        "SolarTV.Composition",
    ]
    doc = (
        f"The backend stages of `{contract}`, each proved on every call (`anyCall`: any "
        "calldata, call value and storage) under every oracle through cut points, whatever "
        "number of times its loops run, and chained from the first lowered MIR "
        f"`{first['def']}` to the final EVM IR `{last['def']}`, both ways."
    )
    (out / "Stages.lean").write_text(
        header(imports, doc, ns, opens) + "\n".join(body) + checks + f"\nend {ns}\n"
    )
    return final


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("gen", type=pathlib.Path)
    ap.add_argument("--gen-module", required=True)
    ap.add_argument("--proof-dir", required=True, type=pathlib.Path)
    ap.add_argument("--proof-module", required=True)
    ap.add_argument("--namespace", required=True)
    ap.add_argument("--label", required=True)
    ap.add_argument(
        "--recdepth",
        default=None,
        help="each cut-point segment proof's maxRecDepth, for stages whose symbolic "
        "segments nest deeper than Lean's default",
    )
    ap.add_argument(
        "--heartbeats",
        default="2000000",
        help="each run proof's maxHeartbeats, four times this for a program with "
        "internal calls, whose runs revisit the callees' blocks",
    )
    ap.add_argument(
        "--loops",
        type=int,
        default=0,
        help="a bound on the iterations a run's loops make in all: each fuel is "
        "multiplied by one more than it, a claim the end proof checks",
    )
    ap.add_argument(
        "--exclude",
        action="append",
        default=[],
        metavar="SELECTOR=SIGNATURE",
        help="leave the calls of this external function out of every theorem's scope: "
        "each run proof assumes the call's selector word is not SELECTOR (hex)",
    )
    ap.add_argument(
        "--axioms",
        default="{}",
        help="JSON: theorem name to its expected axiom list, where it is not " + AXIOMS,
    )
    ap.add_argument(
        "--cut",
        action="store_true",
        help="prove every stage through cut points even when no program loops: with --joins, "
        "one segment proof per join point and module, for programs too large for one "
        "symbolic run per stage",
    )
    ap.add_argument(
        "--segment",
        type=int,
        default=None,
        help="with --joins, the most configurations one EVM IR segment evaluates before it is "
        "cut again (default: cutpoints.SEGMENT)",
    )
    ap.add_argument(
        "--no-mir-joins",
        action="store_true",
        help="with --joins, keep the MIR passes' stages whole (one segment from the entry): "
        "a pass that moves reads across blocks (place-reads) changes which values are live "
        "at a join, so the join relations do not hold there",
    )
    ap.add_argument(
        "--joins",
        action="store_true",
        help="cut the programs at their join points too, so that no path is evaluated twice",
    )
    args = ap.parse_args()
    cutpoints.JOINS = args.joins
    cutpoints.EXCLUDED_SELECTORS = {int(e.partition("=")[0], 16) for e in args.exclude}
    cutpoints.EXCLUDED_FUNCTIONS = {
        e.partition("=")[2].partition("(")[0] for e in args.exclude
    }
    cutpoints.MIR_JOINS = not args.no_mir_joins  # ty: ignore[invalid-assignment]
    if args.segment is not None:
        cutpoints.SEGMENT = args.segment
    manifest, stages = stage_list(args.gen, args.loops)
    global DEPTH
    if any(
        "set_option maxRecDepth" in (args.gen / f"{s['module']}.lean").read_text()
        for s in stages
    ):
        DEPTH = "set_option maxRecDepth 100000\n\n"
    contract = manifest["contract"]
    gm, pm, ns, label = args.gen_module, args.proof_module, args.namespace, args.label
    expected = json.loads(args.axioms)
    exported = export_ns.of_gen(args.gen)
    # sym_exec's steps are unhygienic: their lemmas resolve where it runs.
    opens = f"Machine {exported}" + (
        "" if ns == "SolarTV.Milestone.W1" else " SolarTV.Milestone.W1"
    )
    out = args.proof_dir
    (out / "Runs").mkdir(parents=True, exist_ok=True)
    calls = any(s["calls"] for s in stages)
    hb = int(args.heartbeats) * (4 if calls else 1)
    excluded = []
    for e in args.exclude:
        sel, _, sig = e.partition("=")
        if not re.fullmatch(r"[0-9a-f]{8}", sel) or not sig:
            raise SystemExit(f"error: --exclude {e} is not SELECTOR=SIGNATURE")
        excluded.append((int(sel, 16), sig))
    # A run proof's assumptions: the call's selector word is none of the excluded ones.
    hyps = "".join(
        f" (hsel{i} : EvmRules.Evm.shr 224 (calldataWord call 0) ≠ {sel:#x})"
        for i, (sel, _) in enumerate(excluded)
    )
    scope = "scope" if excluded else "anyCall"
    lam = "fun call h" if excluded else "fun call _"
    every = (
        "every call"
        if not excluded
        else "every call but those of " + ", ".join(f"`{sig}`" for _, sig in excluded)
    )

    def given(h):
        """The run proofs' assumptions from `h : scope call`."""
        return "".join(
            " " + h + ".2" * i + (".1" if i < len(excluded) - 1 else "")
            for i in range(len(excluded))
        )

    # Both orientations of each assumption, so that the case split's test of the selector in
    # either order rewrites to false and the path of an excluded function ends there.
    prune = "".join(
        f"  have hsel{i}s := Ne.symm hsel{i}\n" for i in range(len(excluded))
    )

    def write(path, text):
        path.write_text(text)

    def ax(name):
        """A run's axioms: pinned when given, else checked to be within the standard three."""
        if name in expected:
            return axioms_check(ns, name, expected[name])
        return (
            f"/-- info: '{ns}.{name}' depends on axioms within {AXIOMS} -/\n"
            f"#guard_msgs in\n#axioms_within {name}\n"
        )

    cut = args.cut or (loops_somewhere(args.gen, stages) and not excluded)
    if cut:
        scope_term, scope_def = "anyCall", ""
        if excluded:
            conj = " ∧ ".join(
                f"EvmRules.Evm.shr 224 (calldataWord call 0) ≠ {sel:#x}"
                for sel, _ in excluded
            )
            # Each segment proof assumes the selector word is none of the excluded ones, so the
            # paths of their functions end at the dispatcher's test.
            cutpoints.SCOPE = {  # ty: ignore[invalid-assignment]
                "hyps": hyps,
                "names": "".join(f" hsel{i}" for i in range(len(excluded))),
                "prune": prune,
                "given": given,
                "term": f"(fun call => {conj})",
            }
            scope_term = "scope"
            scope_def = (
                "/-- The calls the stages are about: every call whose selector word is none of "
                + ", ".join(f"`{sel:#010x}` (`{sig}`)" for sel, sig in excluded)
                + ", whose functions are outside these proofs. -/\n"
                f"def scope : Call → Prop := fun call =>\n  {conj}\n"
            )
        final = write_cut_points(
            args, manifest, stages, ns, opens, label, every, scope_term, scope_def
        )
        last = stages[-1]
        first = stages[0]
    else:
        # The first MIR stage ends.
        first = stages[0]
        ends = f"{first['def']}_ends"
        write(
            out / "Runs" / f"{first['module']}.lean",
            header(
                ["W1Proofs.Tactic", "W1Proofs.Axioms", f"{gm}.{first['module']}"],
                f"The first lowered MIR ends on {every} within {first['fuel']} blocks, "
                + (
                    "the number of its blocks"
                    if first["calls"] == 0
                    else "its block count times the square of one more than its call sites"
                )
                + (
                    "."
                    if args.loops == 0
                    else f", times {args.loops + 1}: its loops run at most {args.loops} iterations."
                ),
                ns,
                opens,
            )
            + f"set_option maxHeartbeats {hb} in\n"
            f"theorem {ends} (o : Oracle) (call : Call){hyps} :\n"
            f"    ({run_fn(first)} o {first['def']} call {first['fuel']}).isSome = true := by\n"
            + prune
            + "  sym_exec\n\n"
            + ax(ends)
            + f"\nend {ns}\n",
        )

        # Each pair of consecutive stages ends in the same result.
        pairs = list(itertools.pairwise(stages))
        for a, b in pairs:
            name = f"{b['def']}_run"
            what = (
                "Stack scheduling: the last lowered MIR and the scheduled EVM IR"
                if b["kind"] == "sched"
                else f"`{b['pass']}`: its input and output"
            )
            write(
                out / "Runs" / f"{b['module']}.lean",
                header(
                    [
                        "W1Proofs.Tactic",
                        "W1Proofs.Axioms",
                        f"{gm}.{b['module']}",
                        f"{gm}.{a['module']}",
                    ],
                    f"{what} end in the same result on {every}.",
                    ns,
                    opens,
                )
                + f"set_option maxHeartbeats {hb} in\n"
                f"theorem {name} (o : Oracle) (call : Call){hyps} :\n"
                f"    {run_fn(a)} o {a['def']} call {a['fuel']} = "
                f"{run_fn(b)} o {b['def']} call {b['fuel']} := by\n"
                + prune
                + "  sym_exec\n\n"
                + ax(name)
                + f"\nend {ns}\n",
            )

        # Stages and their composition.
        mirs = [s for s in stages if s["kind"] == "mir"]
        sched = next(s for s in stages if s["kind"] == "sched")
        evms = [s for s in stages if s["kind"] == "evm"]
        last = stages[-1]
        body = []
        prev_ends = f"({lam} => {ends} o call{given('h')})"
        if excluded:
            body.append(
                "/-- The calls the stages are about: every call whose selector word is none of "
                + ", ".join(f"`{sel:#010x}` (`{sig}`)" for sel, sig in excluded)
                + ", whose functions are outside these proofs. -/\n"
                "def scope : Call → Prop := fun call =>\n  "
                + " ∧ ".join(
                    f"EvmRules.Evm.shr 224 (calldataWord call 0) ≠ {sel:#x}"
                    for sel, _ in excluded
                )
                + "\n"
            )
        for a, b in pairs:
            stage = (
                f"theorem stage_{b['def']} (o : Oracle) : Stage ({sem(a)} o) ({sem(b)} o) "
                f"{a['def']} {b['def']} {scope} :=\n"
                f"  stage_of_runs (A := {layer(a)}) (B := {layer(b)}) {a['fuel']} {b['fuel']} "
                f"{prev_ends}\n    ({lam} => {b['def']}_run o call{given('h')})\n"
            )
            body.append(f"/-- `{b['pass']}`: {a['def']} to {b['def']}. -/\n" + stage)
            if b is not last:
                body.append(
                    f"theorem {b['def']}_ends (o : Oracle) : ∀ call, {scope} call →\n"
                    f"    ({run_fn(b)} o {b['def']} call {b['fuel']}).isSome = true :=\n"
                    f"  ends_of_runs (A := {layer(a)}) (B := {layer(b)})\n"
                    f"    (o := o) (p := {a['def']}) (q := {b['def']}) (n := {a['fuel']}) "
                    f"(m := {b['fuel']}) {prev_ends}\n"
                    f"    ({lam} => {b['def']}_run o call{given('h')})\n"
                )
                prev_ends = f"({b['def']}_ends o)"

        def steps(xs):
            return (
                "trivial"
                if not xs
                else "⟨" + ", ".join(f"stage_{x['def']} o" for x in xs) + ", trivial⟩"
            )

        final = f"{label}_mir_to_evmir"
        body.append(
            f"/-- The compilation from the first lowered MIR to the final EVM IR. -/\n"
            f"def compilation : Compilation :=\n"
            f"  ⟨{first['def']}, [{', '.join(m['def'] for m in mirs[1:])}], {sched['def']}, "
            f"[{', '.join(e['def'] for e in evms)}]⟩\n"
        )
        body.append(
            f"/-- Every stage from the first lowered MIR to the final EVM IR, chained by the "
            f"library's composition, on {every}. -/\n"
            f"theorem {final} (o : Oracle) : Stage (mirSem o) (evmirSem o) {first['def']} "
            f"{last['def']} {scope} :=\n"
            f"  mir_to_evmir o {scope} compilation {steps(mirs[1:])} (stage_{sched['def']} o)\n"
            f"    {steps(evms)}\n"
        )
        checks = (
            "".join("\n" + axioms_check(ns, f"stage_{b['def']}") for _, b in pairs)
            + "\n"
            + axioms_check(ns, final)
        )
        imports = [f"{pm}.Runs.{s['module']}" for s in stages] + [
            "W1Proofs.Stage",
            "SolarTV.Composition",
        ]
        doc = (
            f"The backend stages of `{contract}`, each proved on "
            + (
                "every call (`anyCall`: any calldata, call value and storage)"
                if not excluded
                else f"{every} (`scope`: any other calldata, any call value and storage)"
            )
            + " under every oracle, and chained from the first "
            f"lowered MIR `{first['def']}` to the final EVM IR `{last['def']}`. Fuels are the "
            + (
                "programs' block counts."
                if all(s["calls"] == 0 for s in stages)
                else "programs' block counts times the square of one more than their call sites."
            )
        )
        write(
            out / "Stages.lean",
            header(imports, doc, ns, opens)
            + "\n".join(body)
            + checks
            + f"\nend {ns}\n",
        )

    # The final EVM IR against the bytecode.
    bytecode_text = (args.gen / "Bytecode.lean").read_text()
    found = re.search(r'def bytecodeHex : String :=\n  "([0-9a-f]+)"', bytecode_text)
    if found is None:
        raise SystemExit("error: Bytecode.lean defines no bytecodeHex")
    hexcode = found.group(1)
    code = bytes.fromhex(hexcode)
    # The program data after the code (a created contract's code) is no instruction: the walk
    # decodes `bytecode`, which ends where `dataStart` says the data begins.
    data_start = re.search(r"def dataStart : Nat := (\d+)", bytecode_text)
    if data_start is not None:
        code = code[: int(data_start.group(1))]
    # The walk of a bytecode of more than a kilobyte runs on the code's suffixes (walkAt): walk
    # indexes the code from its start at each instruction, quadratic in its length.
    linear = len(code) > 1024
    dests, ops, pc = [], [], 0
    while pc < len(code):
        op = code[pc]
        width = op - 0x5F if 0x60 <= op <= 0x7F else 0
        if op == 0x5B:
            dests.append(pc)
        ops.append((op, int.from_bytes(code[pc + 1 : pc + 1 + width], "big")))
        pc += 1 + width
    last_text = (args.gen / f"{last['module']}.lean").read_text()
    # Every block a static jump targets, whose label is pushed (a call's return) or that a jump
    # table lists has a JUMPDEST, as `W1Proofs.Items.itemsWith` spells it: a block's final
    # transfer to the block after it falls through, with no label.
    tables = [
        [int(t) for t in m.split(",")]
        for m in re.findall(r"\.jumpTable \[([\d, ]+)\]", last_text)
    ]
    targets = sorted(jump_targets(last_text) | {t for table in tables for t in table})
    # Each jump table's encoding, in layout order, from its pick in the bytecode: a packed word
    # of one-byte offsets (PUSHn table; SWAP1; BYTE; JUMP) or a stride table (PUSH1 stride; MUL;
    # PUSH entry; ADD; JUMP), whose entry stubs follow the last block, each at a JUMPDEST, the
    # first named by a label past every block's. These are claims the walk checks.
    is_push = lambda op: 0x60 <= op <= 0x7F
    spellings, stubs, fresh = [], [], blocks(last_text)
    for i in range(len(ops)):
        window = [op for op, _ in ops[i : i + 5]]
        if is_push(window[0]) and window[1:4] == [0x90, 0x1A, 0x56]:
            spellings.append(".packed")
        elif (
            window[0] == 0x60
            and ops[i][1] > 3
            and len(window) == 5
            and is_push(window[2])
            and [window[1], window[3], window[4]] == [0x02, 0x01, 0x56]
        ):
            spellings.append(f"(.stride {ops[i][1] - 3})")
            stubs.append(fresh)
            fresh += 1
    if len(spellings) != len(tables):
        raise SystemExit(
            f"error: {len(tables)} jump tables but {len(spellings)} picks in an "
            "encoding the assembly check spells (packed bytes or a stride table)"
        )
    labelled = list(targets)
    stride_tables = iter(t for t, s in zip(tables, spellings) if s != ".packed")
    for label in stubs:
        labelled += [label] + [None] * (len(next(stride_tables)) - 1)
    if len(labelled) != len(dests):
        raise SystemExit(
            f"error: {len(labelled)} jump targets but {len(dests)} JUMPDESTs"
        )
    offset = "0"
    for t, d in reversed([(t, d) for t, d in zip(labelled, dests) if t is not None]):
        offset = f"if label = {t} then {d:#x} else {offset}"
    items_of = (
        f"itemsWith [{', '.join(spellings)}] {last['def']}"
        if tables
        else f"itemsOf {last['def']}"
    )
    immutables = manifest.get("immutables", [])
    asm = header(
        ["W1Proofs.Items"]
        + (["W1Proofs.WalkAt"] if linear else [])
        + [f"{gm}.{last['module']}", f"{gm}.Bytecode"]
        + ([f"{gm}.Manifest", "SolarTV.Stages.Link"] if immutables else []),
        f"The final EVM IR of `{contract}` against its bytecode, by the assembly "
        "checker's walk: the bytecode decodes, linearly from offset 0, to exactly the "
        "program's instructions, each jumped-to block at its `JUMPDEST`.",
        ns,
        f"SolarTV.Assembly {exported} SolarTV.Milestone.W1",
    )
    # The decisions evaluate the items and the walk recursively, item by item. The kernel
    # evaluates the items of a program of more than a hundred blocks or with environment reads,
    # whose spelling compares their names: the elaborator's `decide` takes many minutes over either.
    items_decide = (
        "decide +kernel"
        if blocks(last_text) > 100 or '.effect (.env "' in last_text
        else "decide"
    )
    asm += "" if DEPTH else "set_option maxRecDepth 100000\n\n"
    asm += (
        f"/-- The items of the final EVM IR. -/\n"
        f"def items : List Item := ({items_of}).getD []\n\n"
        f"theorem items_eq : {items_of} = some items := by {items_decide}\n\n"
        f"/-- The layout: each jumped-to block at the bytecode's next `JUMPDEST`. The walk "
        f"checks it. -/\n"
        f"def offset (label : Nat) : Nat := {offset}\n\n"
        f"/-- The instructions the walk reaches, each at its offset. -/\n"
        + (
            f"def stream : List (Nat × Instr) :=\n"
            f"  (walkAt {len(code)} offset 0 bytecode items).getD []\n\n"
            f"/-- The checker's walk on the bytecode's suffixes, which the kernel evaluates in "
            f"time linear in its length. -/\n"
            f"theorem walkAt_eq : walkAt {len(code)} offset 0 bytecode items = some stream := by\n"
            f"  decide +kernel\n\n"
            f"/-- The checker's walk accepts the exported bytecode against the final EVM IR. -/\n"
            f"theorem walk_eq : walk bytecode offset 0 items = some stream :=\n"
            f"  walk_of_walkAt (by decide +kernel) walkAt_eq\n\n"
            if linear
            else "def stream : List (Nat × Instr) := (walk bytecode offset 0 items).getD []\n\n"
            "/-- The checker's walk accepts the exported bytecode against the final EVM IR. -/\n"
            # The kernel evaluates the walk: the elaborator's `decide` runs out of recursion
            # depth on longer bytecode.
            "theorem walk_eq : walk bytecode offset 0 items = some stream := by decide +kernel\n\n"
        )
        + "theorem checked : Walk bytecode offset 0 items stream := walk_sound walk_eq\n\n"
        "/-- The bytecode's linear decoding is the final EVM IR's instructions at the "
        "layout. -/\n"
        "theorem decodes : decodeFrom bytecode (bytecode.length + 1) 0 = stream :=\n"
        "  walk_decodes checked _ (by have := walk_length checked; omega)\n\n"
        + axioms_check(ns, "walk_eq", "[propext, Quot.sound]")
        + "\n"
        + axioms_check(ns, "decodes", "[propext, Quot.sound]")
    )
    if immutables:
        # The bytecode a deployment runs is this one linked at the immutables' placeholders.
        asm += (
            "\n/-- The placeholders of the final EVM IR's immutables in the bytecode: each one's "
            "immutable, the offset\nof its bytes and their count. -/\n"
            "def placeholders : List Placeholder := placeholdersOf stream items\n\n"
            "/-- They are the ones the manifest lists. -/\n"
            "theorem placeholders_eq :\n"
            "    placeholders.map (fun p => (p.id, p.start, p.bytes)) = Manifest.immutables := by\n"
            "  decide +kernel\n\n"
            "/-- Each placeholder is a push of its width. -/\n"
            "theorem placed_eq : placed bytecode stream items = true := by decide +kernel\n\n"
            "/-- Every bytecode a deployment runs, the bytecode linked with any values of the "
            "immutables, decodes to\nthe final EVM IR's instructions, each placeholder pushing "
            "its immutable's value. -/\n"
            "theorem decodes_linked (values : Nat → Nat) :\n"
            "    decodeFrom (link bytecode placeholders values) (bytecode.length + 1) 0 =\n"
            "      linkedStream values stream items :=\n"
            "  SolarTV.Assembly.decodes_linked checked placed_eq values\n\n"
            "theorem linked_items (values : Nat → Nat) :\n"
            "    (linkedStream values stream items).map Prod.snd =\n"
            "      items.map (Item.linked offset values) :=\n"
            "  linked_instructions checked values\n\n"
            + axioms_check(ns, "decodes_linked")
        )
    asm += f"\nend {ns}\n"
    write(out / "Assembly.lean", asm)
    # The bytecode runs as the final EVM IR does: the assembly checker for code with loops
    # (AssembleLoop.checkL) on a certificate tools/certify/asm_cert.py infers, which the kernel's
    # evaluation of the checker re-derives, composed with the backend stages read backwards.
    executes, rec_exec = None, False
    outside = asm_cert.outside(last_text)
    # Memory at offsets the constant-offset checker cannot bound, GAS or CALLDATACOPY: the checker
    # whose memory operations run at the frame's own gas (AssembleCut.checkC), whose theorems hold at
    # every gas below 2 ^ 256.
    # A recursive program's heights settle only with its cut points as heads (below).
    recursive = rec_cuts.is_recursive(args.gen)
    # The certificate is inferred only for the execution theorem, and the inference is untrusted: a
    # program it finds no certificate for (heights the checker cannot bound, such as a return the
    # checker sends to every return label) gets the stage proofs without the execution theorem.
    dyn, cert, why = False, None, None
    if cut:
        try:
            dyn = outside is not None or (
                not recursive and not asm_cert.static_ok(last_text, code)
            )
            if dyn:
                outside = asm_cert.outside(last_text, dyn=True)
            if not outside and not recursive:
                cert, maxs = asm_cert.lean_cert(asm_cert.infer(last_text, code, dyn))
        except SystemExit as e:
            why = str(e.code).removeprefix("error: ")
    if not cut:
        # The execution theorem reads the stages backwards (LABEL_certified_back), which only
        # the cut-point composition writes; loop-free stages compose forwards only.
        print(
            "assembly: decoded only; the loop-free stages have no backward composition to run "
            "the bytecode with",
            file=sys.stderr,
        )
    elif outside:
        print(
            f"assembly: decoded only; the final EVM IR is outside the executed fragment: {outside}",
            file=sys.stderr,
        )
    elif recursive:
        # A recursive function's cut points are the checker's heads and its dynamic jumps return
        # through its frames (AssembleRec.checkF): the run bounds the heads' entry heights by the
        # measure, and the frame relation types the caller's words where it resumes.
        why = None
        if excluded or dyn:
            why = (
                "the checker with frames (AssembleRec.checkF) takes no excluded calls"
                if excluded
                else "the checker with frames bounds memory at constant offsets only"
            )
        else:
            try:
                ex = rec_execution(
                    args, ns, exported, label, contract, stages, last_text, code, gm, pm
                )
            except (SystemExit, rec_cuts.Unsupported) as e:
                why = str(e).removeprefix("error: ")
        if why is not None:
            print(f"assembly: decoded only; {why}", file=sys.stderr)
        else:
            executes, rec_exec = f"{label}_bytecode", True
            write(out / "Execution.lean", ex)
    elif cert is None:
        print(
            f"assembly: decoded only; no certificate for the assembly checker: {why}",
            file=sys.stderr,
        )
    else:
        executes = f"{label}_bytecode"
        A, L = (
            "SolidCore.Refinement.Solar.Assemble",
            "SolidCore.Refinement.Solar.AssembleLoop",
        )
        C = "SolidCore.Refinement.Solar.AssembleCut"
        ex = header(
            [
                f"{pm}.Stages",
                f"{gm}.Bytecode",
                "SolidCore.Refinement.Solar.AssembleFast",
                "SolidCore.Refinement.Solar.EndToEnd",
                "SolidCore.Refinement.Solar.RunOracle",
            ]
            + (
                [C, "SolidCore.Refinement.Solar.AssembleCutCheckFast"]
                if dyn
                else ["SolidCore.Refinement.Solar.AssembleCheckFast"]
            ),
            f"The bytecode of `{contract}` runs as its final EVM IR does, at every gas, by the "
            "assembly checker for code with loops on an inferred certificate the kernel's evaluation "
            "of the checker re-derives; with the backend stages read backwards, every result of the "
            "bytecode at any gas is the first lowered MIR's or a failure that changes nothing.",
            ns,
            f"SolarTV {exported}",
        )
        ex += (
            "/-- The runtime bytecode, as bytes. -/\n"
            "def code : ByteArray := ⟨(bytecode.map UInt8.ofNat).toArray⟩\n\n"
            "/-- The checker's certificate (tools/certify/asm_cert.py): each labelled block's offset, "
            "each block's lowest\nentry height and entry words, its budget, the memory bound, the "
            "turned branches, the pushed labels and\nwhether the program reads CALLER or hashes. -/\n"
            f"def cert : {A}.Cert :=\n  {cert}\n\n"
            "/-- The highest stack each block is entered with. -/\n"
            f"def maxs : List Nat := {maxs}\n\n"
            # One kernel evaluation of the checker's fast form: the walk on the code's suffixes,
            # which implies the labels' check (Solar/AssembleCheckFast.lean, AssembleCutCheckFast.lean).
            "set_option maxRecDepth 100000 in\n"
            + (
                (
                    "/-- The checker with memory at any offset accepts the bytecode for the final EVM IR: "
                    "one kernel\nevaluation of its fast form (the walk on the code's suffixes, which "
                    "implies the labels' check). -/\n"
                    f"theorem check_ok : {C}.checkC {last['def']} code cert maxs = true :=\n"
                    f"  {C}.checkC_of_checkCS (by decide +kernel)\n\n"
                    "/-- **The assembly stage, run by run, at every gas below 2 ^ 256**: an oracle meeting the "
                    "certificate's\nconstraints under which every result of the bytecode is the final EVM IR's or "
                    "a failure that changes\nnothing. -/\n"
                    f"theorem assembly (scope : Call → Prop) :\n"
                    f"    {C}.WithinRunLt evmirSem (fun _ => {last['def']}) code scope\n"
                    "      (SolidCore.Refinement.Solar.RunOracle.CertOk cert) :=\n"
                    f"  {C}.withinRun_of_checkC check_ok scope\n\n"
                    "/-- **The first lowered MIR to the exact bytecode, run by run, at every gas below 2 ^ 256**: "
                    "for every such\ngas and every call there is an oracle, meeting the certificate's constraints "
                    "(CertOk), under which every\nresult of the bytecode is the first lowered MIR's or a failure "
                    "that changes nothing. -/\n"
                    f"theorem {executes}_run :\n"
                    f"    {C}.WithinRunLt mirSem (fun _ => {first['def']}) code anyCall\n"
                    "      (SolidCore.Refinement.Solar.RunOracle.CertOk cert) :=\n"
                    f"  {C}.withinRun_mir_of_checkC {label}_certified_back\n"
                    "    (fun _ _ => trivial) check_ok\n\n"
                )
                if dyn
                else (
                    "/-- The checker of code with loops accepts the bytecode for the final EVM IR: one "
                    "kernel evaluation\nof its fast form (the walk on the code's suffixes, which implies "
                    "the labels' check). -/\n"
                    f"theorem check_ok : {L}.checkL {last['def']} code cert maxs = true :=\n"
                    f"  {L}.checkL_of_checkLS (by decide +kernel)\n\n"
                    "/-- **The assembly stage at every gas.** -/\n"
                    f"theorem assembly (o : Machine.Oracle) (ho : cert.env = true → {A}.OracleOk o)\n"
                    f"    (he : cert.reads = true → {A}.EnvOk o) (scope : Call → Prop) (gas : Nat) :\n"
                    f"    SolidCore.Refinement.Final.Within (evmirSem o) (SolidCore.Refinement.Final.ethLibWith gas)\n"
                    f"      {last['def']} code scope :=\n"
                    f"  {L}.within_of_checkL check_ok o ho scope gas he\n\n"
                    "/-- **The first lowered MIR to the exact bytecode at every gas**: every result of the "
                    "bytecode, on every call, is\nthe first lowered MIR's or a failure that changes nothing"
                    + (
                        ", under every oracle whose caller and hash are EthLib's"
                        if "env := true" in cert
                        else ""
                    )
                    + (
                        (" and" if "env := true" in cert else ", under every oracle")
                        + " whose reads of the block and the transaction are EthLib's (EnvOk)"
                        if "reads := true" in cert
                        else ""
                    )
                    + ". -/\n"
                    f"theorem {executes} (o : Machine.Oracle) (ho : cert.env = true → {A}.OracleOk o)\n"
                    f"    (he : cert.reads = true → {A}.EnvOk o) (gas : Nat) :\n"
                    f"    SolidCore.Refinement.Final.Within (mirSem o) (SolidCore.Refinement.Final.ethLibWith gas)\n"
                    f"      {first['def']} code anyCall :=\n"
                    f"  SolidCore.Refinement.Solar.EndToEnd.within_of_checkL ({label}_certified_back o)\n"
                    "    (fun _ _ => trivial) check_ok ho gas he\n\n"
                    "/-- **The first lowered MIR to the exact bytecode, run by run**: for every gas and every "
                    "call there is an oracle,\nmeeting the certificate's constraints (CertOk), under which "
                    "every result of the bytecode is the first\nlowered MIR's or a failure that changes "
                    "nothing; it composes with specifications proved for every\noracle "
                    "(RunOracle.holds_of_withinRun). -/\n"
                    f"theorem {executes}_run :\n"
                    f"    SolidCore.Refinement.Solar.RunOracle.WithinRun mirSem (fun _ => {first['def']}) code anyCall\n"
                    "      (SolidCore.Refinement.Solar.RunOracle.CertOk cert) :=\n"
                    f"  SolidCore.Refinement.Solar.RunOracle.withinRun_mir_of_checkL {label}_certified_back\n"
                    "    (fun _ _ => trivial) check_ok\n\n"
                )
            )
            + axioms_check(ns, "check_ok", "[propext, Quot.sound]")
            + ("" if dyn else "\n" + axioms_check(ns, executes))
            + "\n"
            + axioms_check(ns, f"{executes}_run")
            + f"\nend {ns}\n"
        )
        write(out / "Execution.lean", ex)
    root = out.parent / f"{out.name}.lean"
    write(
        root,
        "-- Generated by solar's tools/certify/certify.py; do not edit.\n"
        f"import {pm}.Stages\nimport {pm}.Assembly\n"
        + (f"import {pm}.Execution\n" if executes else ""),
    )
    print(
        json.dumps(
            {
                "final": f"{ns}.{final}",
                **({"executes": f"{ns}.{executes}"} if executes and not dyn else {}),
                **({"executes_gas_below": "2^256"} if executes and dyn else {}),
                **(
                    {"executes_run": f"{ns}.{executes}_run"}
                    if executes and not rec_exec
                    else {}
                ),
                **({"executes_mir": f"{ns}.{executes}_of_mir"} if rec_exec else {}),
                "last": last["def"],
                "first": first["def"],
                "stages": [s["def"] for s in stages],
            }
        )
    )


if __name__ == "__main__":
    main()
