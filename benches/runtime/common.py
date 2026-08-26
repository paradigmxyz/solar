"""Shared process and RPC helpers for the codegen benchmark runners."""

from __future__ import annotations

import gzip
import json
import os
import posixpath
import re
import shlex
import signal
import subprocess
import sys
import tempfile
import time
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import Any

REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
TESTDATA_ROOT = REPOSITORY_ROOT / "testdata"
PROJECTS_ROOT = TESTDATA_ROOT / "projects"
RUNTIME_CORPUS_ROOT = Path(__file__).resolve().parent
DEFAULT_RPC_URL = "http://127.0.0.1:8545"
DEFAULT_PRIVATE_KEY = (
    "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
)

IMPORT_RE = re.compile(r"""\bimport\s+(?:(?:[^;]*?)\s+from\s+)?["']([^"']+)["']\s*;""")


@dataclass(frozen=True)
class CommandResult:
    returncode: int
    stdout: str
    stderr: str
    peak_rss_bytes: int | None = None


def _read_peak_rss(path: Path) -> int | None:
    try:
        peak_rss_kib = int(path.read_text().strip())
    except (OSError, ValueError):
        return None
    return peak_rss_kib * 1024


def run(
    cmd: Sequence[str],
    input_text: str | None = None,
    timeout: int = 120,
    cwd: Path | None = None,
    measure_peak_rss: bool = False,
    input_path: Path | None = None,
) -> CommandResult:
    if input_text is not None and input_path is not None:
        raise ValueError("input_text and input_path are mutually exclusive")

    start = time.monotonic()
    peak_rss_path = None
    run_cmd = list(cmd)
    if measure_peak_rss and sys.platform.startswith("linux"):
        time_binary = Path("/usr/bin/time")
        if time_binary.is_file():
            fd, raw_path = tempfile.mkstemp(prefix="solar-bench-rss-")
            os.close(fd)
            peak_rss_path = Path(raw_path)
            run_cmd = [str(time_binary), "-q", "-f", "%M", "-o", raw_path, "--", *cmd]

    kwargs = {}
    if os.name != "nt":
        kwargs["start_new_session"] = True
    input_file = input_path.open() if input_path is not None else None
    stdin = input_file
    if stdin is None and input_text is not None:
        stdin = subprocess.PIPE
    try:
        try:
            proc = subprocess.Popen(
                run_cmd,
                stdin=stdin,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                cwd=cwd,
                **kwargs,
            )
        except OSError as exc:
            if peak_rss_path is not None:
                peak_rss_path.unlink(missing_ok=True)
            return CommandResult(-1, "", f"failed to run {cmd[0]}: {exc}")
    finally:
        if input_file is not None:
            input_file.close()
    try:
        stdout, stderr = proc.communicate(input=input_text, timeout=timeout)
    except subprocess.TimeoutExpired:
        if os.name != "nt":
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except OSError:
                proc.kill()
        else:
            proc.kill()
        try:
            stdout, stderr = proc.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            stdout, stderr = proc.communicate()
        elapsed = time.monotonic() - start
        stderr = (stderr or "").strip()
        message = (
            f"TIMEOUT after {elapsed:.1f}s: {shlex.join(str(part) for part in cmd)}"
        )
        if stderr:
            message = f"{message}\n{stderr}"
        result = CommandResult(-1, stdout or "", message)
    else:
        result = CommandResult(proc.returncode, stdout, stderr)

    if peak_rss_path is not None:
        peak_rss_bytes = _read_peak_rss(peak_rss_path)
        peak_rss_path.unlink(missing_ok=True)
        result = CommandResult(
            result.returncode, result.stdout, result.stderr, peak_rss_bytes
        )
    return result


def parse_receipt_int(value: object) -> int | None:
    if isinstance(value, str):
        return int(value, 16) if value.startswith("0x") else int(value)
    if value is None:
        return None
    return int(value)


def remappings(settings: dict[str, Any]) -> list[tuple[str, str]]:
    parsed = []
    for remapping in settings.get("remappings", ()):
        prefix, target = remapping.split("=", 1)
        if ":" in prefix:
            _, prefix = prefix.rsplit(":", 1)
        parsed.append((prefix, target))
    parsed.sort(key=lambda item: len(item[0]), reverse=True)
    return parsed


def resolve_import(
    current: str,
    imported: str,
    sources: dict[str, Any],
    mappings: Sequence[tuple[str, str]],
) -> str:
    if imported.startswith("."):
        candidates = [posixpath.join(posixpath.dirname(current), imported)]
    else:
        candidates = [
            target + imported[len(prefix) :]
            for prefix, target in mappings
            if imported.startswith(prefix)
        ]
        candidates.append(imported)

    for candidate in candidates:
        normalized = posixpath.normpath(candidate)
        if normalized in sources:
            return normalized
    raise ValueError(f"cannot resolve import `{imported}` from `{current}`")


def project_slice(project: dict[str, Any], source: str) -> dict[str, Any]:
    sources = project["sources"]
    mappings = remappings(project.get("settings", {}))
    pending = [source]
    selected = set()

    while pending:
        current = pending.pop()
        if current in selected:
            continue
        if current not in sources:
            raise ValueError(f"project does not contain source `{current}`")
        selected.add(current)
        content = sources[current].get("content")
        if not isinstance(content, str):
            raise ValueError(f"project source `{current}` has no inline content")
        pending.extend(
            resolve_import(current, imported, sources, mappings)
            for imported in IMPORT_RE.findall(content)
        )

    return {name: sources[name] for name in sorted(selected)}


def load_project(path: Path) -> dict[str, Any]:
    with gzip.open(path, mode="rt", encoding="utf-8") as file:
        return json.load(file)


def stop_anvil(process: subprocess.Popen[bytes]) -> None:
    process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
