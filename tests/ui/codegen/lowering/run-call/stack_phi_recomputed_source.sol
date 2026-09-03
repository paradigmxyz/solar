//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: count 0, 0x => 0
//@ run-call: count 1, 0x => 1
//@ run-call: count 2, 0x => 2
//@ run-call: count 0, 0x0102 => 0
//@ run-call: count 1, 0x0102 => 1
//@ run-call: count 2, 0x0102 => 2

contract StackPhiRecomputedSource {
    function count(uint256 n, bytes calldata) external pure returns (uint256 result) {
        unchecked {
            for (uint256 i; i < n; ++i) {
                ++result;
            }
        }
    }
}
