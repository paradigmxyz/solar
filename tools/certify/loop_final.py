"""The published final theorem of a loop contract, from the driver's generated modules.

For a source the envelope blocks on its loop, the loop cut hook (tools/certify/loop_cuts.py) has the
base's generator write, per loop function FN on the erasure route, the obligation modules
NAME.Loops.FN.{Data,V*,Obl}: the erasure (erasedOf_eq), the cut check (leaves), the sequents (seqs,
valid) over the simplified erased tree T and their observational equality with the erased tree
(same). This module adds what the published statement still needs and composes it, for every
contract whose loops eraseLoopStmt erases, with no per-contract Lean:

* the backend stage of this run's export, from the cut's first lowered MIR to the final EVM IR
  (tools/certify/certify.py, through cut points when the program loops: NAME.Proofs.Stages and its
  X_mir_to_evmir);
* the fixed-fuel premises on the run's own bytes (tools/certify/loop_resources.py's module
  NAME.LoopResources: check_ok, loops, the inhabited valid calls and the iteration bound Kmax);
* NAME.LoopValid, the valid calls as a statement input (the body of LoopResources' Valid, copied
  byte for byte so the two are the same term);
* NAME.LoopJoin: theorem join, solidity-lean's EraseLoopFinal.final_of_generated applied to the
  above (hence EraseLoopJoin.eraseLoop_certified_generated, hence LeastFuel's BridgeTarget);
* NAME/shape.json for tools/certify/final.py: kind certified, source semantics solidCoreRuntime at
  the erasure's statement fuel (loop_cuts.LOOP_FUEL), the run's fresh export as code (final.py's
  bound), iteration_bound {K, Kmax} when the loop's limit comes from the call, and source_ast so the
  driver binds the checked source AST to this run's source.

The specification, when solar-verify exports one of the loop function (spec_step): the source's
bytes are copied into the clone as Certified/NAME/NAME.sol and exported there (NAME.gen.C.Spec, the
bundle the statement names), NAME.LoopCore is the front end's AST of the same bytes with the
contract's selector table, and spec_connect.py --loops (the O6 spec bridge, Verify/Spec/
TreeLoop{,Check}.lean) writes NAME.LoopSpec: `F_loop_spec`, TreeLoop.LoopSpec of the exported spec on the
loop erasure's tree, with `spec_is_fresh` and `source_is_fresh` binding the bundle and the AST to this
run's bytes. The join then states the user's @custom:ensures over the erased tree: the allowed
statuses `Shapes.bundleAllowed B (fun _ => TreeLoop.ctx0)` and the postcondition
`Shapes.bundlePost B (fun _ => TreeLoop.ctx0) Shapes.idStep`, with the fixed frame named in the
statement (Verify/Spec/TreeLoopFrame.lean's `certified_ctx0`, from the bridge's `ctxOfCall`), through
Verify/Phase3/Route/FinalW.lean's `final_of_obl_spec` (`final_of_generatedW_spec` on the
erasure-inferred route). NAME.LoopNonVac proves that the exported postcondition rejects a result of a
valid call at every iteration bound (the call with zero arguments from the storage [(1, 0)], and a
success that writes 1 to every slot): the specification's half of final.py's NonVacuous. The
bridge takes no succeeds-if conditions on the loop route, so the allowed statuses accept every
revert; the statement declares that (allowed_vacuity) and is not the strongest status. A source
with no spec of the loop function keeps the True specification, declared vacuous, and so does one
the bridge refuses, with the refusal in the declaration and the record.

What it does not give, recorded in shape.json's excluded and never reported as the strongest
status: deployment (the statement is about the runtime bytecode from any storage: runtime-only),
other callers, values and blocks (the one frame TreeLoop.ctx0, until Phase 2's caller
generalization is wired), the other functions of the contract (the valid calls are the loop
function's), and calls whose loop limit exceeds K.

Acceptance (review 189): the spec is exported by the pinned solar-verify (solar-verify.pin: the
commit of O6's branch with its spec-scope fixes, an immutable copy of the binary refused unless it
is the pinned SHA-256), never the shared checkout's; spec_connect.py's receipt is required and must be of
this run's source digest (record source.sha256, the source file now and the copy the export read),
of the pinned exporter and of NAME.LoopSpec as it is now, both after the bridge and again once
check_final accepted the final; and the kernel replays the final's whole constant closure
(replay_closure.lean, accounted by replay.py closure) before the final is recorded.

Refusals are precise and never a weaker proof (Refused, with the step that refused):
"route": a function on the summary route, or the hook did not complete; "functions": not exactly
one loop function; "resources": loop_resources.py's Unsupported; "abi": a parameter that is not
uint256; "stages": the export's stage proofs do not compose from stage0 to the resources' final
EVM IR; "exporter": no pinned solar-verify; "receipt": the receipt is missing or not this run's;
"replay": the kernel replay of the closure fails or is not accounted for.
"""

import hashlib
import json
import os
import pathlib
import re
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import export_ns
import loop_cuts

JOIN_MODULE = "Verify.LoopSource.Equiv.EraseLoopFinal"
JOIN_LEMMA = f"{JOIN_MODULE}.final_of_generated"
# The erasure-inferred route (loop_infer.py's candidates, sequents Verify.Phase3.Infer.allSeqsW) and
# the exported spec on either route (Verify/Phase3/Route/FinalW.lean).
ROUTE_MODULE = "Verify.Phase3.Route.FinalW"
JOIN_W = "Verify.Phase3.Route.final_of_generatedW"
JOIN_OBL_SPEC = "Verify.Phase3.Route.final_of_obl_spec"
JOIN_W_SPEC = "Verify.Phase3.Route.final_of_generatedW_spec"
OBL_OF_SEQS = "Verify.LoopSource.Equiv.Inv.obl_of_seqs"
ROUTES = ("erasure", "erasure-inferred", "while-inferred")
# The while route (loop_cuts step 3w): eraseWhile with the loop's data, the source invariant's
# sequents and the MIR invariant's allSeqsW (Verify/LoopSource/Invariant/Final.lean).
WHILE_MODULE = "Verify.LoopSource.Invariant.Final"
JOIN_WHILE = f"{WHILE_MODULE}.final_of_whileW"
# The fixed frame of the loop route's spec: sender 0x2000, value 0, timestamp 1, self 0x1000.
FRAME = "SolidCore.Spec.TreeLoop.ctx0"
FRAME_MODULE = "Verify.Spec.TreeLoopFrame"
# The non-vacuity witness's storage: every slot 0, slot 1 present (review 154's [(1, 0)]).
WITNESS_STORAGE = "[(1, 0)]"
FUEL = loop_cuts.LOOP_FUEL
SOURCE_SEM = "SolidCore.Refinement.Generic.Loop.solidCoreRuntime"
ORACLE = "SolidCore.Refinement.Shapes.oracle"


