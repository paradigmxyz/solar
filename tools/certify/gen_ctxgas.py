#!/usr/bin/env python3
"""Write the gas certificate module of a contract's exported stages: the checker with contexts
and gas (untrusted inference, kernel-checked).

gen_ctxgas.py GEN_DIR --module-prefix NAME.gen.C --contract C --final evmN --skip B,B --out OUT.lean

--skip names the blocks of the final EVM IR outside the loop checker's fragment that no call in
the statement reaches (a permit path's STATICCALL, GAS and return data, say): the module states
guarded, the final EVM IR with each of them a jump to itself, and proves by the kernel's
evaluation that AssembleCtxGas.checkXL accepts the exact bytecode against the final EVM IR,
every other block checked in each of its calling contexts with memory below the inferred top.
That the statement's calls never reach a skipped block is a stage (guard_run), proved apart by
symbolic execution.
"""

import argparse
import json
import pathlib
import re
import sys

sys.path.insert(0, str(pathlib.Path(__file__).parent))
import asm_cert
import asm_ctxl
import export_ns


class CtxGasError(Exception):
    """The inference cannot certify this program (its message is the CLI's error line)."""


def gas_bound(info, fuel):
    """AssembleLoop.gasBoundL of the certificate's base for FUEL blocks: 22100 per item times the
    final EVM IR's block fuel times the largest checked block's items, plus memory to the top.
    Untrusted: the proof that uses it states it and the kernel checks it (gas_le)."""
    bmax = max(info["costs"]) if info["costs"] else 0
    words = info["top"] // 32
    return 22100 * (fuel * bmax) + 3 * words + words * words // 512


def generate(
    gen, module_prefix, contract, final, skip=None, slice_from=None, selector=None
):
    """The module text and what the inference found: (text, {top, env, reads, costs, F, notes}).
    Raises CtxGasError when the program is outside what the checker can certify, including when
    asm_ctxl's or asm_cert's inference stops (they exit with the CLI's error line, such as a block
    entered in two contexts)."""
    try:
        return _generate(gen, module_prefix, contract, final, skip, slice_from, selector)
    except SystemExit as e:
        why = e.code if isinstance(e.code, str) else f"error: the inference exits ({e.code})"
        raise CtxGasError(why) from None


