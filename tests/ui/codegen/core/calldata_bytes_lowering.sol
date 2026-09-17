//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas -Zdump=mir
//@[intrinsic] filecheck: --check-prefix=INTRINSIC
//@[portable] compile-flags: -Ogas -Zdump=mir -Zno-core-intrinsics
//@[portable] filecheck: --check-prefix=PORTABLE

// A checked calldata read is one masked `calldataload`; a copy out of
// calldata is one `calldatacopy`. The shipped bodies index a byte at a time.
// INTRINSIC-LABEL: fn @read4
// INTRINSIC: calldataload
// INTRINSIC: 0xffffffff00000000000000000000000000000000000000000000000000000000
// INTRINSIC-NOT: icall @readBytes4
// INTRINSIC-LABEL: fn @copyOut
// INTRINSIC: calldatacopy
// INTRINSIC-NOT: icall @copyInto
// PORTABLE-LABEL: fn @read4
// PORTABLE: byte
import {CalldataBytes} from "solar:core/v1/CalldataBytes.sol";

contract Test {
    function read1(bytes calldata b, uint256 offset) public pure returns (bytes1) {
        return CalldataBytes.readBytes1(b, offset);
    }

    function read4(bytes calldata b, uint256 offset) public pure returns (bytes4) {
        return CalldataBytes.readBytes4(b, offset);
    }

    function read32(bytes calldata b, uint256 offset) public pure returns (bytes32) {
        return CalldataBytes.readBytes32(b, offset);
    }

    function readWord(bytes calldata b, uint256 offset) public pure returns (uint256) {
        return CalldataBytes.readUint256BE(b, offset);
    }

    function copyOut(bytes calldata src, uint256 srcOffset, uint256 count)
        public
        pure
        returns (bytes memory out)
    {
        out = new bytes(count);
        CalldataBytes.copyInto(out, 0, src, srcOffset, count);
    }

    function patch(bytes calldata src, uint256 srcOffset, uint256 count, uint256 dstOffset)
        public
        pure
        returns (bytes memory out)
    {
        out = hex"aaaaaaaaaaaa";
        CalldataBytes.copyInto(out, dstOffset, src, srcOffset, count);
    }
}
