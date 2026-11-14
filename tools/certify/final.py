#!/usr/bin/env python3
"""The published statement of a certified contract: a small trusted wrapper.

`final.py SHAPE_JSON --name NAME --contract C --hex HEX --out FINAL_LEAN --statement-out STATEMENT_LEAN`
writes two Lean modules:

* NAME.Statement (STATEMENT_LEAN): the statement `Certified.NAME.Final.Statement`, built here only
  from the shape manifest (CERTIFIED/NAME/shape.json) and from HEX, the runtime bytecode the
  driver's own solar compiled from the source in this run, with the binding `bound` of the fresh
  export to HEX. It imports the shapes, the fresh exports and the reviewed spec modules, never the
  proof module, so nothing the proof module declares (a name, an alias, a notation, a macro) can
  change how the statement elaborates.
* NAME.Final (FINAL_LEAN): imports NAME.Statement and the proof module and states
  `theorem final : Certified.NAME.Final.Statement` with the proof module's term. The driver's checker
  (tools/certify/check_final.lean) then reads the compiled environment and requires `final`'s type
  to be exactly the constant Certified.NAME.Final.Statement of module NAME.Statement, NAME.Statement
  not to import the proof module, and `final`'s axioms to be among propext, Classical.choice and
  Quot.sound.

The statement binds the bytecode once, by exact equality of its bytes with the bytes of HEX,
written out as numbers and decided by the kernel; the code the obligations speak of is the fresh
export NAME.gen.C.Bytecode, so a stale or substituted artifact fails the binding.

shape.json keys (Lean terms without tactics or commands):
  kind      "certified" (Shapes.Certified: equivalence, termination, spec, resources),
            "functional" (Shapes.Functional: the first three),
            "open_functional" (Shapes.OpenFunctional, Verify/ShapeOpen.lean: the first three with
            the bytecode run among any admitted other accounts and the source in every admitted
            open world, for contracts that call others; needs env_ok, wide and admit, and its
            allowed and post read the world) or
            "compiled" (Shapes.Compiled: equivalence with the gas bound, termination with any
            status and resources, for a contract with no specification; allowed and post absent)
  contract  the checked source, `Except _ CheckedContract` (such as M.contract?)
  fuel      the source semantics' fuel (`solidCore fuel`)
  valid     `Call → Prop`: the declared valid calls
  allowed   `Call → Observable → Prop`: the allowed statuses (not for kind "compiled")
  spec_export  {"ctx": `Call → SolidCore.Spec.Ctx`, "step": `SolidCore.Spec.GhostStep`}: the
            specification is solar-verify's export of the source in this run (the driver writes it
            as NAME.gen.C.Spec, bundle Certified.NAME.gen.C.Spec.C), read as a postcondition by
            Shapes.bundlePost; "post" is then absent
  post      `Call → Observable → Prop`: a Lean specification instead, not bound to solar-verify's
            export (never the strongest status)
  nonvacuous  a term proving NonVacuous: a valid call with a result the specification rejects, and
            a valid call with a revert the allowed statuses reject; required for kinds certified
            and functional unless "vacuity" says why it is missing (then never the strongest status)
  vacuity   why non-vacuity is not shown (recorded verbatim)
  gas       `Call → Nat`: the gas bound (kinds "certified" and "compiled")
  spec      what the specification is, or why there is none (recorded verbatim)
  env       list of named `Prop` assumptions, premises of the statement
  spec_modules  modules defining valid/allowed/post/env (reviewed inputs, not proofs)
  env_ok    (open_functional) `TargetEnv → Prop`: the admitted accounts beside the contract
  wide      (open_functional) `World → Prop`: the worlds a result may be explained by
  admit     (open_functional) `World → Prop`: the worlds termination and the spec speak of;
            a result only a wide world explains is reported as Shapes.Unadmitted
  allowed_vacuity  (open_functional) why the allowed statuses reject nothing (then NonVacuous
            states the specification's half only)
  sem, prog (instead of contract and fuel) a source semantics and program below SolidCore, such as
            the first lowered MIR under an oracle; the statement then has no checked contract
  proof_module, proof  the module and the term of type `∀ env…, ∃ c, contract = .ok c ∧ Shape`
  excluded  what the statement does not cover, recorded verbatim
The strongest status is reported only for kind "certified" with the specification from
solar-verify's export (spec_export) and the non-vacuity witnesses (nonvacuous) in the statement.
"""

