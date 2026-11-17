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

Nothing in the module is trusted: each theorem is checked by the kernel when the module builds,
and a function whose shape, tree or spec does not check fails the build. A function the generic
stage does not erase, or a spec with succeeds-if conditions, is refused here (exit 1) and listed
on stderr, never written as passing.
"""

import argparse
import pathlib
import re
import shlex
import subprocess
import sys
import tempfile

PROBE = """import {core}
import {spec}
import Verify.VC.Model

open SolidCore.Refinement SolidCore.Refinement.Generic SolidCore.Solidity

#eval show IO Unit from do
  match {ns}.contract? with
  | .error _ => IO.println "SPEC_CONNECT error the contract does not type check"
  | .ok c =>
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
"""


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


# `tree_succeeds` (Verify/Spec/TreeSucceeds.lean) with every simplification step allowed to
# change nothing: the macro fails with "simp made no progress" when a step has nothing to rewrite,
# as for a succeeds-if over the call's environment alone (`msg.value == 0`). The steps and the
# closing tactics are the macro's; only the `try`s are new, so it proves the same statement.
TREE_SUCCEEDS_TOLERANT = """  intro args s iargs pre hl hargs hs hdec hst ht hreq hinv hsi
  try simp only [Nat.reducePow] at hs
  rcases args with _ | ⟨a0, _ | ⟨a1, _ | ⟨a2, _ | ⟨a3, _ | ⟨a4, _ | ⟨a5, _ | ⟨a6, _ | ⟨a7, _ | ⟨_, _⟩⟩⟩⟩⟩⟩⟩⟩⟩ <;>
    (try simp only [List.length_cons, List.length_nil] at hl) <;> (try omega) <;>
    (try simp only [List.mem_cons, List.not_mem_nil, or_false, forall_eq_or_imp, forall_eq,
      IsEmpty.forall_iff, implies_true, and_true, Nat.reducePow] at hargs) <;>
    (try simp only [@DS@, List.head, List.getElem_cons_zero, List.getElem_cons_succ, decodeWords, Ty.ofWord,
      bind, pure, Option.bind, Nat.reducePow] at hdec) <;>
    (try split_ifs at hdec) <;> (try simp only [Option.some.injEq, reduceCtorEq] at hdec) <;>
    (try subst hdec)
  all_goals try simp only [@DS@, Tree.eval, WExpr.eval, BinOp.eval, UnOp.eval, Generic.wordSize, Res.obs,
    Writes.store, List.getD_cons_zero, List.getD_cons_succ, Nat.reducePow]
  all_goals try simp (disch := simp only [hargs, hs, Nat.reduceLT]) only [Nat.mod_eq_of_lt, ite_mod_nat,
    Nat.reduceMod, Nat.zero_mod, ite10_eq_zero_iff, Decidable.not_not]
  all_goals try simp only [Bundle.satAll, Bundle.sat, @DS@, List.getElem_cons_succ, List.getElem_cons_zero,
    List.mem_cons, List.not_mem_nil, or_false, forall_eq_or_imp, forall_eq, eval, evalList, slotOf, mkEnv,
    Env.cur, Env.bind, BinOp.apply, Ty.ofBits, toWord, List.getD_cons_zero, List.getD_cons_succ,
    List.lookup, ofProp_ne_zero, ite_true, ite_false, Bool.false_eq_true, Nat.zero_mod, hst,
    beq_self_eq_true, Ty.InRange, Option.getD_some, forall_int_range, Nat.cast_ofNat, Int.reducePow] at hsi
  all_goals try simp only [wordMod, Nat.reducePow] at hsi
  all_goals try simp (disch := first | exact hs _ | omega) only [Nat.cast_ofNat,
    Int.reduceMod, Int.reduceToNat, Nat.mod_eq_of_lt, toNat_natCast_emod, toNat_ofNat_emod,
    toNat_zero_emod, toNat_one_emod, wordMod, Nat.reducePow, Nat.reduceMul, Nat.reduceMod, Nat.div_one,
    Nat.reduceDiv, Int.reducePow] at hsi
  all_goals ((try split_ifs) <;> (try rfl) <;> first | omega | (simp_all; done) | (simp_all; omega))"""


def tree_succeeds_tolerant(ds):
    """The tolerant `tree_succeeds` proof body for the simp set `ds`."""
    return TREE_SUCCEEDS_TOLERANT.replace("@DS@", f"{ds}, *")


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
        thm = next(
            (
                re.match(r"theorem (\S+)", out_lines[k]).group(1)
                for k in range(min(line, len(out_lines)) - 1, -1, -1)
                if out_lines[k].startswith("theorem ")
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
        "--tree-only",
        action="store_true",
        help="write only the trees, their erasure checks and the specs (no _meets, no _holds)",
    )
    args = ap.parse_args()
    only = {x for x in args.only.split(",") if x}

    probe = PROBE.format(
        core=args.core_module,
        spec=args.spec_module,
        ns=args.namespace,
        bundle=args.bundle,
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
    m = re.fullmatch(r"deployed some (.*)", lines[0], re.DOTALL)
    if not m:
        sys.exit(
            f"spec_connect: the constructor does not deploy without logs: {lines[0]}"
        )
    deployed = m.group(1)

    spec_slots = spec_mapping_slots(
        pathlib.Path(args.cwd) / (args.spec_module.replace(".", "/") + ".lean")
    )
    fns, refused = [], []
    for k, line in enumerate(lines[1:]):
        parts = line.split(" ", 5)
        name = parts[1]
        if only and name not in only:
            continue
        if parts[2] == "none":
            refused.append(f"{name}: no function of its selector")
            continue
        if parts[3] == "noerase":
            refused.append(f"{name}: outside the generic source stage")
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
        fns.append((k, name, ident(name), n, sel, tree, slots, nsi))
    if refused:
        sys.exit("spec_connect: refused (unproved):\n" + "\n".join(refused))

    ns = args.namespace
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
        "import Verify.VC.KernelIdx",
        "import SolidCore.Refinement.KernelRfl",
        *[f"import {m}" for m in args.imports],
        "",
        "/-!",
        f"# {args.bundle}, of SolidCore's checked contract, from the erased trees",
        "",
        "For each function of the exported spec: the generic source stage's tree (`_erase_eq`, `deploy_eq`),",
        "the spec of the tree (`_meets`) and the spec of SolidCore's contract on the stage's calls",
        "(`_holds`, `SolidCore.Spec.holdsFn_of_tree`). See tools/certify/spec_connect.py.",
        "-/",
        "",
        f"namespace {args.module_name}",
        "",
        "open SolidCore.Spec SolidCore.Refinement SolidCore.Refinement.Generic",
        "",
        "/-- The exported spec. -/",
        f"abbrev spec : Bundle := {args.bundle}",
        "",
        "/-- The call context of the stage's calls (no value), as SolidCore's runs see it. -/",
        "def ctx : Ctx := { sender := 0x2000, value := 0, timestamp := 1, self := 0x1000 }",
        "",
        "/-- The call context of every call: `ctx` with the call's value. -/",
        "def ctxOf (call : SolarTV.Call) : Ctx := { ctx with value := call.callvalue }",
        "",
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
    ]
    meets = "TreeMeetsFixed" if args.ghost_fixed else "TreeMeets"
    tactic = "tree_meets_fixed" if args.ghost_fixed else "tree_meets"
    holds = "holdsFn_of_tree_fixed" if args.ghost_fixed else "holdsFn_of_tree"
    hstep = " hstep" if args.ghost_fixed else ""
    for i, name, fid, n, sel, tree, slots, nsi in fns:
        mod = args.module_name
        lits, nests = slots
        if nests:
            # Mappings of mappings: keccak256's separation on both levels of slots.
            hyp = f" (hsep : NestedSlotsSeparated [{', '.join(map(str, lits))}] [{', '.join(map(str, nests))}])"
            proof = f"  tree_meets_map2 [{fid}_tree, {fid}_spec, {mod}.spec, {args.bundle}, {mod}.ctx] hsep"
            use = f"{fid}_meets hsep"
        elif lits:
            # Mappings: the one hypothesis is keccak256's separation on their slots.
            hyp = f" (hsep : MapSlotsSeparated [{', '.join(map(str, lits))}])"
            proof = f"  tree_meets_map [{fid}_tree, {fid}_spec, {mod}.spec, {args.bundle}, {mod}.ctx] hsep"
            use = f"{fid}_meets hsep"
        else:
            hyp = ""
            proof = f"  {tactic} [{fid}_tree, {fid}_spec, spec, {args.bundle}]"
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
        if args.tree_only:
            continue
        out += (
            succeeds_block(fid, n, hyp, proof, lits, nests, meets, mod, args.bundle)
            if nsi
            else [
                f"/-- `{name}`'s spec holds of its erased tree. -/",
                f"theorem {fid}_meets{hyp} : {meets} spec {fid}_spec ctx {n} {fid}_tree := by",
                proof,
                "",
            ]
        ) + [
            f"/-- **`{name}`'s spec holds of SolidCore's checked contract** on the stage's calls of it. -/",
            f"theorem {fid}_holds{hyp} {{c : SolidCore.Solidity.TypeCheck.CheckedContract}} (hc : {ns}.contract? = .ok c)",
            "    (step : GhostStep)"
            + (
                f" (hstep : ∀ call, SolidCore.Verify.VC.stageScope fn_{fid} deployed call → ∀ g, step call g = g) :"
                if args.ghost_fixed
                else " :"
            ),
            f"    HoldsFn (solidCore {ns}.fuel) c (SolidCore.Verify.VC.stageScope fn_{fid} deployed) spec ctxOf step {fid}_spec := by",
            "  have hdep := deploy_eq",
            "  simp only [deploy?, hc] at hdep",
            "  obtain ⟨dd, hdd⟩ := Wide.deployedAt_of hdep",
            f"  have hmap := {fid}_erase_eq",
            f"  simp only [{fid}_erase?, hc, Option.map_eq_some_iff, Prod.mk.injEq] at hmap",
            "  obtain ⟨f, hf, hlen, hT⟩ := hmap",
            f"  exact {holds} (fn := fn_{fid}) (ctx := ctx) hdd (by decide) hf (by simpa [fn_{fid}] using hlen) hT",
            f"    (fun call h => by simp [ctxOf, ctx, h]){hstep} (by simpa [fn_{fid}] using {use})",
            "",
            f"/-- **`{name}`'s spec holds of SolidCore's checked contract in every world `W`** on the",
            "stage's calls of it (the function calls out nowhere, so its tree is its run in any world). -/",
            f"theorem {fid}_holds_in (W : SolidCore.Refinement.World){hyp} {{c : SolidCore.Solidity.TypeCheck.CheckedContract}}",
            f"    (hc : {ns}.contract? = .ok c) (step : GhostStep)"
            + (
                f" (hstep : ∀ call, SolidCore.Verify.VC.stageScope fn_{fid} deployed call → ∀ g, step call g = g) :"
                if args.ghost_fixed
                else " :"
            ),
            f"    HoldsFn (solidCoreIn W {ns}.fuel) c (SolidCore.Verify.VC.stageScope fn_{fid} deployed) spec ctxOf step {fid}_spec := by",
            "  have hdep := deploy_eq",
            "  simp only [deploy?, hc] at hdep",
            "  obtain ⟨dd, hdd⟩ := Wide.deployedAt_of hdep",
            f"  have hmap := {fid}_erase_eq",
            f"  simp only [{fid}_erase?, hc, Option.map_eq_some_iff, Prod.mk.injEq] at hmap",
            "  obtain ⟨f, hf, hlen, hT⟩ := hmap",
            f"  exact {holds}_in W (fn := fn_{fid}) (ctx := ctx) hdd (by decide) hf (by simpa [fn_{fid}] using hlen) hT",
            f"    (fun call h => by simp [ctxOf, ctx, h]){hstep} (by simpa [fn_{fid}] using {use})",
            "",
            f"/-- info: '{args.module_name}.{fid}_holds_in' depends on axioms: [propext, Classical.choice, Quot.sound] -/",
            "#guard_msgs in",
            f"#print axioms {fid}_holds_in",
            "",
            f"/-- info: '{args.module_name}.{fid}_holds' depends on axioms: [propext, Classical.choice, Quot.sound] -/",
            "#guard_msgs in",
            f"#print axioms {fid}_holds",
            "",
        ]
    out += [f"end {args.module_name}", ""]
    args.out.write_text("\n".join(out).replace("`", chr(96)))
    check_output(args)
    for _, name, fid, *_ in fns:
        print(f"{args.module_name}.{fid}_{'tree' if args.tree_only else 'holds'}")


if __name__ == "__main__":
    main()
