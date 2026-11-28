pragma solidity ^0.8.20;
// `N` has two values, so the local `uint256[N]` has no known size and the file is refused rather
// than read as an empty dynamic array.
contract Other { uint256 constant N = 1; }
contract Target {
    uint256 constant N = 2;
    /// @custom:ensures r == 0
    function f() external pure returns (uint256 r) {
        uint256[N] memory a;
        return a.length;
    }
}
