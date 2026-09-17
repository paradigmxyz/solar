//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: try4 0xdeadbeef01, 0 => true, 0xdeadbeef
//@ run-call: try4 0xdeadbeef01, 1 => true, 0xadbeef01
//@ run-call: try4 0xdeadbeef01, 2 => false, 0x00000000
//@ run-call: try4 0xdeadbeef01, 5 => false, 0x00000000
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
//@ run-call: guarded 0xdeadbeef01, 2 => 0x11223344

// `tryReadBytesN` is the read that answers instead of reverting: `false` and
// zero when the width does not lie inside the buffer, whatever follows it in
// memory.
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
