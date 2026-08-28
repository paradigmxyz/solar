"""Add custom release archive checksums to a cargo-dist manifest."""

# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

from __future__ import annotations

import argparse
import json
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--artifacts-dir", type=Path, required=True)
    args = parser.parse_args()

    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    artifacts = manifest["artifacts"]
    patched = 0
    for checksum_path in sorted(args.artifacts_dir.glob("*.sha256")):
        artifact_name = checksum_path.name.removesuffix(".sha256")
        artifact = artifacts.get(artifact_name)
        if artifact is None:
            raise RuntimeError(
                f"Checksum {checksum_path.name} has no cargo-dist artifact"
            )
        artifact.setdefault("checksums", {})["sha256"] = read_sha256(checksum_path)
        patched += 1

    if patched == 0:
        raise RuntimeError(f"No checksums found in {args.artifacts_dir}")
    args.manifest.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"Patched {patched} artifact checksums in {args.manifest}")


def read_sha256(path: Path) -> str:
    checksum = path.read_text(encoding="utf-8").split(maxsplit=1)[0]
    if len(checksum) != 64 or any(
        character not in "0123456789abcdef" for character in checksum
    ):
        raise RuntimeError(f"Invalid SHA-256 checksum in {path}: {checksum!r}")
    return checksum


if __name__ == "__main__":
    main()
