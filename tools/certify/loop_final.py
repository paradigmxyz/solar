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
revert; the statement declares that (allowed_vacuity) and is not the strongest status.

A part states True only by a checked no-spec determination (spec_gate, review 206): the solc AST
has no NatSpec tag that may carry a spec on the function or one that applies to every function
(spec_inventory), and NAME.LoopSpecInventory proves by decide over this run's export (of this run's source
bytes) that the bundle has no invariant and no entry with the function's name or selector, beside
NAME.LoopSpecInventory.inventory, the export's whole spec list by name and selector. Such a
part is labelled translation only in shape.json, final.json and record loop_final.parts. A function
with a spec, or with a tag that may carry one, that the export or the bridge does not prove refuses
the whole final at step "spec": it is never a True part.

Several loop functions (Refused "functions" before): each function F gets its own part,
NAME.Loop.F.{Resources,Valid,Join,NonVac}, with loop_resources.py's numbers restricted to the loop
F's calls reach (its selector), its own iteration bound K ≤ Kmax and its own valid calls; one
spec_connect.py run connects every function's exported spec in NAME.LoopSpec (a function it refuses
is dropped and the rest are connected again, then spec_gate refuses the final); NAME.LoopJoin imports every
join, and shape.json's parts make final.py state the conjunction, one conjunct per loop function.

The while route ("while-inferred", loop_cuts.py step 3w): the join applies final_of_whileW (no spec)
or final_of_whileW_spec, whose spec premise is the bridge's WhileSpec (spec_connect.py --loops
--while F=NAME.Loops.F.Obl:Certified.NAME.Loops.F: the export's spec on the obligation module's
eraseWhile tree Te, whileSpec_of_check), restated in the fixed frame as on the other routes.

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
"route": a function on the summary route, or the hook did not complete; "functions": no loop
function; "resources": loop_resources.py's Unsupported; "abi": a parameter that is not
uint256; "stages": the export's stage proofs do not compose from stage0 to the resources' final
EVM IR; "exporter": no pinned solar-verify; "spec": the export or the bridge does not prove a function's
spec (or the source may carry one the export lacks); "inventory": NAME.LoopSpecInventory does not
parse or check; "functions" also: a source loop outside every loop function with obligations;
"receipt": the receipt is missing or not this run's;
"replay": the kernel replay of the closure fails or is not accounted for.
"""

import hashlib
import inspect
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
# With the exported spec: the bridge's WhileSpec (spec_connect.py --loops --while).
JOIN_WHILE_SPEC = f"{WHILE_MODULE}.final_of_whileW_spec"
# The fixed frame of the loop route's spec: sender 0x2000, value 0, timestamp 1, self 0x1000.
FRAME = "SolidCore.Spec.TreeLoop.ctx0"
FRAME_MODULE = "Verify.Spec.TreeLoopFrame"
# The non-vacuity witness's storage: every slot 0, slot 1 present (review 154's [(1, 0)]).
WITNESS_STORAGE = "[(1, 0)]"
# The witness's return words a function with returns tries, in order (each decodes as any uint,
# int, bool or address).
NONVAC_WORDS = (1, 0, 2)
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


def loop_functions(record):
    """The loop functions on the erasure routes, in the loop cut hook's order: every function the
    hook gave obligations, none blocked."""
    cuts = record.get("loop_cuts") or {}
    blocked = record.get("loop_cuts_blocked") or {}
    if blocked:
        f, b = next(iter(blocked.items()))
        raise Refused(
            "route",
            f"the loop cut hook did not complete for {f if f != '*' else 'the source'}: "
            f"{b.get('step')}: {b.get('why')}",
        )
    if not cuts:
        raise Refused("functions", "no loop function has obligations")
    for fname, entry in cuts.items():
        if entry.get("route") not in ROUTES:
            raise Refused(
                "route",
                f"{fname}: obligations from a body summary, not the erasure: final_of_generated "
                "takes the generator's erasedOf_eq",
            )
    return list(cuts)


def loop_function(record):
    """The one loop function on the erasure route, from the loop cut hook's record."""
    fnames = loop_functions(record)
    if len(fnames) != 1:
        raise Refused(
            "functions",
            f"{len(fnames)} loop functions with obligations; the single statement covers one "
            "(loop_functions and the parts statement take several)",
        )
    return fnames[0]


def modules(name, fname=None):
    """The modules of one loop function's part of the published statement: NAME.LoopResources,
    NAME.LoopValid, NAME.LoopJoin and NAME.LoopNonVac for a contract with one loop function (FNAME
    None), NAME.Loop.FNAME.{Resources,Valid,Join,NonVac} for each of several."""
    if fname is None:
        m = {k: f"{name}.Loop{k.capitalize()}" for k in ("resources", "valid", "join")}
        m["nonvac"] = f"{name}.LoopNonVac"
    else:
        m = {
            k: f"{name}.Loop.{fname}.{s}"
            for k, s in (
                ("resources", "Resources"),
                ("valid", "Valid"),
                ("join", "Join"),
                ("nonvac", "NonVac"),
            )
        }
    return m


def module_file(root, mod):
    """The file of the Certified module MOD under ROOT (the Certified source directory)."""
    return pathlib.Path(root).joinpath(*mod.split(".")).with_suffix(".lean")


def loop_route(record, fname):
    """The route of FNAME's obligations: erasure (Inv.allSeqs) or erasure-inferred (allSeqsW)."""
    return (record.get("loop_cuts") or {}).get(fname, {}).get("route", "erasure")


# --- the fixed-fuel module -------------------------------------------------------------------


