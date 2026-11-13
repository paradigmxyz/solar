#!/usr/bin/env python3
"""The published statement of a certified contract: a small trusted wrapper.

`final.py SHAPE_JSON --name NAME --contract C --hex HEX --out FINAL_LEAN` writes the Lean module
NAME.Final. Its statement `Statement` is built here, only from the shape manifest
(CERTIFIED/NAME/shape.json: the source contract, the declared valid calls, allowed statuses,
specification, environment assumptions and, for the strongest status, the gas bound) and from HEX,
the runtime bytecode the driver's own solar compiled from the source in this run. Nothing in it
comes from a proof module: the proof module only supplies a term, which the kernel must accept at
exactly this type (`theorem final : Statement := ...`). The statement binds the bytecode once, by
exact equality of its bytes with the bytes of HEX, written out as numbers (the kernel's evaluation
of a hex string of a few hundred bytes outgrows 3 GB), decided by the kernel;
the code the obligations speak of is the fresh export NAME.gen.C.Bytecode, so a stale or
substituted artifact fails the binding by evaluation.

shape.json keys (Lean terms without tactics or commands):
  kind      "certified" (Shapes.Certified: equivalence, termination, spec, resources),
            "functional" (Shapes.Functional: the first three; resources not proved) or
            "compiled" (Shapes.Compiled: equivalence with the gas bound, termination with any
            status and resources, for a contract with no specification; allowed and post absent)
  contract  the checked source, `Except _ CheckedContract` (such as M.contract?)
  fuel      the source semantics' fuel (`solidCore fuel`)
  valid     `Call → Prop`: the declared valid calls
  allowed   `Call → Observable → Prop`: the allowed statuses (not for kind "compiled")
  post      `Call → Observable → Prop`: the specification (not for kind "compiled")
  gas       `Call → Nat`: the gas bound (kinds "certified" and "compiled")
  spec      what the specification is, or why there is none (recorded verbatim)
  env       list of named `Prop` assumptions, premises of the statement
  spec_modules  modules defining valid/allowed/post/env (reviewed inputs, not proofs)
  sem, prog (instead of contract and fuel) a source semantics and program below SolidCore, such as
            the first lowered MIR under an oracle; the statement then has no checked contract
  proof_module, proof  the module and the term (no tactics) of type `∀ env…, ∃ c, contract = .ok c ∧ Shape`
  excluded  what the statement does not cover, recorded verbatim
The driver then audits `final`'s axioms (only propext, Classical.choice, Quot.sound) and derives
the statuses from `kind` alone: the strongest status is reported only for "certified".
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


def fail(msg):
    print(f"final.py: {msg}", file=sys.stderr)
    sys.exit(1)


def expr(shape, key):
    v = shape.get(key)
    if not isinstance(v, str) or not EXPR.match(v) or BANNED & set(re.split(r"[\s()]+", v)):
        fail(f"shape.json: {key} must be a term without tactics or commands, not {v!r}")
    return v


def statuses(kind, spec=None):
    """What the kernel-checked statement of each kind proves, obligation by obligation."""
    res = kind in ("certified", "compiled")
    return {
        "equivalence": "proved (published statement, Shapes.Equiv): at every gas below 2^256 every "
        "result of the exact bytecode on a valid call is the source's, or a failure that changes "
        "nothing (out of gas or a revert, whatever its data)"
        + ("; such a failure only below the gas bound, and from the bound on exactly the source's, "
           "revert data included (Shapes.EquivG, "
           + ("a field of Shapes.Compiled)" if kind == "compiled" else "Shapes.Certified.equivG)")
           if res else ""),
        "termination": "proved (published statement, Shapes.Terminates): the source ends on every "
        "valid call "
        + ("with any status (no specification declares the allowed ones)" if kind == "compiled"
           else "with an allowed status")
        + ("; the bytecode too, at gas from the bound on" if res
           else "; the bytecode's ending at a proved gas is not (resources)"),
        "spec": ((spec if (spec or "").startswith("none") else "none: " + (spec or "no specification declared"))
                 if kind == "compiled" else
                 "proved (published statement, Shapes.Meets): the specification of every source "
                 "result on a valid call, hence of every bytecode result that is not an inert failure"),
        "resources": "proved (published statement, Shapes.Resources): from the gas bound on, below "
        "2^64, the bytecode gives the source's result" if res else "not proved",
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("shape")
    ap.add_argument("--name", required=True)
    ap.add_argument("--contract", required=True)
    ap.add_argument("--hex", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--record", help="write the statuses and statement here as JSON")
    a = ap.parse_args()
    shape = json.load(open(a.shape))
    kind = shape.get("kind")
    if kind not in ("certified", "functional", "compiled"):
        fail("shape.json: kind must be certified, functional or compiled")
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
        exists, close = "", ""
    else:
        contract, fuel = expr(shape, "contract"), expr(shape, "fuel")
        source = f"(solidCore ({fuel})) c"
        exists, close = f"∃ c, {contract} = .ok c ∧\n      ", ""
    valid = expr(shape, "valid")
    if kind == "compiled":
        if "allowed" in shape or "post" in shape:
            fail("shape.json: kind compiled states no specification (no allowed, no post)")
        spec_args = ""
    else:
        allowed, post = expr(shape, "allowed"), expr(shape, "post")
        spec_args = f" ({allowed}) ({post})"
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
    shape_t = {"certified": "Shapes.Certified", "functional": "Shapes.Functional",
               "compiled": "Shapes.Compiled"}[kind]
    gas = f" ({expr(shape, 'gas')})" if kind in ("certified", "compiled") else ""
    export = f"SolarTV.Export.{a.contract}"
    premises = "".join(f"({e}) →\n    " for e in env)
    binders = " ".join(f"_h{i}" for i in range(len(env)))
    term = f"{proof} {binders}".strip()
    imports = ["Verify.Shapes", *(["Verify.ShapeGeneric"] if kind == "compiled" else []),
               f"{a.name}.gen.{a.contract}.Bytecode", *spec_mods, proof_mod]
    lines = [f"import {m}" for m in dict.fromkeys(imports)]
    byte_list = ",".join(("\n   " if i % 16 == 0 and i else "") + " " + hex(b) for i, b in enumerate(bytes.fromhex(a.hex))).strip()
    text = "\n".join(lines) + f"""

