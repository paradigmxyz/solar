//@compile-flags: --evm-version paris -Zdump=disasm-runtime
//@filecheck:

// On pre-Cancun targets there is no MCOPY; memory copy lowers to overlap-safe
// word-copy loops. The identity precompile would be smaller, but a
// precompile call is observable by tooling that keys behavior on "the next
// call" (Foundry's `vm.prank`/`vm.expectRevert`).

contract McopyPreCancun {
    // CHECK-LABEL: McopyPreCancun (runtime)
    // CHECK-NOT: STATICCALL
    // CHECK-NOT: MCOPY
    function rt(bytes memory b) public pure returns (bytes memory) {
        return abi.decode(abi.encode(b), (bytes));
    }
}
