// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Modifiers.sol";

contract ModifiersTest {
    Modifiers modifiers;
    
    function setUp() public {
        modifiers = new Modifiers();
    }
    
    function testOwnerIsSet() public view {
        assert(modifiers.owner() == address(this));
    }
    
    function testSetValueAsOwner() public {
        modifiers.setValue(42);
        assert(modifiers.getValue() == 42);
    }
    
    function testSetValueWithValidValue() public {
        modifiers.setValue(100);
        assert(modifiers.value() == 100);
    }
    
    function testSetValueNonReentrant() public {
        modifiers.setValueNonReentrant(123);
        assert(modifiers.getValue() == 123);
        assert(!modifiers.locked());
    }
    
    function testLockedResetAfterCall() public {
        modifiers.setValueNonReentrant(1);
        assert(!modifiers.locked());
        modifiers.setValueNonReentrant(2);
        assert(modifiers.getValue() == 2);
    }
    
    function testMultipleModifiers() public {
        modifiers.setValue(50);
        assert(modifiers.getValue() == 50);
        modifiers.setValue(75);
        assert(modifiers.getValue() == 75);
    }
}
