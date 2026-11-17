#!/usr/bin/env python3
"""Run leanchecker batches under a whole-tree concurrency and memory cap, and check that it held.

    sched.py RUN_DIR PAR BUDGET_KB EST_FILE -- CMD...

RUN_DIR/batches has one line per batch: "INDEX MODULE...". CMD is the checker command; a batch runs
CMD + its modules under /usr/bin/time -v (SCHED_TIME overrides the path). At most PAR batches run.
A batch starts only when the larger of the machine's used memory and its used memory at the start
plus the running batches' estimated peaks, plus the batch's own estimated peak, stays within
BUDGET_KB. EST_FILE ("INDEX PEAK_KB" from an earlier run) gives the estimates, times 1.25; batches
it lacks get its largest. Heavier batches start first. A sampler records every 2 s the machine's
used memory, the number of running leanchecker processes and the summed RSS of the whole process
tree; if used memory passes BUDGET_KB plus half the remaining headroom, every batch is killed and
the run fails. The gate fails unless at most PAR checkers ever ran at once and used memory never
passed BUDGET_KB. Writes batch-exits ("INDEX EXIT PEAK_KB STATUS"), samples.tsv and gate.txt.

For a run split across machines: SCHED_SKIP names an earlier batch-exits; its batches that ran and
exited 0 are not run again ("skipped-done"). SCHED_STOP_AT (Unix time) stops admitting batches
("not-started"); SCHED_KILL_AT kills the batches still running ("killed-deadline", their modules
unverified). SCHED_WALLS ("INDEX WALL_S", a projected wall per batch, used as given) admits a batch
only if now plus its projected wall is before SCHED_KILL_AT; a batch that cannot fit is
"not-started-projected" (its modules unverified), and since time only advances it is dropped at once.
Batches SCHED_WALLS lacks get its largest. Exit 0: every batch ran here and exited 0 and the gate held. Exit 2: the same for every
batch that ran, but some were skipped, not started or killed at the deadline. Exit 1 otherwise.
"""
import os
import signal
import subprocess
import sys
import threading
import time


def meminfo():
    m = {}
    for line in open("/proc/meminfo"):
        k, v = line.split(":")
        m[k] = int(v.split()[0])
    return m["MemTotal"], m["MemTotal"] - m["MemAvailable"]


def tree_rss(roots):
    kids, rss, comm = {}, {}, {}
    page = os.sysconf("SC_PAGE_SIZE") // 1024
    for p in os.listdir("/proc"):
        if not p.isdigit():
            continue
        try:
            st = open(f"/proc/{p}/stat").read()
            r = st.rsplit(")", 1)[1].split()
            kids.setdefault(int(r[1]), []).append(int(p))
            rss[int(p)] = int(r[21]) * page
            comm[int(p)] = st.split("(", 1)[1].rsplit(")", 1)[0]
        except (OSError, IndexError, ValueError):
            pass
    seen, stack = set(), list(roots)
    while stack:
        p = stack.pop()
        if p in seen:
            continue
        seen.add(p)
        stack += kids.get(p, [])
    return sum(rss.get(p, 0) for p in seen), sum(1 for p in seen if comm.get(p) == "leanchecker")