def resources(name, contract, gen_dir, plain, sel, nargs, params, mod=None, restrict=False):
    """loop_resources.py's module on this export, as MOD (NAME.LoopResources by default), with the
    facts the join needs: the limit's kind, Kmax, E, B, X, the gas parts and the valid calls' body.
    RESTRICT (a contract with several loop functions): the numbers of the loop that calls of SEL
    reach (loop_resources.obligation's selector restriction)."""
    import loop_resources

    mod = mod or f"{name}.LoopResources"
    if any(p != "uint256" for p in params):
        raise Refused(
            "abi",
            f"parameters {params}: the join's valid calls take uint256 words only",
        )
    if restrict and "sel" not in inspect.signature(loop_resources.obligation).parameters:
        raise Refused(
            "resources",
            "loop_resources.py's obligation takes no selector, which several loops need",
        )
    try:
        if restrict:
            ob = loop_resources.obligation(gen_dir, plain, sel=sel, nargs=nargs)
        else:
            ob = loop_resources.obligation(gen_dir, plain)
        last = ob["final_evm_ir"]
        lower = last[0].lower() + last[1:]
        text = loop_resources.generate(
            ns=f"Certified.{mod}",
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


def valid_module(name, facts, mod=None):
    """MOD (NAME.LoopValid by default): the valid calls as a statement input, the same body as the
    resources module's Valid."""
    mod = mod or f"{name}.LoopValid"
    kpar = " (K : Nat)" if facts["bounded"] else ""
    return f"""import Verify.ShapeGeneric
import SolarTV.Composition

-- Generated by solar's tools/certify/loop_final.py; do not edit. The body is the one
-- tools/certify/loop_resources.py proves Loops for (LoopResources.Valid), so the two are one term.

namespace Certified.{mod}

open SolarTV SolarTV.Machine SolidCore.Refinement

/-- The valid calls of the published loop statement. -/
def valid{kpar} (call : Call) : Prop :=
  {facts["valid_body"]}

end Certified.{mod}
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
    m=None,
):
    """NAME.LoopJoin: final_of_generated (final_of_generatedW on the erasure-inferred route) on this
    run's modules; with SPEC (spec_step's record) the exported spec through the bridge instead
    (spec_join_module), importing NAME.LoopNonVac when NONVAC. M: the part's modules (modules)."""
    if isinstance(spec, dict):
        return spec_join_module(
            name, contract, fname, sel, nargs, stages, facts, route, spec, nonvac, m
        )
    m = m or modules(name)
    inferred = route == "erasure-inferred"
    lemma = JOIN_W if inferred else JOIN_LEMMA
    route_import = f"import {ROUTE_MODULE}\n" if inferred else ""
    src_args = "erasedOf_eq\n    sigOf_eq"
    if route == "while-inferred":
        lemma, route_import = JOIN_WHILE, f"import {WHILE_MODULE}\n"
        src_args = f"whileOf_eq\n    sigOf_eq Certified.{name}.Loops.{fname}.src_seqs Certified.{name}.Loops.{fname}.src_valid"
    O = f"Certified.{name}.Loops.{fname}"
    R = f"Certified.{m['resources']}"
    V = f"Certified.{m['valid']}.valid"
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
import {m['resources']}
import {m['valid']}
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

namespace Certified.{m['join']}

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

/-- info: 'Certified.{m['join']}.join' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms join

end Certified.{m['join']}
"""


def spec_join_module(
    name, contract, fname, sel, nargs, stages, facts, route, spec, nonvac, m=None
):
    """NAME.LoopJoin with the exported spec: the bridge's LoopSpec (NAME.LoopSpec's F_loop_spec, of
    NAME.LoopCore's contract, the same AST as NAME.Source's by core_eq) as hspec, and the conclusion
    restated in the fixed frame (certified_ctx0). On the while route the bridge's WhileSpec instead
    (spec_connect.py --while), through final_of_whileW_spec."""
    m = m or modules(name)
    O = f"Certified.{name}.Loops.{fname}"
    R = f"Certified.{m['resources']}"
    V = f"Certified.{m['valid']}.valid"
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
    src = f"{O}.erasedOf_eq\n      sigOf_eq"
    hspec = f"(fun c hc => {spec['loop_spec']} (core_eq.trans hc))"
    route_import = ""
    if route == "while-inferred":
        # The bridge's WhileSpec is of the core module's contract and the obligation module's tree.
        lemma, obl = JOIN_WHILE_SPEC, f"{O}.seqs {O}.valid"
        src = f"{O}.whileOf_eq\n      sigOf_eq {O}.src_seqs {O}.src_valid"
        hspec = f"(by rw [← core_eq]; exact {spec['loop_spec']})"
        route_import = f"import {WHILE_MODULE}\n"
    ct = contract_term(name, contract)
    nv_import = f"import {m['nonvac']}\n" if nonvac else ""
    return f"""import {JOIN_MODULE}
import {ROUTE_MODULE}
{route_import}import {FRAME_MODULE}
import {name}.Loops.{fname}.Obl
import {m['resources']}
import {m['valid']}
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

namespace Certified.{m['join']}

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
    {lemma} (bd := {B}) (by decide) {src} {O}.leaves {obl} {O}.same
      {hspec}
      ({stages["ns"]}.{stages["compose"]} {ORACLE}) (fun _ => trivial) {R}.check_ok
      {hsub_proof(nargs, facts["bounded"])}
      ({R}.inhabited{kapp})
      (fun call hv => {R}.loops {ORACLE}{kapp} call hv) (by decide) (by decide) {fit}
  obtain ⟨c, hc, h, -⟩ := h0
  exact ⟨c, hc, SolidCore.Spec.TreeLoop.certified_ctx0 {value_proof(nargs, facts["bounded"])} h⟩

/-- info: 'Certified.{m['join']}.join' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms join

end Certified.{m['join']}
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


def witness_output(nrets, word):
    """The witness's return data: NRETS ABI words, each WORD (below 256)."""
    return [b for _ in range(nrets) for b in [0] * 31 + [word]]


def nonvac_module(name, contract, sel, nargs, facts, index, m=None, nrets=0, word=1):
    """NAME.LoopNonVac: the exported postcondition, in the fixed frame, rejects a result of a valid
    call at every iteration bound: the loop function's call with zero arguments from
    WITNESS_STORAGE, finishing with a success that writes 1 to every slot and returns NRETS words
    WORD. The witness is well typed (review 207): its return data decodes as the function's returns
    (decodeWords, checked by decide), so the proof must refute the exported ensures or invariants
    themselves, never the decoding. INDEX is the loop function's spec in the bundle. Untrusted: Lean
    checks it, and the driver declares the vacuity instead when it does not check. M: the part's
    modules (modules)."""
    m = m or modules(name)
    out = "[" + ", ".join(map(str, witness_output(nrets, word))) + "]"
    rets = "[" + ", ".join([str(word)] * nrets) + "]"
    B, _, post = spec_terms(name, contract)
    V = f"Certified.{m['valid']}.valid"
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
import {m['valid']}
import Verify.Spec.TreeLoop
import Verify.ShapeBundle

