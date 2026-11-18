#!/usr/bin/env python3
"""test_gate.py: producer_check.py and gate.py on stand-in records, including a refusal before the
build (falseenv's record shape)."""
import json, pathlib, subprocess, sys, tempfile

here = pathlib.Path(__file__).resolve().parent
COMMIT = "65d87ab35420acd048ee1015113b6a887ab065ba"
PROD = {"binary_sha256": "ab" * 32, "solar_commit": COMMIT, "binary_path": "/x/solar",
        "build_command": "cargo build --locked --bin solar"}
BUILT = {"status": "REJECTED", "solar": {"commit": COMMIT, "dirty": False, "binary_sha256": "ab" * 32,
         "binary_commit": COMMIT, "build": {"provenance_version": 2}}}
PRE = {"status": "BLOCKED", "solar": {"commit": COMMIT, "dirty": False},
       "blocked": [{"feature": "environment"}]}
ENV = "^BLOCKED: .*a spec clause reads the environment \\(block\\.chainid\\)"
ENVLINE = "BLOCKED: environment (1x, first line 12: a spec clause reads the environment (block.chainid), which the statement fixes)"
SEM = "^REJECTED: spec: \\S*\\braise_meets0 does not check"
SEMLINE = "REJECTED: spec: raise_meets0 does not check: x"


def case(t, rec, line, cmd="tools/certify/certify x natspec"):
    d = pathlib.Path(t) / "case"; (d / "rec").mkdir(parents=True)
    (d / "rec" / "record.json").write_text(json.dumps(rec))
    (d / "certify.log").write_text("== x\n" + line + "\n")
    (d / "command").write_text(cmd)
    p = pathlib.Path(t) / "producer.json"; p.write_text(json.dumps(PROD))
    r = subprocess.run([sys.executable, str(here / "producer_check.py"), str(p), str(d)], capture_output=True, text=True)
    (d / "producer-check.json").write_text(r.stdout)
    return d, json.loads(r.stdout)["result"], r.returncode


def gate(d, cls, stage):
    r = subprocess.run([sys.executable, str(here / "gate.py"), str(d), cls, stage, "DONE"], capture_output=True, text=True)
    return r.stdout.strip()


res = []


def check(name, cond, info):
    print(("ok " if cond else "FAIL ") + name + ": " + info); res.append(cond)


with tempfile.TemporaryDirectory() as t:
    d, pr, rc = case(t, PRE, ENVLINE)
    check("pre-build BLOCKED, clean producer commit: not_built", pr == "not_built" and rc == 0, f"{pr} rc={rc}")
    check("failclosed accepts not_built", gate(d, "failclosed", ENV) == "stage_ok=yes", gate(d, "failclosed", ENV))
    check("semantic refuses not_built", gate(d, "semantic", ENV) == "stage_ok=no", gate(d, "semantic", ENV))
    check("control refuses not_built", gate(d, "control", ENV) == "stage_ok=no", gate(d, "control", ENV))
with tempfile.TemporaryDirectory() as t:
    d, pr, rc = case(t, {**PRE, "solar": {"commit": "0" * 40, "dirty": False}}, ENVLINE)
    check("pre-build record at another commit: mismatch", pr == "mismatch" and rc == 1, pr)
    check("failclosed refuses it", gate(d, "failclosed", ENV) == "stage_ok=no", gate(d, "failclosed", ENV))
with tempfile.TemporaryDirectory() as t:
    d, pr, _ = case(t, {**PRE, "solar": {"commit": COMMIT, "dirty": True}}, ENVLINE)
    check("pre-build record from a dirty checkout: mismatch", pr == "mismatch", pr)
with tempfile.TemporaryDirectory() as t:
    d, pr, _ = case(t, {**PRE, "status": "REJECTED"}, ENVLINE)
    check("a non-BLOCKED record without a binary: mismatch", pr == "mismatch", pr)
with tempfile.TemporaryDirectory() as t:
    d, pr, _ = case(t, PRE, ENVLINE, cmd="tools/certify/certify x natspec --probe-cache /c")
    check("pre-build record with a stage cache: mismatch", pr == "mismatch", pr)
with tempfile.TemporaryDirectory() as t:
    d, pr, _ = case(t, {**PRE, "solar": {"commit": COMMIT, "dirty": False, "build": {"provenance_version": 2}}}, ENVLINE)
    check("BLOCKED with a build block but no binary: mismatch", pr == "mismatch", pr)
with tempfile.TemporaryDirectory() as t:
    d, pr, _ = case(t, BUILT, SEMLINE)
    check("built record: ok", pr == "ok", pr)
    check("semantic with ok and provenance 2 passes", gate(d, "semantic", SEM) == "stage_ok=yes", gate(d, "semantic", SEM))
    check("failclosed with ok and provenance 2 still passes", gate(d, "failclosed", SEM) == "stage_ok=yes", gate(d, "failclosed", SEM))
with tempfile.TemporaryDirectory() as t:
    d, pr, _ = case(t, {**BUILT, "solar": {**BUILT["solar"], "binary_sha256": "cd" * 32}}, SEMLINE)
    check("built record with another binary: mismatch, semantic fails", pr == "mismatch" and gate(d, "semantic", SEM) == "stage_ok=no", pr)
with tempfile.TemporaryDirectory() as t:
    d, pr, _ = case(t, {**BUILT, "solar": {**BUILT["solar"], "build": {"provenance_version": 1}}}, SEMLINE)
    check("ok producer but provenance 1: semantic fails", gate(d, "semantic", SEM) == "stage_ok=no", gate(d, "semantic", SEM))
with tempfile.TemporaryDirectory() as t:
    d, pr, _ = case(t, BUILT, SEMLINE)
    (d / "producer-check.json").unlink()
    check("no producer-check.json: semantic fails", gate(d, "semantic", SEM) == "stage_ok=no", gate(d, "semantic", SEM))
print(f"fails={res.count(False)}")
sys.exit(res.count(False))
