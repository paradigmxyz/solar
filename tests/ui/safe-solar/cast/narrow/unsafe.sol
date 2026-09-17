//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: narrow 0 => 0
//@ run-call: narrow 1099511627775 => 1099511627775
//@ run-call: narrow 1099511627776 => 0
//@ run-call: narrow 1099511627783 => 7

// The same narrowing as a mask. A value that does not fit comes back as its
// low forty bits, which is a different number and says nothing.
// CHECK-LABEL: fn @narrow
// CHECK: 0xffffffffff
contract Unsafe {
    function narrow(uint256 x) public pure returns (uint40 y) {
        assembly ("memory-safe") {
            y := and(x, 0xffffffffff)
        }
    }
}
