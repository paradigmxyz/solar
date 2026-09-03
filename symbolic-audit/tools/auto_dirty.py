#!/usr/bin/env python3
"""Send non-canonical ABI words to every probe function with narrow value parameters; both compilers must agree."""
import json, re, subprocess, sys
shard, nshards, *files = sys.argv[1:]
shard, nshards = int(shard), int(nshards)
def dirty(t):
    m = re.fullmatch(r"uint(\d+)", t)
    if m and int(m.group(1)) < 256: return (1 << int(m.group(1))) | 1
    m = re.fullmatch(r"int(\d+)", t)
    if m and int(m.group(1)) < 256: return 1 << int(m.group(1))
    if t == "bool": return 2
    if t == "address": return (1 << 160) | 5
    m = re.fullmatch(r"bytes(\d+)", t)
    if m and int(m.group(1)) < 32: return (0xab << 248) | 1
    return None
def clean(t):
    m = re.fullmatch(r"uint(\d*)", t)
    if m: return 3
    m = re.fullmatch(r"int(\d*)", t)
    if m: return (1 << 256) - 2
    if t == "bool": return 1
    if t == "address": return 5
    m = re.fullmatch(r"bytes(\d+)", t)
    if m: return 0xab << 248
    return None
seen = set(); n = 0
for f in files:
    for l in open(f):
        r = json.loads(l)
        key = (r["file"], r["contract"], r["signature"])
        if key in seen: continue
        seen.add(key)
        types = re.match(r".*\((.*)\)$", r["signature"]).group(1)
        ts = [t for t in types.split(",") if t]
        if not ts or any(clean(t) is None for t in ts) or not any(dirty(t) for t in ts): continue
        n += 1
        if n % nshards != shard: continue
        cases = []
        cases.append("raw:" + " ".join(str(dirty(t) or clean(t)) for t in ts))
        for i, t in enumerate(ts):
            if dirty(t): cases.append("raw:" + " ".join(str(dirty(t) if j == i else clean(u)) for j, u in enumerate(ts)))
        cases = list(dict.fromkeys(cases))[:5]
        out = subprocess.run(["python3", "target/symaudit/concrete.py", r["file"], r["contract"], r["signature"], *cases], capture_output=True, text=True).stdout
        lines = [x for x in out.splitlines() if "mismatches" in x or "MISMATCH" in x or "solc " in x or "solar " in x]
        print(r["file"].split("/")[-1], "\n".join(lines[:8]) or out[-200:], flush=True)
