#!/usr/bin/env python3
"""table.py OUT: one row per case directory of OUT, as a Markdown table, then the gate.

Columns: case, the driver (solar) and solidity-lean commits, expected and actual exit, class, kind
(DONE, KILLED-DEADLINE, NOT-ADMITTED, REFUSED-*), whether the verdict matches the case's registered
stage (gate.json, written by gate.py), the independent replay's exit for a control, the producer
check, and the sha256 of certify.log. An expectation of "1|3" accepts either code.

table.py OUT [CASES [STAGES]]: CASES and STAGES default to this directory's cases.tsv and
stages.tsv. The controls are the cases CASES registers with class control in STAGES (they must
be exactly its expect-0 cases); a missing control directory, a missing gate.json, a tier below
certified or an independent --replay not exiting 0 fails the gate.

The gate (CASES.md, Rerun criteria): no negative counts unless every control's stage_ok holds
(the requested tier reached, the strict predicate, the full replay with trusted 0, and the
independent replay exiting 0). A semantic negative then counts only with stage_ok: a rejection
elsewhere (an unsupported feature or tree mismatch, an input or infrastructure failure, a timeout
or a resource kill) is not a semantic rejection. A failclosed case is never evidence."""
import json, pathlib, re, sys

here = pathlib.Path(__file__).resolve().parent
out = pathlib.Path(sys.argv[1])
cases_file = pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else here / "cases.tsv"
stages_file = pathlib.Path(sys.argv[3]) if len(sys.argv) > 3 else here / "stages.tsv"


def rows_of(path, sep=None):
    return [l.rstrip("\n").split(sep) for l in path.read_text().splitlines()
            if l.strip() and not l.startswith("#")]


# The registered controls: every case of cases.tsv whose class in stages.tsv is control. The gate
# is over these names, not over the directories a run left, so a missing control fails it.
cls_of = {r[0]: r[1] for r in rows_of(stages_file, "\t")}
registered = [r[0] for r in rows_of(cases_file)]
want_controls = [n for n in registered if cls_of.get(n) == "control"]
expect0 = [r[0] for r in rows_of(cases_file) if r[2] == "0"]


def control_check(name):
    """Why the registered control NAME fails the gate, or None when it passes."""
    d = out / name
    if not d.is_dir():
        return "missing"
    gp = d / "gate.json"
    if not gp.exists():
        return "no gate.json"
    try:
        g = json.loads(gp.read_text())
    except Exception as e:
        return f"unreadable gate.json ({type(e).__name__})"
    c = g.get("control") or {}
    if g.get("class") != "control":
        return f"gate class {g.get('class')!r}"
    if g.get("kind") != "DONE":
        return f"kind {g.get('kind')}"
    if not g.get("verdict_matches"):
        return "verdict is not CERTIFIED"
    if c.get("tier_reached_is_requested") is not True or c.get("requested_tier") != "certified":
        return f"tier {c.get('tier_reached')!r} (requested {c.get('requested_tier')!r})"
    if c.get("strict_holds") is not True:
        return "strict predicate does not hold"
    if c.get("replay_full") is not True:
        return f"replay not full (trusted {c.get('trusted_count')!r})"
    if c.get("independent_replay_exit") != "0":
        return f"independent --replay exit {c.get('independent_replay_exit')!r}"
    if g.get("stage_ok") is not True:
        return "stage_ok false"
    return None


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
fails = {n: control_check(n) for n in want_controls}
problems = [f"{n} {why}" for n, why in fails.items() if why]
if not want_controls:
    problems.append("no registered controls")
if sorted(want_controls) != sorted(expect0):
    problems.append(f"controls {sorted(want_controls)} are not the expect-0 cases {sorted(expect0)}")
gate = not problems
print()
print(f"Control gate: {'PASS' if gate else 'FAIL'} ("
      + ", ".join(f"{n} {'ok' if not why else 'no: ' + why}" for n, why in fails.items())
      + (f"; {'; '.join(p for p in problems if not p.split()[0] in fails)}" if any(not p.split()[0] in fails for p in problems) else "")
      + ")")
counted = [n for n, o in sem if o] if gate else []
print(f"Semantic rejections counted: {len(counted)} of {len(sem)}" + (f": {', '.join(counted)}" if counted else ""))
