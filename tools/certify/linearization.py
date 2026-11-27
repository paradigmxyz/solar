#!/usr/bin/env python3
"""Prints the C3 theorem generic.frontend appends to a contract's Source.lean (linearization_lean),
or nothing for a contract without bases, for source_bind.sh to append to the frontend's fresh output.

    linearization.py SOURCE.sol CONTRACT SOLC NAMESPACE
"""

import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import generic

if len(sys.argv) != 5:
    sys.exit("usage: linearization.py SOURCE.sol CONTRACT SOLC NAMESPACE")
sol, contract, solc, ns = sys.argv[1:]
sys.stdout.write(generic.linearization_lean(ns, contract, solc, pathlib.Path(sol)))
