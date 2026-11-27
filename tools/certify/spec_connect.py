#!/usr/bin/env python3
"""Writes the Lean module that connects a contract's exported spec to SolidCore, generically.

    spec_connect.py --core-module C --namespace NS --spec-module M --bundle B
        --module-name OUT_MODULE --out OUT.lean --lean 'CMD' [--cwd DIR]

C is a solidity-lean module that defines, in namespace NS, the interpreter fuel `fuel`, the checked
contract `contract?` and `selector_eq` (the checker's selectors by table, as
scripts/verify/uniswap_v2_core.py and Contracts/*/VC write them). M is the module solar-verify's
`--emit-lean-spec` wrote, whose bundle is B. CMD runs Lean on a file inside the solidity-lean
checkout DIR (such as `lake env lean`).

The script asks Lean, by evaluation, for each function of the bundle: SolidCore's function of that
selector, its erased tree (`eraseFn`, the generic source stage) and the storage the constructor
leaves. It then writes OUT.lean, which states for each function F:

* `deploy_eq` and `F_erase_eq` (`kernel_rfl`): the contract deploys with that storage and no logs, and dispatches
  F's selector to a function the generic stage erases to `F_tree`, the tree Lean printed;
* `F_meets`: F's exported spec holds of `F_tree` (`SolidCore.Spec.TreeMeets`, by `tree_meets`);
* `F_holds`: F's spec holds of SolidCore's checked contract on the stage's calls of F
  (`SolidCore.Spec.holdsFn_of_tree`).

With --tree-only it writes only `deploy_eq`, `fn_F`, `F_tree`, `F_erase_eq` and `F_spec`: the
erased tree and the exported spec, for a proof written over them by hand (such as one with
succeeds-if conditions, or one whose hash premise is scoped to the slots a call touches), and it
refuses no function the generic stage erases.

With --loops, a function the generic stage does not erase but the loop erasure does
(`eraseLoopStmt`, one counted loop, then `brTree`; Verify/LoopSource/Proof/EraseLoop.lean) is
connected through the loop route instead of `holdsFn_of_tree`. For such a function F it writes
`F_tree` (the loop erasure's tree), `F_loop_eq` (`kernel_rfl` of `TreeLoop.contractLoopCheck`:
F's selector dispatches to a function of that many parameters, with those argument widths and word
returns, whose loop erasure's tree is `F_tree`, and the bundle is the contract's export with F's spec,
of F's selector and named for F, at F's index), `F_meets` (`TreeMeets` of F's exported spec on
`F_tree`) and `F_loop_spec` (`TreeLoop.LoopSpec`, by `TreeLoop.loopSpec_of_check`, Verify/Spec/
TreeLoopCheck.lean): the source premises of `Verify.LoopSource.Equiv.LeastFuel.eraseLoop_certified`,
with the export's `Shapes.bundleAllowed` and `Shapes.bundlePost` (at `Shapes.ctxOfCall` and
`Shapes.idStep`) as `OnTree` of `F_tree`, and `TreeLoop.ExportedFor` of the bundle. The loop route is
runtime-only and in the fixed frame (sender 0x2000, value 0, timestamp 1, self 0x1000), so with
--loops a constructor that does not deploy refuses only the functions the generic stage erases. A
loop function with succeeds-if conditions, mapping reads, more than eight parameters or under
--any-caller is refused.

With --loops the bundle must be the export of the core module's contract (`provenance_refusal`):
the bundle is named for the contract, the export's manifest names the file whose
`importedSourceName` the core module's imports define, and that file's SHA-256 is the manifest's.
The driver then binds the bundle's contents and the checked AST to those source bytes, as the
published statement binds the bytecode to this run's compile (`runtime_is_fresh`). It reads the
source once (a relative path inside the checkout, `snapshot_source`), runs the provenance gate on those
bytes, copies them, has solar-verify export their specs and solidity-lean's front end
(scripts/solc_ast_to_lean_source.py and its harness_cache.py, caches off, with the pinned solc) translate
them, refuses a bundle whose text is not this run's export (`fresh_export`), prints the SHA-256 of the
exported bundle's text, and writes both outputs into the module as literals: `exported` (with
`exportedSha256`) and `FreshSource`. solar-verify (--solar-verify) must have the SHA-256
--solar-verify-sha256 pins, and every tool runs as a copy of the bytes hashed; the module records the
SHA-256s of the source bytes and of each tool (`sourceSha256`, `exporterSha256`, `astToolSha256`,
`astToolCacheSha256`, `solcSha256`). The kernel checks `spec_is_fresh` (the bundle the spec module
defines is `exported`, by `rfl`: a bundle with an ensures, an invariant or any other clause removed or
changed fails it) and `source_is_fresh` (the core module's `contract?` is the canonical type checker,
its instance `checkedInputSourceUnit` given explicitly, on `FreshSource.importedSourceUnit`, while
`source_is_fresh_discriminates` shows the same checker gives another value on an empty source unit),
and every theorem of the module, the loop route's `_loop_spec` included, names `spec := exported`.
The module also states `spec_source` (the manifest's source is `importedSourceName` and its SHA-256
is `sourceSha256`) and `receipt`, the conjunction of the three. After Lean checks the module the
driver reruns the provenance gate on the file against the receipt's SHA-256 and writes
OUT.lean.receipt.json with the hashes and the module's SHA-256.
Function specs that share a name (overloads) are refused, since probe records are keyed by name.

Nothing in the module is trusted: each theorem is checked by the kernel when the module builds,
and a function whose shape, tree or spec does not check fails the build. A function the generic
stage does not erase (without --loops, or with --loops when the loop erasure does not erase it
either), or a spec with succeeds-if conditions, is refused here (exit 1) and listed on stderr,
never written as passing.
"""

import argparse
import pathlib
import re
import shlex
import shutil
import subprocess
import sys
import tempfile

HERE = pathlib.Path(__file__).resolve().parent

PROBE = """import {core}
import {spec}
import Verify.VC.Model
{sym_import}
open SolidCore.Refinement SolidCore.Refinement.Generic SolidCore.Solidity

#eval show IO Unit from do
  match {ns}.contract? with
  | .error _ => IO.println "SPEC_CONNECT error the contract does not type check"
  | .ok c =>
    IO.println s!"SPEC_CONNECT names {{repr ({bundle}).spec.name}} {{repr c.decl.name}}"
    let dep := match TypeCheck.CheckedContract.constructWithContext? {ns}.fuel c (Wide.ctxOf c)
        Source.State.empty SolidCore.Refinement.selfAddress 0 [] with
      | some (Source.CallResult.returned d _) =>
        if d.events.isEmpty then some d.storage.toList else none
      | _ => none
    IO.println s!"SPEC_CONNECT deployed {{repr dep}}"
    for fs in ({bundle}).spec.specs do
      let r := (c.core.findFunctionBySelector? fs.selector).map fun f =>
        (f.params.length, eraseFn (envOf c.core) {ns}.fuel f)
      match r with
      | none => IO.println s!"SPEC_CONNECT fn {{fs.name}} none"
      | some (n, none) => IO.println s!"SPEC_CONNECT fn {{fs.name}} {{n}} noerase"
      | some (n, some t) =>
        IO.println s!"SPEC_CONNECT fn {{fs.name}} {{n}} {{fs.succeedsIf.length}} {{fs.selector}} TREE {{(repr t).pretty 100000}}"
{sym}"""

# --loops: for each function the generic stage does not erase, its argument widths and the tree of
# its loop erasure (`eraseLoopStmt`, then `brTree`), as `TreeLoop.contractLoopCheck` reads them.
PROBE_LOOP = """    for fs in ({bundle}).spec.specs do
      match c.core.findFunctionBySelector? fs.selector with
      | none => pure ()
      | some f =>
        match eraseFn (envOf c.core) {ns}.fuel f with
        | some _ => pure ()
        | none =>
          match widthsOf f.params f.paramAbiCleanups with
          | none => IO.println s!"SPEC_CONNECT loop {{fs.name}} nowidths"
          | some ws =>
            if f.returns.all wordBinding then
              match (Loop.eraseLoopStmt {{ envOf c.core with facts := argFacts ws 0 }} {ns}.fuel f.body
                  (initState f)).bind (brTree f (argFacts ws 0)) with
              | none => IO.println s!"SPEC_CONNECT loop {{fs.name}} noloop"
              | some t =>
                IO.println s!"SPEC_CONNECT loop {{fs.name}} {{fs.succeedsIf.length}} {{fs.selector}} {{repr ws}} TREE {{(repr t).pretty 100000}}"
            else IO.println s!"SPEC_CONNECT loop {{fs.name}} returns"
"""

# Why the loop erasure gives no tree, by the probe's word.
LOOP_REFUSAL = {
    "nowidths": "outside the generic source stage, and the loop route needs uint or address arguments (widthsOf)",
    "returns": "outside the generic source stage, and the loop route needs word returns (wordBinding)",
    "noloop": "outside the generic source stage and the loop erasure (eraseLoopStmt: not one counted loop)",
}

# --any-caller: the erasure with `msg.sender` the symbolic word (`eraseFnSym`, the stage for
# every caller), whether the function is payable and its argument widths, after the lines above.
PROBE_SYM = """    for fs in ({bundle}).spec.specs do
      match c.core.findFunctionBySelector? fs.selector with
      | none => IO.println s!"SPEC_CONNECT sym {{fs.name}} none"
      | some f =>
        match eraseFnSym (envOf c.core) {ns}.fuel f with
        | none => IO.println s!"SPEC_CONNECT sym {{fs.name}} noerase"
        | some t =>
          IO.println s!"SPEC_CONNECT sym {{fs.name}} {{f.payable}} {{repr (widthsOf f.params f.paramAbiCleanups)}} TREE {{(repr t).pretty 100000}}"
"""


# A function whose spec is false on a small concrete call is refuted rather than left to fail or
# time out: this probe (untrusted, by evaluation) proposes the call, and the module states the
# negation of the theorem that would fail, which the kernel checks (refutation_block). Its reading
# of the spec's terms covers no quantifier, ghost or spec function call; a term it cannot read
# proposes nothing.

SEARCH = """import {spec}
import Verify.Spec.Tree
import Verify.Spec.TreeSucceeds
import Verify.Spec.TreeFrom
@SEARCH@
open SolidCore.Spec SolidCore.Refinement SolidCore.Refinement.Generic

def SpecConnectSearch.ctx : Ctx := {{ sender := 0x2000, value := 0, timestamp := 1, self := 0x1000 }}

#eval show IO Unit from do
  if SpecConnectSearch.initFails ({bundle}) SpecConnectSearch.ctx {deployed} then
    IO.println "SPEC_CONNECT initinv"
{runs}"""

