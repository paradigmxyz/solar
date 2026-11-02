//@ compile-flags: --emit=abi

// SPDX-License-Identifier: MIT
pragma solidity >=0.8.0;

// Each call adds a Call and a Member node, reaching depth 1025.
contract C {
    function next() external view returns (C) { return this; }
    function f() external view returns (C) {
        return
        this.next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next() //~ ERROR: recursion limit reached
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next()
        .next().next().next().next().next().next().next().next().next().next().next().next().next().next().next().next();
    }
}
