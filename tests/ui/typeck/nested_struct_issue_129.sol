// https://github.com/paradigmxyz/solar/issues/129

// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

struct SimpleStruct {
    uint256 a;
}

struct NestedTestStruct {
    SimpleStruct simpleStruct;
    SimpleStruct simpleStruct2;
}

interface TestContract {
    function nestedStructFunction() external pure returns (NestedTestStruct memory);
}
