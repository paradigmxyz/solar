//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: try4 0xdeadbeef01, 0 => true, 0xdeadbeef
//@ run-call: try4 0xdeadbeef01, 1 => true, 0xadbeef01
//@ run-call: try4 0xdeadbeef01, 2 => false, 0x00000000
//@ run-call: try4 0xdeadbeef01, 6 => false, 0x00000000
//@ run-call: try4 0xdeadbeef01, 115792089237316195423570985008687907853269984665640564039457584007913129639935 => false, 0x00000000
//@ run-call: try4 0xdeadbeef01, 115792089237316195423570985008687907853269984665640564039457584007913129639933 => false, 0x00000000
//@ run-call: try4 0x, 0 => false, 0x00000000
//@ run-call: try1 0x7f, 0 => true, 0x7f
//@ run-call: try1 0x7f, 1 => false, 0x00
//@ run-call: try32 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021, 1 => true, 0x02030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021
//@ run-call: try32 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021, 2 => false, 0x0000000000000000000000000000000000000000000000000000000000000000
//@ run-call: tryWord 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021, 0 => true, 455867356320691211509944977504407603390036387149619137164185182714736811808
//@ run-call: tryWord 0x0102, 0 => false, 0
//@ run-call: tryTwo 0xdeadbeef01, 0xcafe, 3 => false, 0x00000000

// `CalldataBytes.tryReadBytesN` answers instead of reverting: `false` and zero
// when the width does not lie inside the slice, whatever calldata follows it.
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
