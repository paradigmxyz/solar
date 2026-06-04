// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/MultiReturn.sol";

contract MultiReturnTest {
    MultiReturn public multiReturn;

    function setUp() public {
        multiReturn = new MultiReturn();
    }

    function test_DirectGetTwo() public view {
        (uint256 a, uint256 b) = multiReturn.getTwo();
        assert(a == 1);
        assert(b == 2);
    }

    function test_DirectGetThree() public view {
        (uint256 a, uint256 b, uint256 c) = multiReturn.getThree();
        assert(a == 10);
        assert(b == 20);
        assert(c == 30);
    }

    function test_MultiReturn() public view {
        (uint256 a, uint256 b) = multiReturn.testTwo();
        assert(a == 1);
        assert(b == 2);
    }

    function test_MultiReturnThree() public view {
        (uint256 a, uint256 b, uint256 c) = multiReturn.testThree();
        assert(a == 10);
        assert(b == 20);
        assert(c == 30);
    }

    function test_MultiReturnConditional() public view {
        uint256 b = multiReturn.testPartialCapture();
        assert(b == 2);
    }

    function test_SimpleReturn() public view {
        (uint256 a, uint256 b) = multiReturn.simpleReturn();
        assert(a == 111);
        assert(b == 222);
    }

    function test_TestSimpleReturn() public view {
        (uint256 a, uint256 b) = multiReturn.testSimpleReturn();
        assert(a == 111);
        assert(b == 222);
    }

    function test_MultiReturnViaCaller() public view {
        (uint256 a, uint256 b) = multiReturn.callVia(address(multiReturn));
        assert(a == 1);
        assert(b == 2);
    }
}
