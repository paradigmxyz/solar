//@ compile-flags: -Ogas -Zdump=mir
//@ filecheck:
//@ run-call: build 0x0102, 0x0304 => 0x01020304, 0x11223344
//@ run-call: build 0xaaaaaaaa, 0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb0000000000000000000000000000000000000000000000000000000000000004deadbeef => 0xaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb0000000000000000000000000000000000000000000000000000000000000004deadbeef, 0x11223344

// Appending to a builder made for four bytes. What does not fit moves the
// builder; the allocation after it keeps its bytes.
// CHECK-LABEL: fn @append
// CHECK: mcopy
import {Buffers, ByteBuilder} from "solar:core/v1/Buffers.sol";

contract Safe {
    using Buffers for ByteBuilder;

    function build(bytes memory a, bytes memory b)
        public
        pure
        returns (bytes memory out, bytes memory next)
    {
        ByteBuilder memory builder = Buffers.create(4);
        next = hex"11223344";
        builder.append(a);
        builder.append(b);
        out = builder.finish();
    }
}
