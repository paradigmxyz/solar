"""Compare baseline and PGO Solar binaries on a source-disjoint corpus."""

# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import platform
import statistics
import subprocess
import sys
from pathlib import Path

REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
RUNTIME_ROOT = REPOSITORY_ROOT / "benches" / "runtime"
RUNTIME_BENCHMARK = RUNTIME_ROOT / "benchmark.py"
sys.path.insert(0, str(RUNTIME_ROOT))

from benchmark import compiler_input
from build import EVALUATION_TESTS, SYNTHETIC_CORPUS, TRAINING_TESTS
from cases import TEST_CASES


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--pgo", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--compile-repeats", type=int, default=10)
    parser.add_argument("--minimum-improvement", type=float, default=3.0)
    parser.add_argument("--maximum-case-regression", type=float, default=5.0)
    parser.add_argument(
        "--maximum-case-regression-milliseconds", type=float, default=5.0
    )
    parser.add_argument("--maximum-run-drift", type=float, default=5.0)
    parser.add_argument("--maximum-run-drift-milliseconds", type=float, default=5.0)
    args = parser.parse_args()

    baseline = args.baseline.resolve()
    pgo = args.pgo.resolve()
    for binary in (baseline, pgo):
        if not binary.is_file():
            parser.error(f"Solar binary not found: {binary}")
    if args.compile_repeats <= 0:
        parser.error("--compile-repeats must be positive")

    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    split = corpus_split()
    write_json(output_dir / "split.json", split)

    for binary in (baseline, pgo):
        run([str(binary), "--version"])

    benchmark(baseline, output_dir / "warmup-baseline.json", 1)
    benchmark(pgo, output_dir / "warmup-pgo.json", 1)

    binaries = {
        "baseline_1": baseline,
        "pgo_1": pgo,
        "pgo_2": pgo,
        "baseline_2": baseline,
    }
    results = {name: {} for name in binaries}
    for test_id in EVALUATION_TESTS:
        for name, binary in binaries.items():
            path = output_dir / f"{test_id}-{name.replace('_', '-')}.json"
            benchmark(binary, path, args.compile_repeats, tests=(test_id,))
            results[name].update(load_results(path))
    report = compare_results(baseline, pgo, results, split, args.compile_repeats)
    report["gates"] = evaluate_gates(
        report,
        minimum_improvement=args.minimum_improvement,
        maximum_case_regression=args.maximum_case_regression,
        maximum_case_regression_milliseconds=args.maximum_case_regression_milliseconds,
        maximum_run_drift=args.maximum_run_drift,
        maximum_run_drift_milliseconds=args.maximum_run_drift_milliseconds,
    )
    write_json(output_dir / "report.json", report)
    markdown = format_markdown(report)
    print(markdown)
    if summary_path := os.environ.get("GITHUB_STEP_SUMMARY"):
        with Path(summary_path).open("a", encoding="utf-8") as summary:
            summary.write(markdown)
            summary.write("\n")

    failed = [gate["failure_message"] for gate in report["gates"] if not gate["passed"]]
    if failed:
        raise RuntimeError("; ".join(failed))


def corpus_split() -> dict[str, object]:
    tests = {test.test_id: test for test in TEST_CASES}
    selected = set(TRAINING_TESTS) | set(EVALUATION_TESTS)
    missing = sorted(selected - tests.keys())
    if missing:
        raise RuntimeError(f"Unknown PGO corpus cases: {', '.join(missing)}")
    overlap = sorted(set(TRAINING_TESTS) & set(EVALUATION_TESTS))
    if overlap:
        raise RuntimeError(f"PGO cases used for training and evaluation: {overlap}")

    training = split_cases(TRAINING_TESTS, tests)
    evaluation = split_cases(EVALUATION_TESTS, tests)
    training_hashes = source_hashes(training)
    synthetic_sources = {
        str(source.relative_to(REPOSITORY_ROOT)): hashlib.sha256(
            source.read_bytes()
        ).hexdigest()
        for source in sorted(SYNTHETIC_CORPUS.glob("*.sol"))
    }
    training_hashes.update(synthetic_sources.values())
    evaluation_hashes = source_hashes(evaluation)
    source_overlap = sorted(training_hashes & evaluation_hashes)
    if source_overlap:
        raise RuntimeError(
            f"PGO training and evaluation share {len(source_overlap)} source hashes"
        )

    return {
        "training": training,
        "synthetic_training_sources": synthetic_sources,
        "evaluation": evaluation,
        "unused_test_ids": sorted(tests.keys() - selected),
        "source_hash_overlap": source_overlap,
    }


