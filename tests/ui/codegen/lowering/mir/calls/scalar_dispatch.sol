//@ codegen-matrix: standard gasMir sizeMir
//@[gasMir] compile-flags: -O gas -Zdump=mir
//@[sizeMir] compile-flags: -O size -Zdump=mir
//@[gasMir,sizeMir] filecheck: --implicit-check-not=invalid
// CHECK-LABEL: @module ScalarDispatch
// CHECK-LABEL: fn @entry()
// CHECK: calldataload 4
// CHECK: calldataload 36
// CHECK: calldataload 68
// CHECK: switch
// CHECK: phi
// CHECK-NOT: tail_call

//@ run-call: difference 17, 23, 0 => 17
//@ run-call: difference 17, 23, 1 => 6
//@ run-call: difference 17, 23, 64 => 113
//@ run-call: sumDifference 17, 23, 0 => 17
//@ run-call: sumDifference 17, 23, 1 => 17
//@ run-call: sumDifference 17, 23, 64 => 0
//@ run-call: complement 17, 23, 0 => 17
//@ run-call: complement 17, 23, 1 => 6
//@ run-call: complement 17, 23, 64 => 1009
//@ run-call: absorb 17, 23, 0 => 17
//@ run-call: absorb 17, 23, 1 => 0
//@ run-call: absorb 17, 23, 64 => 0
//@ run-call-fail: 0xffffffff
//@ run-call-fail: 0x214636e1

contract ScalarDispatch {
    function difference(uint256 x, uint256 y, uint256 rounds) external pure returns (uint256) {
        assembly {
            for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                x := sub(or(x, y), and(x, y))
                y := add(y, i)
            }
        }
        return x;
    }

    function sumDifference(uint256 x, uint256 y, uint256 rounds) external pure returns (uint256) {
        assembly {
            for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                x := sub(add(x, y), or(x, y))
                y := add(y, i)
            }
        }
        return x;
    }

    function complement(uint256 x, uint256 y, uint256 rounds) external pure returns (uint256) {
        assembly {
            for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                x := not(add(x, not(y)))
                y := add(y, i)
            }
        }
        return x;
    }

    function absorb(uint256 x, uint256 y, uint256 rounds) external pure returns (uint256) {
        assembly {
            for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                x := and(x, not(and(x, y)))
                y := add(y, i)
            }
        }
        return x;
    }
}
