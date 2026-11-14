#!/usr/bin/env python3
"""The certificate of the assembly checker with contexts (AssembleCtx.checkXB), inferred.

    asm_ctx.py PROGRAM.lean BYTECODE_HEX

The final EVM IR of a contract whose internal functions are called from several places has no
single entry height and stack type per block, which AssembleCut.checkC needs. checkXB checks each
block once per context instead: an exact entry height, the types of the whole stack, which words
are which labels, and the context each successor enters in. The contexts here are the abstract
configurations cutpoints.evm_states reaches from block 0 (each word the block a pushLabel pushed,
or a data word), one context per configuration of a block. Nothing here is trusted: the
generated module states the certificate as data and the kernel's evaluation of the checker
re-derives every claim, so a wrong inference only fails the proof.
"""

import pathlib
import sys

here = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(here))
sys.path.insert(0, str(here.parents[1] / "solar-tv" / "milestone"))
import asm_cert  # noqa: E402
import cutpoints  # noqa: E402


def infer_ctx(program_text, code):
    blocks = cutpoints.parse_evm(program_text)
    ablocks = asm_cert.parse_program(program_text)
    n = len(blocks)
    assert n == len(ablocks)
    edges = cutpoints.evm_states("final", blocks)
    turned, offsets, labelled = asm_cert.layout(ablocks, code)
    # The contexts of each block: its configurations grouped by the labels on their stacks
    # (each label's depth and block), so that a callee entered from two call sites has a context
    # per return label, while the configurations that differ only in the data words under them
    # (a shared revert reached at several heights) share one, with their range of heights.
    def key(st):
        return tuple((i, w) for i, w in enumerate(st[1]) if w is not None)

    groups = [{} for _ in range(n)]
    for st in sorted(edges, key=lambda s: (s[0], len(s[1]), str(s[1]))):
        groups[st[0]].setdefault(key(st), []).append(st)
    if (0, ()) not in edges or key((0, ())) not in groups[0]:
        raise SystemExit("error: block 0 is not entered with the empty stack")
    order = [sorted(g, key=lambda k: (k != (), len(k), str(k))) for g in groups]
    index = {}
    for b in range(n):
        for j, k in enumerate(order[b]):
            for st in groups[b][k]:
                index[st] = j
    out = []
    for b in range(n):
        pushes = sorted({i[1] for i in ablocks[b] if i[0] == "pushLabel"})
        row = []
        for k in order[b]:
            sts = groups[b][k]
            need = min(len(st[1]) for st in sts)
            high = max(len(st[1]) for st in sts)
            labels = dict(k)
            succ = []
            for st in sts:
                for t in edges[st]:
                    if t[0] == b:
                        if index[t] != index[st]:
                            raise SystemExit(f"error: block {b} jumps to itself in another context")
                        continue
                    pair = (t[0], index[t])
                    prev = [q for q in succ if q[0] == t[0]]
                    if prev and prev[0] != pair:
                        raise SystemExit(
                            f"error: block {b} leaves to block {t[0]} in two contexts"
                        )
                    if not prev:
                        succ.append(pair)
            rets = sorted(set(labels.values()) | set(pushes))
            row.append(
                {
                    "need": need,
                    "max": high,
                    "types": [i in labels for i in range(need)],
                    "labs": [[labels[i]] if i in labels else None for i in range(need)],
                    "succ": succ,
                    "rets": rets,
                }
            )
        out.append(row)
    costs = []
    for b, instrs in enumerate(ablocks):
        costs.append((1 if b in labelled else 0) + sum(asm_cert.lower_len(i) for i in instrs))
    env = any(i[0] == "effect" and i[1] in ("caller", "keccak256") for bl in ablocks for i in bl)
    reads = any(i[0] == "env" for bl in ablocks for i in bl)
    calls = any(i[0] == "other" and "staticcall" in i[1] for bl in ablocks for i in bl)
    rets = sorted({i[1] for bl in ablocks for i in bl if i[0] == "pushLabel"})
    base = {
        "offsets": offsets,
        "turned": turned,
        "costs": costs,
        "rets": rets,
        "env": env,
        "reads": reads,
        "calls": calls,
    }
    return base, out


