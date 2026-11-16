// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract PhiEdges {
    uint256 stored;

    modifier pass() {
        _;
    }

    function loopEntry(uint256 n) external pure returns (uint256) {
        uint256 sum = 0;
        for (uint256 i = 0; i < n; i++) {
            sum = sum + i;
        }
        return sum;
    }

    function armConstant(bool c, uint256 a) external returns (uint256) {
        uint256 x = a + 1;
        if (c) {
            x = 7;
            stored = 2;
        }
        return x + a;
    }

    function earlyReturn(uint256 n) external pass returns (bool) {
        if (n > 3) {
            stored = 1;
            return true;
        }
        return false;
    }

    function branchingAssignment(bool c, uint256 a) external pure returns (uint256) {
        uint256 r = 5;
        if (c) {
            r = a;
        }
        return r + a;
    }

    function checkedArm(bool c, uint256 a) external pure returns (uint256) {
        uint256 x = a + 1;
        if (c) {
            x = 11;
            a = a * 2;
        }
        return x + a;
    }
}
