#!/usr/bin/env python3
"""Merge a kernel replay split across sequential slots into one receipt, or refuse.

    merge.py RECEIPT SLOT_DIR...

Each SLOT_DIR is a replay.sh OUT/replay directory: plan, batches, batch-exits, gate.txt and the
b<INDEX>.out of each batch. The merge refuses unless every slot's plan is byte-identical (the same
modules with the same olean sha256s, so the same compiled inputs), each slot's batches name only
planned modules and none twice (slots may batch differently: parallel shards use smaller batches of
the modules an earlier slot left unverified), no module is trusted by directory, every slot's gate.txt passed, and every planned module was replayed
exactly once, by a batch that ran and exited 0 in exactly one slot, whose leanchecker output names
it. The receipt lists every module with its slot and batch.
"""
import hashlib
import json
import re
import sys


def fail(msg):
    sys.exit(f"merge refused: {msg}")


def read(p):
    return open(p).read()


receipt, slots = sys.argv[1], sys.argv[2:]
if not slots:
    fail("no slots")
plans = [read(f"{s}/plan") for s in slots]
if len(set(plans)) != 1:
    fail("the slots' plans differ: their builds' oleans are not identical, so no slot's replay counts")
rows = [l.split() for l in plans[0].splitlines() if l]
planned = [r[1] for r in rows if r[0] == "replay"]
trusted = [r[1] for r in rows if r[0] == "trusted"]
if trusted:
    fail(f"{len(trusted)} modules are trusted by directory")
for s in slots:
    g = read(f"{s}/gate.txt")
    if "FAILED" in g or not re.search(r"kills 0", g):
        fail(f"{s}/gate.txt did not pass:\n{g}")
credit = {}
for k, s in enumerate(slots, 1):
    batch_mods = {}
    for l in read(f"{s}/batches").splitlines():
        if l:
            i, *mods = l.split()
            if i in batch_mods:
                fail(f"slot {k} has batch {i} twice")
            batch_mods[i] = mods
    named = [m for mods in batch_mods.values() for m in mods]
    if len(set(named)) != len(named) or not set(named) <= set(planned):
        fail(f"slot {k}'s batches name an unplanned module or one module twice")
    for f in (l.split() for l in read(f"{s}/batch-exits").splitlines() if l):
        if f[3] != "ran":
            continue
        if f[1] != "0":
            fail(f"slot {k} batch {f[0]} exited {f[1]}")
        out = set(re.findall(r"^replaying (\S+)$", read(f"{s}/b{f[0]}.out"), re.M))
        for m in batch_mods[f[0]]:
            if m not in out:
                fail(f"slot {k} batch {f[0]} exited 0 but its output does not name {m}")
            if m in credit:
                fail(f"{m} replayed in slot {credit[m][0]} and slot {k}")
            credit[m] = (k, f[0])
missing = [m for m in planned if m not in credit]
if missing:
    fail(f"{len(missing)} of {len(planned)} modules not replayed: {', '.join(missing[:20])}")
olean = {r[1]: r[2] for r in rows if r[0] == "replay"}
inv = "".join(f"{m} {olean[m]}\n" for m in sorted(olean))
rec = {"closure": len(rows), "replayed_count": len(planned), "trusted_count": 0,
       "inventory_sha256": hashlib.sha256(inv.encode()).hexdigest(),
       "plan_sha256": hashlib.sha256(plans[0].encode()).hexdigest(), "slots": slots,
       "replayed": [{"module": m, "olean_sha256": olean[m], "slot": credit[m][0], "batch": credit[m][1]}
                    for m in planned]}
json.dump(rec, open(receipt, "w"), indent=2)
per = {k: sum(1 for m in planned if credit[m][0] == k) for k in range(1, len(slots) + 1)}
print(f"merged: {len(planned)} of {len(planned)} modules replayed exactly once, 0 trusted; per slot {per}; "
      f"inventory {rec['inventory_sha256']}")
