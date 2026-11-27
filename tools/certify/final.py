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
  iteration_bound  {"var": V, "max": N} (kinds certified, functional and compiled): the statement
            holds for every iteration bound `V ≤ N` (N a literal), the terms valid, allowed, post and
            gas may name V, and the proof is a term of type
            `∀ {V : Nat}, V ≤ N → ∀ env…, ∃ c, contract = .ok c ∧ Shape`. With nonvacuous,
            NonVacuous takes V too, the statement is `∀ V, V ≤ N → NonVacuous V ∧ …` (non-vacuity
            at every bound, the bound 0 included), and nonvacuous is a term of type
            `∀ {V : Nat}, V ≤ N → NonVacuous V`
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
allowed statuses and postcondition solar-verify exported in this run; the gas bound. -/
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
    shape = json.load(open(a.shape))  # noqa: SIM115
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
    valid = expr(shape, "valid")
    ns = f"Certified.{a.name}.Final"
    export_ns = f"Certified.{a.name}.gen.{a.contract}.Spec"
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
        spec_args = f" ({allowed}) ({post})"
        if opened:
            spec_args = (
                f" ({expr(shape, 'env_ok')}) ({expr(shape, 'wide')}) ({expr(shape, 'admit')})"
                f" ({valid}) ({allowed}) ({post})"
            )
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
    export = f"_root_.SolarTV.Export.{a.contract}"
    premises = "".join(f"({e}) →\n    " for e in env)
    binders = " ".join(f"_h{i}" for i in range(len(env)))
    term = f"{proof} {binders}".strip()
    imports = [
        "Verify.Shapes",
        *(["Verify.ShapeGeneric"] if kind == "compiled" else []),
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
        nv_bind = f" ({ib[0]} : Nat)" if ib is not None else ""
        nv_at = f" (at the iteration bound {ib[0]})" if ib is not None else ""
        if allowed_vac is not None:
            nonvac_def = f"""
/-- The specification is not vacuous{nv_at}: some valid call has a result it rejects (the allowed
statuses reject nothing: {allowed_vac}). -/
def NonVacuous{nv_bind} : Prop :=
  ∃ (call : {call_t}) (obs : {obs_t}), ({valid}) call ∧ ¬ ({post}) call obs
"""
        else:
            nonvac_def = f"""
/-- The specification and the allowed statuses are not vacuous{nv_at}: some valid call has a result
the specification rejects, and some valid call has a revert the allowed statuses reject. -/
def NonVacuous{nv_bind} : Prop :=
  (∃ (call : {call_t}) (obs : {obs_t}), ({valid}) call ∧ ¬ ({post}) call obs) ∧
    (∃ (call : {call_t}) (obs : {obs_t}), ({valid}) call ∧ obs.success = false ∧
      ¬ ({allowed}) call obs)
"""
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
    {nonvac_conj}({premises}{exists}{shape_t} {source} _root_.{ns}.code
        {"" if opened else "(" + valid + ")"}{spec_args}{gas})

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
