// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
abstract contract B {
    uint8 n;
    // expect: enum `Mode` is declared in `D`, not visible in `B`
    /// @custom:ensures n != Mode.On
    function f() external { n = 0; }
}
contract D is B {
    enum Mode { Off, On }
}
