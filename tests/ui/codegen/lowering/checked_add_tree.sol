//@ run-call: left 1, 2, 3, 4 => 10
//@ run-call: right 1, 2, 3, 4 => 10
//@ run-call-fail: left 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0, 1, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: right 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

contract CheckedAddTree {
    function left(uint256 a, uint256 b, uint256 c, uint256 d) external pure returns (uint256) {
        return a + b + c + d;
    }

    function right(uint256 a, uint256 b, uint256 c, uint256 d) external pure returns (uint256) {
        return a + (b + (c + d));
    }
}