def main():
    run, par, budget, est_file = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), sys.argv[4]
    cmd = sys.argv[sys.argv.index("--") + 1:]
    total, base = meminfo()
    hard = budget + (total - budget) // 2
    batches = [l.split() for l in open(f"{run}/batches") if l.strip()]
    est = {}
    if os.path.exists(est_file):
        est = {l.split()[0]: int(int(l.split()[1]) * 1.25) for l in open(est_file) if l.strip()}
    big = max(est.values(), default=budget // par)
    done = set()
    if os.environ.get("SCHED_SKIP"):
        done = {f[0] for f in (l.split() for l in open(os.environ["SCHED_SKIP"]))
                if len(f) == 4 and f[1] == "0" and f[3] == "ran"}
    stop_at = float(os.environ.get("SCHED_STOP_AT", "inf"))
    kill_at = float(os.environ.get("SCHED_KILL_AT", "inf"))
    stop_at = min(stop_at, kill_at)
    walls = {}
    if os.environ.get("SCHED_WALLS"):
        walls = {l.split()[0]: int(l.split()[1]) for l in open(os.environ["SCHED_WALLS"]) if l.strip()}
    big_wall = max(walls.values(), default=0)
    queue = sorted((b for b in batches if b[0] not in done), key=lambda b: -est.get(b[0], big))
    running, results = {}, []
    peak = {"used": 0, "n": 0, "sum": 0}
    stop, killed, deadline_killed = threading.Event(), [], set()
    samples = open(f"{run}/samples.tsv", "w")

    def sampler():
        while not stop.is_set():
            _, used = meminfo()
            s, n = tree_rss([p.pid for p, _ in list(running.values())])
            samples.write(f"{time.time():.0f}\t{used}\t{n}\t{s}\n")
            samples.flush()
            peak["used"] = max(peak["used"], used)
            peak["n"] = max(peak["n"], n)
            peak["sum"] = max(peak["sum"], s)
            if used > hard and running:
                killed.append(f"{time.time():.0f} used {used} kB > hard limit {hard} kB")
                for p, _ in list(running.values()):
                    p.send_signal(signal.SIGKILL)
            stop.wait(2)

    t = threading.Thread(target=sampler, daemon=True)
    t.start()
    timer = os.environ.get("SCHED_TIME", "/usr/bin/time")
    while queue or running:
        for i, (p, _) in list(running.items()):
            if p.poll() is not None:
                tm = open(f"{run}/b{i}.time").read() if os.path.exists(f"{run}/b{i}.time") else ""
                pk = next((l.split()[-1] for l in tm.splitlines() if "Maximum resident" in l), "?")
                how = "killed-deadline" if i in deadline_killed else ("killed" if killed else "ran")
                results.append(f"{i} {p.returncode} {pk} {how}")
                del running[i]
        if time.time() >= kill_at:
            for i, (p, _) in running.items():
                if p.poll() is None and i not in deadline_killed:
                    deadline_killed.add(i)
                    p.send_signal(signal.SIGKILL)
        if queue and time.time() >= stop_at:
            results.extend(f"{b[0]} -1 0 not-started" for b in queue)
            queue = []
        if queue and walls and kill_at != float("inf"):
            late = [b for b in queue if time.time() + walls.get(b[0], big_wall) > kill_at]
            results.extend(f"{b[0]} -1 0 not-started-projected" for b in late)
            queue = [b for b in queue if b not in late]
        if queue and not killed and len(running) < par:
            _, used = meminfo()
            b = queue[0]
            reserved = base + sum(est.get(j, big) for j in running)
            if not running or max(used, reserved) + est.get(b[0], big) <= budget:
                queue.pop(0)
                i, mods = b[0], b[1:]
                out = open(f"{run}/b{i}.out", "w")
                if os.path.exists(f"{run}/b{i}.time"):
                    os.remove(f"{run}/b{i}.time")
                running[i] = (subprocess.Popen([timer, "-v", "-o", f"{run}/b{i}.time", *cmd, *mods],
                                               stdout=out, stderr=subprocess.STDOUT), out)
                continue
        if killed and not running:
            break
        time.sleep(2)
    stop.set()
    t.join()
    results.extend(f"{b[0]} -1 0 not-started" for b in queue)
    results.extend(f"{i} 0 0 skipped-done" for i in done)
    results.sort(key=lambda r: int(r.split()[0]))
    open(f"{run}/batch-exits", "w").write("\n".join(results) + "\n")
    ok_par, ok_mem = peak["n"] <= par, peak["used"] <= budget
    gate = (f"par {par} max_concurrent_checkers {peak['n']} {'OK' if ok_par else 'FAILED'}\n"
            f"budget_kb {budget} of {total} max_used_kb {peak['used']} headroom "
            f"{100 * (total - peak['used']) // total}% {'OK' if ok_mem else 'FAILED'}\n"
            f"max_tree_rss_kb {peak['sum']}\nhard_limit_kb {hard} kills {len(killed)} {' '.join(killed)}\n")
    open(f"{run}/gate.txt", "w").write(gate)
    print(gate, end="")
    ran = [r.split() for r in results if r.split()[3] == "ran"]
    partial = len(ran) < len(results)
    if not (ok_par and ok_mem) or killed or any(r[1] != "0" for r in ran):
        sys.exit(1)
    sys.exit(2 if partial else 0)


if __name__ == "__main__":
    main()
