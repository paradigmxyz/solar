// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract C {
    struct S {
        uint256 a;
    }

    function(uint256[][] storage) internal f1;
    function(S[] storage) internal f2;
    function(S[2][] storage) internal returns (uint256[][] storage) f3;
    function(uint256[][] memory) internal f4;
}
