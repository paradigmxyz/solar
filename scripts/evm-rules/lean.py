# /// script
# requires-python = ">=3.14"
# dependencies = ["z3-solver==4.16.0.0"]
# ///
"""Generate and check Lean proofs for the actual MIR word rules."""

import argparse
import subprocess
from pathlib import Path

from evm_rules.isle import ISLE
from evm_rules.lean import generate


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output", type=Path, default=Path("target/evm-rules/lean/Word.lean")
    )
    args = parser.parse_args()
    version = (
        (Path(__file__).parent / "lean/lean-toolchain")
        .read_text()
        .strip()
        .removeprefix("leanprover/lean4:v")
    )
    installed = subprocess.run(
        ["lean", "--version"], check=True, capture_output=True, text=True, timeout=10
    ).stdout
    if not installed.startswith(f"Lean (version {version},"):
        parser.error(f"expected Lean {version}, got {installed.strip()}")
    source, count = generate(ISLE / "mir/word.isle")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(source)
    subprocess.run(
        ["lean", "-DwarningAsError=true", str(args.output)], check=True, timeout=600
    )
    print(f"Lean proved all {count} MIR word rules")


if __name__ == "__main__":
    main()
