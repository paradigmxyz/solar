//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: plain 0 => "0"
//@ run-call: plain 15 => "f"
//@ run-call: plain 16 => "10"
//@ run-call: plain 4660 => "1234"
//@ run-call: prefixed 0 => "0x0"
//@ run-call: prefixed 4660 => "0x1234"
//@ run-call: prefixed 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"

import {Strings} from "solar:core/v1/Strings.sol";

contract Safe {
    function plain(uint256 value) public pure returns (string memory) {
        return Strings.toMinimalHexStringNoPrefix(value);
    }

    function prefixed(uint256 value) public pure returns (string memory) {
        return Strings.toMinimalHexString(value);
    }
}
