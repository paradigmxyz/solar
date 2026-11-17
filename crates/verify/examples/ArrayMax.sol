// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

library ArrayMax {
    /// @custom:requires xs.length > 0
    /// @custom:ensures forall uint256 j :: j < xs.length ==> xs[j] <= m
    /// @custom:ensures exists uint256 j :: j < xs.length && xs[j] == m
    function max(uint256[] memory xs) internal pure returns (uint256 m) {
        m = xs[0];
        for (uint256 i = 1; i < xs.length; i++) {
            if (xs[i] > m) {
                m = xs[i];
            }
        }
    }

    /// @custom:ensures r <= n
    function clamp(uint256 x, uint256 n) internal pure returns (uint256 r) {
        r = x > n ? n : x;
    }
}

/// ArrayMax.max behind an external entry point, so that the loop is in the runtime bytecode
/// (an internal library function compiles to none).
contract ArrayMaxEntry {
    /// @custom:ensures forall uint256 j :: j < xs.length ==> xs[j] <= result
    /// @custom:ensures exists uint256 j :: j < xs.length && xs[j] == result
    function max(uint256[] calldata xs) external pure returns (uint256) {
        require(xs.length > 0);
        return ArrayMax.max(xs);
    }
}
