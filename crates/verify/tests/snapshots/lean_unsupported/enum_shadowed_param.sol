// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract C {
    enum Status { Off, On }
    Status s;
    uint256 n;
    // The parameter shadows the enum: `Status.On` reads the parameter, not the enum.
    // expect: member `.On` not exported
    /// @custom:ensures n == 1 && Status.On == Status.On
    function f(uint256 Status) external { n = Status; }
}
