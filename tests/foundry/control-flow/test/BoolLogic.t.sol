// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/BoolLogic.sol";

contract BoolLogicTest {
    BoolLogic target;

    function setUp() public {
        target = new BoolLogic();
    }

    // ========== Pure function tests (no storage) ==========

    function test_pureAnd_ff() public view {
        assert(target.pureAnd(false, false) == false);
    }

    function test_pureAnd_ft() public view {
        assert(target.pureAnd(false, true) == false);
    }

    function test_pureAnd_tf() public view {
        assert(target.pureAnd(true, false) == false);
    }

    function test_pureAnd_tt() public view {
        assert(target.pureAnd(true, true) == true);
    }

    function test_pureOr_ff() public view {
        assert(target.pureOr(false, false) == false);
    }

    function test_pureOr_ft() public view {
        assert(target.pureOr(false, true) == true);
    }

    function test_pureOr_tf() public view {
        assert(target.pureOr(true, false) == true);
    }

    function test_pureOr_tt() public view {
        assert(target.pureOr(true, true) == true);
    }

    // ========== Storage tests ==========

    function test_storageAnd_ff() public {
        target.setFlags(false, false);
        assert(target.testAnd() == false);
    }

    function test_storageAnd_ft() public {
        target.setFlags(false, true);
        assert(target.testAnd() == false);
    }

    function test_storageAnd_tf() public {
        target.setFlags(true, false);
        assert(target.testAnd() == false);
    }

    function test_storageAnd_tt() public {
        target.setFlags(true, true);
        assert(target.testAnd() == true);
    }

    function test_storageOr_ff() public {
        target.setFlags(false, false);
        assert(target.testOr() == false);
    }

    function test_storageOr_ft() public {
        target.setFlags(false, true);
        assert(target.testOr() == true);
    }

    function test_storageOr_tf() public {
        target.setFlags(true, false);
        assert(target.testOr() == true);
    }

    function test_storageOr_tt() public {
        target.setFlags(true, true);
        assert(target.testOr() == true);
    }
}