def split_cases(test_ids: tuple[str, ...], tests: dict[str, object]) -> list[dict]:
    cases = []
    for test_id in test_ids:
        input_text, _, input_fingerprint = compiler_input(tests[test_id], None)
        payload = json.loads(input_text)
        sources = {
            name: hashlib.sha256(source["content"].encode()).hexdigest()
            for name, source in payload["sources"].items()
        }
        cases.append(
            {
                "test_id": test_id,
                "input_fingerprint": input_fingerprint,
                "sources": sources,
            }
        )
    return cases


def source_hashes(cases: list[dict]) -> set[str]:
    return {digest for case in cases for digest in case["sources"].values()}


def benchmark(
    binary: Path,
    output: Path,
    repeats: int,
    *,
    tests: tuple[str, ...] = EVALUATION_TESTS,
) -> None:
    run(
        [
            sys.executable,
            str(RUNTIME_BENCHMARK),
            "--solar",
            str(binary),
            "--solar-only",
            "--mode",
            "runtime",
            "compile-time",
            "--tests",
            *tests,
            "--compile-repeats",
            str(repeats),
            "--repeat-long-compiles",
            "--output",
            str(output),
        ]
    )


def load_results(path: Path) -> dict[str, dict]:
    document = json.loads(path.read_text(encoding="utf-8"))
    return {
        result["test_id"]: result["compilers"]["solar"]
        for result in document["results"]
    }


def compare_results(
    baseline: Path,
    pgo: Path,
    results: dict[str, dict[str, dict]],
    split: dict[str, object],
    repeats: int,
) -> dict[str, object]:
    cases = []
    ratios = []
    baseline_drift_ratios = []
    pgo_drift_ratios = []
    for test_id in EVALUATION_TESTS:
        samples = {name: runs[test_id] for name, runs in results.items()}
        for name, result in samples.items():
            if result["status"] != "ok":
                raise RuntimeError(f"{test_id} failed in {name}: {result['error']}")
            sample_count = len(result["compile_time_samples"])
            if sample_count != repeats:
                raise RuntimeError(
                    f"{test_id} recorded {sample_count} samples in {name}, "
                    f"expected {repeats}"
                )

        input_fingerprints = {
            result["input_fingerprint"] for result in samples.values()
        }
        output_fingerprints = {
            result["output_fingerprint"] for result in samples.values()
        }
        if len(input_fingerprints) != 1 or len(output_fingerprints) != 1:
            raise RuntimeError(f"Compiler input or output differs for {test_id}")

        baseline_samples = [
            *samples["baseline_1"]["compile_time_samples"],
            *samples["baseline_2"]["compile_time_samples"],
        ]
        pgo_samples = [
            *samples["pgo_1"]["compile_time_samples"],
            *samples["pgo_2"]["compile_time_samples"],
        ]
        baseline_median = statistics.median(baseline_samples)
        pgo_median = statistics.median(pgo_samples)
        ratio = pgo_median / baseline_median
        baseline_drift_ratio, baseline_case_drift_seconds = sample_drift(
            samples, "baseline_1", "baseline_2"
        )
        pgo_drift_ratio, pgo_case_drift_seconds = sample_drift(
            samples, "pgo_1", "pgo_2"
        )
        ratios.append(ratio)
        baseline_drift_ratios.append(baseline_drift_ratio)
        pgo_drift_ratios.append(pgo_drift_ratio)
        cases.append(
            {
                "test_id": test_id,
                "baseline_samples": len(baseline_samples),
                "pgo_samples": len(pgo_samples),
                "baseline_median_seconds": baseline_median,
                "pgo_median_seconds": pgo_median,
                "pgo_to_baseline_ratio": ratio,
                "elapsed_time_reduction_percent": (1.0 - ratio) * 100.0,
                "baseline_run_drift_percent": (baseline_drift_ratio - 1.0) * 100.0,
                "pgo_run_drift_percent": (pgo_drift_ratio - 1.0) * 100.0,
                "baseline_run_drift_seconds": baseline_case_drift_seconds,
                "pgo_run_drift_seconds": pgo_case_drift_seconds,
                "output_fingerprint": output_fingerprints.pop(),
            }
        )

    aggregate_ratio = math.exp(sum(math.log(ratio) for ratio in ratios) / len(ratios))
    baseline_drift = geometric_mean_change(baseline_drift_ratios)
    pgo_drift = geometric_mean_change(pgo_drift_ratios)
    maximum_case_drift = max(
        abs(case[key])
        for case in cases
        for key in ("baseline_run_drift_percent", "pgo_run_drift_percent")
    )
    maximum_case_drift_seconds = max(
        abs(case[key])
        for case in cases
        for key in ("baseline_run_drift_seconds", "pgo_run_drift_seconds")
    )
    return {
        "environment": environment_report(),
        "split": split,
        "binaries": {
            "baseline": binary_report(baseline),
            "pgo": binary_report(pgo),
        },
        "cases": cases,
        "aggregate": {
            "pgo_to_baseline_ratio": aggregate_ratio,
            "elapsed_time_reduction_percent": (1.0 - aggregate_ratio) * 100.0,
            "throughput_gain_percent": (1.0 / aggregate_ratio - 1.0) * 100.0,
            "baseline_run_drift_percent": baseline_drift,
            "pgo_run_drift_percent": pgo_drift,
            "maximum_case_run_drift_percent": maximum_case_drift,
            "maximum_case_run_drift_seconds": maximum_case_drift_seconds,
            "binary_size_reduction_percent": (
                1.0 - pgo.stat().st_size / baseline.stat().st_size
            )
            * 100.0,
        },
    }