# The solar-verify that exports the loop function's spec (spec_exporter). The shared checkout's
# solar-verify lacks O6's spec-scope fixes (review 189: enum members and inherited names resolved in
# the annotation's scope, storage keyed by declaration), so the spec comes from the commit that
# solar-verify.pin names, an immutable copy of the binary refused unless it is the pinned SHA-256
# (the digest spec_connect.py's --solar-verify-sha256 and the receipt then carry). VERIFY_FIXES are
# the commits the pinned one must contain.
VERIFY_PIN = HERE / "solar-verify.pin"
VERIFY_FIXES = (
    "5a0d4408f5468bc99e6cc8c8c17d2c04a60b1099",  # feat(verify): export enum members in Lean specs
    "f28eaa584daad7af3b467c25734f8130c8803929",  # fix(verify): resolve spec enum members in scope
    "62b957b195c9bc0ac5a11c7c4778556a046a8233",  # fix(verify): resolve inherited spec names in scope
    "1b7cb452aab34daaa8dcaa974333ff461bf190df",  # fix(verify): key spec storage by declaration
)
# The receipt fields the acceptance check compares (spec_connect.py's write_receipt).
RECEIPT_KEYS = (
    "module",
    "contract",
    "source",
    "sourceSha256",
    "exportedSha256",
    "exporterSha256",
    "astToolSha256",
    "astToolCacheSha256",
    "solcSha256",
    "moduleSha256",
)


class Refused(Exception):
    """Why the run publishes no loop final theorem, at which step."""

    def __init__(self, step, why):
        super().__init__(f"{step}: {why}")
        self.step = step
        self.why = why


def contract_term(name, contract):
    """The checked source: the same term the obligation generator's erasure names (gen_driver)."""
    return (
        "SolidCore.Solidity.TypeCheck.CheckedInput.contract "
        f'Certified.{name}.Src.importedSourceUnit "{contract}"'
    )


def loop_function(record):
    """The one loop function on the erasure route, from the loop cut hook's record."""
    cuts = record.get("loop_cuts") or {}
    blocked = record.get("loop_cuts_blocked") or {}
    if blocked:
        f, b = next(iter(blocked.items()))
        raise Refused(
            "route",
            f"the loop cut hook did not complete for {f if f != '*' else 'the source'}: "
            f"{b.get('step')}: {b.get('why')}",
        )
    if len(cuts) != 1:
        raise Refused(
            "functions",
            f"{len(cuts)} loop functions with obligations; the statement covers one loop function",
        )
    fname, entry = next(iter(cuts.items()))
    if entry.get("route") not in ROUTES:
        raise Refused(
            "route",
            f"{fname}: obligations from a body summary, not the erasure: final_of_generated "
            "takes the generator's erasedOf_eq",
        )
    return fname


def loop_route(record, fname):
    """The route of FNAME's obligations: erasure (Inv.allSeqs) or erasure-inferred (allSeqsW)."""
    return (record.get("loop_cuts") or {}).get(fname, {}).get("route", "erasure")


# --- the fixed-fuel module -------------------------------------------------------------------


def resources(name, contract, gen_dir, plain, sel, nargs, params):
    """loop_resources.py's module on this export, as NAME.LoopResources, with the facts the join
    needs: the limit's kind, Kmax, E, B, X, the gas parts and the valid calls' body."""
    import loop_resources

    if any(p != "uint256" for p in params):
        raise Refused(
            "abi",
            f"parameters {params}: the join's valid calls take uint256 words only",
        )
    try:
        ob = loop_resources.obligation(gen_dir, plain)
        last = ob["final_evm_ir"]
        lower = last[0].lower() + last[1:]
        text = loop_resources.generate(
            ns=f"Certified.{name}.LoopResources",
            contract=contract,
            last=lower,
            gen_dir=gen_dir,
            ob=ob,
            code_term=f"⟨({export_ns.of_gen(gen_dir)}.bytecode.map UInt8.ofNat).toArray⟩",
            code_import=f"{name}.gen.{contract}.Bytecode",
            sel=sel,
            nargs=nargs,
            source={"imports": [f"{name}.gen.{contract}.{last}"]},
        )
    except loop_resources.Unsupported as e:
        raise Refused("resources", str(e)) from None
    facts = parse_resources(text)
    facts.update(
        final_evm_ir=lower,
        per_iteration_gas=ob["per_iteration_gas"],
        exit_gas=ob["exit_gas"],
    )
    return text, facts


def parse_resources(text):
    """The limit's kind, Kmax, E B X and the valid calls' body, read off the module's text."""
    bounded = re.search(
        r"^def Valid \(K : Nat\) \(call : Call\) : Prop :=\n  (.+)$", text, re.MULTILINE
    )
    const = re.search(
        r"^def Valid \(call : Call\) : Prop :=\n  (.+)$", text, re.MULTILINE
    )
    kmax = re.search(r"^def Kmax : Nat := (\d+)$", text, re.MULTILINE)
    ebx = re.search(r"record (\d+) (\d+) (\d+) (K|M)\)", text)
    if not (kmax and ebx and (bounded or const)):
        raise Refused(
            "resources",
            "loop_resources.py's module has no Valid, Kmax or Loops premise",
        )
    m = re.search(r"^abbrev M : Nat := (\d+)$", text, re.MULTILINE)
    return {
        "bounded": bool(bounded),
        "valid_body": (bounded or const).group(1),
        "Kmax": int(kmax.group(1)),
        "E": int(ebx.group(1)),
        "B": int(ebx.group(2)),
        "X": int(ebx.group(3)),
        "M": int(m.group(1)) if m else None,
    }


def valid_module(name, facts):
    """NAME.LoopValid: the valid calls as a statement input, the same body as LoopResources' Valid."""
    kpar = " (K : Nat)" if facts["bounded"] else ""
    return f"""import Verify.ShapeGeneric
import SolarTV.Composition

-- Generated by solar's tools/certify/loop_final.py; do not edit. The body is the one
-- tools/certify/loop_resources.py proves Loops for (LoopResources.Valid), so the two are one term.

namespace Certified.{name}.LoopValid

open SolarTV SolarTV.Machine SolidCore.Refinement

/-- The valid calls of the published loop statement. -/
def valid{kpar} (call : Call) : Prop :=
  {facts["valid_body"]}

end Certified.{name}.LoopValid
"""


# --- the join ----------------------------------------------------------------------------------


def hsub_proof(nargs, bounded):
    """The proof that every valid call is a call of the selector with clean argument words."""
    args = [f"a{i}" for i in range(nargs)]
    hs = [f"h{i}" for i in range(nargs)]
    pat = ", ".join(["σ", *args, *hs] + (["-"] if bounded else []) + ["rfl"])
    lst = "[" + ", ".join(args) + "]"
    if nargs:
        alts = " | ".join(["rfl"] * nargs)
        mem = (
            "fun x hx => by simp only [List.mem_cons, List.not_mem_nil, or_false] at hx; "
            f"rcases hx with {alts}{' <;> ' if nargs > 1 else '; '}assumption"
        )
        conj = hs[0] if nargs == 1 else "⟨" + ", ".join(hs) + "⟩"
        clean = (
            "by simp only [SolidCore.Refinement.Generic.argsClean, Bool.and_true, Bool.and_eq_true, "
            f"decide_eq_true_eq]; exact {conj}"
        )
    else:
        mem = "fun x hx => by simp at hx"
        clean = "rfl"
    return f"(by\n      rintro call ⟨{pat}⟩\n      exact ⟨σ, {lst}, rfl, {mem}, {clean}, rfl⟩)"


def value_proof(nargs, bounded):
    """The proof that every valid call carries no value."""
    pat = ", ".join(
        ["σ", *[f"a{i}" for i in range(nargs)], *(["-"] * nargs)]
        + (["-"] if bounded else [])
        + ["rfl"]
    )
    return f"(by rintro call ⟨{pat}⟩; rfl)"


def spec_terms(name, contract):
    """The bundle the statement names, and its allowed statuses and postcondition in the fixed frame."""
    B = f"Certified.{name}.gen.{contract}.Spec.{contract}"
    return (
        B,
        f"SolidCore.Refinement.Shapes.bundleAllowed {B} (fun _ => {FRAME})",
        (
            f"SolidCore.Refinement.Shapes.bundlePost {B} (fun _ => {FRAME}) "
            "SolidCore.Refinement.Shapes.idStep"
        ),
    )


