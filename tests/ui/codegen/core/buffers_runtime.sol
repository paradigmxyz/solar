//@ codegen-matrix: standard
//@ run-call: concat 0x0102, 0x030405, 8 => 0x0102030405
//@ run-call: concat 0x0102, 0x030405, 0 => 0x0102030405
//@ run-call: concat 0x, 0x, 0 => 0x
//@ run-call: concat 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021, 0xa1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebf, 4 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebf
//@ run-call: counted 0 => 0x, 0
//@ run-call: counted 5 => 0x0001020304, 5
//@ run-call: counted 40 => 0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021222324252627, 40
//@ run-call: neighbour 0xaaaaaaaa, 0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb0000000000000000000000000000000000000000000000000000000000000004deadbeef => 0xaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb0000000000000000000000000000000000000000000000000000000000000004deadbeef, 0x11223344

// `Buffers` builds output whose length is not known up front. The capacity is
// a hint: an append that does not fit moves the builder instead of writing
// past it, so the allocation that follows the builder keeps its bytes.
// `finish` hands the bytes over and leaves the builder empty; using a builder
// after that is rejected, as `buffers_finish.sol` shows.
import {Buffers, ByteBuilder} from "solar:core/v1/Buffers.sol";

contract Test {
    using Buffers for ByteBuilder;

    function concat(bytes memory a, bytes memory b, uint256 capacity) public pure returns (bytes memory) {
        ByteBuilder memory builder = Buffers.create(capacity);
        builder.append(a);
        builder.append(b);
        return builder.finish();
    }

    function counted(uint256 n) public pure returns (bytes memory out, uint256 length) {
        ByteBuilder memory builder = Buffers.create(2);
        for (uint256 i; i < n; ++i) {
            builder.appendByte(bytes1(uint8(i)));
        }
        length = builder.length();
        out = builder.finish();
    }

    function neighbour(bytes memory a, bytes memory b)
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
