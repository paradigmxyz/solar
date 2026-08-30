"""Read native release settings from dist-workspace.toml."""

# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

from __future__ import annotations

import argparse
from pathlib import Path

import tomllib

REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
DIST_WORKSPACE = REPOSITORY_ROOT / "dist-workspace.toml"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    args = parser.parse_args()

    config = load_dist_config()
    print(f"rust_toolchain={config['rust-toolchain-version']}")
    print(
        f"min_glibc_version={config.get('min-glibc-version', {}).get(args.target, '')}"
    )


def load_dist_config() -> dict:
    return tomllib.loads(DIST_WORKSPACE.read_text(encoding="utf-8"))["dist"]


def release_features() -> list[str]:
    return load_dist_config()["features"]


if __name__ == "__main__":
    main()
