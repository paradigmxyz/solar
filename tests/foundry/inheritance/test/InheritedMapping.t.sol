// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/InheritedMapping.sol";

contract InheritedMappingTest {
    Child public c;

    function setUp() public {
        c = new Child();
    }

    function test_InheritedMappingStorage() public {
        address user = address(0x1234);
        c.mint(user, 1000);
        
        // Check totalSupply was set correctly
        assert(c.totalSupply() == 1000);
        
        // Check balanceOf was set correctly - this is the bug!
        // The _mint function writes to slot 0 (base contract's perspective)
        // but the getter reads from a different slot (child contract's perspective)
        assert(c.balanceOf(user) == 1000);
    }
}