def lean_lab(l):
    return "none" if l is None else "some [" + ", ".join(map(str, l)) + "]"


def lean_ctx(c):
    return (
        "{ need := " + str(c["need"]) + ", max := " + str(c["max"])
        + ", types := [" + ", ".join("some true" if t else "some false" for t in c["types"]) + "]"
        + ", labs := [" + ", ".join(lean_lab(l) for l in c["labs"]) + "]"
        + ", succ := [" + ", ".join(f"({t}, {j})" for t, j in c["succ"]) + "]"
        + ", rets := " + str(c["rets"]) + " }"
    )


def lean_cx(base, ctxs):
    b = (
        "{ offsets := [" + ", ".join(hex(o) for o in base["offsets"]) + "],\n"
        "    needs := [], top := 2 ^ 140,\n"
        "    turned := " + str(base["turned"]) + ",\n"
        "    costs := " + str(base["costs"]) + ",\n"
        "    rets := " + str(base["rets"]) + ",\n"
        "    env := " + ("true" if base["env"] else "false")
        + ", reads := " + ("true" if base["reads"] else "false")
        + ", calls := " + ("true" if base["calls"] else "false") + ", creates := false }"
    )
    rows = ",\n    ".join("[" + ", ".join(lean_ctx(c) for c in row) + "]" for row in ctxs)
    return b, "{ base := base\n    ctxs := [\n    " + rows + "] }"


MODULE = """-- Generated by solar's tools/certify/asm_ctx.py; do not edit.
import SolidCore.Refinement.Solar.AssembleCtxCall
import W1Proofs.WalkAt
import {gen}.{last}
import {gen}.Bytecode

/-! The exact runtime bytecode of {contract} against its final EVM IR {last_def}, by the checker
with contexts and STATICCALL (AssembleCtx.checkXB) on the certificate asm_ctx.py inferred, which
the kernel's evaluation of the checker re-derives, and the assembly premise it gives at every gas
below 2 ^ 256 under the checker's named hypotheses on the message runner (StaticRunner,
CalleesRun). -/

namespace {ns}

open SolidCore.Refinement.Solar SolidCore.Refinement.Solar.AssembleCtx

set_option maxRecDepth 100000

/-- The runtime bytecode solar emitted, as bytes. -/
def bytes : List Nat := {export}.bytecode

/-- The runtime bytecode. -/
def code : ByteArray := ⟨(bytes.map UInt8.ofNat).toArray⟩

/-- The certificate's layout and flags. -/
def base : Assemble.Cert :=
  {base}

/-- The certificate: the layout, and each block's contexts. -/
def cx : CtxCert :=
  {cx}

/-- The final EVM IR in the checker's effect form. -/
def prog : Assemble.Program := norm {export}.{last_def}

theorem prog_length : prog.blocks.length = {nblocks} := by decide +kernel

/-- The code decodes, linearly from offset 0, to the program's lowered items. -/
theorem bytes_eq : Assemble.bytesOf code = bytes := by decide +kernel

theorem bytes_length : bytes.length = {ncode} := by decide +kernel

/-- The walk on the code's suffixes, which the kernel evaluates in time linear in its length. -/
theorem walkAt_ok : (SolarTV.Assembly.walkAt {ncode} cx.base.offset 0 bytes (Assemble.lower prog cx.base)).isSome = true := by
  decide +kernel

theorem walk_ok : (SolarTV.Assembly.walk (Assemble.bytesOf code) cx.base.offset 0 (Assemble.lower prog cx.base)).isSome = true := by
  rw [bytes_eq, ← SolarTV.Assembly.walkAt_eq, List.drop_zero, bytes_length]
  exact walkAt_ok

theorem rest_ok : (decide (cx.base.top % 32 = 0) && decide (cx.base.top ≤ 2 ^ 200) &&
    decide (code.size < 2 ^ 32) && decide (0 < cx.count 0) && decide ((cx.at 0 0).need = 0)) = true := by
  decide +kernel

/-! Each block in each of its contexts, one evaluation per block. -/

{block_lemmas}

theorem blocks_ok : ∀ k (_ : k < prog.blocks.length), (List.range (cx.count k)).all
    (fun j => ctxOk prog cx k j (prog.blocks.getD k [])) = true := by
  intro k hk
  rw [prog_length] at hk
  exact match k, hk with
{block_cases}
  | _ + {nblocks}, h => absurd h (by omega)

/-- The checker with contexts accepts the exact bytecode against the final EVM IR. -/
theorem check_ok : checkXB prog code cx = true :=
  checkXB_of_parts walk_ok rest_ok blocks_ok

/-- **The bytecode runs as the final EVM IR, at every gas below 2 ^ 256**, on every call of a
scope whose callees run (CalleesRun), under a runner of static messages (StaticRunner). -/
theorem bytecode (codes : SolidCore.Solidity.Shared.Account.BytesMap) (scope : SolarTV.Call → Prop)
    (hP : ∀ gas, AssembleCallSeg.StaticRunner gas)
    (hQ : ∀ gas call, scope call → AssembleCallSeg.CalleesRun gas code call) :
    ∀ gas, gas < 2 ^ 256 → ∀ call, scope call → ∃ (o : SolarTV.Machine.Oracle)
      (W : SolidCore.Refinement.World), W.Open ∧ SolidCore.Refinement.CallsAgree o W ∧
      RunOracle.CertOk cx.base o ∧
      SolidCore.Refinement.Final.Within (SolarTV.evmirSem o) (SolidCore.Refinement.Final.ethLibWith gas)
        {export}.{last_def} code (· = call) :=
  assembly_of_checkXB_norm (p := {export}.{last_def}) check_ok codes scope hP hQ

/-- info: '{ns}.bytecode' depends on axioms: [propext, Classical.choice, Quot.sound] -/
#guard_msgs (whitespace := lax) in
#print axioms bytecode

end {ns}
"""


