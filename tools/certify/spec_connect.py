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

* `F_shape_eq` (`kernel_rfl`): the contract deploys with that storage and no logs, and dispatches
  F's selector to a function the generic stage erases to `F_tree`, the tree Lean printed;
* `F_meets`: F's exported spec holds of `F_tree` (`SolidCore.Spec.TreeMeets`, by `tree_meets`);
* `F_holds`: F's spec holds of SolidCore's checked contract on the stage's calls of F
  (`SolidCore.Spec.holdsFn_of_tree`).

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
    """The mapping slots a tree hashes keys into, or None when one is not a literal slot."""
    slots = set()
    for m in re.finditer(r"WExpr\.hash\b", tree):
        args = groups(tree, m.end())[:2]
        if len(args) != 2:
            return None
        lit = re.fullmatch(
            r"\((?:[\w.]*\.)?WExpr\.lit (\d+)\)", re.sub(r"\s+", " ", args[1])
        )
        if not lit:
            return None
        slots.add(int(lit.group(1)))
    return sorted(slots)


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
        if nsi != 0:
            refused.append(f"{name}: succeeds-if conditions are not proved yet")
            continue
        if n > 8:
            refused.append(f"{name}: more than eight parameters")
            continue
        tree = parts[5].removeprefix("TREE ")
        slots = mapping_slots(tree)
        if slots is None:
            refused.append(f"{name}: hashes a key into a slot that is not a literal")
            continue
        if slots and not args.ghost_fixed:
            refused.append(
                f"{name}: reads mappings, which tree_meets_map proves only with --ghost-fixed"
            )
            continue
        fns.append((k, name, ident(name), n, sel, tree, slots))
    if refused:
        sys.exit("spec_connect: refused (unproved):\n" + "\n".join(refused))

    ns = args.namespace
    out = [
        "-- Generated by solar's tools/certify/spec_connect.py. Do not edit.",
        f"import {args.core_module}",
        f"import {args.spec_module}",
        "import Verify.Spec.Tree",
        "import Verify.Spec.TreeMap",
        "import Verify.VC.KernelIdx",
        "import SolidCore.Refinement.KernelRfl",
        "",
        "/-!",
        f"# {args.bundle}, of SolidCore's checked contract, from the erased trees",
        "",
        "For each function of the exported spec: the generic source stage's tree (`_shape_eq`),",
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
    ]
    meets = "TreeMeetsFixed" if args.ghost_fixed else "TreeMeets"
    tactic = "tree_meets_fixed" if args.ghost_fixed else "tree_meets"
    holds = "holdsFn_of_tree_fixed" if args.ghost_fixed else "holdsFn_of_tree"
    hstep = " hstep" if args.ghost_fixed else ""
    for i, name, fid, n, sel, tree, slots in fns:
        mod = args.module_name
        if slots:
            # Mappings: the one hypothesis is keccak256's separation on their slots.
            ms = "[" + ", ".join(map(str, slots)) + "]"
            hyp = f" (hsep : MapSlotsSeparated {ms})"
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
            f"/-- How the contract deploys, and how SolidCore dispatches and the stage erases `{name}`. -/",
            f"def {fid}_shape? : Option (Bool × Option (Nat × Option Tree)) :=",
            f"  match {ns}.contract? with",
            f"  | .ok c => some (Wide.deployedAt {ns}.fuel c deployed,",
            f"      (c.core.findFunctionBySelector? {sel}).map fun f => (f.params.length, eraseFn (envOf c.core) {ns}.fuel f))",
            "  | .error _ => none",
            "",
            f"kernel_rfl {fid}_shape_eq {fid}_shape? (some (true, some ({n}, some {fid}_tree))) using {ns}.selector_eq SolidCore.Verify.VC.mkIdx_eq",
            "",
            f"/-- `{name}`'s exported spec. -/",
            f"def {fid}_spec : FnSpec := spec.spec.specs[{i}]'(by decide)",
            "",
            f"/-- `{name}`'s spec holds of its erased tree. -/",
            f"theorem {fid}_meets{hyp} : {meets} spec {fid}_spec ctx {n} {fid}_tree := by",
            proof,
            "",
            f"/-- **`{name}`'s spec holds of SolidCore's checked contract** on the stage's calls of it. -/",
            f"theorem {fid}_holds{hyp} {{c : SolidCore.Solidity.TypeCheck.CheckedContract}} (hc : {ns}.contract? = .ok c)",
            "    (step : GhostStep)"
            + (
                f" (hstep : ∀ call, SolidCore.Verify.VC.stageScope fn_{fid} deployed call → ∀ g, step call g = g) :"
                if args.ghost_fixed
                else " :"
            ),
            f"    HoldsFn (solidCore {ns}.fuel) c (SolidCore.Verify.VC.stageScope fn_{fid} deployed) spec ctxOf step {fid}_spec := by",
            f"  have hshape := {fid}_shape_eq",
            f"  simp only [{fid}_shape?, hc, Option.some.injEq, Prod.mk.injEq] at hshape",
            "  obtain ⟨hdep, hmap⟩ := hshape",
            "  obtain ⟨dd, hdd⟩ := Wide.deployedAt_of hdep",
            "  simp only [Option.map_eq_some_iff, Prod.mk.injEq] at hmap",
            "  obtain ⟨f, hf, hlen, hT⟩ := hmap",
            f"  exact {holds} (fn := fn_{fid}) (ctx := ctx) hdd (by decide) hf (by simpa [fn_{fid}] using hlen) hT",
            f"    (fun call h => by simp [ctxOf, ctx, h]){hstep} (by simpa [fn_{fid}] using {use})",
            "",
            f"/-- info: '{args.module_name}.{fid}_holds' depends on axioms: [propext, Classical.choice, Quot.sound] -/",
            "#guard_msgs in",
            f"#print axioms {fid}_holds",
            "",
        ]
    out += [f"end {args.module_name}", ""]
    args.out.write_text("\n".join(out).replace("`", chr(96)))
    for _, name, fid, *_ in fns:
        print(f"{args.module_name}.{fid}_holds")


if __name__ == "__main__":
    main()