def join_module(
    name,
    contract,
    fname,
    sel,
    nargs,
    stages,
    facts,
    route="erasure",
    spec=None,
    nonvac=False,
):
    """NAME.LoopJoin: final_of_generated (final_of_generatedW on the erasure-inferred route) on this
    run's modules; with SPEC (spec_step's record) the exported spec through the bridge instead
    (spec_join_module), importing NAME.LoopNonVac when NONVAC."""
    if isinstance(spec, dict):
        return spec_join_module(
            name, contract, fname, sel, nargs, stages, facts, route, spec, nonvac
        )
    inferred = route == "erasure-inferred"
    lemma = JOIN_W if inferred else JOIN_LEMMA
    route_import = f"import {ROUTE_MODULE}\n" if inferred else ""
    src_args = "erasedOf_eq\n    sigOf_eq"
    if route == "while-inferred":
        lemma, route_import = JOIN_WHILE, f"import {WHILE_MODULE}\n"
        src_args = f"whileOf_eq\n    sigOf_eq Certified.{name}.Loops.{fname}.src_seqs Certified.{name}.Loops.{fname}.src_valid"
    O = f"Certified.{name}.Loops.{fname}"
    R = f"Certified.{name}.LoopResources"
    V = f"Certified.{name}.LoopValid.valid"
    pi, ex = facts["per_iteration_gas"], facts["exit_gas"]
    if facts["bounded"]:
        binder, kt, kapp = f" {{K : Nat}} (hK : K ≤ {facts['Kmax']})", "K", " K"
        fit = "(by omega)"
    else:
        binder, kt, kapp = "", str(facts["M"]), ""
        fit = "(by decide)"
    ws = "[" + ", ".join(["256"] * nargs) + "]"
    return f"""import {JOIN_MODULE}
{route_import}import {name}.Loops.{fname}.Obl
import {name}.LoopResources
import {name}.LoopValid
import {name}.Proofs.Stages
import {name}.Source
import SolidCore.Refinement.KernelRfl

-- Generated by solar's tools/certify/loop_final.py from this run's generated modules; do not edit.
-- Untrusted composition: the kernel re-derives every claim.

/-!
# {contract}.{fname}: the loop's published theorem

`join` applies `{lemma}` to the generator's {route}-route obligations
(`{O}`), the backend stage of this run's export (`{stages["ns"]}.{stages["compose"]}`,
`{stages["first"]}` to `{stages["last"]}`) and the fixed-fuel premises on this run's bytes (`{R}`).
-/

namespace Certified.{name}.LoopJoin

/-- The loop function's ABI widths and word returns, as the kernel computes them. -/
def sigV : Option (List Nat × Bool) :=
  {JOIN_MODULE}.sigOf ({contract_term(name, contract)}) {sel}

kernel_rfl sig_eq sigV (some ({ws}, true))

theorem sigOf_eq :
    {JOIN_MODULE}.sigOf ({contract_term(name, contract)}) {sel} = some ({ws}, true) :=
  sig_eq

theorem join{binder} :
    ∃ c, {contract_term(name, contract)} = .ok c ∧
      SolidCore.Refinement.Shapes.Certified ({SOURCE_SEM} ({FUEL})) c
        {R}.code ({V}{kapp}) (fun _ _ => True) (fun _ _ => True)
        (fun _ => {kt} * {pi} + {ex}) :=
  {lemma} (by decide) {O}.{src_args} {O}.leaves {O}.seqs {O}.valid {O}.same
    ({stages["ns"]}.{stages["compose"]} {ORACLE}) (fun _ => trivial) {R}.check_ok
    {hsub_proof(nargs, facts["bounded"])}
    ({R}.inhabited{kapp}) (fun _ _ _ _ _ => trivial) (fun _ _ _ _ _ => trivial)
    (fun call hv => {R}.loops {ORACLE}{kapp} call hv) (by decide) (by decide) {fit}

/-- info: 'Certified.{name}.LoopJoin.join' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms join

end Certified.{name}.LoopJoin
"""


def spec_join_module(
    name, contract, fname, sel, nargs, stages, facts, route, spec, nonvac
):
    """NAME.LoopJoin with the exported spec: the bridge's LoopSpec (NAME.LoopSpec's F_loop_spec, of
    NAME.LoopCore's contract, the same AST as NAME.Source's by core_eq) as hspec, and the conclusion
    restated in the fixed frame (certified_ctx0)."""
    O = f"Certified.{name}.Loops.{fname}"
    R = f"Certified.{name}.LoopResources"
    V = f"Certified.{name}.LoopValid.valid"
    C = f"Certified.{name}.LoopCore"
    pi, ex = facts["per_iteration_gas"], facts["exit_gas"]
    if facts["bounded"]:
        binder, kt, kapp = f" {{K : Nat}} (hK : K ≤ {facts['Kmax']})", "K", " K"
        fit = "(by omega)"
    else:
        binder, kt, kapp = "", str(facts["M"]), ""
        fit = "(by decide)"
    ws = "[" + ", ".join(["256"] * nargs) + "]"
    B, allowed, post = spec_terms(name, contract)
    inferred = route == "erasure-inferred"
    if inferred:
        lemma, obl = JOIN_W_SPEC, f"{O}.seqs {O}.valid"
    else:
        lemma, obl = JOIN_OBL_SPEC, f"({OBL_OF_SEQS} {O}.seqs {O}.valid)"
    ct = contract_term(name, contract)
    nv_import = f"import {name}.LoopNonVac\n" if nonvac else ""
    return f"""import {JOIN_MODULE}
import {ROUTE_MODULE}
import {FRAME_MODULE}
import {name}.Loops.{fname}.Obl
import {name}.LoopResources
import {name}.LoopValid
import {name}.LoopSpec
{nv_import}import {name}.Proofs.Stages
import {name}.Source
import {name}.gen.{contract}.Spec
import SolidCore.Refinement.KernelRfl

-- Generated by solar's tools/certify/loop_final.py from this run's generated modules; do not edit.
-- Untrusted composition: the kernel re-derives every claim.

/-!
# {contract}.{fname}: the loop's published theorem, with the exported spec

`join` applies `{lemma}` to the generator's {route}-route obligations (`{O}`), the
backend stage of this run's export (`{stages["ns"]}.{stages["compose"]}`, `{stages["first"]}` to
`{stages["last"]}`), the fixed-fuel premises on this run's bytes (`{R}`) and the spec bridge's
`{spec["loop_spec"]}`: solar-verify's export `{B}` (this run's export of the source bytes,
`Certified.{name}.LoopSpec.spec_is_fresh`) as the allowed statuses and the postcondition on the loop
erasure's tree, in the fixed frame `{FRAME}` (sender 0x2000, value 0, timestamp 1, self 0x1000).
-/

namespace Certified.{name}.LoopJoin

/-- The loop function's ABI widths and word returns, as the kernel computes them. -/
def sigV : Option (List Nat × Bool) :=
  {JOIN_MODULE}.sigOf ({ct}) {sel}

kernel_rfl sig_eq sigV (some ({ws}, true))

theorem sigOf_eq :
    {JOIN_MODULE}.sigOf ({ct}) {sel} = some ({ws}, true) :=
  sig_eq

/-- The bridge's core module checks the same AST as the statement's source module: both are the
front end's translation of this run's bytes (the bridge's own copy: `source_is_fresh`). -/
theorem core_eq : {C}.contract? = {ct} := rfl

theorem join{binder} :
    ∃ c, {ct} = .ok c ∧
      SolidCore.Refinement.Shapes.Certified ({SOURCE_SEM} ({FUEL})) c
        {R}.code ({V}{kapp}) ({allowed})
        ({post})
        (fun _ => {kt} * {pi} + {ex}) := by
  have h0 : ∃ c, {ct} = .ok c ∧
      SolidCore.Refinement.Shapes.Certified ({SOURCE_SEM} ({FUEL})) c
        {R}.code ({V}{kapp}) (SolidCore.Refinement.Shapes.bundleAllowed {B} SolidCore.Refinement.Shapes.ctxOfCall)
        (SolidCore.Refinement.Shapes.bundlePost {B} SolidCore.Refinement.Shapes.ctxOfCall SolidCore.Refinement.Shapes.idStep)
        (fun _ => {kt} * {pi} + {ex}) ∧ SolidCore.Spec.TreeLoop.ExportedFor c {B} {sel} :=
    {lemma} (bd := {B}) (by decide) {O}.erasedOf_eq
      sigOf_eq {O}.leaves {obl} {O}.same
      (fun c hc => {spec["loop_spec"]} (core_eq.trans hc))
      ({stages["ns"]}.{stages["compose"]} {ORACLE}) (fun _ => trivial) {R}.check_ok
      {hsub_proof(nargs, facts["bounded"])}
      ({R}.inhabited{kapp})
      (fun call hv => {R}.loops {ORACLE}{kapp} call hv) (by decide) (by decide) {fit}
  obtain ⟨c, hc, h, -⟩ := h0
  exact ⟨c, hc, SolidCore.Spec.TreeLoop.certified_ctx0 {value_proof(nargs, facts["bounded"])} h⟩

/-- info: 'Certified.{name}.LoopJoin.join' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms join

end Certified.{name}.LoopJoin
"""


