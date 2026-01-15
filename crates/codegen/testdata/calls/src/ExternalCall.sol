// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface ICallee {
    function add(uint256 a, uint256 b) external pure returns (uint256);
    function multiply(uint256 a, uint256 b) external pure returns (uint256);
}

contract Callee {
    function add(uint256 a, uint256 b) external pure returns (uint256) {
        return a + b;
    }

    function multiply(uint256 a, uint256 b) external pure returns (uint256) {
        return a * b;
    }
}

contract Caller {
    function callAdd(address callee, uint256 a, uint256 b) external view returns (uint256) {
        return ICallee(callee).add(a, b);
    }

    function callMultiply(address callee, uint256 a, uint256 b) external view returns (uint256) {
        return ICallee(callee).multiply(a, b);
    }

    function chainedCalls(address callee, uint256 x) external view returns (uint256) {
        uint256 added = ICallee(callee).add(x, 10);
        uint256 multiplied = ICallee(callee).multiply(added, 2);
        return multiplied;
    }
}