SEARCH_RUN = """  match SpecConnectSearch.search ({bundle}) (({bundle}).spec.specs[{i}]'(by decide)) SpecConnectSearch.ctx {n}
      ({tree}) {deployed} with
  | some (k, a, ia, ps) => IO.println s!"SPEC_CONNECT cex {fid} {{k}} {{a}} {{ia}} {{ps}}"
  | none => IO.println "SPEC_CONNECT nocex {fid}"
  match SpecConnectSearch.valueCex (({bundle}).spec.specs[{i}]'(by decide)) SpecConnectSearch.ctx {n} {deployed} with
  | some (ia, ps) => IO.println s!"SPEC_CONNECT value {fid} {{ia}} {{ps}}"
  | none => pure ()
  if SpecConnectSearch.requiresUnsat (({bundle}).spec.specs[{i}]'(by decide)) SpecConnectSearch.ctx {n} {deployed} then
    IO.println "SPEC_CONNECT unsat {fid}"
"""


def search_cex(args, fns, deployed):
    """The probe's proposed counterexamples: fid -> (kind, args, decoded args, storage pairs), each
    as Lean text. A probe that fails proposes nothing (the run goes on to the proofs)."""
    if not fns:
        return {}
    runs = "".join(
        SEARCH_RUN.format(
            bundle=args.bundle, i=i, n=n, tree=tree, deployed=deployed, fid=fid
        )
        for i, _name, fid, n, _sel, tree, _slots, _nsi, *_ in fns
    )
    text = SEARCH.format(
        spec=args.spec_module, runs=runs, bundle=args.bundle, deployed=deployed
    ).replace("@SEARCH@", (HERE / "spec_search.lean").read_text())
    with tempfile.NamedTemporaryFile(
        "w", suffix=".lean", dir=args.cwd, delete=False
    ) as f:
        f.write(text)
        path = pathlib.Path(f.name)
    try:
        res = subprocess.run(
            shlex.split(args.lean) + [str(path)],
            cwd=args.cwd,
            capture_output=True,
            text=True,
            check=False,
        )
    finally:
        path.unlink()
    out = {}
    if res.returncode != 0:
        return out
    pat = re.compile(
        r"SPEC_CONNECT cex (\w+) (meets0inv|meets0|succeeds) (\[[^\]]*\]) (\[[^\]]*\]) (\[.*\])$"
    )
    val = re.compile(r"SPEC_CONNECT value (\w+) (\[[^\]]*\]) (\[.*\])$")
    for l in res.stdout.splitlines():
        l = l.strip()
        if m := pat.match(l):
            out.setdefault(m.group(1), []).append(m.groups()[1:])
        elif m := val.match(l):
            out.setdefault(m.group(1), []).append(
                ("value", None, m.group(2), m.group(3))
            )
        elif m := re.fullmatch(r"SPEC_CONNECT unsat (\w+)", l):
            out.setdefault(m.group(1), []).append(("unsat", None, None, None))
        elif l == "SPEC_CONNECT initinv":
            out["@init"] = [("init", None, None, None)]
    return out


REFUTE_LEMMAS = [
    "/-- A call meeting the precondition whose observable fails the postcondition refutes",
    "`TreeMeetsFixed`. -/",
    "theorem not_treeMeetsFixed_of {b : Bundle} {fs : FnSpec} {ctx : Ctx} {n : Nat} {T : Tree}",
    "    (args : List Nat) (s : Storage) (iargs : List Int) (pre : State)",
    "    (hl : args.length = n) (ha : ∀ a ∈ args, a < 2 ^ 256) (hs : ∀ k, s k < 2 ^ 256)",
    "    (hd : decodeWords fs.params args = some iargs) (hst : pre.storage = s) (ht : pre.time ≤ ctx.timestamp)",
    "    (hr : b.satAll fs.requires (mkEnv ctx iargs [] pre pre))",
    "    (hi : b.satAll b.spec.invariants (mkEnv ctx iargs [] pre pre))",
    "    (hpost : ¬ Post b fs ctx pre pre.ghost (T.eval ⟨args, s, solHash, callerWord⟩).obs iargs) :",
    "    ¬ TreeMeetsFixed b fs ctx n T :=",
    "  fun h => hpost (h args s iargs pre hl ha hs hd hst ht hr hi)",
    "",
    "/-- A call meeting the precondition and the succeeds-if conditions on which the tree fails refutes",
    "`TreeSucceeds`. -/",
    "theorem not_treeSucceeds_of {b : Bundle} {fs : FnSpec} {ctx : Ctx} {n : Nat} {T : Tree}",
    "    (args : List Nat) (s : Storage) (iargs : List Int) (pre : State)",
    "    (hl : args.length = n) (ha : ∀ a ∈ args, a < 2 ^ 256) (hs : ∀ k, s k < 2 ^ 256)",
    "    (hd : decodeWords fs.params args = some iargs) (hst : pre.storage = s) (ht : pre.time ≤ ctx.timestamp)",
    "    (hr : b.satAll fs.requires (mkEnv ctx iargs [] pre pre))",
    "    (hi : b.satAll b.spec.invariants (mkEnv ctx iargs [] pre pre))",
    "    (hsi : b.satAll fs.succeedsIf (mkEnv ctx iargs [] pre pre))",
    "    (hf : (T.eval ⟨args, s, solHash, callerWord⟩).obs.success = false) :",
    "    ¬ TreeSucceeds b fs ctx n T :=",
    "  fun h => by rw [h args s iargs pre hl ha hs hd hst ht hr hi hsi] at hf; cases hf",
    "",
]


def refuted_name(fid, kind, nsi):
    """The obligation a proposal refutes, and the name of its refutation: _meets0 (_meets without
    succeeds-if) or _succeeds of the tree; _succeeds0, the succeeds-if conditions at every value,
    refuted as ValueRefuses (the dispatcher of a nonpayable function reverts a call with a value, so
    its succeeds-if conditions must exclude one); the requires clauses admitting no call (the
    function's statement vacuous); the invariants of the storage the constructor leaves."""
    if kind == "succeeds":
        return f"{fid}_succeeds", f"{fid}_succeeds_refuted"
    if kind == "unsat":
        return (
            f"vacuous: {fid}: its requires clauses admit no call",
            f"{fid}_requires_unsat",
        )
    if kind == "init":
        return (
            "invariant: the invariants fail in the initial state",
            "invariants_initial_refuted",
        )
    if kind == "value":
        return f"{fid}_succeeds0", f"{fid}_value_refuses_refuted"
    t = f"{fid}_meets0" if nsi else f"{fid}_meets"
    return t, f"{t}_refuted"


def refutation_block(fid, n, nsi, cex, mod, bundle):
    """The negation of the theorem CEX refutes, proved on its call by evaluation (simp over the
    tree, the spec and its terms; every step checked by the kernel), with its axioms pinned."""
    kind, a, ia, ps = cex
    _, thm = refuted_name(fid, kind, nsi)
    ds = (
        (f"{fid}_spec, " if fid else "")
        + f"{mod}.spec, {bundle}, SolidCore.Spec.FnSpec.noSucc, {mod}.ctx, "
        + ("SolidCore.Spec.ctxAt, " if kind == "value" else "")
        + ""
        "decodeWords, Ty.ofWord, Post, Tree.eval, WExpr.eval, Res.obs, Bundle.satAll, Bundle.sat, eval, "
        "BinOp.apply, ofProp, mkEnv, Env.cur, slotOf, Ty.ofBits, toWord, List.lookup"
    )
    pin = [
        "",
        f"/-- info: '{mod}.{thm}' depends on axioms: [propext, Classical.choice, Quot.sound] -/",
        "#guard_msgs (whitespace := lax) in",
        f"#print axioms {thm}",
        "",
    ]
    opts = [
        "set_option maxHeartbeats 4000000 in",
        "set_option linter.unusedSimpArgs false in",
        "set_option linter.unusedTactic false in",
        "set_option linter.unreachableTactic false in",
    ]
    if kind == "unsat":
        return (
            opts
            + [
                f"/-- **`{fid}`'s requires clauses admit no call**, in any context and from any state: its",
                "statement would hold of no call (vacuous). -/",
                f"theorem {thm} : ∀ (c : Ctx) (iargs : List Int) (pre : State),",
                f"    ¬ spec.satAll {fid}_spec.requires (mkEnv c iargs [] pre pre) := by",
                "  intro c iargs pre h",
                f"  simp [{ds}] at h",
                "  all_goals omega",
            ]
            + pin
        )
    if kind == "init":
        return (
            opts
            + [
                "/-- **The invariants are false of the storage the constructor leaves** (`deployed`, SolidCore's",
                "constructor run by `deploy_eq`). -/",
                f"theorem {thm} : ¬ spec.satAll spec.spec.invariants",
                "    (mkEnv ctx [] [] ⟨fun k => (deployed.lookup k).getD 0 % 2 ^ 256, SolidCore.Spec.Ghost.zero, ctx.timestamp⟩",
                "      ⟨fun k => (deployed.lookup k).getD 0 % 2 ^ 256, SolidCore.Spec.Ghost.zero, ctx.timestamp⟩) := by",
                f"  simp [deployed, {ds}]",
                "  all_goals first | rfl | decide | omega",
            ]
            + pin
        )
    st = f"(fun k => (({ps} : List (Nat × Nat)).lookup k).getD 0 % 2 ^ 256)"
    if kind == "value":
        return (
            opts
            + [
                f"/-- **`{fid}`'s succeeds-if conditions admit a call with value 1**, which the nonpayable",
                "function's dispatcher reverts: they do not exclude the value. -/",
                f"theorem {thm} : ¬ ValueRefuses spec {fid}_spec := by",
                "  intro h",
                f"  refine h 0x2000 1 {ia} ⟨{st}, SolidCore.Spec.Ghost.zero, 1⟩ (by decide) (by decide) ?_ ?_ <;>",
                f"  simp [{ds}]",
                "  all_goals first | rfl | decide",
            ]
            + pin
        )
    common = (
        f"{a} {st} {ia} ⟨{st}, SolidCore.Spec.Ghost.zero, ctx.timestamp⟩\n"
        "    rfl (by decide) (fun _ => Nat.mod_lt _ (by decide)) ?_ rfl (le_refl _) ?_ ?_ ?_"
    )
    ds = f"{fid}_tree, " + ds
    if kind == "succeeds":
        stmt = f"¬ TreeSucceeds spec {fid}_spec ctx {n} {fid}_tree"
        intro = f"  refine not_treeSucceeds_of {common} ?_ <;>"
        what = (
            "meets its precondition and its succeeds-if conditions, and its tree fails"
        )
    else:
        spec_t = f"{fid}_spec.noSucc" if nsi else f"{fid}_spec"
        stmt = f"¬ TreeMeetsFixed spec {spec_t} ctx {n} {fid}_tree"
        intro = f"  refine not_treeMeetsFixed_of {common} <;>"
        what = "meets its precondition, and its result fails the postcondition"
    return (
        opts
        + [
            f"/-- **`{thm[: -len('_refuted')]}` is false**: the call with argument words `{a}` from the storage `{ps}` {what}. -/",
            f"theorem {thm} : {stmt} := by",
            intro,
            f"  simp [{ds}]",
            "  all_goals first | rfl | decide",
        ]
        + pin
    )


def ident(name):
    """A Lean identifier for a spec function's name (its part after the contract)."""
    base = name.rsplit(".", 1)[-1]
    return re.sub(r"[^A-Za-z0-9_]", "_", base)