def witness_call(valid_body, nargs):
    """The non-vacuity witness's call: the valid calls' literal with zero arguments and the storage
    WITNESS_STORAGE."""
    m = re.search(r"call = (\{ calldata := .* \})$", valid_body)
    if not m or "storage := σ" not in m.group(1):
        raise Refused(
            "nonvacuous", "the valid calls have no call literal to instantiate"
        )
    lit = m.group(1)
    for i in range(nargs):
        lit = re.sub(rf"\ba{i}\b", "0", lit)
    return lit.replace("storage := σ", f"storage := {WITNESS_STORAGE}")


def nonvac_module(name, contract, sel, nargs, facts, index):
    """NAME.LoopNonVac: the exported postcondition, in the fixed frame, rejects a result of a valid
    call at every iteration bound: the loop function's call with zero arguments from
    WITNESS_STORAGE, finishing with a success that writes 1 to every slot. INDEX is the loop
    function's spec in the bundle. Untrusted: Lean checks it, and the driver declares the vacuity
    instead when it does not check."""
    B, _, post = spec_terms(name, contract)
    V = f"Certified.{name}.LoopValid.valid"
    call = witness_call(facts["valid_body"], nargs)
    zeros = ", ".join(["0"] * nargs)
    if facts["bounded"]:
        binder, kapp = f" {{K : Nat}} (_hK : K ≤ {facts['Kmax']})", " K"
        doc = "every iteration bound `K`"
    else:
        binder, kapp, doc = "", "", "the loop's valid calls"
    unfold = (
        f"Bundle.satAll, Bundle.sat, {B}, eval, slotOf, mkEnv, Env.cur, BinOp.apply, "
        "Ty.ofBits, SolidCore.Refinement.slotValue"
    )
    return f"""import {name}.gen.{contract}.Spec
import {name}.LoopValid
import Verify.Spec.TreeLoop
import Verify.ShapeBundle

-- Generated by solar's tools/certify/loop_final.py; do not edit. Untrusted: the kernel checks it.

/-!
# {contract}: the exported postcondition rejects something on the loop's valid calls

`post_rejects`: for {doc}, the call with zero arguments from the storage {WITNESS_STORAGE} is valid,
and a success that writes 1 to every slot does not meet `{B}`'s postcondition in the fixed frame
`{FRAME}`: the specification's half of the published statement's NonVacuous.
-/

namespace Certified.{name}.LoopNonVac

open SolarTV SolidCore.Refinement SolidCore.Spec

theorem post_rejects{binder} :
    ∃ (call : SolarTV.Call) (obs : SolarTV.Observable), ({V}{kapp}) call ∧
      ¬ ({post}) call obs := by
  refine ⟨{call}, ⟨true, [], [], fun _ => 1⟩, ⟨{WITNESS_STORAGE}, {zeros},
    by and_intros <;> first | decide | exact Nat.zero_le _ | rfl⟩, fun h => ?_⟩
  have hpre : Pre {B} ({B}.spec.specs[{index}]'(by decide)) {FRAME}
      ⟨SolidCore.Refinement.slotValue {WITNESS_STORAGE}, Ghost.zero, 1⟩ {call} [{zeros}] := by
    refine ⟨⟨?_, ?_, ?_⟩, rfl, ?_, ?_, ?_⟩
    · decide
    · exact SolidCore.Refinement.Shapes.selectorOf_calldata (by decide) [{zeros}] 0 {WITNESS_STORAGE}
    · have := SolidCore.Spec.words_calldata {sel} (args := [{zeros}]) (by simp)
      dsimp only
      rw [this]; decide
    · simp [{FRAME}]
    · simp [{unfold}]
    · simp [{unfold}]
  obtain ⟨rets, -, hens, -⟩ := (h _ (List.getElem_mem _) _ _ hpre).1 rfl
  simp [{unfold}] at hens
  all_goals (revert hens; decide)

/-- info: 'Certified.{name}.LoopNonVac.post_rejects' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms post_rejects

end Certified.{name}.LoopNonVac
"""


# --- the spec step: export, core module, bridge ----------------------------------------------------


def selector_entries(ast, contract):
    """(signature, selector) of the contract's external functions and public getters with
    elementary parameters, for the core module's selector table (untrusted: the kernel checks each
    entry, KernelTable.eq_tableOr)."""
    elem = re.compile(r"^(u?int\d*|address|bool|bytes\d+)$")
    cdef = next(
        c
        for c in ast.get("nodes", [])
        if c.get("nodeType") == "ContractDefinition" and c.get("name") == contract
    )
    out = []
    for n in cdef.get("nodes", []):
        sel = n.get("functionSelector")
        if not sel:
            continue
        if n.get("nodeType") == "FunctionDefinition":
            ts = [
                p["typeDescriptions"]["typeString"]
                for p in n["parameters"]["parameters"]
            ]
        elif (
            n.get("nodeType") == "VariableDeclaration"
            and n.get("typeName", {}).get("nodeType") == "ElementaryTypeName"
        ):
            ts = []
        else:
            continue
        if all(elem.match(t) for t in ts):
            out.append((f"{n['name']}({','.join(ts)})", int(sel, 16)))
    return out


