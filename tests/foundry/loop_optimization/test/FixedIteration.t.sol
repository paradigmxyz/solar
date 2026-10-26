// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "../src/fixed_iteration.sol";

contract FixedIterationTest {
    FixedIteration c;

    function setUp() public {
        c = new FixedIteration();
    }

    function test_InitSmall() public view {
        uint256[4] memory r = c.initSmall();
        for (uint256 i = 0; i < 4; i++) {
            assert(r[i] == i * 10);
        }
    }

    function test_InitMedium() public view {
        uint256[8] memory r = c.initMedium();
        for (uint256 i = 0; i < 8; i++) {
            assert(r[i] == i * 10);
        }
    }

    function test_SumSquares() public view {
        assert(c.sumSquares() == 14);
    }

    function test_DoubleAll() public view {
        uint256[4] memory arr = [uint256(1), 2, 3, 4];
        uint256[4] memory r = c.doubleAll(arr);
        for (uint256 i = 0; i < 4; i++) {
            assert(r[i] == arr[i] * 4);
        }
    }
}
