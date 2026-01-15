// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/StackDeep.sol";

contract StackDeepTest {
    StackDeep public stackDeep;

    function setUp() public {
        stackDeep = new StackDeep();
    }

    function test_ManyLocals() public view {
        uint256 result = stackDeep.manyLocals(1, 2, 3, 4, 5, 6, 7, 8);
        // v1=3, v2=7, v3=11, v4=15, v5=10, v6=26, v7=36, v8=37, v9=39, v10=42, v11=46, v12=51
        // sum = 3+7+11+15+10+26+36+37+39+42+46+51+6+7+8 = 344
        assert(result == 344);
    }

    function test_ExtremeLocals() public view {
        uint256 result = stackDeep.extremeLocals(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12);
        // This tests 12 params + 14 local vars = 26 variables active
        assert(result > 0);
    }
}
