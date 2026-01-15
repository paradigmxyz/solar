// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/StorageInit.sol";

contract StorageInitTest {
    StorageInit public s;

    function setUp() public {
        s = new StorageInit();
    }

    function test_ValueInitialized() public view {
        require(s.value() == 42, "value should be 42");
    }

    function test_AnotherValueInitialized() public view {
        require(s.anotherValue() == 100, "anotherValue should be 100");
    }

    function test_GetValue() public view {
        require(s.getValue() == 42, "getValue should return 42");
    }
}
