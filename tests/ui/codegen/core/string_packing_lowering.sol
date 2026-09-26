//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas -Zdump=mir
//@[intrinsic] filecheck: --check-prefix=INTRINSIC
//@[portable] compile-flags: -Ogas -Zdump=mir -Zno-core-intrinsics
//@[portable] filecheck: --check-prefix=PORTABLE

import {Strings} from "solar:core/v1/Strings.sol";

contract Test {
    // The intrinsic packs each string from one word load. The portable body is
    // the checked byte loop that defines the same operation.
    // INTRINSIC-LABEL: fn @packPair
    // INTRINSIC: mload
    // INTRINSIC: mload
    // INTRINSIC-NOT: icall @packTwo
    // PORTABLE-LABEL: fn @packPair
    // PORTABLE: phi
    // PORTABLE: byte 0
    function packPair(string memory a, string memory b) public pure returns (bytes32) {
        return Strings.packTwo(a, b);
    }

    // INTRINSIC-LABEL: fn @unpackPair
    // INTRINSIC: byte 0
    // INTRINSIC: [[FMP:v[0-9]+]] = mload 64
    // INTRINSIC-NEXT: [[END:v[0-9]+]] = add [[FMP]], 128
    // INTRINSIC-NEXT: mstore 64, [[END]]
    // INTRINSIC: mstore
    // INTRINSIC: byte 0, {{v[0-9]+}}
    // INTRINSIC: mstore
    // INTRINSIC-NOT: icall @unpackTwo
    // INTRINSIC-NOT: mcopy
    // PORTABLE-LABEL: fn @unpackPair
    // PORTABLE: icall @unpackTwo
    function unpackPair(bytes32 packed) public pure returns (string memory, string memory) {
        return Strings.unpackTwo(packed);
    }
}
