// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

library GuardLib {
    function bump(uint256[] storage a, uint256 v) external returns (uint256) {
        a.push(v);
        return a.length;
    }

    function peek(uint256[] storage a) external view returns (uint256) {
        return a.length;
    }

    function twice(uint256 x) external pure returns (uint256) {
        return 2 * x;
    }
}

contract GuardUser {
    uint256[] internal values;

    // The library sees this call's value through `DELEGATECALL` and must accept it.
    function bump(uint256 v) external payable returns (uint256) {
        return GuardLib.bump(values, v);
    }

    function peek() external view returns (uint256) {
        return GuardLib.peek(values);
    }
}
