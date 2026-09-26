// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract Target {
    error Failed(uint256 code);

    uint256 private counter;

    function whoAmI() external returns (address sender, uint256 count) {
        counter += 1;
        return (msg.sender, counter);
    }

    function fail(uint256 code) external pure {
        revert Failed(code);
    }

    function greet() external pure returns (string memory) {
        return "hello, forwarded world";
    }
}
