#!/usr/bin/env python3
"""test_join.py: join_controls.py over stand-in c1 and c2 archives."""
import hashlib, io, json, pathlib, subprocess, sys, tarfile, tempfile

here = pathlib.Path(__file__).resolve().parent
CONTROLS = ["control-flag", "control-siblingok", "control-flagrenamed"]
REC = {"solar": {"build": {"provenance_version": 2}}, "strict": {"tier": "certified", "holds": True},
       "tier_reached": "certified",
       "replay": {"closure": 9, "replayed_count": 9, "trusted_count": 0, "unaccounted": []}}


def tgz(path, files):
    with tarfile.open(path, "w:gz") as tf:
        for name, data in files.items():
            b = data if isinstance(data, bytes) else data.encode()
            ti = tarfile.TarInfo(name); ti.size = len(b); tf.addfile(ti, io.BytesIO(b))


def make(t, c2_status=None, c2_rec=None, drop=None, tamper=None, missing=None):
    t = pathlib.Path(t); c1 = t / "c1"; c2 = t / "c2"; c1.mkdir(); c2.mkdir()
    for n in CONTROLS:
        rec = json.dumps(REC)
        tgz(c1 / f"{n}.tgz", {f"{n}/status": "START driver=x\nDONE exit=0 expect=0 match=yes class=control stage_ok=no replay=skipped-c1 producer=ok\n",
                              f"{n}/certify.log": f"== x\nCERTIFIED: {n}: the requested tier holds (record r)\n",
                              f"{n}/rec/record.json": rec, f"{n}/replay.exit": "skipped-c1\n",
                              f"{n}/producer-check.json": json.dumps({"result": "ok"})})
        st = (c2_status or {}).get(n, "DONE exit=0 replay=PASS closure=9 replayed=9 trusted=0 axioms=Classical.choice,Quot.sound,propext")
        tgz(c2 / f"{n}.tgz", {f"{n}/status": "START x\n" + st + "\n", f"{n}/rec/record.json": (c2_rec or {}).get(n, rec)})
    for d, f in ((c1, t / "c1.sha256"), (c2, t / "c2.sha256")):
        lines = [f"{hashlib.sha256((d / f'{n}.tgz').read_bytes()).hexdigest()}  {n}.tgz" for n in CONTROLS if not (d == c2 and n == drop)]
        if tamper and d == c2:
            lines[0] = "0" * 64 + lines[0][64:]
        f.write_text("\n".join(lines) + "\n")
    if missing:
        (c2 / f"{missing}.tgz").unlink()
    return t


def run(name, want_rc, want, **kw):
    with tempfile.TemporaryDirectory() as t:
        t = make(t, **kw)
        r = subprocess.run([sys.executable, str(here / "join_controls.py"), str(t / "c1"), str(t / "c1.sha256"),
                            str(t / "c2"), str(t / "c2.sha256"), str(t / "out")], capture_output=True, text=True)
        tb = subprocess.run([sys.executable, str(here / "table.py"), str(t / "out")], capture_output=True, text=True).stdout
        gate = next((l for l in tb.splitlines() if l.startswith("Control gate:")), "")
        ok = r.returncode == want_rc and want in (r.stdout + " " + gate)
        print(("ok " if ok else "FAIL ") + name + ": " + r.stdout.strip() + " | " + gate)
        return ok


res = [
    run("all three c2 PASS: join PASS, table gate PASS", 0, "Control gate: PASS"),
    run("one c2 replay FAIL", 1, "control-siblingok: stage_ok=no", c2_status={"control-siblingok": "DONE exit=1 replay=FAIL(exit 1; no reproduced line) closure=None"}),
    run("one c2 replay killed", 1, "control-flag: stage_ok=no", c2_status={"control-flag": "KILLED-DEADLINE exit=124 replay=FAIL(exit 124) closure=None"}),
    run("c2 replayed a different record", 1, "control-flagrenamed: c2 replayed a different record", c2_rec={"control-flagrenamed": json.dumps({**REC, "tier_reached": "x"})}),
    run("a c2 digest missing from its list", 1, "control-flagrenamed: c2 archive not in its digest list", drop="control-flagrenamed"),
    run("a c2 digest mismatch", 1, "control-flag: c2 archive digest mismatch", tamper=True),
    run("a c2 archive missing", 1, "control-siblingok: c2 archive missing", missing="control-siblingok"),
    run("failed join leaves the table gate FAIL", 1, "Control gate: FAIL", c2_status={"control-siblingok": "DONE exit=0 replay=FAIL(axioms ['sorryAx'])"}),
]
print(f"fails={res.count(False)}")
sys.exit(res.count(False))
