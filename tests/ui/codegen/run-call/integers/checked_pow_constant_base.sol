//@ codegen-matrix: standard
//@ run-call: ten 77 => 100000000000000000000000000000000000000000000000000000000000000000000000000000
//@ run-call-fail: ten 78 => Panic(0x11)
//@ run-call-fail: ten 115792089237316195423570985008687907853269984665640564039457584007913129639935 => Panic(0x11)
//@ run-call: ten 0 => 1
//@ run-call: two 255 => 57896044618658097711785492504343953926634992332820282019728792003956564819968
//@ run-call-fail: two 256 => Panic(0x11)
//@ run-call: threeSigned 160 => 21847450052839212624230656502990235142567050104912751880812823948662932355201
//@ run-call-fail: threeSigned 161 => Panic(0x11)
//@ run-call: tenByte 2 => 100
//@ run-call-fail: tenByte 3 => Panic(0x11)
//@ run-call: zero 0 => 1
//@ run-call: zero 5 => 0

// A constant nonnegative base raises with one `EXP` after a bound on the
// exponent: the largest exponent whose power fits the result type passes, and
// the next one panics, however large it is.
contract CheckedPowConstantBase {
    function ten(uint256 e) external pure returns (uint256) {
        return 10 ** e;
    }

    function two(uint256 e) external pure returns (uint256) {
        return 2 ** e;
    }

    function threeSigned(uint256 e) external pure returns (int256) {
        return int256(3) ** e;
    }

    function tenByte(uint8 e) external pure returns (uint8) {
        return uint8(10) ** e;
    }

    function zero(uint256 e) external pure returns (uint256) {
        return 0 ** e;
    }
}
