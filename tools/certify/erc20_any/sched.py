#!/usr/bin/env python3
"""Run leanchecker batches under a whole-tree concurrency and memory cap, and check that it held.

    sched.py RUN_DIR PAR BUDGET_KB EST_FILE -- CMD...   (each line of RUN_DIR/batches is a batch)

RUN_DIR/batches has one line per batch: "INDEX MODULE...". CMD is the checker command; a batch runs
CMD + its modules under /usr/bin/time -v. A batch starts only when fewer than PAR batches run and
the larger of the machine's used memory and its used memory at the start plus the estimated
peaks of the running batches, plus the batch's own estimated peak (EST_FILE: "INDEX PEAK_KB", from an
earlier run, times 1.25; batches it lacks get its largest) stays within BUDGET_KB. Heavier batches
start first. A sampler records every 2 s the machine's used memory, the number of running checker
processes and their summed RSS (whole process tree). If used memory passes BUDGET_KB plus half the
remaining headroom, every batch is killed and the run fails. Afterwards the gate fails unless at
most PAR checkers ever ran at once and used memory never passed BUDGET_KB. Writes batch-exits
("INDEX EXIT PEAK_KB STATUS"), samples.tsv and gate.txt to RUN_DIR; exits 0 only if every batch
exited 0 and the gate held.
"""
import os, signal, subprocess, sys, threading, time

def meminfo():
    m = {}
    for line in open("/proc/meminfo"):
        k, v = line.split(":"); m[k] = int(v.split()[0])
    return m["MemTotal"], m["MemTotal"] - m["MemAvailable"]

def tree_rss(roots):
    kids, rss, comm = {}, {}, {}
    for p in os.listdir("/proc"):
        if not p.isdigit(): continue
        try:
            st = open(f"/proc/{p}/stat").read()
            r = st.rsplit(")", 1)[1].split()
            kids.setdefault(int(r[1]), []).append(int(p))
            rss[int(p)] = int(r[21]) * os.sysconf("SC_PAGE_SIZE") // 1024
            comm[int(p)] = st.split("(", 1)[1].rsplit(")", 1)[0]
        except (OSError, IndexError, ValueError):
            pass
    seen, stack = set(), list(roots)
    while stack:
        p = stack.pop()
        if p in seen: continue
        seen.add(p); stack += kids.get(p, [])
    return sum(rss.get(p, 0) for p in seen), sum(1 for p in seen if comm.get(p) == "leanchecker")

def main():
    run, par, budget, est_file = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), sys.argv[4]
    cmd = sys.argv[sys.argv.index("--") + 1:]
    total, _ = meminfo()
    hard = budget + (total - budget) // 2
    _, base = meminfo()
    batches = [l.split() for l in open(f"{run}/batches") if l.strip()]
    est = {}
    if os.path.exists(est_file):
        est = {l.split()[0]: int(int(l.split()[1]) * 1.25) for l in open(est_file) if l.strip()}
    big = max(est.values(), default=budget // par)
    queue = sorted(batches, key=lambda b: -est.get(b[0], big))
    running, results, peak = {}, [], {"used": 0, "n": 0, "sum": 0}
    stop = threading.Event(); killed = []
    samples = open(f"{run}/samples.tsv", "w")
    def sampler():
        while not stop.is_set():
            _, used = meminfo()
            s, n = tree_rss([p.pid for p, _ in running.values()])
            samples.write(f"{time.time():.0f}\t{used}\t{n}\t{s}\n"); samples.flush()
            peak["used"] = max(peak["used"], used); peak["n"] = max(peak["n"], n); peak["sum"] = max(peak["sum"], s)
            if used > hard and running:
                killed.append(f"{time.time():.0f} used {used} kB > hard limit {hard} kB")
                for p, _ in list(running.values()): p.send_signal(signal.SIGKILL)
            stop.wait(2)
    t = threading.Thread(target=sampler, daemon=True); t.start()
    while queue or running:
        for i, (p, _) in list(running.items()):
            if p.poll() is not None:
                tm = open(f"{run}/b{i}.time").read() if os.path.exists(f"{run}/b{i}.time") else ""
                pk = next((l.split()[-1] for l in tm.splitlines() if "Maximum resident" in l), "?")
                results.append(f"{i} {p.returncode} {pk} {'killed' if killed else 'ran'}")
                del running[i]
        if queue and not killed and len(running) < par:
            _, used = meminfo(); b = queue[0]
            reserved = base + sum(est.get(j, big) for j in running)
            if not running or max(used, reserved) + est.get(b[0], big) <= budget:
                queue.pop(0); i, mods = b[0], b[1:]
                out = open(f"{run}/b{i}.out", "w")
                running[i] = (subprocess.Popen([os.environ.get("SCHED_TIME", "/usr/bin/time"), "-v", "-o", f"{run}/b{i}.time", *cmd, *mods],
                                               stdout=out, stderr=subprocess.STDOUT), out)
                continue
        if killed and not running: break
        time.sleep(2)
    stop.set(); t.join()
    for b in queue: results.append(f"{b[0]} -1 0 not-started")
    open(f"{run}/batch-exits", "w").write("\n".join(sorted(results, key=lambda r: int(r.split()[0]))) + "\n")
    ok_par, ok_mem = peak["n"] <= par, peak["used"] <= budget
    gate = (f"par {par} max_concurrent_checkers {peak['n']} {'OK' if ok_par else 'FAILED'}\n"
            f"budget_kb {budget} of {total} max_used_kb {peak['used']} headroom "
            f"{100 * (total - peak['used']) // total}% {'OK' if ok_mem else 'FAILED'}\n"
            f"max_tree_rss_kb {peak['sum']}\nhard_limit_kb {hard} kills {len(killed)} {' '.join(killed)}\n")
    open(f"{run}/gate.txt", "w").write(gate); print(gate, end="")
    bad = any(r.split()[1] != "0" for r in results)
    sys.exit(0 if ok_par and ok_mem and not killed and not bad else 1)

if __name__ == "__main__":
    main()
