#!/usr/bin/env python3
"""Writes Counter's mutants: one edit to its code (the spec must then fail) or to its specs."""

import pathlib
import sys

MUTANTS = {
    # Code.
    "src_increment_add2": ("        count += 1;", "        count += 2;"),
    "src_bump_le": ("i < n; i++", "i <= n; i++"),
    "src_bump_drop_require": ('        require(count + n <= limit, "over");\n', ""),
    "src_max_returns_a": ("        return b;\n", "        return a;\n"),
    "src_ctor_limit_plus1": ("        limit = _limit;", "        limit = _limit + 1;"),
    # Specs.
    "spec_increment_plus2": (
        "/// @custom:ensures count == old(count) + 1",
        "/// @custom:ensures count == old(count) + 2",
    ),
    "spec_invariant_strict": (
        "/// @custom:invariant count <= limit",
        "/// @custom:invariant count < limit",
    ),
    "spec_max_gt": (
        "/// @custom:ensures result >= a && result >= b",
        "/// @custom:ensures result > a && result >= b",
    ),
    "spec_increment_drop_requires": ("    /// @custom:requires count < limit\n", ""),
    "spec_loop_invariant_off": (
        "/// @custom:invariant count == old(count) + i",
        "/// @custom:invariant count == old(count) + i + 1",
    ),
}


def main():
    source = pathlib.Path(sys.argv[1]).read_text()
    out = pathlib.Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    for name, (old, new) in MUTANTS.items():
        if source.count(old) != 1:
            raise SystemExit(f"error: {name}: {old!r} is not in the source once")
        (out / f"{name}.sol").write_text(source.replace(old, new))


if __name__ == "__main__":
    main()
