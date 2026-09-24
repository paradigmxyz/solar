//@ revisions: intrinsic size portable
//@[intrinsic] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: plainFixed 0x0, 0 => ""
//@ run-call: prefixedFixed 0x0, 0 => "0x"
//@ run-call: plainFixed 0x0, 1 => "00"
//@ run-call: prefixedFixed 0x0, 1 => "0x00"
//@ run-call: plainFixed 0x12, 1 => "12"
//@ run-call: prefixedFixed 0x12, 1 => "0x12"
//@ run-call: plainFixed 0x1234, 2 => "1234"
//@ run-call: prefixedFixed 0x1234, 2 => "0x1234"
//@ run-call: plainFixed 0xffff, 2 => "ffff"
//@ run-call: prefixedFixed 0xffff, 2 => "0xffff"
//@ run-call: plainFixed 0xabcdef, 3 => "abcdef"
//@ run-call: prefixedFixed 0xabcdef, 3 => "0xabcdef"
//@ run-call: plainFixed 0x1, 15 => "000000000000000000000000000001"
//@ run-call: prefixedFixed 0x1, 15 => "0x000000000000000000000000000001"
//@ run-call: plainFixed 0x1, 16 => "00000000000000000000000000000001"
//@ run-call: prefixedFixed 0x1, 16 => "0x00000000000000000000000000000001"
//@ run-call: plainFixed 0xabcdef, 17 => "0000000000000000000000000000abcdef"
//@ run-call: prefixedFixed 0xabcdef, 17 => "0x0000000000000000000000000000abcdef"
//@ run-call: plainFixed 0xdeadbeef0000000000000000000000000, 20 => "0000000deadbeef0000000000000000000000000"
//@ run-call: prefixedFixed 0xdeadbeef0000000000000000000000000, 20 => "0x0000000deadbeef0000000000000000000000000"
//@ run-call: plainFixed 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 31 => "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
//@ run-call: prefixedFixed 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 31 => "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
//@ run-call: plainFixed 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 32 => "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
//@ run-call: prefixedFixed 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 32 => "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
//@ run-call: plainFixed 0xab, 33 => "0000000000000000000000000000000000000000000000000000000000000000ab"
//@ run-call: prefixedFixed 0xab, 33 => "0x0000000000000000000000000000000000000000000000000000000000000000ab"
//@ run-call: plainFixed 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 40 => "0000000000000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
//@ run-call: prefixedFixed 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 40 => "0x0000000000000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
//@ run-call: plainFixed 0x8000000000000000000000000000000000000000000000000000000000000000, 64 => "00000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000"
//@ run-call: prefixedFixed 0x8000000000000000000000000000000000000000000000000000000000000000, 64 => "0x00000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000"
//@ run-call-fail: plainFixed 0x1, 0 => 0x2194895a
//@ run-call-fail: prefixedFixed 0x1, 0 => 0x2194895a
//@ run-call-fail: plainFixed 0x100, 1 => 0x2194895a
//@ run-call-fail: prefixedFixed 0x100, 1 => 0x2194895a
//@ run-call-fail: plainFixed 0x10000, 2 => 0x2194895a
//@ run-call-fail: prefixedFixed 0x10000, 2 => 0x2194895a
//@ run-call-fail: plainFixed 0x1000000, 3 => 0x2194895a
//@ run-call-fail: prefixedFixed 0x1000000, 3 => 0x2194895a
//@ run-call-fail: plainFixed 0x100000000000000000000000000000000, 16 => 0x2194895a
//@ run-call-fail: prefixedFixed 0x100000000000000000000000000000000, 16 => 0x2194895a
//@ run-call-fail: plainFixed 0x100000000000000000000000000000000000000000000000000000000000000, 31 => 0x2194895a
//@ run-call-fail: prefixedFixed 0x100000000000000000000000000000000000000000000000000000000000000, 31 => 0x2194895a
//@ run-call: plain 0x0 => "00"
//@ run-call: prefixed 0x0 => "0x00"
//@ run-call: plain 0x1 => "01"
//@ run-call: prefixed 0x1 => "0x01"
//@ run-call: plain 0xf => "0f"
//@ run-call: prefixed 0xf => "0x0f"
//@ run-call: plain 0xff => "ff"
//@ run-call: prefixed 0xff => "0xff"
//@ run-call: plain 0x100 => "0100"
//@ run-call: prefixed 0x100 => "0x0100"
//@ run-call: plain 0x101 => "0101"
//@ run-call: prefixed 0x101 => "0x0101"
//@ run-call: plain 0xffff => "ffff"
//@ run-call: prefixed 0xffff => "0xffff"
//@ run-call: plain 0x800000000000 => "800000000000"
//@ run-call: prefixed 0x800000000000 => "0x800000000000"
//@ run-call: plain 0xffffffffffffffffffffffffffffffff => "ffffffffffffffffffffffffffffffff"
//@ run-call: prefixed 0xffffffffffffffffffffffffffffffff => "0xffffffffffffffffffffffffffffffff"
//@ run-call: plain 0x100000000000000000000000000000000 => "0100000000000000000000000000000000"
//@ run-call: prefixed 0x100000000000000000000000000000000 => "0x0100000000000000000000000000000000"
//@ run-call: plain 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
//@ run-call: prefixed 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
//@ run-call: plain 0x10000 => "010000"
//@ run-call: prefixed 0x10000 => "0x010000"
//@ run-call: plain 0xabcdef => "abcdef"
//@ run-call: prefixed 0xabcdef => "0xabcdef"
//@ run-call: plain 0x1000000 => "01000000"
//@ run-call: prefixed 0x1000000 => "0x01000000"
//@ run-call: plain 0xfffff => "0fffff"
//@ run-call: prefixed 0xfffff => "0x0fffff"
//@ run-call: plain 0x123456789abcdef0 => "123456789abcdef0"
//@ run-call: prefixed 0x123456789abcdef0 => "0x123456789abcdef0"
//@ run-call: plain 0x100000000000000000000000000000000000000 => "0100000000000000000000000000000000000000"
//@ run-call: prefixed 0x100000000000000000000000000000000000000 => "0x0100000000000000000000000000000000000000"
//@ run-call: plain 0xffffffffffffffffffffffffffffffffff => "ffffffffffffffffffffffffffffffffff"
//@ run-call: prefixed 0xffffffffffffffffffffffffffffffffff => "0xffffffffffffffffffffffffffffffffff"
//@ run-call: plain 0x100000000000000000000000000000000 => "0100000000000000000000000000000000"
//@ run-call: prefixed 0x100000000000000000000000000000000 => "0x0100000000000000000000000000000000"
//@ run-call: plain 0x100000000000000000000000000000042 => "0100000000000000000000000000000042"
//@ run-call: prefixed 0x100000000000000000000000000000042 => "0x0100000000000000000000000000000042"
//@ run-call: plain 0x8000000000000000000000000000000000000000000000000000000000000000 => "8000000000000000000000000000000000000000000000000000000000000000"
//@ run-call: prefixed 0x8000000000000000000000000000000000000000000000000000000000000000 => "0x8000000000000000000000000000000000000000000000000000000000000000"
//@ run-call: plain 0x100000000000000000000000000000000000000000000000000000000000000 => "0100000000000000000000000000000000000000000000000000000000000000"
//@ run-call: prefixed 0x100000000000000000000000000000000000000000000000000000000000000 => "0x0100000000000000000000000000000000000000000000000000000000000000"
//@ run-call: plain 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => "0fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
//@ run-call: prefixed 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => "0x0fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
//@ run-call: plain 0xdeadbeef00000000000000000000000000000000000000000000000000cafe => "deadbeef00000000000000000000000000000000000000000000000000cafe"
//@ run-call: prefixed 0xdeadbeef00000000000000000000000000000000000000000000000000cafe => "0xdeadbeef00000000000000000000000000000000000000000000000000cafe"

// Fixed widths spell the low `byteCount` bytes, two digits each, and revert
// with `HexLengthInsufficient()` when the value does not fit; the one-argument
// forms spell the fewest whole bytes, at least one. Both lowering and the
// checked portable bodies must agree at every width class.
import {Strings} from "solar:core/v1/Strings.sol";

contract Safe {
    function plainFixed(uint256 value, uint256 byteCount) public pure returns (string memory) {
        return Strings.toHexStringNoPrefix(value, byteCount);
    }

    function prefixedFixed(uint256 value, uint256 byteCount) public pure returns (string memory) {
        return Strings.toHexString(value, byteCount);
    }

    function plain(uint256 value) public pure returns (string memory) {
        return Strings.toHexStringNoPrefix(value);
    }

    function prefixed(uint256 value) public pure returns (string memory) {
        return Strings.toHexString(value);
    }
}
