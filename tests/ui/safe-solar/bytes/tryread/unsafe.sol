//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: peek 0xdeadbeef01, 0 => true, 0xdeadbeef
//@ run-call: peek 0xdeadbeef01, 1 => true, 0xadbeef01
//@ run-call: peek 0xdeadbeef01, 3 => true, 0xef010000

// The same load in assembly, with nothing to say it went past the end: two
// bytes of the buffer and two of the padding after it, reported as a value.
// CHECK-LABEL: fn @peek
// CHECK: mload
contract Unsafe {
    function peek(bytes memory b, uint256 offset) public pure returns (bool ok, bytes4 out) {
        bytes memory next = hex"11223344";
        next;
        assembly ("memory-safe") {
            out := mload(add(add(b, 0x20), offset))
            ok := 1
        }
    }
}
