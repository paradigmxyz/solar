#!/usr/bin/env python3
"""join_controls.py C1_DIR C1_SHAS C2_DIR C2_SHAS OUT: the controls' gate from their two phases.

c1 certified each control without its replay (PHASE=c1); c2 replayed each from its c1 archive on
another machine (c2.sh). For each control registered in cases.tsv and stages.tsv, this checks both
archives against their digest lists (lines "<sha256>  NAME.tgz"), unpacks the c1 case directory
into OUT/NAME, and records c2's verdict as the control's independent replay: replay.exit is "0"
only when c2's status line is DONE with exit 0 and replay=PASS, and is otherwise c2's exit or its
failure. It then reruns gate.py on the joined directory and writes OUT/NAME/join.json (both archive
digests, c2's status line). A missing archive, a digest mismatch or a c2 record that does not
replay this c1 record fails the control: its directory gets no gate.json, so table.py fails the
gate. Exits 0 only when every control joined and its stage_ok holds."""
import hashlib, json, pathlib, re, subprocess, sys, tarfile

here = pathlib.Path(__file__).resolve().parent
c1d, c1s, c2d, c2s, out = map(pathlib.Path, sys.argv[1:6])
out.mkdir(parents=True, exist_ok=True)


def rows(p, sep=None):
    return [l.split(sep) for l in p.read_text().splitlines() if l.strip() and not l.startswith("#")]


cls = {r[0]: (r[1], r[2]) for r in rows(here / "stages.tsv", "\t")}
controls = [r[0] for r in rows(here / "cases.tsv") if cls.get(r[0], ("",))[0] == "control"]
shas = {}
for f, tag in ((c1s, "c1"), (c2s, "c2")):
    for r in rows(f):
        shas[(tag, r[1].removesuffix(".tgz"))] = r[0]


def digest(p):
    h = hashlib.sha256()
    with open(p, "rb") as fh:
        for b in iter(lambda: fh.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def member(tf, name):
    f = tf.extractfile(name)
    return f.read() if f else None


failed = []
for n in controls:
    j = {"control": n}
    why = None
    for tag, d in (("c1", c1d), ("c2", c2d)):
        p = d / f"{n}.tgz"
        want = shas.get((tag, n))
        got = digest(p) if p.exists() else None
        j[f"{tag}_archive_sha256"] = got
        j[f"{tag}_expected_sha256"] = want
        if not want:
            why = why or f"{tag} archive not in its digest list"
        elif got is None:
            why = why or f"{tag} archive missing"
        elif got != want:
            why = why or f"{tag} archive digest mismatch"
    dst = out / n
    if dst.exists():
        subprocess.run(["rm", "-rf", str(dst)], check=True)
    if why is None:
        with tarfile.open(c1d / f"{n}.tgz") as tf:
            tf.extractall(out, filter="data")
        with tarfile.open(c2d / f"{n}.tgz") as tf:
            st2 = (member(tf, f"{n}/status") or b"").decode().splitlines()
            rec2 = member(tf, f"{n}/rec/record.json")
        last = st2[-1] if st2 else ""
        j["c2_status"] = last
        # c2 must have replayed exactly this c1 record.
        if rec2 is None or rec2 != (dst / "rec" / "record.json").read_bytes():
            why = "c2 replayed a different record"
        m = re.match(r"(\S+) exit=(\S+) (replay=\S+)", last)
        if why is None and m and m.group(1) == "DONE" and m.group(2) == "0" and m.group(3) == "replay=PASS":
            rx = "0"
        else:
            rx = (m.group(2) if m and m.group(2) != "0" else "fail") if why is None else "fail"
        (dst / "replay.exit").write_text(rx + "\n") if dst.exists() else None
        j["independent_replay_exit"] = rx
    if why is not None:
        dst.mkdir(parents=True, exist_ok=True)
        j["error"] = why
        (dst / "join.json").write_text(json.dumps(j, indent=1) + "\n")
        failed.append(f"{n}: {why}")
        continue
    st1 = (dst / "status").read_text().splitlines()
    kind = st1[-1].split()[0] if st1 else "MISSING"
    c, stage = cls[n]
    r = subprocess.run([sys.executable, str(here / "gate.py"), str(dst), c, stage, kind], capture_output=True, text=True)
    j["gate"] = r.stdout.strip()
    (dst / "join.json").write_text(json.dumps(j, indent=1) + "\n")
    if r.stdout.strip() != "stage_ok=yes":
        failed.append(f"{n}: {r.stdout.strip() or r.stderr.strip()}")
print("JOIN " + ("PASS" if not failed else "FAIL: " + "; ".join(failed)))
sys.exit(1 if failed else 0)
