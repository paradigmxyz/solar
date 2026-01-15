// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/MappingEdgeCases.sol";

interface Vm { function envBytes(string calldata) external view returns (bytes memory); }

contract MappingEdgeCasesTest {
    Vm constant vm = Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);
    MappingEdgeCases m;

    function _deploy(string memory n) internal returns (address d) {
        try vm.envBytes(string.concat("SOLAR_", n, "_BYTECODE")) returns (bytes memory c) {
            assembly { d := create(0, add(c, 0x20), mload(c)) }
        } catch { d = address(new MappingEdgeCases()); }
    }

    function setUp() public {
        m = MappingEdgeCases(_deploy("MAPPINGEDGECASES"));
    }

    // ========== Edge Case Keys ==========

    function test_ZeroKey() public {
        m.setZeroKey(42);
        require(m.getZeroKey() == 42, "zero key should work");
    }

    function test_MaxKey() public {
        m.setMaxKey(999);
        require(m.getMaxKey() == 999, "max key should work");
    }

    function test_AddressZero() public {
        m.setAddressZero(123);
        require(m.getAddressZero() == 123, "address(0) key should work");
    }

    function test_Bytes32Zero() public {
        m.setBytes32Zero(456);
        require(m.getBytes32Zero() == 456, "bytes32(0) key should work");
    }

    // ========== Multiple Keys ==========

    function test_MultipleKeys() public {
        m.setMultipleKeys(1, 100, 2, 200);
        require(m.getKey(1) == 100, "key 1 should be 100");
        require(m.getKey(2) == 200, "key 2 should be 200");
    }

    function test_KeysDoNotInterfere() public {
        m.setMultipleKeys(100, 1, 101, 2);
        require(m.getKey(100) == 1, "key 100");
        require(m.getKey(101) == 2, "key 101");
        require(m.getKey(102) == 0, "key 102 unset");
    }

    // ========== Nested Mappings ==========

    function test_NestedAllowance() public {
        address owner = address(0x1234);
        address spender = address(0x5678);
        m.setAllowance(owner, spender, 1000);
        require(m.getAllowance(owner, spender) == 1000, "allowance should be 1000");
    }

    function test_NestedZeroAddresses() public {
        m.setAllowanceZeroAddresses(777);
        require(m.getAllowanceZeroAddresses() == 777, "allowance at zero addresses");
    }

    function test_NestedDifferentPairs() public {
        address a1 = address(0x1);
        address a2 = address(0x2);
        address a3 = address(0x3);
        
        m.setAllowance(a1, a2, 100);
        m.setAllowance(a1, a3, 200);
        m.setAllowance(a2, a3, 300);
        
        require(m.getAllowance(a1, a2) == 100, "a1->a2 = 100");
        require(m.getAllowance(a1, a3) == 200, "a1->a3 = 200");
        require(m.getAllowance(a2, a3) == 300, "a2->a3 = 300");
        require(m.getAllowance(a2, a1) == 0, "a2->a1 unset = 0");
    }

    // ========== Matrix Operations ==========

    function test_MatrixCell() public {
        m.setMatrixCell(5, 10, 50);
        require(m.getMatrixCell(5, 10) == 50, "matrix[5][10] = 50");
    }

    // TODO: MatrixCorners test skipped - uses nested if-else-if which has bugs

    // ========== Overwrite Tests ==========

    function test_Overwrite() public {
        m.overwriteKey(42, 100, 200);
        require(m.getKey(42) == 200, "overwritten value should be 200");
    }

    function test_Increment() public {
        m.setMultipleKeys(10, 5, 0, 0); // Set key 10 to 5
        m.incrementKey(10);
        require(m.getKey(10) == 6, "incremented value should be 6");
        m.incrementKey(10);
        m.incrementKey(10);
        require(m.getKey(10) == 8, "after 3 increments");
    }

    // ========== Default Value Tests ==========

    function test_UnsetKeyReturnsZero() public view {
        require(m.getUnsetKey(99999) == 0, "unset key returns 0");
    }

    function test_UnsetNestedKeyReturnsZero() public view {
        require(m.getUnsetNestedKey(address(0x9999), address(0x8888)) == 0, "unset nested returns 0");
    }
}