def groups(text, start):
    """The balanced parenthesized groups of `text` from `start` on, each as a string."""
    out, i = [], start
    while True:
        while i < len(text) and text[i].isspace():
            i += 1
        if i >= len(text) or text[i] != "(":
            return out
        depth, j = 0, i
        while True:
            if text[j] == "(":
                depth += 1
            elif text[j] == ")":
                depth -= 1
                if depth == 0:
                    break
            j += 1
        out.append(text[i : j + 1])
        i = j + 1


def mapping_slots(tree):
    """The mapping slots a tree hashes keys into, as (first-level slots, slots of mappings of
    mappings), or None when a key is hashed into a slot that is neither a literal nor a literal
    mapping's entry."""
    slots, nested = set(), set()
    for m in re.finditer(r"WExpr\.hash\b", tree):
        args = groups(tree, m.end())[:2]
        if len(args) != 2:
            return None
        arg = re.sub(r"\s+", " ", args[1])
        lit = re.fullmatch(r"\((?:[\w.]*\.)?WExpr\.lit (\d+)\)", arg)
        if lit:
            slots.add(int(lit.group(1)))
            continue
        inner = re.fullmatch(r"\((?:[\w.]*\.)?WExpr\.hash (.*)\)", arg)
        if inner:
            sub = groups(inner.group(1), 0)[:2]
            ilit = len(sub) == 2 and re.fullmatch(
                r"\((?:[\w.]*\.)?WExpr\.lit (\d+)\)", sub[1]
            )
            if ilit:
                nested.add(int(ilit.group(1)))
                continue
        return None
    return sorted(slots - nested), sorted(nested)


def spec_mapping_slots(path):
    """The mapping slots the exported spec module reads, as (slots of mappings, slots of mappings of
    mappings): every `(.entry (.slot N) _)` and `(.entry (.entry (.slot N) _) _)` of its terms."""
    text = path.read_text() if path.exists() else ""
    nested = {int(n) for n in re.findall(r"\(\.entry \(\.entry \(\.slot (\d+)\)", text)}
    flat = {int(n) for n in re.findall(r"\(\.entry \(\.slot (\d+)\)", text)}
    return flat - nested, nested


def tree_succeeds_tolerant(ds):
    """The succeeds-if proof for the simp set `ds`: `tree_succeeds_try` (Verify/Spec/TreeSucceedsTry.lean),
    `tree_succeeds` with every step allowed to change nothing, as for a succeeds-if over the call's
    environment alone (`msg.value == 0`), where the macro's step has nothing to rewrite."""
    return f"  tree_succeeds_try [{ds}]"


def parse_sym(line):
    """A probe line `sym NAME PAYABLE (some [w, ...]) TREE t` as (name, payable, widths, tree):
    payable is None (no function of its selector) or "noerase" when there is no tree."""
    parts = line.split(" ", 2)
    name, rest = parts[1], parts[2] if len(parts) > 2 else ""
    if rest in ("none", "noerase"):
        return name, (None if rest == "none" else "noerase"), None, None
    m = re.fullmatch(
        r"(true|false) (?:some )?(\[[^\]]*\]|none) TREE (.*)", rest, re.DOTALL
    )
    if not m:
        sys.exit(f"spec_connect: unreadable probe line: {line[:200]}")
    return (
        name,
        m.group(1) == "true",
        None if m.group(2) == "none" else m.group(2),
        m.group(3),
    )


def any_caller_refusal(sym, slots, tree_only=False):
    """Why --any-caller cannot state a function's spec from every caller, or None. With
    --tree-only only the symbolic-caller tree is written, so only its absence refuses."""
    if sym is None or sym[0] is None:
        return "no function of its selector"
    if sym[0] == "noerase":
        return "outside the generic source stage for every caller (eraseFnSym)"
    if tree_only:
        return None
    payable, ws, _ = sym
    if payable:
        return "payable: a call with value runs its tree, which holdsFn_of_tree_from does not cover"
    if ws is None:
        return "its argument widths are not known (widthsOf)"
    if slots[0] or slots[1]:
        return "reads mappings, which the every-caller proof does not cover yet"
    return None


def parse_loop(line):
    """A probe line `loop NAME NSI SEL [w, ...] TREE t` as (nsi, sel, widths, tree), or
    `loop NAME WHY` as (WHY, None, None, None)."""
    m = re.fullmatch(r"loop \S+ (\d+) (\d+) (\[[^\]]*\]) TREE (.*)", line, re.DOTALL)
    if m:
        return int(m.group(1)), int(m.group(2)), m.group(3), m.group(4)
    m = re.fullmatch(r"loop \S+ (\w+)", line)
    if not m:
        sys.exit(f"spec_connect: unreadable probe line: {line[:200]}")
    return m.group(1), None, None, None


def loop_refusal(loop, n, spec_slots, args):
    """Why --loops cannot connect a function the generic stage does not erase, or None."""
    if loop is None:
        return "outside the generic source stage, and the probe printed no loop erasure"
    if loop[1] is None:
        return LOOP_REFUSAL.get(
            loop[0], f"outside the generic source stage ({loop[0]})"
        )
    if args.any_caller:
        return "a loop function: the loop route states the fixed caller's calls only (selCall)"
    if n > 8:
        return "a loop function with more than eight parameters"
    if args.tree_only:
        return None
    if loop[0]:
        return "a loop function with succeeds-if conditions, which the loop bridge does not prove"
    slots = mapping_slots(loop[3])
    if slots is None or slots[0] or slots[1] or spec_slots[0] or spec_slots[1]:
        return (
            "a loop function that reads mappings, which the loop bridge does not prove"
        )
    return None


def lean_string(text):
    """The value of a Lean string literal `repr` printed, such as `"Counter"`."""
    m = re.fullmatch(r'"((?:[^"\\]|\\.)*)"', text.strip())
    if not m:
        sys.exit(f"spec_connect: unreadable Lean string: {text[:200]}")
    return re.sub(r"\\(.)", r"\1", m.group(1))


def manifest_of(spec_text):
    """The (source, sourceSha256) of the exported spec module's `manifest`, or None."""
    m = re.search(
        r'def manifest : Manifest :=\s*\{\s*source := "([^"]*)",\s*sourceSha256 := "([0-9a-f]*)"',
        spec_text,
    )
    return (m.group(1), m.group(2)) if m else None


def module_path(cwd, module):
    """The file of a Lean module in the checkout `cwd`: at its path from the root or under one
    library's source directory (such as Certified/ for `verify_counter.*`); None when there is not
    exactly one."""
    root = pathlib.Path(cwd)
    rel = module.replace(".", "/") + ".lean"
    if (root / rel).is_file():
        return root / rel
    found = [
        p
        for p in root.glob(f"*/{rel}")
        if p.is_file() and ".lake" not in p.relative_to(root).parts
    ]
    return found[0] if len(found) == 1 else None


def core_source(cwd, module):
    """The `importedSourceName` definitions in the core module's imports within the checkout `cwd`,
    as (full Lean name, value) pairs: the file the checked contract was imported from."""
    found, seen, todo = [], set(), [module]
    root = pathlib.Path(cwd)
    while todo:
        mod = todo.pop()
        if mod in seen:
            continue
        seen.add(mod)
        path = module_path(root, mod)
        if path is None:
            continue
        stack = []
        for line in path.read_text().splitlines():
            imp = re.match(r"^import\s+(\S+)\s*$", line)
            if imp:
                todo.append(imp.group(1))
                continue
            ns = re.match(r"^namespace\s+(\S+)\s*$", line)
            if ns:
                stack.append(ns.group(1))
                continue
            end = re.match(r"^end\s+(\S+)\s*$", line)
            if end and stack and stack[-1] == end.group(1):
                stack.pop()
                continue
            d = re.match(r'^def importedSourceName : String := "([^"]*)"\s*$', line)
            if d:
                found.append((".".join([*stack, "importedSourceName"]), d.group(1)))
    return found


def provenance_refusal(names, manifest, sources, cwd, data=None):
    """Why the bundle is not the export of the core module's contract, or None: the bundle and the
    contract are named alike (`names`, the probe's), the export's manifest names the one file the
    core module's imports say the contract was imported from (`sources`, from `core_source`), a
    relative path inside the checkout `cwd`, and that file's bytes have the manifest's SHA-256: the
    bytes `data` when given (the snapshot this run exports and its receipt records), else the
    file's bytes now."""
    import hashlib

    if names is None:
        return "the probe printed no bundle and contract names"
    bname, cname = names
    if bname != cname:
        return f"the bundle is the export of `{bname}`, not of the checked contract `{cname}`"
    if manifest is None:
        return "the exported spec module has no manifest with its source and SHA-256"
    src, sha = manifest
    if len(sources) != 1:
        return (
            "the core module's imports define no single importedSourceName "
            f"({len(sources)} found), so the contract's source file is not known"
        )
    if sources[0][1] != src:
        return f"the export is of {src}, but the contract was imported from {sources[0][1]}"
    if data is None:
        data = snapshot_source(cwd, src)
        if isinstance(data, str):
            return data
    got = hashlib.sha256(data).hexdigest()
    if got != sha:
        return f"{src} has SHA-256 {got}, not the export's {sha}"
    return None


def snapshot_source(cwd, src):
    """The bytes of `src`, read once: a relative path inside the checkout `cwd` (no ``..``, not
    absolute, not a link out of it), so that its copy in a temporary directory is a copy and not
    the original. The refusal string otherwise."""
    rel = pathlib.PurePosixPath(src)
    if not src or rel.is_absolute() or ".." in rel.parts or "\\" in src:
        return f"the export's source {src!r} is not a relative path inside the checkout"
    root = pathlib.Path(cwd).resolve()
    path = root / rel
    try:
        path.resolve().relative_to(root)
    except ValueError:
        return f"the export's source {src} leads out of the checkout"
    if not path.is_file():
        return f"the export's source {src} is not in the checkout"
    return path.read_bytes()


def pinned_exporter(path, sha):
    """The bytes of the solar-verify at `path` and their SHA-256, which must be the pinned `sha`:
    the driver runs a copy of exactly those bytes. The refusal string otherwise."""
    import hashlib

    if not path:
        return "no solar-verify is given (--solar-verify or SOLAR_VERIFY)"
    if not sha or not re.fullmatch(r"[0-9a-f]{64}", sha):
        return (
            "no pinned SHA-256 of solar-verify is given (--solar-verify-sha256 or "
            "SOLAR_VERIFY_SHA256, 64 lowercase hex digits)"
        )
    p = pathlib.Path(path)
    if not p.is_file():
        return f"solar-verify {path} is not a file"
    data = p.read_bytes()
    got = hashlib.sha256(data).hexdigest()
    if got != sha:
        return f"{path} is not the pinned solar-verify (SHA-256 {got}, pinned {sha})"
    return data, got


# The front end that writes the checked AST: the script and the sibling it loads by path.
FRONT_END = ("solc_ast_to_lean_source.py", "harness_cache.py")