def core_module(name, contract, body, rel_src, entries):
    """NAME.LoopCore: the front end's AST of the bytes at REL_SRC (its importedSourceName that
    checkout-relative path, which spec_connect.py's provenance gate compares with the export's
    manifest), the checked contract, the statement fuel and the selector table."""
    import generic

    lines = body.splitlines()
    hits = [i for i, l in enumerate(lines) if l.startswith("def importedSourceName :")]
    if len(hits) != 1:
        raise Refused("spec", "the front end's AST has no single importedSourceName")
    lines[hits[0]] = f"def importedSourceName : String := {json.dumps(rel_src)}"
    table = generic.kernel_table(
        *generic.SELECTOR_TABLE, [(sig, hex(v)) for sig, v in entries]
    ).replace("\u00a7", "\u0060")
    ast_text = "\n".join(lines)
    return f"""-- Generated by solar's tools/certify/loop_final.py with scripts/solc_ast_to_lean_source.py; do not edit.
-- Untrusted: spec_connect.py's source_is_fresh checks this AST against its own translation of the bytes.

import SolidCore.Solidity.Checked
import SolidCore.Refinement.KernelRfl
import SolidCore.Refinement.KernelTable

open SolidCore.Solidity

{ast_text}

namespace Certified.{name}.LoopCore

open SolidCore.Refinement SolidCore.Solidity SolidCore.Solidity.Source

/-- The statement fuel of the loop route (loop_cuts.LOOP_FUEL). -/
def fuel : Nat := {FUEL}

/-- SolidCore's checked `{contract}`. -/
def contract? : Except TypeCheck.TypeError TypeCheck.CheckedContract :=
  TypeCheck.CheckedInput.contract Certified.{name}.LoopCore.importedSourceUnit {json.dumps(contract)}

{table}
end Certified.{name}.LoopCore
"""


def spec_exporter(run, pin_file=None):
    """(path, provenance) of the pinned solar-verify (VERIFY_PIN): an immutable copy of the binary
    (the pin's binary, which no build writes) whose SHA-256 is the pinned one, built from the
    pinned commit, which must contain VERIFY_FIXES (checked in the pin's tree, or
    CERTIFY_SPEC_VERIFY_TREE). Nothing is built here: debug builds are not reproducible across
    machines and caches, so the pin is the artifact's digest, as spec_connect.py's
    --solar-verify-sha256 is. Raises Refused("exporter", why) otherwise: the loop route never exports
    a spec with another solar-verify."""
    try:
        pin = json.loads(pathlib.Path(pin_file or VERIFY_PIN).read_text())
        commit, want, tree, exe = pin["commit"], pin["sha256"], pin["tree"], pin["binary"]
    except (OSError, ValueError, KeyError, TypeError) as e:
        raise Refused("exporter", f"no usable solar-verify pin: {e}") from None
    exe = pathlib.Path(exe)
    if not exe.is_file():
        raise Refused("exporter", f"the pinned solar-verify {exe} is missing")
    got = sha(exe)
    if got != want:
        raise Refused(
            "exporter", f"the pinned solar-verify {exe} is {got}, not the pinned {want}"
        )
    tree = pathlib.Path(os.environ.get("CERTIFY_SPEC_VERIFY_TREE") or tree)
    for c in VERIFY_FIXES:
        rc, _, err = run(["git", "-C", str(tree), "merge-base", "--is-ancestor", c, commit])
        if rc != 0:
            raise Refused(
                "exporter",
                f"the pinned solar-verify commit {commit} lacks {c} ({err.strip()[:120]})",
            )
    return exe, {
        "branch": pin.get("branch"),
        "commit": commit,
        "binary": str(exe),
        "sha256": got,
        "fixes": list(VERIFY_FIXES),
    }


def receipt_problem(path, *, module, contract, source, source_sha, exporter_sha, module_file):
    """Why PATH (spec_connect.py's OUT.lean.receipt.json) is not the receipt of this run's spec
    module, or None: it must exist and parse, name MODULE, CONTRACT and the copied source SOURCE,
    record SOURCE_SHA (the run's source digest) as the bytes the export and the AST read, the
    pinned exporter EXPORTER_SHA, and the SHA-256 of MODULE_FILE as it is now; and MODULE_FILE
    must state the same source, exported-bundle and tool digests (the defs its receipt theorem
    and spec_source refer to)."""
    path = pathlib.Path(path)
    if not path.is_file():
        return f"no receipt {path.name}: spec_connect.py wrote none"
    try:
        r = json.loads(path.read_text())
    except ValueError as e:
        return f"the receipt {path.name} does not parse: {e}"
    if not isinstance(r, dict) or any(not isinstance(r.get(k), str) for k in RECEIPT_KEYS):
        return f"the receipt {path.name} lacks one of {', '.join(RECEIPT_KEYS)}"
    want = {
        "module": module,
        "contract": contract,
        "source": source,
        "sourceSha256": source_sha,
        "exporterSha256": exporter_sha,
    }
    for k, v in want.items():
        if r[k] != v:
            return f"the receipt's {k} is {r[k]!r}, not this run's {v!r}"
    mf = pathlib.Path(module_file)
    if not mf.is_file():
        return f"no spec module {mf.name}"
    if sha(mf) != r["moduleSha256"]:
        return f"{mf.name} is not the module the receipt recorded (moduleSha256)"
    text = mf.read_text()
    for k in (
        "sourceSha256",
        "exportedSha256",
        "exporterSha256",
        "astToolSha256",
        "astToolCacheSha256",
        "solcSha256",
    ):
        if f'def {k} : String := "{r[k]}"' not in text:
            return f"{mf.name}'s {k} is not the receipt's"
    return None


