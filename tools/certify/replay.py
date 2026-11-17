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
replay.py closure PREFIX N RECEIPT --theorem T --statement S --toolchain DIR --checker PATH --tool PATH
    accounts for tools/certify/replay_closure.lean's replay of T's constant closure in N shards,
    each of which left PREFIX.K-of-N.{status,out,plan,proofs}. Fails unless every shard exits 0
    and reports replaying the closure of T as shard K of N without a test-only defect, all shards
    planned the same closure (sorted, each constant once, T a theorem of it), it holds no axiom
    beyond propext, Classical.choice and Quot.sound, and the shards' checked proofs are an exact
    disjoint union of its theorems; PATH must be DIR's bin/lean. Then writes RECEIPT (JSON: the
    closure's count, digest, kinds and constants, its axioms, each shard, the checker and the
    tool with their digests) and prints the count.
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


KINDS = {"axiom", "def", "theorem", "opaque", "quot", "inductive", "ctor", "rec"}
ALLOWED_AXIOMS = ["Classical.choice", "Quot.sound", "propext"]


def closure(argv):
    usage = ("usage: replay.py closure PREFIX N RECEIPT --theorem T --statement S "
             "--toolchain DIR --checker PATH --tool PATH")
    if len(argv) != 13:
        fail(usage)
    prefix, n, receipt = argv[:3]
    opts = dict(zip(argv[3::2], argv[4::2]))
    if set(opts) != {"--theorem", "--statement", "--toolchain", "--checker", "--tool"}:
        fail(usage)
    if not re.fullmatch(r"[1-9][0-9]*", n):
        fail(f"the shard count {n!r} is not a positive number")
    n = int(n)
    want = os.path.realpath(os.path.join(opts["--toolchain"], "bin", "lean"))
    got = os.path.realpath(opts["--checker"])
    if got != want or not os.path.isfile(got):
        fail(f"the replay ran {opts['--checker']} ({got}), not the toolchain's lean {want}")
    thm = opts["--theorem"]
    shards, plan_text = [], None
    for k in range(n):
        base = f"{prefix}.{k}-of-{n}"
        try:
            status = open(base + ".status").read().strip()  # noqa: SIM115
            out = open(base + ".out", errors="replace").read()  # noqa: SIM115
            text = open(base + ".plan").read()  # noqa: SIM115
            proofs = open(base + ".proofs").read().splitlines()  # noqa: SIM115
        except OSError as e:
            fail(f"shard {k} of {n} of the closure replay left no record: {e}")
        if status != "0":
            tail = "\n".join(out.splitlines()[-20:])
            fail(f"shard {k} of {n} of the closure replay exits {status}: the kernel rejects the closure\n{tail}")
        if re.search(r"^TEST ", out, re.M):
            fail(f"shard {k} of {n} ran with a test-only defect (--tamper or --omit)")
        done = re.findall(r"^REPLAYED (\d+) constants, (\d+) theorem proofs, of the closure of (\S+) "
                          r"\(shard (\d+) of (\d+)\)$", out, re.M)
        if len(done) != 1 or done[0][2] != thm or (int(done[0][3]), int(done[0][4])) != (k, n):
            fail(f"shard {k} of {n} does not report replaying the closure of {thm}")
        if plan_text is None:
            plan_text = text
        elif text != plan_text:
            fail(f"shard {k} of {n} replayed a different closure than shard 0")
        if int(done[0][1]) != len(proofs) or proofs != sorted(set(proofs)):
            fail(f"shard {k} of {n}'s proof list is malformed")
        shards.append({"shard": k, "proofs": proofs, "replayed": int(done[0][0]),
                       "out_sha256": hashlib.sha256(out.encode()).hexdigest()})
    rows = [l.split() for l in plan_text.splitlines()]
    if not rows or any(len(r) != 3 or r[2] not in KINDS for r in rows):
        fail("the closure plan is empty or malformed")
    names = [r[0] for r in rows]
    if names != sorted(set(names)):
        fail("the closure plan is not sorted or names a constant twice")
    kind = {r[0]: r[2] for r in rows}
    if kind.get(thm) != "theorem":
        fail(f"the closure plan does not hold the theorem {thm}")
    if any(s["replayed"] != len(rows) for s in shards):
        fail("a shard replayed a different number of constants than the plan holds")
    axioms = sorted(c for c in names if kind[c] == "axiom")
    if [a for a in axioms if a not in ALLOWED_AXIOMS]:
        fail(f"the closure holds axioms beyond {', '.join(ALLOWED_AXIOMS)}: {axioms}")
    theorems = {c for c in names if kind[c] == "theorem"}
    seen = {}
    for s in shards:
        for p in s["proofs"]:
            if p in seen:
                fail(f"the proof of {p} was checked by shards {seen[p]} and {s['shard']}: the shards overlap")
            seen[p] = s["shard"]
    if set(seen) != theorems:
        missing = sorted(theorems - set(seen))
        extra = sorted(set(seen) - theorems)
        fail(f"the shards' proofs are not the closure's theorems: {len(missing)} unchecked "
             f"({', '.join(missing[:10])}), {len(extra)} outside the closure ({', '.join(extra[:10])})")
    counts = {}
    for c in names:
        counts[kind[c]] = counts.get(kind[c], 0) + 1
    rec = {
        "closure": len(rows),
        "replayed_count": len(rows),
        "trusted_count": 0,
        "trusted": {},
        "unaccounted": [],
        "unit": "constants",
        "theorem": thm,
        "statement": opts["--statement"],
        "closure_sha256": hashlib.sha256(plan_text.encode()).hexdigest(),
        "kinds": dict(sorted(counts.items())),
        "theorem_proofs": len(theorems),
        "axioms": axioms,
        "axioms_allowed": ALLOWED_AXIOMS,
        "sorry_present": "sorryAx" in kind,
        "constants": [[r[0], r[2]] for r in rows],
        "shards": [{"shard": s["shard"], "of": n, "proofs_checked": len(s["proofs"]),
                    "proofs_sha256": hashlib.sha256("".join(p + "\n" for p in s["proofs"]).encode()).hexdigest(),
                    "out_sha256": s["out_sha256"], "exit": 0} for s in shards],
        "checker": {"path": got, "sha256": sha(got), "tool": os.path.realpath(opts["--tool"]),
                    "tool_sha256": sha(opts["--tool"]), "run": "lean --run replay_closure.lean", "exit": 0},
        "toolchain": os.path.realpath(opts["--toolchain"]),
    }
    with open(receipt, "w") as f:
        json.dump(rec, f, indent=2)
        f.write("\n")
    print(f"kernel replay: all {len(rows)} constants of the closure of {thm} replayed in this run "
          f"({len(theorems)} theorem proofs over {n} shard(s), disjoint and complete); closure sha256 "
          f"{rec['closure_sha256']}; axioms {', '.join(axioms) or 'none'}; 0 unaccounted")


if __name__ == "__main__":
    cmds = {"plan": plan, "account": account, "closure": closure}
    if len(sys.argv) < 2 or sys.argv[1] not in cmds:
        fail("usage: replay.py plan ... | replay.py account ... | replay.py closure ...")
    cmds[sys.argv[1]](sys.argv[2:])
