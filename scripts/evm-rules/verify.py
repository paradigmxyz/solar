# /// script
# requires-python = ">=3.14"
# ///
"""Prove the actual ISLE rules in Lean, or discover candidates offline; see README.md."""

import argparse
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

from evm_rules.discovery import discover_rules
from evm_rules.isle import ISLE, ROOT
from evm_rules.mining import mine
from evm_rules.prover import LEAN_PROJECT, lean_environment
from evm_rules.verification import DEFAULT_FILES, verify_files


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    verify = subparsers.add_parser(
        "verify", help="fail unless Lean proves every selected source rule"
    )
    verify.add_argument("files", nargs="*", type=Path, default=DEFAULT_FILES)
    verify.add_argument("--jobs", type=int, default=os.cpu_count() or 4)
    verify.add_argument(
        "--isolated",
        action="store_true",
        help="start Lean per theorem instead of reusing workers, for comparison",
    )
    verify.add_argument(
        "--timeout-s",
        type=int,
        default=120,
        help="SAT limit per proof; each rule may run twice that, plus 30 seconds",
    )
    verify.add_argument(
        "--work-dir",
        type=Path,
        default=Path("target/evm-rules/lean"),
        help="directory for the checked theorem files",
    )
    verify.add_argument("--output", type=Path, required=True)
    discover = subparsers.add_parser(
        "discover", help="bounded enumerative search with Lean validation"
    )
    discover.add_argument("--max-ops", type=int, default=3)
    discover.add_argument(
        "--max-rhs-ops",
        type=int,
        default=2,
        help="maximum operations in a replacement recipe",
    )
    discover.add_argument("--max-expressions", type=int, default=10000)
    discover.add_argument("--max-rules", type=int, default=32)
    discover.add_argument(
        "--seed-expressions",
        type=Path,
        help="JSON input trees to simplify against the bounded replacement frontier",
    )
    discover.add_argument(
        "--ops", nargs="+", default=["and", "or", "xor", "not", "add", "sub"]
    )
    discover.add_argument(
        "--result-ops",
        nargs="+",
        help="focus emitted candidates on these replacement root operations",
    )
    discover.add_argument("--variables", nargs="+", default=["x", "y"])
    discover.add_argument(
        "--include-constants",
        action="store_true",
        help="also enumerate literal 0, 1 and MAX inputs",
    )
    discover.add_argument(
        "--constants",
        nargs="+",
        type=lambda s: int(s, 0),
        help="literal inputs/results with exported Target prices",
    )
    discover.add_argument("--evm-version", default="osaka")
    discover.add_argument(
        "--objective", choices=["gas", "size", "lifetime"], default="gas"
    )
    discover.add_argument("--runs", type=int, default=200)
    discover.add_argument(
        "--timeout-ms",
        type=int,
        default=1000,
        help="SAT limit per candidate query, rounded up to whole seconds",
    )
    discover.add_argument("--output", type=Path, required=True)
    discover.add_argument("--emit-isle", type=Path)
    miner = subparsers.add_parser(
        "mine", help="mine bounded pure trees from real MIR artifacts"
    )
    miner.add_argument("files", nargs="+", type=Path)
    miner.add_argument("--max-ops", type=int, default=8)
    miner.add_argument("--max-seeds", type=int, default=128)
    miner.add_argument(
        "--abstract-subtrees",
        action="store_true",
        help="also replace one internal subtree with an independent symbolic input",
    )
    miner.add_argument("--evm-version", default="osaka")
    miner.add_argument(
        "--objective", choices=["gas", "size", "lifetime"], default="gas"
    )
    miner.add_argument("--runs", type=int, default=200)
    miner.add_argument("--output", type=Path, required=True)
    miner.add_argument("--emit-seeds", type=Path, required=True)
    args = parser.parse_args()
    if getattr(args, "timeout_ms", 1) <= 0:
        parser.error("--timeout-ms must be positive")
    if getattr(args, "timeout_s", 1) <= 0:
        parser.error("--timeout-s must be positive")
    if getattr(args, "jobs", 1) <= 0:
        parser.error("--jobs must be positive")
    if args.command == "mine":
        if args.runs < 0:
            parser.error("--runs must be nonnegative")
        report = mine(
            args.files,
            fork=args.evm_version,
            objective=args.objective,
            runs=args.runs,
            max_ops=args.max_ops,
            max_seeds=args.max_seeds,
            abstract_subtrees=args.abstract_subtrees,
        )
        args.emit_seeds.parent.mkdir(parents=True, exist_ok=True)
        args.emit_seeds.write_text(
            json.dumps([row["tree"] for row in report["candidates"]], indent=2) + "\n"
        )
        exit_code = 0 if report["candidates"] else 1
    elif args.command == "discover":
        report = discover_rules(args)
        exit_code = 0 if report.get("accepted", True) else 1
    else:
        try:
            lean_path = lean_environment()
        except subprocess.CalledProcessError as error:
            print(error.stdout + error.stderr, file=sys.stderr)
            return 1
        report = verify_files(
            args.files,
            args.work_dir.resolve(),
            lean_path,
            jobs=args.jobs,
            timeout_s=args.timeout_s,
            isolated=args.isolated,
        )
        for file in report["files"]:
            for rule in file["rules"]:
                if rule["status"] != "proved":
                    reason = rule.get("error") or next(
                        (
                            p.get("reason") or p.get("output", "")[-300:]
                            for p in rule.get("proofs", [])
                            if p.get("status") != "proved"
                        ),
                        "",
                    )
                    source = rule.get("source", file["source"])
                    print(
                        f"{source}:{rule['line']}: {rule['status']}: {reason}",
                        file=sys.stderr,
                    )
        counts = report["counts"]
        exit_code = 0 if counts.get("proved", 0) and set(counts) == {"proved"} else 1
    implementation = sorted((Path(__file__).parent / "evm_rules").glob("*.py")) + [
        Path(__file__)
    ]
    lean_sources = [
        path
        for path in sorted(LEAN_PROJECT.glob("**/*.lean"))
        if ".lake" not in path.parts
    ]
    report.update(
        schema="solar:evm-word-rules@2",
        word_bits=256,
        prover=subprocess.run(
            ["lean", "--version"],
            cwd=LEAN_PROJECT,
            capture_output=True,
            text=True,
            check=False,
        ).stdout.strip(),
        implementation_sha256=hashlib.sha256(
            b"".join(p.read_bytes() for p in implementation)
        ).hexdigest(),
        lean_sha256=hashlib.sha256(
            b"".join(p.read_bytes() for p in lean_sources)
        ).hexdigest(),
        selection_sha256=hashlib.sha256(
            (ISLE / "mir-to-evm/select.isle").read_bytes()
        ).hexdigest(),
        prelude_sha256=hashlib.sha256(
            (ISLE / "mir/prelude.isle").read_bytes()
        ).hexdigest(),
        extractors_sha256=hashlib.sha256(
            (ISLE / "mir/extractors.isle").read_bytes()
        ).hexdigest(),
    )
    # These implementations remain trusted; record the exact versions reviewed
    # with the model rather than implying that their Rust bodies were proved.
    report["trusted_sources_sha256"] = {
        path: hashlib.sha256((ROOT / path).read_bytes()).hexdigest()
        for path in (
            "crates/codegen/src/mir/op_schema.rs",
            "crates/codegen/src/mir/transform/egraph/isle.rs",
            "crates/codegen/src/mir/transform/egraph.rs",
            "crates/codegen/src/mir/utils/eval.rs",
            "crates/codegen/src/mir/memory.rs",
            "crates/codegen/src/mir/types.rs",
            "crates/codegen/src/mir/transform/lower_memory_objects.rs",
            "crates/codegen/src/mir/transform/lower_slices.rs",
            "crates/codegen/src/mir/transform/word_sequence.rs",
            "crates/codegen/src/mir/transform/word_sequence/isle.rs",
            "crates/codegen/src/backend/evm/codegen/select.rs",
            "crates/codegen/src/backend/evm/codegen/planning/isle.rs",
            "crates/codegen/src/backend/evm/ir/passes/peephole.rs",
            "crates/codegen/src/backend/evm/ir/passes/peephole/isle.rs",
            "crates/codegen/src/backend/evm/op.rs",
        )
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(report, indent=2, sort_keys=True, default=str) + "\n"
    )
    if "methods" in report:
        print(json.dumps({"methods": report["methods"]}, sort_keys=True))
    print(json.dumps(report.get("counts", report.get("summary")), sort_keys=True))
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
