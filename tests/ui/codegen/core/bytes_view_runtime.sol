//@ revisions: intrinsic portable size
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[size] compile-flags: -Osize
//@ run-call: header 0x01a9059cbbfeed => 1, 0xa9059cbb, 0xf677149710826c7b4deb651d3e14cdb7645dc2fc876de193ad6f01bd7948796e, 2
//@ run-call: header 0x01a9059cbb => 1, 0xa9059cbb, 0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470, 0
//@ run-call-fail: header 0x01a9059c => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: viewed 0x00112233445566, 2, 3 => 3, 0xc5fbfe21ffccce605b5855129e69312921520a47ee1b268760df076b0c598a28, 0x22
//@ run-call: viewed 0x00112233445566, 7, 0 => 0, 0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470, 0x00
//@ run-call-fail: viewed 0x00112233445566, 5, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: viewed 0x00112233445566, 115792089237316195423570985008687907853269984665640564039457584007913129639935, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: sliced 0x00112233445566, 2, 3 => 0x223344
//@ run-call: sliced 0x00112233445566, 7, 0 => 0x
//@ run-call-fail: sliced 0x00112233445566, 6, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: nested 0x00112233445566, 0 => 0x22
//@ run-call: nested 0x00112233445566, 1 => 0x33
//@ run-call-fail: nested 0x00112233445566, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: nested 0x0011, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: copied 0x00112233445566 => 0xff2233, 0x00112233445566
//@ run-call: tryRead 0xaabb0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20cc, 0 => true, 455867356320691211509944977504407603390036387149619137164185182714736811808
//@ run-call: tryRead 0xaabb0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20cc, 1 => true, 909953980780754722974929232440438614579330444661935074573822767059494183116
//@ run-call: tryRead 0xaabb0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20cc, 2 => false, 0
//@ run-call: equalsIn 0x00112233, 0x1122 => true, true
//@ run-call: equalsIn 0x00112233, 0x1123 => false, true
//@ run-call-fail: equalsIn 0x00112233, 0x11223344 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: copyOut 0x0011223344 => 0x441122334400
//@ run-call: words 0x0700000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002ff => [1, 2]
//@ run-call: words 0x07 => []
//@ run-call: twice 0xaabb => true
//@ run-call-fail: twice 0xaacc
//@ run-call-fail: twice 0xaa => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: rewrite 0x0102030400 => 0x0203040500
//@ run-call: local 7 => 0x07, 0x07
//@ run-call: allocAfterCall 0xaabb => 0xbb, 0xaa000000
//@ run-call: writeParam 0xaabb => 0x22, 0x01bb

// `@custom:solar-view` reads a `Bytes.slice` range in place, and must behave
// exactly like the copy the portable body makes, which is what the `portable`
// revision runs.
import {Bytes} from "solar:core/v1/Bytes.sol";

