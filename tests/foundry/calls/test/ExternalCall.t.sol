// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/ExternalCall.sol";

contract ExternalCallTest {
    Callee public callee;
    Caller public caller;

    function setUp() public {
        callee = new Callee();
        caller = new Caller();
    }

    function test_DirectAdd() public view {
        uint256 result = callee.add(5, 3);
        assert(result == 8);
    }

    function test_DirectMultiply() public view {
        uint256 result = callee.multiply(7, 6);
        assert(result == 42);
    }

    function test_ExternalAdd() public view {
        uint256 result = caller.callAdd(address(callee), 5, 3);
        assert(result == 8);
    }

    function test_ExternalMultiply() public view {
        uint256 result = caller.callMultiply(address(callee), 7, 6);
        assert(result == 42);
    }

    function test_ChainedCalls() public view {
        uint256 result = caller.chainedCalls(address(callee), 5);
        assert(result == 30);
    }

    /// @notice Regression test: multiple external calls with assert
    /// This verifies that stack tracking remains correct after multiple CALL instructions.
    function test_MultipleCallsWithAssert() public view {
        uint256 result = caller.multipleCallsWithAssert(address(callee), 10);
        assert(result == 22); // (10+1) + (10+1) = 22
    }

    /// @notice Regression test: three consecutive calls with assertions
    function test_ThreeCallsWithAssert() public view {
        uint256 result = caller.threeCallsWithAssert(address(callee), 7);
        assert(result == 21); // 7 + 7 + 7 = 21
    }
}
