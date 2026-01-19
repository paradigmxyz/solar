// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title MultipleReturns - Contract with multiple return values for equivalence testing
contract MultipleReturns {
    uint256 public x;
    uint256 public y;

    function setValues(uint256 _x, uint256 _y) external {
        x = _x;
        y = _y;
    }

    function getTwo() external view returns (uint256, uint256) {
        return (x, y);
    }

    function getThree() external view returns (uint256, uint256, uint256) {
        return (x, y, x + y);
    }

    function getSwapped() external view returns (uint256 b, uint256 a) {
        a = x;
        b = y;
    }
}
