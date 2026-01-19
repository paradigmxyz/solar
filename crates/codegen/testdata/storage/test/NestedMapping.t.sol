// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/NestedMapping.sol";

contract NestedMappingTest {
    NestedMapping public nm;

    address constant ALICE = address(0x1111);
    address constant BOB = address(0x2222);
    address constant CHARLIE = address(0x3333);

    function setUp() public {
        nm = new NestedMapping();
    }

    function test_SetAndGetAllowance() public {
        nm.setAllowance(ALICE, BOB, 1000);
        uint256 allowance = nm.getAllowance(ALICE, BOB);
        assert(allowance == 1000);
    }

    function test_IndependentAllowances() public {
        nm.setAllowance(ALICE, BOB, 100);
        nm.setAllowance(ALICE, CHARLIE, 200);
        nm.setAllowance(BOB, ALICE, 300);

        assert(nm.getAllowance(ALICE, BOB) == 100);
        assert(nm.getAllowance(ALICE, CHARLIE) == 200);
        assert(nm.getAllowance(BOB, ALICE) == 300);
        assert(nm.getAllowance(BOB, CHARLIE) == 0);
    }

    function test_IncreaseAllowance() public {
        nm.setAllowance(ALICE, BOB, 100);
        nm.increaseAllowance(ALICE, BOB, 50);
        assert(nm.getAllowance(ALICE, BOB) == 150);
    }

    function test_MatrixOperations() public {
        nm.setMatrix(0, 0, 1);
        nm.setMatrix(0, 1, 2);
        nm.setMatrix(1, 0, 3);
        nm.setMatrix(1, 1, 4);

        assert(nm.getMatrix(0, 0) == 1);
        assert(nm.getMatrix(0, 1) == 2);
        assert(nm.getMatrix(1, 0) == 3);
        assert(nm.getMatrix(1, 1) == 4);
    }

    function test_ThreeLevelPermissions() public {
        nm.setPermission(ALICE, BOB, 1, true);
        nm.setPermission(ALICE, CHARLIE, 1, true);

        bool perm1 = nm.hasPermission(ALICE, BOB, 1);
        bool perm2 = nm.hasPermission(ALICE, CHARLIE, 1);
        bool perm3 = nm.hasPermission(BOB, ALICE, 1);

        assert(perm1);
        assert(perm2);
        assert(!perm3);
    }

    function test_ThreeLevelPermissions_FalseValue() public {
        nm.setPermission(ALICE, BOB, 1, true);
        nm.setPermission(ALICE, BOB, 2, false);

        bool perm1 = nm.hasPermission(ALICE, BOB, 1);
        bool perm2 = nm.hasPermission(ALICE, BOB, 2);

        assert(perm1 == true);
        assert(perm2 == false);
    }
}
