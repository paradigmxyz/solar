// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract ForwardTarget {
    error Failed(uint256 code);

    uint256 private counter;

    function sum(uint256 a, uint256 b) external pure returns (uint256) {
        return a + b;
    }

    function whoAmI() external returns (address sender, uint256 count) {
        counter += 1;
        return (msg.sender, counter);
    }

    function fail(uint256 code) external pure {
        revert Failed(code);
    }

    function paid() external payable returns (uint256) {
        return msg.value;
    }

    function greet() external pure returns (string memory) {
        return "hello, forwarded world";
    }
}