def sample_drift(
    samples: dict[str, dict], first: str, second: str
) -> tuple[float, float]:
    first_median = statistics.median(samples[first]["compile_time_samples"])
    second_median = statistics.median(samples[second]["compile_time_samples"])
    return second_median / first_median, second_median - first_median


def geometric_mean_change(ratios: list[float]) -> float:
    ratio = math.exp(sum(math.log(value) for value in ratios) / len(ratios))
    return (ratio - 1.0) * 100.0


def evaluate_gates(
    report: dict[str, object],
    *,
    minimum_improvement: float,
    maximum_case_regression: float,
    maximum_case_regression_milliseconds: float,
    maximum_run_drift: float,
    maximum_run_drift_milliseconds: float,
) -> list[dict[str, object]]:
    aggregate = report["aggregate"]
    improvement = aggregate["elapsed_time_reduction_percent"]
    regression_observations = []
    regressed_cases = []
    unstable_cases = []
    drift_observations = []
    for case in report["cases"]:
        percent = max(0.0, -case["elapsed_time_reduction_percent"])
        milliseconds = max(
            0.0,
            (case["pgo_median_seconds"] - case["baseline_median_seconds"]) * 1000.0,
        )
        passed = (
            percent <= maximum_case_regression
            or milliseconds <= maximum_case_regression_milliseconds
        )
        regression_observations.append(
            {
                "test_id": case["test_id"],
                "percent": percent,
                "milliseconds": milliseconds,
                "passed": passed,
            }
        )
        if not passed:
            regressed_cases.append(case["test_id"])

        for prefix in ("baseline", "pgo"):
            percent = abs(case[f"{prefix}_run_drift_percent"])
            milliseconds = abs(case[f"{prefix}_run_drift_seconds"]) * 1000.0
            passed = (
                percent <= maximum_run_drift
                or milliseconds <= maximum_run_drift_milliseconds
            )
            drift_observations.append(
                {
                    "test_id": case["test_id"],
                    "binary": prefix,
                    "percent": percent,
                    "milliseconds": milliseconds,
                    "passed": passed,
                }
            )
            if not passed and case["test_id"] not in unstable_cases:
                unstable_cases.append(case["test_id"])
    size_change = aggregate["binary_size_reduction_percent"]
    return [
        {
            "name": "minimum_elapsed_time_reduction",
            "threshold_percent": minimum_improvement,
            "value_percent": improvement,
            "passed": improvement >= minimum_improvement,
            "failure_message": (
                f"PGO improvement {improvement:.2f}% is below "
                f"{minimum_improvement:.2f}%"
            ),
        },
        {
            "name": "maximum_case_regression",
            "threshold_percent": maximum_case_regression,
            "threshold_milliseconds": maximum_case_regression_milliseconds,
            "observations": regression_observations,
            "regressed_test_ids": regressed_cases,
            "passed": not regressed_cases,
            "failure_message": (
                "PGO case regression exceeded both "
                f"{maximum_case_regression:.2f}% and "
                f"{maximum_case_regression_milliseconds:.2f}ms for "
                f"{', '.join(regressed_cases)}"
            ),
        },
        {
            "name": "maximum_case_run_drift",
            "threshold_percent": maximum_run_drift,
            "threshold_milliseconds": maximum_run_drift_milliseconds,
            "observations": drift_observations,
            "unstable_test_ids": unstable_cases,
            "passed": not unstable_cases,
            "failure_message": (
                "Run drift exceeded both "
                f"{maximum_run_drift:.2f}% and "
                f"{maximum_run_drift_milliseconds:.2f}ms for "
                f"{', '.join(unstable_cases)}"
            ),
        },
        {
            "name": "no_binary_size_growth",
            "threshold_percent": 0.0,
            "value_percent": size_change,
            "passed": size_change >= 0.0,
            "failure_message": f"PGO increased binary size by {-size_change:.2f}%",
        },
    ]


