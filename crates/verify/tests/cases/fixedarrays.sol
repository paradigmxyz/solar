pragma solidity ^0.8.20;
/// Fixed-size arrays inside structs: reads are real values, not 0, and lengths are the declared size.
library FixedArr {
    uint256 constant N = 4;
    struct A { uint256 x; uint256 y; }
    struct R { A[5] a; uint256[N] w; uint256 n; }
    function bad_nested(R memory r, uint256 i) internal pure {
        if (i < 5 && r.a[i].x != 0) {
            /// @custom:assert false
            r.n = 1;
        }
    }
    function bad_const_index(R memory r) internal pure {
        if (r.a[1].x != 0) {
            /// @custom:assert false
            r.n = 1;
        }
    }
    /// @custom:ensures result == 5
    function ok_len(R memory r) internal pure returns (uint256) { return r.a.length; }
    /// @custom:ensures result == 4
    function ok_const_len(R memory r) internal pure returns (uint256) { return r.w.length; }
    /// @custom:succeeds_if true
    function ok_in_bounds(R memory r) internal pure returns (uint256) { return r.a[4].y + r.w[3] * 0; }
    /// @custom:succeeds_if true
    function bad_out_of_bounds(R memory r) internal pure returns (uint256) { return r.w[4]; }
    /// @custom:ensures result == 3
    function ok_local(uint256[3] memory a) internal pure returns (uint256) { uint256[3] memory b = a; return b.length; }
}

/// @custom:ghost uint256 gLen
contract RetLen {
    function ok_retlen(address token) external {
        (bool ok, bytes memory result) = token.call("");
        /// @custom:ghost gLen = result.length
        require(ok);
        /// @custom:assert gLen == result.length
        ok = false;
    }
}
