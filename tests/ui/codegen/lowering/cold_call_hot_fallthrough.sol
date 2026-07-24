//@compile-flags: -Zcodegen -O size -Zdump=evm-ir-runtime --pretty-json

// Calls to the non-returning helper make their blocks cold. The backend should
// lay out each successful continuation as the branch fallthrough.
contract ColdCallHotFallthrough {
    function nonzero(uint256 value) external pure returns (uint256) {
        if (value == 0) abort(value);
        return value;
    }

    function belowLimit(uint256 value) external pure returns (uint256) {
        if (value < 100) abort(value);
        return value;
    }

    // Keep this large enough to remain a call after the inlining pass.
    function abort(uint256 value) internal pure {
        if (value == 0) revert("zero");
        if (value == 1) revert("one");
        if (value == 2) revert("two");
        revert("other");
    }
}
