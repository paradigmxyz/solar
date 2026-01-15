// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Inheritance.sol";

contract InheritanceTest {
    Child public c;

    function setUp() public {
        c = new Child();
    }

    function test_InheritedStorage() public {
        c.setParent(42);
        assert(c.parentValue() == 42);
    }

    function test_ChildStorage() public {
        c.setChild(100);
        assert(c.childValue() == 100);
    }

    function test_SetBoth() public {
        c.setBoth(1, 2);
        assert(c.parentValue() == 1);
        assert(c.childValue() == 2);
    }

    function test_InheritedFunction() public {
        c.setParent(77);
        assert(c.parentValue() == 77);
    }

    function test_StorageLayout() public {
        c.setBoth(111, 222);
        assert(c.parentValue() == 111);
        assert(c.childValue() == 222);
    }
}
