// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract C {
    struct S { uint256 a; }
    S[] xs;
    // expect: arrays of structs are not exported
    /// @custom:ensures xs.length == 0 || xs[0].a >= 0
    function f() external {}
}
