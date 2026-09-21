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

//@ run-call: tree 1, 2, 3, 4 => 10
//@ run-call: reused 1, 2, 3 => 9
//@ run-call: subtract 20, 3, 2 => 15
//@ run-call: narrowRecovered 100, 20, 10 => 110
//@ run-call: signedRecovered 100, 20, 10 => 110
//@ run-call: multiply 2, 3, 4 => 24
//@ run-call-fail: tree 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1, 2, 3 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: tree 1, 2, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: subtract 0, 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: narrowRecovered 250, 10, 10 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: signedRecovered 120, 10, 10 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: multiply 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: remainderBoundary 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: remainderBoundary 1, 2, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call-fail: reverseDivisionBoundary 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call-fail: powerBoundary 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

//@ run-call: uint128Chain 10, 2, 3 => 9
//@ run-call: uint128Chain 340282366920938463463374607431768211455, 0, 0 => 340282366920938463463374607431768211455
//@ run-call-fail: uint128Chain 340282366920938463463374607431768211455, 1, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: int128Chain 10, 2, 3 => 9
//@ run-call: int128Chain 170141183460469231731687303715884105727, 0, 0 => 170141183460469231731687303715884105727
//@ run-call-fail: int128Chain 170141183460469231731687303715884105727, 1, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: int128Chain -170141183460469231731687303715884105728, -1, -1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: uint248Chain 10, 2, 3 => 9
//@ run-call: uint248Chain 452312848583266388373324160190187140051835877600158453279131187530910662655, 0, 0 => 452312848583266388373324160190187140051835877600158453279131187530910662655
//@ run-call-fail: uint248Chain 452312848583266388373324160190187140051835877600158453279131187530910662655, 1, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: int248Chain 10, 2, 3 => 9
//@ run-call: int248Chain 226156424291633194186662080095093570025917938800079226639565593765455331327, 0, 0 => 226156424291633194186662080095093570025917938800079226639565593765455331327
//@ run-call-fail: int248Chain 226156424291633194186662080095093570025917938800079226639565593765455331327, 1, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: int248Chain -226156424291633194186662080095093570025917938800079226639565593765455331328, -1, -1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: uint256Chain 10, 2, 3 => 9
//@ run-call: uint256Chain 115792089237316195423570985008687907853269984665640564039457584007913129639935, 0, 0 => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call-fail: uint256Chain 115792089237316195423570985008687907853269984665640564039457584007913129639935, 1, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: int256Chain 10, 2, 3 => 9
//@ run-call: int256Chain 57896044618658097711785492504343953926634992332820282019728792003956564819967, 0, 0 => 57896044618658097711785492504343953926634992332820282019728792003956564819967
//@ run-call-fail: int256Chain 57896044618658097711785492504343953926634992332820282019728792003956564819967, 1, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: int256Chain -57896044618658097711785492504343953926634992332820282019728792003956564819968, -1, -1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

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

    function tree(uint256 a, uint256 b, uint256 c, uint256 d) external pure returns (uint256) {
        return (a + b) + (c + d);
    }

    function reused(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        uint256 x = a + b;
        return (c + x) + x;
    }

    function subtract(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        return a - b - c;
    }

    function narrowRecovered(uint8 a, uint8 b, uint8 c) external pure returns (uint8) {
        return a + b - c;
    }

    function signedRecovered(int8 a, int8 b, int8 c) external pure returns (int8) {
        return a + b - c;
    }

    function multiply(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        return a * b * c;
    }

    function remainderBoundary(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        return (a + b) % c + b;
    }

    function reverseDivisionBoundary(uint256 a, uint256 b) external pure returns (uint256) {
        return a / b + a + a;
    }

    function powerBoundary(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        return (a + b) ** c + b;
    }

    function uint128Chain(uint128 a, uint128 b, uint128 c) external pure returns (uint128) {
        return a + b - c;
    }

    function int128Chain(int128 a, int128 b, int128 c) external pure returns (int128) {
        return a + b - c;
    }

    function uint248Chain(uint248 a, uint248 b, uint248 c) external pure returns (uint248) {
        return a + b - c;
    }

    function int248Chain(int248 a, int248 b, int248 c) external pure returns (int248) {
        return a + b - c;
    }

    function uint256Chain(uint256 a, uint256 b, uint256 c) external pure returns (uint256) {
        return a + b - c;
    }

    function int256Chain(int256 a, int256 b, int256 c) external pure returns (int256) {
        return a + b - c;
    }
}
