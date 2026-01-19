// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/ConstructorArgs.sol";

contract ConstructorArgsTest {
    ConstructorArgs public c;
    
    uint256 constant TEST_VALUE = 12345;
    address constant TEST_OWNER = address(0xBEEF);

    function setUp() public {
        c = new ConstructorArgs(TEST_VALUE, TEST_OWNER);
    }

    function test_ValueSet() public view {
        assert(c.value() == TEST_VALUE);
    }

    function test_OwnerSet() public view {
        assert(c.owner() == TEST_OWNER);
    }

    function test_GetValue() public view {
        assert(c.getValue() == TEST_VALUE);
    }

    function test_GetOwner() public view {
        assert(c.getOwner() == TEST_OWNER);
    }
}