def main():
    import argparse

    ap = argparse.ArgumentParser()
    ap.add_argument("gen", type=pathlib.Path, help="the exported stages' directory")
    ap.add_argument("--gen-module", required=True)
    ap.add_argument("--out", required=True, type=pathlib.Path)
    args = ap.parse_args()
    import json

    manifest = json.loads((args.gen / "manifest.json").read_text())
    contract = manifest["contract"]
    last = [s for s in manifest["stages"] if s["file"] != "Bytecode.lean"][-1]
    module = last["file"].removesuffix(".lean")
    program = (args.gen / last["file"]).read_text()
    base, ctxs = infer_ctx(program, bytes.fromhex(manifest["bytecode"]))
    export = f"SolarTV.Export.{contract}"
    norm_blocks = [
        [".effect .returndatasize 0" if i == ".op (.stack .returndatasize)" else i for i in instrs]
        for _, instrs in cutpoints.parse_evm(program)
    ]
    args.out.write_text(
        MODULE.replace("{gen}", args.gen_module)
        .replace("{last}", module)
        .replace("{last_def}", last["definition"])
        .replace("{contract}", contract)
        .replace("{export}", export)
        .replace("{ns}", f"{export}.Ctx")
        .replace("{ncode}", str(len(bytes.fromhex(manifest["bytecode"]))))
        .replace("{nblocks}", str(len(norm_blocks)))
        .replace("{block_lemmas}", "\n\n".join(
            f"theorem block_{k} : (List.range (cx.count {k})).all (fun j => ctxOk prog cx {k} j (prog.blocks.getD {k} [])) = true := by\n  decide +kernel"
            for k in range(len(norm_blocks))))
        .replace("{block_cases}", "\n".join(f"  | {k}, _ => block_{k}" for k in range(len(norm_blocks))))
        .replace("{base}", lean_cx(base, ctxs)[0])
        .replace("{cx}", lean_cx(base, ctxs)[1])
    )


if __name__ == "__main__":
    main()
