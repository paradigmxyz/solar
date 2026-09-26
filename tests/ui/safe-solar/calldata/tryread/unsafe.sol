//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: peek 0xdeadbeef01, 0 => true, 0xdeadbeef
//@ run-call: peek 0xdeadbeef01, 1 => true, 0xadbeef01
//@ run-call: peek 0xdeadbeef01, 3 => true, 0xef010000

// The same load in assembly, with nothing to say it ran past the slice: two
// bytes of the slice and two of the padding after it, reported as a value.
// CHECK-LABEL: fn @peek
// CHECK: calldataload
contract Unsafe {
    function peek(bytes calldata b, uint256 offset) public pure returns (bool ok, bytes4 out) {
        assembly ("memory-safe") {
            out := calldataload(add(b.offset, offset))
            ok := 1
        }
    }
}
