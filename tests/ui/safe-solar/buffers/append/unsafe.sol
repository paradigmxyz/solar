//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: build 0x0102, 0x0304 => 0x01020304, 0x11223344
//@ run-call: build 0xaaaaaaaa, 0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb0000000000000000000000000000000000000000000000000000000000000004deadbeef => 0xaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb0000000000000000000000000000000000000000000000000000000000000004deadbeef, 0xdeadbeef

// The same appends in assembly into a buffer of four bytes, with nothing
// that looks at the capacity. Sixty-eight bytes run through the buffer, over
// the next allocation's length and into its contents: `next` was never
// assigned to and comes back as the tail of `b`.
// CHECK-LABEL: fn @build
// CHECK: mcopy
contract Unsafe {
    function build(bytes memory a, bytes memory b)
        public
        pure
        returns (bytes memory out, bytes memory next)
    {
        out = new bytes(4);
        next = hex"11223344";
        assembly ("memory-safe") {
            let used := mload(a)
            mcopy(add(out, 0x20), add(a, 0x20), used)
            mcopy(add(add(out, 0x20), used), add(b, 0x20), mload(b))
            mstore(out, add(used, mload(b)))
        }
    }
}
