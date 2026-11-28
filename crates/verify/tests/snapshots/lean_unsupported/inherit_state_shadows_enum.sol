// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
enum Color { Red, Blue }
// A base's spec names the file-level enum; the derived contract alone declares a variable
// `Color`, so the inherited spec would read its storage there instead.
contract Base {
    uint8 n;
    // expect: `Color` names no state variable where the spec is written (in `Base`) but a state variable in `Derived`
    /// @custom:ensures n == Color.Blue
    function f() external { n = 1; }
}
contract Derived is Base {
    struct S { uint8 Blue; }
    S Color;
}
