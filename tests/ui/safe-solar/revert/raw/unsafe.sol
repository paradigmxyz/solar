//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call-fail: boom 0xdeadbeef => 0xdeadbeef
//@ run-call-fail: boom 0x
//@ run-call: maybe 0x01, false => 7
//@ run-call-fail: maybe 0x0102, true => 0x0102

// The same revert in assembly, as the libraries write it. It behaves the
// same; what it lacks is any statement of its effect that the compiler can
// read, so every caller of a helper written this way is opaque to it.
// CHECK-LABEL: fn @boom
// CHECK: revert
contract Unsafe {
    function boom(bytes memory data) public pure {
        assembly ("memory-safe") {
            revert(add(data, 0x20), mload(data))
        }
    }

    function maybe(bytes memory data, bool go) public pure returns (uint256) {
        if (go) {
            assembly ("memory-safe") {
                revert(add(data, 0x20), mload(data))
            }
        }
        return 7;
    }
}
