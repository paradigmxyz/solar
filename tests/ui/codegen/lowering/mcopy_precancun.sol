//@compile-flags: -Zcodegen --evm-version paris -Zdump=disasm-runtime
//@filecheck:

// On pre-Cancun targets there is no MCOPY; memory copy lowers to the identity
// precompile (address 0x04), which returns its input and copies exactly the
// requested length. Verified behaviorally against solc on paris, london, and
// shanghai for bytes and string round-trips.

contract McopyPreCancun {
    // CHECK-LABEL: McopyPreCancun (runtime)
    // CHECK: STATICCALL
    // CHECK-NOT: MCOPY
    function rt(bytes memory b) public pure returns (bytes memory) {
        return abi.decode(abi.encode(b), (bytes));
    }
}