def spec_step(
    *, run, lean_dir, name, contract, fname, ast, src, verify, lean_cmd, build, env=None
):
    """The exported spec of the loop function on its erased tree (the O6 bridge), in the clone
    LEAN_DIR: the source's bytes as Certified/NAME/NAME.sol, solar-verify's export of them
    (NAME.gen.C.Spec), NAME.LoopCore, then spec_connect.py --loops --only C.FNAME, which writes and
    Lean-checks NAME.LoopSpec (with its receipt). BUILD(modules) builds Lean modules (lake in the
    driver) and returns None or why not; LEAN_CMD is spec_connect.py's --lean. Returns None when the
    export has no spec of the loop function, else {"loop_spec", "index", "verify_sha256",
    "receipt"}; raises Refused("spec", why) when a step refuses, and Refused("receipt", why) when
    spec_connect.py's receipt is missing or is not of this run's source bytes, exporter and module
    (receipt_problem)."""
    import hashlib

    import spec_connect as sc

    lean_dir = pathlib.Path(lean_dir)
    d = lean_dir / "Certified" / name
    rel = f"Certified/{name}/{name}.sol"
    data = pathlib.Path(src).read_bytes()
    (lean_dir / rel).write_bytes(data)
    # A receipt left by an earlier run is never this run's.
    (d / "LoopSpec.lean.receipt.json").unlink(missing_ok=True)
    spec_ns = f"Certified.{name}.gen.{contract}.Spec"
    out = d / "gen" / contract / "Spec.lean"
    out.parent.mkdir(parents=True, exist_ok=True)
    rc, _, err = run(
        [str(verify), "--emit-lean-spec", str(out), "--lean-spec-module", spec_ns, rel],
        cwd=lean_dir,
        env=env,
    )
    if rc != 0 or not out.is_file():
        raise Refused(
            "spec", f"solar-verify cannot export the spec: {err.strip()[-300:]}"
        )
    if f'name := "{contract}.{fname}"' not in out.read_text():
        return None
    solc = sc.pinned_solc(str(lean_dir))
    if isinstance(solc, str):
        raise Refused("spec", solc)
    rc, body, err = run(
        [
            sys.executable,
            str(lean_dir / "scripts/solc_ast_to_lean_source.py"),
            rel,
            "--contract",
            contract,
            "--solc",
            solc[0],
            "--namespace",
            f"Certified.{name}.LoopCore",
            "--body-only",
        ],
        cwd=lean_dir,
        env=env,
    )
    if rc != 0:
        raise Refused("spec", f"the front end refuses the source: {err.strip()[-300:]}")
    (d / "LoopCore.lean").write_text(
        core_module(name, contract, body, rel, selector_entries(ast, contract))
    )
    why = build([f"{name}.LoopCore", f"{name}.gen.{contract}.Spec"])
    if why:
        raise Refused("spec", f"the core or spec module does not build: {why}")
    vsha = hashlib.sha256(pathlib.Path(verify).read_bytes()).hexdigest()
    rc, outp, err = run(
        [
            sys.executable,
            str(HERE / "spec_connect.py"),
            "--core-module",
            f"{name}.LoopCore",
            "--namespace",
            f"Certified.{name}.LoopCore",
            "--spec-module",
            f"{name}.gen.{contract}.Spec",
            "--bundle",
            f"{spec_ns}.{contract}",
            "--module-name",
            f"Certified.{name}.LoopSpec",
            "--out",
            str(d / "LoopSpec.lean"),
            "--lean",
            lean_cmd,
            "--cwd",
            str(lean_dir),
            "--loops",
            "--only",
            f"{contract}.{fname}",
            "--solar-verify",
            str(verify),
            "--solar-verify-sha256",
            vsha,
            "--solc",
            solc[0],
        ],
        env=env,
    )
    thm = [l for l in outp.split() if l.endswith("_loop_spec")]
    if rc != 0 or len(thm) != 1:
        raise Refused(
            "spec", f"spec_connect.py --loops refuses: {(err or outp).strip()[-400:]}"
        )
    fid = thm[0].rsplit(".", 1)[1][: -len("_loop_spec")]
    m = re.search(
        rf"^def {re.escape(fid)}_spec : FnSpec := spec\.spec\.specs\[(\d+)\]",
        (d / "LoopSpec.lean").read_text(),
        re.MULTILINE,
    )
    if not m:
        raise Refused(
            "spec", "the bridge's module names no spec index for the loop function"
        )
    receipt = d / "LoopSpec.lean.receipt.json"
    why = receipt_problem(
        receipt,
        module=f"Certified.{name}.LoopSpec",
        contract=contract,
        source=rel,
        source_sha=hashlib.sha256(data).hexdigest(),
        exporter_sha=vsha,
        module_file=d / "LoopSpec.lean",
    )
    if why:
        raise Refused("receipt", why)
    return {
        "loop_spec": thm[0],
        "index": int(m.group(1)),
        "verify_sha256": vsha,
        "receipt": json.loads(receipt.read_text()),
    }


def shape(name, contract, fname, sig, facts, spec=None, nonvac=False):
    """shape.json for final.py. SPEC: spec_step's record (the exported spec in the fixed frame), a
    refusal string (True, declared vacuous with the refusal) or None (no spec of the loop function).
    NONVAC: True when NAME.LoopNonVac checked, else why there is no witness."""
    pi, ex = facts["per_iteration_gas"], facts["exit_gas"]
    bounded = facts["bounded"]
    kt = "K" if bounded else str(facts["M"])
    lim = (
        f"the loop's limit at most an iteration bound K ≤ {facts['Kmax']}"
        if bounded
        else f"the loop's literal limit ({facts['M']} iterations)"
    )
    s = {
        "kind": "certified",
        "contract": contract_term(name, contract),
        "fuel": FUEL,
        "source_sem": SOURCE_SEM,
        "valid": f"Certified.{name}.LoopValid.valid" + (" K" if bounded else ""),
        "allowed": "fun _ _ => True",
        "post": "fun _ _ => True",
        "vacuity": (
            "no specification: the allowed statuses and the postcondition are True, so the "
            "statement is translation (equivalence), termination and resources only; "
            "solar-verify's export of the loop function's spec is not proved on the loop's tree (O6)"
        ),
        "gas": f"fun _ => {kt} * {pi} + {ex}",
        "env": [],
        "spec": "none proved: True, declared vacuous; the source's NatSpec is not bound by this statement",
        "spec_modules": [f"{name}.Source", f"{name}.LoopValid"],
        "proof_module": f"{name}.LoopJoin",
        "proof": f"Certified.{name}.LoopJoin.join",
        "export_ns": export_ns.of_run(name, contract),
        "excluded": [
            (
                "runtime only: the statement is about the runtime bytecode run from any storage "
                "(solidCoreRuntime), not the deployed contract; deployment and the constructor are not "
                "covered (no lifecycle composition)"
            ),
            (
                f"calls: {sig} with uint256 argument words, no value, any storage, and {lim}; the "
                "contract's other functions and other call data are outside the statement"
            ),
            (
                f"gas: the bound is {kt} * {pi} + {ex} (the loop checker's fixed fuel); below it the "
                "bytecode may fail without effects, which equivalence admits"
            ),
            "spec: none (True); see vacuity",
            "environment: one fixed frame (Shapes.oracle), as the backend stage and Loops are stated",
            (
                "source model trusted: SolidCore's checked contract (the solc AST through "
                "scripts/solc_ast_to_lean_source.py and the type checker) is the meaning of the source "
                "(frontend trust boundary); source_ast binds that AST to this run's source"
            ),
        ],
        "source_ast": {
            "file": f"Certified/{name}/Source.lean",
            "namespace": f"Certified.{name}.Src",
            "contract": contract,
        },
        "loop": {
            "function": fname,
            "join": JOIN_LEMMA,
            "route": "erasure",
            "E": facts["E"],
            "B": facts["B"],
            "X": facts["X"],
            "final_evm_ir": facts["final_evm_ir"],
        },
    }
    if bounded:
        s["iteration_bound"] = {"var": "K", "max": facts["Kmax"]}
    if spec is None:
        return s
    if isinstance(spec, str):
        s["vacuity"] += f"; the spec bridge refused: {spec}"
        s["spec"] = f"none proved: the spec bridge refused ({spec})"
        return s
    B, _, _ = spec_terms(name, contract)
    for k in ("allowed", "post", "vacuity"):
        s.pop(k)
    s["allowed_export"] = True
    s["spec_export"] = {
        "ctx": f"fun _ => {FRAME}",
        "step": "SolidCore.Refinement.Shapes.idStep",
    }
    s["spec"] = (
        f"solar-verify's export of the source's NatSpec in this run ({B}, this run's export of the "
        f"source bytes: Certified.{name}.LoopSpec.spec_is_fresh), proved on the loop erasure's tree by "
        f"the O6 bridge ({spec['loop_spec']}), in the fixed frame {FRAME}"
    )
    s["spec_modules"] = [*s["spec_modules"], "Verify.Spec.TreeLoop"]
    s["excluded"] = [
        e for e in s["excluded"] if not e.startswith(("spec:", "environment:"))
    ] + [
        (
            f"environment: the fixed frame {FRAME} (sender 0x2000, value 0, timestamp 1, self "
            "0x1000), named in the statement as the spec's context, and the backend's one oracle "
            "(Shapes.oracle); other callers, values and blocks are outside the statement until "
            "Phase 2's caller generalization is wired"
        ),
    ]
    if nonvac is True:
        s["nonvacuous"] = f"Certified.{name}.LoopNonVac.post_rejects"
        s["allowed_vacuity"] = (
            f"{sig}'s exported spec has no @custom:succeeds-if (the loop bridge takes none), so "
            "every revert is an allowed status"
        )
    else:
        s["vacuity"] = (
            "the exported postcondition is not shown to reject a result of a valid call: "
            + (nonvac or "no witness")
        )
    return s