-- Generated by solar's tools/certify/loop_final.py; do not edit. Untrusted: the kernel checks it.

/-!
# {contract}: the exported postcondition rejects something on the loop's valid calls

`post_rejects`: for {doc}, the call with zero arguments from the storage {WITNESS_STORAGE} is valid,
and a success that writes 1 to every slot and returns {nrets} word(s) {word} (which decode as the
function's returns) does not meet `{B}`'s postcondition in the fixed frame
`{FRAME}`: the specification's half of the published statement's NonVacuous.
-/

namespace Certified.{m['nonvac']}

open SolarTV SolidCore.Refinement SolidCore.Spec

/-- The witness's return data decodes as the function's returns (the spec's own types). -/
theorem decodes : decodeWords ({B}.spec.specs[{index}]'(by decide)).rets (words {out}) = some {rets} := by
  decide +kernel

theorem post_rejects{binder} :
    ∃ (call : SolarTV.Call) (obs : SolarTV.Observable), ({V}{kapp}) call ∧
      (∃ s ∈ {B}.spec.specs, s.selector = {sel} ∧
        ∃ vs, SolidCore.Spec.decodeWords s.rets (SolidCore.Spec.words obs.output) = some vs) ∧
      ¬ ({post}) call obs := by
  refine ⟨{call}, ⟨true, {out}, [], fun _ => 1⟩, ⟨{WITNESS_STORAGE}, {zeros},
    by and_intros <;> first | decide | exact Nat.zero_le _ | rfl⟩,
    ⟨{B}.spec.specs[{index}]'(by decide), List.getElem_mem _, by decide, {rets}, decodes⟩, fun h => ?_⟩
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
  obtain ⟨rets, hdec, hens, -⟩ := (h _ (List.getElem_mem _) _ _ hpre).1 rfl
  -- The return data decodes (decodes, also in the type): the rejection below is the ensures' or
  -- the invariants', not the ABI's.
  have hr := Option.some.inj (hdec.symm.trans decodes)
  subst hr
  simp [{unfold}] at hens
  all_goals (revert hens; decide)

/-- info: 'Certified.{m['nonvac']}.post_rejects' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms post_rejects

end Certified.{m['nonvac']}
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
    *,
    run,
    lean_dir,
    name,
    contract,
    fname=None,
    ast,
    src,
    verify,
    lean_cmd,
    build,
    env=None,
    fnames=None,
    whiles=(),
):
    """The exported spec of the loop functions on their erased trees (the O6 bridge), in the clone
    LEAN_DIR: the source's bytes as Certified/NAME/NAME.sol, solar-verify's export of them
    (NAME.gen.C.Spec), NAME.LoopCore, then spec_connect.py --loops --only C.F,… (with --while
    C.F=NAME.Loops.F.Obl:Certified.NAME.Loops.F for each function in WHILES, the while route), which
    writes and Lean-checks NAME.LoopSpec (with its receipt). FNAMES are the loop functions (FNAME: the
    one of a single-function call). BUILD(modules) builds Lean modules (lake in the driver) and
    returns None or why not; LEAN_CMD is spec_connect.py's --lean. Returns, with FNAME, None when the
    export has no spec of the loop function, else {"loop_spec", "index", "verify_sha256", "receipt"};
    with FNAMES, {F: that record, None, or the reason the bridge refused F's spec}: a function the
    bridge refuses (exit 1, by name) or whose theorem does not check is dropped and the rest are
    connected again without it. Raises Refused("spec", why) when a step before the bridge refuses,
    and Refused("receipt", why) when spec_connect.py's receipt is missing or is not of this run's
    source bytes, exporter and module (receipt_problem)."""
    import hashlib

    import spec_connect as sc

    single = fnames is None
    fnames = [fname] if single else list(fnames)
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
    exported = out.read_text()
    res = {
        f: None for f in fnames if f'name := "{contract}.{f}"' not in exported
    }
    todo = [f for f in fnames if f not in res]
    if not todo:
        return None if single else res
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
    while todo:
        cmd = [
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
            ",".join(f"{contract}.{f}" for f in todo),
            "--solar-verify",
            str(verify),
            "--solar-verify-sha256",
            vsha,
            "--solc",
            solc[0],
        ]
        for f in todo:
            if f in whiles:
                cmd += [
                    "--while",
                    f"{contract}.{f}={name}.Loops.{f}.Obl:Certified.{name}.Loops.{f}",
                ]
        (d / "LoopSpec.lean.receipt.json").unlink(missing_ok=True)
        rc, outp, err = run(cmd, env=env)
        thms = {
            l.rsplit(".", 1)[1][: -len("_loop_spec")]: l
            for l in outp.split()
            if l.endswith("_loop_spec")
        }
        if rc == 0 and set(thms) == {sc.ident(f"{contract}.{f}") for f in todo}:
            break
        text = (err or outp).strip()
        why = f"spec_connect.py --loops refuses: {text[-400:]}"
        # The functions the bridge names: each refused one, or the one whose theorem fails.
        named = [
            f
            for f in todo
            if re.search(rf"^{re.escape(contract)}\.{re.escape(f)}: ", text, re.MULTILINE)
            or re.search(
                rf"REJECTED: spec: \S*\b{re.escape(sc.ident(contract + '.' + f))}_\w+ does not check",
                text,
            )
        ]
        if single or not named or len(named) == len(todo):
            if single:
                raise Refused("spec", why)
            res.update({f: why for f in todo})
            return res
        for f in named:
            m = re.search(rf"^{re.escape(contract)}\.{re.escape(f)}: (.*)$", text, re.MULTILINE)
            res[f] = f"spec_connect.py --loops refuses: {(m.group(1) if m else text[-300:])}"
        todo = [f for f in todo if f not in named]
    spec_text = (d / "LoopSpec.lean").read_text()
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
    for f in todo:
        fid = sc.ident(f"{contract}.{f}")
        m = re.search(
            rf"^def {re.escape(fid)}_spec : FnSpec := spec\.spec\.specs\[(\d+)\]",
            spec_text,
            re.MULTILINE,
        )
        if not m:
            raise Refused(
                "spec", f"the bridge's module names no spec index for {f}"
            )
        res[f] = {
            "loop_spec": thms[fid],
            "index": int(m.group(1)),
            "verify_sha256": vsha,
            "receipt": json.loads(receipt.read_text()),
        }
    return res[fnames[0]] if single else res


def shape(
    name,
    contract,
    fname,
    sig,
    facts,
    spec=None,
    nonvac=False,
    m=None,
    route="erasure",
    no_spec=None,
    sel=None,
    nargs=None,
):
    """shape.json for final.py. SPEC: spec_step's record (the exported spec in the fixed frame), a
    refusal string (True, declared vacuous with the refusal) or None (no spec of the loop function).
    NONVAC: True when NAME.LoopNonVac checked, else why there is no witness. M: the part's modules
    (modules); ROUTE: the function's loop route. NO_SPEC: with SPEC None, the checked no-spec
    determination (attach's record of NAME.LoopSpecInventory), which labels the part translation only.
    SEL, NARGS: the function's selector and argument count, for the call data final.py writes the
    expected statement's valid calls and gas bound from (call)."""
    m = m or modules(name)
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
        "valid": f"Certified.{m['valid']}.valid" + (" K" if bounded else ""),
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
        "spec_modules": [f"{name}.Source", m["valid"]],
        "proof_module": m["join"],
        "proof": f"Certified.{m['join']}.join",
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
            "join": {
                "erasure": JOIN_LEMMA,
                "erasure-inferred": JOIN_W,
                "while-inferred": JOIN_WHILE,
            }.get(route, JOIN_LEMMA),
            "route": route,
            "E": facts["E"],
            "B": facts["B"],
            "X": facts["X"],
            "final_evm_ir": facts["final_evm_ir"],
        },
    }
    if bounded:
        s["iteration_bound"] = {"var": "K", "max": facts["Kmax"]}
    if sel is not None:
        s["call"] = call_data(facts, sel, nargs)
    if spec is None and no_spec:
        s["spec"] = (
            f"translation only: {fname} has no specification (no NatSpec tag on it or the contract, "
            f"and no entry with its name or selector in this run's export: {no_spec['theorem']}); True"
        )
        s["vacuity"] = (
            f"translation only: {fname} has no specification ({no_spec['theorem']}), so the allowed "
            "statuses and the postcondition are True and the statement is translation (equivalence), "
            "termination and resources only"
        )
        s["excluded"] = [
            "spec: none (True), translation only; see vacuity" if e.startswith("spec:") else e
            for e in s["excluded"]
        ]
        s["no_spec"] = no_spec["theorem"]
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
        s["nonvacuous"] = f"Certified.{m['nonvac']}.post_rejects"
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


