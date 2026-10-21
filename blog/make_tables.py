"""Render full Markdown tables for codegen benchmark results.

Usage: uv run blog/make_tables.py LABEL=results.json [LABEL=results.json ...]

For each run, prints a summary of Solar against every other compiler, then one table per
metric (runtime gas, deployment gas, runtime size, creation size, compile time, peak RSS)
with every case and compiler, and the failures.
"""

import importlib.util
import json
import math
import statistics
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "benches/runtime"))
spec = importlib.util.spec_from_file_location(
    "bc", ROOT / "benches/runtime/benchmark-compare.py"
)
bc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bc)

ORDER = ["solc", "solx", "oksolc", "solar"]
CORPORA = (
    ("project-derived", {"repository", "large"}),
    ("micro", {"micro"}),
    ("all runtime", {"repository", "large", "micro"}),
    ("whole project", {"heavy"}),
)


def ok(e):
    return e is not None and e.get("status") == "ok"


def ctime(e):
    s = [x for x in e.get("compile_time_samples") or [] if isinstance(x, (int, float))]
    return statistics.median(s) if s else e.get("compile_time_seconds")


def gm(xs):
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else None


def pct(ratio):
    return "n/a" if ratio is None else f"{100 * (ratio - 1):+.2f}%"


def gas(e, other):
    if not ok(e) or e.get("runtime_status", "ok") not in ("ok", "skipped"):
        return None
    return bc.comparable_gas(e, other)


METRICS = (
    ("Runtime gas (comparable transaction gas, summed per workload)", "gas", False),
    ("Deployment gas", "deploy_gas", False),
    ("Runtime bytecode size (bytes)", "runtime_size", False),
    ("Creation bytecode size (bytes)", "bytecode_size", False),
    ("Compile time (median seconds)", "time", True),
    ("Peak RSS (MiB)", "rss", True),
)


def value(metric, e, solar):
    if not ok(e):
        return None
    if metric == "gas":
        return gas(e, solar)
    if metric == "time":
        return ctime(e)
    if metric == "rss":
        return e["peak_rss_bytes"] / 2**20 if e.get("peak_rss_bytes") else None
    return e.get(metric)


def fmt(metric, v):
    if v is None:
        return "—"
    if metric == "time":
        return f"{v:.3f}"
    if metric == "rss":
        return f"{v:.1f}"
    return f"{v:,}"


def render(label, path):
    data = json.loads(Path(path).read_text())
    rows = data["results"]
    present = [c for c in ORDER if any(c in r["compilers"] for r in rows)]
    refs = [c for c in present if c != "solar"]
    labels = {}
    for r in rows:
        for c, e in r["compilers"].items():
            labels.setdefault(c, e.get("label", c))
    out = [f"## {label}", "", f"Source: `{Path(path).name}`.", ""]
    out.append("Compilers: " + ", ".join(f"`{labels[c]}`" for c in present) + ".")
    if data.get("evm_version_override"):
        out.append(f" EVM version override: `{data['evm_version_override']}`.")
    out += ["", "### Summary: Solar vs each compiler", ""]
    out.append(
        "Geometric means of per-case Solar/reference ratios over cases both compilers built; "
        "negative is better for Solar. Speedup is reference time / Solar time."
    )
    out += [
        "",
        "| Reference | Corpus | Cases | Compile speedup | Runtime gas | Deploy gas | Runtime size | Creation size | Peak RSS | Gas wins |",
        "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for ref in refs:
        for name, suites in CORPORA:
            acc = {k: [] for k in ("t", "g", "d", "s", "c", "m")}
            n = wins = 0
            for r in rows:
                if r["suite"] not in suites:
                    continue
                s, o = r["compilers"].get("solar"), r["compilers"].get(ref)
                if not ok(s) or not ok(o):
                    continue
                n += 1
                for key, a, b in (
                    ("t", ctime(o), ctime(s)),
                    ("m", o.get("peak_rss_bytes"), s.get("peak_rss_bytes")),
                ):
                    if a and b:
                        acc[key].append(a / b if key == "t" else b / a)
                gs, go = gas(s, o), gas(o, s)
                if gs and go:
                    acc["g"].append(gs / go)
                    wins += gs < go
                for key, f in (("d", "deploy_gas"), ("s", "runtime_size"), ("c", "bytecode_size")):
                    if s.get(f) and o.get(f):
                        acc[key].append(s[f] / o[f])
            if not n:
                continue
            t = gm(acc["t"])
            out.append(
                f"| {ref} | {name} | {n} | {t:.2f}× | {pct(gm(acc['g']))} | {pct(gm(acc['d']))} | "
                f"{pct(gm(acc['s']))} | {pct(gm(acc['c']))} | {pct(gm(acc['m']))} | "
                + (f"{wins}/{len(acc['g'])}" if acc["g"] else "n/a")
                + " |"
            )
    for title, metric, all_suites in METRICS:
        cases = [r for r in rows if all_suites or r["suite"] != "heavy"]
        out += ["", f"### {title}", ""]
        head = "| Case | Suite | " + " | ".join(present)
        head += " | " + " | ".join(f"Solar vs {c}" for c in refs) + " |"
        out += [head, "|---|---|" + "---:|" * (len(present) + len(refs))]
        for r in cases:
            s = r["compilers"].get("solar")
            vals = {c: value(metric, r["compilers"].get(c), s) for c in present}
            deltas = []
            for c in refs:
                a, b = vals["solar"], vals[c]
                if metric == "time":
                    deltas.append(f"{b / a:.2f}×" if a and b else "—")
                else:
                    deltas.append(pct(a / b) if a and b else "—")
            out.append(
                f"| {r['test_id']} | {r['suite']} | "
                + " | ".join(fmt(metric, vals[c]) for c in present)
                + " | "
                + " | ".join(deltas)
                + " |"
            )
    out += ["", "### Compile time samples (seconds)", ""]
    out += ["| Case | Compiler | n | Median | Min | Max |", "|---|---|---:|---:|---:|---:|"]
    for r in rows:
        for c in present:
            e = r["compilers"].get(c)
            if ok(e) and e.get("compile_time_samples"):
                s = e["compile_time_samples"]
                out.append(
                    f"| {r['test_id']} | {c} | {len(s)} | {statistics.median(s):.3f} | {min(s):.3f} | {max(s):.3f} |"
                )
    fails = [
        (r["test_id"], c, e)
        for r in rows
        for c, e in r["compilers"].items()
        if not ok(e) or e.get("runtime_status") not in (None, "ok", "skipped")
    ]
    if fails:
        out += ["", "### Failures", "", "| Case | Compiler | Error |", "|---|---|---|"]
        for t, c, e in fails:
            msg = (e.get("error") or e.get("runtime_status") or "").replace("\n", " ")
            msg = msg.replace("|", "\\|")[:160]
            out.append(f"| {t} | {c} | {msg} |")
    return "\n".join(out)


if __name__ == "__main__":
    parts = []
    for arg in sys.argv[1:]:
        label, path = arg.split("=", 1)
        parts.append(render(label, path))
    print("\n\n".join(parts))
