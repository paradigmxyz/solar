//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=mir
//@[gas] filecheck:
//@ run-call: first 7 => 8
//@ run-call: second 7 => 14
//@ run-call-fail: first 0 => 0x
//@ run-call-fail: second 0 => 0x

// The default pipeline must inline a shared returning guard. Checking the
// complete module catches both remaining calls and the dead helper body.
// CHECK: @module Guards
// CHECK-NOT: icall @guard
// CHECK-NOT: fn @guard
contract Guards {
    function guard(uint256 x) private pure {
        require(x != 0);
    }
    function first(uint256 x) external pure returns (uint256) {
        guard(x);
        unchecked { return x + 1; }
    }
    function second(uint256 x) external pure returns (uint256) {
        guard(x);
        unchecked { return x * 2; }
    }
}
