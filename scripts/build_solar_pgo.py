"""Build Solar with profile-guided optimization using the benchmark corpus."""

from __future__ import annotations

import argparse
import os
import re
import shlex
import subprocess
import sys
import tempfile
from pathlib import Path

REPOSITORY_ROOT = Path(__file__).resolve().parent.parent
RUNTIME_BENCHMARK = REPOSITORY_ROOT / "benches" / "runtime" / "benchmark.py"
SYNTHETIC_CORPUS = REPOSITORY_ROOT / "testdata" / "repros"

# Keep several large projects out of the training corpus so they can be used as
# hold-outs when evaluating changes to the profile.
TRAINING_TESTS = (
    "factorial",
    "counter",
    "sum-array",
    "arithmetic",
    "openzeppelin-erc20-mock",
    "nitro-one-step-proof",
    "aave-l2-encoder",
    "lilweb3-fractional",
    "maple-erc20",
    "openzeppelin-governor",
)

DEBUG_TRAINING_TESTS = (
    "factorial",
    "openzeppelin-erc20-mock",
)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", help="Host-native Rust target triple")
    parser.add_argument(
        "--debug",
        action="store_true",
        help="Use debug builds and a reduced corpus to validate the PGO pipeline",
    )
    parser.add_argument(
        "--target-dir",
        type=Path,
        help="Cargo target directory (default: CARGO_TARGET_DIR or target/solar-pgo)",
    )
    parser.add_argument(
        "--profile-dir",
        type=Path,
        help="Raw profile directory (default: <target-dir>/profiles)",
    )
    parser.add_argument(
        "--llvm-profdata",
        type=Path,
        help="Override the active Rust toolchain's llvm-profdata executable",
    )
    parser.add_argument(
        "--train-only",
        action="store_true",
        help="Only produce <target-dir>/solar.profdata for a release build",
    )
    args = parser.parse_args()

    target_dir = (
        args.target_dir
        or Path(
            os.environ.get("CARGO_TARGET_DIR", REPOSITORY_ROOT / "target" / "solar-pgo")
        )
    ).resolve()
    profile_dir = (args.profile_dir or target_dir / "profiles").resolve()
    merged_profile = target_dir / "solar.profdata"

    host = rustc_host()
    target = args.target or host
    if target != host:
        parser.error(
            f"PGO training requires the host-native target {host}, got {target}"
        )

    profiler = find_llvm_profdata(host, args.llvm_profdata)
    environment = os.environ.copy()
    environment["CARGO_INCREMENTAL"] = "0"
    environment["RUSTFLAGS"] = target_rustflags(target, environment.get("RUSTFLAGS"))

    profile_dir.mkdir(parents=True, exist_ok=True)
    for profile in profile_dir.glob("solar-*.profraw"):
        profile.unlink()

    profile = "dev" if args.debug else "dist"
    profile_directory = "debug" if profile == "dev" else profile
    instrumented_target_dir = target_dir / "instrumented"
    instrumented_environment = environment | {
        "CARGO_TARGET_DIR": str(instrumented_target_dir),
        "RUSTFLAGS": append_flags(
            environment.get("RUSTFLAGS"), f"-Cprofile-generate={profile_dir}"
        ),
    }
    print(f"Building instrumented {profile} Solar", flush=True)
    run(
        cargo_command(target, profile),
        environment=instrumented_environment,
    )

    binary_name = "solar.exe" if "windows" in target else "solar"
    instrumented_binary = (
        instrumented_target_dir / target / profile_directory / binary_name
    )
    if not instrumented_binary.is_file():
        raise RuntimeError(
            f"Instrumented Solar binary not found: {instrumented_binary}"
        )

    profiles = train_solar(
        instrumented_binary,
        target_dir,
        profile_dir,
        debug=args.debug,
        environment=instrumented_environment,
    )
    merge_profiles(profiler, profiles, merged_profile, environment=environment)
    hot_count = profile_hot_count(profiler, merged_profile, environment=environment)
    (target_dir / "solar.profile-hot-count").write_text(
        f"{hot_count}\n", encoding="utf-8"
    )

    if args.train_only:
        return

    optimized_environment = environment | {
        "CARGO_TARGET_DIR": str(target_dir),
        "RUSTFLAGS": append_flags(
            environment.get("RUSTFLAGS"),
            f"-Cprofile-use={merged_profile} "
            f"-Cllvm-args=--profile-summary-hot-count={hot_count}",
        ),
    }
    print(f"Building profile-guided {profile} Solar", flush=True)
    run(cargo_command(target, profile), environment=optimized_environment)
    print(
        f"Profile-guided Solar: {target_dir / target / profile_directory / binary_name}",
        flush=True,
    )