def call_data(facts, sel, nargs):
    """The data the expected statement writes a part's valid calls and gas bound from (final.py's
    call): the selector, the argument count, the loop's limit (["arg", i] or ["slot", n] at most K,
    or None for a literal limit of M iterations) and the gas parts. Read off loop_resources.py's
    Valid; the kernel then unifies the statement's valid calls with that Valid (the join's)."""
    vb = facts["valid_body"]
    a = re.search(r"\ba(\d+) ≤ K\b", vb)
    s = re.search(r"\bslotValue σ (\d+) ≤ K\b", vb)
    if facts["bounded"] and not (a or s):
        raise Refused("resources", f"the valid calls bound no limit by K: {vb}")
    lim = ["arg", int(a.group(1))] if a else ["slot", int(s.group(1))] if s else None
    return {
        "selector": sel,
        "arity": nargs,
        "limit": lim if facts["bounded"] else None,
        "per_iteration_gas": facts["per_iteration_gas"],
        "exit_gas": facts["exit_gas"],
        "M": None if facts["bounded"] else facts["M"],
    }


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
    gen_dir,
    plain,
    stages_text,
    parts=None,
    sig=None,
    sel=None,
    params=None,
    spec=None,
    nonvac=False,
    expected=None,
):
    """Write the loop functions' modules and NAME/shape.json under ROOT (the Certified source
    directory). PARTS: one {"fname", "sig", "sel", "params", "spec", "nonvac"} per loop function, in
    the hook's order (without PARTS: the record's one loop function, with SIG, SEL, PARAMS, SPEC and
    NONVAC). For one function: NAME.LoopResources, NAME.LoopValid, NAME.LoopJoin and, with a spec
    (spec_step's record), NAME.LoopNonVac, which the join imports when its NONVAC is True; shape.json
    is the single statement. For several: NAME.Loop.F.{Resources,Valid,Join,NonVac} for each F (its
    resources restricted to the loop F's calls reach), NAME.LoopJoin importing every F's join, and
    shape.json's parts, one conjunct per function with its own bound and valid calls (final.py's
    parts). EXPECTED (attach's {toolchain, inventory, inventory_module}): shape.json is final.py's
    expected statement instead, parts for one function too (shape_parts). Returns (shape, files)."""
    if parts is None:
        parts = [
            {
                "fname": loop_function(record),
                "sig": sig,
                "sel": sel,
                "params": params,
                "spec": spec,
                "nonvac": nonvac,
            }
        ]
    multi = len(parts) > 1
    root = pathlib.Path(root)
    (root / name).mkdir(parents=True, exist_ok=True)
    files, shapes = {}, []
    for p in parts:
        fname, psel, pparams, pspec = p["fname"], p["sel"], p["params"], p["spec"]
        m = modules(name, fname if multi else None)
        route = loop_route(record, fname)
        rtext, facts = resources(
            name,
            contract,
            gen_dir,
            plain,
            psel,
            len(pparams),
            pparams,
            mod=m["resources"],
            restrict=multi,
        )
        st = compose_of(stages_text, facts)
        files[m["resources"]] = rtext
        files[m["valid"]] = valid_module(name, facts, m["valid"])
        files[m["join"]] = join_module(
            name,
            contract,
            fname,
            psel,
            len(pparams),
            st,
            facts,
            route,
            pspec,
            p["nonvac"] is True,
            m,
        )
        if isinstance(pspec, dict):
            try:
                files[m["nonvac"]] = nonvac_module(
                    name,
                    contract,
                    psel,
                    len(pparams),
                    facts,
                    pspec["index"],
                    m,
                    p.get("nrets", 0),
                    p.get("nonvac_word", 1),
                )
            except Refused as e:
                if p["nonvac"] is True:
                    raise
                if multi:
                    record.setdefault("loop_final_nonvac", {})[fname] = e.why
                else:
                    record.setdefault("loop_final_nonvac", e.why)
        shapes.append(
            shape(
                name,
                contract,
                fname,
                p["sig"],
                facts,
                pspec,
                p["nonvac"],
                m,
                route,
                p.get("no_spec"),
                psel,
                len(pparams),
            )
        )
    if multi:
        files[f"{name}.LoopJoin"] = parts_join_module(
            name, contract, [modules(name, p["fname"])["join"] for p in parts]
        )
    if expected is not None:
        sh = shape_parts(name, shapes, expected)
    elif multi:
        sh = shape_parts(name, shapes)
    else:
        sh = shapes[0]
    out = []
    for mod, t in files.items():
        f = module_file(root, mod)
        f.parent.mkdir(parents=True, exist_ok=True)
        if not f.is_file() or f.read_text() != t:
            f.write_text(t)
        out.append(f)
    (root / name / "shape.json").write_text(
        json.dumps(sh, indent=2, ensure_ascii=False) + "\n"
    )
    out.append(root / name / "shape.json")
    return sh, [{"path": str(p), "sha256": sha(p)} for p in out]


