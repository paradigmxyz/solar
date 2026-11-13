#!/usr/bin/env python3
"""The proof module of a loop-free generic-stage contract's published statement, generated.

`shape_gen.py SHAPE_JSON --name NAME --certified DIR --out SHAPE_LEAN` writes the Lean module
NAME.Shape: the term `published` that tools/certify/final.py's fixed statement asks for, for a
contract whose source stage is solidity-lean's generic check (`Generic.check`, and
`Generic.checkValue` for calls with a value) and whose final EVM IR the checker of code with loops
(`AssembleLoop.checkL`) accepts against the fresh bytecode. Everything here is untrusted data and
composition: the module applies `SolidCore.Refinement.Shapes.compiled` (or `compiled0`) to

* the source check of the contract's Lowering module (shape.json `generic`),
* solar-tv's backend stages of this run's export (DIR/NAME/Proofs/Stages.lean) and the last
  stage's fuel, which bounds the final EVM IR's run on every call,
* the loop checker's certificate, inferred by tools/certify/asm_cert.py from this run's final
  EVM IR and bytecode,

and, for kind `certified`, the triples shape.json `generic.triples` names (solar-verify's
discharged obligations of the same SolidCore contract). The kernel re-derives every claim: a wrong
certificate, a stale export or a gas bound below the checker's fails the module.

shape.json `generic` keys:
  lowering   the module of the source check (imported)
  ns         its namespace (`contract?`, `fuel`, `mirFuel`, `fns` and the check theorems)
  style      "core" (`deployed_of hc : c.core = coreLit ∧ ∃ d, DeployedAt ...`, `check` about
             `coreLit`) or "checked" (`deployed hc : ∃ d, ...`, `checks` of `checkOn` per function)
  check, check_value   the check theorems (style core); `check_value` absent: no value calls
  checks     list of [fn, theorem] (style checked)
  fns        the functions term; storage  the constructor's storage term
  triples    (kind certified) list of terms `hc ↦ Triple ...`, written with `hc` and the
             statement's premises `_h0`, `_h1`, …
"""

import argparse
import json
import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).parent))
import asm_cert  # noqa: E402


def fail(msg):
    print(f"shape_gen.py: {msg}", file=sys.stderr)
    sys.exit(1)


