#!/usr/bin/env python3
"""Writes the HIR shadow of one contract: interface facts from the compiler's analysis.

    tools/hir-shadow/hir_shadow.py --solar BIN SOURCE.sol --contract NAME \
        --json OUT.json [--lean OUT.lean --namespace NS]

The facts (format version 1) are the contract's state variables with their storage slots, and
its functions, public getters and constructor with canonical signatures, visibility, mutability,
ABI types and selectors. They are read from one Standard JSON compilation that stops after
analysis: the AST JSON, whose IDs, types, selectors and resolutions come from the HIR, the storage
layout, the method identifiers and the ABI. Each fact that two outputs both state must agree
(an AST function's `functionSelector` and its method identifier), or nothing is written.

The facts are untrusted hints for the Lean side (solidity-lean `Verify/HirShadow`), which accepts
them only when they agree with its own source model. A contract outside the envelope (inheritance,
non-value state variables, modifiers, events, errors, user-defined types, receive or fallback)
exits 2 with BLOCKED and the reason; it is never written as facts.
"""

import argparse
import hashlib
import json
import pathlib
import re
import subprocess
import sys

VERSION = 1
ELEMENTARY = re.compile(r"^(bool|address|u?int(8|16|24|32|40|48|56|64|72|80|88|96|104|112|120|128|"
                        r"136|144|152|160|168|176|184|192|200|208|216|224|232|240|248|256)|bytes([1-9]|"
                        r"[12][0-9]|3[0-2]))$")


class Blocked(Exception):
    pass


def die(msg: str) -> None:
    sys.exit(f"hir-shadow: error: {msg}")


def canon(type_string: str) -> str:
    t = "address" if type_string == "address payable" else type_string
    if t == "uint":
        t = "uint256"
    if t == "int":
        t = "int256"
    if not ELEMENTARY.match(t):
        raise Blocked(f"type `{type_string}` is not a value type")
    return t


def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def compile_json(solar: str, src: pathlib.Path) -> dict:
    inp = {
        "language": "Solidity",
        "sources": {src.name: {"content": src.read_text()}},
        "settings": {
            "stopAfter": None,
            "outputSelection": {"*": {"": ["ast"], "*": ["abi", "evm.methodIdentifiers", "storageLayout"]}},
        },
    }
    del inp["settings"]["stopAfter"]
    out = subprocess.run([solar, "--standard-json"], input=json.dumps(inp), capture_output=True,
                         text=True, check=False)
    if out.returncode != 0 and not out.stdout:
        die(f"solar failed: {out.stderr.strip()}")
    res = json.loads(out.stdout)
    errs = [e for e in res.get("errors", []) if e.get("severity") == "error"]
    if errs:
        die("solar reports errors: " + "; ".join(e.get("message", "") for e in errs))
    return res


def facts(res: dict, src_name: str, contract: str) -> tuple[list, list]:
    ast = res["sources"][src_name]["ast"]
    out = res["contracts"][src_name][contract]
    methods = out["evm"]["methodIdentifiers"]
    by_selector = {int(v, 16): k for k, v in methods.items()}
    nodes = [n for n in ast["nodes"] if n.get("nodeType") == "ContractDefinition" and n["name"] == contract]
    if len(nodes) != 1:
        die(f"no single contract `{contract}` in the AST")
    c = nodes[0]
    if c.get("contractKind") != "contract" or c.get("abstract"):
        raise Blocked("not a plain contract")
    if c.get("baseContracts"):
        raise Blocked("inheritance")
    layout = out.get("storageLayout") or {}
    types = layout.get("types") or {}
    state = []
    for e in layout.get("storage", []):
        t = types[e["type"]]
        state.append({"name": e["label"], "type": canon(t["label"]), "slot": int(e["slot"]),
                      "offset": int(e["offset"]), "bytes": int(t["numberOfBytes"])})

    def selector_of(node: dict, sig: str) -> int:
        sel = node.get("functionSelector")
        if sel is None:
            die(f"`{sig}` has no selector in the AST")
        s = int(sel, 16)
        if by_selector.get(s) != sig:
            die(f"AST selector of `{sig}` is {sel}, method identifiers say {by_selector.get(s)}")
        return s

    fns = []
    for n in c["nodes"]:
        kind = n.get("nodeType")
        if kind == "VariableDeclaration":
            if n.get("visibility") != "public":
                continue
            t = canon(n["typeDescriptions"]["typeString"])
            sig = f"{n['name']}()"
            fns.append({"kind": "getter", "name": n["name"], "signature": sig,
                        "selector": selector_of(n, sig), "visibility": "external",
                        "mutability": "view", "inputs": [], "outputs": [t]})
        elif kind == "FunctionDefinition":
            fkind = n["kind"]
            if fkind in ("receive", "fallback"):
                raise Blocked(f"{fkind} function")
            if n.get("modifiers"):
                raise Blocked(f"function `{n['name']}` has modifiers")
            ins = [canon(p["typeDescriptions"]["typeString"]) for p in n["parameters"]["parameters"]]
            outs = [canon(p["typeDescriptions"]["typeString"]) for p in n["returnParameters"]["parameters"]]
            if fkind == "constructor":
                fns.append({"kind": "constructor", "name": "constructor",
                            "signature": f"constructor({','.join(ins)})", "selector": None,
                            "visibility": n["visibility"], "mutability": n["stateMutability"],
                            "inputs": ins, "outputs": []})
                continue
            sig = f"{n['name']}({','.join(ins)})"
            vis = n["visibility"]
            sel = selector_of(n, sig) if vis in ("external", "public") else None
            fns.append({"kind": "function", "name": n["name"], "signature": sig, "selector": sel,
                        "visibility": vis, "mutability": n["stateMutability"], "inputs": ins,
                        "outputs": outs})
        else:
            raise Blocked(f"{kind} in the contract")
    return state, fns


