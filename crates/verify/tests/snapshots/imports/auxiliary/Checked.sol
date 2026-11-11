pragma solidity ^0.8.20;

contract Checked {
    uint256 internal stored;

    /// @custom:check amount <= 1000
    function store(uint256 amount) public {
        stored = amount;
    }
}
