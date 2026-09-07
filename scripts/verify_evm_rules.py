# /// script
# requires-python = ">=3.11"
# dependencies = ["z3-solver==4.16.0.0"]
# ///
"""Verify actual ISLE rules or discover candidates offline; see evm_rules/README.md."""

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path

import z3

from evm_rules.isle import ISLE, ROOT, verify_file
from evm_rules.discovery import discover_rules


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    verify = subparsers.add_parser("verify", help="fail unless every selected source rule is proved")
    verify.add_argument("files", nargs="*", type=Path, default=[ISLE / "word.isle", ISLE / "stack_select.isle"])
    verify.add_argument("--timeout-ms", type=int, default=5000)
    verify.add_argument("--output", type=Path, required=True)
    verify.add_argument("--artifacts", type=Path)
    discover = subparsers.add_parser("discover", help="bounded enumerative search with SMT validation")
    discover.add_argument("--max-ops", type=int, default=3)
    discover.add_argument("--max-expressions", type=int, default=10000)
    discover.add_argument("--max-rules", type=int, default=32)
    discover.add_argument("--ops", nargs="+", default=["and", "or", "xor", "not", "add", "sub"])
    discover.add_argument("--variables", nargs="+", default=["x", "y"])
    discover.add_argument("--include-constants", action="store_true", help="also enumerate literal 0, 1 and MAX inputs")
    discover.add_argument("--evm-version", default="osaka")
    discover.add_argument("--objective", choices=["gas", "size", "lifetime"], default="gas")
    discover.add_argument("--runs", type=int, default=200)
    discover.add_argument("--timeout-ms", type=int, default=1000)
    discover.add_argument("--output", type=Path, required=True)
    discover.add_argument("--emit-isle", type=Path)
    args = parser.parse_args()
    if args.timeout_ms <= 0:
        parser.error("--timeout-ms must be positive")
    if args.command == "discover":
        report = discover_rules(args)
        exit_code = 0
    else:
        files = [verify_file(path, args.timeout_ms, args.artifacts) for path in args.files]
        counts = Counter(rule["status"] for file in files for rule in file["rules"])
        report = {"files": files, "counts": dict(counts)}
        exit_code = 0 if counts.get("proved", 0) and set(counts) == {"proved"} else 1
    implementation = sorted((Path(__file__).parent / "evm_rules").glob("*.py")) + [Path(__file__)]
    report.update(schema="solar:evm-word-rules@1", word_bits=256, solver=z3.get_version_string(),
                  implementation_sha256=hashlib.sha256(b"".join(p.read_bytes() for p in implementation)).hexdigest(),
                  selection_sha256=hashlib.sha256((ISLE / "select.isle").read_bytes()).hexdigest(),
                  extractors_sha256=hashlib.sha256((ISLE / "extractors.isle").read_bytes()).hexdigest())
    # These implementations remain trusted; record the exact versions reviewed
    # with the model rather than implying that their Rust bodies were proved.
    report["trusted_sources_sha256"] = {
        path: hashlib.sha256((ROOT / path).read_bytes()).hexdigest()
        for path in (
            "crates/codegen/src/mir/op_schema.rs",
            "crates/codegen/src/transform/egraph/isle.rs",
            "crates/codegen/src/backend/evm/codegen/select.rs",
            "crates/codegen/src/backend/evm/codegen/planning/isle.rs",
        )
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps(report.get("counts", report.get("summary")), sort_keys=True))
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
