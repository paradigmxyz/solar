// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/StorageInit.sol";

contract StorageInitTest {
    StorageInit public s;

    function setUp() public {
        s = new StorageInit();
    }

    function test_ValueInitialized() public view {
        assert(s.value() == 42);
    }

    function test_AnotherValueInitialized() public view {
        assert(s.anotherValue() == 100);
    }

    function test_GetValue() public view {
        assert(s.getValue() == 42);
    }
}
