// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import { Differential } from "./Differential.sol";

/// Reduced counterexamples from differential fuzzing, one per divergence
/// fixed in the rewrite (see DIVERGENCES.md).
contract RegressionsTest is Differential {
    function setUp() public {
        _setUpVariants(OPTIMIZED, vm.envOr("SEAPORT_B", HIGHLEVEL));
    }

    /// D1: incrementCounter wraps when the counter plus the shifted
    /// blockhash exceeds 2^256 - 1; the reference panicked with 0x11.
    function test_D1_incrementCounterWraps() public {
        address caller = address(0xC0FFEE);
        vm.roll(2);
        vm.setBlockhash(1, bytes32(uint256(1) << 128));
        vm.store(seaport, keccak256(abi.encode(caller, uint256(0))), bytes32(type(uint256).max));
        Outcome memory o = _diff(caller, 0, abi.encodeWithSignature("incrementCounter()"));
        assertTrue(o.ok);
        assertEq(abi.decode(o.ret, (uint256)), 0);
    }
}