def front_end(cwd):
    """The bytes of solidity-lean's front end in the checkout `cwd` (`FRONT_END` under scripts/),
    by file name, read once: the driver runs a copy of exactly these. The refusal string if one is
    missing."""
    files = {}
    for name in FRONT_END:
        path = pathlib.Path(cwd) / "scripts" / name
        if not path.is_file():
            return f"the front end's {path} is missing"
        files[name] = path.read_bytes()
    return files


def bundle_block(text, cname):
    """The definition of the bundle `cname` in an exported spec module, as solar-verify printed it:
    from `def cname : Bundle where` to the blank line that ends it. None when there is none."""
    m = re.search(
        rf"^def {re.escape(cname)} : Bundle where\n(?:[^\n]+\n)*?(?=\n)",
        text,
        re.MULTILINE,
    )
    return m.group(0) if m else None


def pinned_solc(cwd, given=None):
    """The pinned solc of the solidity-lean checkout `cwd` (the version and SHA-256
    scripts/refinement/gen.sh names), as tools/certify/source_bind.sh finds it: `given`, else SOLC,
    else the checkout's .cache, else ~/.cache/solc. A (path, version, SHA-256) triple, or the refusal
    string."""
    import hashlib
    import os

    gen = pathlib.Path(cwd) / "scripts/refinement/gen.sh"
    if not gen.is_file():
        return f"{gen} is missing, so the pinned solc is not known"
    text = gen.read_text()
    ver = re.search(r"^solc_version=(\S+)$", text, re.MULTILINE)
    sha = re.search(r"^solc_sha256=([0-9a-f]{64})$", text, re.MULTILINE)
    if not ver or not sha:
        return f"cannot read the pinned solc from {gen}"
    ver = ver.group(1)
    cands = (
        [given, os.environ.get("SOLC")]
        if given or os.environ.get("SOLC")
        else [
            pathlib.Path(cwd) / f".cache/solc-{ver}",
            pathlib.Path.home() / f".cache/solc/solc-{ver}",
        ]
    )
    for c in cands:
        if c and pathlib.Path(c).is_file():
            got = hashlib.sha256(pathlib.Path(c).read_bytes()).hexdigest()
            if got != sha.group(1):
                return f"{c} is not the pinned solc {ver} (SHA-256 {got})"
            return (str(c), ver, got)
    return f"no solc {ver} (set SOLC)"


def fresh_export(src, data, cname, spec_ns, spec_text, exporter, solc, front, ast_ns):
    """This run's export and AST of the bytes `data` of `src` (a relative path the checkout's
    manifest names; `snapshot_source` checked it): solar-verify's bundle of `cname` and solidity-lean's
    front end's AST, from one copy of those bytes in a temporary directory, whose SHA-256 the
    export's manifest must record. `exporter` is the pinned solar-verify's (bytes, SHA-256)
    (`pinned_exporter`), `front` the front end's files (`front_end`) and `solc` the pinned solc's
    (path, version, SHA-256): each runs as a copy of the bytes hashed, the front end with its
    caches off and Python isolated. The supplied spec module's text `spec_text` must define that
    same bundle, character for character. Returns (bundle text, its SHA-256, the source's SHA-256,
    the AST body in namespace `ast_ns`), or the refusal string."""
    import hashlib
    import os

    sha = hashlib.sha256(data).hexdigest()
    if isinstance(solc, str):
        return f"the checked AST cannot be regenerated: {solc}"
    with tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as tools:
        copy = pathlib.Path(d) / src
        copy.parent.mkdir(parents=True, exist_ok=True)
        copy.write_bytes(data)
        if hashlib.sha256(copy.read_bytes()).hexdigest() != sha:
            return f"the copy of {src} is not the bytes hashed"
        # The tools run from copies of the bytes hashed, in their own directory.
        verify = pathlib.Path(tools) / "solar-verify"
        verify.write_bytes(exporter[0])
        verify.chmod(0o755)
        solc_data = pathlib.Path(solc[0]).read_bytes()
        if hashlib.sha256(solc_data).hexdigest() != solc[2]:
            return (
                f"{solc[0]} changed after it was found to be the pinned solc {solc[1]}"
            )
        solc_copy = pathlib.Path(tools) / "solc"
        solc_copy.write_bytes(solc_data)
        solc_copy.chmod(0o755)
        for name, body in front.items():
            (pathlib.Path(tools) / name).write_bytes(body)
        out = pathlib.Path(d) / "fresh-spec.lean"
        try:
            res = subprocess.run(
                [
                    str(verify),
                    "--emit-lean-spec",
                    str(out),
                    "--lean-spec-module",
                    spec_ns,
                    src,
                ],
                cwd=d,
                capture_output=True,
                text=True,
                check=False,
            )
        except OSError as e:
            return f"solar-verify does not run: {e}"
        if res.returncode != 0 or not out.is_file():
            err = (res.stderr.strip().splitlines() or ["no output"])[0]
            return f"solar-verify does not export the specs of {src}: {err[:200]}"
        fresh = out.read_text()
        env = {k: v for k, v in os.environ.items() if k != "CONTEST_CACHE_DIR"}
        env["CONTEST_DISABLE_CACHE"] = "1"
        try:
            ast = subprocess.run(
                [
                    sys.executable,
                    "-I",
                    str(pathlib.Path(tools) / FRONT_END[0]),
                    src,
                    "--contract",
                    cname,
                    "--solc",
                    str(solc_copy),
                    "--namespace",
                    ast_ns,
                    "--body-only",
                ],
                cwd=d,
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )
        except OSError as e:
            return f"the front end does not run: {e}"
        if ast.returncode != 0 or "importedSourceUnit" not in ast.stdout:
            err = (ast.stderr.strip().splitlines() or ["no output"])[-1]
            return f"the front end cannot read {src}: {err[:200]}"
    if manifest_of(fresh) != (src, sha):
        return f"solar-verify's export of {src} does not record its SHA-256 {sha}"
    mine = bundle_block(fresh, cname)
    if mine is None:
        return f"solar-verify exported no bundle `{cname}` from {src}"
    theirs = bundle_block(spec_text, cname)
    if theirs != mine:
        if theirs is None:
            return f"the spec module defines no bundle `{cname}`"
        a, b = theirs.splitlines(), mine.splitlines()
        k = next(
            (i for i in range(min(len(a), len(b))) if a[i] != b[i]), min(len(a), len(b))
        )
        got = a[k].strip() if k < len(a) else "(nothing)"
        want = b[k].strip() if k < len(b) else "(nothing)"
        return (
            f"the bundle is not solar-verify's export of {src} in this run: line {k + 1} of "
            f"`{cname}` is {got[:120]!r}, the export's is {want[:120]!r}"
        )
    # The source's path label is dropped: the AST is compared, not where it was read from.
    body = "\n".join(
        l
        for l in ast.stdout.splitlines()
        if not l.startswith("def importedSourceName :")
    )
    return mine, hashlib.sha256(mine.encode()).hexdigest(), sha, body


def tool_hashes(exporter, front, solc):
    """The SHA-256s the receipt records of the tools that read the source bytes: solar-verify, each
    file of the front end, and solc."""
    import hashlib

    return {
        "exporterSha256": exporter[1],
        "astToolSha256": hashlib.sha256(front[FRONT_END[0]]).hexdigest(),
        "astToolCacheSha256": hashlib.sha256(front[FRONT_END[1]]).hexdigest(),
        "solcSha256": solc[2],
    }


# The canonical type checker on a source unit, with its instance given rather than found: an import
# that adds an instance of higher priority cannot change what `source_is_fresh` states.
CHECKED_CONTRACT = (
    "@SolidCore.Solidity.TypeCheck.CheckedInput.contract SolidCore.Solidity.TypeCheck.SourceUnitAst\n"
    "      SolidCore.Solidity.TypeCheck.checkedInputSourceUnit"
)


def fresh_block(
    mod, ns, bundle, cname, src, sha, block, bsha, solc_ver, hashes, usings
):
    """--loops: this run's export of the source bytes as the literal `exported`, its hash, the
    SHA-256s of the source bytes and of the tools that read them (`hashes`, `tool_hashes`), and the
    kernel's checks that the spec module's bundle is `exported` and that the core module's checked
    contract is the canonical type checker (its instance given explicitly) on the front end's AST
    `FreshSource` of the same bytes, and that the same checker on an empty source unit gives
    another value (`source_is_fresh_discriminates`)."""
    body = block.split("\n", 1)[1].rstrip("\n")
    return [
        "/-- The SHA-256 of the source bytes this run hashed, exported and translated. -/",
        f'def sourceSha256 : String := "{sha}"',
        "",
        "/-- The SHA-256 of the solar-verify binary that exported them, pinned by spec_connect.py's",
        "--solar-verify-sha256 and run as a copy of the bytes hashed. -/",
        f'def exporterSha256 : String := "{hashes["exporterSha256"]}"',
        "",
        "/-- The SHA-256s of solidity-lean's front end that translated them, run as copies of the bytes",
        "hashed: scripts/solc_ast_to_lean_source.py, scripts/harness_cache.py (caches off), and the",
        f"pinned solc {solc_ver}. -/",
        f'def astToolSha256 : String := "{hashes["astToolSha256"]}"',
        f'def astToolCacheSha256 : String := "{hashes["astToolCacheSha256"]}"',
        f'def solcSha256 : String := "{hashes["solcSha256"]}"',
        "",
        f"/-- `{cname}`'s specs as solar-verify exported them in this run from the bytes of {src}",
        f"(SHA-256 {sha}): the definition's text has SHA-256 {bsha} (`exportedSha256`). -/",
        "def exported : Bundle where",
        body,
        "",
        "/-- The SHA-256 of the text of `exported`'s definition as solar-verify printed it (named for the",
        "contract, `def C : Bundle where` and its body), which spec_connect.py printed. -/",
        f'def exportedSha256 : String := "{bsha}"',
        "",
        "/-- **The bundle is this run's export of the source bytes**: the kernel checks that the spec",
        "module's bundle is the literal `exported`, so a bundle with an ensures, an invariant or any other",
        "clause removed, added or changed fails here. -/",
        f"theorem spec_is_fresh : {bundle} = exported := rfl",
        "",
        "/-- **The checked contract is the front end's AST of the source bytes**: the core module's",
        "`contract?` is the type checker on `FreshSource.importedSourceUnit`, which solidity-lean's front end",
        f"(scripts/solc_ast_to_lean_source.py, solc {solc_ver}) wrote in this run from the bytes above. The",
        "type checker is the canonical one, `checkedInputSourceUnit`, given explicitly. -/",
        f"theorem source_is_fresh : {ns}.contract? =",
        f'    {CHECKED_CONTRACT} {mod}.FreshSource.importedSourceUnit "{cname}" :=',
        "  rfl",
        "",
        "/-- Whether the core module's checked contract is a contract, as the kernel evaluates it. -/",
        f"def contractOk? : Bool := ({ns}.contract?).isOk",
        "",
        f"kernel_rfl contractOk_eq contractOk? true using {usings}",
        "",
        "/-- Whether the canonical type checker gives a contract on a source unit without it. -/",
        "def emptyOk? : Bool :=",
        f'  ({CHECKED_CONTRACT} ({{ items := [] }} : SolidCore.Solidity.SourceUnit) "{cname}").isOk',
        "",
        f"kernel_rfl emptyOk_eq emptyOk? false using {usings}",
        "",
        "/-- The equality tells source units apart: the canonical type checker on a source unit without",
        "the contract (a changed AST) gives no contract, while the core module's `contract?` is one. -/",
        f"theorem source_is_fresh_discriminates : {ns}.contract? ≠",
        f'    {CHECKED_CONTRACT} ({{ items := [] }} : SolidCore.Solidity.SourceUnit) "{cname}" := by',
        "  intro h",
        "  have hl := contractOk_eq",
        "  have hr := emptyOk_eq",
        "  unfold contractOk? at hl",
        "  unfold emptyOk? at hr",
        "  rw [h, hr] at hl",
        "  exact Bool.false_ne_true hl",
        "",
        f"/-- info: '{mod}.spec_is_fresh' does not depend on any axioms -/",
        "#guard_msgs in",
        "#print axioms spec_is_fresh",
        "",
        f"/-- info: '{mod}.source_is_fresh' depends on axioms: [propext, Classical.choice, Quot.sound] -/",
        "#guard_msgs in",
        "#print axioms source_is_fresh",
        "",
    ]


