// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/StressArrays.sol";

contract StressArraysTest {
    StressArrays sa;
    
    function setUp() public {
        sa = new StressArrays();
    }
    
    // ========== Basic push/pop tests ==========
    
    function test_PushUint() public {
        sa.pushUint(100);
        assert(sa.getUintsLength() == 1);
        assert(sa.getUintAt(0) == 100);
    }
    
    function test_PopUint() public {
        sa.pushUint(1);
        sa.pushUint(2);
        sa.popUint();
        assert(sa.getUintsLength() == 1);
    }
    
    function test_PushAddress() public {
        address addr = address(0x1234);
        sa.pushAddress(addr);
        assert(sa.getAddressesLength() == 1);
        assert(sa.getAddressAt(0) == addr);
    }
    
    function test_PushBool() public {
        sa.pushBool(true);
        sa.pushBool(false);
        assert(sa.getBoolsLength() == 2);
    }
    
    function test_PushBytes32() public {
        bytes32 data = keccak256("test");
        sa.pushBytes32(data);
        assert(sa.getBytes32Length() == 1);
    }
    
    // ========== Multiple push tests ==========
    
    function test_PushMultipleUints() public {
        sa.pushMultipleUints(10, 20, 30);
        assert(sa.getUintsLength() == 3);
        assert(sa.getUintAt(0) == 10);
        assert(sa.getUintAt(1) == 20);
        assert(sa.getUintAt(2) == 30);
    }
    
    function test_PushMultipleAddresses() public {
        address a = address(0x1);
        address b = address(0x2);
        sa.pushMultipleAddresses(a, b);
        assert(sa.getAddressesLength() == 2);
        assert(sa.getAddressAt(0) == a);
        assert(sa.getAddressAt(1) == b);
    }
    
    // ========== Index access tests ==========
    
    function test_SetUintAt() public {
        sa.pushUint(0);
        sa.pushUint(0);
        sa.setUintAt(0, 100);
        sa.setUintAt(1, 200);
        assert(sa.getUintAt(0) == 100);
        assert(sa.getUintAt(1) == 200);
    }
    
    function test_SetAddressAt() public {
        sa.pushAddress(address(0));
        address newAddr = address(0xABCD);
        sa.setAddressAt(0, newAddr);
        assert(sa.getAddressAt(0) == newAddr);
    }
    
    // ========== Fixed array tests ==========
    
    function test_FixedUint5() public {
        sa.setFixedUint5(0, 10);
        sa.setFixedUint5(4, 50);
        assert(sa.getFixedUint5(0) == 10);
        assert(sa.getFixedUint5(4) == 50);
    }
    
    function test_SetAllFixedUint5() public {
        sa.setAllFixedUint5(1, 2, 3, 4, 5);
        assert(sa.sumFixedUint5() == 15);
    }
    
    function test_FixedUint10() public {
        sa.setFixedUint10(0, 100);
        sa.setFixedUint10(9, 900);
        assert(sa.getFixedUint10(0) == 100);
        assert(sa.getFixedUint10(9) == 900);
    }
    
    function test_FixedAddress3() public {
        address a = address(0x111);
        address b = address(0x222);
        address c = address(0x333);
        
        sa.setFixedAddress3(0, a);
        sa.setFixedAddress3(1, b);
        sa.setFixedAddress3(2, c);
        
        assert(sa.getFixedAddress3(0) == a);
        assert(sa.getFixedAddress3(1) == b);
        assert(sa.getFixedAddress3(2) == c);
    }
    
    function test_FixedBool8() public {
        sa.setFixedBool8(0, true);
        sa.setFixedBool8(7, true);
        sa.setFixedBool8(3, false);
        
        assert(sa.getFixedBool8(0) == true);
        assert(sa.getFixedBool8(7) == true);
        assert(sa.getFixedBool8(3) == false);
    }
    
    // ========== Nested array tests ==========
    
    function test_NestedArray() public {
        sa.addNestedArray();
        sa.pushToNested(0, 10);
        sa.pushToNested(0, 20);
        sa.pushToNested(0, 30);
        
        assert(sa.getNestedOuterLength() == 1);
        assert(sa.getNestedLength(0) == 3);
        assert(sa.getNestedValue(0, 0) == 10);
        assert(sa.getNestedValue(0, 2) == 30);
    }
    
    function test_MultipleNestedArrays() public {
        sa.addNestedArray();
        sa.addNestedArray();
        
        sa.pushToNested(0, 100);
        sa.pushToNested(1, 200);
        
        assert(sa.getNestedOuterLength() == 2);
        assert(sa.getNestedValue(0, 0) == 100);
        assert(sa.getNestedValue(1, 0) == 200);
    }
    
    // ========== Dynamic of fixed tests ==========
    
    function test_DynamicOfFixed() public {
        sa.addDynamicOfFixed(1, 2, 3);
        sa.addDynamicOfFixed(4, 5, 6);
        
        assert(sa.getDynamicOfFixedLength() == 2);
        assert(sa.getDynamicOfFixed(0, 0) == 1);
        assert(sa.getDynamicOfFixed(0, 2) == 3);
        assert(sa.getDynamicOfFixed(1, 1) == 5);
    }
    
    // ========== Struct array tests ==========
    
    function test_AddItem() public {
        sa.addItem(1, 100, true);
        sa.addItem(2, 200, false);
        
        assert(sa.getItemsLength() == 2);
        
        (uint256 id1, uint256 value1, bool active1) = sa.getItem(0);
        assert(id1 == 1 && value1 == 100 && active1 == true);
        
        (uint256 id2, uint256 value2, bool active2) = sa.getItem(1);
        assert(id2 == 2 && value2 == 200 && active2 == false);
    }
    
    function test_FixedItem() public {
        sa.setFixedItem(0, 10, 1000, true);
        sa.setFixedItem(9, 90, 9000, false);
        
        (uint256 id0, uint256 value0, bool active0) = sa.getFixedItem(0);
        assert(id0 == 10 && value0 == 1000 && active0 == true);
        
        (uint256 id9, uint256 value9, bool active9) = sa.getFixedItem(9);
        assert(id9 == 90 && value9 == 9000 && active9 == false);
    }
    
    function test_UpdateItemValue() public {
        sa.addItem(1, 100, true);
        sa.updateItemValue(0, 999);
        
        (, uint256 value,) = sa.getItem(0);
        assert(value == 999);
    }
    
    function test_SetItemActive() public {
        sa.addItem(1, 100, true);
        sa.setItemActive(0, false);
        
        (,, bool active) = sa.getItem(0);
        assert(active == false);
    }
    
    // ========== Memory array tests ==========
    
    function test_CreateMemoryArray() public view {
        uint256[] memory arr = sa.createMemoryArray(5);
        assert(arr.length == 5);
        assert(arr[0] == 0);
        assert(arr[1] == 10);
        assert(arr[4] == 40);
    }
    
    function test_SumMemoryArray() public view {
        uint256[] memory arr = new uint256[](4);
        arr[0] = 10;
        arr[1] = 20;
        arr[2] = 30;
        arr[3] = 40;
        
        uint256 sum = sa.sumMemoryArray(arr);
        assert(sum == 100);
    }
    
    function test_ReverseArray() public view {
        uint256[] memory arr = new uint256[](3);
        arr[0] = 1;
        arr[1] = 2;
        arr[2] = 3;
        
        uint256[] memory reversed = sa.reverseArray(arr);
        assert(reversed[0] == 3);
        assert(reversed[1] == 2);
        assert(reversed[2] == 1);
    }
    
    function test_FilterGreaterThan() public view {
        uint256[] memory arr = new uint256[](5);
        arr[0] = 10;
        arr[1] = 20;
        arr[2] = 5;
        arr[3] = 30;
        arr[4] = 15;
        
        uint256 count = sa.filterGreaterThan(arr, 10);
        assert(count == 3); // 20, 30, 15
    }
    
    // ========== Copy tests ==========
    
    function test_CopyFromMemory() public {
        uint256[] memory arr = new uint256[](3);
        arr[0] = 100;
        arr[1] = 200;
        arr[2] = 300;
        
        sa.copyFromMemory(arr);
        
        assert(sa.getUintsLength() == 3);
        assert(sa.getUintAt(0) == 100);
        assert(sa.getUintAt(2) == 300);
    }
    
    function test_CopyToMemory() public {
        sa.pushMultipleUints(1, 2, 3);
        
        uint256[] memory arr = sa.copyToMemory();
        
        assert(arr.length == 3);
        assert(arr[0] == 1);
        assert(arr[1] == 2);
        assert(arr[2] == 3);
    }
    
    // ========== Delete/clear tests ==========
    
    function test_ClearDynamicUints() public {
        sa.pushMultipleUints(1, 2, 3);
        sa.clearDynamicUints();
        assert(sa.getUintsLength() == 0);
    }
    
    function test_ResetFixedUints5() public {
        sa.setAllFixedUint5(1, 2, 3, 4, 5);
        sa.resetFixedUints5();
        assert(sa.sumFixedUint5() == 0);
    }
    
    // ========== Complex operation tests ==========
    
    function test_FindMaxInStorage() public {
        sa.pushMultipleUints(50, 100, 25);
        sa.pushUint(75);
        
        uint256 max = sa.findMaxInStorage();
        assert(max == 100);
    }
    
    function test_FindMinInStorage() public {
        sa.pushMultipleUints(50, 100, 25);
        sa.pushUint(75);
        
        uint256 min = sa.findMinInStorage();
        assert(min == 25);
    }
    
    function test_SumStorage() public {
        sa.pushMultipleUints(10, 20, 30);
        
        uint256 sum = sa.sumStorage();
        assert(sum == 60);
    }
    
    function test_CountActiveItems() public {
        sa.addItem(1, 100, true);
        sa.addItem(2, 200, false);
        sa.addItem(3, 300, true);
        sa.addItem(4, 400, true);
        
        uint256 count = sa.countActiveItems();
        assert(count == 3);
    }
    
    function test_GetTotalItemValue() public {
        sa.addItem(1, 100, true);
        sa.addItem(2, 200, false);
        sa.addItem(3, 300, true);
        
        uint256 total = sa.getTotalItemValue();
        assert(total == 400); // Only active items: 100 + 300
    }
}
