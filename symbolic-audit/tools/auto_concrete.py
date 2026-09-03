#!/usr/bin/env python3
"""Replay incomplete wrapper functions from a results file concretely with boundary values per parameter type."""
import json, re, subprocess, sys, itertools
results, optflags = sys.argv[1], sys.argv[2:]
MAX = (1 << 256) - 1; HALF = 1 << 255; IMIN = -(1 << 255); IMAX = (1 << 255) - 1
def vals(t):
    m = re.fullmatch(r"uint(\d*)", t)
    if m: b = int(m.group(1) or 256); mx = (1 << b) - 1; return [0, 1, mx, mx // 2 + 1, 1 << (b // 2)] if b > 8 else [0, 1, mx, 128]
    m = re.fullmatch(r"int(\d*)", t)
    if m: b = int(m.group(1) or 256); mn = -(1 << (b - 1)); mx = (1 << (b - 1)) - 1; return [mn, -1, 0, 1, mx, mn // 2]
    if t == "bool": return ["true", "false"]
    if t == "address": return ["0x0000000000000000000000000000000000000000", "0xffffffffffffffffffffffffffffffffffffffff", "0x0000000000000000000000000000000000000001"]
    m = re.fullmatch(r"bytes(\d+)", t)
    if m: n = int(m.group(1)); return ["0x" + "00" * n, "0x" + "ff" * n, "0x" + "01" * n]
    if t in ("bytes", "string"): return ["0x", "0x01", "0x" + "ab" * 32, "0x" + "ab" * 33] if t == "bytes" else ["a", "", "k" * 33]
    if t.endswith("[]"):
        inner = vals(t[:-2]);
        return ["[]", f"[{inner[0]}]", f"[{inner[-1]},{inner[1] if len(inner)>1 else inner[0]}]"] if inner else None
    return None
seen = set()
for l in open(results):
    r = json.loads(l)
    if not r["contract"].startswith("W_") or r["status"] != "incomplete": continue
    if "toString" in str(r["reason"]): continue
    key = (r["file"], r["signature"])
    if key in seen: continue
    seen.add(key)
    sig = r["signature"]; types = re.match(r".*\((.*)\)$", sig).group(1)
    if "(" in types: print("skip struct", sig); continue
    ts = [t for t in types.split(",") if t]
    cols = [vals(t) for t in ts]
    if any(c is None for c in cols): print("skip", sig); continue
    combos = []
    for i in range(6):
        combos.append(" ".join(str(c[i % len(c)]) for c in cols))
    # Also cross-pair extremes for two-argument functions.
    if len(cols) == 2:
        for a in cols[0][:3]:
            for b in cols[1][:3]: combos.append(f"{a} {b}")
    combos = list(dict.fromkeys(combos))[:14] or [""]
    proj = "/home/doni/github/paradigmxyz/solar.3/" + r["file"].rsplit("/", 1)[0]
    cmd = ["python3", "target/symaudit/concrete.py", *optflags, "--project-root", proj, proj + "/W.sol", r["contract"], sig, *combos]
    out = subprocess.run(cmd, capture_output=True, text=True).stdout
    lines = [x for x in out.splitlines() if "mismatches" in x or "MISMATCH" in x or "solc " in x or "solar " in x]
    print(r["file"].split("/")[-2], "\n".join(lines[:8]) or out[-300:])
