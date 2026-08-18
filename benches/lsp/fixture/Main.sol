// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import {Math} from "./Math.sol";

contract Main {
    uint256 public value;

    function double(uint256 input) public pure returns (uint256) {
        return Math.twice(input);
    }

    function compute() external pure returns (uint256) {
        return double(21);
    }

    function completions() external returns (uint256) {
        return this.value();
    }
}
