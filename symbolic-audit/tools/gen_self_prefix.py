#!/usr/bin/env python3
"""Build a task file where each nonpayable probe function is first called once with representative
arguments (a concrete prefix), then compared symbolically against non-zero storage."""
import json, re, subprocess, sys, glob
def val(t):
    m = re.fullmatch(r"uint(\d*)", t)
    if m: b = int(m.group(1) or 256); return str(min(3, (1 << b) - 1))
    m = re.fullmatch(r"int(\d*)", t)
    if m: return "-2"
    if t == "bool": return "true"
    if t == "address": return "0x0000000000000000000000000000000000000005"
    m = re.fullmatch(r"bytes(\d+)", t)
    if m: return "0x" + "ab" * int(m.group(1))
    if t == "bytes": return "0x0102"
    if t == "string": return "hi"
    if t.endswith("[]"): inner = val(t[:-2]); return f"[{inner},{inner}]" if inner else None
    m = re.fullmatch(r"(.*)\[(\d+)\]", t)
    if m: inner = val(m.group(1)); return "[" + ",".join([inner] * int(m.group(2))) + "]" if inner else None
    return None
tasks = []; seen = set()
for f in sys.argv[1:]:
    for l in open(f):
        r = json.loads(l)
        if r["mutability"] != "nonpayable": continue
        key = (r["file"], r["contract"], r["signature"])
        if key in seen: continue
        seen.add(key)
        types = re.match(r".*\((.*)\)$", r["signature"]).group(1)
        if "(" in types: continue
        ts = [t for t in types.split(",") if t]
        args = [val(t) for t in ts]
        if any(a is None for a in args): continue
        cd = subprocess.run(["cast", "calldata", r["signature"], *args], capture_output=True, text=True).stdout.strip()
        if not cd.startswith("0x"): continue
        tasks.append(f"{r['file']}|{r['contract']}|{r['signature']}|nonpayable|{cd}")
open("target/symaudit/prefix-self-tasks.txt", "w").write("\n".join(tasks) + "\n")
print(len(tasks), "tasks")
