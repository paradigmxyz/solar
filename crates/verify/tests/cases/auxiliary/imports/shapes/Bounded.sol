pragma solidity ^0.8.20;

library Bounded {
    uint256 internal constant MAX = 1000;

    /// @custom:ensures result <= MAX
    function clamp(uint256 x) internal pure returns (uint256 result) {
        result = x > MAX ? MAX : x;
    }
}
