//@ codegen-matrix: standard
//@ run-call: sumRange 1, 10 => 55
//@ run-call: sumRange 10, 200 => 20055
//@ run-call: sumRange 9, 3 => 0
//@ run-call: sumRange 0, 0 => 0
//@ run-call-fail: sumRange 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: sumRange 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0x8000000000000000000000000000000000000000000000000000000000000001 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

contract LoopLiveOrder {
    // Both the sum and the induction value cross each overflow branch. Their
    // successor order must preserve the still-live original operand on both edges.
    function sumRange(uint256 start, uint256 end) external pure returns (uint256 sum) {
        for (uint256 i = start; i <= end; ++i) {
            sum += i;
        }
    }
}