-- Generated by solar's tools/certify/final.py from {a.name}/shape.json and the bytecode the
-- driver compiled in this run; do not edit. The statement is fixed by the driver; the proof
-- module only supplies a term the kernel checks against it.

/-!
# {a.contract}: the published statement

`Statement` binds the exact runtime bytecode (`runtimeHex`, from the driver's own compile) and
states {shape_t} of it against the checked source on the declared valid calls.
-/

namespace Certified.{a.name}.Final

open SolidCore.Refinement

/-- The runtime bytecode the pinned solar emitted in this run, as hex (for the reader). -/
def runtimeHex : String :=
  "{a.hex}"

/-- The same bytes, as numbers: what the statement binds. -/
def runtimeBytes : List Nat :=
  [{byte_list}]

/-- The freshly exported bytecode the obligations speak of. -/
def code : ByteArray := ⟨({export}.bytecode.map UInt8.ofNat).toArray⟩

/-- **The published statement.** -/
def Statement : Prop :=
  code.data.toList.map (·.toNat) = runtimeBytes ∧
    ({premises}{exists}{shape_t} {source} code
        ({valid}){spec_args}{gas})

set_option maxRecDepth 100000 in
theorem bound : code.data.toList.map (·.toNat) = runtimeBytes :=
  @of_decide_eq_true _ (instDecidableEqList _ _) (by decide +kernel)

theorem final : Statement :=
  ⟨bound, {"fun " + binders + " => " if env else ""}{term}⟩

#print axioms final

end Certified.{a.name}.Final
"""
    open(a.out, "w").write(text)
    stmt = " ".join(text.split("def Statement : Prop :=", 1)[1].split("set_option maxRecDepth 100000 in\ntheorem bound")[0].split())
    if a.record:
        json.dump(
            {
                "module": f"{a.name}.Final",
                "theorem": f"Certified.{a.name}.Final.final",
                "kind": kind,
                "statement": stmt,
                "assumptions": env,
                "excluded": shape.get("excluded", []),
                "spec": shape.get("spec"),
                "statuses": statuses(kind, shape.get("spec")),
            },
            open(a.record, "w"),
            indent=2,
            ensure_ascii=False,
        )
    print(f"Certified.{a.name}.Final.final")


if __name__ == "__main__":
    main()
