// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.20;

contract C {
    // expect: `__returndatasize` (the latest call's return data) is not exported
    /// @custom:ensures ok ==> __returndatasize == 0
    function f(address t) external returns (bool ok) {
        (ok, ) = t.call("");
    }
}
