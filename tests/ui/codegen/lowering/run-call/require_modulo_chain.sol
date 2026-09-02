//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: chain 12, 3 => 15
//@ run-call-fail: chain 12, 0 => Panic(0x12)
//@ run-call-fail: chain 11, 3
//@ run-call: chain 0, 1 => 1

contract RequireModuloCases {
    function chain(uint256 a, uint256 b) external pure returns (uint256) {
        require(a % b == 0);
        require(b % 3 == 0 || a % 2 == 0);
        return a + b;
    }
}
