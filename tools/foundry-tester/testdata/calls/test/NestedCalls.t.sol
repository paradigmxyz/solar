// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/NestedCalls.sol";

contract NestedCallsTest {
    NestedCalls public nc;

    function setUp() public {
        nc = new NestedCalls();
    }

    function test_Add() public view {
        assert(nc.add(5, 3) == 8);
    }

    function test_Mul() public view {
        assert(nc.mul(7, 6) == 42);
    }

    function test_Nested2() public view {
        assert(nc.nested2(3, 4, 5) == 17);
    }

    function test_Nested3() public view {
        assert(nc.nested3(1, 2, 3, 4) == 10);
    }

    function test_DeepNested() public view {
        assert(nc.deepNested(10) == 16);
    }

    function test_Inner() public view {
        assert(nc.inner(5) == 10);
    }

    function test_Outer() public view {
        assert(nc.outer(3) == 12);
        assert(nc.outer(10) == 40);
    }

    function test_MixedBitwise() public view {
        assert(nc.mixedBitwise(0xAB, 0xCD) == 0xAD);
    }

    function test_NestedShifts() public view {
        assert(nc.nestedShifts(0) == 16);
    }
}
