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
  allowed_export  true (with spec_export, instead of allowed): the allowed statuses are read off the
            same export, Shapes.bundleAllowed (success, or a call from which no function spec's
            succeeds-if conditions require success)
  source_sem  the source semantics of the checked contract, a function of the fuel (default
            SolidCore.Refinement.solidCore; SolidCore.Refinement.solidCoreRuntime runs SolidCore's
            dispatcher on the runtime code from the call's storage)
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
  allowed_vacuity  (kinds open_functional, certified and functional, with nonvacuous) why the
            allowed statuses reject nothing, such as a spec without succeeds-if conditions (then
            NonVacuous states the specification's half only, and the status is never the strongest)
  sem, prog (instead of contract and fuel) a source semantics and program below SolidCore, such as
            the first lowered MIR under an oracle; the statement then has no checked contract
  proof_module, proof  the module and the term of type `∀ env…, ∃ c, contract = .ok c ∧ Shape`
  excluded  what the statement does not cover, recorded verbatim
  pinned_bytes, pinned_module  (kinds certified, functional and compiled) the proof speaks of the
            code `⟨(PINNED.map UInt8.ofNat).toArray⟩` of a byte list (`List Nat`) that the data
            module pinned_module defines (such as a stage export the proof was built on), not of
            this run's export. The statement then binds that list to the run's bytes,
            `runtime_is_fresh : PINNED = runtimeBytes`, decided by the kernel, and states the shape of
            `code`, the run's bytes themselves; the proof module's term is carried over by
            `code_eq`. A pinned artifact that is stale or substituted (any byte other than this run's)
            fails the binding.
  ctor_args  [{"var": "A0", "lt": "2 ^ W"}, ...] (kind compiled, on a checked source): the statement
            holds for every constructor argument list: `∀ (A0 A1 … : Nat), A0 < 2 ^ W0 → … →`, the
            source semantics is `solidCoreFromArgs callerAddress fuel [A0, A1, …]` (the contract
            deployed by SolidCore's constructor on those words, then called) and valid may name the
            words; W is a width 8..256 (a multiple of 8; 160 for an address). The proof is a term of
            type `∀ (A0 … : Nat), A0 < 2 ^ W0 → … → ∃ c, contract = .ok c ∧ Shape`
  ctor_returns  {"witness": [w0, w1, …]} (with ctor_args): the constructor may revert for some
            arguments, so the claim has the premise `Generic.CtorReturns fuel contract [A0, …]` (the
            checked contract's constructor returns on those words) after the bounds, and the statement
            adds `∧ Generic.CtorReturns fuel contract [w0, …]` (literal words): one argument list on which
            it returns, so the premise is not vacuous. The proof is then a term of the claim and that
            conjunct together (`claim ∧ CtorReturns fuel contract [w0, …]`)
  iteration_bound  {"var": V, "max": N} (kinds certified, functional and compiled): the statement
            holds for every iteration bound `V ≤ N` (N a literal), the terms valid, allowed, post and
            gas may name V, and the proof is a term of type
            `∀ {V : Nat}, V ≤ N → ∀ env…, ∃ c, contract = .ok c ∧ Shape`. With nonvacuous,
            NonVacuous takes V too, the statement is `∀ V, V ≤ N → NonVacuous V ∧ …` (non-vacuity
            at every bound, the bound 0 included), and nonvacuous is a term of type
            `∀ {V : Nat}, V ≤ N → NonVacuous V`
  parts     (kinds certified, functional and compiled) a list of claims, one per function of a
            contract with several loop functions: each part names its function (an identifier) and
            carries the claim keys valid, allowed or allowed_export, post or spec_export, nonvacuous
            or vacuity, allowed_vacuity, gas, iteration_bound, proof, spec and excluded, which the
            top level then lacks (it keeps kind, contract, fuel, source_sem, spec_modules,
            proof_module, spec and excluded). The statement binds the bytes once and is the
            conjunction, in order, of the claim the single mode states for each part, each with its
            own iteration bound and its own NonVacuous_FUNCTION; pinned_bytes, sem/prog, an open
            world and env are refused with parts. The record adds parts (each part's statuses) and is
            strongest only when every part is.
  statement "expected" with parts (kind certified, the loop route; loop_expected_mode): the
            driver-fixed statement, written from data only: the top level's expected {toolchain,
            inventory, inventory_module} and each part's call {selector, arity, limit,
            per_iteration_gas, exit_gas, M} with no_spec (a part without the export's spec) instead
            of the Lean terms valid, allowed, post and gas. It binds in its type the fresh source
            (--source, hashed here) and the export's inventory, the stage manifest's source,
            compiler, options and the digest of the first lowered MIR and every stage (each file
            under --gen hashed here), the bytes, and per part the inhabited valid calls,
            Shapes.Certified and the well-typed NonVacuous, or the checked absence of a spec; it
            imports no generated proof-side module, and Final.lean imports the inventory module.
The strongest status is reported only for kind "certified" with the specification from
solar-verify's export (spec_export) and the non-vacuity witnesses (nonvacuous) in the statement.
"""

import argparse
import json
import re
import sys

sys.path.insert(0, str(__import__("pathlib").Path(__file__).resolve().parent))
import succeeds_witness

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
    if (
        not isinstance(v, str)
        or not EXPR.match(v)
        or BANNED & set(re.split(r"[\s()]+", v))
    ):
        fail(f"shape.json: {key} must be a term without tactics or commands, not {v!r}")
    return v


def statuses(kind, spec=None, bound=False, vacuity=None):
    """What the kernel-checked statement of each kind proves, obligation by obligation."""
    if kind == "open_functional":
        st = statuses("functional", spec, bound, vacuity)
        st["equivalence"] = (
            "proved (published statement, Shapes.OpenFunctional): in every admitted "
            "environment of other accounts, at every gas below 2^256, every result of the exact "
            "bytecode on a valid call is the source's in some admitted open world, or a failure that "
            "changes nothing, or a result only a world outside the admitted ones explains "
            "(Shapes.Unadmitted, reported, outside the specification's guarantee)"
        )
        st["termination"] = st["termination"].replace(
            "the source ends on every valid call",
            "the source ends on every valid call in every admitted world",
        )
        st["spec"] = st["spec"].replace(
            "of every source result on a valid call",
            "of every source result on a valid call in every admitted world",
        )
        return st
    res = kind in ("certified", "compiled")
    weak = "" if vacuity is None else f" (not shown non-vacuous: {vacuity})"
    if kind == "compiled":
        spec_s = (
            spec
            if (spec or "").startswith("none")
            else "none: " + (spec or "no specification declared")
        )
    elif not bound:
        spec_s = (
            "partial: proved of a Lean specification (shape.json post), not bound to "
            "solar-verify's export of the source" + weak
        )
    elif vacuity is not None:
        spec_s = "partial: proved of solar-verify's exported specification" + weak
    else:
        spec_s = (
            "proved (published statement, Shapes.Meets of Shapes.bundlePost of the "
            "specification solar-verify exported in this run, NonVacuous in the statement): "
            "the specification of every source result on a valid call, hence of every "
            "bytecode result that is not an inert failure"
        )
    term = (
        "proved (published statement, Shapes.Terminates): the source ends on every valid call "
        + (
            "with any status (no specification declares the allowed ones)"
            if kind == "compiled"
            else "with an allowed status"
        )
        + (
            "; the bytecode too, at gas from the bound on"
            if res
            else "; the bytecode's ending at a proved gas is not (resources)"
        )
    )
    if kind != "compiled" and vacuity is not None:
        term = "partial: " + term + weak
    return {
        "equivalence": "proved (published statement, Shapes.Equiv): at every gas below 2^256 every "
        "result of the exact bytecode on a valid call is the source's, or a failure that changes "
        "nothing (out of gas or a revert, whatever its data)"
        + (
            "; such a failure only below the gas bound, and from the bound on exactly the source's, "
            "revert data included (Shapes.EquivG, "
            + (
                "a field of Shapes.Compiled)"
                if kind == "compiled"
                else "Shapes.Certified.equivG)"
            )
            if res
            else ""
        ),
        "termination": term,
        "spec": spec_s,
        "resources": "proved (published statement, Shapes.Resources): from the gas bound on, below "
        "2^64, the bytecode gives the source's result"
        if res
        else "not proved",
        "vacuity": (
            "not applicable (no specification)"
            if kind == "compiled"
            else "proved non-vacuous (NonVacuous in the statement)"
            if vacuity is None
            else "not shown: " + vacuity
        ),
    }


def caller_route(shape):
    """The callers the published statement is about. shape.json's generic.caller is "0x2000" (or
    absent: the fixed frame, Context.default) or "any" (the driver's check for every caller,
    tools/certify/generic.py's caller_route, accepted every function: Lowering.lean proves
    check_sym_stage0 and checkValueSym_stage0). For "any" the statement adds the equivalence in
    every real context from the gas bound any_caller.gas on (a `Call → Nat` term: the bound of the
    checker with contexts and gas, AssembleCtxGas.checkXL, which may exceed the loop checker's), and
    the proof is any_caller.proof, of module any_caller.module: a term of type

        ∀ ctx : Generic.Context, ctx.Ok → ∃ c, CONTRACT = .ok c ∧ ∀ call, VALID call →
          ∀ gas, GAS call ≤ gas → Generic.runAt ctx gas CODE call =
            (solidCoreFrom ctx.caller FUEL).run c call

    (CODE the fresh export's bytes, GAS any_caller.gas). Nothing here is trusted: the kernel checks the proof at the
    statement's type. Every other combination fails closed, so a fixed-frame statement never
    claims a caller it was not checked for, and a statement never omits a claim its shape names."""
    gen = shape.get("generic")
    caller = gen.get("caller", "0x2000") if isinstance(gen, dict) else "0x2000"
    anyc = shape.get("any_caller")
    if caller == "0x2000":
        if anyc is not None:
            fail(
                "shape.json: any_caller is for generic.caller any; the fixed route claims one caller"
            )
        return None
    if caller != "any":
        fail(f"shape.json: generic.caller must be 0x2000 or any, not {caller!r}")
    if not (
        gen.get("check_sym") == "check_sym_stage0"
        and gen.get("check_value_sym") == "checkValueSym_stage0"
    ):
        fail(
            "shape.json: generic.caller any needs the check for every caller (check_sym, check_value_sym)"
        )
    if not (isinstance(anyc, dict) and set(anyc) == {"module", "proof", "gas"}):
        fail("shape.json: generic.caller any needs any_caller {module, proof, gas}")
    mod = anyc["module"]
    if not isinstance(mod, str) or not NAME.match(mod):
        fail("shape.json: any_caller.module must be a module name")
    return mod, expr(anyc, "proof"), expr(anyc, "gas")


def expected_mode(
    a,
    shape,
    ns,
    valid,
    allowed,
    post,
    nonvac,
    env,
    spec_mods,
    proof_mod,
    proof,
    byte_list,
    export,
):
    """The driver-fixed statement: solidity-lean's Generic.Expected src spec runtimeBytes (Verify/Generic/
    Expected.lean), the same for every contract, with the binding of the fresh export to the bytes.
    Only these inputs come from shape.json: the source unit and contract (src), the valid calls,
    allowed statuses, postcondition and gas bound (spec, all in the default context), the entries
    (one per external function, each its own selector's calls) and the coverage witnesses. The proof
    module still supplies a Shapes.Certified term; Final.lean carries it into Expected through
    Generic.certifiedAt_default, and proves the coverage from the non-vacuity witnesses."""
    if env:
        fail(
            "shape.json: statement expected with assumptions (env) is not supported yet"
        )
    if nonvac is None:
        fail(
            "shape.json: statement expected needs nonvacuous (Spec.Covered's rejections)"
        )
    unit, sname = expr(shape, "source_unit"), shape.get("source_contract")
    if not isinstance(sname, str) or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", sname):
        fail("shape.json: source_contract must be the contract's name")
    # The source unit may hold other contracts, interfaces and libraries: src names the one the
    # statement is about, and it must be the contract whose fresh export (code, runtimeBytes) the
    # statement binds, never a sibling.
    if sname != a.contract:
        fail(
            f"shape.json: source_contract {sname} is not --contract {a.contract}, whose export the "
            "statement binds"
        )
    entries = shape.get("entries")
    if not isinstance(entries, list) or not entries:
        fail("shape.json: entries must list the external functions")
    storage = expr(shape, "storage")
    route = caller_route(shape)
    ent_terms, witnesses = [], []
    for e in entries:
        if not (isinstance(e, dict) and set(e) == {"signature", "fn", "sel", "arity"}):
            fail("shape.json: each entry is {signature, fn, sel, arity}")
        if (
            not re.fullmatch(
                r"[A-Za-z_][A-Za-z0-9_]*\([a-z0-9,\[\]]*\)", e["signature"]
            )
            or not isinstance(e["sel"], int)
            or not isinstance(e["arity"], int)
        ):
            fail(f"shape.json: entry {e!r}")
        fn = expr(e, "fn")
        ent_terms.append(
            f"({json.dumps(e['signature'])}, fun _ call => _root_.SolidCore.Refinement.Shapes.genericValid [{fn}] ({storage}) call)"
        )
        witnesses.append((e["sel"], e["arity"]))
    G = "_root_.SolidCore.Verify.Generic"
    sw_doc = (
        ""
        if "succeeds_witness" not in shape
        else " (the allowed statuses and the\npostcondition each with the driver's witness clause: the call \u0060"
        + shape["succeeds_witness"]["function"]
        + "\u0060 on zero words with no value, from the\nconstructor's slots written with 0, from a pre-state meeting a function spec's precondition, succeeds)"
    )
    # The caller route "any": the same bytes and source in every real context (generic.caller any).
    # The fixed route adds nothing, so its text is the fixed-frame statement byte for byte.
    any_doc = any_stmt = any_import = any_proof = ""
    if route is not None:
        any_mod, any_term, any_gas = route
        any_doc = (
            " With the check for every caller, also: in every real context\n"
            "(a caller below §2 ^ 160§ other than the contract), every valid call and every gas from the\n"
            "bound of the checker with contexts on, the exact bytes run from that caller are the source\n"
            "run from it."
        )
        any_stmt = (
            f" ∧\n    ∀ ctx : {G}.Context, ctx.Ok → ∃ c, _root_.{ns}.src.checked = .ok c ∧\n"
            f"      ∀ call, _root_.{ns}.spec.Valid ctx call → ∀ gas, ({any_gas}) call ≤ gas →\n"
            f"        {G}.runAt ctx gas ({G}.runtimeCode _root_.{ns}.runtimeBytes) call =\n"
            f"          (_root_.{ns}.src.semAt ctx).run c call"
        )
        any_import = f"import {any_mod}\n"
        any_proof = f""",
    fun ctx hctx => by
      obtain ⟨c, hc, h⟩ := ({any_term}) ctx hctx
      refine ⟨c, hc, fun call hv gas hg => ?_⟩
      rw [{G}.runtimeCode_eq _root_.{ns}.bound]
      exact h call hv gas hg"""
    imports = [
        "Verify.Generic.Expected",
        "Verify.ShapeBundle",
        "Verify.ShapeSpecExport",
        f"{a.name}.gen.{a.contract}.Spec",
        f"{a.name}.gen.{a.contract}.Bytecode",
        *spec_mods,
    ]
    lines = [f"import {m}" for m in dict.fromkeys(imports)]
    stmt_text = (
        "\n".join(lines)
        + f"""

-- Generated by solar's tools/certify/final.py from {a.name}/shape.json and the bytecode the
-- driver compiled in this run; do not edit. This module never imports the proof module.

/-!
# {a.contract}: the published statement

§Statement§ is the driver-fixed §Generic.Expected src spec runtimeBytes§, the same theorem for every
contract, with the binding of the fresh export to the bytes the driver's solar emitted.
-/

namespace {ns}

/-- The runtime bytecode the pinned solar emitted in this run, as hex (for the reader). -/
def runtimeHex : String :=
  "{a.hex}"

/-- The same bytes, as numbers: what the statement binds. -/
def runtimeBytes : List Nat :=
  [{byte_list}]

/-- The freshly exported bytecode. -/
def code : ByteArray := ⟨({export}.bytecode.map UInt8.ofNat).toArray⟩

/-- The source: solc's AST of the source unit, the contract and SolidCore's fuel. -/
def src : {G}.Source := ⟨{unit}, {json.dumps(sname)}, {expr(shape, "fuel")}⟩

/-- The specification: the default context; one entry per external function; the valid calls,
allowed statuses and postcondition solar-verify exported in this run{sw_doc}; the gas bound. -/
def spec : {G}.Spec where
  Contexts ctx := ctx = {G}.Context.default
  entries := [{", ".join(ent_terms)}]
  Valid _ := {valid}
  Allowed _ := {allowed}
  Post _ := {post}
  gas _ := {expr(shape, "gas")}
  assumptions := []

/-- **The published statement.**{any_doc} -/
def Statement : Prop :=
  _root_.{ns}.code.data.toList.map (·.toNat) = _root_.{ns}.runtimeBytes ∧
    {G}.Expected _root_.{ns}.src _root_.{ns}.spec _root_.{ns}.runtimeBytes{any_stmt}

set_option maxRecDepth 100000 in
theorem bound : _root_.{ns}.code.data.toList.map (·.toNat) = _root_.{ns}.runtimeBytes :=
  @of_decide_eq_true _ (instDecidableEqList _ _) (by decide +kernel)

end {ns}
""".replace("§", "\u0060")
    )
    expected_parts = (
        f"{G}.all_lt_of_eq _root_.{ns}.bound, ⟨⟨_, rfl⟩, ?_, ?_, ?_, ?_⟩, ?_"
    )
    refine = (
        f"refine ⟨_root_.{ns}.bound, {expected_parts}⟩"
        if route is None
        else f"refine ⟨_root_.{ns}.bound, ⟨{expected_parts}⟩{any_proof}⟩"
    )
    alts = " | ".join(["rfl"] * len(witnesses))
    wit = "\n".join(
        f"    · exact ⟨_, _root_.SolidCore.Refinement.Shapes.valid_of_find (sel := {s}) (n := {n}) "
        f"(_root_.SolidCore.Refinement.Shapes.covers_zeros _) (by decide),\n"
        f"        _root_.SolidCore.Refinement.Shapes.valid_of_find (sel := {s}) (n := {n}) "
        f"(_root_.SolidCore.Refinement.Shapes.covers_zeros _) (by decide)⟩"
        for s, n in witnesses
    )
    final_text = f"""import {a.name}.Statement
import {proof_mod}
{any_import}
-- Generated by solar's tools/certify/final.py; do not edit. The statement is
-- Certified.{a.name}.Final.Statement of {a.name}.Statement, which does not import the proof
-- module; the driver's checker requires §final§'s type to be exactly that constant.

set_option maxRecDepth 100000 in
theorem {ns}.final : _root_.{ns}.Statement := by
  {refine}
  · rintro _ rfl
    exact {G}.Context.default_ok
  · rintro _ rfl e he
    simp only [_root_.{ns}.spec, List.mem_cons, List.not_mem_nil, or_false] at he
    rcases he with {alts}
{wit}
  · obtain ⟨⟨call, obs, hv, hn⟩, -⟩ := ({nonvac})
    exact ⟨_, call, obs, rfl, hv, hn⟩
  · obtain ⟨-, ⟨call, obs, hv, hs, hn⟩⟩ := ({nonvac})
    exact ⟨_, call, obs, rfl, hv, hs, hn⟩
  · rintro - _ rfl
    obtain ⟨c, hc, h⟩ := ({proof})
    refine ⟨c, hc, ?_⟩
    rw [{G}.runtimeCode_eq _root_.{ns}.bound]
    exact {G}.certifiedAt_default h

#print axioms {ns}.final
""".replace("§", "\u0060")
    return stmt_text, final_text


def ctor_binders(shape, kind):
    """shape.json's ctor_args, checked: ([(var, bound)], the claim's binders and premises) or
    (None, "")."""
    if "ctor_args" not in shape:
        return None, ""
    b = shape["ctor_args"]
    if kind != "compiled" or "sem" in shape or "source_sem" in shape:
        fail(
            "shape.json: ctor_args is for kind compiled on a checked source with SolidCore's semantics"
        )
    if not (isinstance(b, list) and b):
        fail("shape.json: ctor_args must be a non-empty list")
    out = []
    if "ctor_returns" in shape:
        r = shape["ctor_returns"]
        w = r.get("witness") if isinstance(r, dict) and set(r) == {"witness"} else None
        if not (
            isinstance(w, list)
            and len(w) == len(b)
            and all(
                isinstance(x, int) and not isinstance(x, bool) and x >= 0 for x in w
            )
        ):
            fail(
                'shape.json: ctor_returns must be {"witness": [one natural number per argument]}'
            )
    for i, x in enumerate(b):
        if not (isinstance(x, dict) and set(x) == {"var", "lt"}):
            fail('shape.json: each ctor_args entry must be {"var": ..., "lt": ...}')
        m = re.fullmatch(r"2 \^ (\d+)", x["lt"]) if isinstance(x["lt"], str) else None
        if x["var"] != f"A{i}":
            fail(f"shape.json: ctor_args entry {i} must name A{i}")
        if m is None or not (8 <= int(m.group(1)) <= 256 and int(m.group(1)) % 8 == 0):
            fail(
                "shape.json: a ctor_args bound must be 2 ^ W, W a multiple of 8 from 8 to 256"
            )
        out.append((x["var"], x["lt"]))
    contract, fuel = expr(shape, "contract"), expr(shape, "fuel")
    returns = ""
    if "ctor_returns" in shape:
        for x, (_, lt) in zip(shape["ctor_returns"]["witness"], out):
            if x >= 2 ** int(lt.split("^")[1]):
                fail(f"shape.json: the witness word {x} is not below {lt}")
        returns = (
            f"_root_.SolidCore.Refinement.Generic.CtorReturns ({fuel}) ({contract}) "
            f"[{', '.join(v for v, _ in out)}] → "
        )
    claim = (
        f"∀ ({' '.join(v for v, _ in out)} : Nat), "
        + "".join(f"{v} < {lt} → " for v, lt in out)
        + returns
        + "\n      "
    )
    return out, claim


def ctor_witness(shape):
    """The statement's last conjunct for ctor_returns: the constructor returns on the witness words."""
    if "ctor_returns" not in shape:
        return ""
    w = shape["ctor_returns"]["witness"]
    return (
        f" ∧\n    _root_.SolidCore.Refinement.Generic.CtorReturns ({expr(shape, 'fuel')}) "
        f"({expr(shape, 'contract')}) [{', '.join(str(x) for x in w)}]"
    )


def pinned_and_bound(shape, kind, proof_mod, spec_mods, nonvac):
    """shape.json's pinned_bytes/pinned_module (the byte list the proof's code is built from and its
    data module, or None) and iteration_bound (the bound's variable and its literal maximum, or
    None), checked."""
    pinned = ib = None
    if "pinned_bytes" in shape or "pinned_module" in shape:
        if kind not in ("certified", "functional", "compiled"):
            fail(
                "shape.json: pinned_bytes is for kinds certified, functional and compiled"
            )
        pmod = shape.get("pinned_module")
        if not isinstance(pmod, str) or not NAME.match(pmod):
            fail(
                "shape.json: pinned_bytes needs pinned_module, the data module that defines it"
            )
        if pmod == proof_mod:
            fail("shape.json: the pinned bytes cannot come from the proof module")
        pinned = (expr(shape, "pinned_bytes"), pmod)
    if "iteration_bound" in shape:
        b = shape["iteration_bound"]
        if kind not in ("certified", "functional", "compiled"):
            fail(
                "shape.json: iteration_bound is for kinds certified, functional and compiled"
            )
        if not (isinstance(b, dict) and set(b) == {"var", "max"}):
            fail('shape.json: iteration_bound must be {"var": V, "max": N}')
        v, n = b["var"], b["max"]
        if not (isinstance(v, str) and re.fullmatch(r"[A-Z][A-Za-z0-9]*", v)):
            fail("shape.json: iteration_bound.var must be a capitalised identifier")
        if not (isinstance(n, int) and not isinstance(n, bool) and n >= 0):
            fail("shape.json: iteration_bound.max must be a natural number literal")
        if v in {"code", "runtimeBytes", "c"} or any(
            re.search(rf"\b{re.escape(v)}\b", m) for m in spec_mods + [proof_mod]
        ):
            fail(
                f"shape.json: iteration_bound.var {v!r} clashes with a module or statement name"
            )
        ib = (v, n)
    return pinned, ib


def bound_mode(
    a,
    ns,
    lines,
    byte_list,
    export,
    shape_t,
    nonvac_def,
    nonvac_conj,
    claim,
    pinned,
    ib,
    proof_mod,
    env,
    proof,
    binders,
    nonvac,
):
    """The statement with this run's bytes bound to a pinned artifact (pinned_bytes) and/or quantified
    over an iteration bound (iteration_bound). With pinned_bytes, `code` is the run's bytes
    themselves, and `runtime_is_fresh` (decided by the kernel) is the statement's first conjunct: the
    pinned list the proof was built on is exactly this run's bytes, so a stale or substituted artifact,
    or a mutated byte of the run, fails. Without it, `code` is the run's fresh export and `bound`
    binds it, as in the plain mode."""
    rest = f" {binders}" if env else ""
    if ib is not None and nonvac is not None:
        # Non-vacuity at every bound: NonVacuous takes the bound, inside its binder.
        v, n = ib
        claim = f"∀ ({v} : Nat), {v} ≤ {n} →\n      (_root_.{ns}.NonVacuous {v} ∧ ({claim}))"
        inner = f"fun{rest} => ({proof}) h{v}{rest}" if env else f"({proof}) h{v}"
        body = f"fun {v} h{v} => ⟨({nonvac}) h{v}, {inner}⟩"
        nonvac_conj = ""
    elif ib is not None:
        v, n = ib
        claim = f"∀ ({v} : Nat), {v} ≤ {n} →\n      ({claim})"
        body = f"fun {v} h{v}{rest} => ({proof}) h{v}{rest}"
    else:
        body = f"fun{rest} => ({proof}){rest}" if env else f"({proof})"
    if pinned is not None:
        pb = pinned[0]
        code_def = f"""/-- The runtime bytes this run emitted, as code: what the statement's shape speaks of. -/
def code : ByteArray := ⟨(_root_.{ns}.runtimeBytes.map UInt8.ofNat).toArray⟩"""
        binding = f"({pb}) = _root_.{ns}.runtimeBytes"
        thms = f"""set_option maxRecDepth 100000 in
theorem runtime_is_fresh : ({pb}) = _root_.{ns}.runtimeBytes :=
  @of_decide_eq_true _ (instDecidableEqList _ _) (by decide +kernel)

/-- The proof's code is the statement's `code`: the byte list the proof was built on, bound to this
run's bytes by `runtime_is_fresh` (the kernel's evaluation of the equality of the two literals). -/
theorem code_eq : (⟨(({pb}).map UInt8.ofNat).toArray⟩ : ByteArray) = _root_.{ns}.code :=
  congrArg (fun l : List Nat => (⟨(l.map UInt8.ofNat).toArray⟩ : ByteArray)) _root_.{ns}.runtime_is_fresh
"""
        doc = (
            "binds the byte list the proof was built on to the runtime bytecode this run emitted\n"
            "(`runtimeBytes`, from the driver's own compile) and states"
        )
        first = f"_root_.{ns}.runtime_is_fresh"
        carry = f"  rw [← _root_.{ns}.code_eq]\n"
    else:
        code_def = f"""/-- The freshly exported bytecode the obligations speak of. -/
def code : ByteArray := ⟨({export}.bytecode.map UInt8.ofNat).toArray⟩"""
        binding = (
            f"_root_.{ns}.code.data.toList.map (·.toNat) = _root_.{ns}.runtimeBytes"
        )
        thms = f"""set_option maxRecDepth 100000 in
theorem bound : _root_.{ns}.code.data.toList.map (·.toNat) = _root_.{ns}.runtimeBytes :=
  @of_decide_eq_true _ (instDecidableEqList _ _) (by decide +kernel)
"""
        doc = "binds the exact runtime bytecode (`runtimeHex`, from the driver's own compile) and states"
        first = f"_root_.{ns}.bound"
        carry = ""
    bound_doc = (
        f" for every iteration bound `{ib[0]} ≤ {ib[1]}`" if ib is not None else ""
    )
    stmt_text = (
        "\n".join(lines)
        + f"""

-- Generated by solar's tools/certify/final.py from {a.name}/shape.json and the bytecode the
-- driver compiled in this run; do not edit. This module never imports the proof module.

/-!
# {a.contract}: the published statement

`Statement` {doc}
{shape_t.replace("_root_.", "")} of it against the checked source on the declared valid calls{bound_doc}.
-/

namespace {ns}

/-- The runtime bytecode the pinned solar emitted in this run, as hex (for the reader). -/
def runtimeHex : String :=
  "{a.hex}"

/-- The same bytes, as numbers: what the statement binds. -/
def runtimeBytes : List Nat :=
  [{byte_list}]

{code_def}
{nonvac_def}
/-- **The published statement.** -/
def Statement : Prop :=
  {binding} ∧
    {nonvac_conj}({claim})

{thms}
end {ns}
"""
    )
    nv = f"  refine ⟨{nonvac}, ?_⟩\n" if nonvac is not None and ib is None else ""
    final_text = f"""import {a.name}.Statement
import {proof_mod}

-- Generated by solar's tools/certify/final.py; do not edit. The statement is
-- Certified.{a.name}.Final.Statement of {a.name}.Statement, which does not import the proof
-- module; the driver's checker requires `final`'s type to be exactly that constant.

theorem {ns}.final : _root_.{ns}.Statement := by
  refine ⟨{first}, ?_⟩
{nv}{carry}  exact {body}

#print axioms {ns}.final
"""
    return stmt_text, final_text


def nonvac_text(ns, valid, post, allowed, allowed_vac, ib, nv_name="NonVacuous"):
    """The definition of the statement's non-vacuity (kinds other than open_functional), named
    NV_NAME, of the valid calls VALID, the postcondition POST and the allowed statuses ALLOWED (only
    the specification's half when ALLOWED_VAC says why the allowed statuses reject nothing), taking
    the iteration bound IB's variable when there is one."""
    call_t, obs_t = "_root_.SolarTV.Call", "_root_.SolarTV.Observable"
    nv_bind = f" ({ib[0]} : Nat)" if ib is not None else ""
    nv_at = f" (at the iteration bound {ib[0]})" if ib is not None else ""
    if allowed_vac is not None:
        nonvac_def = f"""
/-- The specification is not vacuous{nv_at}: some valid call has a result it rejects (the allowed
statuses reject nothing: {allowed_vac}). -/
def {nv_name}{nv_bind} : Prop :=
  ∃ (call : {call_t}) (obs : {obs_t}), ({valid}) call ∧ ¬ ({post}) call obs
"""
    else:
        nonvac_def = f"""
/-- The specification and the allowed statuses are not vacuous{nv_at}: some valid call has a result
the specification rejects, and some valid call has a revert the allowed statuses reject. -/
def {nv_name}{nv_bind} : Prop :=
  (∃ (call : {call_t}) (obs : {obs_t}), ({valid}) call ∧ ¬ ({post}) call obs) ∧
    (∃ (call : {call_t}) (obs : {obs_t}), ({valid}) call ∧ obs.success = false ∧
      ¬ ({allowed}) call obs)
"""
    return nonvac_def


def spec_of(shape, a, kind, opened, export_ns, valid):
    """shape.json's specification keys, checked (kinds other than compiled): the allowed statuses and
    the postcondition (or the export's), whether they are bound to solar-verify's export, the vacuity
    or non-vacuity witness, the allowed statuses' vacuity and the shape's specification arguments."""
    allowed_vac = None
    bound_spec, bound_allowed, vacuity, nonvac = False, False, None, None
    allowed = post = None
    if kind == "compiled":
        for k in (
            "allowed",
            "post",
            "spec_export",
            "nonvacuous",
            "vacuity",
            "allowed_vacuity",
        ):
            if k in shape:
                fail(f"shape.json: kind compiled states no specification (no {k})")
        spec_args = ""
    else:
        if shape.get("allowed_export") is True:
            if "allowed" in shape or "spec_export" not in shape:
                fail(
                    "shape.json: allowed_export needs spec_export and excludes allowed"
                )
            se = shape["spec_export"]
            allowed = (
                f"_root_.SolidCore.Refinement.Shapes.bundleAllowed _root_.{export_ns}.{a.contract} "
                f"({expr(se, 'ctx')})"
            )
            bound_allowed = True
        elif "allowed_export" in shape:
            fail("shape.json: allowed_export must be true when present")
        else:
            allowed = expr(shape, "allowed")
        if "spec_export" in shape:
            if "post" in shape:
                fail("shape.json: spec_export and post exclude each other")
            se = shape["spec_export"]
            if not isinstance(se, dict) or set(se) != {"ctx", "step"}:
                fail('shape.json: spec_export must be {"ctx": ..., "step": ...}')
            post = (
                f"_root_.SolidCore.Refinement.Shapes.bundlePost _root_.{export_ns}.{a.contract} "
                f"({expr(se, 'ctx')}) ({expr(se, 'step')})"
            )
            if opened:
                post = f"fun _ => {post}"
            bound_spec = True
        else:
            post = expr(shape, "post")
        if "allowed_vacuity" in shape:
            if not (
                isinstance(shape["allowed_vacuity"], str)
                and shape["allowed_vacuity"].strip()
            ):
                fail("shape.json: allowed_vacuity must say why")
            if "nonvacuous" not in shape:
                fail(
                    "shape.json: allowed_vacuity narrows nonvacuous (the specification's half "
                    "stays): it needs nonvacuous"
                )
            allowed_vac = shape["allowed_vacuity"].strip()
        for k, v in (("allowed", allowed), ("post", post)):
            if k == "allowed" and allowed_vac is not None:
                continue
            if TRIVIAL.match(v) and not (  # ty: ignore[no-matching-overload]
                isinstance(shape.get("vacuity"), str) and shape["vacuity"].strip()
            ):
                fail(
                    f"shape.json: {k} is trivially true ({v!r}): it must reject something, or "
                    "vacuity must say why it does not"
                )
        if "nonvacuous" in shape:
            if "vacuity" in shape:
                fail("shape.json: nonvacuous and vacuity exclude each other")
            nonvac = expr(shape, "nonvacuous")
        elif isinstance(shape.get("vacuity"), str) and shape["vacuity"].strip():
            vacuity = shape["vacuity"].strip()
        else:
            fail(
                f"shape.json: kind {kind} needs nonvacuous (a proof that the specification and the "
                "allowed statuses reject something) or vacuity (why that is not shown)"
            )
        if "succeeds_witness" in shape:
            # A spec without succeeds-if conditions: the export's allowed statuses and postcondition,
            # each with the driver's witness clause (succeeds_witness.py), which only strengthens them.
            if not (bound_spec and bound_allowed and nonvac is not None) or opened:
                fail(
                    "shape.json: succeeds_witness needs spec_export, allowed_export and nonvacuous"
                )
            if shape.get("statement") != "expected":
                fail("shape.json: succeeds_witness is for statement expected only")
            try:
                ws = [
                    succeeds_witness.strengthen(
                        t,
                        shape["succeeds_witness"],
                        expr(shape, "storage"),
                        f"{export_ns}.{a.contract}",
                        expr(shape["spec_export"], "ctx"),
                    )
                    for t in (allowed, post)
                ]
            except ValueError as e:
                fail(f"shape.json: {e}")
            allowed, post = ws
            for t in ws:
                if not EXPR.match(t):
                    fail(
                        "shape.json: succeeds_witness makes a term with a colon or newline"
                    )
        spec_args = f" ({allowed}) ({post})"
        if opened:
            spec_args = (
                f" ({expr(shape, 'env_ok')}) ({expr(shape, 'wide')}) ({expr(shape, 'admit')})"
                f" ({valid}) ({allowed}) ({post})"
            )
    return {
        "allowed": allowed,
        "post": post,
        "bound_spec": bound_spec,
        "bound_allowed": bound_allowed,
        "vacuity": vacuity,
        "nonvac": nonvac,
        "allowed_vac": allowed_vac,
        "spec_args": spec_args,
    }


# shape.json keys that state a single claim: with parts, each part states its own instead.
PART_KEYS = (
    "function",
    "valid",
    "allowed",
    "allowed_export",
    "post",
    "spec_export",
    "nonvacuous",
    "vacuity",
    "allowed_vacuity",
    "gas",
    "iteration_bound",
    "proof",
    "spec",
    "excluded",
)
# What a parts statement does not take (at the top level or in a part): a pinned artifact, a source
# below SolidCore, an open world, assumptions, or the expected statement.
NOT_IN_PARTS = (
    "pinned_bytes",
    "pinned_module",
    "sem",
    "prog",
    "env_ok",
    "wide",
    "admit",
    "statement",
    "ctor_args",
    "ctor_returns",
    "succeeds_witness",
)


def parts_mode(a, shape):
    """shape.json with parts: a contract with several loop functions, each certified on its own valid
    calls. The statement binds the bytes once (bound) and states, for each part in order, the claim
    the single mode states for a shape with that part's keys: `∃ c, CONTRACT = .ok c ∧ SHAPE …` on
    the part's valid calls, with the part's allowed statuses, postcondition (or the export's) and gas
    bound, quantified over the part's own iteration bound and with the part's own NonVacuous
    (`NonVacuous_FUNCTION`) when it has one. The top level names the kind, the checked source
    (contract, fuel, source_sem), spec_modules and proof_module; every claim key belongs to a part
    (PART_KEYS). Each part's proof is a term of the type the single mode asks for. Writes the same
    two modules and record as main, with the record's statuses per part."""
    kind = shape.get("kind")
    if kind not in ("certified", "functional", "compiled"):
        fail("shape.json: parts are for kinds certified, functional and compiled")
    for k in [*PART_KEYS, *NOT_IN_PARTS]:
        if k in shape and k not in ("spec", "excluded"):
            fail(f"shape.json: with parts, {k} belongs to a part, or is not supported")
    if shape.get("env"):
        fail("shape.json: parts take no env assumptions")
    if not re.fullmatch(r"[0-9a-f]+", a.hex) or len(a.hex) % 2:
        fail("--hex is not the lowercase hex of the bytecode")
    for k in ("name", "contract"):
        if not NAME.match(getattr(a, k)):
            fail(f"--{k} is not a Lean name")
    parts = shape.get("parts")
    if not (
        isinstance(parts, list) and parts and all(isinstance(p, dict) for p in parts)
    ):
        fail("shape.json: parts must be a non-empty list of objects")
    fns = [p.get("function") for p in parts]
    if any(
        not (isinstance(f, str) and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", f))
        for f in fns
    ):
        fail("shape.json: every part needs function, an identifier")
    if len(set(fns)) != len(fns):
        fail("shape.json: two parts name the same function")
    contract, fuel = expr(shape, "contract"), expr(shape, "fuel")
    sem_fn = shape.get("source_sem", "SolidCore.Refinement.solidCore")
    if not isinstance(sem_fn, str) or not NAME.match(sem_fn):
        fail("shape.json: source_sem must be a Lean name")
    ns = f"Certified.{a.name}.Final"
    export_ns = f"Certified.{a.name}.gen.{a.contract}.Spec"
    spec_mods = shape.get("spec_modules", [])
    proof_mod = shape.get("proof_module")
    for m in spec_mods + [proof_mod]:
        if not isinstance(m, str) or not NAME.match(m):
            fail(f"shape.json: {m!r} is not a Lean name")
    if proof_mod in spec_mods:
        fail("shape.json: the proof module cannot define the statement's inputs")
    if proof_mod in (f"{a.name}.Statement", f"{a.name}.Final"):
        fail("shape.json: the proof module cannot be the statement or the final module")
    shape_t = {
        "certified": "_root_.SolidCore.Refinement.Shapes.Certified",
        "functional": "_root_.SolidCore.Refinement.Shapes.Functional",
        "compiled": "_root_.SolidCore.Refinement.Shapes.Compiled",
    }[kind]
    # The run's export namespace, as main reads it (shape.json's export_ns, else solar's default).
    plain = re.fullmatch(r"[A-Za-z][A-Za-z0-9_]*", a.contract)
    exported = (
        shape.get("export_ns")
        or f"SolarTV.Export.{a.contract if plain else f'«{a.contract}»'}"
    )
    if not isinstance(exported, str) or not NAME.match(
        exported.replace("«", "").replace("»", "")
    ):
        fail("shape.json: export_ns is not a Lean name")
    export = f"_root_.{exported}"
    claims, terms, defs, recs = [], [], [], []
    any_spec = any_allowed = False
    for p, fn in zip(parts, fns):
        for k in p:
            if k not in PART_KEYS:
                fail(f"shape.json: part {fn}: unknown key {k}")
        valid = expr(p, "valid")
        sp = spec_of(p, a, kind, False, export_ns, valid)
        nonvac = sp["nonvac"]
        _, ib = pinned_and_bound(p, kind, proof_mod, spec_mods, nonvac)
        gas = f" ({expr(p, 'gas')})" if kind in ("certified", "compiled") else ""
        proof = expr(p, "proof")
        any_spec |= sp["bound_spec"]
        any_allowed |= sp["bound_allowed"]
        base = (
            f"∃ c, {contract} = .ok c ∧\n      {shape_t} (_root_.{sem_fn} ({fuel})) c "
            f"_root_.{ns}.code\n        ({valid}){sp['spec_args']}{gas}"
        )
        nv = f"NonVacuous_{fn}"
        if nonvac is not None:
            defs.append(
                nonvac_text(
                    ns, valid, sp["post"], sp["allowed"], sp["allowed_vac"], ib, nv
                )
            )
        if ib is not None:
            v, n = ib
            if nonvac is not None:
                claim = f"∀ ({v} : Nat), {v} ≤ {n} →\n      (_root_.{ns}.{nv} {v} ∧ ({base}))"
                term = f"fun {v} h{v} => ⟨({nonvac}) h{v}, ({proof}) h{v}⟩"
            else:
                claim = f"∀ ({v} : Nat), {v} ≤ {n} →\n      ({base})"
                term = f"fun {v} h{v} => ({proof}) h{v}"
        elif nonvac is not None:
            claim = f"_root_.{ns}.{nv} ∧ ({base})"
            term = f"⟨{nonvac}, {proof}⟩"
        else:
            claim, term = base, proof
        claims.append(claim)
        terms.append(term)
        st = statuses(kind, p.get("spec"), sp["bound_spec"], sp["vacuity"])
        if sp["allowed_vac"] is not None:
            st["termination"] += (
                f" (the allowed statuses reject nothing: {sp['allowed_vac']})"
            )
            st["vacuity"] += (
                f"; the allowed statuses: none rejected ({sp['allowed_vac']})"
            )
        recs.append(
            {
                "function": fn,
                "nonvacuous": f"{ns}.{nv}" if nonvac is not None else None,
                "vacuity": sp["vacuity"],
                "allowed_vacuity": sp["allowed_vac"],
                "spec_bound_to_export": (
                    f"{export_ns}.{a.contract}" if sp["bound_spec"] else None
                ),
                "allowed_bound_to_export": sp["bound_allowed"],
                "spec": p.get("spec"),
                "statuses": st,
                "strongest": kind == "certified"
                and sp["bound_spec"]
                and sp["vacuity"] is None
                and sp["allowed_vac"] is None,
                "iteration_bound": {"var": ib[0], "max": ib[1]} if ib else None,
                "excluded": p.get("excluded", []),
            }
        )
    imports = [
        "Verify.Shapes",
        *(["Verify.ShapeGeneric"] if kind == "compiled" else []),
        *(
            ["Verify.ShapeBundle", f"{a.name}.gen.{a.contract}.Spec"]
            if any_spec
            else []
        ),
        *(["Verify.ShapeSpecExport"] if any_allowed else []),
        f"{a.name}.gen.{a.contract}.Bytecode",
        *spec_mods,
    ]
    lines = [f"import {m}" for m in dict.fromkeys(imports)]
    byte_list = ",".join(
        ("\n   " if i % 16 == 0 and i else "") + " " + hex(b)
        for i, b in enumerate(bytes.fromhex(a.hex))
    ).strip()
    claim = " ∧\n    ".join(f"({c})" for c in claims)
    named = ", ".join(f"`{f}`" for f in fns)
    stmt_text = (
        "\n".join(lines)
        + f"""

-- Generated by solar's tools/certify/final.py from {a.name}/shape.json and the bytecode the
-- driver compiled in this run; do not edit. This module never imports the proof module.

/-!
# {a.contract}: the published statement

`Statement` binds the exact runtime bytecode (`runtimeHex`, from the driver's own compile) and
states {shape_t.replace("_root_.", "")} of it against the checked source, once for each of
{named}, on that function's declared valid calls.
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
{"".join(defs)}
/-- **The published statement.** -/
def Statement : Prop :=
  _root_.{ns}.code.data.toList.map (·.toNat) = _root_.{ns}.runtimeBytes ∧
    ({claim})

set_option maxRecDepth 100000 in
theorem bound : _root_.{ns}.code.data.toList.map (·.toNat) = _root_.{ns}.runtimeBytes :=
  @of_decide_eq_true _ (instDecidableEqList _ _) (by decide +kernel)

end {ns}
"""
    )
    final_text = f"""import {a.name}.Statement
import {proof_mod}

-- Generated by solar's tools/certify/final.py; do not edit. The statement is
-- Certified.{a.name}.Final.Statement of {a.name}.Statement, which does not import the proof
-- module; the driver's checker requires `final`'s type to be exactly that constant.

theorem {ns}.final : _root_.{ns}.Statement :=
  ⟨{", ".join([f"_root_.{ns}.bound", *terms])}⟩

#print axioms {ns}.final
"""
    open(a.statement_out, "w").write(stmt_text)  # noqa: SIM115
    open(a.out, "w").write(final_text)  # noqa: SIM115
    stmt = " ".join(
        stmt_text.split("def Statement : Prop :=", 1)[1]
        .split("set_option maxRecDepth 100000 in\ntheorem ")[0]
        .split()
    )

    def per(key):
        vals = [r["statuses"][key] for r in recs]
        if len(set(vals)) == 1:
            return vals[0]
        return "per function: " + "; ".join(
            f"{r['function']}: {v}" for r, v in zip(recs, vals)
        )

    def some(key):
        vals = [(r["function"], r[key]) for r in recs if r[key] is not None]
        return "; ".join(f"{f}: {v}" for f, v in vals) if vals else None

    exported = [r["spec_bound_to_export"] for r in recs if r["spec_bound_to_export"]]
    if a.record:
        json.dump(
            {
                "module": f"{a.name}.Final",
                "statement_module": f"{a.name}.Statement",
                "theorem": f"{ns}.final",
                "kind": kind,
                "shape": shape_t.replace("_root_.", ""),
                "statement": stmt,
                "nonvacuous": some("nonvacuous"),
                "vacuity": some("vacuity"),
                "allowed_vacuity": some("allowed_vacuity"),
                "spec_bound_to_export": exported[0] if exported else None,
                "allowed_bound_to_export": any(
                    r["allowed_bound_to_export"] for r in recs
                ),
                "assumptions": [],
                "excluded": shape.get("excluded", []),
                "spec": shape.get("spec"),
                "statuses": {k: per(k) for k in recs[0]["statuses"]},
                "strongest": all(r["strongest"] for r in recs),
                "parts": recs,
            },
            open(a.record, "w"),  # noqa: SIM115
            indent=2,
            ensure_ascii=False,
        )
    print(f"{ns}.final")


# --- the expected statement of a contract with loop functions (parts, iteration bounds) ---------

# The keys of a part of the expected statement: the claim keys a part has (PART_KEYS) but the Lean
# terms of its valid calls, statuses and gas bound, which the statement writes here from the part's
# call data (call), and the checked absence of its spec (no_spec).
EXPECTED_PART_KEYS = (
    *(k for k in PART_KEYS if k not in ("valid", "allowed", "post", "gas")),
    "call",
    "no_spec",
)
# The source semantics the loop route's statement runs the checked contract with.
LOOP_SEM = "SolidCore.Refinement.Generic.Loop.solidCoreRuntime"
# The single fixed frame the loop route's spec is stated in (Verify/Spec/TreeLoop.lean).
LOOP_FRAME = "SolidCore.Spec.TreeLoop.ctx0"


def run_export(shape, contract):
    """The namespace of the run's export: shape.json's export_ns (the driver exports each run under
    its own, tools/certify/export_ns.py), else solar's default for CONTRACT; refused unless a Lean
    name."""
    plain = re.fullmatch(r"[A-Za-z][A-Za-z0-9_]*", contract)
    exported = (
        shape.get("export_ns")
        or f"SolarTV.Export.{contract if plain else f'«{contract}»'}"
    )
    if not isinstance(exported, str) or not NAME.match(
        exported.replace("«", "").replace("»", "")
    ):
        fail("shape.json: export_ns is not a Lean name")
    return exported


def lean_str(s):
    """A Lean string literal of S (printable ASCII only: the driver's hashes, names and options)."""
    if not (
        isinstance(s, str)
        and re.fullmatch(r"[ -~]*", s)
        and '"' not in s
        and "\\" not in s
    ):
        fail(f"not a plain string literal: {s!r}")
    return f'"{s}"'


def stage_manifest(gen, contract, exported=None):
    """The stage export's manifest (GEN/Manifest.lean, solar -Zemit-lean-stages), read for its stage
    list, and every stage file's SHA-256 computed here from the file Lean builds: [(file, def, pass,
    sha)]. The statement then states the manifest's digests equal to these literal hashes, and the
    kernel compares them (a stage edited after the export, or a manifest that names another file's
    digest, fails). Every stage module of GEN must be listed, the first lowered MIR (Stage0,
    lower-evm-shaped) first and the assembled Bytecode last."""
    import hashlib
    import pathlib

    gen = pathlib.Path(gen)
    mf = gen / "Manifest.lean"
    if not mf.is_file():
        fail(f"statement expected: no stage manifest {mf}")
    text = mf.read_text()
    exported = exported or f"SolarTV.Export.{contract}"
    if not re.search(
        rf"^namespace {re.escape(exported)}\.Manifest$", text, re.MULTILINE
    ):
        fail(f"statement expected: {mf} is not the manifest of {contract}'s export")
    m = re.search(
        r"^def stages : List \(String × String × String × String\) :=\n(.*?)\n\n",
        text,
        re.MULTILINE | re.DOTALL,
    )
    if not m:
        fail(f"statement expected: {mf} has no stage list")
    rows = re.findall(
        r'\("([A-Za-z0-9_]+)", "([A-Za-z0-9_]+)", "([a-z0-9-]+)", "([0-9a-f]{64})"\)',
        m.group(1),
    )
    if m.group(1).count("(") != len(rows) or not rows:
        fail(f"statement expected: {mf}'s stage list does not parse")
    out = []
    for f, d, p, _ in rows:
        sf = gen / f"{f}.lean"
        if not sf.is_file():
            fail(f"statement expected: the manifest's stage {f} has no file {sf}")
        out.append((f, d, p, hashlib.sha256(sf.read_bytes()).hexdigest()))
    listed = {f for f, *_ in out}
    extra = sorted(
        x.stem
        for x in gen.glob("*.lean")
        if x.stem not in listed | {"Manifest", "Spec"}
    )
    if extra:
        fail(f"statement expected: stage modules the manifest does not list: {extra}")
    if out[0][:3] != ("Stage0", "stage0", "lower-evm-shaped") or out[-1][:3] != (
        "Bytecode",
        "bytecode",
        "assemble",
    ):
        fail(
            "statement expected: the stage list is not first lowered MIR (Stage0) to Bytecode"
        )
    return out


def expected_valid(call, bound):
    """The valid calls of a part, written here from its call data: a call of the selector with
    ARITY ABI words below 2^256, no value, any storage, and the loop's limit (a calldata argument or
    an initial storage word) at most the iteration bound BOUND (or a literal limit, no bound)."""
    n, sel, lim = call["arity"], call["selector"], call["limit"]
    args = [f"a{i}" for i in range(n)]
    conj = [f"{x} < 2 ^ 256" for x in args]
    if lim is not None:
        kind, i = lim
        conj.append(
            f"a{i} ≤ {bound}"
            if kind == "arg"
            else f"_root_.SolarTV.slotValue σ {i} ≤ {bound}"
        )
    ex = " ".join(["σ", *args])
    pre = "".join(c + " ∧ " for c in conj)
    return (
        f"∃ {ex}, {pre}call = {{ calldata := _root_.SolidCore.Refinement.Abi.calldata {sel} "
        f"[{', '.join(args)}], callvalue := 0, storage := σ }}"
    )


def expected_inhabited(call, bound):
    """A proof of ∃ call, VALID: the call with every argument zero from empty storage."""
    n, lim = call["arity"], call["limit"]
    ps = ["[]", *["0"] * n, *["by decide"] * n]
    if lim is not None:
        ps.append(f"Nat.zero_le {bound}")
    return f"⟨_, {', '.join(ps)}, rfl⟩"


def check_call(p, fn):
    """A part's call data (expected statement), checked."""
    c = p.get("call")
    keys = {"selector", "arity", "limit", "per_iteration_gas", "exit_gas", "M"}
    if not (isinstance(c, dict) and set(c) == keys):
        fail(
            f"shape.json: part {fn}: call must be {{selector, arity, limit, per_iteration_gas, exit_gas, M}}"
        )

    def nat(v):
        return isinstance(v, int) and not isinstance(v, bool) and v >= 0

    if not (
        nat(c["selector"])
        and c["selector"] < 2**32
        and nat(c["arity"])
        and nat(c["per_iteration_gas"])
        and nat(c["exit_gas"])
    ):
        fail(
            f"shape.json: part {fn}: call's selector, arity and gas parts must be naturals"
        )
    lim = c["limit"]
    if lim is not None and not (
        isinstance(lim, list)
        and len(lim) == 2
        and lim[0] in ("arg", "slot")
        and nat(lim[1])
        and (lim[0] == "slot" or lim[1] < c["arity"])
    ):
        fail(
            f'shape.json: part {fn}: call.limit must be null, ["arg", i] or ["slot", n]'
        )
    if (lim is None) != (c["M"] is not None) or (
        c["M"] is not None and not nat(c["M"])
    ):
        fail(
            f"shape.json: part {fn}: a literal limit (call.limit null) needs its iteration count M, and only it"
        )
    return c


def loop_expected_mode(a, shape):
    """shape.json with parts and statement expected: the driver-fixed statement of a contract with
    loop functions (one part per loop function, one or several), written here from data only, never
    from the parts' Lean terms. It imports the library, this run's exports (the spec bundle, the
    stage manifest, the bytecode) and the source's AST module, and no generated proof-side module (no
    valid calls, witness, join or inventory module), so the proof modules' types are unified against
    it by the kernel. The statement is the conjunction of:
    * the binding of the fresh export's bytes to the bytes this run compiled (check_final's gate);
    * the fresh source and its spec: the export's manifest sourceSha256 is this run's source, hashed
      here (--source); the export's bundle specifies exactly the inventory [(name, selector)] (with
      True parts: it has no contract invariant);
    * the compiler, its options, the MIR and every stage: the stage export's manifest names this
      run's source and its hash, the compiler and its arguments (the driver's), and its stage
      digests (the first lowered MIR, Stage0, to the assembled Bytecode) equal the literal SHA-256
      of each stage file, hashed here, and its bytecode digest that of this run's bytes;
    * per part (its function's calls): for every iteration bound K ≤ Kmax (a literal limit: once),
      the valid calls are inhabited (a satisfiable precondition), and Shapes.Certified on them:
      equivalence, termination, the property and the gas bound; with the export's spec, also its
      NonVacuous: a valid call and a result whose return data decodes as the function's returns
      (the spec's own types) that the exported postcondition rejects; without one, the
      kernel-checked absence of its name and selector from the export (translation only).
    The proof is the parts' terms with the inventory module's theorems; Final.lean imports the
    inventory module, so the closure replay covers it."""
    import hashlib
    import pathlib

    kind = shape.get("kind")
    if kind != "certified":
        fail("shape.json: statement expected with parts is for kind certified")
    for k in [*PART_KEYS, *(x for x in NOT_IN_PARTS if x != "statement")]:
        if k in shape and k not in ("spec", "excluded"):
            fail(f"shape.json: with parts, {k} belongs to a part, or is not supported")
    if shape.get("env"):
        fail("shape.json: parts take no env assumptions")
    if not re.fullmatch(r"[0-9a-f]+", a.hex) or len(a.hex) % 2:
        fail("--hex is not the lowercase hex of the bytecode")
    for k in ("name", "contract"):
        if not NAME.match(getattr(a, k)):
            fail(f"--{k} is not a Lean name")
    if not a.source:
        fail("statement expected needs --source, this run's source file")
    src = pathlib.Path(a.source)
    if not src.is_file():
        fail(f"--source {src} is not a file")
    src_sha = hashlib.sha256(src.read_bytes()).hexdigest()
    gen = (
        pathlib.Path(a.gen)
        if a.gen
        else pathlib.Path(a.statement_out).parent / "gen" / a.contract
    )
    stages = stage_manifest(gen, a.contract, run_export(shape, a.contract))
    ex = shape.get("expected")
    if not (
        isinstance(ex, dict)
        and set(ex) == {"toolchain", "inventory", "inventory_module"}
    ):
        fail(
            "shape.json: statement expected needs expected {toolchain, inventory, inventory_module}"
        )
    tc = ex["toolchain"]
    if not (
        isinstance(tc, dict)
        and set(tc) == {"compiler", "arguments", "evm_version", "optimization"}
        and isinstance(tc["arguments"], list)
        and tc["arguments"]
    ):
        fail(
            "shape.json: expected.toolchain must be {compiler, arguments, evm_version, optimization}"
        )
    if tc["arguments"][0] != src.name:
        fail(
            f"shape.json: the compiler's first argument is {tc['arguments'][0]!r}, not the source {src.name!r}"
        )
    inv = ex["inventory"]
    if not (
        isinstance(inv, list)
        and all(
            isinstance(e, list)
            and len(e) == 2
            and isinstance(e[0], str)
            and isinstance(e[1], int)
            for e in inv
        )
    ):
        fail("shape.json: expected.inventory must list [name, selector] pairs")
    inv_mod = ex["inventory_module"]
    if not (isinstance(inv_mod, str) and NAME.match(inv_mod)):
        fail("shape.json: expected.inventory_module must be a module name")
    sa = shape.get("source_ast")
    src_mod = f"{a.name}.Source"
    if not (
        isinstance(sa, dict)
        and sa.get("namespace") == f"Certified.{a.name}.Src"
        and sa.get("contract") == a.contract
    ):
        fail(
            f"shape.json: statement expected needs source_ast of this contract in Certified.{a.name}.Src ({src_mod})"
        )
    if shape.get("source_sem") != LOOP_SEM:
        fail(
            f"shape.json: statement expected states the loop route's source semantics {LOOP_SEM}"
        )
    fuel = expr(shape, "fuel")
    if not re.fullmatch(r"[0-9 ^+*()]+", fuel):
        fail("shape.json: fuel must be a numeral expression")
    proof_mod = shape.get("proof_module")
    if not (isinstance(proof_mod, str) and NAME.match(proof_mod)) or proof_mod in (
        f"{a.name}.Statement",
        f"{a.name}.Final",
        src_mod,
        inv_mod,
    ):
        fail(
            "shape.json: proof_module must be a module other than the statement's inputs"
        )
    parts = shape.get("parts")
    if not (
        isinstance(parts, list) and parts and all(isinstance(p, dict) for p in parts)
    ):
        fail("shape.json: parts must be a non-empty list of objects")
    fns = [p.get("function") for p in parts]
    if any(
        not (isinstance(f, str) and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", f))
        for f in fns
    ):
        fail("shape.json: every part needs function, an identifier")
    if len(set(fns)) != len(fns):
        fail("shape.json: two parts name the same function")
    ns = f"Certified.{a.name}.Final"
    S = f"_root_.Certified.{a.name}.gen.{a.contract}.Spec"
    B = f"{S}.{a.contract}"
    M = f"_root_.{run_export(shape, a.contract)}.Manifest"
    I = f"_root_.Certified.{inv_mod}"
    contract = (
        "_root_.SolidCore.Solidity.TypeCheck.CheckedInput.contract "
        f"_root_.{sa['namespace']}.importedSourceUnit {lean_str(a.contract)}"
    )
    sem = f"_root_.{LOOP_SEM} ({fuel})"
    post = (
        f"_root_.SolidCore.Refinement.Shapes.bundlePost {B} (fun _ => _root_.{LOOP_FRAME}) "
        "_root_.SolidCore.Refinement.Shapes.idStep"
    )
    allowed = f"_root_.SolidCore.Refinement.Shapes.bundleAllowed {B} (fun _ => _root_.{LOOP_FRAME})"
    inv_names = {n for n, _ in inv}
    inv_sels = {s for _, s in inv}
    claims, terms, recs, sels = [], [], [], []
    nospec_any = False
    for p, fn in zip(parts, fns):
        for k in p:
            if k not in EXPECTED_PART_KEYS:
                fail(f"shape.json: part {fn}: unknown key {k}")
        c = check_call(p, fn)
        sel = c["selector"]
        if sel in sels:
            fail(f"shape.json: two parts state the selector {sel:#010x}")
        sels.append(sel)
        proof = expr(p, "proof")
        ib = p.get("iteration_bound")
        if (ib is None) != (c["limit"] is None):
            fail(
                f"shape.json: part {fn}: an iteration bound exactly when the loop's limit is the call's"
            )
        if ib is not None:
            if not (
                isinstance(ib, dict)
                and set(ib) == {"var", "max"}
                and ib["var"] == "K"
                and isinstance(ib["max"], int)
                and not isinstance(ib["max"], bool)
                and ib["max"] >= 0
            ):
                fail(
                    f'shape.json: part {fn}: iteration_bound must be {{"var": "K", "max": N}}'
                )
            bnd = "K"
            gas = f"fun _ => K * {c['per_iteration_gas']} + {c['exit_gas']}"
        else:
            bnd = None
            gas = f"fun _ => {c['M']} * {c['per_iteration_gas']} + {c['exit_gas']}"
        valid = expected_valid(c, bnd)
        inh = expected_inhabited(c, bnd)
        arg = " hK" if bnd else ""
        has_spec = "spec_export" in p
        if has_spec:
            if p.get("allowed_export") is not True or p["spec_export"] != {
                "ctx": f"fun _ => {LOOP_FRAME}",
                "step": "SolidCore.Refinement.Shapes.idStep",
            }:
                fail(
                    f"shape.json: part {fn}: the expected statement states the export in the fixed frame {LOOP_FRAME}"
                )
            if sel not in inv_sels:
                fail(
                    f"shape.json: part {fn}: its selector {sel:#010x} is not in the export's inventory"
                )
            if "no_spec" in p or "vacuity" in p:
                fail(
                    f"shape.json: part {fn}: a part with the export's spec has nonvacuous, not no_spec or vacuity"
                )
            nonvac = expr(p, "nonvacuous")
            nv = (
                f"∃ (call : _root_.SolarTV.Call) (obs : _root_.SolarTV.Observable), ({valid}) ∧\n"
                f"        (∃ s ∈ {B}.spec.specs, s.selector = {sel} ∧\n"
                "          ∃ vs, _root_.SolidCore.Spec.decodeWords s.rets "
                "(_root_.SolidCore.Spec.words obs.output) = some vs) ∧\n"
                f"        ¬ {post} call obs"
            )
            body = (
                f"(∃ (call : _root_.SolarTV.Call), {valid}) ∧\n      ({nv}) ∧\n"
                f"      ∃ c, {contract} = .ok c ∧\n"
                f"        _root_.SolidCore.Refinement.Shapes.Certified ({sem}) c _root_.{ns}.code\n"
                f"          (fun call => {valid})\n          ({allowed}) ({post}) ({gas})"
            )
            inner = f"⟨{inh}, ({nonvac}){arg}, ({proof}){arg}⟩"
            claim = f"∀ (K : Nat), K ≤ {ib['max']} →\n      {body}" if bnd else body
            term = f"fun K hK => {inner}" if bnd else inner
        else:
            ns_thm = p.get("no_spec")
            if not (
                isinstance(ns_thm, str)
                and NAME.match(ns_thm)
                and ns_thm.startswith(f"Certified.{inv_mod}.")
            ):
                fail(
                    f"shape.json: part {fn}: a part without the export's spec needs no_spec, a theorem of {inv_mod}"
                )
            if any(k in p for k in ("nonvacuous", "allowed_export", "allowed_vacuity")):
                fail(
                    f"shape.json: part {fn}: a part without a spec has no nonvacuous or allowed statuses"
                )
            qual = f"{a.contract}.{fn}"
            if sel in inv_sels or qual in inv_names:
                fail(
                    f"shape.json: part {fn}: the export specifies it (name or selector): never True"
                )
            nospec_any = True
            absent = f"({B}.spec.specs.all fun s => s.name != {lean_str(qual)} && s.selector != {sel}) = true"
            body = (
                f"(∃ (call : _root_.SolarTV.Call), {valid}) ∧\n"
                f"      ∃ c, {contract} = .ok c ∧\n"
                f"        _root_.SolidCore.Refinement.Shapes.Certified ({sem}) c _root_.{ns}.code\n"
                f"          (fun call => {valid})\n          (fun _ _ => True) (fun _ _ => True) ({gas})"
            )
            inner = f"⟨{inh}, ({proof}){arg}⟩"
            if bnd:
                claim = f"{absent} ∧\n    ∀ (K : Nat), K ≤ {ib['max']} →\n      {body}"
                term = f"⟨_root_.{ns_thm}, fun K hK => {inner}⟩"
            else:
                claim = f"{absent} ∧\n    {body}"
                term = f"⟨_root_.{ns_thm}, {inner}⟩"
        claims.append(claim)
        terms.append(term)
        vac = (
            None
            if has_spec
            else (p.get("vacuity") or "translation only: no spec in the export")
        )
        avac = p.get("allowed_vacuity") if has_spec else None
        st = statuses(kind, p.get("spec"), has_spec, vac)
        if avac is not None:
            st["termination"] += f" (the allowed statuses reject nothing: {avac})"
            st["vacuity"] += f"; the allowed statuses: none rejected ({avac})"
        recs.append(
            {
                "function": fn,
                "selector": f"0x{sel:08x}",
                "valid": f"fun call => {valid}",
                "gas": gas,
                "nonvacuous": f"{ns}.Statement (well-typed, by {p.get('nonvacuous')})"
                if has_spec
                else None,
                "no_spec": p.get("no_spec"),
                "vacuity": vac,
                "allowed_vacuity": avac,
                "spec_bound_to_export": B.removeprefix("_root_.") if has_spec else None,
                "allowed_bound_to_export": has_spec,
                "spec": p.get("spec"),
                "statuses": st,
                "strongest": has_spec and avac is None,
                "iteration_bound": {"var": "K", "max": ib["max"]} if ib else None,
                "excluded": p.get("excluded", []),
            }
        )
    # An export spec no part states (another function's) stays in the stated inventory, and the
    # statement claims nothing of it; the driver's promotion refuses such a final
    # (generic.loop_promotion_problem), but the final itself still checks.
    byte_list = ",".join(
        ("\n   " if i % 16 == 0 and i else "") + " " + hex(b)
        for i, b in enumerate(bytes.fromhex(a.hex))
    ).strip()
    bytes_sha = hashlib.sha256(bytes.fromhex(a.hex)).hexdigest()
    inv_lit = ", ".join(f"({lean_str(n)}, {s:#010x})" for n, s in inv)
    stage_lit = ",\n      ".join(
        f"({lean_str(f)}, {lean_str(d)}, {lean_str(p)}, {lean_str(h)})"
        for f, d, p, h in stages
    )
    args_lit = ", ".join(lean_str(x) for x in tc["arguments"])
    dec = "by decide +kernel"
    fixed = [
        (
            "the fresh source",
            f"{S}.manifest.sourceSha256 = {lean_str(src_sha)}",
            f"{I}.source_is_fresh",
        ),
        (
            "the spec's inventory",
            f"{B}.spec.specs.map (fun s => (s.name, s.selector)) = [{inv_lit}]",
            f"{I}.inventory",
        ),
        *(
            [
                (
                    "no contract invariant",
                    f"{B}.spec.invariants = []",
                    f"{I}.no_invariants",
                )
            ]
            if nospec_any
            else []
        ),
        (
            "the source the stages were compiled from",
            f"{M}.sources = [({lean_str(src.name)}, {lean_str(src_sha)})]",
            dec,
        ),
        ("the compiler", f"{M}.compiler = {lean_str(tc['compiler'])}", dec),
        ("its options", f"{M}.arguments = [{args_lit}]", dec),
        ("the EVM version", f"{M}.evmVersion = {lean_str(tc['evm_version'])}", dec),
        ("the optimization", f"{M}.optimization = {lean_str(tc['optimization'])}", dec),
        ("the MIR and every stage", f"{M}.stages =\n      [{stage_lit}]", dec),
        ("the assembled bytes", f"{M}.bytecodeSha256 = {lean_str(bytes_sha)}", dec),
    ]
    imports = [
        "Verify.Shapes",
        "Verify.ShapeBundle",
        "Verify.ShapeSpecExport",
        "Verify.Spec.Contract",
        "Verify.Spec.TreeLoop",
        f"{a.name}.gen.{a.contract}.Spec",
        f"{a.name}.gen.{a.contract}.Manifest",
        f"{a.name}.gen.{a.contract}.Bytecode",
        src_mod,
    ]
    lines = [f"import {m}" for m in imports]
    fixed_text = "".join(f"\n    -- {what}\n    ({prop}) ∧" for what, prop, _ in fixed)
    claim = " ∧\n    ".join(f"({c})" for c in claims)
    named = ", ".join(f"`{f}`" for f in fns)
    export = f"_root_.{run_export(shape, a.contract)}"
    stmt_text = (
        "\n".join(lines)
        + f"""

-- Generated by solar's tools/certify/final.py (statement expected) from this run's source, its
-- exports and the driver's data; do not edit. This module imports no proof module and no generated
-- proof-side module: the proof's types are unified against it by the kernel.

/-!
# {a.contract}: the published statement (expected)

`Statement` is the driver-fixed expected statement of a contract with loop functions ({named}):
the fresh export's bytes are the bytes the driver compiled; the export's manifest is of this run's
source (SHA-256 {src_sha}) and its spec bundle specifies exactly the inventory; the stage export's
manifest names the source, the compiler {tc["compiler"]}, its options and the digest of the first
lowered MIR and of every stage, each equal to the literal SHA-256 of the file; and for each
function, at every iteration bound, its valid calls are inhabited and the exact bytes are
Shapes.Certified against the checked source (equivalence, termination, the property, resources at
the gas bound), with the exported spec's NonVacuous on a well-typed result, or the kernel-checked
absence of a spec (translation only).
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

/-- **The published statement** (expected). -/
def Statement : Prop :=
  _root_.{ns}.code.data.toList.map (·.toNat) = _root_.{ns}.runtimeBytes ∧{fixed_text}
    -- per function
    {claim}

set_option maxRecDepth 100000 in
theorem bound : _root_.{ns}.code.data.toList.map (·.toNat) = _root_.{ns}.runtimeBytes :=
  @of_decide_eq_true _ (instDecidableEqList _ _) (by decide +kernel)

end {ns}
"""
    )
    proofs = [
        f"_root_.{ns}.bound",
        *(pf if pf.startswith("_root_") else f"({pf})" for _, _, pf in fixed),
        *terms,
    ]
    sep = ",\n   "
    final_text = f"""import {a.name}.Statement
import {proof_mod}
import {inv_mod}

-- Generated by solar's tools/certify/final.py (statement expected); do not edit. The statement is
-- Certified.{a.name}.Final.Statement of {a.name}.Statement, which imports no proof module; the
-- driver's checker requires `final`'s type to be exactly that constant. {inv_mod} is
-- imported and used here, so the closure replay covers the export's inventory.

set_option maxRecDepth 100000 in
theorem {ns}.final : _root_.{ns}.Statement :=
  ⟨{sep.join(proofs)}⟩

/-- The expected statement, as written independently of the proof modules, holds of the final. -/
example : _root_.{ns}.Statement := _root_.{ns}.final

#print axioms {ns}.final
"""
    open(a.statement_out, "w").write(stmt_text)  # noqa: SIM115
    open(a.out, "w").write(final_text)  # noqa: SIM115
    stmt = " ".join(
        stmt_text.split("def Statement : Prop :=", 1)[1]
        .split("set_option maxRecDepth 100000 in\ntheorem ")[0]
        .split()
    )

    def per(key):
        vals = [r["statuses"][key] for r in recs]
        if len(set(vals)) == 1:
            return vals[0]
        return "per function: " + "; ".join(
            f"{r['function']}: {v}" for r, v in zip(recs, vals)
        )

    def some(key):
        vals = [(r["function"], r[key]) for r in recs if r[key] is not None]
        return "; ".join(f"{f}: {v}" for f, v in vals) if vals else None

    exported = [r["spec_bound_to_export"] for r in recs if r["spec_bound_to_export"]]
    if a.record:
        json.dump(
            {
                "module": f"{a.name}.Final",
                "statement_module": f"{a.name}.Statement",
                "theorem": f"{ns}.final",
                "kind": kind,
                "shape": "SolidCore.Refinement.Shapes.Certified",
                "statement": stmt,
                "statement_kind": "expected",
                "expected": {
                    "source_sha256": src_sha,
                    "source_file": src.name,
                    "spec_bundle": B.removeprefix("_root_."),
                    "inventory": [
                        {"name": n, "selector": f"0x{s:08x}"} for n, s in inv
                    ],
                    "inventory_module": inv_mod,
                    "no_invariants": nospec_any,
                    "toolchain": tc,
                    "stages": [
                        {"file": f, "def": d, "pass": p, "sha256": h}
                        for f, d, p, h in stages
                    ],
                    "runtime_sha256": bytes_sha,
                    "binds": [w for w, _, _ in fixed]
                    + [
                        "the runtime bytes",
                        (
                            "per function: inhabited valid calls and Shapes.Certified, with the "
                            "export's well-typed NonVacuous or the checked absence of a spec"
                        ),
                    ],
                },
                "nonvacuous": some("nonvacuous"),
                "vacuity": some("vacuity"),
                "allowed_vacuity": some("allowed_vacuity"),
                "spec_bound_to_export": exported[0] if exported else None,
                "allowed_bound_to_export": any(
                    r["allowed_bound_to_export"] for r in recs
                ),
                "assumptions": [],
                "excluded": shape.get("excluded", []),
                "spec": shape.get("spec"),
                "statuses": {k: per(k) for k in recs[0]["statuses"]},
                "strongest": all(r["strongest"] for r in recs),
                "parts": recs,
            },
            open(a.record, "w"),  # noqa: SIM115
            indent=2,
            ensure_ascii=False,
        )
    print(f"{ns}.final")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("shape")
    ap.add_argument("--name", required=True)
    ap.add_argument("--contract", required=True)
    ap.add_argument("--hex", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--statement-out", required=True)
    ap.add_argument("--record", help="write the statuses and statement here as JSON")
    ap.add_argument(
        "--source", help="statement expected: this run's source file, hashed here"
    )
    ap.add_argument(
        "--gen",
        help="statement expected: the stage export's directory (default: gen/CONTRACT beside --statement-out)",
    )
    a = ap.parse_args()
    shape = json.load(open(a.shape))  # noqa: SIM115
    if "parts" in shape and shape.get("statement") == "expected":
        return loop_expected_mode(a, shape)
    if "parts" in shape:
        return parts_mode(a, shape)
    kind = shape.get("kind")
    if kind not in ("certified", "functional", "compiled", "open_functional"):
        fail(
            "shape.json: kind must be certified, functional, compiled or open_functional"
        )
    opened = kind == "open_functional"
    if opened and "sem" in shape:
        fail(
            "shape.json: kind open_functional states a checked source (contract and fuel)"
        )
    if not opened:
        for k in ("env_ok", "wide", "admit"):
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
        sem_fn = shape.get("source_sem", "SolidCore.Refinement.solidCore")
        if not isinstance(sem_fn, str) or not NAME.match(sem_fn):
            fail("shape.json: source_sem must be a Lean name")
        source = f"({fuel}) c" if opened else f"(_root_.{sem_fn} ({fuel})) c"
        exists = f"∃ c, {contract} = .ok c ∧\n      "
    ctor_args, ctor_claim = ctor_binders(shape, kind)
    if ctor_args is not None:
        source = (
            "(_root_.SolidCore.Refinement.solidCoreFromArgs _root_.SolidCore.Refinement.callerAddress "
            f"({fuel}) [{', '.join(v for v, _ in ctor_args)}]) c"
        )
    valid = expr(shape, "valid")
    ns = f"Certified.{a.name}.Final"
    export_ns = f"Certified.{a.name}.gen.{a.contract}.Spec"
    sp = spec_of(shape, a, kind, opened, export_ns, valid)
    allowed, post, spec_args = sp["allowed"], sp["post"], sp["spec_args"]
    bound_spec, bound_allowed = sp["bound_spec"], sp["bound_allowed"]
    vacuity, nonvac, allowed_vac = sp["vacuity"], sp["nonvac"], sp["allowed_vac"]
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
    pinned, ib = pinned_and_bound(shape, kind, proof_mod, spec_mods, nonvac)
    shape_t = {
        "certified": "_root_.SolidCore.Refinement.Shapes.Certified",
        "functional": "_root_.SolidCore.Refinement.Shapes.Functional",
        "compiled": "_root_.SolidCore.Refinement.Shapes.Compiled",
        "open_functional": "_root_.SolidCore.Refinement.Shapes.OpenFunctional",
    }[kind]
    gas = f" ({expr(shape, 'gas')})" if kind in ("certified", "compiled") else ""
    # The namespace of the run's export: shape.json's export_ns (the driver's runs, exported under their
    # own namespace, tools/certify/export_ns.py), else solar's default for the contract.
    plain = re.fullmatch(r"[A-Za-z][A-Za-z0-9_]*", a.contract)
    exported = (
        shape.get("export_ns")
        or f"SolarTV.Export.{a.contract if plain else f'«{a.contract}»'}"
    )
    if not isinstance(exported, str) or not NAME.match(
        exported.replace("«", "").replace("»", "")
    ):
        fail("shape.json: export_ns is not a Lean name")
    export = f"_root_.{exported}"
    premises = "".join(f"({e}) →\n    " for e in env)
    binders = " ".join(f"_h{i}" for i in range(len(env)))
    term = f"{proof} {binders}".strip()
    imports = [
        "Verify.Shapes",
        *(["Verify.ShapeGeneric"] if kind == "compiled" else []),
        *(["SolidCore.Refinement.Generic.StageFrom"] if ctor_args is not None else []),
        *(["Verify.ShapeOpen"] if opened else []),
        *(
            ["Verify.ShapeBundle", f"{a.name}.gen.{a.contract}.Spec"]
            if bound_spec
            else []
        ),
        *(["Verify.ShapeSpecExport"] if bound_allowed else []),
        pinned[1] if pinned else f"{a.name}.gen.{a.contract}.Bytecode",
        *spec_mods,
    ]
    lines = [f"import {m}" for m in dict.fromkeys(imports)]
    byte_list = ",".join(
        ("\n   " if i % 16 == 0 and i else "") + " " + hex(b)
        for i, b in enumerate(bytes.fromhex(a.hex))
    ).strip()
    call_t, obs_t = "_root_.SolarTV.Call", "_root_.SolarTV.Observable"
    nonvac_def = nonvac_conj = ""
    if nonvac is not None and opened:
        allowed_half = (
            ""
            if allowed_vac is not None
            else f""" ∧
    (∃ (call : {call_t}) (obs : {obs_t}), ({valid}) call ∧ obs.success = false ∧
      ∀ (W : _root_.SolidCore.Refinement.World), ({expr(shape, "admit")}) W → ¬ ({allowed}) W call obs)"""
        )
        nonvac_def = f"""
/-- The specification is not vacuous: some valid call has a result it rejects in every world
{"(the allowed statuses reject nothing: " + allowed_vac + ")" if allowed_vac is not None else "and some valid call has a revert no admitted world allows"}. -/
def NonVacuous : Prop :=
  (∃ (call : {call_t}) (obs : {obs_t}), ({valid}) call ∧ ∀ (W : _root_.SolidCore.Refinement.World), ¬ ({post}) W call obs){allowed_half}
"""
        nonvac_conj = f"_root_.{ns}.NonVacuous ∧\n    "
    elif nonvac is not None:
        nonvac_def = nonvac_text(ns, valid, post, allowed, allowed_vac, ib)
        nonvac_conj = f"_root_.{ns}.NonVacuous ∧\n    "
    stmt_text = (
        "\n".join(lines)
        + f"""

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
    {nonvac_conj}({premises}{ctor_claim}{exists}{shape_t} {source} _root_.{ns}.code
        {"" if opened else "(" + valid + ")"}{spec_args}{gas}){ctor_witness(shape)}

set_option maxRecDepth 100000 in
theorem bound : _root_.{ns}.code.data.toList.map (·.toNat) = _root_.{ns}.runtimeBytes :=
  @of_decide_eq_true _ (instDecidableEqList _ _) (by decide +kernel)

end {ns}
"""
    )
    parts = (
        [f"_root_.{ns}.bound"]
        + ([nonvac] if nonvac is not None else [])
        + [("fun " + binders + " => " if env else "") + term]
    )
    final_text = f"""import {a.name}.Statement
import {proof_mod}

-- Generated by solar's tools/certify/final.py; do not edit. The statement is
-- Certified.{a.name}.Final.Statement of {a.name}.Statement, which does not import the proof
-- module; the driver's checker requires `final`'s type to be exactly that constant.

theorem {ns}.final : _root_.{ns}.Statement :=
  ⟨{", ".join(parts)}⟩

#print axioms {ns}.final
"""
    if ctor_args is not None and (
        pinned is not None or ib is not None or env or "statement" in shape
    ):
        fail(
            "shape.json: ctor_args is stated alone (no pinned_bytes, iteration_bound, env or statement)"
        )
    if pinned is not None or ib is not None:
        stmt_text, final_text = bound_mode(
            a,
            ns,
            lines,
            byte_list,
            export,
            shape_t,
            nonvac_def,
            nonvac_conj,
            f"{premises}{exists}{shape_t} {source} _root_.{ns}.code\n"
            f"        {'' if opened else '(' + valid + ')'}{spec_args}{gas}",
            pinned,
            ib,
            proof_mod,
            env,
            proof,
            binders,
            nonvac,
        )
    expected = shape.get("statement") == "expected"
    if "statement" in shape and not expected:
        fail("shape.json: statement must be expected when present")
    if expected and (pinned is not None or ib is not None):
        fail(
            "shape.json: statement expected binds the run's bytes itself (no pinned_bytes or iteration_bound)"
        )
    if expected:
        if (
            kind != "certified"
            or not bound_spec
            or not bound_allowed
            or "contract" not in shape
        ):
            fail(
                "shape.json: statement expected needs kind certified, spec_export, allowed_export and a checked source"
            )
        stmt_text, final_text = expected_mode(
            a,
            shape,
            ns,
            valid,
            allowed,
            post,
            nonvac,
            env,
            spec_mods,
            proof_mod,
            proof,
            byte_list,
            export,
        )
    open(a.statement_out, "w").write(stmt_text)  # noqa: SIM115
    open(a.out, "w").write(final_text)  # noqa: SIM115
    stmt = " ".join(
        stmt_text.split("def Statement : Prop :=", 1)[1]
        .split("set_option maxRecDepth 100000 in\ntheorem ")[0]
        .split()
    )
    strongest = (
        kind == "certified" and bound_spec and vacuity is None and allowed_vac is None
    )
    st_rec = statuses(kind, shape.get("spec"), bound_spec, vacuity)
    sw = shape.get("succeeds_witness")
    if sw is not None:
        st_rec["spec"] += (
            "; the exported allowed statuses and postcondition are each strengthened by the "
            "witness clause (succeeds_witness.py): " + succeeds_witness.describe(sw)
        )
        st_rec["vacuity"] = (
            "proved non-vacuous (NonVacuous in the statement) by the driver's succeeds witness, "
            "the spec having no succeeds-if conditions: "
            + succeeds_witness.describe(sw)
        )
    if allowed_vac is not None:
        st_rec["termination"] += (
            f" (the allowed statuses reject nothing: {allowed_vac})"
        )
        st_rec["vacuity"] += f"; the allowed statuses: none rejected ({allowed_vac})"
    extra = {}
    if pinned is not None:
        extra["fresh_binding"] = {
            "theorem": f"{ns}.runtime_is_fresh",
            "pinned_bytes": pinned[0],
            "pinned_module": pinned[1],
        }
    if ib is not None:
        extra["iteration_bound"] = {"var": ib[0], "max": ib[1]}
    if ctor_args is not None:
        extra["ctor_args"] = [{"var": v, "lt": lt} for v, lt in ctor_args]
        if "ctor_returns" in shape:
            extra["ctor_returns"] = shape["ctor_returns"]
    if a.record:
        json.dump(
            {
                "module": f"{a.name}.Final",
                "statement_module": f"{a.name}.Statement",
                "theorem": f"{ns}.final",
                "kind": kind,
                "shape": "SolidCore.Verify.Generic.Expected"
                if expected
                else shape_t.replace("_root_.", ""),
                "statement": stmt,
                "nonvacuous": (f"{ns}.NonVacuous" if nonvac is not None else None),
                "vacuity": vacuity,
                "allowed_vacuity": allowed_vac,
                "spec_bound_to_export": (
                    f"{export_ns}.{a.contract}" if bound_spec else None
                ),
                "allowed_bound_to_export": bound_allowed,
                "succeeds_witness": sw,
                "assumptions": env,
                "excluded": shape.get("excluded", []),
                "spec": shape.get("spec"),
                "statuses": st_rec,
                "strongest": strongest,
                **extra,
            },
            open(a.record, "w"),  # noqa: SIM115
            indent=2,
            ensure_ascii=False,
        )
    print(f"{ns}.final")


if __name__ == "__main__":
    main()
