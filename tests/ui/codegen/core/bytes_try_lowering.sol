//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas -Zdump=mir
//@[intrinsic] filecheck: --check-prefix=INTRINSIC
//@[portable] compile-flags: -Ogas -Zdump=mir -Zno-core-intrinsics
//@[portable] filecheck: --check-prefix=PORTABLE

// A `tryRead` is the same masked `mload` as a read, with the range test
// turned into the flag and into a zeroed result, and no branch.
// INTRINSIC-LABEL: fn @try4
// INTRINSIC: mload
// INTRINSIC: 0xffffffff00000000000000000000000000000000000000000000000000000000
// INTRINSIC-NOT: icall @tryReadBytes4
// PORTABLE-LABEL: fn @try4
// PORTABLE: icall @tryReadBytes4
import {Bytes} from "solar:core/v1/Bytes.sol";

contract Test {
    function try1(bytes memory b, uint256 offset) public pure returns (bool, bytes1) {
        return Bytes.tryReadBytes1(b, offset);
    }

    function try4(bytes memory b, uint256 offset) public pure returns (bool, bytes4) {
        return Bytes.tryReadBytes4(b, offset);
    }

    function try32(bytes memory b, uint256 offset) public pure returns (bool, bytes32) {
        return Bytes.tryReadBytes32(b, offset);
    }

    function tryWord(bytes memory b, uint256 offset) public pure returns (bool, uint256) {
        return Bytes.tryReadUint256BE(b, offset);
    }

    // A failed read must not see the allocation that follows the buffer.
    function guarded(bytes memory b, uint256 offset) public pure returns (bytes4) {
        bytes memory next = hex"11223344";
        (bool ok, bytes4 value) = Bytes.tryReadBytes4(b, offset);
        return ok ? value : Bytes.readBytes4(next, 0);
    }
}