def parts_join_module(name, contract, joins):
    """NAME.LoopJoin of a contract with several loop functions: the proof module of the parts
    statement, importing each function's join (each part's proof is its join's `join`)."""
    imports = "".join(f"import {j}\n" for j in joins)
    names = ", ".join(f"`Certified.{j}.join`" for j in joins)
    return f"""{imports}
-- Generated by solar's tools/certify/loop_final.py from this run's generated modules; do not edit.

/-!
# {contract}: the proof module of the published statement, one part per loop function

The statement's parts are proved by {names}, each on its function's valid calls.
-/
"""


def shape_parts(name, shapes, expected=None):
    """shape.json of a contract with several loop functions (final.py's parts): the checked source,
    the statement's inputs and NAME.LoopJoin at the top level, and each function's single-statement
    shape (SHAPES) as one part, with its own valid calls, bound, spec and proof. EXPECTED: final.py's
    expected statement (statement expected, top-level expected), whose parts carry their call data
    instead of the Lean terms of their valid calls, statuses and gas bound, which final.py writes
    itself; the single function's join is NAME.LoopJoin, as several's is."""
    import final

    top = {k: shapes[0][k] for k in ("kind", "contract", "fuel", "source_sem")}
    # The run's export namespace (tools/certify/export_ns.py), which every part shares.
    if "export_ns" in shapes[0]:
        top["export_ns"] = shapes[0]["export_ns"]
    mods, excl = [], []
    for sh in shapes:
        mods += [x for x in sh["spec_modules"] if x not in mods]
        excl += [x for x in sh["excluded"] if x not in excl]
    parts = []
    for sh in shapes:
        part = {"function": sh["loop"]["function"]}
        if expected is None:
            part.update({k: sh[k] for k in final.PART_KEYS if k in sh and k != "function"})
        else:
            part.update({k: sh[k] for k in final.EXPECTED_PART_KEYS if k in sh and k != "function"})
        parts.append(part)
    top.update(
        spec_modules=mods,
        proof_module=f"{name}.LoopJoin",
        excluded=excl,
        spec="per function: "
        + "; ".join(f"{sh['loop']['function']}: {sh['spec']}" for sh in shapes),
        source_ast=shapes[0]["source_ast"],
        loop={"functions": [sh["loop"] for sh in shapes]},
        parts=parts,
    )
    if expected is not None:
        top.update(statement="expected", expected=expected)
    return top


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


LOOP_NODES = {"ForStatement", "WhileStatement", "DoWhileStatement"}
# A NatSpec tag that may carry a specification: any @custom: tag, and @inheritdoc (the spec of a base).
SPEC_TAG = re.compile(r"@custom:[A-Za-z0-9_-]+|@inheritdoc\b")


def doc_text(node):
    """The NatSpec text solc's AST attaches to NODE (a StructuredDocumentation or, in older ASTs, a
    string), or ''."""
    d = node.get("documentation")
    if isinstance(d, dict):
        return d.get("text") or ""
    return d if isinstance(d, str) else ""


def spec_inventory(ast, contract):
    """The source side of the no-spec determination, read from the solc AST (not from solar-verify):
    {"functions": {F: tags}, "contract": [(where, tag)]} for CONTRACT and every contract it
    inherits (linearizedBaseContracts). "functions" holds each function's own NatSpec tags that may
    carry a spec (SPEC_TAG); "contract" every such tag that applies beyond one function: on a
    contract, a state variable, a modifier, an event or a constructor."""
    defs = {
        c.get("id"): c
        for c in ast.get("nodes", [])
        if c.get("nodeType") == "ContractDefinition"
    }
    top = next((c for c in defs.values() if c.get("name") == contract), None)
    if top is None:
        raise Refused("spec", f"no contract {contract} in the AST")
    fns, wide = {}, []
    for cid in top.get("linearizedBaseContracts") or [top.get("id")]:
        c = defs.get(cid)
        if c is None:
            raise Refused("spec", f"a base of {contract} (id {cid}) is not in the source's AST")
        wide += [(c.get("name"), tag) for tag in SPEC_TAG.findall(doc_text(c))]
        for n in c.get("nodes", []):
            tags = SPEC_TAG.findall(doc_text(n))
            if (
                n.get("nodeType") == "FunctionDefinition"
                and n.get("kind") == "function"
                and n.get("name")
            ):
                fns.setdefault(n["name"], []).extend(tags)
            elif tags:
                wide += [(f"{c.get('name')}.{n.get('name') or n.get('kind') or n.get('nodeType')}", tag) for tag in tags]
    return {"functions": fns, "contract": wide}