def binary_report(binary: Path) -> dict[str, object]:
    return {
        "path": str(binary),
        "size_bytes": binary.stat().st_size,
        "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    }


def environment_report() -> dict[str, object]:
    return {
        "git_commit": capture(["git", "rev-parse", "HEAD"]),
        "git_status": capture(["git", "status", "--short"]),
        "rustc": capture(["rustc", "--version", "--verbose"]),
        "cargo": capture(["cargo", "--version"]),
        "platform": platform.platform(),
        "processor": platform.processor(),
        "cpu_model": linux_cpu_model(),
        "cpu_count": os.cpu_count(),
        "cpu_affinity": (
            sorted(os.sched_getaffinity(0))
            if hasattr(os, "sched_getaffinity")
            else None
        ),
        "load_average": os.getloadavg() if hasattr(os, "getloadavg") else None,
        "cpu_governor": read_text_if_exists(
            Path("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
        ),
        "github_runner_name": os.environ.get("RUNNER_NAME"),
        "github_runner_environment": os.environ.get("RUNNER_ENVIRONMENT"),
        "github_runner_arch": os.environ.get("RUNNER_ARCH"),
        "github_runner_os": os.environ.get("RUNNER_OS"),
        "github_runner_image": os.environ.get("ImageOS"),
        "github_runner_image_version": os.environ.get("ImageVersion"),
        "rayon_num_threads": os.environ.get("RAYON_NUM_THREADS"),
    }


def linux_cpu_model() -> str | None:
    cpuinfo = Path("/proc/cpuinfo")
    if not cpuinfo.is_file():
        return None
    for line in cpuinfo.read_text(encoding="utf-8").splitlines():
        if line.startswith("model name"):
            return line.partition(":")[2].strip()
    return None


def read_text_if_exists(path: Path) -> str | None:
    return path.read_text(encoding="utf-8").strip() if path.is_file() else None


def format_markdown(report: dict[str, object]) -> str:
    lines = [
        "## Solar PGO comparison",
        "",
        "| Case | Samples | Baseline | PGO | Time reduction |",
        "| --- | ---: | ---: | ---: | ---: |",
    ]
    for case in report["cases"]:
        lines.append(
            f"| `{case['test_id']}` | {case['baseline_samples']} / "
            f"{case['pgo_samples']} | "
            f"{case['baseline_median_seconds']:.6f}s | "
            f"{case['pgo_median_seconds']:.6f}s | "
            f"{case['elapsed_time_reduction_percent']:+.2f}% |"
        )
    aggregate = report["aggregate"]
    drift_gate = next(
        gate for gate in report["gates"] if gate["name"] == "maximum_case_run_drift"
    )
    baseline = report["binaries"]["baseline"]
    pgo = report["binaries"]["pgo"]
    lines.extend(
        [
            "",
            f"Elapsed-time reduction: **{aggregate['elapsed_time_reduction_percent']:.2f}%**",
            f"Throughput gain: **{aggregate['throughput_gain_percent']:.2f}%**",
            f"Baseline run drift: **{aggregate['baseline_run_drift_percent']:+.2f}%**",
            f"PGO run drift: **{aggregate['pgo_run_drift_percent']:+.2f}%**",
            f"Largest percentage run drift: **{aggregate['maximum_case_run_drift_percent']:.2f}%**",
            (
                "Largest absolute run drift: "
                f"**{aggregate['maximum_case_run_drift_seconds'] * 1000.0:.2f}ms**"
            ),
            (
                "Drift tolerance per case: "
                f"**{drift_gate['threshold_percent']:.2f}% or "
                f"{drift_gate['threshold_milliseconds']:.2f}ms**"
            ),
            (
                f"Binary size: **{baseline['size_bytes']:,} → "
                f"{pgo['size_bytes']:,} bytes** "
                f"({aggregate['binary_size_reduction_percent']:.2f}% smaller)"
            ),
            "Canonical compiler outputs matched for every case; diagnostic order was ignored.",
        ]
    )
    return "\n".join(lines)


def write_json(path: Path, value: object) -> None:
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )


def capture(command: list[str]) -> str:
    return subprocess.run(
        command,
        cwd=REPOSITORY_ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def run(command: list[str]) -> None:
    print(f"+ {' '.join(command)}", flush=True)
    subprocess.run(command, cwd=REPOSITORY_ROOT, check=True)


if __name__ == "__main__":
    main()
