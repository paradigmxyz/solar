//@ compile-flags: --emit=abi

// SPDX-License-Identifier: MIT
pragma solidity >=0.8.0;

// Each access adds an Index and a Member node, reaching depth 1025.
contract C {
    struct Node { Node[] children; }
    function f(Node memory x) internal pure returns (Node memory) {
        return
        x.children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0] //~ ERROR: recursion limit reached
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0]
        .children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0].children[0];
    }
}
