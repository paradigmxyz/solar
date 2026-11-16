// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
contract C {
    string name;
    uint256 x;
    // expect: a spec reads `opaque` as a whole
    /// @custom:ensures name == name
    function f() external {}
}
