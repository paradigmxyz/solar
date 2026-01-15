// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/StackDeep.sol";

contract StackDeepTest {
    StackDeep public stackDeep;

    function setUp() public {
        stackDeep = new StackDeep();
    }

    function test_ManyLocals() public view {
        // manyLocals(10) should compute:
        // v1=11, v2=12, v3=13, ..., v20=30
        // sum = 11+12+13+...+30 = sum(11..30) = (11+30)*20/2 = 410
        uint256 result = stackDeep.manyLocals(10);
        require(result == 410, "manyLocals(10) should return 410");
    }

    function test_ManyLocals_Zero() public view {
        // manyLocals(0): v1=1, v2=2, ..., v20=20
        // sum = 1+2+...+20 = 210
        uint256 result = stackDeep.manyLocals(0);
        require(result == 210, "manyLocals(0) should return 210");
    }

    function test_ManyParamsAndLocals() public view {
        // Tests 8 params + 15 locals = 23 active variables
        uint256 result = stackDeep.manyParamsAndLocals(1, 2, 3, 4, 5, 6, 7, 8);
        // v1=3, v2=7, v3=11, v4=15, v5=10, v6=26, v7=36,
        // v8=37, v9=39, v10=42, v11=46, v12=51, v13=57, v14=64, v15=72
        // sum = 3+7+11+15+10+26+36+37+39+42+46+51+57+64+72 = 516
        require(result == 516, "manyParamsAndLocals should return 516");
    }
}
