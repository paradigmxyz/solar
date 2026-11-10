//@ codegen-matrix: standard
//@[mir] run-call-fail: spin 0x8000000000000000000000000000000000000000000000000000000000000000; gas=1000000
//@[gas] run-call: spin 0x8000000000000000000000000000000000000000000000000000000000000000; gas=1000000 => 1
//@[size] run-call: spin 0x8000000000000000000000000000000000000000000000000000000000000000; gas=1000000 => 1
//@ run-call: spin 100 => 1
pragma solidity ^0.8.0;

// An optimized build skips a loop that only counts, even one that would run out of gas.
contract DeadLoopOutOfGas {
    function spin(uint256 n) external pure returns (uint256) {
        unchecked {
            for (uint256 i; i < n; ++i) {}
        }
        return 1;
    }
}
