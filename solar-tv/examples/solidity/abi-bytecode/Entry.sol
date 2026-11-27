pragma solidity ^0.8.0;

contract Entry {
    function echo(bool enabled, uint256 value) external pure returns (uint256) {
        if (!enabled) revert();
        return value;
    }
}
