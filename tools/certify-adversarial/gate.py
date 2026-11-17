#!/usr/bin/env python3
"""gate.py CASE_DIR CLASS STAGE KIND: the case's gate, written to CASE_DIR/gate.json and printed as
"stage_ok=yes|no". The verdict line is certify.log's last non-empty line. stage_ok holds only when
the run ended (KIND, from case.sh, is DONE) and the line matches STAGE (re.search). A control also needs, from
its record's typed fields: tier_reached equal to the requested tier, the strict predicate holding,
replay closure fully replayed with trusted_count 0 and nothing unaccounted, and the independent
replay (replay.exit, written by case.sh) exiting 0."""
import json, pathlib, re, sys

d, cls, stage, kind = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3], sys.argv[4]
log = d / "certify.log"
lines = [l for l in log.read_text(errors="replace").splitlines() if l.strip()] if log.exists() else []
verdict = lines[-1] if lines else ""
g = {"class": cls, "stage": stage, "kind": kind, "verdict": verdict[:2000],
     "verdict_matches": bool(re.search(stage, verdict))}
ok = kind == "DONE" and g["verdict_matches"]
if cls == "control":
    try:
        rec = json.loads((d / "rec" / "record.json").read_text())
    except Exception as e:
        rec = {}
        g["record_error"] = f"{type(e).__name__}: {e}"
    st = rec.get("strict") or {}
    rp = rec.get("replay") or {}
    want = st.get("tier")
    rx = (d / "replay.exit").read_text().strip() if (d / "replay.exit").exists() else "not-run"
    c = {
        "requested_tier": want,
        "tier_reached": rec.get("tier_reached"),
        "tier_reached_is_requested": want is not None and rec.get("tier_reached") == want,
        "strict_holds": st.get("holds") is True,
        "replay_full": isinstance(rp.get("closure"), int) and rp.get("closure", 0) > 0
        and rp.get("replayed_count") == rp.get("closure") and rp.get("trusted_count") == 0
        and rp.get("unaccounted") == [],
        "trusted_count": rp.get("trusted_count"),
        "independent_replay_exit": rx,
    }
    g["control"] = c
    ok = ok and c["tier_reached_is_requested"] and c["strict_holds"] and c["replay_full"] and rx == "0"
g["stage_ok"] = ok
(d / "gate.json").write_text(json.dumps(g, indent=1) + "\n")
print("stage_ok=" + ("yes" if ok else "no"))
