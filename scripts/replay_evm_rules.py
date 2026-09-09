# /// script
# requires-python = ">=3.11"
# ///
"""Replay every saved proof query with cvc5, failing unless all return UNSAT.

This independently checks the exported formulas, including partition coverage
and physical-stack variants. It does not independently validate the semantics
that generated those formulas or provide a checked proof certificate.
"""

import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

from evm_rules.artifacts import query_paths


def replay_query(path, digest, solver, timeout_ms):
    result = {"query": path, "sha256": digest}
    try:
        data = Path(path).read_bytes()
        if hashlib.sha256(data).hexdigest() != digest:
            raise ValueError("saved query hash differs from the proof report")
        # Hash and execute the same bytes, even if the file changes concurrently.
        attempts = []
        for strategy in ([], ["--solve-bv-as-int=sum"]):
            try:
                process = subprocess.run(
                    [solver, "--lang", "smt2", f"--tlimit={timeout_ms}", *strategy],
                    input=data, capture_output=True, timeout=timeout_ms / 1000 + 1,
                )
                stdout = process.stdout.decode(errors="replace").strip()
                stderr = process.stderr.decode(errors="replace").strip()
                status = stdout if process.returncode == 0 and stdout in ("unsat", "sat", "unknown") else "error"
                if status == "error" and "interrupted by timeout" in stderr:
                    status = "timeout"
                attempt = dict(status=status, flags=strategy, returncode=process.returncode,
                               stdout=stdout[:4096], stderr=stderr[:4096])
            except subprocess.TimeoutExpired:
                attempt = dict(status="timeout", flags=strategy,
                               reason="solver process exceeded its time limit")
            attempts.append(attempt)
            result.update(attempt)
            # Never hide SAT or a parse/process error by trying another strategy.
            if attempt["status"] not in ("timeout", "unknown"):
                break
        result["attempts"] = attempts
    except (OSError, ValueError) as error:
        result.update(status="error", reason=str(error))
    return result


def replay_report(report_path, solver="cvc5", timeout_ms=5000, jobs=4):
    data = report_path.read_bytes()
    report = json.loads(data)
    if report.get("schema") != "solar:evm-word-rules@1" or report.get("word_bits") != 256:
        raise ValueError("unsupported proof report schema or word width")
    paths = query_paths(report, require_proved=True)
    manifest = report.get("query_sha256", {})
    if set(manifest) != set(paths):
        raise ValueError("proof report must fingerprint exactly every saved query")
    for digest in manifest.values():
        if not isinstance(digest, str) or len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest):
            raise ValueError("invalid query SHA-256")
    executable = shutil.which(solver)
    if executable is None:
        raise ValueError(f"solver executable not found: {solver}")
    version = subprocess.run([executable, "--version"], capture_output=True, text=True, timeout=5)
    if version.returncode != 0 or "cvc5" not in version.stdout.lower():
        raise ValueError("the replay solver must identify itself as cvc5")
    with ThreadPoolExecutor(max_workers=jobs) as pool:
        results = list(pool.map(lambda path: replay_query(path, manifest[path], executable, timeout_ms), paths))
    counts = dict(Counter(row["status"] for row in results))
    return {
        "schema": "solar:evm-rule-replay@1",
        "proof_report": str(report_path),
        "proof_report_sha256": hashlib.sha256(data).hexdigest(),
        "solver": version.stdout.strip(),
        "solver_executable": executable,
        "timeout_ms_per_strategy": timeout_ms,
        "rule_count": sum(len(file["rules"]) for file in report["files"]),
        "query_count": len(paths),
        "counts": counts,
        "queries": results,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("--solver", default="cvc5")
    parser.add_argument("--timeout-ms", type=int, default=5000)
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.timeout_ms <= 0 or not 1 <= args.jobs <= 32:
        parser.error("timeout must be positive and jobs must be between 1 and 32")
    try:
        report = replay_report(args.report, args.solver, args.timeout_ms, args.jobs)
    except (OSError, ValueError, subprocess.TimeoutExpired) as error:
        report = {"schema": "solar:evm-rule-replay@1", "counts": {"error": 1}, "error": str(error)}
        print(str(error), file=sys.stderr)
    for row in report.get("queries", []):
        if row["status"] != "unsat":
            print(f"{row['query']}: {row['status']}: {row.get('reason', row.get('stderr', ''))}", file=sys.stderr)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps(report["counts"], sort_keys=True))
    return 0 if report["counts"].get("unsat", 0) and set(report["counts"]) == {"unsat"} else 1


if __name__ == "__main__":
    raise SystemExit(main())
