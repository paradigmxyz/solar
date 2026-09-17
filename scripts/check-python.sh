#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
node --version
cvc5 --version
uv sync --locked --all-packages
uv run --locked --all-packages ruff format --check .
uv run --locked --all-packages ruff check .
uv run --locked --all-packages ty check tools/compiler-diff fuzz/fandango benches/analyze
uv run --locked --all-packages ty check --extra-search-path benches/lsp benches/lsp
uv run --locked --all-packages ty check --extra-search-path benches/runtime benches/runtime
uv run --locked --all-packages ty check --extra-search-path scripts/pgo --extra-search-path benches/runtime --extra-search-path scripts/evm-rules scripts
uv run --locked --all-packages compiler-diff self-test
uv run --locked --all-packages python -m unittest discover -s fuzz/fandango -p 'test_*.py'
uv run --locked --all-packages python -m unittest discover -s benches/runtime -p 'test_*.py'
uv run --locked --all-packages python -m unittest discover -s benches/lsp -p 'test_*.py'
uv run --locked --all-packages python -m unittest discover -s scripts/evm-rules -p test.py
