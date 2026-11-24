pragma solidity ^0.8.20;
// `N` has two values, so `uint256[N]` has no known size and the file is refused rather than
// read as a dynamic array.
contract Other { uint256 constant N = 1; }
contract Target {
    uint256 constant N = 2;
    uint256[N] private xs;
    /// @custom:ensures xs.length == 0
    constructor() {}
}
