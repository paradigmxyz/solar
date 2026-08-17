#!/usr/bin/env python3
"""Create one temporary LSP benchmark config for a PR-built Solar binary."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import tempfile
from pathlib import Path


def replace_once(text: str, pattern: str, replacement: str, description: str) -> str:
    matches = list(re.finditer(pattern, text, flags=re.MULTILINE))
    if len(matches) != 1:
        raise ValueError(f"expected exactly one {description}, found {len(matches)}")
    match = matches[0]
    return text[: match.start()] + replacement + text[match.end() :]


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def paths_alias(left: Path, right: Path) -> bool:
    if left == right:
        return True
    try:
        return left.samefile(right)
    except FileNotFoundError:
        return False


def write_text_atomic(path: Path, text: str) -> None:
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as file:
            file.write(text)
        os.replace(temporary, path)
    finally:
        try:
            Path(temporary).unlink()
        except FileNotFoundError:
            pass


def update_solar_server(
    lock: str, binary: Path, revision: str, source_url: str, role: str
) -> str:
    matches = list(re.finditer(r"^  - id: solar\s*$", lock, flags=re.MULTILINE))
    if len(matches) != 1:
        raise ValueError(f"expected exactly one Solar server, found {len(matches)}")

    start = matches[0].start()
    next_server = re.search(r"^  - id: ", lock[matches[0].end() :], flags=re.MULTILINE)
    end = matches[0].end() + next_server.start() if next_server else len(lock)
    server = lock[start:end]
    label = f"Solar PR {role} {revision[:12]}"
    replacements = [
        (r"^    label: .*$", f"    label: {json.dumps(label)}", "Solar label"),
        (r"^    command: .*$", f"    command: {json.dumps(str(binary))}", "Solar command"),
        (r"^    locked_version: .*$", "    locked_version: null", "Solar locked version"),
        (r"^      url: .*$", f"      url: {json.dumps(source_url)}", "Solar source URL"),
        (r"^      revision: .*$", f"      revision: {revision}", "Solar source revision"),
        (r"^      path: .*$", f"      path: {json.dumps(str(binary))}", "Solar artifact path"),
        (r"^      sha256: .*$", f"      sha256: {file_sha256(binary)}", "Solar artifact digest"),
    ]
    for pattern, replacement, description in replacements:
        server = replace_once(server, pattern, replacement, description)
    return lock[:start] + server + lock[end:]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--servers-lock", type=Path, required=True)
    parser.add_argument("--solar-binary", type=Path, required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--source-url", required=True)
    parser.add_argument("--role", choices=("baseline", "candidate"), required=True)
    parser.add_argument("--output-config", type=Path, required=True)
    parser.add_argument("--output-servers-lock", type=Path, required=True)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if not re.fullmatch(r"[0-9a-fA-F]{40}", args.revision):
        raise ValueError("revision must be a full 40-character Git commit")
    if not args.source_url or any(character in args.source_url for character in "\r\n"):
        raise ValueError("source URL must be a non-empty single-line value")

    config = args.config.resolve()
    servers_lock = args.servers_lock.resolve()
    binary = args.solar_binary.resolve()
    output_config = args.output_config.resolve()
    output_servers_lock = args.output_servers_lock.resolve()
    if not binary.is_file():
        raise ValueError(f"Solar binary does not exist: {binary}")
    if output_config.parent.resolve() != config.parent.resolve():
        raise ValueError("output config must remain beside the source config")
    if output_servers_lock.parent.resolve() != config.parent.resolve():
        raise ValueError("output server lock must remain beside the source config")
    protected = (config, servers_lock, binary)
    if any(paths_alias(output, source) for output in (output_config, output_servers_lock) for source in protected):
        raise ValueError("generated files must not overwrite source manifests or the Solar binary")
    if paths_alias(output_config, output_servers_lock):
        raise ValueError("generated config and server lock must be different files")

    config_text = config.read_text(encoding="utf-8")
    config_text = replace_once(
        config_text,
        r"^servers_lock: .*$",
        f"servers_lock: {json.dumps(output_servers_lock.name)}",
        "server lock reference",
    )
    lock_text = update_solar_server(
        servers_lock.read_text(encoding="utf-8"),
        binary,
        args.revision.lower(),
        args.source_url,
        args.role,
    )
    write_text_atomic(output_config, config_text)
    write_text_atomic(output_servers_lock, lock_text)


if __name__ == "__main__":
    main()