def train_solar(
    binary: Path,
    target_dir: Path,
    profile_dir: Path,
    *,
    debug: bool,
    environment: dict[str, str],
) -> list[Path]:
    training_environment = environment | {
        "LLVM_PROFILE_FILE": str(profile_dir / "solar-%m-%p.profraw")
    }

    sizes = ("small",) if debug else ("small", "medium", "large")
    synthetic_sources = [
        source
        for size in sizes
        for source in sorted(SYNTHETIC_CORPUS.glob(f"*_{size}.sol"))
    ]
    if not synthetic_sources:
        raise RuntimeError(f"No synthetic benchmark inputs found in {SYNTHETIC_CORPUS}")

    print(
        f"Training on {len(synthetic_sources)} synthetic benchmark inputs", flush=True
    )
    for source in synthetic_sources:
        # The stress corpus includes expected-invalid inputs, such as one that
        # deliberately reaches the parser recursion limit.
        run(
            [str(binary), str(source), "--stop-after=parsing"],
            environment=training_environment,
            allowed_exit_codes=(0, 1),
        )
        run(
            [str(binary), str(source), "--stop-after=analysis"],
            environment=training_environment,
            allowed_exit_codes=(0, 1),
        )

    benchmark_output = target_dir / "training-corpus.json"
    tests = DEBUG_TRAINING_TESTS if debug else TRAINING_TESTS
    print(f"Training on {len(tests)} Standard JSON benchmark cases", flush=True)
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
            "--output",
            str(benchmark_output),
        ],
        environment=training_environment,
    )

    profiles = sorted(profile_dir.glob("solar-*.profraw"))
    if not profiles or any(profile.stat().st_size == 0 for profile in profiles):
        raise RuntimeError(f"No complete Solar profiling data found in {profile_dir}")
    return profiles


def merge_profiles(
    profiler: Path,
    profiles: list[Path],
    destination: Path,
    *,
    environment: dict[str, str],
) -> None:
    profile_size = sum(profile.stat().st_size for profile in profiles)
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        dir=destination.parent, prefix="solar-", suffix=".profdata", delete=False
    ) as temporary_file:
        temporary_profile = Path(temporary_file.name)
    try:
        run(
            [
                str(profiler),
                "merge",
                "--output",
                str(temporary_profile),
                *map(str, profiles),
            ],
            environment=environment,
        )
        temporary_profile.replace(destination)
    finally:
        temporary_profile.unlink(missing_ok=True)
    print(
        f"Merged {len(profiles)} PGO profiles ({profile_size:,} bytes): {destination}",
        flush=True,
    )


def rustc_host() -> str:
    version = subprocess.run(
        ["rustc", "--version", "--verbose"],
        cwd=REPOSITORY_ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    for line in version.splitlines():
        if line.startswith("host: "):
            return line.removeprefix("host: ")
    raise RuntimeError("Could not determine the active Rust compiler's host target")


def profile_hot_count(
    profiler: Path, profile: Path, *, environment: dict[str, str]
) -> int:
    # LLVM uses the 95th percentile for profile-guided size optimization, but
    # defaults to the 99th percentile for hot-code optimization. Align them to
    # avoid aggressively expanding moderately hot functions.
    summary = subprocess.run(
        [
            str(profiler),
            "show",
            "--detailed-summary",
            "--detailed-summary-cutoffs=950000",
            str(profile),
        ],
        cwd=REPOSITORY_ROOT,
        env=environment,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    match = re.search(
        r"with count >= (\d+) account for 95% of the total counts\.", summary
    )
    if match is None:
        raise RuntimeError("Could not determine the 95th-percentile PGO hot count")

    count = int(match.group(1))
    if count <= 0:
        raise RuntimeError(f"PGO hot count must be positive, got {count}")
    return count


def find_llvm_profdata(host: str, override: Path | None) -> Path:
    if override is not None:
        profiler = override.resolve()
    else:
        sysroot = subprocess.run(
            ["rustc", "--print", "sysroot"],
            cwd=REPOSITORY_ROOT,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        binary_name = "llvm-profdata.exe" if "windows" in host else "llvm-profdata"
        profiler = Path(sysroot) / "lib" / "rustlib" / host / "bin" / binary_name

    if not profiler.is_file() or not os.access(profiler, os.X_OK):
        raise RuntimeError(
            f"Rust toolchain llvm-profdata not found: {profiler}; "
            "run `rustup component add llvm-tools-preview`"
        )
    return profiler


def cargo_command(target: str, profile: str) -> list[str]:
    cargo = shlex.split(os.environ.get("CARGO", "cargo"))
    return [
        *cargo,
        "build",
        "--locked",
        "--profile",
        profile,
        "--target",
        target,
        "--package",
        "solar-compiler",
        "--bin",
        "solar",
        "--features",
        "cli,asm,mimalloc",
    ]


def target_rustflags(target: str, flags: str | None) -> str:
    if target == "x86_64-pc-windows-msvc" and "/STACK:10000000" not in (flags or ""):
        return append_flags(flags, "-Clink-arg=/STACK:10000000")
    return flags or ""


def append_flags(current: str | None, addition: str) -> str:
    return f"{current} {addition}".strip() if current else addition


def run(
    command: list[str],
    *,
    environment: dict[str, str],
    allowed_exit_codes: tuple[int, ...] = (0,),
) -> None:
    print(f"+ {shlex.join(command)}", flush=True)
    result = subprocess.run(command, cwd=REPOSITORY_ROOT, env=environment, check=False)
    if result.returncode not in allowed_exit_codes:
        raise subprocess.CalledProcessError(result.returncode, command)


if __name__ == "__main__":
    main()