def _generate(gen, module_prefix, contract, final, skip, slice_from, selector):
    a = argparse.Namespace(
        gen=pathlib.Path(gen),
        module_prefix=module_prefix,
        contract=contract,
        final=final,
        skip=skip,
        slice_from=slice_from,
        selector=selector,
    )
    notes = []
    final_mod = a.final[0].upper() + a.final[1:]
    text = (a.gen / f"{final_mod}.lean").read_text()
    code = bytes.fromhex(json.loads((a.gen / "manifest.json").read_text())["bytecode"])
    blocks = asm_cert.parse_program(text)
    n = len(blocks)

    def succ(b):
        out, term = set(), False
        for i in blocks[b]:
            if i[0] == "jump":
                out.add(i[1])
                term = True
                break
            if i[0] == "jumpi":
                out.add(i[1])
            if i[0] == "pushLabel":
                out.add(i[1])
            if i[0] in ("ret", "revert", "stop", "jumpDyn", "jumpTable", "invalid"):
                term = True
                break
        if not term:
            out.add(b + 1)
        return out

    if a.selector is not None:
        sel = int(a.selector, 16)
        hits = set()
        for bl in blocks:
            for x, y, z in zip(bl, bl[1:], bl[2:]):
                if x == ("push", sel) and y == ("stack", "eq") and z[0] == "jumpi":
                    hits.add(z[1])
        if len(hits) != 1:
            raise CtxGasError(
                f"error: the dispatcher jumps on selector {a.selector} to {sorted(hits)}, not one block"
            )
        a.slice_from = hits.pop()
        notes.append(f"selector {a.selector}: block {a.slice_from}")
    if a.slice_from is not None:
        # Untrusted: a block left out of F is checked, and the guard stage proves that no call of
        # the statement enters a block of F, so a wrong slice only makes a proof fail.
        seen, work = {0, 1}, [a.slice_from]
        while work:
            b = work.pop()
            if b in seen or b >= n:
                continue
            seen.add(b)
            work.extend(succ(b))
        F = [b for b in range(n) if b not in seen]
    else:
        F = sorted(int(x) for x in (a.skip or "").split(",") if x)
    if 0 in F or any(b >= n for b in F):
        raise CtxGasError(
            "error: --skip names the entry block or a block past the program"
        )
    turned, _offsets, _labelled = asm_cert.layout(blocks, code)
    c = asm_ctxl.infer_ctx(text, code)
    # A block of F is never entered (the guard stage), so it needs no items: the bound's largest block
    # is the largest one a call of the statement runs.
    c["costs"] = [0 if b in F else x for b, x in enumerate(c["costs"])]
    # Memory reached by the checked blocks, in every context: constant offsets only.
    top, bail = 0, []
    for b in range(n):
        if b in F:
            continue
        for x in c["ctxs"][b]:
            ty = [bool(v) if v is not None else False for v in x["types"]]
            _, _, t = asm_cert.run_block(
                blocks, turned, c["rets"], b, x["need"], ty, False, bail
            )
            top = max(top, t)
    if bail:
        raise CtxGasError(
            f"error: blocks {sorted(set(bail))} reach memory at offsets the checker does not know"
        )
    top = (top + 31) // 32 * 32
    env = any(
        i[0] == "effect" and i[1] in ("caller", "keccak256")
        for b in range(n)
        if b not in F
        for i in blocks[b]
    )
    reads = any(i[0] == "env" for b in range(n) if b not in F for i in blocks[b])
    cert = asm_ctxl.lean_ctx_cert(c)
    # One field per line: the base certificate's flags on one line do not parse after a list.
    cert = cert.replace("top := 2 ^ 140", f"top := {top}")
    cert = re.sub(
        r"env := (true|false), reads := (true|false), calls := (true|false)",
        f"env := {'true' if env else 'false'}, reads := {'true' if reads else 'false'}",
        cert,
    )
    # Each flag of the base certificate on its own line, as Lean parses them.
    cert = cert.replace(", env := ", ",\n      env := ").replace(
        ", reads := ", ",\n      reads := "
    )
    # The guarded program: the same text with each skipped block a jump to itself.
    m = re.search(r"\{ blocks := \[\n", text)
    start = m.end()  # ty: ignore[unresolved-attribute]
    end = text.rindex("] }")
    body = text[start:end]
    parts = re.split(r"(?m)^(    -- bb\d+\n)", body)
    heads, bodies = parts[1::2], parts[2::2]
    if len(bodies) != n:
        raise CtxGasError(f"error: {len(bodies)} blocks in the text, {n} parsed")
    out_blocks = []
    for b, (h, bl) in enumerate(zip(heads, bodies)):
        if b in F:
            sep = "," if b < n - 1 else ""
            bl = f"    [.jump {b}]{sep}\n"
        out_blocks.append(h + bl)
    guarded = "{ blocks := [\n" + "".join(out_blocks) + "] }"
    export = export_ns.of_gen(a.gen)
    ns = f"{export}.CtxGas"
    q = "\x60"
    out = f"""import {a.module_prefix}.{final_mod}
import {a.module_prefix}.Bytecode
import SolidCore.Refinement.Solar.AssembleCtxGas
import SolidCore.Refinement.Solar.AssembleCheckFast
import SolidCore.Refinement.Solar.AssembleCtxCall

-- Generated by solar's tools/certify/gen_ctxgas.py from the run's export; do not edit.

/-!
# {a.contract}: the exact bytecode against the final EVM IR, with contexts and a gas bound

{q}cx{q} is tools/certify/asm_ctxl.py's inferred certificate (untrusted data) with the memory bound
{top} of every block but {F}: those are outside the loop checker's fragment and {q}checkXL{q} leaves
them unchecked. {q}check_ok{q}: the kernel evaluates {q}AssembleCtxGas.checkXL{q} of {q}{a.final}{q} against the
exact bytecode, with RETURNDATASIZE in its effect form ({q}AssembleCtx.norm{q}, the same runs:
{q}AssembleCtx.run_norm{q}). {q}guarded{q} is that program with those blocks a jump to themselves
({q}guarded_eq{q}); that the statement's calls never reach them is the stage {q}guard_run{q}.
-/

namespace {ns}

open SolidCore.Refinement.Solar SolidCore.Refinement.Solar.AssembleCtx

/-- The exact runtime bytecode, byte by byte. -/
abbrev bytes : List Nat := {export}.bytecode

def code : ByteArray := ⟨(bytes.map UInt8.ofNat).toArray⟩

/-- The final EVM IR. -/
abbrev prog : SolidCore.Refinement.Solar.Assemble.Program := {export}.{a.final}

/-- The blocks outside the checker's fragment. -/
def F : List Nat := {F}

def cx : CtxCert :=
  {cert}

set_option maxRecDepth 100000 in
/-- The checker with contexts and gas accepts the exact bytes against the final EVM IR: one kernel
evaluation of its fast form (the walk on the code's suffixes, which implies the labels' check). -/
theorem check_ok : AssembleCtxGas.checkXL F (AssembleCtx.norm prog) code cx = true :=
  AssembleCtxGas.checkXL_of_checkXLS (by decide +kernel)

/-- The final EVM IR with each block of {q}F{q} a jump to itself. -/
def guarded : SolarTV.EVM.Program.Program :=
  {guarded}

theorem guarded_eq : AssembleCtxGas.guard F (AssembleCtx.norm prog) = guarded := rfl

/-- info: '{ns}.check_ok' depends on axioms: [propext, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms check_ok

end {ns}
"""
    return out, {
        "top": top,
        "env": env,
        "reads": reads,
        "costs": c["costs"],
        "F": F,
        "notes": notes,
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("gen", type=pathlib.Path)
    ap.add_argument("--module-prefix", required=True)
    ap.add_argument("--contract", required=True)
    ap.add_argument("--final", required=True)
    ap.add_argument("--skip", default=None)
    ap.add_argument(
        "--slice-from",
        default=None,
        type=int,
        help="skip every block but the entry's dispatcher (blocks 0 and 1) and those reachable "
        "from this block (a function's entry): its jumps, falls and the labels it pushes",
    )
    ap.add_argument(
        "--selector",
        default=None,
        help="--slice-from the block the dispatcher jumps to on this selector (push SEL; eq; jumpi B)",
    )
    ap.add_argument("--out", required=True, type=pathlib.Path)
    a = ap.parse_args()
    try:
        out, info = generate(
            a.gen,
            a.module_prefix,
            a.contract,
            a.final,
            a.skip,
            a.slice_from,
            a.selector,
        )
    except CtxGasError as e:
        raise SystemExit(str(e)) from None
    for n in info["notes"]:
        print(n)
    a.out.write_text(out)
    print(f"top={info['top']} env={info['env']} reads={info['reads']}")


if __name__ == "__main__":
    main()
