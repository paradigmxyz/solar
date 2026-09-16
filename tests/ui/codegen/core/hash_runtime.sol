//@ revisions: intrinsic portable
//@[intrinsic] compile-flags: -Ogas
//@[portable] compile-flags: -Ogas -Zno-core-intrinsics
//@ run-call: hash 0x0102030405060708, 0, 8 => 0xfec062278915ba5c3c3af6ebf470b5afc94fedadf39fe78eea427b9aa5df9692
//@ run-call: hash 0x0102030405060708, 2, 3 => 0xfe60c754eeb6f4271f086228744a2bb133832435a98f1d79b65583db7d2e406b
//@ run-call: hash 0x0102030405060708, 8, 0 => 0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
//@ run-call: hash 0x0102030405060708, 0, 0 => 0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
//@ run-call: hash 0x0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20, 0, 32 => 0x52b3f53ff196a28e7d2d01283ef9427070bda64128fb5630b97b6ab17a8ff0a8
//@ run-call-fail: hash 0x0102030405060708, 6, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call-fail: hash 0x0102030405060708, 115792089237316195423570985008687907853269984665640564039457584007913129639935, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

// `Hash.keccak256Range` hashes a range of a buffer. The shipped body copies
// the range out and hashes the copy; the intrinsic hashes it where it lies.
// Both check the range and fail the same way when it does not fit.
import {Hash} from "solar:core/v1/Hash.sol";

contract Test {
    function hash(bytes memory b, uint256 offset, uint256 count) public pure returns (bytes32) {
        return Hash.keccak256Range(b, offset, count);
    }
}
