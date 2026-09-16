//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: shift 0x0102030405 => 0x0101020304
//@ run-call: between 0xaaaaaaaaaaaaaaaa, 0x1122334455667788 => 0xaaaa2233445566aa
//@ run-call: between 0xaaaaaaaa, 0x1122334455667788 => 0xaaaa2233

// The same copies as a bare `mcopy`. On valid input they agree. Copying five
// bytes into offset two of a four-byte buffer does not fail: two land inside
// and three overrun into whatever memory follows.
// CHECK-LABEL: fn @shift
// CHECK: mcopy
contract Unsafe {
    function shift(bytes memory b) public pure returns (bytes memory) {
        assembly ("memory-safe") {
            let data := add(b, 0x20)
            mcopy(add(data, 1), data, sub(mload(b), 1))
        }
        return b;
    }

    function between(bytes memory dst, bytes memory src) public pure returns (bytes memory) {
        assembly ("memory-safe") {
            mcopy(add(dst, 0x22), add(src, 0x21), 5)
        }
        return dst;
    }
}
