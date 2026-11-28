pragma solidity ^0.8.20;
// Two functions that pass each other a memory object of their own: asking whether either writes
// its parameter once walked the other's body again at every call, without end.
contract Cycle {
    uint256 public n;

    /// @custom:ensures result == 0
    function _a(uint256[2] memory xs, uint256 k) internal pure returns (uint256 result) {
        uint256[2] memory t;
        if (k == 0) return xs[0] * 0;
        result = _b(t, k - 1);
    }

    /// @custom:ensures result == 0
    function _b(uint256[2] memory ys, uint256 k) internal pure returns (uint256 result) {
        uint256[2] memory t;
        result = _a(t, k) + ys[0] * 0;
    }

    /// @custom:ensures n == old(n) + 1
    function ok_bump(uint256 k) external {
        uint256[2] memory xs;
        n += 1 + _a(xs, k);
    }

    /// @custom:ensures n == old(n) + 1
    function ok_loop(uint256 k) external {
        uint256[2] memory xs;
        /// @custom:invariant i <= 1
        /// @custom:invariant n == old(n) + i
        for (uint256 i = 0; i < 1; i++) {
            n += 1 + _a(xs, k);
        }
    }
}
