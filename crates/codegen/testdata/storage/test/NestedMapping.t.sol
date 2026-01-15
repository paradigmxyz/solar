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
        require(allowance == 1000, "allowance should be 1000");
    }
    
    function test_IndependentAllowances() public {
        nm.setAllowance(ALICE, BOB, 100);
        nm.setAllowance(ALICE, CHARLIE, 200);
        nm.setAllowance(BOB, ALICE, 300);
        
        require(nm.getAllowance(ALICE, BOB) == 100, "ALICE->BOB");
        require(nm.getAllowance(ALICE, CHARLIE) == 200, "ALICE->CHARLIE");
        require(nm.getAllowance(BOB, ALICE) == 300, "BOB->ALICE");
        require(nm.getAllowance(BOB, CHARLIE) == 0, "BOB->CHARLIE should be 0");
    }
    
    function test_IncreaseAllowance() public {
        nm.setAllowance(ALICE, BOB, 100);
        nm.increaseAllowance(ALICE, BOB, 50);
        require(nm.getAllowance(ALICE, BOB) == 150, "should be 150");
    }
    
    function test_MatrixOperations() public {
        nm.setMatrix(0, 0, 1);
        nm.setMatrix(0, 1, 2);
        nm.setMatrix(1, 0, 3);
        nm.setMatrix(1, 1, 4);
        
        require(nm.getMatrix(0, 0) == 1, "m[0][0]");
        require(nm.getMatrix(0, 1) == 2, "m[0][1]");
        require(nm.getMatrix(1, 0) == 3, "m[1][0]");
        require(nm.getMatrix(1, 1) == 4, "m[1][1]");
    }
    
    function test_ThreeLevelPermissions() public {
        nm.setPermission(ALICE, BOB, 1, true);
        nm.setPermission(ALICE, CHARLIE, 1, true);
        
        bool perm1 = nm.hasPermission(ALICE, BOB, 1);
        bool perm2 = nm.hasPermission(ALICE, CHARLIE, 1);
        bool perm3 = nm.hasPermission(BOB, ALICE, 1);
        
        require(perm1, "ALICE-BOB-1");
        require(perm2, "ALICE-CHARLIE-1");
        require(!perm3, "BOB-ALICE-1 default");
    }
    
    function test_ThreeLevelPermissions_FalseValue() public {
        nm.setPermission(ALICE, BOB, 1, true);
        nm.setPermission(ALICE, BOB, 2, false);
        
        bool perm1 = nm.hasPermission(ALICE, BOB, 1);
        bool perm2 = nm.hasPermission(ALICE, BOB, 2);
        
        require(perm1 == true, "ALICE-BOB-1");
        require(perm2 == false, "ALICE-BOB-2");
    }
}
