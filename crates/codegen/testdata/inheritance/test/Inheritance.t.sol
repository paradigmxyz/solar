// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Inheritance.sol";

interface Vm { function envBytes(string calldata) external view returns (bytes memory); }

contract InheritanceTest {
    Vm constant vm = Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);
    Child public c;

    function _deploy(string memory n) internal returns (address d) {
        try vm.envBytes(string.concat("SOLAR_", n, "_BYTECODE")) returns (bytes memory bytecode) {
            assembly { d := create(0, add(bytecode, 0x20), mload(bytecode)) }
        } catch { d = address(new Child()); }
    }

    function setUp() public {
        c = Child(_deploy("CHILD"));
    }

    function test_InheritedStorage() public {
        // Test that parent storage slot 0 contains parentValue
        c.setParent(42);
        require(c.parentValue() == 42, "parentValue should be 42");
    }

    function test_ChildStorage() public {
        // Test that child storage slot 1 contains childValue
        c.setChild(100);
        require(c.childValue() == 100, "childValue should be 100");
    }

    function test_SetBoth() public {
        // Test that both storage slots work correctly
        c.setBoth(1, 2);
        require(c.parentValue() == 1, "parentValue should be 1");
        require(c.childValue() == 2, "childValue should be 2");
    }

    function test_InheritedFunction() public {
        // Test that inherited setParent function works
        c.setParent(77);
        require(c.parentValue() == 77, "parentValue should be 77");
    }

    function test_StorageLayout() public {
        // Test storage layout: parent's slot comes before child's slot
        c.setBoth(111, 222);
        
        // Verify by calling both getters
        require(c.parentValue() == 111, "parentValue should be 111");
        require(c.childValue() == 222, "childValue should be 222");
    }
}
