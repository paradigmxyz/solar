#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.14"
# dependencies = ["duckdb==1.5.0"]
# ///
"""Compatibility entry point for tools/compiler-diff."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools/compiler-diff"))

from compiler_diff.cli import entrypoint

entrypoint()