def export_inventory(text, contract):
    """The specs of the bundle CONTRACT in solar-verify's export TEXT, in order: [(name, selector)].
    Parsed here, untrusted: inventory_module states exactly this list and the kernel checks it."""
    m = re.search(rf"^def {re.escape(contract)} : Bundle where$(.*?)^/-- Every spec", text, re.M | re.S)
    if not m:
        raise Refused("inventory", f"the export has no bundle {contract}")
    body = m.group(1)
    specs = body[body.index("specs := [") :] if "specs := [" in body else ""
    inv = [
        (n, int(s, 16))
        for n, s in re.findall(r'\{ name := "([^"]+)",\s*selector := (0x[0-9a-fA-F]+),', specs)
    ]
    if specs.count("selector :=") != len(inv):
        raise Refused("inventory", "the export's spec list does not parse as name/selector entries")
    return inv


def inventory_module(name, contract, src_sha, inventory, nospec):
    """NAME.LoopSpecInventory, the export's selector inventory, kernel-checked: this run's export
    (NAME.gen.C.Spec, of the source bytes SRC_SHA) specifies exactly INVENTORY ([(name, selector)],
    in order). With True parts (NOSPEC: their {"fname", "sel"}), it also has no contract invariant,
    and for each such function no entry with its name or selector."""
    S = f"Certified.{name}.gen.{contract}.Spec"
    B = f"{S}.{contract}"
    lst = ", ".join(f'("{n}", 0x{s:08x})' for n, s in inventory)
    thms = []
    for p in nospec:
        fid = re.sub(r"[^A-Za-z0-9_]", "_", p["fname"])
        thms.append(
            f"""/-- {p['fname']} (selector 0x{p['sel']:08x}) has no spec in the export: no entry with its name or selector. -/
theorem {fid}_no_spec :
    ({B}.spec.specs.all fun s => s.name != "{contract}.{p['fname']}" && s.selector != 0x{p['sel']:08x}) = true := by
  decide +kernel
"""
        )
    if nospec:
        thms.insert(
            0,
            f"""/-- The export has no contract invariant (no spec that applies to every function). -/
theorem no_invariants : {B}.spec.invariants = [] := by
  decide +kernel
""",
        )
    body = "\n".join(thms)
    return f"""import {name}.gen.{contract}.Spec

-- Generated by solar's tools/certify/loop_final.py from this run's export; do not edit.

/-!
# {contract}: the export's spec inventory

Which functions this run's export of the source specifies, by name and selector. A part of the
published statement states True (translation only) only for a function this inventory lacks.
-/

namespace Certified.{name}.LoopSpecInventory

/-- The export is of this run's source bytes. -/
theorem source_is_fresh : {S}.manifest.sourceSha256 = "{src_sha}" := by
  decide +kernel

/-- Every spec of the export, by name and selector, in order. -/
theorem inventory : {B}.spec.specs.map (fun s => (s.name, s.selector)) = [{lst}] := by
  decide +kernel

{body}
end Certified.{name}.LoopSpecInventory
"""


def has_loop(n):
    """Whether the solc AST node N holds a loop (LOOP_NODES) anywhere below it."""
    if n.get("nodeType") in LOOP_NODES:
        return True
    return any(
        has_loop(v)
        for x in n.values()
        for v in (x if isinstance(x, list) else [x])
        if isinstance(v, dict)
    )


def source_loops(ast, contract):
    """Where the source loops, from the solc AST: {"functions": {F}, "elsewhere": [where]} over
    CONTRACT and the contracts it inherits (by the enclosing function's name), every library and
    every free function; "elsewhere" names a loop in a modifier, a free or library function, or a
    base's function the scored contract overrides."""
    defs = {c.get("id"): c for c in ast.get("nodes", []) if c.get("nodeType") == "ContractDefinition"}
    top = next((c for c in defs.values() if c.get("name") == contract), None)
    if top is None:
        raise Refused("functions", f"no contract {contract} in the AST")

    has = has_loop
    fns, elsewhere, seen = set(), [], set()
    for cid in top.get("linearizedBaseContracts") or [top.get("id")]:
        c = defs.get(cid)
        if c is None:
            elsewhere.append(f"a base (id {cid}) outside the source's AST")
            continue
        for n in c.get("nodes", []):
            if n.get("nodeType") == "FunctionDefinition" and n.get("kind") == "function":
                loops = has(n)
                if loops and n.get("name") in seen:
                    elsewhere.append(f"{c.get('name')}.{n.get('name')} (overridden)")
                elif loops:
                    fns.add(n.get("name"))
                seen.add(n.get("name"))
            elif has(n):
                elsewhere.append(f"{c.get('name')}.{n.get('name') or n.get('kind') or n.get('nodeType')}")
    for n in ast.get("nodes", []):
        if n.get("nodeType") == "FunctionDefinition" and has(n):
            elsewhere.append(f"free function {n.get('name')}")
        if n.get("nodeType") == "ContractDefinition" and n.get("contractKind") == "library" and has(n):
            elsewhere.append(f"library {n.get('name')}")
    return {"functions": fns, "elsewhere": elsewhere}


