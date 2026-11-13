pragma solidity ^0.8.20;

/// The same constant as Base.sol's, as Seaport's constant files repeat words: either reads alike.
uint256 constant LIMIT = 100;

/// A file-level function, called as an internal library function would be.
function halve(uint256 x) pure returns (uint256) {
    return x / 2;
}
