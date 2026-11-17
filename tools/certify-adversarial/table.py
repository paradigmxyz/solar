#!/usr/bin/env python3
"""table.py OUT: one row per case directory of OUT, as a Markdown table, then the gate.

Columns: case, the driver (solar) and solidity-lean commits, expected and actual exit, class, kind
(DONE, KILLED-DEADLINE, NOT-ADMITTED, REFUSED-*), whether the verdict matches the case's registered
stage (gate.json, written by gate.py), the independent replay's exit for a control, the producer
check, and the sha256 of certify.log. An expectation of "1|3" accepts either code.

The gate (CASES.md, Rerun criteria): no negative counts unless every control's stage_ok holds
(the requested tier reached, the strict predicate, the full replay with trusted 0, and the
independent replay exiting 0). A semantic negative then counts only with stage_ok: a rejection
elsewhere (an unsupported feature or tree mismatch, an input or infrastructure failure, a timeout
or a resource kill) is not a semantic rejection. A failclosed case is never evidence."""
import json, pathlib, re, sys

out = pathlib.Path(sys.argv[1])
rows, controls, sem = [], [], []
for d in sorted(p for p in out.iterdir() if p.is_dir() and not p.name.startswith(".") and p.name != "outbox"):
    st = (d / "status").read_text().splitlines() if (d / "status").exists() else []
    start = next((l for l in st if l.startswith("START")), "")
    end = st[-1] if st else ""
    kv = dict(re.findall(r"(\w+)=(\S+)", start + " " + end))
    kind = end.split()[0] if end else "MISSING"
    g = json.loads((d / "gate.json").read_text()) if (d / "gate.json").exists() else {}
    cls = g.get("class") or kv.get("class", "")
    ok = g.get("stage_ok") is True
    match = "" if kind != "DONE" else ("yes" if kv.get("exit") in kv.get("expect", "").split("|") else "**NO**")
    if cls == "control":
        controls.append((d.name, ok))
    elif cls == "semantic":
        sem.append((d.name, ok))
    rows.append([d.name, kv.get("driver", kv.get("solar", ""))[:10], kv.get("lean", "")[:10],
                 kv.get("expect", ""), kv.get("exit", ""), match, cls, kind,
                 "yes" if ok else ("no" if g else ""), kv.get("replay", ""), kv.get("producer", ""),
                 kv.get("log_sha256", "")])
print("| case | solar | lean | expect | exit | match | class | kind | stage_ok | replay | producer | certify.log sha256 |")
print("|---|---|---|---|---|---|---|---|---|---|---|---|")
for r in rows:
    print("| " + " | ".join(x.replace("|", "\\|") for x in r) + " |")
gate = bool(controls) and all(ok for _, ok in controls)
print()
print(f"Control gate: {'PASS' if gate else 'FAIL'} ({', '.join(f'{n} {"ok" if o else "no"}' for n, o in controls) or 'no controls'})")
counted = [n for n, o in sem if o] if gate else []
print(f"Semantic rejections counted: {len(counted)} of {len(sem)}" + (f": {', '.join(counted)}" if counted else ""))
