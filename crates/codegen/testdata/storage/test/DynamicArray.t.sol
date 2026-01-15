// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/DynamicArray.sol";

contract DynamicArrayTest {
    DynamicArray arr;

    function setUp() public {
        arr = new DynamicArray();
    }

    function test_initialLength() public view {
        assert(arr.length() == 0);
    }

    function test_push() public {
        arr.push(42);
        assert(arr.length() == 1);
        assert(arr.get(0) == 42);
    }

    function test_pushMultiple() public {
        arr.push(10);
        arr.push(20);
        arr.push(30);
        assert(arr.length() == 3);
        assert(arr.get(0) == 10);
        assert(arr.get(1) == 20);
        assert(arr.get(2) == 30);
    }

    function test_pop() public {
        arr.push(100);
        arr.push(200);
        assert(arr.length() == 2);

        arr.pop();
        assert(arr.length() == 1);
        assert(arr.get(0) == 100);
    }

    function test_pushPop() public {
        arr.push(1);
        arr.push(2);
        arr.push(3);
        assert(arr.length() == 3);

        arr.pop();
        assert(arr.length() == 2);

        arr.push(4);
        assert(arr.length() == 3);
        assert(arr.get(2) == 4);
    }

    function test_pushMultipleFunction() public {
        arr.pushMultiple(111, 222, 333);
        assert(arr.length() == 3);
        assert(arr.get(0) == 111);
        assert(arr.get(1) == 222);
        assert(arr.get(2) == 333);
    }
}
