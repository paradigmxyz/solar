//@ codegen-matrix: standard portable
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@[mir] filecheck:
// CHECK-NOT: string-packing-runtime-diagnostic
//@ run-call: packSingle "" => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: packSingle "hello" => 0x0568656c6c6f0000000000000000000000000000000000000000000000000000
//@ run-call: packSingle "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz" => 0x1f7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a
//@ run-call: packPair "hello", "world" => 0x0568656c6c6f05776f726c640000000000000000000000000000000000000000
//@ run-call: packPair "zzzzzzzzzzzzzzz", "yyyyyyyyyyyyyyy" => 0x0f7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a0f797979797979797979797979797979
//@ run-call: packPair "", "" => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: packPair "", "hello" => 0x000568656c6c6f00000000000000000000000000000000000000000000000000
//@ run-call: packPair "hello", "" => 0x0568656c6c6f0000000000000000000000000000000000000000000000000000
//@ run-call: packPair "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "" => 0x1e61616161616161616161616161616161616161616161616161616161616100
//@ run-call: packPair "", "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" => 0x001e626262626262626262626262626262626262626262626262626262626262
//@ run-call: packPair "aaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "b" => 0x1d61616161616161616161616161616161616161616161616161616161610162
//@ run-call: packPair "aaaaaaaaaaaaaaaa", "bbbbbbbbbbbbbbb" => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: packPair "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "" => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: packPair "", "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: packPair "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "b" => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: packPair "a", "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: packShortened "helloXXXX", 5, "worldYYYY", 5 => 0x0568656c6c6f05776f726c640000000000000000000000000000000000000000
//@ run-call: packShortened "abcdefghijklmnopqrstuvwxyz0123456789", 3, "ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ", 2 => 0x03616263025a5a00000000000000000000000000000000000000000000000000
//@ run-call: packShortened "qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq", 0, "rrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrr", 30 => 0x001e727272727272727272727272727272727272727272727272727272727272
//@ run-call: packLongLater "ccccc", 400 => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: packLongLater "", 31 => 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: unpackSingle 0x0568656c6c6f0000000000000000000000000000000000000000000000000000 => "hello"
//@ run-call: unpackSingle 0x0000000000000000000000000000000000000000000000000000000000000000 => ""
//@ run-call: unpackSingle 0x2061616161616161616161616161616161616161616161616161616161616161 => "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
//@ run-call: unpackPair 0x0568656c6c6f05776f726c640000000000000000000000000000000000000000 => "hello", "world"
//@ run-call: unpackPair 0x0f7a7a7a7a7a7a7a7a7a7a7a7a7a7a7a0f797979797979797979797979797979 => "zzzzzzzzzzzzzzz", "yyyyyyyyyyyyyyy"
//@ run-call: unpackPair 0x000568656c6c6f00000000000000000000000000000000000000000000000000 => "", "hello"
//@ run-call: unpackPair 0x0568656c6c6f0000000000000000000000000000000000000000000000000000 => "hello", ""
//@ run-call: unpackPair 0x0000000000000000000000000000000000000000000000000000000000000000 => "", ""
//@ run-call: unpackPair 0x1f61616161616161616161616161616161616161616161616161616161616161 => "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", ""

import {Strings} from "solar:core/v1/Strings.sol";

contract Test {
    function packSingle(string memory value) public pure returns (bytes32) {
        return Strings.packOne(value);
    }

    function unpackSingle(bytes32 packed) public pure returns (string memory) {
        return Strings.unpackOne(packed);
    }

    function packPair(string memory a, string memory b) public pure returns (bytes32) {
        return Strings.packTwo(a, b);
    }

    /// Shortens both strings in place, leaving stale bytes after each.
    function packShortened(string memory a, uint256 aLength, string memory b, uint256 bLength)
        public
        pure
        returns (bytes32)
    {
        assembly {
            mstore(a, aLength)
            mstore(b, bLength)
        }
        return Strings.packTwo(a, b);
    }

    /// A long `a` allocated above `b`: the pair is invalid.
    function packLongLater(string memory b, uint256 aLength) public pure returns (bytes32) {
        string memory a = string(new bytes(aLength));
        return Strings.packTwo(a, b);
    }

    function unpackPair(bytes32 packed) public pure returns (string memory, string memory) {
        return Strings.unpackTwo(packed);
    }
}
