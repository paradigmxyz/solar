#!/usr/bin/env python3
"""table.py OUT: one row per case directory of OUT, as a Markdown table.

Columns: case, solar and solidity-lean commits, expected and actual exit, the kind (DONE,
KILLED-DEADLINE, NOT-ADMITTED), the stage the verdict line names, the producer check, and the
sha256 of certify.log. The stage is read from the driver's last line only; a row whose exit is not
the expected one is marked. An expectation of "1|3" accepts either code."""
import pathlib, re, sys

out = pathlib.Path(sys.argv[1])
rows = []
for d in sorted(p for p in out.iterdir() if p.is_dir() and not p.name.startswith(".") and p.name != "outbox"):
    st = (d / "status").read_text().splitlines() if (d / "status").exists() else []
    start = next((l for l in st if l.startswith("START")), "")
    end = st[-1] if st else ""
    kv = dict(re.findall(r"(\w+)=(\S+)", start + " " + end))
    kind = end.split()[0] if end else "MISSING"
    verdict = ""
    if (d / "certify.log").exists():
        lines = [l for l in (d / "certify.log").read_text(errors="replace").splitlines() if l.strip()]
        verdict = lines[-1] if lines else ""
    m = re.match(r"(CERTIFIED|REJECTED|BLOCKED)(?::\s*([\w/. -]+?)(?::|\s\(|$))?", verdict)
    stage = (m.group(1) + (": " + m.group(2) if m.group(2) else "")) if m else ("(no verdict)" if kind != "DONE" else verdict[:60])
    match = "" if kind != "DONE" else ("yes" if kv.get("exit") in kv.get("expect", "").split("|") else "**NO**")
    rows.append([d.name, kv.get("solar", "")[:10], kv.get("lean", "")[:10], kv.get("expect", ""),
                 kv.get("exit", ""), match, kind, stage, kv.get("producer", ""), kv.get("log_sha256", "")])
print("| case | solar | lean | expect | exit | match | kind | stage | producer | certify.log sha256 |")
print("|---|---|---|---|---|---|---|---|---|---|")
for r in rows:
    print("| " + " | ".join(x.replace("|", "\\|") for x in r) + " |")
