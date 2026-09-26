//@ revisions: intrinsic size portable
//@[intrinsic] compile-flags: -Ogas
//@[size] compile-flags: -Osize
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: plain 0 => "0"
//@ run-call: plain 15 => "f"
//@ run-call: plain 16 => "10"
//@ run-call: plain 4660 => "1234"
//@ run-call: prefixed 0 => "0x0"
//@ run-call: prefixed 4660 => "0x1234"
//@ run-call: prefixed 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
//@ run-call: plain 0x10000 => "10000"
//@ run-call: prefixed 0x10000 => "0x10000"
//@ run-call: plain 0xabcdef => "abcdef"
//@ run-call: prefixed 0xabcdef => "0xabcdef"
//@ run-call: plain 0x1000000 => "1000000"
//@ run-call: prefixed 0x1000000 => "0x1000000"
//@ run-call: plain 0xfffff => "fffff"
//@ run-call: prefixed 0xfffff => "0xfffff"
//@ run-call: plain 0x123456789abcdef0 => "123456789abcdef0"
//@ run-call: prefixed 0x123456789abcdef0 => "0x123456789abcdef0"
//@ run-call: plain 0x100000000000000000000000000000000000000 => "100000000000000000000000000000000000000"
//@ run-call: prefixed 0x100000000000000000000000000000000000000 => "0x100000000000000000000000000000000000000"
//@ run-call: plain 0xffffffffffffffffffffffffffffffffff => "ffffffffffffffffffffffffffffffffff"
//@ run-call: prefixed 0xffffffffffffffffffffffffffffffffff => "0xffffffffffffffffffffffffffffffffff"
//@ run-call: plain 0x100000000000000000000000000000000 => "100000000000000000000000000000000"
//@ run-call: prefixed 0x100000000000000000000000000000000 => "0x100000000000000000000000000000000"
//@ run-call: plain 0x100000000000000000000000000000042 => "100000000000000000000000000000042"
//@ run-call: prefixed 0x100000000000000000000000000000042 => "0x100000000000000000000000000000042"
//@ run-call: plain 0x8000000000000000000000000000000000000000000000000000000000000000 => "8000000000000000000000000000000000000000000000000000000000000000"
//@ run-call: prefixed 0x8000000000000000000000000000000000000000000000000000000000000000 => "0x8000000000000000000000000000000000000000000000000000000000000000"
//@ run-call: plain 0x100000000000000000000000000000000000000000000000000000000000000 => "100000000000000000000000000000000000000000000000000000000000000"
//@ run-call: prefixed 0x100000000000000000000000000000000000000000000000000000000000000 => "0x100000000000000000000000000000000000000000000000000000000000000"
//@ run-call: plain 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => "fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
//@ run-call: prefixed 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => "0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
//@ run-call: plain 0xdeadbeef00000000000000000000000000000000000000000000000000cafe => "deadbeef00000000000000000000000000000000000000000000000000cafe"
//@ run-call: prefixed 0xdeadbeef00000000000000000000000000000000000000000000000000cafe => "0xdeadbeef00000000000000000000000000000000000000000000000000cafe"
//@ run-call: plain 0xff => "ff"
//@ run-call: plain 0x100 => "100"
//@ run-call: plain 0xfff => "fff"
//@ run-call: plain 0x1000 => "1000"
//@ run-call: plain 0xffff => "ffff"

import {Strings} from "solar:core/v1/Strings.sol";

contract Safe {
    function plain(uint256 value) public pure returns (string memory) {
        return Strings.toMinimalHexStringNoPrefix(value);
    }

    function prefixed(uint256 value) public pure returns (string memory) {
        return Strings.toMinimalHexString(value);
    }
}