def duplicate_names(fn_lines):
    """The spec names more than one function spec has (overloads): probe records are keyed by name."""
    seen, dup = set(), set()
    for line in fn_lines:
        name = line.split(" ", 2)[1]
        (dup if name in seen else seen).add(name)
    return sorted(dup)


def loop_block(
    fid, name, i, n, sel, ws, tree, ns, mod, bundle, usings, ghost_fixed, tree_only
):
    """--loops: `fid`'s loop erasure's tree, the kernel's check of it, `fid`'s spec on the tree and
    the loop route's source and spec premises (`TreeLoop.LoopSpec`, Verify/Spec/TreeLoopCheck.lean)."""
    meets = "TreeMeetsFixed" if ghost_fixed else "TreeMeets"
    tactic = "tree_meets_fixed" if ghost_fixed else "tree_meets"
    of_check = "loopSpec_of_check_fixed" if ghost_fixed else "loopSpec_of_check"
    out = [
        f"/-- The loop erasure's tree of `{name}` (`eraseLoopStmt`, then `brTree`): the generic stage",
        "does not erase it. -/",
        f"def {fid}_tree : Tree :=",
        f"  {tree}",
        "",
        f"/-- How SolidCore dispatches `{name}`: {n} parameters of widths {ws}, word returns, the",
        f"loop erasure's tree, and `spec` the contract's export with `{name}`'s spec at index {i}",
        "(`spec` is `exported`, this run's export of the source bytes: `spec_is_fresh`). -/",
        f"def {fid}_loop? : Bool :=",
        f"  TreeLoop.contractLoopCheck {ns}.contract? {ns}.fuel {sel} {n} {ws} {fid}_tree spec {i}",
        "",
        f"kernel_rfl {fid}_loop_eq {fid}_loop? true using {usings}",
        "",
        f"/-- `{name}`'s exported spec. -/",
        f"def {fid}_spec : FnSpec := spec.spec.specs[{i}]'(by decide)",
        "",
    ]
    if tree_only:
        return out
    return out + [
        f"/-- `{name}`'s spec holds of its loop erasure's tree. -/",
        f"theorem {fid}_meets : {meets} spec {fid}_spec ctx {n} {fid}_tree := by",
        f"  {tactic} [{fid}_tree, {fid}_spec, {mod}.spec, {bundle}, {mod}.ctx, UnOp.eval]",
        "",
        f"/-- **`{name}`'s source and spec premises of the loop route**",
        "(`Verify.LoopSource.Equiv.LeastFuel.eraseLoop_certified`): SolidCore's function of its selector,",
        "its widths, word returns, its loop erasure to the tree above, the exported spec's allowed",
        "statuses and postcondition (`Shapes.ctxOfCall`, `Shapes.idStep`: sender 0x2000, value 0,",
        "timestamp 1, self 0x1000) on that tree, and `spec` the contract's export (`ExportedFor`), whose",
        "contents are this run's export of the source bytes (`spec := exported`, `spec_is_fresh`). -/",
        f"theorem {fid}_loop_spec {{c : SolidCore.Solidity.TypeCheck.CheckedContract}} (hc : {ns}.contract? = .ok c) :",
        f"    TreeLoop.LoopSpec c {ns}.fuel {sel} {ws} {fid}_tree spec :=",
        f"  TreeLoop.{of_check} hc {fid}_loop_eq (by decide)",
        f"    (TreeLoop.meets_of_index (P := fun fs => {meets} spec fs TreeLoop.ctx0 {n} {fid}_tree) {i}",
        f"      (by decide) (by decide) {fid}_meets)",
        "",
        f"/-- info: '{mod}.{fid}_loop_spec' depends on axioms: [propext, Classical.choice, Quot.sound] -/",
        "#guard_msgs in",
        f"#print axioms {fid}_loop_spec",
        "",
    ]


def from_block(
    fid, name, n, nsi, ns, mod, bundle, ws, usings, payable=False, tree_only=False
):
    """--any-caller: `fid`'s spec of SolidCore's run sent by every account `a < 2 ^ 160` other than the
    contract, each in its own context (`ctxOfCallFrom a`), at every value below `2 ^ 256`
    (`SolidCore.Spec.holdsFn_of_tree_from`, Verify/Spec/TreeFrom.lean), from the symbolic-caller
    tree `{fid}_tree_from`. No theorem of it names the fixed `ctx`, `ctxOf` or a literal caller;
    what each theorem proves is checked by the kernel when the module builds."""
    ds = f"{fid}_tree_from, {fid}_spec, {mod}.spec, {bundle}, SolidCore.Spec.ctxAt, hca"
    ds0 = f"{fid}_tree_from, {fid}_spec, SolidCore.Spec.FnSpec.noSucc, {mod}.spec, {bundle}, SolidCore.Spec.ctxAt, hca"
    payable = "true" if payable else "false"
    wsl = "none" if ws is None else f"some {ws}"
    caller = (
        "  intro a ha hne\n  have hca : a % 2 ^ 256 = a := Nat.mod_eq_of_lt (by omega)"
    )
    meets0 = f"{caller}\n  tree_meets_fixed [{ds0 if nsi else ds}]"
    out = [
        f"/-- The stage's erasure of `{name}` for every caller (`msg.sender` the symbolic word). -/",
        f"def {fid}_tree_from : Tree :=",
        "  @TREE@",
        "",
        f"/-- How SolidCore dispatches `{name}`, the erasure for every caller, whether it is payable and",
        "its argument widths. -/",
        f"def {fid}_erase_from? : Option (Nat × Option Tree × Bool × Option (List Nat)) :=",
        f"  match {ns}.contract? with",
        f"  | .ok c => (c.core.findFunctionBySelector? (fn_{fid}).sel).map fun f =>",
        f"      (f.params.length, eraseFnSym (envOf c.core) {ns}.fuel f, f.payable, widthsOf f.params f.paramAbiCleanups)",
        "  | .error _ => none",
        "",
        f"kernel_rfl {fid}_erase_from_eq {fid}_erase_from? (some ({n}, some {fid}_tree_from, {payable}, {wsl})) using {usings}",
        "",
    ]
    if tree_only:
        return out
    if nsi:
        out += [
            "set_option maxHeartbeats 4000000 in",
            f"/-- `{name}`'s spec without its succeeds-if conditions, from every caller. -/",
            f"theorem {fid}_meets0_from : TreeMeetsFrom spec {fid}_spec.noSucc {n} {fid}_tree_from := by",
            meets0,
            "",
            "set_option maxHeartbeats 4000000 in",
            "set_option linter.unusedTactic false in",
            "set_option linter.unreachableTactic false in",
            "set_option linter.unnecessarySeqFocus false in",
            f"/-- `{name}`'s succeeds-if conditions, from every caller. -/",
            f"theorem {fid}_succeeds_from : ∀ a, a < 2 ^ 160 → a ≠ selfAddress →",
            f"    TreeSucceedsAt a spec {fid}_spec (ctxAt a 0) {n} {fid}_tree_from := by",
            caller,
            tree_succeeds_tolerant(ds).rstrip(),
            "",
            f"/-- `{name}`'s spec, from every caller. -/",
            f"theorem {fid}_meets_from : TreeMeetsFrom spec {fid}_spec {n} {fid}_tree_from :=",
            f"  fun a ha hne => treeMeetsFixedAt_of_succeeds ({fid}_meets0_from a ha hne) ({fid}_succeeds_from a ha hne)",
            "",
        ]
    else:
        out += [
            "set_option maxHeartbeats 4000000 in",
            f"/-- `{name}`'s spec, from every caller. -/",
            f"theorem {fid}_meets_from : TreeMeetsFrom spec {fid}_spec {n} {fid}_tree_from := by",
            meets0,
            "",
        ]
    out += [
        f"/-- No call of `{name}` with a value other than 0 meets its succeeds-if conditions. -/",
        f"theorem {fid}_value_refuses : ValueRefuses spec {fid}_spec := by",
        "  intro a v iargs pre hv0 hv hne hsat",
        "  first",
        f"  | (simp [{fid}_spec, {mod}.spec, {bundle}] at hne; done)",
        f"  | (try simp only [Bundle.satAll, Bundle.sat, {fid}_spec, {mod}.spec, {bundle},",
        "        List.getElem_cons_zero, List.getElem_cons_succ, List.mem_cons, List.not_mem_nil, or_false,",
        "        forall_eq_or_imp, forall_eq, eval, mkEnv, Env.cur, SolidCore.Spec.ctxAt, BinOp.apply,",
        "        ofProp_ne_zero, decide_eq_true_eq] at hsat",
        "     simp_all; done)",
        "",
        f"/-- **`{name}`'s spec holds of SolidCore's checked contract from every caller, at every value**",
        "(the contract's address and the block's timestamp those of `ctxOfCallFrom`). -/",
        f"theorem {fid}_holds_from {{c : SolidCore.Solidity.TypeCheck.CheckedContract}} (hc : {ns}.contract? = .ok c)",
        f"    (step : GhostStep) (hstep : ∀ call, valueScope fn_{fid} deployed call → ∀ g, step call g = g) :",
        "    ∀ a, a < 2 ^ 160 → a ≠ selfAddress →",
        f"      HoldsFn (solidCoreFrom a {ns}.fuel) c (valueScope fn_{fid} deployed) spec",
        f"        (SolidCore.Verify.AnyCaller.ctxOfCallFrom a) step {fid}_spec := by",
        "  obtain ⟨dd, hdd⟩ := deploy_ok hc",
        f"  have hmap := {fid}_erase_from_eq",
        f"  simp only [{fid}_erase_from?, hc, Option.map_eq_some_iff, Prod.mk.injEq] at hmap",
        "  obtain ⟨f, hf, hlen, hT, hpay, hw⟩ := hmap",
        f"  exact holdsFn_of_tree_from hdd (by decide) hf (by simpa [fn_{fid}] using hlen) hT hw hpay hstep",
        f"    (by simpa [fn_{fid}] using {fid}_meets_from) {fid}_value_refuses",
        "",
        f"/-- info: '{mod}.{fid}_holds_from' depends on axioms: [propext, Classical.choice, Quot.sound] -/",
        "#guard_msgs in",
        f"#print axioms {fid}_holds_from",
        "",
    ]
    return out


