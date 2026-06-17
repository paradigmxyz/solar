// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/StressFunctions.sol";

contract StressFunctionsTest {
    StressFunctions sf;
    
    function setUp() public {
        sf = new StressFunctions();
    }
    
    // ========== No parameters tests ==========
    function test_NoParams() public view {
        assert(sf.noParams1() == 1);
        assert(sf.noParams2() == 2);
        assert(sf.noParams3() == 3);
        assert(sf.noParams4() == 4);
        assert(sf.noParams5() == 5);
    }
    
    // ========== Single uint256 tests ==========
    function test_SingleUint() public view {
        assert(sf.singleUint1(100) == 100);
        assert(sf.singleUint2(100) == 101);
        assert(sf.singleUint3(100) == 200);
        assert(sf.singleUint4(100) == 50);
        assert(sf.singleUint5(123) == 3);
    }
    
    // ========== Two uint256 tests ==========
    function test_TwoUints() public view {
        assert(sf.twoUints1(10, 20) == 30);
        assert(sf.twoUints2(20, 10) == 10);
        assert(sf.twoUints3(5, 6) == 30);
        assert(sf.twoUints4(10, 20) == 20);
        assert(sf.twoUints5(10, 20) == 10);
    }
    
    // ========== Three uint256 tests ==========
    function test_ThreeUints() public view {
        assert(sf.threeUints1(1, 2, 3) == 6);
        assert(sf.threeUints2(2, 3, 4) == 10);
        assert(sf.threeUints3(2, 3, 4) == 20);
        assert(sf.threeUints5(10, 20, 5) == 25);
    }
    
    // ========== Address tests ==========
    function test_AddressFunctions() public view {
        address a = address(0x1234);
        address b = address(0x5678);
        
        assert(sf.singleAddress(a) == a);
        assert(sf.addressToUint(a) == uint256(uint160(a)));
        assert(sf.twoAddresses(a, a) == true);
        assert(sf.twoAddresses(a, b) == false);
    }
    
    // ========== Boolean tests ==========
    function test_BoolFunctions() public view {
        assert(sf.singleBool(true) == true);
        assert(sf.singleBool(false) == false);
        assert(sf.notBool(true) == false);
        assert(sf.notBool(false) == true);
        assert(sf.twoBools(true, true) == true);
        assert(sf.twoBools(true, false) == false);
        assert(sf.orBools(true, false) == true);
        assert(sf.orBools(false, false) == false);
        assert(sf.xorBools(true, false) == true);
        assert(sf.xorBools(true, true) == false);
    }
    
    // ========== Bytes32 tests ==========
    function test_Bytes32Functions() public view {
        bytes32 a = bytes32(uint256(0xFF00));
        bytes32 b = bytes32(uint256(0x00FF));
        
        assert(sf.singleBytes32(a) == a);
        assert(sf.xorBytes32(a, b) == bytes32(uint256(0xFFFF)));
        assert(sf.andBytes32(a, b) == bytes32(0));
        assert(sf.orBytes32(a, b) == bytes32(uint256(0xFFFF)));
    }
    
    // ========== Mixed parameter tests ==========
    function test_MixedParams() public view {
        assert(sf.mixUintBool(100, true) == 100);
        assert(sf.mixUintBool(100, false) == 0);
        assert(sf.mixBoolUint(true, 100) == 100);
        assert(sf.mixBoolUint(false, 100) == type(uint256).max);
    }
    
    // ========== Multiple return values tests ==========
    function test_MultipleReturns() public view {
        (uint256 a, uint256 b) = sf.returnTwo();
        assert(a == 1 && b == 2);
        
        (uint256 x, uint256 y, uint256 z) = sf.returnThree();
        assert(x == 1 && y == 2 && z == 3);
        
        (uint256 p, address q, bool r) = sf.returnMixed();
        assert(p == 42 && q == address(0) && r == true);
    }
    
    function test_ReturnComputed() public view {
        (uint256 sum, uint256 diff, uint256 prod) = sf.returnComputed(10, 3);
        assert(sum == 13);
        assert(diff == 7);
        assert(prod == 30);
    }
    
    // ========== Smaller integer type tests ==========
    function test_SmallIntTypes() public view {
        assert(sf.uint8Param(10) == 11);
        assert(sf.uint16Param(100) == 101);
        assert(sf.uint32Param(1000) == 1001);
        assert(sf.uint64Param(10000) == 10001);
        assert(sf.uint128Param(100000) == 100001);
    }
    
    // ========== Signed integer tests ==========
    function test_SignedIntegers() public view {
        assert(sf.int256Param(10) == 11);
        assert(sf.int256Param(-10) == -9);
        assert(sf.int256Negate(10) == -10);
        assert(sf.int256Negate(-10) == 10);
        assert(sf.int256Abs(10) == 10);
        assert(sf.int256Abs(-10) == 10);
        assert(sf.twoInt256(5, -3) == 2);
        assert(sf.int256Compare(10, -10) == 10);
    }
    
    // ========== State-modifying tests ==========
    function test_Counter() public {
        assert(sf.getCounter() == 0);
        
        sf.increment();
        assert(sf.getCounter() == 1);
        
        sf.increment();
        sf.increment();
        assert(sf.getCounter() == 3);
        
        sf.decrement();
        assert(sf.getCounter() == 2);
        
        sf.addToCounter(8);
        assert(sf.getCounter() == 10);
        
        sf.multiplyCounter(5);
        assert(sf.getCounter() == 50);
        
        sf.resetCounter();
        assert(sf.getCounter() == 0);
    }
    
    // ========== View functions with storage tests ==========
    function test_CounterViews() public {
        sf.addToCounter(10);
        
        assert(sf.getCounter() == 10);
        assert(sf.getCounterPlusOne() == 11);
        assert(sf.getCounterTimesTwo() == 20);
        assert(sf.isCounterZero() == false);
        assert(sf.isCounterGreaterThan(5) == true);
        assert(sf.isCounterGreaterThan(15) == false);
    }
    
    // ========== Complex computation tests ==========
    function test_Fibonacci() public view {
        assert(sf.fibonacci(0) == 0);
        assert(sf.fibonacci(1) == 1);
        assert(sf.fibonacci(2) == 1);
        assert(sf.fibonacci(3) == 2);
        assert(sf.fibonacci(4) == 3);
        assert(sf.fibonacci(5) == 5);
        assert(sf.fibonacci(10) == 55);
    }
    
    function test_Factorial() public view {
        assert(sf.factorial(0) == 1);
        assert(sf.factorial(1) == 1);
        assert(sf.factorial(2) == 2);
        assert(sf.factorial(3) == 6);
        assert(sf.factorial(4) == 24);
        assert(sf.factorial(5) == 120);
    }
    
    function test_IsPrime() public view {
        assert(sf.isPrime(0) == false);
        assert(sf.isPrime(1) == false);
        assert(sf.isPrime(2) == true);
        assert(sf.isPrime(3) == true);
        assert(sf.isPrime(4) == false);
        assert(sf.isPrime(5) == true);
        assert(sf.isPrime(17) == true);
        assert(sf.isPrime(18) == false);
    }
    
    function test_GCD() public view {
        assert(sf.gcd(12, 8) == 4);
        assert(sf.gcd(17, 13) == 1);
        assert(sf.gcd(100, 25) == 25);
        assert(sf.gcd(48, 18) == 6);
    }
    
    function test_Power() public view {
        assert(sf.power(2, 0) == 1);
        assert(sf.power(2, 1) == 2);
        assert(sf.power(2, 10) == 1024);
        assert(sf.power(3, 4) == 81);
    }
    
    // ========== Memory array tests ==========
    function test_SumArray() public view {
        uint256[] memory arr = new uint256[](4);
        arr[0] = 1;
        arr[1] = 2;
        arr[2] = 3;
        arr[3] = 4;
        assert(sf.sumArray(arr) == 10);
    }
    
    function test_MaxArray() public view {
        uint256[] memory arr = new uint256[](4);
        arr[0] = 5;
        arr[1] = 2;
        arr[2] = 8;
        arr[3] = 3;
        assert(sf.maxArray(arr) == 8);
    }
    
    function test_MinArray() public view {
        uint256[] memory arr = new uint256[](4);
        arr[0] = 5;
        arr[1] = 2;
        arr[2] = 8;
        arr[3] = 3;
        assert(sf.minArray(arr) == 2);
    }
    
    function test_AvgArray() public view {
        uint256[] memory arr = new uint256[](4);
        arr[0] = 10;
        arr[1] = 20;
        arr[2] = 30;
        arr[3] = 40;
        assert(sf.avgArray(arr) == 25);
    }
    
    function test_CountArray() public view {
        uint256[] memory arr = new uint256[](5);
        assert(sf.countArray(arr) == 5);
    }
}
