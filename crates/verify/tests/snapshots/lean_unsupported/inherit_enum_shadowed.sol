// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
enum Color { Red, Blue }
// A base's spec names the file-level enum; the derived contract declares its own `Color`, in
// which `Blue` has another index.
contract Base {
    uint8 n;
    // expect: `Color.Blue` means `.lit 1` where the spec is written but not in `Derived`
    /// @custom:ensures n == Color.Blue
    function f() external { n = 1; }
}
contract Derived is Base {
    enum Color { Blue, Red }
}
