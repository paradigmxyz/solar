//@ codegen-matrix: standard optimized
//@[optimized] compile-flags: -Ogas -Zdump=mir
//@[optimized] filecheck:
//@ run-call: accumulate 0, 7 => 7
//@ run-call: accumulate 1, 7 => 7
//@ run-call: accumulate 2, 7 => 8
//@ run-call: accumulate 10, 7 => 52
//@ run-call: accumulate 1000, 7 => 499507
//@ run-call: accumulate 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call-fail: accumulate 2, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: accumulate 1001, 0
//@ run-call: add 7, 9 => 16
//@ run-call-fail: add 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

contract LoopCheckedHelper {
    function checkedAdd(uint256 a, uint256 b) internal pure returns (uint256 c) {
        unchecked { c = a + b; }
        if (c < a) panicArithmetic();
    }

    function panicArithmetic() internal pure {
        assembly {
            mstore(0, shl(224, 0x4e487b71))
            mstore(4, 0x11)
            revert(0, 36)
        }
    }

    // CHECK-LABEL: fn @accumulate(
    // CHECK-NOT: icall @checkedAdd
    // CHECK: [[SUM:v[0-9]+]] = add
    // CHECK-NEXT: [[WRAP:v[0-9]+]] = lt [[SUM]],
    // CHECK-NEXT: jumpi [[WRAP]],
    // CHECK: = add {{v[0-9]+}}, 1
    // CHECK-NEXT: jump
    // CHECK-NOT: icall @checkedAdd
    // CHECK: tail_call @panicArithmetic
    // CHECK-NOT: icall @checkedAdd
    // CHECK-LABEL: fn @add(
    function accumulate(uint256 n, uint256 seed) public pure returns (uint256 acc) {
        require(n <= 1000);
        acc = seed;
        for (uint256 i = 0; i < n; i = checkedAdd(i, 1)) {
            acc = checkedAdd(acc, i);
        }
    }

    function add(uint256 a, uint256 b) public pure returns (uint256) {
        return checkedAdd(a, b);
    }
}