def compose_of(stages_text, facts):
    """The export's composed backend stage (certify.py's X_mir_to_evmir), from stage0 to the
    resources' final EVM IR. A looping export proves its stages through cut points, so only the
    composition theorem is read."""
    ns = re.search(r"^namespace (SolarTV\.Milestone\.\w+)", stages_text, re.MULTILINE)
    comp = re.search(
        r"theorem (\w+_mir_to_evmir) \(o : Oracle\) : Stage \(mirSem o\) \(evmirSem o\) (\w+) (\w+) anyCall",
        stages_text,
    )
    if not (ns and comp):
        raise Refused(
            "stages", "Stages.lean has no composed stage from the first lowered MIR"
        )
    st = {
        "ns": ns.group(1),
        "compose": comp.group(1),
        "first": comp.group(2),
        "last": comp.group(3),
    }
    if st["first"] != "stage0":
        raise Refused(
            "stages", f"the backend composition starts at {st['first']}, not stage0"
        )
    if st["last"] != facts["final_evm_ir"]:
        raise Refused(
            "stages",
            f"the backend composition ends at {st['last']}, not the loop's final EVM IR "
            f"{facts['final_evm_ir']}",
        )
    return st


def sha(p):
    return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()


def write(
    root,
    name,
    record,
    *,
    contract,
    sig,
    sel,
    params,
    gen_dir,
    plain,
    stages_text,
    spec=None,
    nonvac=False,
):
    """Write NAME.LoopResources, NAME.LoopValid, NAME.LoopJoin and NAME/shape.json under ROOT (the
    Certified source directory) for the record's loop function; with SPEC (spec_step's record) also
    NAME.LoopNonVac, which the join imports when NONVAC is True (shape's SPEC and NONVAC). Returns
    (shape, files)."""
    fname = loop_function(record)
    route = loop_route(record, fname)
    rtext, facts = resources(name, contract, gen_dir, plain, sel, len(params), params)
    st = compose_of(stages_text, facts)
    d = pathlib.Path(root) / name
    d.mkdir(parents=True, exist_ok=True)
    files = {
        "LoopResources.lean": rtext,
        "LoopValid.lean": valid_module(name, facts),
        "LoopJoin.lean": join_module(
            name,
            contract,
            fname,
            sel,
            len(params),
            st,
            facts,
            route,
            spec,
            nonvac is True,
        ),
    }
    if isinstance(spec, dict):
        try:
            files["LoopNonVac.lean"] = nonvac_module(
                name, contract, sel, len(params), facts, spec["index"]
            )
        except Refused as e:
            if nonvac is True:
                raise
            record.setdefault("loop_final_nonvac", e.why)
    for f, t in files.items():
        if not (d / f).is_file() or (d / f).read_text() != t:
            (d / f).write_text(t)
    sh = shape(name, contract, fname, sig, facts, spec, nonvac)
    (d / "shape.json").write_text(json.dumps(sh, indent=2, ensure_ascii=False) + "\n")
    out = [d / f for f in files] + [d / "shape.json"]
    return sh, [{"path": str(p), "sha256": sha(p)} for p in out]


def replay_closure(run, lean_dir, name, env):
    """The kernel replay of NAME.Final's constant closure (tools/certify/replay_closure.lean), as
    scripts/certify's step 6b runs it: an olean records declarations the kernel need not have
    checked, so check_final's reading of the built environment is only as good as this replay.
    CERTIFY_REPLAY_SHARDS=N (default 1) shards, run one after another with the toolchain's lean
    under lake env in the clone LEAN_DIR, then tools/certify/replay.py closure accounts for them.
    Returns the accounting's receipt; raises Refused("replay", why) otherwise."""
    shards = os.environ.get("CERTIFY_REPLAY_SHARDS", "1")
    if not re.fullmatch(r"[1-9][0-9]*", shards):
        raise Refused("replay", f"CERTIFY_REPLAY_SHARDS is not a positive number: {shards}")
    n = int(shards)
    lean_dir = pathlib.Path(lean_dir)
    rc, out, err = run(
        [*loop_cuts.LAKE, "env", "lean", "--print-prefix"], cwd=lean_dir, env=env
    )
    toolchain = pathlib.Path(out.strip().splitlines()[-1]) if rc == 0 and out.strip() else None
    lean = toolchain / "bin" / "lean" if toolchain else None
    if lean is None or not lean.is_file():
        raise Refused("replay", f"cannot find the toolchain's lean: {(out + err).strip()[-200:]}")
    thm, stmt = f"Certified.{name}.Final.final", f"Certified.{name}.Final.Statement"
    prefix = lean_dir / ".lake" / f"certify-{name}.loop-closure"
    prefix.parent.mkdir(parents=True, exist_ok=True)
    for f in prefix.parent.glob(prefix.name + ".*"):
        f.unlink()
    for k in range(n):
        base = f"{prefix}.{k}-of-{n}"
        rc, out, err = run(
            [*loop_cuts.LAKE, "env", str(lean), "-M", "12000", "--run"]
            + [str(HERE / "replay_closure.lean"), f"{name}.Final", thm, stmt, base]
            + [str(k), str(n)],
            cwd=lean_dir,
            env=env,
        )
        pathlib.Path(base + ".out").write_text(out + err)
        pathlib.Path(base + ".status").write_text(f"{rc}\n")
    receipt = pathlib.Path(f"{prefix}.receipt.json")
    rc, out, err = run(
        [sys.executable, str(HERE / "replay.py"), "closure", str(prefix), str(n)]
        + [str(receipt), "--theorem", thm, "--statement", stmt, "--toolchain", str(toolchain)]
        + ["--checker", str(lean), "--tool", str(HERE / "replay_closure.lean")]
    )
    if rc != 0 or not receipt.is_file():
        raise Refused(
            "replay",
            f"the kernel replay of {name}.Final's closure is not accounted for: "
            f"{(err or out).strip()[-300:]}",
        )
    r = json.loads(receipt.read_text())
    return {k: r.get(k) for k in ("closure", "replayed_count", "trusted_count", "axioms")} | {
        "shards": n,
        "receipt": str(receipt),
        "receipt_sha256": sha(receipt),
    }


def acceptance_problem(record, src, lean_dir, name, contract, exporter_sha):
    """Why the run's checked final may not be accepted with its spec module, or None: the run's
    source digest (record["source"]["sha256"], which the driver records before any step) must be
    the SHA-256 of SRC now and of the copy Certified/NAME/NAME.sol in LEAN_DIR, and the receipt
    must be of those bytes, the pinned exporter EXPORTER_SHA and NAME.LoopSpec as it is now
    (receipt_problem)."""
    d = pathlib.Path(lean_dir) / "Certified" / name
    rel = f"Certified/{name}/{name}.sol"
    have = sha(src)
    run_sha = ((record or {}).get("source") or {}).get("sha256")
    if not isinstance(run_sha, str):
        return "the run recorded no source digest (record source.sha256) to bind the receipt to"
    if run_sha != have:
        return f"the source is not the run's: SHA-256 {have}, the run recorded {run_sha}"
    copy = pathlib.Path(lean_dir) / rel
    if not copy.is_file() or sha(copy) != have:
        return f"{rel} is not the run's source bytes"
    return receipt_problem(
        d / "LoopSpec.lean.receipt.json",
        module=f"Certified.{name}.LoopSpec",
        contract=contract,
        source=rel,
        source_sha=have,
        exporter_sha=exporter_sha,
        module_file=d / "LoopSpec.lean",
    )


