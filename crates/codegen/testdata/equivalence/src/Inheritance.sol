// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title Base - Base contract for inheritance testing
contract Base {
    uint256 public baseValue;

    function setBaseValue(uint256 v) external virtual {
        baseValue = v;
    }

    function getBaseValue() external view returns (uint256) {
        return baseValue;
    }
}

/// @title Derived - Derived contract for inheritance testing
contract Derived is Base {
    uint256 public derivedValue;

    function setBaseValue(uint256 v) external override {
        baseValue = v * 2;
    }

    function setDerivedValue(uint256 v) external {
        derivedValue = v;
    }

    function getSum() external view returns (uint256) {
        return baseValue + derivedValue;
    }
}