def lean_str(s: str) -> str:
    return json.dumps(s)


def lean_list(xs: list[str]) -> str:
    return "[" + ", ".join(lean_str(x) for x in xs) + "]"


def to_lean(doc: dict, ns: str) -> str:
    sv = ",\n    ".join(
        f"{{ name := {lean_str(v['name'])}, type := {lean_str(v['type'])}, slot := {v['slot']}, "
        f"offset := {v['offset']}, bytes := {v['bytes']} }}" for v in doc["state_variables"])
    fs = ",\n    ".join(
        f"{{ kind := {lean_str(f['kind'])}, name := {lean_str(f['name'])}, "
        f"signature := {lean_str(f['signature'])}, "
        f"selector := {'none' if f['selector'] is None else 'some ' + str(f['selector'])}, "
        f"visibility := {lean_str(f['visibility'])}, mutability := {lean_str(f['mutability'])}, "
        f"inputs := {lean_list(f['inputs'])}, outputs := {lean_list(f['outputs'])} }}"
        for f in doc["functions"])
    sels = [f for f in doc["functions"] if f["selector"] is not None]
    k = "SolidCore.Solidity.Source.Keccak.selector4"
    table = "\n".join(f"  {'if' if i == 0 else 'else if'} s = {lean_str(f['signature'])} then {f['selector']}"
                      for i, f in enumerate(sels))
    lemmas = "\n".join(f"def sel{i} : Nat := {k} {lean_str(f['signature'])}\n"
                       f"kernel_rfl sel{i}_eq sel{i} {f['selector']}" for i, f in enumerate(sels))
    cases = "\n".join(f"  by_cases h{i} : s = {lean_str(f['signature'])}\n"
                      f"  · rw [if_pos h{i}, h{i}]; exact sel{i}_eq\n"
                      f"  rw [if_neg h{i}]" for i, f in enumerate(sels))
    return f"""-- Generated by solar's tools/hir-shadow/hir_shadow.py (HIR shadow version {doc['version']}). Do not edit.

import SolidCore.Solidity.Keccak
import SolidCore.Refinement.KernelRfl
import Verify.HirShadow.Facts

namespace {ns}
open Verify.HirShadow

/-- solar's facts of `{doc['contract']}` (untrusted until `Verify.HirShadow.check` agrees). -/
def facts : Facts :=
  {{ version := {doc['version']},
    solarCommit := {lean_str(doc['solar']['commit'])},
    solarBinarySha256 := {lean_str(doc['solar']['binary_sha256'])},
    source := {lean_str(doc['source']['path'])},
    sourceSha256 := {lean_str(doc['source']['sha256'])},
    contract := {lean_str(doc['contract'])},
    stateVars := [
    {sv}],
    functions := [
    {fs}] }}

/-- Keccak-256 selectors, by the signatures solar states. -/
def selectorTable (s : String) : Nat :=
{table}
  else {k} s

-- Each selector solar states is Keccak-256 of its signature: one kernel evaluation each. A wrong
-- one fails here, so the facts are rejected before any check reads them.
{lemmas}

theorem selector_eq : {k} = selectorTable := by
  funext s
  unfold selectorTable
{cases}

end {ns}
"""


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("source")
    ap.add_argument("--solar", required=True)
    ap.add_argument("--solar-commit", default="")
    ap.add_argument("--contract", required=True)
    ap.add_argument("--json", required=True)
    ap.add_argument("--lean")
    ap.add_argument("--namespace", default="Verify.HirShadow.Generated")
    a = ap.parse_args()
    src = pathlib.Path(a.source)
    commit = a.solar_commit
    if not commit:
        here = pathlib.Path(__file__).resolve().parent
        commit = subprocess.run(["git", "-C", str(here), "rev-parse", "HEAD"], capture_output=True,
                                text=True, check=True).stdout.strip()
    res = compile_json(a.solar, src)
    try:
        state, fns = facts(res, src.name, a.contract)
    except Blocked as e:
        print(f"BLOCKED: {a.contract}: {e}")
        sys.exit(2)
    doc = {"format": "solar-hir-shadow", "version": VERSION,
           "solar": {"commit": commit, "binary_sha256": sha256(pathlib.Path(a.solar))},
           "source": {"path": a.source, "sha256": sha256(src)}, "contract": a.contract,
           "state_variables": state, "functions": fns}
    pathlib.Path(a.json).write_text(json.dumps(doc, indent=2) + "\n")
    if a.lean:
        pathlib.Path(a.lean).write_text(to_lean(doc, a.namespace))
    print(f"hir-shadow: {a.contract}: {len(state)} state variables, {len(fns)} functions")


if __name__ == "__main__":
    main()
