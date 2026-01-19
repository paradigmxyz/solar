// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/SafeMath.sol";

contract SafeMathTest {
    TestLibrary public lib;

    function setUp() public {
        lib = new TestLibrary();
    }

    function test_safeAdd() public view {
        uint256 result = lib.safeAddDirect(1, 2);
        assert(result == 3);
    }

    function test_safeAddZero() public view {
        uint256 result = lib.safeAddDirect(0, 0);
        assert(result == 0);
    }

    function test_safeAddLarge() public view {
        uint256 result = lib.safeAddDirect(100, 200);
        assert(result == 300);
    }

    function test_safeSub() public view {
        uint256 result = lib.safeSubDirect(5, 3);
        assert(result == 2);
    }

    function test_safeSubLarge() public view {
        uint256 result = lib.safeSubDirect(100, 50);
        assert(result == 50);
    }

    function test_safeMul() public view {
        uint256 result = lib.safeMulDirect(3, 4);
        assert(result == 12);
    }

    function test_safeMulZero() public view {
        uint256 result = lib.safeMulDirect(0, 100);
        assert(result == 0);
    }

    function test_chainedOps() public view {
        // (2 + 3) * 4 = 20
        uint256 result = lib.chainedOps(2, 3, 4);
        assert(result == 20);
    }

    function test_chainedOpsLarge() public view {
        // (10 + 5) * 2 = 30
        uint256 result = lib.chainedOps(10, 5, 2);
        assert(result == 30);
    }
}
