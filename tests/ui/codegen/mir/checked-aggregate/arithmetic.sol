//@ codegen-matrix: standard
//@ run-call: direct 1, 2, 3 => 6
//@ run-call: temporary 1, 2, 3 => 6
//@ run-call: mixed 4, 2, 3, 1 => 17
//@ run-call: narrow 100, 20, 2 => 240
//@ run-call: signed -10, 3, -2 => 14
//@ run-call: negated -5, 3 => 8
//@ run-call: longChain 1 => 9
//@ run-call: independent 1, 2, 3, 4 => 3, 7
//@ run-call: effectBoundary 1, 2, 3 => 6
//@ run-call-fail: narrow 250, 10, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: narrow 100, 20, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: signed 127, 1, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: signed -100, 0, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: negated -128, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: mixed 1, 1, 1, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: direct 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: direct 0, 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: direct 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: temporary 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: divisionBoundary 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: divisionBoundary 1, 2, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call-fail: effectBoundary 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

//@ run-call-fail: direct 115792089237316195423570985008687907853269984665640564039457584007913129639935, 115792089237316195423570985008687907853269984665640564039457584007913129639935, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: mixed 115792089237316195423570985008687907853269984665640564039457584007913129639935, 1, 0, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: mixed 115792089237316195423570985008687907853269984665640564039457584007913129639935, 0, 2, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: longChain 14474011154664524427946373126085988481658748083205070504932198000989141204991 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: longChain 28948022309329048855892746252171976963317496166410141009864396001978282409983 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: longChain 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

contract CheckedAggregation {
    uint256 public stored;

    function direct(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        return a + b + c;
    }

    function temporary(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        uint256 d = a + b;
        return d + c;
    }

    function mixed(uint256 a, uint256 b, uint256 c, uint256 d) external pure returns (uint256) {
        return (a + b) * c - d;
    }

    function narrow(uint8 a, uint8 b, uint8 c) external pure returns (uint8) {
        return (a + b) * c;
    }

    function signed(int8 a, int8 b, int8 c) external pure returns (int8) {
        return (a + b) * c;
    }

    function negated(int8 a, int8 b) external pure returns (int8) {
        return -a + b;
    }

    function longChain(uint256 a) external pure returns (uint256) {
        return a + a + a + a + a + a + a + a + a;
    }

    function independent(uint256 a, uint256 b, uint256 c, uint256 d) external pure returns (uint256, uint256) {
        return (a + b, c + d);
    }

    function divisionBoundary(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        return (a + b) / c + b;
    }

    function effectBoundary(uint256 a, uint256 b, uint256 c) external returns (uint256) {
        uint256 d = a + b;
        stored = d;
        require(c != 0, "boundary");
        return d + c;
    }
}