def succeeds_block(fid, n, hyp, proof, lits, nests, meets, mod, bundle):
    """A function with succeeds-if conditions: the spec without them (the same tactic, with the
    conditions removed by FnSpec.noSucc), the conditions apart (TreeSucceeds), and both put
    together (Verify/Spec/TreeSucceeds.lean)."""
    ds = f"{fid}_tree, {fid}_spec, {mod}.spec, {bundle}, {mod}.ctx"
    use = " hsep" if hyp else ""
    if nests:
        sproof = f"  have hsep' := NestedSlotsSeparated.map hsep\n  tree_succeeds_map [{ds}] hsep'"
    elif lits:
        sproof = f"  tree_succeeds_map [{ds}] hsep"
    else:
        sproof = tree_succeeds_tolerant(ds)
    if meets == "TreeMeetsFixed" and not nests:
        # tree_meets_map', which tolerates simplification steps that change nothing (a spec
        # with no invariants); with no mapping, from the separation of no slots.
        sep = "hsep" if lits else "SolidCore.Spec.mapSlotsSeparated_nil"
        ds0 = f"{fid}_tree, {fid}_spec, SolidCore.Spec.FnSpec.noSucc, {mod}.spec, {bundle}, {mod}.ctx"
        proof0 = f"  tree_meets_map' [{ds0}] {sep}"
        if not lits:
            # A tree with no branch (a function that only writes, such as one setting a slot to a
            # constant) leaves tree_meets_map' no if-then-else to split; tree_meets_fixed proves it.
            proof0 = f"  first\n  | (tree_meets_map' [{ds0}] {sep})\n  | (tree_meets_fixed [{ds0}])"
    else:
        proof0 = proof.replace("_spec,", "_spec, SolidCore.Spec.FnSpec.noSucc,", 1)
    comb = (
        "treeMeetsFixed_of_succeeds"
        if meets == "TreeMeetsFixed"
        else "treeMeets_of_succeeds"
    )
    return [
        "set_option maxHeartbeats 4000000 in",
        f"/-- `{fid}`'s spec without its succeeds-if conditions holds of its erased tree. -/",
        f"theorem {fid}_meets0{hyp} : {meets} spec {fid}_spec.noSucc ctx {n} {fid}_tree := by",
        proof0,
        "",
        "set_option maxHeartbeats 4000000 in",
        "set_option linter.unusedTactic false in",
        "set_option linter.unreachableTactic false in",
        "set_option linter.unnecessarySeqFocus false in",
        f"/-- From a pre-state meeting `{fid}`'s succeeds-if conditions, its erased tree succeeds. -/",
        f"theorem {fid}_succeeds{hyp} : TreeSucceeds spec {fid}_spec ctx {n} {fid}_tree := by",
        sproof,
        "",
        f"/-- `{fid}`'s spec holds of its erased tree. -/",
        f"theorem {fid}_meets{hyp} : {meets} spec {fid}_spec ctx {n} {fid}_tree :=",
        f"  {comb} ({fid}_meets0{use}) ({fid}_succeeds{use})",
        "",
    ]


def failing_theorems(text, out_lines):
    """The theorems of the written module that the errors in Lean's output text fall in, with
    each one's first error, in order: (theorem, line, message)."""
    found = {}
    for m in re.finditer(r":(\d+):\d+: error: (.*)", text):
        if "#guard_msgs" in m.group(2):
            # The axiom checks at the end fail only because a theorem above them did.
            continue
        line = int(m.group(1))
        k = min(line, len(out_lines)) - 1
        # Lean places a command's error at its first line: a set_option ... in prefix or the doc
        # comment, which sit directly above the theorem (no blank line between).
        if 0 <= k and out_lines[k].startswith(("set_option ", "/--")):
            j = k
            while (
                j < len(out_lines)
                and out_lines[j]
                and not out_lines[j].startswith("theorem ")
            ):
                j += 1
            if j < len(out_lines) and out_lines[j].startswith("theorem "):
                k = j
        thm = next(
            (
                re.match(r"theorem (\S+)", out_lines[j]).group(1)
                for j in range(k, -1, -1)
                if out_lines[j].startswith("theorem ")
            ),
            "the module header",
        )
        found.setdefault(thm, (line, m.group(2).strip()))
    return [(t, l, msg) for t, (l, msg) in found.items()]


def check_output(args):
    """Elaborate the written module before any theorem is named as connected, so that a false spec
    is refused here, by the theorem that does not check, rather than later as a build error of the
    published statement's proof."""
    res = subprocess.run(
        shlex.split(args.lean) + [str(args.out)],
        cwd=args.cwd,
        capture_output=True,
        text=True,
        check=False,
    )
    text = res.stdout + res.stderr
    if res.returncode == 0 and "sorryAx" not in text and ": error" not in text:
        return
    bad = failing_theorems(text, args.out.read_text().splitlines())
    sys.stderr.write(text[-4000:])
    if bad:
        thm, line, msg = bad[0]
        more = f" (and {', '.join(t for t, _, _ in bad[1:])})" if len(bad) > 1 else ""
        sys.exit(
            f"spec_connect: REJECTED: spec: {thm} does not check{more}: {msg} ({args.out.name}:{line})"
        )
    sys.exit(
        f"spec_connect: REJECTED: spec: {args.out.name} does not check (exit {res.returncode})"
    )


def refutation_verdict(args, fns, attempt):
    """Elaborate the module with its refutations. When it checks, exit REJECTED, naming each refuted
    theorem and its kernel-checked refutation; when it does not, return (the proofs decide)."""
    res = subprocess.run(
        shlex.split(args.lean) + [str(args.out)],
        cwd=args.cwd,
        capture_output=True,
        text=True,
        check=False,
    )
    text = res.stdout + res.stderr
    if res.returncode != 0 or "sorryAx" in text or ": error" in text:
        # The module with the refutations and Lean's output stay beside OUT for review.
        shutil.copy(args.out, args.out.with_suffix(".refute.lean"))
        args.out.with_suffix(".refute.log").write_text(text)
        sys.stderr.write(
            "spec_connect: a proposed counterexample's refutation does not check; the proofs decide "
            f"({args.out.with_suffix('.refute.log').name})\n"
        )
        return
    order = {
        "unsat": 0,
        "init": 1,
        "meets0inv": 2,
        "meets0": 2,
        "succeeds": 3,
        "value": 4,
    }
    why = {
        "meets0inv": lambda k: f": an invariant is not preserved by {k}",
        "value": lambda k: (
            f": {k} is nonpayable, so the dispatcher reverts a call with a value, but its "
            "succeeds-if conditions admit value 1 (ValueRefuses)"
        ),
    }
    nsis = {fid: nsi for _i, _name, fid, _n, _sel, _tree, _slots, nsi, *_ in fns}
    refs = sorted(
        ((order[c[0]], k, c) for k, cs in attempt.items() for c in cs),
        key=lambda x: x[0],
    )
    lines = []
    for _, k, (kind, a, ia, ps) in refs:
        what, thm = refuted_name(k, kind, nsis.get(k, 0))
        where = (
            f", on the call with argument words {a} from the storage {ps}"
            if a is not None
            else f", at value 1 with arguments {ia} from the storage {ps}"
            if kind == "value"
            else ""
        )
        if kind in ("unsat", "init"):
            lines.append(
                f"spec: {what}: refuted in the kernel by {args.module_name}.{thm}"
            )
        else:
            lines.append(
                f"spec: {what} does not check{why.get(kind, lambda _: '')(k)}: refuted in the kernel by "
                f"{args.module_name}.{thm}{where}"
            )
    for l in lines[1:]:
        sys.stderr.write(f"spec_connect: also: {l}\n")
    sys.exit(f"spec_connect: REJECTED: {lines[0]} ({args.out.name})")


def write_receipt(args, cname, src, fresh, hashes):
    """--loops: OUT.receipt.json beside the module, with what its receipt records (the source and
    the SHA-256s of its bytes, of the exported bundle's text and of each tool that read the bytes)
    and the SHA-256 of the module text Lean checked."""
    import hashlib
    import json

    rec = {
        "module": args.module_name,
        "contract": cname,
        "source": src,
        "sourceSha256": fresh[2],
        "exportedSha256": fresh[1],
        **hashes,
        "moduleSha256": hashlib.sha256(args.out.read_bytes()).hexdigest(),
    }
    path = args.out.with_name(args.out.name + ".receipt.json")
    path.write_text(json.dumps(rec, indent=2, sort_keys=True) + "\n")
    print(f"spec_connect: receipt {path}", file=sys.stderr)