def spec_gate(ast, contract, parts, specs, inventory):
    """The per-function decision on SPECS (spec_step's {F: record, None, or the bridge's refusal})
    against INVENTORY (export_inventory, the export's specs by name and selector): a function the
    export specifies must have its spec proved by the bridge, and one the bridge refused refuses the
    whole final (it never becomes a True part); a function the export does not specify is a True
    part only when the source has no NatSpec tag that may carry a spec either (spec_inventory: on the
    function, or one that applies to every function). Returns the True parts; raises Refused("spec")."""
    inv = spec_inventory(ast, contract)
    names = {n for n, _ in inventory}
    sels = {s for _, s in inventory}
    none = []
    for p in parts:
        f, s = p["fname"], specs.get(p["fname"], "missing from the bridge's result")
        listed = f"{contract}.{f}" in names or p["sel"] in sels
        if isinstance(s, dict) and listed:
            continue
        if isinstance(s, dict):
            raise Refused("spec", f"{f}: the bridge proved a spec the export's inventory does not list")
        if isinstance(s, str):
            raise Refused(
                "spec",
                f"{f}: the spec bridge does not prove its spec, and a function with a spec is never "
                f"stated True: {s}",
            )
        if listed:
            raise Refused(
                "spec",
                f"{f}: the export lists a spec of it (name or selector 0x{p['sel']:08x}) that the "
                "bridge was never given: no True part for a specified function",
            )
        tags = inv["functions"].get(f, [])
        if tags or inv["contract"]:
            where = [f"{f}: {x}" for x in tags] + [f"{w}: {x}" for w, x in inv["contract"]]
            raise Refused(
                "spec",
                f"{f}: the export has no spec of it but the source has NatSpec tags that may carry "
                f"one ({', '.join(where[:4])}): no True part for a function that may have a spec",
            )
        none.append(p)
    return none


def part_record(p, sh, multi, fin=None):
    """record loop_final.parts[F] for the part P of the statement SH (shape.json): the spec it states,
    read per function (proved by the bridge, or True by the checked no-spec determination: translation
    only), its non-vacuity, its valid calls, its iteration bound and its gas bound (FIN, final.py's
    record: those of the expected statement, which final.py writes itself)."""
    s = next(x for x in sh["parts"] if x["function"] == p["fname"]) if "parts" in sh else sh
    if fin and fin.get("parts"):
        s = dict(s, **{k: v for k, v in next(x for x in fin["parts"] if x["function"] == p["fname"]).items() if k in ("valid", "gas")})
    spec = p["spec"]
    if isinstance(spec, dict):
        kind, label = "proved", f"solar-verify's export, proved by {spec['loop_spec']}"
    elif spec is None and p.get("no_spec"):
        kind, label = "none", f"translation only (True): no spec, {p['no_spec']['theorem']}"
    else:
        kind, label = "refused", f"not stated: {spec}"
    return {
        "signature": p["sig"],
        "selector": f"0x{p['sel']:08x}",
        "spec": kind,
        "label": label,
        "loop_spec": spec.get("loop_spec") if isinstance(spec, dict) else None,
        "no_spec": p.get("no_spec"),
        "nonvacuous": p["nonvac"],
        "nonvac_witness": p.get("nonvac_witness"),
        "valid": s.get("valid"),
        "iteration_bound": s.get("iteration_bound"),
        "gas": s.get("gas"),
    }


def reason(e, where=""):
    """The run's BLOCKED reason for a refusal."""
    return {
        "feature": "loop",
        "where": where,
        "detail": f"loop final: {e.step}: {e.why}",
    }


