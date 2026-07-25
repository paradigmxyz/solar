//@compile-flags: -Zcodegen --evm-version paris -Zdump=mir
//@filecheck: --check-prefix=MC

// On pre-Cancun targets there is no MCOPY; memory copy lowers to the identity
// precompile (address 0x04), which returns its input and copies exactly the
// requested length. Verified behaviorally against solc on paris, london, and
// shanghai for bytes and string round-trips.

contract McopyPreCancun {
    // MC-LABEL: fn @rt
    // MC: staticcall
    // MC-NOT: mcopy
    function rt(bytes memory b) public pure returns (bytes memory) {
        return abi.decode(abi.encode(b), (bytes));
    }
}
