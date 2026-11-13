#!/usr/bin/env python3
"""Writes Token's mutants: one edit to its code (the spec must then fail) or to its specs."""

import pathlib
import sys

MUTANTS = {
    # Code.
    "src_mint_supply_plus1": ("        totalSupply += amount;", "        totalSupply += amount + 1;"),
    "src_transfer_keeps_sender": ("        balanceOf[msg.sender] = bal - amount;", "        balanceOf[msg.sender] = bal;"),
    "src_transfer_to_double": (
        "        balanceOf[msg.sender] = bal - amount;\n        balanceOf[to] += amount;",
        "        balanceOf[msg.sender] = bal - amount;\n        balanceOf[to] += amount + amount;",
    ),
    "src_burn_half": ("        balanceOf[msg.sender] -= amount;", "        balanceOf[msg.sender] -= amount / 2;"),
    "src_burn_keeps_supply": ("        totalSupply -= amount;", "        totalSupply -= amount / 2;"),
    "src_ctor_owner_zero": ("        owner = msg.sender;", "        owner = address(0);"),
    # Specs.
    "spec_invariant_plus1": (
        "/// @custom:invariant totalSupply == sumBalances",
        "/// @custom:invariant totalSupply == sumBalances + 1",
    ),
    "spec_mint_to_plus1": (
        "/// @custom:ensures balanceOf[to] == old(balanceOf[to]) + amount\n",
        "/// @custom:ensures balanceOf[to] == old(balanceOf[to]) + amount + 1\n",
    ),
    "spec_transfer_supply_changes": (
        "/// @custom:ensures totalSupply == old(totalSupply)\n",
        "/// @custom:ensures totalSupply == old(totalSupply) + 1\n",
    ),
    "spec_transfer_self": (
        "/// @custom:ensures msg.sender != to ==> balanceOf[msg.sender] ==",
        "/// @custom:ensures balanceOf[msg.sender] ==",
    ),
    "spec_transfer_frame_to": (
        "forall address a :: a != msg.sender && a != to ==>",
        "forall address a :: a != msg.sender ==>",
    ),
    "spec_burn_plus": (
        "/// @custom:ensures balanceOf[msg.sender] == old(balanceOf[msg.sender]) - amount\n",
        "/// @custom:ensures balanceOf[msg.sender] == old(balanceOf[msg.sender]) - amount + 1\n",
    ),
    "spec_burn_ghost_dropped": ("        /// @custom:ghost sumBalances = sumBalances - amount\n", ""),
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