def last_stage(stages_text):
    """The last EVM IR stage of Stages.lean: names, fuels and the composition theorem."""
    ns = re.search(r"^namespace (SolarTV\.Milestone\.\w+)", stages_text, re.M)
    st = re.findall(
        r"theorem stage_(\w+) \(o : Oracle\) : Stage \(evmirSem o\) \(evmirSem o\) (\w+) (\w+) anyCall :=\n"
        r"\s+stage_of_runs \(A := EVM\.Program\.layer\) \(B := EVM\.Program\.layer\) (\d+) (\d+) "
        r"\((\w+) o\)\s*\n\s*\(fun call _ => (\w+) o call\)",
        stages_text,
    )
    comp = re.search(r"theorem (\w+_mir_to_evmir) \(o : Oracle\) : Stage \(mirSem o\) \(evmirSem o\) (\w+) (\w+) anyCall", stages_text)
    if not (ns and st and comp):
        fail("Stages.lean: no EVM IR stage chain to the final EVM IR")
    _, prev, last, fuel0, fuel, prev_ends, last_run = st[-1]
    if last != comp.group(3):
        fail("Stages.lean: the last EVM IR stage is not the composition's target")
    return {"ns": ns.group(1), "prev": prev, "last": last, "fuel": int(fuel), "fuel0": int(fuel0),
            "prev_ends": prev_ends, "last_run": last_run, "compose": comp.group(1), "first": comp.group(2)}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("shape")
    ap.add_argument("--name", required=True)
    ap.add_argument("--certified", required=True, help="the solidity-lean Certified directory")
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    shape = json.load(open(a.shape))
    g = shape.get("generic")
    if not isinstance(g, dict):
        fail("shape.json has no generic block")
    kind = shape.get("kind")
    root = pathlib.Path(a.certified) / a.name
    gens = [p for p in (root / "gen").iterdir() if p.is_dir()]
    if len(gens) != 1:
        fail("expected one exported contract under gen/")
    gen = gens[0]
    contract = gen.name
    stages = last_stage((root / "Proofs" / "Stages.lean").read_text())
    bt = (gen / "Bytecode.lean").read_text()
    hx = re.search(r'def bytecodeHex : String :=\n  "([0-9a-f]+)"', bt)
    if hx is None:
        fail("Bytecode.lean defines no bytecodeHex")
    code = bytes.fromhex(hx.group(1))
    if re.search(r"def dataStart : Nat := (\d+)", bt):
        fail("the runtime bytecode carries data: outside the loop-free shape")
    last_module = "Evm" + stages["last"][3:] if stages["last"].startswith("evm") else None
    last_text = (gen / f"{last_module}.lean").read_text()
    outside = asm_cert.outside(last_text)
    if outside:
        fail(f"the final EVM IR is outside the loop checker's fragment: {outside}")
    inferred = asm_cert.infer(last_text, code)
    cert, maxs = asm_cert.lean_cert(inferred)
    N = stages["fuel"]
    bmax = max(inferred["costs"]) if inferred["costs"] else 0
    words = inferred["top"] // 32
    mem = 3 * words + words * words // 512
    need = 22100 * (N * bmax) + mem
    gas = shape.get("gas", "")
    m = re.fullmatch(r"fun _ => (\d+)", gas)
    if m is None:
        fail(f"shape.json: gas must be a constant bound, fun _ => K (the checker needs {need})")
    K = int(m.group(1))
    if K < need or K >= 2 ** 64:
        fail(f"shape.json: gas bound {K} is below the checker's {need} or not below 2^64")
    X = f"SolarTV.Export.{contract}"
    M = stages["ns"]
    ns = g["ns"]
    sns = g.get("source_ns", ns)
    A, L, S = ("SolidCore.Refinement.Solar.Assemble", "SolidCore.Refinement.Solar.AssembleLoop",
               "SolidCore.Refinement.Shapes")
    last = f"{X}.{stages['last']}"
    value = "check_value" in g
    valid = f"{S}.genericValid{'' if value else '0'} ({g['fns']}) ({g['storage']})"
    if shape.get("valid") != valid:
        fail(f"shape.json: valid must be {valid!r} for this generic block")
    env = shape.get("env", [])
    binders = " ".join(f"_h{i}" for i in range(len(env)))
    if g["style"] == "core":
        start = (f"  obtain ⟨hk, d, hd⟩ := {ns}.deployed_of hc\n"
                 f"  have hchk := {ns}.{g['check']}\n  rw [← hk] at hchk\n")
        if value:
            start += f"  have hval := {ns}.{g['check_value']}\n  rw [← hk] at hval\n"
        okthm = "contract_ok"
        okdef = (f"/-- SolidCore type checks the contract. -/\n"
                 f"theorem contract_ok : ∃ c, {ns}.contract? = .ok c := by\n"
                 f"  have h := {ns}.shape_eq\n  unfold {ns}.shape? at h\n  split at h\n"
                 f"  · next c hc => exact ⟨c, hc⟩\n  · simp at h\n\n")
    elif g["style"] == "checked":
        hs = " ".join(f"h{i}" for i in range(len(g["checks"])))
        start = f"  obtain ⟨d, hd⟩ := {ns}.deployed hc\n"
        for i, (_, thm) in enumerate(g["checks"]):
            start += f"  have h{i} := {ns}.check_of hc {thm}\n"
        start += (f"  have hchk : Generic.check {sns}.fuel {ns}.mirFuel c.core {X}.stage0 ({g['fns']}) .all = true := by\n"
                  f"    simp only [Generic.check, List.all_cons, List.all_nil, Bool.and_true] at {hs} ⊢\n"
                  f"    simp only [{', '.join(f'h{i}' for i in range(len(g['checks'])))}, Bool.and_self]\n")
        if value:
            fail("style checked has no value check")
        okthm, okdef = g["contract_ok"], ""
    else:
        fail("generic.style must be core or checked")
    comp = (f"{S}.compiled{'' if value else '0'} hd hchk {'hval ' if value else ''}"
            f"({S}.genericValid{'' if value else '0'}_inhabited _ _ _)\n"
            f"      ({M}.{stages['compose']} {S}.oracle) (fun _ _ => trivial) ends check_ok gas_lt")
    if kind == "compiled":
        shape_t = f"{S}.Compiled (solidCore {sns}.fuel) c code ({valid}) ({gas})"
        body = f"  exact ⟨c, hc, ({comp}).mono_gas (fun _ => gas_le) (fun _ => by decide)⟩\n"
    elif kind == "certified":
        triples = g.get("triples", [])
        if not triples:
            fail("kind certified needs generic.triples")
        meets = f"{S}.meets_of_triple ({triples[-1]})"
        for t in reversed(triples[:-1]):
            meets = f"{S}.Meets.and ({S}.meets_of_triple ({t})) ({meets})"
        shape_t = (f"{S}.Certified (solidCore {sns}.fuel) c code ({valid}) ({shape['allowed']}) "
                   f"({shape['post']}) ({gas})")
        body = (f"  exact ⟨c, hc, (({comp}).mono_gas (fun _ => gas_le) (fun _ => by decide)).certified\n"
                f"    (fun _ _ _ _ => trivial) ({meets})⟩\n")
        if shape.get("allowed") != "fun _ _ => True":
            fail("kind certified from triples: allowed must be fun _ _ => True (the triples state no revert condition)")
    else:
        fail("kind must be compiled or certified")
    # Negative controls the module proves: the loop checker rejects the bytecode with a jump
    # target moved, an SSTORE dropped (as POP) and an opcode outside the fragment (CREATE) in its
    # place, each by the kernel's evaluation, failing at the walk.
    insns = asm_cert.decode(code)
    mutants = []
    for (pc, op, _), nxt in zip(insns, insns[1:]):
        if op == 0x60 and nxt[1] in (0x56, 0x57):
            mutants.append(("jump", "a jump target moved", pc + 1, (code[pc + 1] + 1) % 256))
            break
    for pc, op, _ in insns:
        if op == 0x55:
            mutants.append(("sstore", "an SSTORE dropped (as POP)", pc, 0x50))
            mutants.append(("opcode", "an opcode outside the fragment (CREATE) for an SSTORE", pc, 0xF0))
            break
    negs = "".join(
        f"set_option maxRecDepth 100000 in\n/-- The checker rejects the bytecode with {what} (byte {i}). -/\n"
        f"theorem rejects_{tag} : {L}.checkL {last} (mutant {i} {b}) cert maxs = false := by\n"
        "  decide +kernel\n\n"
        for tag, what, i, b in mutants
    )
    premises = "".join(f"({e}) →\n    " for e in env)
    intro = f"  intro {binders}\n" if env else ""
    mods = ["Verify.ShapeGeneric", g["lowering"], f"{a.name}.Proofs.Stages", f"{a.name}.gen.{contract}.Bytecode",
            *g.get("imports", [])]
    text = "\n".join(f"import {m}" for m in dict.fromkeys(mods)) + f"""

-- Generated by solar's tools/certify/shape_gen.py from {a.name}/shape.json and this run's export;
-- do not edit. Untrusted data and composition: the kernel re-derives every claim.

/-!
# {contract}: the proof of the published statement

`published` applies `{S}.compiled{'' if value else '0'}` to the source check of `{ns}`, solar-tv's backend
stages to `{stages['last']}` (`{M}.{stages['compose']}`), the final EVM IR's run within {N} blocks on every
call (the last stage's fuel) and the loop checker on the exact bytecode, with the certificate
tools/certify/asm_cert.py inferred. The gas bound is `gasBoundL cert {N}` = 22100 · ({N} · {bmax}) + {mem}
= {need}, at most the statement's {K}.
-/

namespace Certified.{a.name}.Shape

open SolidCore.Refinement SolidCore.Refinement.Solar SolarTV

/-- The exported runtime bytecode. -/
def code : ByteArray := ⟨({X}.bytecode.map UInt8.ofNat).toArray⟩

/-- The loop checker's certificate (tools/certify/asm_cert.py). -/
def cert : {A}.Cert :=
  {cert}

/-- The highest stack each block is entered with. -/
def maxs : List Nat := {maxs}

set_option maxRecDepth 100000 in
theorem walk_ok : (SolarTV.Assembly.walk ({A}.bytesOf code) cert.offset 0
    ({A}.lower {last} cert)).isSome = true := by
  have hw := {A}.walkS_drop ({A}.bytesOf code) cert.offset ({A}.lower {last} cert) 0
    (Nat.zero_le _)
  rw [List.drop_zero] at hw
  rw [← hw]
  decide +kernel

set_option maxRecDepth 100000 in
theorem blocks_ok : {L}.blocksOkL {last} cert maxs 0 {last}.blocks = true := by
  decide +kernel

theorem labels_ok : {A}.labelsValid {last} cert code = true :=
  {A}.labelsValid_of_walk walk_ok

theorem size_ok : code.size < 2 ^ 32 := by decide +kernel

/-- The checker of code with loops accepts the exact bytecode for the final EVM IR. -/
theorem check_ok : {L}.checkL {last} code cert maxs = true := by
  simp only [{L}.checkL, walk_ok, blocks_ok, labels_ok, decide_eq_true size_ok, Bool.and_true,
    Bool.true_and]
  decide

/-- The final EVM IR ends within {N} blocks on every call, under the bytecode's oracle. -/
theorem ends (call : Call) : (EVM.Program.run {S}.oracle {last} call {N}).isSome = true :=
  SolarTV.Milestone.W1.ends_of_runs (A := EVM.Program.layer) (B := EVM.Program.layer)
    (o := {S}.oracle) (p := {X}.{stages['prev']}) (q := {last}) (n := {stages['fuel0']}) (m := {N})
    ({M}.{stages['prev_ends']} {S}.oracle) (fun call _ => {M}.{stages['last_run']} {S}.oracle call) call trivial

/-- The bytecode with byte `i` replaced by `b`. -/
def mutant (i b : Nat) : ByteArray := ⟨(({X}.bytecode.set i b).map UInt8.ofNat).toArray⟩

{negs}theorem gas_lt : {L}.gasBoundL cert {N} < 2 ^ 64 := by decide +kernel

theorem gas_le : {L}.gasBoundL cert {N} ≤ {K} := by decide +kernel

{okdef}
/-- **What the published statement asks of this module.** -/
theorem published : {premises}∃ c, {sns}.contract? = .ok c ∧
      {shape_t} := by
{intro}  obtain ⟨c, hc⟩ := {okthm}
{start}{body}
/-- info: 'Certified.{a.name}.Shape.published' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms published

end Certified.{a.name}.Shape
"""
    text = text.replace("`", chr(96))
    pathlib.Path(a.out).write_text(text)
    print(json.dumps({"proof": f"Certified.{a.name}.Shape.published", "gas_needed": need, "blocks": N}))


if __name__ == "__main__":
    main()
