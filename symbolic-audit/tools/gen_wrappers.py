#!/usr/bin/env python3
"""Generate an external wrapper contract for every internal pure/view library or free function in a file.

usage: gen_wrappers.py <base_dir> <rel_source> <out_project_dir>
Creates out_project_dir/W.sol, out_project_dir/ROOT.txt, and hard-links base_dir's tree into out_project_dir.
"""
import json, os, pathlib, re, subprocess, sys
base, rel, out = pathlib.Path(sys.argv[1]).resolve(), sys.argv[2], pathlib.Path(sys.argv[3])
r = subprocess.run(["solc", "--base-path", str(base), "--ast-compact-json", "--stop-after", "parsing", rel], capture_output=True, text=True, cwd=base)
if r.returncode != 0:
    print(r.stderr[-2000:]); sys.exit(1)
# Output has "======= path =======\nJSON" sections; take the one for rel.
sections = re.split(r"^======= (.*?) =======$", r.stdout, flags=re.M)
ast = None
for i in range(1, len(sections), 2):
    if sections[i].strip() == rel:
        ast = json.loads(sections[i + 1]); break
if ast is None:
    print("no ast for", rel, [sections[i].strip() for i in range(1, len(sections), 2)][:5]); sys.exit(1)
BAD = ("function", "mapping", "storage", "super", "type(", "module")
LOCAL = {}
def conv(p):
    ts = p["typeName"]
    loc = p.get("storageLocation", "default")
    def tn(t):
        k = t["nodeType"]
        if k == "ElementaryTypeName": return t["name"] if t["name"] != "address" or t.get("stateMutability") != "payable" else "address payable"
        if k == "UserDefinedTypeName":
            n = t.get("pathNode", {}).get("name") or t["name"]
            if n in LOCAL and "." not in n: n = LOCAL[n] + "." + n
            return n
        if k == "ArrayTypeName":
            inner = tn(t["baseType"]);
            if inner is None: return None
            ln = t.get("length")
            if ln is None: return inner + "[]"
            if ln["nodeType"] == "Literal": return f"{inner}[{ln['value']}]"
            if ln["nodeType"] == "Identifier": return f"{inner}[{ln['name']}]"
            return None
        return None
    name = tn(ts)
    if name is None: return None
    if loc == "storage": return None
    if loc in ("memory", "calldata"): name += " " + loc
    return name
wrappers = []
def emit(fn, qual):
    if fn["kind"] not in ("function", "freeFunction") or fn.get("stateMutability") not in ("pure", "view"): return
    if fn.get("visibility") == "private": return
    params = [conv(p) for p in fn["parameters"]["parameters"]]
    rets = [conv(p) for p in fn["returnParameters"]["parameters"]]
    if any(p is None for p in params + rets): return
    name = fn["name"]; idx = len(wrappers)
    args = ", ".join(f"{t} a{i}" for i, t in enumerate(params))
    call = f"{qual}{name}({', '.join(f'a{i}' for i in range(len(params)))})"
    ret = f" returns ({', '.join(rets)})" if rets else ""
    body = f"return {call};" if rets else f"{call};"
    wrappers.append(f"    function w{idx}_{name}({args}) external {fn['stateMutability']}{ret} {{ {body} }}")
for node in ast["nodes"]:
    if node["nodeType"] == "FunctionDefinition": emit(node, "")
    elif node["nodeType"] == "ContractDefinition" and node.get("contractKind") == "library":
        for sub in node["nodes"]:
            if sub["nodeType"] in ("EnumDefinition", "StructDefinition", "UserDefinedValueTypeDefinition"): LOCAL[sub["name"]] = node["name"]
        for sub in node["nodes"]:
            if sub["nodeType"] == "FunctionDefinition" and sub.get("visibility") in ("internal",): emit(sub, node["name"] + ".")
if not wrappers:
    print("no wrappers for", rel); sys.exit(0)
out.mkdir(parents=True, exist_ok=True)
for top in os.listdir(base):
    dst = out / top
    if not dst.exists():
        subprocess.run(["cp", "-al", str(base / top), str(dst)], check=True)
cname = "W_" + re.sub(r"\W", "_", pathlib.Path(rel).stem)
(out / "W.sol").write_text(f'// SPDX-License-Identifier: MIT\npragma solidity ^0.8.20;\nimport "./{rel}";\ncontract {cname} {{\n' + "\n".join(wrappers) + "\n}\n")
(out / "ROOT.txt").write_text("W.sol\n")
print(f"{rel}: {len(wrappers)} wrappers -> {out}")
