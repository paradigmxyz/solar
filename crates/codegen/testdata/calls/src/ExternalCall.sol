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

    /// @notice Regression test: multiple external calls with assert
    /// This tests that stack model stays in sync after multiple CALL instructions.
    /// Previously, POPs after CALL weren't reflected in StackScheduler, causing
    /// incorrect DUP depths for subsequent values.
    function multipleCallsWithAssert(address callee, uint256 x) external view returns (uint256) {
        uint256 a = ICallee(callee).add(x, 1);
        uint256 b = ICallee(callee).add(x, 1);
        assert(a == b);
        return a + b;
    }

    /// @notice Three consecutive calls with assertion
    function threeCallsWithAssert(address callee, uint256 x) external view returns (uint256) {
        uint256 a = ICallee(callee).add(x, 0);
        uint256 b = ICallee(callee).add(x, 0);
        uint256 c = ICallee(callee).add(x, 0);
        assert(a == b);
        assert(b == c);
        return a + b + c;
    }
}
