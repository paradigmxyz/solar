// https://github.com/paradigmxyz/solar/issues/128

// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

struct StructTest {
    uint256 a;
    mapping(uint256 index => uint256) data;
}

library TestLibrary {
    function testFunction(StructTest storage testParameter) external view returns (uint256) {
        return testParameter.a;
    }
}
