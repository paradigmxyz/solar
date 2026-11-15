#!/usr/bin/env python3
"""The accounting of the kernel replay of a published statement's closure (scripts/certify 6b).

replay.py plan CHECK_LOG OUT --syslib DIR [--require MODULE]... [--include-trusted]
    reads the "MODULE NAME PATH replay|trusted IMPORTS..." lines tools/certify/check_final.lean
    printed for the closure and writes OUT, one line per module: "replay NAME SHA256 PATH" for
    each module leanchecker must replay in this run, "trusted NAME SHA256 PATH WHY" for each it
    trusts by directory (WHY: toolchain, or package:DIR). It decides trust again itself, from
    DIR (the toolchain's lib/lean) and CERTIFY_TRUSTED_ROOTS: a module the checker marks
    trusted but that is under none of those directories with one of their roots is refused,
    never trusted. Fails on no closure, a malformed or duplicated line, a REQUIRED module
    missing or trusted, and no module to replay. With --include-trusted every module is
    replayed.
replay.py account PLAN CHECKER_OUT STATUS RECEIPT [--checker PATH --toolchain DIR]
    STATUS is leanchecker --verbose's exit status and CHECKER_OUT its output, which prints
    "replaying NAME" for each module it replays and stops with a non-zero status at the first
    failure. Fails unless STATUS is 0, every module of PLAN to replay has its "replaying" line,
    and every olean of PLAN still has the digest it had when planned; then writes RECEIPT (JSON:
    the closure's size, each module replayed with its olean's digest, the trusted ones by
    directory, those leanchecker replayed beyond the plan, and none unaccounted for) and prints
    the count and the list. With --checker, the leanchecker that ran must be DIR's bin/leanchecker
    (by real path), and the receipt records it with its digest.
"""

import hashlib
import json
import os
import re
import sys


def fail(msg):
    print(f"certify: error: {msg}", file=sys.stderr)
    sys.exit(1)


