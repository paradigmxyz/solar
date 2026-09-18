//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: selector 0xdeadbeef01, 0 => 0xdeadbeef
//@ run-call: selector 0xdeadbeef01, 1 => 0xadbeef01
//@ run-call: selector 0xdeadbeef, 2 => 0xbeef0000

// The same load in assembly. Two bytes into a four-byte slice it reads two
// bytes of the slice and two of whatever calldata follows it, here padding.
// CHECK-LABEL: fn @selector
// CHECK: calldataload
contract Unsafe {
    function selector(bytes calldata b, uint256 offset) public pure returns (bytes4 out) {
        assembly ("memory-safe") {
            out := calldataload(add(b.offset, offset))
        }
    }
}
