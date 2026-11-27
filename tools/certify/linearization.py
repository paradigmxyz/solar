#!/usr/bin/env python3
"""Regenerates Certified/NAME/Linearization.lean (generic.linearization_module) from SOURCE.sol with
SOLC, for tools/certify/linearization_bind.sh to compare with the checked-in module: prints the
module's text, or nothing for a contract without bases, and exits non-zero (with the reason on
stderr) when CONTRACT has bases whose C3 order the driver does not certify, or cannot be read.

    linearization.py SOURCE.sol NAME CONTRACT SOLC
"""

import hashlib
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import generic

if len(sys.argv) != 5:
    sys.exit("usage: linearization.py SOURCE.sol NAME CONTRACT SOLC")
sol, name, contract, solc = sys.argv[1:]
src = pathlib.Path(sol)
sha = hashlib.sha256(src.read_bytes()).hexdigest()
try:
    ast, _ = generic.solc_ast(solc, src)
    text = generic.linearization_module(name, contract, ast, sha)
except generic.Failed as e:
    sys.exit(f"linearization.py: {e}")
except generic.Blocked as e:
    sys.exit(f"linearization.py: {e.reasons}")
sys.stdout.write(text)
