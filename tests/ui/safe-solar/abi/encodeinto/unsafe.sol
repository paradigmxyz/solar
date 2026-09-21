//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: payload 0x000000000000000000000000000000000000dEaD, 1 => 0xa9059cbb000000000000000000000000000000000000000000000000000000000000dead0000000000000000000000000000000000000000000000000000000000000001
//@ run-call: short 0x000000000000000000000000000000000000dEaD, 1 => 0xa9059cbb000000000000000000000000000000000000000000000000000000000000dead

// The same payload in assembly. On a buffer of the right size they agree.
// The short buffer does not fail: the last word lands in whatever memory
// follows it, and the returned bytes stop at the length, so the overwrite is
// invisible here and corrupts whatever was allocated next.
// CHECK-LABEL: fn @payload
// CHECK: mstore
// CHECK: mstore
contract Unsafe {
    function payload(address to, uint256 amount) public pure returns (bytes memory out) {
        out = new bytes(4 + 64);
        assembly ("memory-safe") {
            let p := add(out, 0x20)
            mstore(p, shl(224, 0xa9059cbb))
            mstore(add(p, 4), to)
            mstore(add(p, 36), amount)
        }
    }

    function short(address to, uint256 amount) public pure returns (bytes memory out) {
        out = new bytes(4 + 32);
        assembly ("memory-safe") {
            let p := add(out, 0x20)
            mstore(p, shl(224, 0xa9059cbb))
            mstore(add(p, 4), to)
            mstore(add(p, 36), amount)
        }
    }
}
