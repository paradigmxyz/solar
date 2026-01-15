// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Require and revert edge cases
/// @notice Tests for require(), revert(), and error handling
contract Require {

    function requireTrue(bool cond) public pure {
        require(cond);
    }

    function requireWithMessage(bool cond) public pure {
        require(cond, "condition failed");
    }

    function revertAlways() public pure {
        revert();
    }

    function revertWithMessage() public pure {
        revert("always reverts");
    }

    function divideChecked(uint256 a, uint256 b) public pure returns (uint256) {
        require(b != 0, "division by zero");
        return a / b;
    }

    // TODO: requireChain skipped - modulo operator has bugs in require condition
}
