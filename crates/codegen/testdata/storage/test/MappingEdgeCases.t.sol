// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/MappingEdgeCases.sol";

contract MappingEdgeCasesTest {
    MappingEdgeCases m;

    function setUp() public {
        m = new MappingEdgeCases();
    }

    function test_ZeroKey() public {
        m.setZeroKey(42);
        assert(m.getZeroKey() == 42);
    }

    function test_MaxKey() public {
        m.setMaxKey(999);
        assert(m.getMaxKey() == 999);
    }

    function test_AddressZero() public {
        m.setAddressZero(123);
        assert(m.getAddressZero() == 123);
    }

    function test_Bytes32Zero() public {
        m.setBytes32Zero(456);
        assert(m.getBytes32Zero() == 456);
    }

    function test_MultipleKeys() public {
        m.setMultipleKeys(1, 100, 2, 200);
        assert(m.getKey(1) == 100);
        assert(m.getKey(2) == 200);
    }

    function test_KeysDoNotInterfere() public {
        m.setMultipleKeys(100, 1, 101, 2);
        assert(m.getKey(100) == 1);
        assert(m.getKey(101) == 2);
        assert(m.getKey(102) == 0);
    }

    function test_NestedAllowance() public {
        address owner = address(0x1234);
        address spender = address(0x5678);
        m.setAllowance(owner, spender, 1000);
        assert(m.getAllowance(owner, spender) == 1000);
    }

    function test_NestedZeroAddresses() public {
        m.setAllowanceZeroAddresses(777);
        assert(m.getAllowanceZeroAddresses() == 777);
    }

    function test_NestedDifferentPairs() public {
        address a1 = address(0x1);
        address a2 = address(0x2);
        address a3 = address(0x3);

        m.setAllowance(a1, a2, 100);
        m.setAllowance(a1, a3, 200);
        m.setAllowance(a2, a3, 300);

        assert(m.getAllowance(a1, a2) == 100);
        assert(m.getAllowance(a1, a3) == 200);
        assert(m.getAllowance(a2, a3) == 300);
        assert(m.getAllowance(a2, a1) == 0);
    }

    function test_MatrixCell() public {
        m.setMatrixCell(5, 10, 50);
        assert(m.getMatrixCell(5, 10) == 50);
    }

    function test_Overwrite() public {
        m.overwriteKey(42, 100, 200);
        assert(m.getKey(42) == 200);
    }

    function test_Increment() public {
        m.setMultipleKeys(10, 5, 0, 0);
        m.incrementKey(10);
        assert(m.getKey(10) == 6);
        m.incrementKey(10);
        m.incrementKey(10);
        assert(m.getKey(10) == 8);
    }

    function test_UnsetKeyReturnsZero() public view {
        assert(m.getUnsetKey(99999) == 0);
    }

    function test_UnsetNestedKeyReturnsZero() public view {
        assert(m.getUnsetNestedKey(address(0x9999), address(0x8888)) == 0);
    }
}
