//@ codegen-matrix: standard
//@ run-call: u8 255 => 255
//@ run-call-fail: 0x52adca660000000000000000000000000000000000000000000000000000000000000100
//@ run-call: u16 65535 => 65535
//@ run-call-fail: 0x65f9a35b0000000000000000000000000000000000000000000000000000000000010000
//@ run-call: flag true => true
//@ run-call-fail: 0xa92a4c3b0000000000000000000000000000000000000000000000000000000000000002
//@ run-call: dir 3 => 3
//@ run-call-fail: 0x528a27a30000000000000000000000000000000000000000000000000000000000000004
//@ run-call: who 0xffffffffffffffffffffffffffffffffffffffff => 0xffffffffffffffffffffffffffffffffffffffff
//@ run-call-fail: 0x6ac3162c0000000000000000000000010000000000000000000000000000000000000000
//@ run-call: add16 65535, 0 => 65535
//@ run-call-fail: add16 65535, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: mul64 4294967296, 4294967295 => 18446744069414584320
//@ run-call-fail: mul64 4294967296, 4294967296 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: len [1, 2, 3] => 3
//@ run-call-fail: 0xececb20a000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000007fffffffffffffe => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041
//@ run-call-fail: 0xececb20a000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000007ffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041
//@ run-call-fail: 0xececb20a00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000800000000000000 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000041

// A test that a word fits in `k` bits is spelled as a shift or a comparison, whichever the
// objective prices lower. Both sides of each limit behave the same either way: the highest word
// that fits passes and the lowest that does not reverts. A decoded length at or past `2**59 - 1`
// words panics before allocating, as the allocation end passes the limit.
contract FitsInBits {
    enum Dir {
        Up,
        Down,
        Left,
        Right
    }

    function u8(uint8 x) external pure returns (uint8) {
        return x;
    }

    function u16(uint16 x) external pure returns (uint16) {
        return x;
    }

    function flag(bool b) external pure returns (bool) {
        return b;
    }

    function dir(Dir d) external pure returns (Dir) {
        return d;
    }

    function who(address a) external pure returns (address) {
        return a;
    }

    function add16(uint16 a, uint16 b) external pure returns (uint16) {
        return a + b;
    }

    function mul64(uint64 a, uint64 b) external pure returns (uint64) {
        return a * b;
    }

    function len(uint256[] memory a) external pure returns (uint256) {
        return a.length;
    }
}
