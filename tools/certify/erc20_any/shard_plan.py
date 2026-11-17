#!/usr/bin/env python3
"""Split the modules an earlier slot left unverified into shards of small batches for parallel boxes.

    shard_plan.py PLAN PRIOR_REPLAY NSHARDS CHUNK OUT [PEAKS_FILE WALLS_FILE]

PLAN is the replay plan every box must reproduce byte for byte. PRIOR_REPLAY is the earlier slot's
OUT/replay (its batches, verdicts and batch-exits). Every module whose verdict there is not
"verified" is put in a batch of at most CHUNK modules; a batch never crosses one of the earlier
slot's batches, so each batch inherits that batch's measured cost: its peak RSS (the larger of
PEAKS_FILE, default replay-peaks.59d0fa44, and the earlier slot's batch-exits) and its wall time
(the larger of WALLS_FILE, default replay-walls.59d0fa44, and the earlier slot's b<INDEX>.time),
scaled by the batch's share of the modules and by 1.5. A batch's index is the earlier batch's index
plus its first module's offset in it, so indices are unique across shards. Batches go to NSHARDS
shards by longest processing time first on estimated peak times wall, so the boxes carry about the
same memory-time.

OUT/shard<K>/ gets batches ("INDEX MODULE..."), peaks ("INDEX PEAK_KB", read by sched.py, which
adds its own 1.25) and walls ("INDEX WALL_S", the projected wall that sched.py admits a batch
against the deadline with). OUT/shards.txt lists each shard's batches, modules, estimates and the
sha256 of its three files; OUT/modules lists every module assigned, which must equal the earlier
slot's unverified ones.
"""
import hashlib
import math
import os
import re
import sys

plan, prior, nshards, chunk, out = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4]), sys.argv[5]
here = os.path.dirname(os.path.abspath(__file__))
peaks_file = sys.argv[6] if len(sys.argv) > 6 else f"{here}/replay-peaks.59d0fa44"
walls_file = sys.argv[7] if len(sys.argv) > 7 else f"{here}/replay-walls.59d0fa44"
if nshards < 1 or not 1 <= chunk <= 400:
    sys.exit("shard_plan: NSHARDS >= 1 and 1 <= CHUNK <= 400")
planned = [l.split()[1] for l in open(plan) if l.startswith("replay ")]
if len(set(planned)) != len(planned):
    sys.exit("shard_plan: duplicate module in plan")
batches = [l.split() for l in open(f"{prior}/batches") if l.strip()]
if sorted(m for b in batches for m in b[1:]) != sorted(planned):
    sys.exit("shard_plan: the earlier slot's batches do not cover the plan exactly")
verdict = {}
for l in open(f"{prior}/verdicts"):
    f = l.split()
    if f[0] in verdict:
        sys.exit(f"shard_plan: {f[0]} has two verdicts")
    verdict[f[0]] = f[2]
if set(verdict) != set(planned):
    sys.exit("shard_plan: the earlier slot's verdicts do not cover the plan exactly")


def table(p):
    return {l.split()[0]: int(l.split()[1]) for l in open(p) if l.strip()}


peak, wall = table(peaks_file), table(walls_file)
for f in (l.split() for l in open(f"{prior}/batch-exits") if l.strip()):
    if f[3] == "ran" and f[2].isdigit():
        peak[f[0]] = max(peak.get(f[0], 0), int(f[2]))
for b in batches:
    t = f"{prior}/b{b[0]}.time"
    if os.path.exists(t):
        m = re.search(r"Elapsed \(wall clock\) time \(h:mm:ss or m:ss\): (\S+)", open(t).read())
        if m:
            parts = [float(x) for x in m.group(1).split(":")]
            s = parts[-1] + 60 * parts[-2] + (3600 * parts[-3] if len(parts) == 3 else 0)
            wall[b[0]] = max(wall.get(b[0], 0), math.ceil(s))
jobs = []
for b in batches:
    i, mods = int(b[0]), b[1:]
    if b[0] not in peak or b[0] not in wall:
        sys.exit(f"shard_plan: no measured peak or wall for batch {b[0]}")
    todo = [(k, m) for k, m in enumerate(mods) if verdict[m] != "verified"]
    for s in range(0, len(todo), chunk):
        part = todo[s:s + chunk]
        est_wall = math.ceil(wall[b[0]] * len(part) / len(mods) * 1.5)
        jobs.append((i + part[0][0], [m for _, m in part], peak[b[0]], est_wall))
if len({j[0] for j in jobs}) != len(jobs):
    sys.exit("shard_plan: batch indices collide")
load = [0] * nshards
shards = [[] for _ in range(nshards)]
for j in sorted(jobs, key=lambda j: (-j[2] * j[3], j[0])):
    k = min(range(nshards), key=lambda k: (load[k], k))
    shards[k].append(j)
    load[k] += j[2] * j[3]
os.makedirs(out, exist_ok=True)
summary, assigned = [], []
for k, sh in enumerate(shards, 1):
    d = f"{out}/shard{k}"
    os.makedirs(d, exist_ok=True)
    sh.sort(key=lambda j: j[0])
    files = {"batches": "".join(f"{j[0]} {' '.join(j[1])}\n" for j in sh),
             "peaks": "".join(f"{j[0]} {j[2]}\n" for j in sh),
             "walls": "".join(f"{j[0]} {j[3]}\n" for j in sh)}
    for n, body in files.items():
        open(f"{d}/{n}", "w").write(body)
    assigned += [m for j in sh for m in j[1]]
    summary.append(f"shard{k} batches {len(sh)} modules {sum(len(j[1]) for j in sh)} indices "
                   f"{','.join(str(j[0]) for j in sh)} est_wall_s {sum(j[3] for j in sh)} "
                   f"max_peak_kb {max((j[2] for j in sh), default=0)} "
                   + " ".join(f"{n} {hashlib.sha256(b.encode()).hexdigest()}" for n, b in files.items()))
want = sorted(m for m in planned if verdict[m] != "verified")
if sorted(assigned) != want:
    sys.exit("shard_plan: the shards do not cover the unverified modules exactly once")
open(f"{out}/modules", "w").write("".join(f"{m}\n" for m in sorted(assigned)))
summary.insert(0, f"plan {hashlib.sha256(open(plan, 'rb').read()).hexdigest()} unverified {len(want)} "
                  f"chunk {chunk} shards {nshards} batches {len(jobs)}")
open(f"{out}/shards.txt", "w").write("\n".join(summary) + "\n")
print("\n".join(summary))