import argparse
import json
import re
import sys

NAME = re.compile(r"^[A-Za-z_][A-Za-z0-9_.?'!]*$")
# A statement input: a term without tactics, commands or type ascriptions (no newline, colon,
# semicolon, backtick or tactic keyword); its meaning is checked by the kernel like the rest.
EXPR = re.compile(r"^[^\n:;\`]+$")
BANNED = {"sorry", "by", "decide", "native_decide", "admit", "axiom", "#eval", "#exit"}
# An allowed status or specification that is true of everything, written out: refused at once
# (NonVacuous refuses the rest in the kernel).
TRIVIAL = re.compile(r"^[(\s]*fun\s+[\w\s]*=>[(\s]*True[)\s]*$")


def fail(msg):
    print(f"final.py: {msg}", file=sys.stderr)
    sys.exit(1)


def expr(shape, key):
    v = shape.get(key)
    if not isinstance(v, str) or not EXPR.match(v) or BANNED & set(re.split(r"[\s()]+", v)):
        fail(f"shape.json: {key} must be a term without tactics or commands, not {v!r}")
    return v


def statuses(kind, spec=None, bound=False, vacuity=None):
    """What the kernel-checked statement of each kind proves, obligation by obligation."""
    if kind == "open_functional":
        st = statuses("functional", spec, bound, vacuity)
        st["equivalence"] = ("proved (published statement, Shapes.OpenFunctional): in every admitted "
            "environment of other accounts, at every gas below 2^256, every result of the exact "
            "bytecode on a valid call is the source's in some admitted open world, or a failure that "
            "changes nothing, or a result only a world outside the admitted ones explains "
            "(Shapes.Unadmitted, reported, outside the specification's guarantee)")
        st["termination"] = st["termination"].replace("the source ends on every valid call",
            "the source ends on every valid call in every admitted world")
        st["spec"] = st["spec"].replace("of every source result on a valid call",
            "of every source result on a valid call in every admitted world")
        return st
    res = kind in ("certified", "compiled")
    weak = "" if vacuity is None else f" (not shown non-vacuous: {vacuity})"
    if kind == "compiled":
        spec_s = spec if (spec or "").startswith("none") else "none: " + (spec or "no specification declared")
    elif not bound:
        spec_s = ("partial: proved of a Lean specification (shape.json post), not bound to "
                  "solar-verify's export of the source" + weak)
    elif vacuity is not None:
        spec_s = "partial: proved of solar-verify's exported specification" + weak
    else:
        spec_s = ("proved (published statement, Shapes.Meets of Shapes.bundlePost of the "
                  "specification solar-verify exported in this run, NonVacuous in the statement): "
                  "the specification of every source result on a valid call, hence of every "
                  "bytecode result that is not an inert failure")
    term = ("proved (published statement, Shapes.Terminates): the source ends on every valid call "
            + ("with any status (no specification declares the allowed ones)" if kind == "compiled"
               else "with an allowed status")
            + ("; the bytecode too, at gas from the bound on" if res
               else "; the bytecode's ending at a proved gas is not (resources)"))
    if kind != "compiled" and vacuity is not None:
        term = "partial: " + term + weak
    return {
        "equivalence": "proved (published statement, Shapes.Equiv): at every gas below 2^256 every "
        "result of the exact bytecode on a valid call is the source's, or a failure that changes "
        "nothing (out of gas or a revert, whatever its data)"
        + ("; such a failure only below the gas bound, and from the bound on exactly the source's, "
           "revert data included (Shapes.EquivG, "
           + ("a field of Shapes.Compiled)" if kind == "compiled" else "Shapes.Certified.equivG)")
           if res else ""),
        "termination": term,
        "spec": spec_s,
        "resources": "proved (published statement, Shapes.Resources): from the gas bound on, below "
        "2^64, the bytecode gives the source's result" if res else "not proved",
        "vacuity": ("not applicable (no specification)" if kind == "compiled" else
                    "proved non-vacuous (NonVacuous in the statement)" if vacuity is None else
                    "not shown: " + vacuity),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("shape")
    ap.add_argument("--name", required=True)
    ap.add_argument("--contract", required=True)
    ap.add_argument("--hex", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--statement-out", required=True)
    ap.add_argument("--record", help="write the statuses and statement here as JSON")
    a = ap.parse_args()
    shape = json.load(open(a.shape))
    kind = shape.get("kind")
    if kind not in ("certified", "functional", "compiled", "open_functional"):
        fail("shape.json: kind must be certified, functional, compiled or open_functional")
    opened = kind == "open_functional"
    if opened and "sem" in shape:
        fail("shape.json: kind open_functional states a checked source (contract and fuel)")
    if not opened:
        for k in ("env_ok", "wide", "admit", "allowed_vacuity"):
            if k in shape:
                fail(f"shape.json: {k} is for kind open_functional only")
    allowed_vac = None
    if not re.fullmatch(r"[0-9a-f]+", a.hex) or len(a.hex) % 2:
        fail("--hex is not the lowercase hex of the bytecode")
    for k in ("name", "contract"):
        if not NAME.match(getattr(a, k)):
            fail(f"--{k} is not a Lean name")
    if "sem" in shape:
        # A source below SolidCore (the first lowered MIR, for code no SolidCore stage reaches):
        # the statement names its semantics and program; the frontend trust boundary moves there.
        if "contract" in shape or "fuel" in shape:
            fail("shape.json: sem/prog and contract/fuel exclude each other")
        source = f"({expr(shape, 'sem')}) ({expr(shape, 'prog')})"
        exists = ""
    else:
        contract, fuel = expr(shape, "contract"), expr(shape, "fuel")
        source = f"({fuel}) c" if opened else f"(_root_.SolidCore.Refinement.solidCore ({fuel})) c"
        exists = f"∃ c, {contract} = .ok c ∧\n      "
    valid = expr(shape, "valid")
    ns = f"Certified.{a.name}.Final"
    export_ns = f"Certified.{a.name}.gen.{a.contract}.Spec"
    bound_spec, vacuity, nonvac = False, None, None
    allowed = post = None
    if kind == "compiled":
        for k in ("allowed", "post", "spec_export", "nonvacuous", "vacuity"):
            if k in shape:
                fail(f"shape.json: kind compiled states no specification (no {k})")
        spec_args = ""
    else:
        allowed = expr(shape, "allowed")
        if "spec_export" in shape:
            if "post" in shape:
                fail("shape.json: spec_export and post exclude each other")
            se = shape["spec_export"]
            if not isinstance(se, dict) or set(se) != {"ctx", "step"}:
                fail('shape.json: spec_export must be {"ctx": ..., "step": ...}')
            post = (f"_root_.SolidCore.Refinement.Shapes.bundlePost _root_.{export_ns}.{a.contract} "
                    f"({expr(se, 'ctx')}) ({expr(se, 'step')})")
            if opened:
                post = f"fun _ => {post}"
            bound_spec = True
        else:
            post = expr(shape, "post")
        if opened and isinstance(shape.get("allowed_vacuity"), str) and shape["allowed_vacuity"].strip():
            allowed_vac = shape["allowed_vacuity"].strip()
        for k, v in (("allowed", allowed), ("post", post)):
            if k == "allowed" and allowed_vac is not None:
                continue
            if TRIVIAL.match(v) and not (isinstance(shape.get("vacuity"), str) and shape["vacuity"].strip()):
                fail(f"shape.json: {k} is trivially true ({v!r}): it must reject something, or "
                     "vacuity must say why it does not")
        if "nonvacuous" in shape:
            if "vacuity" in shape:
                fail("shape.json: nonvacuous and vacuity exclude each other")
            nonvac = expr(shape, "nonvacuous")
        elif isinstance(shape.get("vacuity"), str) and shape["vacuity"].strip():
            vacuity = shape["vacuity"].strip()
        else:
            fail(f"shape.json: kind {kind} needs nonvacuous (a proof that the specification and the "
                 "allowed statuses reject something) or vacuity (why that is not shown)")
        spec_args = f" ({allowed}) ({post})"
        if opened:
            spec_args = (f" ({expr(shape, 'env_ok')}) ({expr(shape, 'wide')}) ({expr(shape, 'admit')})"
                         f" ({valid}) ({allowed}) ({post})")
    env = shape.get("env", [])
    if not isinstance(env, list):
        fail("shape.json: env must be a list")
    env = [expr({"e": e}, "e") for e in env]
    spec_mods = shape.get("spec_modules", [])
    proof_mod, proof = shape.get("proof_module"), expr(shape, "proof")
    for m in spec_mods + [proof_mod]:
        if not isinstance(m, str) or not NAME.match(m):
            fail(f"shape.json: {m!r} is not a Lean name")
    if proof_mod in spec_mods:
        fail("shape.json: the proof module cannot define the statement's inputs")
    if proof_mod in (f"{a.name}.Statement", f"{a.name}.Final"):
        fail("shape.json: the proof module cannot be the statement or the final module")
    shape_t = {"certified": "_root_.SolidCore.Refinement.Shapes.Certified",
               "functional": "_root_.SolidCore.Refinement.Shapes.Functional",
               "compiled": "_root_.SolidCore.Refinement.Shapes.Compiled",
               "open_functional": "_root_.SolidCore.Refinement.Shapes.OpenFunctional"}[kind]
    gas = f" ({expr(shape, 'gas')})" if kind in ("certified", "compiled") else ""
    export = f"_root_.SolarTV.Export.{a.contract}"
    premises = "".join(f"({e}) →\n    " for e in env)
    binders = " ".join(f"_h{i}" for i in range(len(env)))
    term = f"{proof} {binders}".strip()
    imports = ["Verify.Shapes", *(["Verify.ShapeGeneric"] if kind == "compiled" else []),
               *(["Verify.ShapeOpen"] if opened else []),
               *(["Verify.ShapeBundle", f"{a.name}.gen.{a.contract}.Spec"] if bound_spec else []),
               f"{a.name}.gen.{a.contract}.Bytecode", *spec_mods]
    lines = [f"import {m}" for m in dict.fromkeys(imports)]
    byte_list = ",".join(("\n   " if i % 16 == 0 and i else "") + " " + hex(b) for i, b in enumerate(bytes.fromhex(a.hex))).strip()
    call_t, obs_t = "_root_.SolarTV.Call", "_root_.SolarTV.Observable"
    nonvac_def = nonvac_conj = ""
    if nonvac is not None and opened:
        allowed_half = "" if allowed_vac is not None else f""" ∧
    (∃ (call : {call_t}) (obs : {obs_t}), ({valid}) call ∧ obs.success = false ∧
      ∀ (W : _root_.SolidCore.Refinement.World), ({expr(shape, 'admit')}) W → ¬ ({allowed}) W call obs)"""
        nonvac_def = f"""
/-- The specification is not vacuous: some valid call has a result it rejects in every world
{"(the allowed statuses reject nothing: " + allowed_vac + ")" if allowed_vac is not None else "and some valid call has a revert no admitted world allows"}. -/
def NonVacuous : Prop :=
  (∃ (call : {call_t}) (obs : {obs_t}), ({valid}) call ∧ ∀ (W : _root_.SolidCore.Refinement.World), ¬ ({post}) W call obs){allowed_half}
"""
        nonvac_conj = f"_root_.{ns}.NonVacuous ∧\n    "
    elif nonvac is not None:
        nonvac_def = f"""
/-- The specification and the allowed statuses are not vacuous: some valid call has a result the
specification rejects, and some valid call has a revert the allowed statuses reject. -/
def NonVacuous : Prop :=
  (∃ (call : {call_t}) (obs : {obs_t}), ({valid}) call ∧ ¬ ({post}) call obs) ∧
    (∃ (call : {call_t}) (obs : {obs_t}), ({valid}) call ∧ obs.success = false ∧
      ¬ ({allowed}) call obs)
"""
        nonvac_conj = f"_root_.{ns}.NonVacuous ∧\n    "
    stmt_text = "\n".join(lines) + f"""

-- Generated by solar's tools/certify/final.py from {a.name}/shape.json and the bytecode the
-- driver compiled in this run; do not edit. This module never imports the proof module.

/-!
# {a.contract}: the published statement

`Statement` binds the exact runtime bytecode (`runtimeHex`, from the driver's own compile) and
states {shape_t} of it against the checked source on the declared valid calls.
-/

namespace {ns}

/-- The runtime bytecode the pinned solar emitted in this run, as hex (for the reader). -/
def runtimeHex : String :=
  "{a.hex}"

/-- The same bytes, as numbers: what the statement binds. -/
def runtimeBytes : List Nat :=
  [{byte_list}]

/-- The freshly exported bytecode the obligations speak of. -/
def code : ByteArray := ⟨({export}.bytecode.map UInt8.ofNat).toArray⟩
{nonvac_def}
/-- **The published statement.** -/
def Statement : Prop :=
  _root_.{ns}.code.data.toList.map (·.toNat) = _root_.{ns}.runtimeBytes ∧
    {nonvac_conj}({premises}{exists}{shape_t} {source} _root_.{ns}.code
        {"" if opened else "(" + valid + ")"}{spec_args}{gas})

set_option maxRecDepth 100000 in
theorem bound : _root_.{ns}.code.data.toList.map (·.toNat) = _root_.{ns}.runtimeBytes :=
  @of_decide_eq_true _ (instDecidableEqList _ _) (by decide +kernel)

end {ns}
"""
    parts = [f"_root_.{ns}.bound"] + ([nonvac] if nonvac is not None else []) + \
        [("fun " + binders + " => " if env else "") + term]
    final_text = f"""import {a.name}.Statement
import {proof_mod}

-- Generated by solar's tools/certify/final.py; do not edit. The statement is
-- Certified.{a.name}.Final.Statement of {a.name}.Statement, which does not import the proof
-- module; the driver's checker requires `final`'s type to be exactly that constant.

theorem {ns}.final : _root_.{ns}.Statement :=
  ⟨{", ".join(parts)}⟩

#print axioms {ns}.final
"""
    open(a.statement_out, "w").write(stmt_text)
    open(a.out, "w").write(final_text)
    stmt = " ".join(stmt_text.split("def Statement : Prop :=", 1)[1].split("set_option maxRecDepth 100000 in\ntheorem bound")[0].split())
    strongest = kind == "certified" and bound_spec and vacuity is None
    st_rec = statuses(kind, shape.get("spec"), bound_spec, vacuity)
    if allowed_vac is not None:
        st_rec["termination"] += f" (the allowed statuses reject nothing: {allowed_vac})"
        st_rec["vacuity"] += f"; the allowed statuses: none rejected ({allowed_vac})"
    if a.record:
        json.dump(
            {
                "module": f"{a.name}.Final",
                "statement_module": f"{a.name}.Statement",
                "theorem": f"{ns}.final",
                "kind": kind,
                "statement": stmt,
                "nonvacuous": (f"{ns}.NonVacuous" if nonvac is not None else None),
                "vacuity": vacuity,
                "allowed_vacuity": allowed_vac,
                "spec_bound_to_export": (f"{export_ns}.{a.contract}" if bound_spec else None),
                "assumptions": env,
                "excluded": shape.get("excluded", []),
                "spec": shape.get("spec"),
                "statuses": st_rec,
                "strongest": strongest,
            },
            open(a.record, "w"),
            indent=2,
            ensure_ascii=False,
        )
    print(f"{ns}.final")


if __name__ == "__main__":
    main()