contract Test {
    using Bytes for bytes;

    // A packet's header fields and the hash of its body.
    function header(bytes memory packet)
        public
        pure
        returns (uint8 version, bytes4 tag, bytes32 hash, uint256 length)
    {
        /// @custom:solar-view
        bytes memory head = Bytes.slice(packet, 0, 5);
        version = uint8(head[0]);
        tag = head.readBytes4(1);
        /// @custom:solar-view
        bytes memory body = packet.slice(5, packet.length - 5);
        hash = keccak256(body);
        length = body.length;
    }

    function viewed(bytes memory b, uint256 offset, uint256 count)
        public
        pure
        returns (uint256 length, bytes32 hash, bytes1 first)
    {
        /// @custom:solar-view
        bytes memory v = b.slice(offset, count);
        length = v.length;
        hash = keccak256(v);
        if (count != 0) first = v[0];
    }

    // Without the tag, a slice is a copy.
    function sliced(bytes memory b, uint256 offset, uint256 count)
        public
        pure
        returns (bytes memory)
    {
        return b.slice(offset, count);
    }

    // A view of a view narrows the same bytes, and indexing past its end fails
    // like any index.
    function nested(bytes memory b, uint256 at) public pure returns (bytes1) {
        /// @custom:solar-view
        bytes memory outer = b.slice(1, b.length - 1);
        /// @custom:solar-view
        bytes memory inner = outer.slice(1, 2);
        return inner[at];
    }

    // An untagged slice of a view is a copy of its own.
    function copied(bytes memory b) public pure returns (bytes memory copy, bytes memory original) {
        /// @custom:solar-view
        bytes memory v = b.slice(1, 3);
        copy = v.slice(0, v.length);
        copy[0] = 0xff;
        original = b;
    }

    function tryRead(bytes memory b, uint256 at) public pure returns (bool ok, uint256 word) {
        /// @custom:solar-view
        bytes memory v = b.slice(2, b.length - 2);
        (ok, word) = v.tryReadUint256BE(at);
    }

    function equalsIn(bytes memory b, bytes memory needle)
        public
        pure
        returns (bool inView, bool viewInB)
    {
        /// @custom:solar-view
        bytes memory v = b.slice(1, b.length - 1);
        inView = v.equalsAt(0, needle);
        viewInB = b.equalsAt(1, v);
    }

    // Copying out of a view writes only fresh memory, so the view is read on.
    function copyOut(bytes memory b) public pure returns (bytes memory out) {
        /// @custom:solar-view
        bytes memory v = b.slice(1, 4);
        out = new bytes(6);
        Bytes.copyInto(out, 1, v, 0, 4);
        out[0] = v[3];
    }

    // A loop reads through a view made before it and writes a fresh array.
    function words(bytes memory b) public pure returns (uint256[] memory out) {
        /// @custom:solar-view
        bytes memory body = b.slice(1, b.length - 1);
        out = new uint256[](body.length / 32);
        for (uint256 i; i < out.length; ++i) {
            out[i] = body.readUint256BE(i * 32);
        }
    }

    // Each instance of a modifier reads its own view after the body runs.
    modifier expect(bytes memory b, uint256 at, bytes1 expected) {
        /// @custom:solar-view
        bytes memory v = b.slice(at, 1);
        _;
        require(v[0] == expected);
    }

    function twice(bytes memory b) public pure expect(b, 0, 0xaa) expect(b, 1, 0xbb) returns (bool) {
        return true;
    }

    // A view made inside a loop is made again on every iteration, so the write
    // at the start of an iteration comes before that iteration's view.
    function rewrite(bytes memory b) public pure returns (bytes memory) {
        for (uint256 i; i + 1 < b.length; ++i) {
            b[i] = bytes1(uint8(b[i]) + 1);
            /// @custom:solar-view
            bytes memory pair = b.slice(i, 2);
            if (pair[1] == 0) break;
        }
        return b;
    }

    // A view of a fresh object, next to a write of another one.
    function local(uint8 x) public pure returns (bytes1 first, bytes1 copy) {
        bytes memory data = new bytes(40);
        data[5] = bytes1(x);
        /// @custom:solar-view
        bytes memory v = data.slice(4, 4);
        bytes memory other = new bytes(4);
        other[0] = v[1];
        first = v[1];
        copy = other[0];
    }

    function next(uint256 x) internal pure returns (uint256) {
        return x + 1;
    }

    // A call that cannot reset the free memory pointer keeps later
    // allocations fresh.
    function allocAfterCall(bytes memory b) public pure returns (bytes1, bytes memory out) {
        /// @custom:solar-view
        bytes memory v = b.slice(0, 2);
        out = new bytes(next(3));
        out[0] = v[0];
        return (v[1], out);
    }

    // A checked write of a parameter's object stays below a fresh object.
    function writeParam(bytes memory p) public pure returns (bytes1 first, bytes memory) {
        bytes memory data = new bytes(4);
        data[1] = 0x22;
        /// @custom:solar-view
        bytes memory v = data.slice(0, 2);
        p.writeBytes1(0, 0x01);
        first = v[1];
        return (first, p);
    }
}
