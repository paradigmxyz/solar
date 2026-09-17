//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas -Zdump=mir
//@[intrinsic] filecheck: --check-prefix=INTRINSIC
//@[portable] compile-flags: -Ogas -Zdump=mir -Zno-core-intrinsics
//@[portable] filecheck: --check-prefix=PORTABLE

// A calldata `tryRead` is the masked `calldataload` of a read, with the range
// test turned into the flag and into a zeroed result, and no branch.
// INTRINSIC-LABEL: fn @try4
// INTRINSIC: calldataload
// INTRINSIC: 0xffffffff00000000000000000000000000000000000000000000000000000000
// INTRINSIC-NOT: icall @tryReadBytes4
// PORTABLE-LABEL: fn @try4
// PORTABLE: icall @tryReadBytes4
import {CalldataBytes} from "solar:core/v1/CalldataBytes.sol";

contract Test {
    function try1(bytes calldata b, uint256 offset) public pure returns (bool, bytes1) {
        return CalldataBytes.tryReadBytes1(b, offset);
    }

    function try4(bytes calldata b, uint256 offset) public pure returns (bool, bytes4) {
        return CalldataBytes.tryReadBytes4(b, offset);
    }

    function try32(bytes calldata b, uint256 offset) public pure returns (bool, bytes32) {
        return CalldataBytes.tryReadBytes32(b, offset);
    }

    function tryWord(bytes calldata b, uint256 offset) public pure returns (bool, uint256) {
        return CalldataBytes.tryReadUint256BE(b, offset);
    }

    // A failed read must not see the calldata that follows the slice.
    function tryTwo(bytes calldata b, bytes calldata next, uint256 offset)
        public
        pure
        returns (bool, bytes4)
    {
        next;
        return CalldataBytes.tryReadBytes4(b, offset);
    }
}
