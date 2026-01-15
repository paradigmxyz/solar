// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/EdgeCases.sol";

contract EdgeCasesTest {
    EdgeCases c;

    function setUp() public {
        c = new EdgeCases();
    }

    function test_addMax() public view {
        assert(c.addMax() == type(uint256).max);
    }

    function test_addZero() public view {
        assert(c.addZero(0) == 0);
        assert(c.addZero(1) == 1);
        assert(c.addZero(type(uint256).max) == type(uint256).max);
    }

    function test_mulZero() public view {
        assert(c.mulZero(0) == 0);
        assert(c.mulZero(1) == 0);
        assert(c.mulZero(type(uint256).max) == 0);
    }

    function test_mulOne() public view {
        assert(c.mulOne(0) == 0);
        assert(c.mulOne(1) == 1);
        assert(c.mulOne(type(uint256).max) == type(uint256).max);
    }

    function test_divOne() public view {
        assert(c.divOne(0) == 0);
        assert(c.divOne(1) == 1);
        assert(c.divOne(type(uint256).max) == type(uint256).max);
    }

    function test_subSame() public view {
        assert(c.subSame(0) == 0);
        assert(c.subSame(1) == 0);
        assert(c.subSame(type(uint256).max) == 0);
    }

    function test_modSame() public view {
        assert(c.modSame(0) == 0);
        assert(c.modSame(1) == 0);
        assert(c.modSame(type(uint256).max) == 0);
    }

    function test_maxInt() public view {
        assert(c.maxInt() == type(int256).max);
    }

    function test_minInt() public view {
        assert(c.minInt() == type(int256).min);
    }

    function test_maxUint8() public view {
        assert(c.maxUint8() == type(uint8).max);
    }

    function test_identityBool() public view {
        assert(c.identityBool(true) == true);
        assert(c.identityBool(false) == false);
    }

    function test_negateBool() public view {
        assert(c.negateBool(true) == false);
        assert(c.negateBool(false) == true);
    }
}
