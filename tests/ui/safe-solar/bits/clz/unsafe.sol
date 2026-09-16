//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: leading 0 => 256
//@ run-call: leading 1 => 255
//@ run-call: leading 57896044618658097711785492504343953926634992332820282019728792003956564819968 => 0
//@ run-call: leading 255 => 248

// The instruction itself, which only compiles for a target that has it.
// CHECK-LABEL: fn @leading
// CHECK: clz
contract Unsafe {
    function leading(uint256 x) public pure returns (uint256 n) {
        assembly ("memory-safe") {
            n := clz(x)
        }
    }
}
