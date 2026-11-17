#!/usr/bin/env python3
"""receipt.py --out RECEIPT.json -- CMD...: run CMD and keep a raw memory trace.

Every 2 s it samples machine-wide used memory (MemTotal - MemAvailable) and the summed VmRSS of
CMD's live process tree, into RECEIPT.trace.tsv next to RECEIPT.json; at the end it writes the
receipt (the same fields and definitions as the driver owner's receipts) and exits with CMD's code."""
import json, os, pathlib, resource, subprocess, sys, threading, time


def meminfo():
    m = {}
    for line in open("/proc/meminfo"):
        k, v = line.split(":", 1)
        m[k] = int(v.split()[0]) // 1024
    return m


def tree_rss(root):
    kids = {}
    for p in os.listdir("/proc"):
        if not p.isdigit():
            continue
        try:
            st = open(f"/proc/{p}/stat").read()
            ppid = int(st[st.rindex(")") + 2:].split()[1])
        except (OSError, ValueError):
            continue
        kids.setdefault(ppid, []).append(int(p))
    total, todo = 0, [root]
    while todo:
        pid = todo.pop()
        try:
            for line in open(f"/proc/{pid}/status"):
                if line.startswith("VmRSS:"):
                    total += int(line.split()[1]) // 1024
        except OSError:
            pass
        todo += kids.get(pid, [])
    return total


def main():
    a = sys.argv[1:]
    out = pathlib.Path(a[a.index("--out") + 1]).resolve()
    cmd = a[a.index("--") + 1:]
    out.parent.mkdir(parents=True, exist_ok=True)
    trace = out.with_suffix(".trace.tsv")
    started = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    t0 = time.time()
    p = subprocess.Popen(cmd)
    peak = {"sys": 0, "tree": 0, "n": 0}
    stop = threading.Event()

    def sample():
        with open(trace, "w") as f:
            f.write("unix_time\tsystem_used_mib\ttree_rss_mib\n")
            while not stop.is_set():
                m = meminfo()
                used, tr = m["MemTotal"] - m["MemAvailable"], tree_rss(p.pid)
                f.write(f"{time.time():.1f}\t{used}\t{tr}\n")
                f.flush()
                peak["sys"], peak["tree"], peak["n"] = max(peak["sys"], used), max(peak["tree"], tr), peak["n"] + 1
                stop.wait(2)

    th = threading.Thread(target=sample, daemon=True)
    th.start()
    rc = p.wait()
    stop.set()
    th.join()
    ru = resource.getrusage(resource.RUSAGE_CHILDREN)
    rec = {"command": cmd, "exit_code": rc, "wall_seconds": round(time.time() - t0, 1),
           "max_rss_one_process_mib": ru.ru_maxrss // 1024, "peak_tree_rss_mib": peak["tree"],
           "peak_system_used_mib": peak["sys"], "mem_total_mib": meminfo()["MemTotal"], "samples": peak["n"],
           "sample_interval_s": 2, "trace": trace.name,
           "memory_definitions": {
               "system_used_mib": "machine-wide MemTotal - MemAvailable (/proc/meminfo), all processes",
               "tree_rss_mib": "sum of VmRSS over the command's live process tree; shared pages count per process",
               "max_rss_one_process_mib": "getrusage(RUSAGE_CHILDREN).ru_maxrss: largest peak RSS of one reaped descendant"},
           "started_utc": started}
    out.write_text(json.dumps(rec, indent=2) + "\n")
    sys.exit(rc)


if __name__ == "__main__":
    main()
