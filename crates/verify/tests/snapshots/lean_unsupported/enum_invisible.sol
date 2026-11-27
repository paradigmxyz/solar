// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract Other {
    enum Color { Red, Green, Blue }
    Color c;
}
contract C {
    uint8 n;
    // expect: enum `Color` is declared in `Other`, not visible in `C`
    /// @custom:ensures n != Color.Blue
    function f() external { n = 1; }
}
