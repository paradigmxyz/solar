//@ revisions: intrinsic portable size
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[size] compile-flags: -Osize
//@ run-call: ofObject 0x00112233 => 0x94adf24644fa29e7241fa1c04e2541c8aa8240b4f4898fb0343d3a8dc4f85170, 4, 0x00
//@ run-call: ofObject 0x => 0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470, 0, 0x00
//@ run-call: ofView 0x00112233 => 0x50fceab2fe7ed15023d21b343e098d8a822f44ed61ba7e988e708db9c68c2535, 3, 0x11
//@ run-call: ofCalldata 0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000004aabbccdd00000000000000000000000000000000000000000000000000000000 => 0x40eed0325a12c6c6af8db2ea05450bfe21d6343b6fe955bff65045b67d9d5fe6, 4, 0xaa
//@ run-call: ofLiteral => 0x1c8aff950685c2ed4bc3174f3472287b56d9517b9c948127319a09a7a36deac8, 5, 0x68
//@ run-call: ofStrings "hi", 0x010203 => 0x7624778dedc75f8b322b9fa1632a610d40b85e106c7d9bf0e743a9ce291b9c6f, 5
//@ run-call: headerOf 0xa9059cbb0102 => 0xa9059cbb, 0x22ae6da6b482f9b1b19b0b897c3fd43884180a1c5ee361e1107a1bc635649dda
//@ run-call-fail: headerOf 0xa9059c => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: counted 0x01020101 => 3
//@ run-call: shortenedByLaterArgument 0x00112233 => 0xcac1bb71f0a97c8ac94ca9546b43178a9ad254c7b757ac07433aa6df35cd8089, 2
//@ run-call: reversed 0x010203 => 0x030201
//@ run-call: mixedCalldata 0x000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000003010203000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000037461670000000000000000000000000000000000000000000000000000000000, 0x0405 => 0xe3cbd67d0e6118092819507a8c550916c12e7bf5eaa8681ee6ddb32ca9206528, 6005005
//@ run-call-fail: mixedCalldata 0x000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000ff0000000000000000000000000000000000000000000000000000000000000003010203000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000037461670000000000000000000000000000000000000000000000000000000000, 0x0405 => 0x
//@ run-call: nestedCalldata 0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000009deadbeef01010102030000000000000000000000000000000000000000000000 => 0x6139b7a7cc8b9a479c9e6516b14672c33c6bd0ace68993b0162d9cb9cb83a0e2, 832, 0xdeadbeef, 0xbde0928d821e4d8a6db3cc20d49de60ecb1f3385ded89c297ef8d5540ce04a64, 3
//@ run-call-fail: nestedCalldata 0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000002dead000000000000000000000000000000000000000000000000000000000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: elementCalldata 0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000e0000000000000000000000000000000000000000000000000000000000000000111000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002223300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000034455660000000000000000000000000000000000000000000000000000000000 => 0x9d8b800494b54340a06546d4801181c6cf6b4fe1d322f55416e2b0b62da2af34, 3, 0x44

// A parameter named by `@custom:solar-view` on an internal function is a view:
// callers pass a view as it is and a `bytes` object as the slice of its bytes,
// with no copy, and the function reads it in place. Other compilers pass the
// object, which the `portable` revision runs, with the same results.
import {Bytes} from "solar:core/v1/Bytes.sol";

library Count {
    /// @custom:solar-view b
    function count(bytes memory b, bytes1 x) internal pure returns (uint256 n) {
        for (uint256 i; i < b.length; ++i) {
            if (b[i] == x) ++n;
        }
    }
}