def sha(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def under(d, f):
    return f.startswith(d.rstrip("/") + "/")


def plan(argv):
    if len(argv) < 2:
        fail(
            "usage: replay.py plan CHECK_LOG OUT --syslib DIR [--require MODULE]... [--include-trusted]"
        )
    log, out, rest = argv[0], argv[1], argv[2:]
    syslib, required, include = None, [], False
    i = 0
    while i < len(rest):
        if rest[i] == "--syslib" and i + 1 < len(rest):
            syslib = os.path.realpath(rest[i + 1])
            i += 2
        elif rest[i] == "--require" and i + 1 < len(rest):
            required.append(rest[i + 1])
            i += 2
        elif rest[i] == "--include-trusted":
            include = True
            i += 1
        else:
            fail(f"replay.py plan: unknown argument {rest[i]}")
    if not syslib:
        fail("replay.py plan needs --syslib")
    pkgs = []
    for e in os.environ.get("CERTIFY_TRUSTED_ROOTS", "").split(";"):
        if not e:
            continue
        d, sep, roots = e.partition("=")
        if not sep or not d:
            fail(f"CERTIFY_TRUSTED_ROOTS has a malformed entry {e!r}")
        pkgs.append((os.path.realpath(d), roots.split(",")))
    try:
        lines = [l for l in open(log).read().splitlines() if l.startswith("MODULE ")]  # noqa: SIM115
    except OSError as e:
        fail(f"no closure to account for: {e}")
    if not lines:
        fail(f"{log} lists no module of the closure")
    seen, rows, bad = set(), [], []
    for n, line in enumerate(lines, 1):
        parts = line.split()
        if (
            len(parts) < 4
            or parts[3] not in ("replay", "trusted")
            or not re.fullmatch(r"[^\s/]+", parts[1])
        ):
            fail(f"{log}: malformed closure line {n}: {line[:160]!r}")
        _, mod, path, kind = parts[:4]
        if mod in seen:
            fail(f"{log}: module {mod} is listed twice")
        seen.add(mod)
        real = os.path.realpath(path)
        if not os.path.isfile(real):
            fail(f"{log}: the olean of {mod} at {path} does not exist")
        root = mod.split(".", 1)[0]
        why = (
            "toolchain"
            if under(syslib, real)
            else next(
                (
                    f"package:{d}"
                    for d, roots in pkgs
                    if under(d, real) and root in roots
                ),
                None,
            )
        )
        if kind == "trusted" and why is None:
            bad.append(
                f"{mod} ({path}) is marked trusted but is under no trusted directory"
            )
        if kind == "replay" or include:
            rows.append(("replay", mod, sha(real), real, ""))
        else:
            rows.append(("trusted", mod, sha(real), real, why))
    if bad:
        fail("modules of the closure are unaccounted for: " + "; ".join(bad[:20]))
    kinds = {mod: k for k, mod, *_ in rows}
    for m in required:
        if m not in kinds:
            fail(f"the closure has no module {m}")
        if kinds[m] != "replay":
            fail(f"{m} is in the closure but would not be replayed")
    if not any(k == "replay" for k, *_ in rows):
        fail("the closure has no module to replay")
    with open(out, "w") as f:
        for k, mod, h, p, why in rows:
            f.write(" ".join(x for x in (k, mod, h, p, why) if x) + "\n")
    n_replay = sum(1 for r in rows if r[0] == "replay")
    print(
        f"closure: {len(rows)} modules, {n_replay} to replay, {len(rows) - n_replay} trusted by directory"
    )


def account(argv):
    if len(argv) not in (4, 8):
        fail(
            "usage: replay.py account PLAN CHECKER_OUT STATUS RECEIPT [--checker PATH --toolchain DIR]"
        )
    plan_file, checker_out, status, receipt = argv[:4]
    opts = dict(zip(argv[4::2], argv[5::2]))
    if len(argv) == 8 and set(opts) != {"--checker", "--toolchain"}:
        fail("replay.py account: --checker and --toolchain go together")
    checker = None
    if opts:
        want = os.path.realpath(os.path.join(opts["--toolchain"], "bin", "leanchecker"))
        got = os.path.realpath(opts["--checker"])
        if got != want or not os.path.isfile(got):
            fail(
                f"the replay ran {opts['--checker']} ({got}), not the toolchain's leanchecker {want}"
            )
        checker = {"path": got, "sha256": sha(got)}
    if not re.fullmatch(r"[0-9]+", status):
        fail(f"leanchecker's exit status {status!r} is not a number")
    try:
        rows = [l.split() for l in open(plan_file).read().splitlines() if l]  # noqa: SIM115
    except OSError as e:
        fail(f"no replay plan: {e}")
    if not rows:
        fail(f"the replay plan {plan_file} is empty")
    try:
        out = open(checker_out, errors="replace").read()  # noqa: SIM115
    except OSError as e:
        fail(f"no leanchecker output: {e}")
    if status != "0":
        tail = "\n".join(out.splitlines()[-20:])
        fail(
            f"leanchecker exits {status}: the kernel replay of the closure fails\n{tail}"
        )
    replayed = set(re.findall(r"^replaying (\S+)$", out, re.MULTILINE))
    planned = [r for r in rows if r[0] == "replay"]
    trusted = [r for r in rows if r[0] == "trusted"]
    if not planned:
        fail(f"the replay plan {plan_file} names no module to replay")
    missing = [r[1] for r in planned if r[1] not in replayed]
    if missing:
        fail(
            f"{len(missing)} of {len(planned)} modules of the closure were not replayed by leanchecker in this run: "
            + ", ".join(missing[:20])
            + (" ..." if len(missing) > 20 else "")
        )
    changed = [r[1] for r in rows if not os.path.isfile(r[3]) or sha(r[3]) != r[2]]
    if changed:
        fail(
            "oleans of the closure changed since they were planned: "
            + ", ".join(changed[:20])
        )
    by_dir = {}
    for r in trusted:
        by_dir.setdefault(r[4], []).append(r[1])
    planned_names = {r[1] for r in planned}
    rec = {
        "closure": len(rows),
        "replayed_count": len(planned),
        "replayed": [{"module": r[1], "olean_sha256": r[2]} for r in planned],
        "trusted_count": len(trusted),
        "trusted": {k: sorted(v) for k, v in sorted(by_dir.items())},
        "replayed_beyond_plan": sorted(
            replayed - planned_names - {r[1] for r in trusted}
        ),
        "unaccounted": [],
        "checker": dict(checker or {}, run="leanchecker --verbose", exit=0),
    }
    with open(receipt, "w") as f:
        json.dump(rec, f, indent=2)
        f.write("\n")
    print(
        f"kernel replay: {len(planned)} of {len(planned)} modules of the closure outside the trusted directories "
        f"replayed by leanchecker in this run; {len(trusted)} trusted by directory "
        f"({', '.join(f'{k}: {len(v)}' for k, v in sorted(by_dir.items())) or 'none'}); 0 unaccounted; "
        f"closure {len(rows)} = {len(planned)} + {len(trusted)}"
    )
    for r in planned:
        print(f"replayed {r[1]} {r[2]}")


if __name__ == "__main__":
    if len(sys.argv) < 2 or sys.argv[1] not in ("plan", "account"):
        fail("usage: replay.py plan ... | replay.py account ...")
    (plan if sys.argv[1] == "plan" else account)(sys.argv[2:])
