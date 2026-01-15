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
        require(result == 8, "direct add should return 8");
    }

    function test_DirectMultiply() public view {
        uint256 result = callee.multiply(7, 6);
        require(result == 42, "direct multiply should return 42");
    }

    function test_ExternalAdd() public view {
        uint256 result = caller.callAdd(address(callee), 5, 3);
        require(result == 8, "external add should return 8");
    }

    function test_ExternalMultiply() public view {
        uint256 result = caller.callMultiply(address(callee), 7, 6);
        require(result == 42, "external multiply should return 42");
    }

    function test_ChainedCalls() public view {
        uint256 result = caller.chainedCalls(address(callee), 5);
        require(result == 30, "chained calls should return 30");
    }
}
