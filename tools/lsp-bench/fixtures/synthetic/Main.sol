// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Math} from "./Math.sol";

contract Main {
    uint256 public stored;

    function calculate(uint256 input) external pure returns (uint256) {
        return Math.double(input);
    }

    function status() external pure returns (string memory) {
        return "ready";
    }
}
