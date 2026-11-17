#!/usr/bin/env python3
"""test_table.py: the control gate over the registered controls (cases.tsv, stages.tsv)."""
import json, pathlib, subprocess, sys, tempfile

here = pathlib.Path(__file__).resolve().parent
CONTROLS = ["control-flag", "control-siblingok", "control-flagrenamed"]


def good(**over):
    c = {"requested_tier": "certified", "tier_reached": "certified", "tier_reached_is_requested": True,
         "strict_holds": True, "replay_full": True, "trusted_count": 0, "independent_replay_exit": "0"}
    c.update(over)
    return {"class": "control", "kind": "DONE", "verdict_matches": True, "control": c, "stage_ok": True}


def table(out):
    r = subprocess.run([sys.executable, str(here / "table.py"), str(out)], capture_output=True, text=True)
    assert r.returncode == 0, r.stderr
    return next(l for l in r.stdout.splitlines() if l.startswith("Control gate:"))


def run(name, gates, want):
    with tempfile.TemporaryDirectory() as t:
        out = pathlib.Path(t)
        for n, g in gates.items():
            (out / n).mkdir()
            (out / n / "status").write_text("START driver=x\nDONE exit=0 expect=0\n")
            if g is not None:
                (out / n / "gate.json").write_text(json.dumps(g))
        line = table(out)
        ok = line.startswith(f"Control gate: {want}")
        print(("ok " if ok else "FAIL ") + name + ": " + line)
        return ok


all_good = {n: good() for n in CONTROLS}
res = [
    run("all three pass", all_good, "PASS"),
    run("control-flagrenamed missing", {n: good() for n in CONTROLS[:2]}, "FAIL"),
    run("control-flag missing", {n: good() for n in CONTROLS[1:]}, "FAIL"),
    run("no controls at all", {}, "FAIL"),
    run("one control without gate.json", {**all_good, "control-siblingok": None}, "FAIL"),
    run("one control below certified", {**all_good, "control-flag": good(tier_reached="functional", tier_reached_is_requested=False)}, "FAIL"),
    run("one control requested a lower tier", {**all_good, "control-flag": good(requested_tier="functional")}, "FAIL"),
    run("one control's independent replay exit 1", {**all_good, "control-flagrenamed": good(independent_replay_exit="1")}, "FAIL"),
    run("one control's independent replay not run", {**all_good, "control-flagrenamed": good(independent_replay_exit="not-run")}, "FAIL"),
    run("one control with trusted replay", {**all_good, "control-siblingok": good(replay_full=False, trusted_count=1)}, "FAIL"),
    run("one control killed", {**all_good, "control-flag": {**good(), "kind": "KILLED-DEADLINE"}}, "FAIL"),
    run("one control with stage_ok false", {**all_good, "control-flag": {**good(), "stage_ok": False}}, "FAIL"),
    run("extra unregistered control dir ignored", {**all_good, "control-extra": good()}, "PASS"),
]
print(f"fails={res.count(False)}")
sys.exit(res.count(False))