def toolchain(run, solar, src, name=None):
    """The compiler and the options the driver's stage export ran with (generic.export_stages), for
    the expected statement's binding of the stage manifest: the solc-compatible version the pinned
    solar reports, and the arguments it was given (the source's name, generic.OPTIONS, the runtime
    emit and the stage export, under run NAME's own namespace when NAME is given, as
    generic.export_stages passes it: export_ns.run_prefix)."""
    import generic

    rc, out, err = run([str(solar), "--version"])
    m = re.search(r"^Solc-Compatible Version: (\S+)$", out or "", re.M) if rc == 0 else None
    if m is None:
        raise Refused("export", f"{solar} --version reports no solc-compatible version: {(out or err)[-200:]}")
    opts = list(generic.OPTIONS)
    if "--evm-version" not in opts or not any(o.startswith("-O") for o in opts):
        raise Refused("export", f"the driver's options {opts} name no EVM version or optimization")
    level = next(o for o in opts if o.startswith("-O"))[2:]
    return {
        "compiler": m.group(1),
        "arguments": [
            pathlib.Path(src).name,
            *opts,
            "--emit=bin-runtime",
            "-Zemit-lean-stages=gen",
            *([f"-Zlean-stages-namespace={export_ns.run_prefix(name)}"] if name is not None else []),
        ],
        "evm_version": opts[opts.index("--evm-version") + 1],
        "optimization": level[:1].upper() + level[1:],
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
    their backend proofs (certify.py, through cut points), write the modules of every loop function
    (one part each when there are several) and shape.json,
    the published statement (final.py), build NAME.Final with lake and check it with check_final.lean
    and this run's bytes PLAIN (the fresh-byte gate). Records record["loop_final"]; returns [] when the
    final theorem checked, else the run's BLOCKED reason (a refusal names its step)."""
    import shutil

    import generic

    lean_dir = pathlib.Path(lean_dir)
    d = lean_dir / "Certified" / name
    try:
        fnames = loop_functions(record)
        # Every source loop is in a loop function with obligations: a function solar-verify gave no
        # loop heads (a failing spec, zero heads) or the hook dropped never vanishes from the final.
        loops = source_loops(ast, contract)
        if loops["elsewhere"] or loops["functions"] - set(fnames):
            raise Refused(
                "functions",
                "source loops without obligations: "
                + ", ".join(sorted(loops["functions"] - set(fnames)) + loops["elsewhere"]),
            )
        multi = len(fnames) > 1
        cdef = next(
            c
            for c in ast.get("nodes", [])
            if c.get("nodeType") == "ContractDefinition" and c.get("name") == contract
        )
        parts = []
        for fname in fnames:
            # The loop function by name, among overloads the one that loops: the hook keys its
            # obligations by name, so two looping overloads are ambiguous and refused. A non-looping
            # overload is another entry point, which the promotion's selector coverage refuses.
            same = [
                f
                for f in cdef.get("nodes", [])
                if f.get("nodeType") == "FunctionDefinition" and f.get("name") == fname
            ]
            looping = [f for f in same if has_loop(f)]
            if len(looping) > 1 or not same:
                raise Refused(
                    "functions",
                    f"{fname}: {len(looping)} looping overloads; the obligations are keyed by name",
                )
            fdef = (looping or same)[0]
            params = [
                p["typeDescriptions"]["typeString"]
                for p in fdef["parameters"]["parameters"]
            ]
            parts.append(
                {
                    "fname": fname,
                    "sig": f"{fname}({','.join(params)})",
                    "sel": int(fdef["functionSelector"], 16),
                    "params": params,
                    "nrets": len((fdef.get("returnParameters") or {}).get("parameters") or []),
                }
            )
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
        # Any refusal of the export or the bridge refuses the final: a part is True only by the
        # checked no-spec determination below (spec_gate, NAME.LoopSpecInventory), never by a refusal.
        specs = spec_step(
            run=lambda c, cwd=None, env=None: run(c, cwd=cwd, env=env),
            lean_dir=lean_dir,
            name=name,
            contract=contract,
            fnames=fnames,
            whiles={f for f in fnames if loop_route(record, f) == "while-inferred"},
            ast=ast,
            src=src,
            verify=verify,
            lean_cmd=" ".join([*loop_cuts.LAKE, "env", "lean"]),
            build=build,
            env=lake_env,
        )
        # The export's selector inventory, kernel-checked, decides every part (spec_gate).
        spec_file = d / "gen" / contract / "Spec.lean"
        inventory = export_inventory(spec_file.read_text(), contract)
        nospec = spec_gate(ast, contract, parts, specs, inventory)
        inv_file = d / "LoopSpecInventory.lean"
        inv_file.write_text(inventory_module(name, contract, sha(src), inventory, nospec))
        why = build([f"{name}.LoopSpecInventory"])
        if why:
            raise Refused("inventory", f"the export's spec inventory does not check: {why}")
        inv_mod = f"Certified.{name}.LoopSpecInventory"
        no_spec = {
            p["fname"]: {
                "module": f"{name}.LoopSpecInventory",
                "theorem": f"{inv_mod}.{re.sub(r'[^A-Za-z0-9_]', '_', p['fname'])}_no_spec",
                "selector": f"0x{p['sel']:08x}",
                "module_sha256": sha(inv_file),
            }
            for p in nospec
        }
        for p in parts:
            p["spec"], p["nonvac"] = specs[p["fname"]], False
            p["no_spec"] = no_spec.get(p["fname"])
        common = {
            "contract": contract,
            "gen_dir": g,
            "plain": plain,
            "stages_text": (d / "Proofs" / "Stages.lean").read_text(),
            "parts": parts,
            # The expected statement (final.py): the driver's compiler and options, and the export's
            # inventory, which the statement states and the kernel checks against the exports.
            "expected": {
                "toolchain": toolchain(run, solar, src, name),
                "inventory": [[n, s] for n, s in inventory],
                "inventory_module": f"{name}.LoopSpecInventory",
            },
        }
        if any(isinstance(p["spec"], dict) for p in parts):
            for p in parts:
                if not isinstance(p["spec"], dict):
                    continue
                # A well-typed witness: return words the ensures must reject, tried in order.
                for w in NONVAC_WORDS if p["nrets"] else NONVAC_WORDS[:1]:
                    p["nonvac_word"] = w
                    write(lean_dir / "Certified", name, record, **common)
                    why = build([modules(name, p["fname"] if multi else None)["nonvac"]])
                    if why is None:
                        break
                p["nonvac"] = True if why is None else f"the witness does not check: {why}"
                p["nonvac_witness"] = {
                    "args": [0] * len(p["params"]),
                    "storage": WITNESS_STORAGE,
                    "output_words": [p["nonvac_word"]] * p["nrets"],
                    "well_typed": why is None,
                }
        sh, files = write(lean_dir / "Certified", name, record, **common)
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
                "--source",
                str(src),
                "--gen",
                str(g),
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
        if any(isinstance(p["spec"], dict) for p in parts):
            accepted = acceptance_problem(
                record, src, lean_dir, name, contract, verify_pin["sha256"]
            )
            if accepted:
                raise Refused("receipt", accepted)
            accepted = json.loads((d / "LoopSpec.lean.receipt.json").read_text())
        axioms = re.search(r"axioms \[([^\]]*)\]", out)
        fresh_line = next((l for l in out.splitlines() if l.startswith("FRESH ")), None)
        if fresh_line is None:
            raise Refused("check", "check_final printed no fresh-byte line for this run's bytes")
        one = (lambda k: parts[0][k]) if not multi else (lambda k: {p["fname"]: p[k] for p in parts})
        fin = json.loads((d / "final.json").read_text())
        record["loop_final"] = {
            "function": fnames[0],
            "functions": fnames,
            "route": loop_route(record, fnames[0])
            if not multi
            else {f: loop_route(record, f) for f in fnames},
            "spec": one("spec"),
            "spec_exporter": verify_pin,
            "receipt": accepted,
            "replay": replayed,
            "nonvacuous": one("nonvac"),
            "files": files,
            "final": fin,
            # What the expected statement binds in its type (final.py): the fresh source and the
            # export's inventory, the compiler, options, MIR and stage digests, the bytes.
            "expected": fin.get("expected"),
            "axioms": axioms.group(1).split(", ") if axioms else None,
            "fresh_byte": {
                "check_final": fresh_line,
                "runtime_sha256": hashlib.sha256(bytes.fromhex(plain)).hexdigest(),
                "bytes": len(plain) // 2,
            },
            # Per function, never the aggregate: the spec it states (proved by the bridge, or True
            # only by the checked no-spec determination: translation only), its bound and valid calls.
            "parts": {p["fname"]: part_record(p, sh, multi, fin) for p in parts},
            "no_spec": no_spec,
            "inventory": {
                "module": f"{name}.LoopSpecInventory",
                "theorem": f"{inv_mod}.inventory",
                "specs": [{"name": n, "selector": f"0x{s:08x}"} for n, s in inventory],
                "module_sha256": sha(inv_file),
            },
        }
        log(f"loop final {', '.join(fnames)}: Certified.{name}.Final.final checked")
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