def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("--core-module", required=True)
    ap.add_argument("--namespace", required=True)
    ap.add_argument("--spec-module", required=True)
    ap.add_argument("--bundle", required=True)
    ap.add_argument("--module-name", required=True)
    ap.add_argument("--out", required=True, type=pathlib.Path)
    ap.add_argument("--lean", required=True)
    ap.add_argument("--cwd", default=".")
    ap.add_argument(
        "--ghost-fixed",
        action="store_true",
        help="state each function's spec with the ghost state unchanged by its calls "
        "(TreeMeetsFixed), and take that of the ghost step as a premise of _holds",
    )
    ap.add_argument(
        "--only",
        default="",
        help="comma-separated spec function names (such as Token.transfer) to connect; default all",
    )
    ap.add_argument(
        "--using",
        action="append",
        default=[],
        help="an extra equation a = b for kernel_rfl's checks (repeatable), such as a table of the "
        "constructor's Keccak-256 hashes; with --import for its module",
    )
    ap.add_argument("--import", dest="imports", action="append", default=[])
    ap.add_argument(
        "--any-caller",
        action="store_true",
        help="also prove each function's spec from every caller at every value (_holds_from, "
        "Verify/Spec/TreeFrom.lean), with the ghost state unchanged by its calls as a premise",
    )
    ap.add_argument(
        "--loops",
        action="store_true",
        help="connect functions the generic stage does not erase through the loop erasure "
        "(eraseLoopStmt) and the loop route's premises (_loop_spec, Verify/Spec/TreeLoopCheck.lean)",
    )
    ap.add_argument(
        "--solar-verify",
        default=None,
        help="with --loops: the solar-verify that exports the source's specs in this run "
        "(default: SOLAR_VERIFY; required)",
    )
    ap.add_argument(
        "--solar-verify-sha256",
        default=None,
        help="with --loops: the SHA-256 that solar-verify's bytes must have; the driver runs a "
        "copy of exactly those bytes (default: SOLAR_VERIFY_SHA256; required)",
    )
    ap.add_argument(
        "--solc",
        default=None,
        help="with --loops: the pinned solc for the front end (default: SOLC, else the checkout's "
        ".cache, else ~/.cache/solc)",
    )
    ap.add_argument(
        "--tree-only",
        action="store_true",
        help="write only the trees, their erasure checks and the specs (no _meets, no _holds)",
    )
    args = ap.parse_args()
    only = {x for x in args.only.split(",") if x}

    sym = (
        PROBE_SYM.format(ns=args.namespace, bundle=args.bundle)
        if args.any_caller
        else ""
    ) + (PROBE_LOOP.format(ns=args.namespace, bundle=args.bundle) if args.loops else "")

    probe = PROBE.format(
        core=args.core_module,
        spec=args.spec_module,
        ns=args.namespace,
        bundle=args.bundle,
        sym=sym,
        sym_import=(
            "import SolidCore.Refinement.Generic.StageFrom\n" if args.any_caller else ""
        )
        + ("import Verify.LoopSource.Proof.EraseLoop\n" if args.loops else ""),
    )
    with tempfile.NamedTemporaryFile(
        "w", suffix=".lean", dir=args.cwd, delete=False
    ) as f:
        f.write(probe)
        probe_path = pathlib.Path(f.name)
    try:
        res = subprocess.run(
            shlex.split(args.lean) + [str(probe_path)],
            cwd=args.cwd,
            capture_output=True,
            text=True,
            check=False,
        )
    finally:
        probe_path.unlink()
    lines = [
        l[len("SPEC_CONNECT ") :]
        for l in res.stdout.splitlines()
        if l.startswith("SPEC_CONNECT ")
    ]
    if res.returncode != 0 or not lines:
        sys.exit(f"spec_connect: Lean's probe failed:\n{res.stdout}\n{res.stderr}")
    if lines[0].startswith("error"):
        sys.exit(f"spec_connect: {lines[0][6:]}")
    names = None
    if lines[0].startswith("names "):
        bname, cname = re.fullmatch(
            r'names ("(?:[^"\\]|\\.)*") ("(?:[^"\\]|\\.)*")', lines[0]
        ).groups()
        names = (lean_string(bname), lean_string(cname))
        lines = lines[1:]
    m = re.fullmatch(r"deployed some (.*)", lines[0], re.DOTALL)
    if not m and not args.loops:
        sys.exit(
            f"spec_connect: the constructor does not deploy without logs: {lines[0]}"
        )
    # With --loops a constructor that does not deploy refuses only the generic stage's functions:
    # the loop route is runtime-only.
    deployed = m.group(1) if m else None

    spec_path = pathlib.Path(args.cwd) / (args.spec_module.replace(".", "/") + ".lean")
    if args.loops:
        # The export may sit under a library's source directory (Certified/).
        spec_path = module_path(args.cwd, args.spec_module) or spec_path
    spec_slots = spec_mapping_slots(spec_path)
    source_def = fresh = fresh_src = solc = hashes = sources = manifest0 = None
    if args.loops:
        # The loop route states the spec of the bundle it names: that bundle must be this
        # contract's export, from the file the contract was imported from. The source is read
        # once: the provenance gate, the export, the AST and the receipt are all of those bytes.
        import os

        why_pre = "spec_connect: refused (unproved): the exported spec: "
        spec_text = spec_path.read_text() if spec_path.exists() else ""
        sources = core_source(args.cwd, args.core_module)
        manifest0 = manifest_of(spec_text)
        snapshot = snapshot_source(args.cwd, manifest0[0]) if manifest0 else None
        if isinstance(snapshot, str):
            sys.exit(why_pre + snapshot)
        why = provenance_refusal(names, manifest0, sources, args.cwd, snapshot)
        if why:
            sys.exit(why_pre + why)
        source_def = sources[0][0]
        fresh_src = manifest0[0]
        # The tools that read the bytes, each pinned or hashed and run as a copy of what was hashed.
        exporter = pinned_exporter(
            args.solar_verify or os.environ.get("SOLAR_VERIFY"),
            args.solar_verify_sha256 or os.environ.get("SOLAR_VERIFY_SHA256"),
        )
        if isinstance(exporter, str):
            sys.exit(why_pre + exporter)
        front = front_end(args.cwd)
        if isinstance(front, str):
            sys.exit(why_pre + front)
        solc = pinned_solc(args.cwd, args.solc)
        fresh = fresh_export(
            fresh_src,
            snapshot,
            names[1],
            args.bundle.rsplit(".", 1)[0],
            spec_text,
            exporter,
            solc,
            front,
            f"{args.module_name}.FreshSource",
        )
        if isinstance(fresh, str):
            sys.exit(why_pre + fresh)
        hashes = tool_hashes(exporter, front, solc)
        print(
            f"spec_connect: exported bundle {names[1]} SHA-256 {fresh[1]} "
            f"(source SHA-256 {fresh[2]}, solar-verify SHA-256 {hashes['exporterSha256']}, "
            f"front end SHA-256 {hashes['astToolSha256']})",
            file=sys.stderr,
        )
    syms = {}
    for line in [x for x in lines[1:] if x.startswith("sym ")]:
        parsed = parse_sym(line)
        syms[parsed[0]] = parsed[1:]
    loops = {}
    for line in [x for x in lines[1:] if x.startswith("loop ")]:
        loops[line.split(" ", 2)[1]] = parse_loop(line)
    lines = [lines[0]] + [
        x for x in lines[1:] if not x.startswith("sym ") and not x.startswith("loop ")
    ]
    fns, refused = [], []
    dups = duplicate_names(lines[1:]) if args.loops else []
    for name in dups:
        if not only or name in only:
            refused.append(
                f"{name}: several function specs share the name (overloads), which --loops keys by name"
            )
    for k, line in enumerate(lines[1:]):
        parts = line.split(" ", 5)
        name = parts[1]
        if (only and name not in only) or name in dups:
            continue
        if parts[2] == "none":
            refused.append(f"{name}: no function of its selector")
            continue
        if parts[3] == "noerase":
            if not args.loops:
                refused.append(f"{name}: outside the generic source stage")
                continue
            why = loop_refusal(loops.get(name), int(parts[2]), spec_slots, args)
            if why:
                refused.append(f"{name}: {why}")
                continue
            _, sel, ws, tree = loops[name]
            fns.append(
                (k, name, ident(name), int(parts[2]), sel, tree, ([], []), 0, ws)
            )
            continue
        if deployed is None:
            refused.append(
                f"{name}: the constructor does not deploy without logs, which _holds needs: {lines[0]}"
            )
            continue
        n, nsi, sel = int(parts[2]), int(parts[3]), int(parts[4])
        if n > 8:
            refused.append(f"{name}: more than eight parameters")
            continue
        tree = parts[5].removeprefix("TREE ")
        slots = mapping_slots(tree)
        if slots is None:
            refused.append(f"{name}: hashes a key into a slot that is not a literal")
            continue
        # The spec's own mapping reads (frame clauses, invariants) need their slots apart too.
        lits = (set(slots[0]) | spec_slots[0]) - (set(slots[1]) | spec_slots[1])
        slots = (sorted(lits), sorted(set(slots[1]) | spec_slots[1]))
        if (slots[0] or slots[1]) and not args.ghost_fixed and not args.tree_only:
            refused.append(
                f"{name}: reads mappings, which tree_meets_map proves only with --ghost-fixed"
            )
            continue
        if args.any_caller:
            why = any_caller_refusal(syms.get(name), slots, args.tree_only)
            if why:
                refused.append(f"{name}: {why}")
                continue
        fns.append((k, name, ident(name), n, sel, tree, slots, nsi, None))
    if refused:
        sys.exit("spec_connect: refused (unproved):\n" + "\n".join(refused))

    # A counterexample the search proposes is refuted in the kernel first; when its refutation does
    # not check, the module is written again without it and the proofs decide.
    cex = (
        search_cex(args, fns, deployed)
        if args.ghost_fixed and not args.tree_only and source_def is None
        else {}
    )
    for attempt in [cex, {}] if cex else [{}]:
        ns = args.namespace
        # With --loops every theorem names this run's export (`exported`), which the kernel checks the
        # spec module's bundle against (`spec_is_fresh`).
        bundle_ref = (
            f"{args.module_name}.exported" if source_def is not None else args.bundle
        )
        usings = " ".join(
            [f"{ns}.selector_eq", "SolidCore.Verify.VC.mkIdx_eq", *args.using]
        )
        out = [
            "-- Generated by solar's tools/certify/spec_connect.py. Do not edit.",
            f"import {args.core_module}",
            f"import {args.spec_module}",
            "import Verify.Spec.Tree",
            "import Verify.Spec.TreeMap",
            "import Verify.Spec.TreeMapNested",
            "import Verify.Spec.TreeIn",
            "import Verify.Spec.TreeSucceeds",
            "import Verify.Spec.TreeSucceedsTry",
            *(["import Verify.Spec.TreeLoopCheck"] if any(x[8] for x in fns) else []),
            "import Verify.VC.KernelIdx",
            "import SolidCore.Refinement.KernelRfl",
            *(["import SolidCore.Solidity.Checked"] if source_def is not None else []),
            *(
                ["import Verify.Spec.TreeFrom"]
                if args.any_caller
                or any(c[0] == "value" for v in attempt.values() for c in v)
                else []
            ),
            *[f"import {m}" for m in args.imports],
            "",
            "/-!",
            f"# {args.bundle}, of SolidCore's checked contract, from the erased trees",
            "",
            "For each function of the exported spec: the generic source stage's tree (`_erase_eq`, `deploy_eq`),",
            "the spec of the tree (`_meets`) and the spec of SolidCore's contract on the stage's calls",
            "(`_holds`, `SolidCore.Spec.holdsFn_of_tree`). See tools/certify/spec_connect.py.",
            *(
                [
                    "Functions the generic stage does not erase are connected through their loop erasure instead",
                    "(`_loop_eq`, `_meets`, `_loop_spec`; Verify/Spec/TreeLoopCheck.lean).",
                ]
                if any(x[8] for x in fns)
                else []
            ),
            "-/",
            "",
            *(
                [
                    "/-! The front end's AST of the source bytes this run hashed (`source_is_fresh`). -/",
                    "section",
                    "open SolidCore.Solidity",
                    "",
                    fresh[3],
                    "",
                    "end",
                    "",
                ]
                if source_def is not None
                else []
            ),
            f"namespace {args.module_name}",
            "",
            "open SolidCore.Spec SolidCore.Refinement SolidCore.Refinement.Generic",
            "",
            *(
                fresh_block(
                    args.module_name,
                    args.namespace,
                    args.bundle,
                    names[1],
                    fresh_src,
                    fresh[2],
                    fresh[0],
                    fresh[1],
                    solc[1],
                    hashes,
                    usings,
                )
                if source_def is not None
                else []
            ),
            "/-- The exported spec"
            + (
                ": this run's export of the source bytes (`spec_is_fresh`). -/"
                if source_def is not None
                else ". -/"
            ),
            f"abbrev spec : Bundle := {'exported' if source_def is not None else args.bundle}",
            "",
            "/-- The call context of the stage's calls (no value), as SolidCore's runs see it. -/",
            "def ctx : Ctx := { sender := 0x2000, value := 0, timestamp := 1, self := 0x1000 }",
            "",
            "/-- The call context of every call: `ctx` with the call's value. -/",
            "def ctxOf (call : SolarTV.Call) : Ctx := { ctx with value := call.callvalue }",
            "",
            *(REFUTE_LEMMAS if attempt else []),
        ]
        if source_def is not None:
            manifest = args.bundle.rsplit(".", 1)[0] + ".manifest"
            out += [
                "/-- The export's manifest names the file SolidCore's contract was imported from, with the SHA-256",
                "of the bytes spec_connect.py hashed and exported (`exported`, `FreshSource`) in this run. -/",
                f"theorem spec_source : {manifest}.source = {source_def} ∧",
                f"    {manifest}.sourceSha256 = sourceSha256 := ⟨rfl, rfl⟩",
                "",
                "/-- **The receipt**: the spec module's bundle is this run's export `exported` (`spec_is_fresh`), the",
                "core module's checked contract is the canonical type checker on this run's AST `FreshSource`",
                "(`source_is_fresh`), and the export's manifest names the file the contract was imported from, with",
                "the SHA-256 `sourceSha256` of the bytes that solar-verify (`exporterSha256`) exported and the front",
                "end (`astToolSha256`, `astToolCacheSha256`, `solcSha256`) translated. spec_connect.py checked those",
                "bytes against the manifest before the run and the file against them after Lean checked this",
                "module; replaying the module in Lean does not reread the file. -/",
                f"theorem receipt : {args.bundle} = exported ∧",
                f"    {ns}.contract? =",
                f'      {CHECKED_CONTRACT} {args.module_name}.FreshSource.importedSourceUnit "{names[1]}" ∧',
                f"    {manifest}.source = {source_def} ∧ {manifest}.sourceSha256 = sourceSha256 :=",
                "  ⟨spec_is_fresh, source_is_fresh, spec_source.1, spec_source.2⟩",
                "",
                f"/-- info: '{args.module_name}.receipt' depends on axioms: [propext, Classical.choice, Quot.sound] -/",
                "#guard_msgs in",
                "#print axioms receipt",
                "",
            ]
        if deployed is not None:
            out += [
                "/-- The storage the constructor leaves. -/",
                f"def deployed : List (Nat × Nat) := {deployed}",
                "",
                "/-- Whether the contract deploys with that storage and no logs. -/",
                "def deploy? : Bool :=",
                f"  match {args.namespace}.contract? with",
                f"  | .ok c => Wide.deployedAt {args.namespace}.fuel c deployed",
                "  | .error _ => false",
                "",
                f"kernel_rfl deploy_eq deploy? true using {usings}",
                "",
                *(
                    refutation_block(
                        "", 0, 0, attempt["@init"][0], args.module_name, args.bundle
                    )
                    if "@init" in attempt
                    else []
                ),
                "/-- The contract deploys with that storage. -/",
                f"theorem deploy_ok {{c : SolidCore.Solidity.TypeCheck.CheckedContract}} (hc : {ns}.contract? = .ok c) :",
                f"    ∃ d, Wide.DeployedAt {ns}.fuel c d deployed := by",
                "  have h := deploy_eq",
                "  simp only [deploy?, hc] at h",
                "  exact Wide.deployedAt_of h",
                "",
            ]
        meets = "TreeMeetsFixed" if args.ghost_fixed else "TreeMeets"
        tactic = "tree_meets_fixed" if args.ghost_fixed else "tree_meets"
        holds = "holdsFn_of_tree_fixed" if args.ghost_fixed else "holdsFn_of_tree"
        hstep = " hstep" if args.ghost_fixed else ""
        for i, name, fid, n, sel, tree, slots, nsi, ws in fns:
            if ws is not None:
                out += loop_block(
                    fid,
                    name,
                    i,
                    n,
                    sel,
                    ws,
                    tree,
                    ns,
                    args.module_name,
                    bundle_ref,
                    usings,
                    args.ghost_fixed,
                    args.tree_only,
                )
                continue
            mod = args.module_name
            lits, nests = slots
            use_hyp = " hsep" if lits or nests else ""
            hstep_t = (
                f" (hstep : ∀ call, SolidCore.Verify.VC.stageScope fn_{fid} deployed call → ∀ g, step call g = g)"
                if args.ghost_fixed
                else ""
            )
            if nests:
                # Mappings of mappings: keccak256's separation on both levels of slots.
                hyp = f" (hsep : NestedSlotsSeparated [{', '.join(map(str, lits))}] [{', '.join(map(str, nests))}])"
                proof = f"  tree_meets_map2 [{fid}_tree, {fid}_spec, {mod}.spec, {bundle_ref}, {mod}.ctx] hsep"
                use = f"{fid}_meets hsep"
            elif lits:
                # Mappings: the one hypothesis is keccak256's separation on their slots.
                hyp = f" (hsep : MapSlotsSeparated [{', '.join(map(str, lits))}])"
                proof = f"  tree_meets_map [{fid}_tree, {fid}_spec, {mod}.spec, {bundle_ref}, {mod}.ctx] hsep"
                use = f"{fid}_meets hsep"
            else:
                hyp = ""
                proof = f"  {tactic} [{fid}_tree, {fid}_spec, spec, {bundle_ref}]"
                use = f"{fid}_meets"
            out += [
                f"/-- `{name}` as the stage's ABI sees it (only the number of its parameters matters). -/",
                f"def fn_{fid} : SolidCore.Refinement.Abi.Fn := ⟨{sel}, List.replicate {n} (.uint 256)⟩",
                "",
                f"/-- The generic source stage's erasure of `{name}`. -/",
                f"def {fid}_tree : Tree :=",
                f"  {tree}",
                "",
                f"/-- How SolidCore dispatches and the stage erases `{name}`. -/",
                f"def {fid}_erase? : Option (Nat × Option Tree) :=",
                f"  match {ns}.contract? with",
                f"  | .ok c => (c.core.findFunctionBySelector? {sel}).map fun f => (f.params.length, eraseFn (envOf c.core) {ns}.fuel f)",
                "  | .error _ => none",
                "",
                f"kernel_rfl {fid}_erase_eq {fid}_erase? (some ({n}, some {fid}_tree)) using {usings}",
                "",
                f"/-- `{name}`'s exported spec. -/",
                f"def {fid}_spec : FnSpec := spec.spec.specs[{i}]'(by decide)",
                "",
            ]
            if fid in attempt:
                for c in attempt[fid]:
                    out += refutation_block(fid, n, nsi, c, mod, bundle_ref)
                continue
            if args.tree_only:
                if args.any_caller:
                    payable, ws, ftree = syms[name]
                    out += [
                        x.replace("@TREE@", ftree)
                        for x in from_block(
                            fid,
                            name,
                            n,
                            nsi,
                            ns,
                            mod,
                            bundle_ref,
                            ws,
                            usings,
                            payable,
                            True,
                        )
                    ]
                continue
            out += (
                succeeds_block(fid, n, hyp, proof, lits, nests, meets, mod, bundle_ref)
                if nsi
                else [
                    f"/-- `{name}`'s spec holds of its erased tree. -/",
                    f"theorem {fid}_meets{hyp} : {meets} spec {fid}_spec ctx {n} {fid}_tree := by",
                    proof,
                    "",
                ]
            ) + [
                f"/-- **`{name}`'s spec holds of SolidCore's checked contract in every world `W`** on the",
                "stage's calls of it (the function calls out nowhere, so its tree is its run in any world). -/",
                f"theorem {fid}_holds_in (W : SolidCore.Refinement.World){hyp} {{c : SolidCore.Solidity.TypeCheck.CheckedContract}}",
                f"    (hc : {ns}.contract? = .ok c) (step : GhostStep){hstep_t} :",
                f"    HoldsFn (solidCoreIn W {ns}.fuel) c (SolidCore.Verify.VC.stageScope fn_{fid} deployed) spec ctxOf step {fid}_spec := by",
                "  obtain ⟨dd, hdd⟩ := deploy_ok hc",
                f"  have hmap := {fid}_erase_eq",
                f"  simp only [{fid}_erase?, hc, Option.map_eq_some_iff, Prod.mk.injEq] at hmap",
                "  obtain ⟨f, hf, hlen, hT⟩ := hmap",
                f"  exact {holds}_in W (fn := fn_{fid}) (ctx := ctx) hdd (by decide) hf (by simpa [fn_{fid}] using hlen) hT",
                f"    (fun call h => by simp [ctxOf, ctx, h]){hstep} (by simpa [fn_{fid}] using {use})",
                "",
                # _holds_in is a published theorem of its own (every world W), so its axioms are
                # guarded on their own, not only through _holds (review 189).
                f"/-- info: '{args.module_name}.{fid}_holds_in' depends on axioms: [propext, Classical.choice, Quot.sound] -/",
                "#guard_msgs in",
                f"#print axioms {fid}_holds_in",
                "",
                f"/-- **`{name}`'s spec holds of SolidCore's checked contract** on the stage's calls of it: the",
                "closed world's case of `_holds_in`. -/",
                f"theorem {fid}_holds{hyp} {{c : SolidCore.Solidity.TypeCheck.CheckedContract}} (hc : {ns}.contract? = .ok c)",
                f"    (step : GhostStep){hstep_t} :",
                f"    HoldsFn (solidCore {ns}.fuel) c (SolidCore.Verify.VC.stageScope fn_{fid} deployed) spec ctxOf step {fid}_spec := by",
                "  dsimp only [solidCore]; rw [← Source.runIn_closed]",
                f"  exact {fid}_holds_in World.closed{use_hyp} hc step{hstep}",
                "",
                f"/-- info: '{args.module_name}.{fid}_holds' depends on axioms: [propext, Classical.choice, Quot.sound] -/",
                "#guard_msgs in",
                f"#print axioms {fid}_holds",
                "",
            ]
            if args.any_caller:
                _, ws, ftree = syms[name]
                out += [
                    x.replace("@TREE@", ftree)
                    for x in from_block(
                        fid, name, n, nsi, ns, mod, bundle_ref, ws, usings
                    )
                ]
        out += [f"end {args.module_name}", ""]
        args.out.write_text("\n".join(out).replace("`", chr(96)))
        if attempt:
            refutation_verdict(args, fns, attempt)
            continue
        check_output(args)
        break
    if source_def is not None:
        # The provenance gate again, after Lean: the file must still be the receipt's bytes.
        why = provenance_refusal(names, (fresh_src, fresh[2]), sources, args.cwd) or (
            None
            if manifest_of(spec_path.read_text()) == manifest0
            else "the spec module's manifest changed during the run"
        )
        if why:
            sys.exit(
                f"spec_connect: REJECTED: the receipt's source bytes are no longer the file's: {why}"
            )
        write_receipt(args, names[1], fresh_src, fresh, hashes)
    for _, name, fid, *_, ws in fns:
        if ws is not None:
            print(
                f"{args.module_name}.{fid}_{'tree' if args.tree_only else 'loop_spec'}"
            )
            continue
        print(f"{args.module_name}.{fid}_{'tree' if args.tree_only else 'holds'}")
        if args.any_caller:
            print(
                f"{args.module_name}.{fid}_{'tree_from' if args.tree_only else 'holds_from'}"
            )


if __name__ == "__main__":
    main()
