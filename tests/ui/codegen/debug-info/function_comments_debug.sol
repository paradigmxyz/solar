//@ compile-flags: -Ogas -Zdump=evm-ir-runtime --emit=srcmap-runtime

// The comment after a function's entry label appears with debug information too, beside the
// `invoke` attribute, and leaves the debug information as it is.

contract C {
    function add(uint256 a, uint256 b) external pure returns (uint256) {
        return a + b;
    }
}