contract Test {
    using Bytes for bytes;
    using Count for bytes;

    /// @custom:solar-view data
    function checksum(bytes memory data)
        internal
        pure
        returns (bytes32 hash, uint256 length, bytes1 first)
    {
        hash = keccak256(data);
        length = data.length;
        if (length != 0) first = data[0];
    }

    /// @custom:solar-view data tag
    function mixed(bytes memory data, string memory tag) internal pure returns (bytes32, uint256) {
        return (keccak256(bytes(tag)), data.length + bytes(tag).length);
    }

    // A view parameter reads in place like any view, a view of it included.
    /// @custom:solar-view data
    function header(bytes memory data) internal pure returns (bytes4 tag, bytes32 body) {
        tag = data.readBytes4(0);
        /// @custom:solar-view
        bytes memory rest = Bytes.slice(data, 4, data.length - 4);
        body = keccak256(rest);
    }

    function ofObject(bytes memory b) public pure returns (bytes32, uint256, bytes1) {
        return checksum(b);
    }

    function ofView(bytes memory b) public pure returns (bytes32, uint256, bytes1) {
        /// @custom:solar-view
        bytes memory v = Bytes.slice(b, 1, b.length - 1);
        return checksum(v);
    }

    function ofCalldata(bytes calldata data) external pure returns (bytes32, uint256, bytes1) {
        /// @custom:solar-view
        (bytes memory v) = abi.decode(data, (bytes));
        return checksum(v);
    }

    function ofLiteral() public pure returns (bytes32, uint256, bytes1) {
        return checksum("hello");
    }

    function ofStrings(string memory s, bytes memory b) public pure returns (bytes32, uint256) {
        return mixed(b, s);
    }

    function headerOf(bytes memory b) public pure returns (bytes4, bytes32) {
        return header(b);
    }

    function counted(bytes memory b) public pure returns (uint256) {
        return b.count(0x01);
    }

    function shortenedTo(bytes memory data, uint256 length) internal pure returns (string memory) {
        assembly {
            mstore(data, length)
        }
        return "t";
    }

    // A later argument shortens the object, and the function reads the bytes the object holds
    // at the call, as it does when passed the object.
    function shortenedByLaterArgument(bytes memory b) public pure returns (bytes32, uint256) {
        return mixed(b, shortenedTo(b, 1));
    }

    // Writing a fresh object while the view parameter is read is fine.
    /// @custom:solar-view data
    function copyOut(bytes memory data) internal pure returns (bytes memory out) {
        out = new bytes(data.length);
        for (uint256 i; i < data.length; ++i) {
            out[i] = data[data.length - 1 - i];
        }
    }

    function reversed(bytes memory b) public pure returns (bytes memory) {
        return copyOut(b);
    }

    // A view of calldata is passed as it is, to a copy of the function that reads
    // its parameter in calldata; so are calldata views in any position.
    function mixedCalldata(bytes calldata data, bytes memory b)
        external
        pure
        returns (bytes32 h, uint256 n)
    {
        /// @custom:solar-view
        (bytes memory v, string memory s) = abi.decode(data, (bytes, string));
        (h, n) = mixed(v, s);
        (bytes32 x, uint256 y) = mixed(b, s);
        (h, n) = (keccak256(abi.encode(h, x)), n * 1000 + y);
        (x, y) = mixed(v, string(b));
        (h, n) = (keccak256(abi.encode(h, x)), n * 1000 + y);
    }

    /// @custom:solar-view data
    function sumDown(bytes memory data, uint256 i) internal pure returns (uint256) {
        if (i == 0) return 0;
        return uint8(data[i - 1]) + sumDown(data, i - 1);
    }

    /// @custom:solar-view data
    function outer(bytes memory data) internal pure returns (bytes32 hash, uint256 total) {
        (hash,,) = checksum(data);
        total = sumDown(data, data.length);
    }

    // A copy that passes its calldata parameter on calls a copy in turn,
    // itself included.
    function nestedCalldata(bytes calldata data)
        external
        pure
        returns (bytes32, uint256, bytes4, bytes32, uint256)
    {
        /// @custom:solar-view
        (bytes memory v) = abi.decode(data, (bytes));
        (bytes32 hash, uint256 total) = outer(v);
        (bytes4 tag, bytes32 body) = header(v);
        return (hash, total, tag, body, v.count(0x01));
    }

    // An element of a view of calldata is a view of calldata.
    function elementCalldata(bytes calldata data) external pure returns (bytes32, uint256, bytes1) {
        /// @custom:solar-view
        (bytes[] memory items) = abi.decode(data, (bytes[]));
        return checksum(items[items.length - 1]);
    }
}
