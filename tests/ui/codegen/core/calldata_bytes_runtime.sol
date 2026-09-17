//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: read1 0x82, 0 => 0x82
//@ run-call: read1 0xb70eee7f1a50, 3 => 0x7f
//@ run-call-fail: read1 0xa99a, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: read1 0x, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: read4 0xdeadbeef01, 0 => 0xdeadbeef
//@ run-call: read4 0xdeadbeef01, 1 => 0xadbeef01
//@ run-call-fail: read4 0xdeadbeef01, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: read4 0xdeadbeef01, 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: read4 0xdeadbeef01, 115792089237316195423570985008687907853269984665640564039457584007913129639933 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: read32 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021, 0 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20
//@ run-call: read32 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021, 1 => 0x02030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021
//@ run-call-fail: read32 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: readWord 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021, 1 => 0x02030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021
//@ run-call-fail: readWord 0x0102, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: copyOut 0x0102030405, 1, 3 => 0x020304
//@ run-call: copyOut 0x0102030405, 5, 0 => 0x
//@ run-call: copyOut 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021, 0, 33 => 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2021
//@ run-call-fail: copyOut 0x0102030405, 3, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: copyOut 0x0102030405, 115792089237316195423570985008687907853269984665640564039457584007913129639935, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: patch 0x0102030405, 1, 2, 3 => 0xaaaaaa0203aa
//@ run-call-fail: patch 0x0102030405, 0, 4, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: patch 0x0102030405, 0, 1, 6 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// `CalldataBytes` is `Bytes` over a calldata slice: a read is checked against
// the slice and never padded, and a copy out of it checks both ranges before
// it writes.
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
