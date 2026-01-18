// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Stress test for dynamic and fixed arrays
/// @notice Tests compiler handling of various array patterns

contract StressArrays {
    // ========== Storage arrays ==========
    uint256[] public dynamicUints;
    address[] public dynamicAddresses;
    bool[] public dynamicBools;
    bytes32[] public dynamicBytes32;
    
    // ========== Fixed-size storage arrays ==========
    uint256[5] public fixedUints5;
    uint256[10] public fixedUints10;
    uint256[32] public fixedUints32;
    address[3] public fixedAddresses3;
    bool[8] public fixedBools8;
    bytes32[4] public fixedBytes32_4;
    
    // ========== Nested arrays in storage ==========
    uint256[][] public nestedDynamic;
    uint256[3][] public dynamicOfFixed;
    
    // ========== Struct arrays ==========
    struct Item {
        uint256 id;
        uint256 value;
        bool active;
    }
    
    Item[] public items;
    Item[10] public fixedItems;
    
    // ========== Basic push/pop operations ==========
    
    function pushUint(uint256 value) public {
        dynamicUints.push(value);
    }
    
    function popUint() public {
        dynamicUints.pop();
    }
    
    function pushAddress(address value) public {
        dynamicAddresses.push(value);
    }
    
    function popAddress() public {
        dynamicAddresses.pop();
    }
    
    function pushBool(bool value) public {
        dynamicBools.push(value);
    }
    
    function popBool() public {
        dynamicBools.pop();
    }
    
    function pushBytes32(bytes32 value) public {
        dynamicBytes32.push(value);
    }
    
    // ========== Multiple push operations ==========
    
    function pushMultipleUints(uint256 a, uint256 b, uint256 c) public {
        dynamicUints.push(a);
        dynamicUints.push(b);
        dynamicUints.push(c);
    }
    
    function pushMultipleAddresses(address a, address b) public {
        dynamicAddresses.push(a);
        dynamicAddresses.push(b);
    }
    
    // ========== Length operations ==========
    
    function getUintsLength() public view returns (uint256) {
        return dynamicUints.length;
    }
    
    function getAddressesLength() public view returns (uint256) {
        return dynamicAddresses.length;
    }
    
    function getBoolsLength() public view returns (uint256) {
        return dynamicBools.length;
    }
    
    function getBytes32Length() public view returns (uint256) {
        return dynamicBytes32.length;
    }
    
    // ========== Index access ==========
    
    function getUintAt(uint256 index) public view returns (uint256) {
        return dynamicUints[index];
    }
    
    function setUintAt(uint256 index, uint256 value) public {
        dynamicUints[index] = value;
    }
    
    function getAddressAt(uint256 index) public view returns (address) {
        return dynamicAddresses[index];
    }
    
    function setAddressAt(uint256 index, address value) public {
        dynamicAddresses[index] = value;
    }
    
    // ========== Fixed array operations ==========
    
    function setFixedUint5(uint256 index, uint256 value) public {
        require(index < 5, "Index out of bounds");
        fixedUints5[index] = value;
    }
    
    function getFixedUint5(uint256 index) public view returns (uint256) {
        return fixedUints5[index];
    }
    
    function setAllFixedUint5(uint256 a, uint256 b, uint256 c, uint256 d, uint256 e) public {
        fixedUints5[0] = a;
        fixedUints5[1] = b;
        fixedUints5[2] = c;
        fixedUints5[3] = d;
        fixedUints5[4] = e;
    }
    
    function sumFixedUint5() public view returns (uint256) {
        return fixedUints5[0] + fixedUints5[1] + fixedUints5[2] + fixedUints5[3] + fixedUints5[4];
    }
    
    function setFixedUint10(uint256 index, uint256 value) public {
        require(index < 10, "Index out of bounds");
        fixedUints10[index] = value;
    }
    
    function getFixedUint10(uint256 index) public view returns (uint256) {
        return fixedUints10[index];
    }
    
    function setFixedAddress3(uint256 index, address value) public {
        require(index < 3, "Index out of bounds");
        fixedAddresses3[index] = value;
    }
    
    function getFixedAddress3(uint256 index) public view returns (address) {
        return fixedAddresses3[index];
    }
    
    function setFixedBool8(uint256 index, bool value) public {
        require(index < 8, "Index out of bounds");
        fixedBools8[index] = value;
    }
    
    function getFixedBool8(uint256 index) public view returns (bool) {
        return fixedBools8[index];
    }
    
    // ========== Nested array operations ==========
    
    function addNestedArray() public {
        nestedDynamic.push();
    }
    
    function pushToNested(uint256 outerIndex, uint256 value) public {
        nestedDynamic[outerIndex].push(value);
    }
    
    function getNestedValue(uint256 outerIndex, uint256 innerIndex) public view returns (uint256) {
        return nestedDynamic[outerIndex][innerIndex];
    }
    
    function getNestedLength(uint256 outerIndex) public view returns (uint256) {
        return nestedDynamic[outerIndex].length;
    }
    
    function getNestedOuterLength() public view returns (uint256) {
        return nestedDynamic.length;
    }
    
    // ========== Dynamic of fixed array operations ==========
    
    function addDynamicOfFixed(uint256 a, uint256 b, uint256 c) public {
        dynamicOfFixed.push([a, b, c]);
    }
    
    function getDynamicOfFixed(uint256 outerIndex, uint256 innerIndex) public view returns (uint256) {
        return dynamicOfFixed[outerIndex][innerIndex];
    }
    
    function getDynamicOfFixedLength() public view returns (uint256) {
        return dynamicOfFixed.length;
    }
    
    // ========== Struct array operations ==========
    
    function addItem(uint256 id, uint256 value, bool active) public {
        items.push(Item(id, value, active));
    }
    
    function getItem(uint256 index) public view returns (uint256, uint256, bool) {
        Item memory item = items[index];
        return (item.id, item.value, item.active);
    }
    
    function getItemsLength() public view returns (uint256) {
        return items.length;
    }
    
    function setFixedItem(uint256 index, uint256 id, uint256 value, bool active) public {
        require(index < 10, "Index out of bounds");
        fixedItems[index] = Item(id, value, active);
    }
    
    function getFixedItem(uint256 index) public view returns (uint256, uint256, bool) {
        Item memory item = fixedItems[index];
        return (item.id, item.value, item.active);
    }
    
    function updateItemValue(uint256 index, uint256 newValue) public {
        items[index].value = newValue;
    }
    
    function setItemActive(uint256 index, bool active) public {
        items[index].active = active;
    }
    
    // ========== Memory array operations ==========
    
    function createMemoryArray(uint256 size) public pure returns (uint256[] memory) {
        uint256[] memory arr = new uint256[](size);
        for (uint256 i = 0; i < size; i++) {
            arr[i] = i * 10;
        }
        return arr;
    }
    
    function sumMemoryArray(uint256[] memory arr) public pure returns (uint256) {
        uint256 sum = 0;
        for (uint256 i = 0; i < arr.length; i++) {
            sum += arr[i];
        }
        return sum;
    }
    
    function reverseArray(uint256[] memory arr) public pure returns (uint256[] memory) {
        uint256[] memory reversed = new uint256[](arr.length);
        for (uint256 i = 0; i < arr.length; i++) {
            reversed[i] = arr[arr.length - 1 - i];
        }
        return reversed;
    }
    
    function filterGreaterThan(uint256[] memory arr, uint256 threshold) public pure returns (uint256) {
        uint256 count = 0;
        for (uint256 i = 0; i < arr.length; i++) {
            if (arr[i] > threshold) {
                count++;
            }
        }
        return count;
    }
    
    // ========== Copy between memory and storage ==========
    
    function copyFromMemory(uint256[] memory arr) public {
        delete dynamicUints;
        for (uint256 i = 0; i < arr.length; i++) {
            dynamicUints.push(arr[i]);
        }
    }
    
    function copyToMemory() public view returns (uint256[] memory) {
        uint256[] memory arr = new uint256[](dynamicUints.length);
        for (uint256 i = 0; i < dynamicUints.length; i++) {
            arr[i] = dynamicUints[i];
        }
        return arr;
    }
    
    // ========== Delete/clear operations ==========
    
    function clearDynamicUints() public {
        delete dynamicUints;
    }
    
    function clearDynamicAddresses() public {
        delete dynamicAddresses;
    }
    
    function resetFixedUints5() public {
        delete fixedUints5;
    }
    
    // ========== Complex operations ==========
    
    function findMaxInStorage() public view returns (uint256) {
        require(dynamicUints.length > 0, "Array empty");
        uint256 max = dynamicUints[0];
        for (uint256 i = 1; i < dynamicUints.length; i++) {
            if (dynamicUints[i] > max) {
                max = dynamicUints[i];
            }
        }
        return max;
    }
    
    function findMinInStorage() public view returns (uint256) {
        require(dynamicUints.length > 0, "Array empty");
        uint256 min = dynamicUints[0];
        for (uint256 i = 1; i < dynamicUints.length; i++) {
            if (dynamicUints[i] < min) {
                min = dynamicUints[i];
            }
        }
        return min;
    }
    
    function sumStorage() public view returns (uint256) {
        uint256 sum = 0;
        for (uint256 i = 0; i < dynamicUints.length; i++) {
            sum += dynamicUints[i];
        }
        return sum;
    }
    
    function countActiveItems() public view returns (uint256) {
        uint256 count = 0;
        for (uint256 i = 0; i < items.length; i++) {
            if (items[i].active) {
                count++;
            }
        }
        return count;
    }
    
    function getTotalItemValue() public view returns (uint256) {
        uint256 total = 0;
        for (uint256 i = 0; i < items.length; i++) {
            if (items[i].active) {
                total += items[i].value;
            }
        }
        return total;
    }
}
