// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Stress test for many functions with various signatures
/// @notice Tests compiler handling of 50+ functions with different parameter types

contract StressFunctions {
    uint256 public counter;
    
    // ========== No parameters ==========
    function noParams1() public pure returns (uint256) { return 1; }
    function noParams2() public pure returns (uint256) { return 2; }
    function noParams3() public pure returns (uint256) { return 3; }
    function noParams4() public pure returns (uint256) { return 4; }
    function noParams5() public pure returns (uint256) { return 5; }
    
    // ========== Single uint256 parameter ==========
    function singleUint1(uint256 a) public pure returns (uint256) { return a; }
    function singleUint2(uint256 a) public pure returns (uint256) { return a + 1; }
    function singleUint3(uint256 a) public pure returns (uint256) { return a * 2; }
    function singleUint4(uint256 a) public pure returns (uint256) { return a / 2; }
    function singleUint5(uint256 a) public pure returns (uint256) { return a % 10; }
    
    // ========== Two uint256 parameters ==========
    function twoUints1(uint256 a, uint256 b) public pure returns (uint256) { return a + b; }
    function twoUints2(uint256 a, uint256 b) public pure returns (uint256) { return a - b; }
    function twoUints3(uint256 a, uint256 b) public pure returns (uint256) { return a * b; }
    function twoUints4(uint256 a, uint256 b) public pure returns (uint256) { return a > b ? a : b; }
    function twoUints5(uint256 a, uint256 b) public pure returns (uint256) { return a < b ? a : b; }
    
    // ========== Three uint256 parameters ==========
    function threeUints1(uint256 a, uint256 b, uint256 c) public pure returns (uint256) { return a + b + c; }
    function threeUints2(uint256 a, uint256 b, uint256 c) public pure returns (uint256) { return a * b + c; }
    function threeUints3(uint256 a, uint256 b, uint256 c) public pure returns (uint256) { return (a + b) * c; }
    function threeUints4(uint256 a, uint256 b, uint256 c) public pure returns (uint256) { return a > b ? (b > c ? b : c) : (a > c ? a : c); }
    function threeUints5(uint256 a, uint256 b, uint256 c) public pure returns (uint256) { return a + b > c ? a + b - c : 0; }
    
    // ========== Address parameters ==========
    function singleAddress(address a) public pure returns (address) { return a; }
    function addressToUint(address a) public pure returns (uint256) { return uint256(uint160(a)); }
    function twoAddresses(address a, address b) public pure returns (bool) { return a == b; }
    function addressCompare(address a, address b) public pure returns (address) { return a > b ? a : b; }
    function threeAddresses(address a, address b, address c) public pure returns (bool) { return a == b || b == c || a == c; }
    
    // ========== Boolean parameters ==========
    function singleBool(bool a) public pure returns (bool) { return a; }
    function notBool(bool a) public pure returns (bool) { return !a; }
    function twoBools(bool a, bool b) public pure returns (bool) { return a && b; }
    function orBools(bool a, bool b) public pure returns (bool) { return a || b; }
    function xorBools(bool a, bool b) public pure returns (bool) { return a != b; }
    
    // ========== Bytes32 parameters ==========
    function singleBytes32(bytes32 a) public pure returns (bytes32) { return a; }
    function xorBytes32(bytes32 a, bytes32 b) public pure returns (bytes32) { return a ^ b; }
    function andBytes32(bytes32 a, bytes32 b) public pure returns (bytes32) { return a & b; }
    function orBytes32(bytes32 a, bytes32 b) public pure returns (bytes32) { return a | b; }
    function notBytes32(bytes32 a) public pure returns (bytes32) { return ~a; }
    
    // ========== Mixed parameters ==========
    function mixUintAddress(uint256 a, address b) public pure returns (uint256) { return a + uint256(uint160(b)); }
    function mixAddressUint(address a, uint256 b) public pure returns (uint256) { return uint256(uint160(a)) + b; }
    function mixUintBool(uint256 a, bool b) public pure returns (uint256) { return b ? a : 0; }
    function mixBoolUint(bool a, uint256 b) public pure returns (uint256) { return a ? b : type(uint256).max; }
    function mixAll(uint256 a, address b, bool c, bytes32 d) public pure returns (uint256) {
        if (!c) return 0;
        return a + uint256(uint160(b)) + uint256(d);
    }
    
    // ========== Multiple return values ==========
    function returnTwo() public pure returns (uint256, uint256) { return (1, 2); }
    function returnThree() public pure returns (uint256, uint256, uint256) { return (1, 2, 3); }
    function returnFour() public pure returns (uint256, uint256, uint256, uint256) { return (1, 2, 3, 4); }
    function returnMixed() public pure returns (uint256, address, bool) { return (42, address(0), true); }
    function returnComputed(uint256 a, uint256 b) public pure returns (uint256, uint256, uint256) {
        return (a + b, a - b, a * b);
    }
    
    // ========== Smaller integer types ==========
    function uint8Param(uint8 a) public pure returns (uint8) { return a + 1; }
    function uint16Param(uint16 a) public pure returns (uint16) { return a + 1; }
    function uint32Param(uint32 a) public pure returns (uint32) { return a + 1; }
    function uint64Param(uint64 a) public pure returns (uint64) { return a + 1; }
    function uint128Param(uint128 a) public pure returns (uint128) { return a + 1; }
    
    // ========== Signed integers ==========
    function int256Param(int256 a) public pure returns (int256) { return a + 1; }
    function int256Negate(int256 a) public pure returns (int256) { return -a; }
    function int256Abs(int256 a) public pure returns (int256) { return a >= 0 ? a : -a; }
    function twoInt256(int256 a, int256 b) public pure returns (int256) { return a + b; }
    function int256Compare(int256 a, int256 b) public pure returns (int256) { return a > b ? a : b; }
    
    // ========== State-modifying functions ==========
    function increment() public { counter++; }
    function decrement() public { counter--; }
    function addToCounter(uint256 a) public { counter += a; }
    function multiplyCounter(uint256 a) public { counter *= a; }
    function resetCounter() public { counter = 0; }
    
    // ========== View functions with storage ==========
    function getCounter() public view returns (uint256) { return counter; }
    function getCounterPlusOne() public view returns (uint256) { return counter + 1; }
    function getCounterTimesTwo() public view returns (uint256) { return counter * 2; }
    function isCounterZero() public view returns (bool) { return counter == 0; }
    function isCounterGreaterThan(uint256 a) public view returns (bool) { return counter > a; }
    
    // ========== Complex computation functions ==========
    function fibonacci(uint256 n) public pure returns (uint256) {
        if (n <= 1) return n;
        uint256 a = 0;
        uint256 b = 1;
        for (uint256 i = 2; i <= n; i++) {
            uint256 c = a + b;
            a = b;
            b = c;
        }
        return b;
    }
    
    function factorial(uint256 n) public pure returns (uint256) {
        uint256 result = 1;
        for (uint256 i = 2; i <= n; i++) {
            result *= i;
        }
        return result;
    }
    
    function isPrime(uint256 n) public pure returns (bool) {
        if (n < 2) return false;
        if (n == 2) return true;
        if (n % 2 == 0) return false;
        for (uint256 i = 3; i * i <= n; i += 2) {
            if (n % i == 0) return false;
        }
        return true;
    }
    
    function gcd(uint256 a, uint256 b) public pure returns (uint256) {
        while (b != 0) {
            uint256 t = b;
            b = a % b;
            a = t;
        }
        return a;
    }
    
    function power(uint256 base, uint256 exp) public pure returns (uint256) {
        uint256 result = 1;
        for (uint256 i = 0; i < exp; i++) {
            result *= base;
        }
        return result;
    }
    
    // ========== Memory array parameters ==========
    function sumArray(uint256[] memory arr) public pure returns (uint256) {
        uint256 sum = 0;
        for (uint256 i = 0; i < arr.length; i++) {
            sum += arr[i];
        }
        return sum;
    }
    
    function maxArray(uint256[] memory arr) public pure returns (uint256) {
        require(arr.length > 0, "Empty array");
        uint256 max = arr[0];
        for (uint256 i = 1; i < arr.length; i++) {
            if (arr[i] > max) max = arr[i];
        }
        return max;
    }
    
    function minArray(uint256[] memory arr) public pure returns (uint256) {
        require(arr.length > 0, "Empty array");
        uint256 min = arr[0];
        for (uint256 i = 1; i < arr.length; i++) {
            if (arr[i] < min) min = arr[i];
        }
        return min;
    }
    
    function avgArray(uint256[] memory arr) public pure returns (uint256) {
        require(arr.length > 0, "Empty array");
        uint256 sum = 0;
        for (uint256 i = 0; i < arr.length; i++) {
            sum += arr[i];
        }
        return sum / arr.length;
    }
    
    function countArray(uint256[] memory arr) public pure returns (uint256) {
        return arr.length;
    }
}
