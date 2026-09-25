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
//@ run-call: reversed 0x010203 => 0x030201

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
}
