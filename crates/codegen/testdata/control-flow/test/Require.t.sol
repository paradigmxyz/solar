// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/Require.sol";

/// @dev Minimal Foundry cheatcode interface
interface Vm {
    function expectRevert() external;
}

contract RequireTest {
    Vm constant vm = Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);
    Require req;

    function setUp() public {
        req = new Require();
    }

    // ========== require() tests ==========

    function test_RequireTrue() public view {
        req.requireTrue(true); // should not revert
    }

    function test_RequireFalseReverts() public {
        vm.expectRevert();
        req.requireTrue(false);
    }

    function test_RequireWithMessageTrue() public view {
        req.requireWithMessage(true); // should not revert
    }

    function test_RequireWithMessageFalseReverts() public {
        vm.expectRevert();
        req.requireWithMessage(false);
    }

    // ========== revert() tests ==========

    function test_RevertAlwaysReverts() public {
        vm.expectRevert();
        req.revertAlways();
    }

    function test_RevertWithMessageReverts() public {
        vm.expectRevert();
        req.revertWithMessage();
    }

    // ========== divideChecked tests ==========

    function test_DivideCheckedSuccess() public view {
        assert(req.divideChecked(10, 2) == 5);
        assert(req.divideChecked(100, 10) == 10);
        assert(req.divideChecked(7, 3) == 2);
        assert(req.divideChecked(0, 5) == 0);
    }

    function test_DivisionByZeroReverts() public {
        vm.expectRevert();
        req.divideChecked(10, 0);
    }
}