def reason(e, where=""):
    """The run's BLOCKED reason for a refusal."""
    return {
        "feature": "loop",
        "where": where,
        "detail": f"loop final: {e.step}: {e.why}",
    }


# --- the driver's hook ---------------------------------------------------------------------------


def attach(
    record,
    *,
    run,
    name,
    contract,
    ast,
    solar,
    src,
    work,
    lean_dir,
    lake_env,
    plain,
    log,
):
    """The loop branch of tools/certify/generic.py, after the loop cut hook (loop_cuts.hook) built the
    obligations in the run's clone LEAN_DIR: export this run's stages (solar -Zemit-lean-stages) and
    their backend proofs (certify.py, through cut points), write the loop's modules and shape.json,
    the published statement (final.py), build NAME.Final with lake and check it with check_final.lean
    and this run's bytes PLAIN (the fresh-byte gate). Records record["loop_final"]; returns [] when the
    final theorem checked, else the run's BLOCKED reason (a refusal names its step)."""
    import shutil

    import generic

    lean_dir = pathlib.Path(lean_dir)
    d = lean_dir / "Certified" / name
    try:
        fname = loop_function(record)
        cdef = next(
            c
            for c in ast.get("nodes", [])
            if c.get("nodeType") == "ContractDefinition" and c.get("name") == contract
        )
        fdef = next(
            f
            for f in cdef.get("nodes", [])
            if f.get("nodeType") == "FunctionDefinition" and f.get("name") == fname
        )
        params = [
            p["typeDescriptions"]["typeString"]
            for p in fdef["parameters"]["parameters"]
        ]
        sel = int(fdef["functionSelector"], 16)
        tmp = pathlib.Path(work) / "loop-final"
        shutil.rmtree(tmp, ignore_errors=True)
        tmp.mkdir(parents=True)
        fresh, err = generic.export_stages(solar, src, contract, tmp)
        if fresh != plain:
            raise Refused(
                "export", "the stage export's runtime bytes are not this run's"
            )
        gen = tmp / "gen" / contract
        g = d / "gen" / contract
        old = g / "Stage0.lean"
        if old.is_file() and old.read_text() != (gen / "Stage0.lean").read_text():
            raise Refused(
                "export",
                "the obligations were built on another Stage0 than this export's",
            )
        g.mkdir(parents=True, exist_ok=True)
        for f in gen.glob("*.lean"):
            shutil.copy(f, g / f.name)
        rc, _, err = run(
            [
                sys.executable,
                str(HERE / "certify.py"),
                str(gen),
                "--gen-module",
                f"{name}.gen.{contract}",
                "--proof-dir",
                str(d / "Proofs"),
                "--proof-module",
                f"{name}.Proofs",
                "--namespace",
                "SolarTV.Milestone.X",
                "--label",
                name,
                "--loops",
                "0",
                "--heartbeats",
                "2000000",
            ]
        )
        if rc != 0:
            raise Refused(
                "stages",
                f"certify.py cannot write the stage proofs: {err.strip()[-300:]}",
            )

        def build(mods):
            rc, out, err = run(
                [*loop_cuts.LAKE, "build", *mods], cwd=lean_dir, env=lake_env
            )
            return None if rc == 0 else (out + err).strip()[-300:]

        # The exported spec through the bridge, with the pinned solar-verify; a refusal of the
        # bridge keeps True and is recorded, but a missing or foreign exporter or receipt refuses
        # the final.
        verify, verify_pin = spec_exporter(run)
        try:
            spec = spec_step(
                run=lambda c, cwd=None, env=None: run(c, cwd=cwd, env=env),
                lean_dir=lean_dir,
                name=name,
                contract=contract,
                fname=fname,
                ast=ast,
                src=src,
                verify=verify,
                lean_cmd=" ".join([*loop_cuts.LAKE, "env", "lean"]),
                build=build,
                env=lake_env,
            )
        except Refused as e:
            if e.step != "spec":
                raise
            spec = e.why
        common = {
            "contract": contract,
            "sig": f"{fname}({','.join(params)})",
            "sel": sel,
            "params": params,
            "gen_dir": g,
            "plain": plain,
            "stages_text": (d / "Proofs" / "Stages.lean").read_text(),
            "spec": spec,
        }
        nonvac = False
        if isinstance(spec, dict):
            write(lean_dir / "Certified", name, record, **common, nonvac=False)
            why = build([f"{name}.LoopNonVac"])
            nonvac = True if why is None else f"the witness does not check: {why}"
        sh, files = write(lean_dir / "Certified", name, record, **common, nonvac=nonvac)
        rc, out, err = run(
            [
                sys.executable,
                str(HERE / "final.py"),
                str(d / "shape.json"),
                "--name",
                name,
                "--contract",
                contract,
                "--hex",
                plain,
                "--out",
                str(d / "Final.lean"),
                "--statement-out",
                str(d / "Statement.lean"),
                "--record",
                str(d / "final.json"),
            ]
        )
        if rc != 0:
            raise Refused("statement", f"final.py refuses: {err.strip()[-300:]}")
        rc, out, err = run(
            [*loop_cuts.LAKE, "build", f"{name}.Final"], cwd=lean_dir, env=lake_env
        )
        if rc != 0:
            raise Refused(
                "build", f"lake cannot build {name}.Final: {(out + err).strip()[-300:]}"
            )
        rc, out, err = run(
            [
                *loop_cuts.LAKE,
                "env",
                "lean",
                "--run",
                str(HERE / "check_final.lean"),
                name,
                sh["proof_module"],
                plain,
            ],
            cwd=lean_dir,
            env=lake_env,
        )
        if rc != 0:
            raise Refused("check", f"check_final refuses: {(out + err).strip()[-300:]}")
        # The kernel replays the final's whole constant closure before it is accepted.
        replayed = replay_closure(run, lean_dir, name, lake_env)
        log(f"loop final: kernel replay of the closure: {replayed['closure']} constants")
        # Acceptance: a final that states the exported spec needs its receipt, of this run's source
        # bytes (the digest the run recorded, the file now and the copy the export read), of the
        # pinned exporter, and of the spec module as it is now.
        accepted = None
        if isinstance(spec, dict):
            accepted = acceptance_problem(
                record, src, lean_dir, name, contract, verify_pin["sha256"]
            )
            if accepted:
                raise Refused("receipt", accepted)
            accepted = json.loads((d / "LoopSpec.lean.receipt.json").read_text())
        axioms = re.search(r"axioms \[([^\]]*)\]", out)
        record["loop_final"] = {
            "function": fname,
            "route": loop_route(record, fname),
            "spec": spec,
            "spec_exporter": verify_pin,
            "receipt": accepted,
            "replay": replayed,
            "nonvacuous": nonvac,
            "files": files,
            "final": json.loads((d / "final.json").read_text()),
            "axioms": axioms.group(1).split(", ") if axioms else None,
        }
        log(f"loop final {fname}: Certified.{name}.Final.final checked")
        return []
    except Refused as e:
        record["loop_final"] = {"refused": {"step": e.step, "why": e.why}}
        log(f"loop final: REFUSED at {e.step}: {e.why}")
        return [reason(e)]
    except (OSError, ValueError, KeyError, StopIteration) as e:
        record["loop_final"] = {
            "refused": {"step": "internal", "why": f"{type(e).__name__}: {e}"}
        }
        return [reason(Refused("internal", f"{type(e).__name__}: {e}"))]
