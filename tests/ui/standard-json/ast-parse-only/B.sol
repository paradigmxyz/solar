// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract B {
    uint public y;

    function f(uint a) internal pure virtual returns (uint) {
        return a * 2;
    }
}
