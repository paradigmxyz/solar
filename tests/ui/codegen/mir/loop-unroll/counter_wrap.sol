//@ codegen-matrix: standard dump
//@ compile-flags: --optimize-runs 10000
//@[dump] compile-flags: -Ogas -Zdump=mir
//@ run-call: below() => 7
//@ run-call: inclusive() => 7
//@ run-call: inclusiveStep() => 7
//@ run-call: fullRange() => 7
pragma solidity ^0.8.0;

contract CounterWrap {
    function below() external pure returns (uint256) {
        unchecked {
            for (uint256 i = type(uint256).max - 1; i < type(uint256).max; i += 2) {
                if (i == 0) return 7;
            }
        }
        return 9;
    }

    function inclusive() external pure returns (uint256) {
        unchecked {
            for (uint256 i = type(uint256).max; i <= type(uint256).max; i++) {
                if (i == 0) return 7;
            }
        }
        return 9;
    }

    function inclusiveStep() external pure returns (uint256) {
        unchecked {
            for (uint256 i = type(uint256).max - 1; i <= type(uint256).max; i += 2) {
                if (i == 0) return 7;
            }
        }
        return 9;
    }

    function fullRange() external pure returns (uint256) {
        unchecked {
            for (uint256 i = 0; i <= type(uint256).max; i++) {
                if (i == 1) return 7;
            }
        }
        return 9;
    }
}
