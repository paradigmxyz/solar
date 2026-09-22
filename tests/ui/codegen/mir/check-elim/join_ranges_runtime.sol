//@ codegen-matrix: standard
//@ run-call: choose true, 99, 79 => 100
//@ run-call: choose false, 99, 79 => 80
//@ run-call-fail: choose true, 100, 79
//@ run-call-fail: choose false, 99, 80
//@ run-call: boundedLoop 0 => 0
//@ run-call: boundedLoop 100 => 100
//@ run-call: wrapLoop 257 => 1
//@ run-call: wrapLoop 511 => 255
//@ run-call-fail: checked 256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: castBound 127 => 127
//@ run-call-fail: castBound 128
//@ run-call: lossyCast 256 => 0
//@ run-call: lossyCast 257 => 1
//@ run-call: signedCast 127 => 127
//@ run-call: signedCast 128 => -128
//@ run-call: signedCast 255 => -1
//@ run-call: signedJoin true, -1, -2 => -1
//@ run-call: signedJoin false, -1, -2 => -2
//@ run-call: signedJoin true, -57896044618658097711785492504343953926634992332820282019728792003956564819968, -2 => -57896044618658097711785492504343953926634992332820282019728792003956564819968
//@ run-call-fail: signedJoin true, 0, -2
//@ run-call-fail: signedJoin false, -1, 0
//@ run-call: signedPositive 0 => 0
//@ run-call: signedPositive 99 => 99
//@ run-call-fail: signedPositive -1
//@ run-call-fail: signedPositive 100
//@ run-call-fail: signedPositive 57896044618658097711785492504343953926634992332820282019728792003956564819967
//@ run-call-fail: signedPositive -57896044618658097711785492504343953926634992332820282019728792003956564819968
//@ run-call-fail: crossSign -1
//@ run-call: crossSign 99 => 99
//@ run-call: signedNegative -100 => 115792089237316195423570985008687907853269984665640564039457584007913129639836
//@ run-call: signedNegative -11 => 115792089237316195423570985008687907853269984665640564039457584007913129639925
//@ run-call-fail: signedNegative -101
//@ run-call-fail: signedNegative -10
//@ run-call-fail: signedNegative 0
contract JoinedRanges {
    function castBound(uint256 x) external pure returns (uint256) {
        require(x < 128);
        uint256 y = uint256(uint160(x));
        require(y < 128);
        return y;
    }
    function lossyCast(uint256 x) external pure returns (uint256) {
        require(x > 255);
        return uint256(uint8(x));
    }
    function signedCast(uint256 x) external pure returns (int256) {
        require(x < 256);
        return int256(int8(uint8(x)));
    }
    function signedNegative(int256 x) external pure returns (uint256) {
        require(x < 0);
        require(x >= -100 && x < -10);
        require(uint256(x) >= uint256(type(uint256).max - 99));
        return uint256(x);
    }

    function signedJoin(bool choice, int256 a, int256 b) external pure returns (int256) {
        int256 x;
        if (choice) { require(a < 0); x = a; }
        else { require(b < 0); x = b; }
        require(x < 1);
        return x;
    }
    function signedPositive(int256 x) external pure returns (uint256) {
        require(x >= 0 && x < 100);
        require(uint256(x) < 100);
        return uint256(x);
    }
    function crossSign(int256 x) external pure returns (uint256) {
        require(x < 100);
        require(uint256(x) < 100);
        return uint256(x);
    }

    function choose(bool choice, uint256 a, uint256 b) external pure returns (uint256) {
        uint256 x;
        if (choice) { require(a < 100); x = a; }
        else { require(b < 80); x = b; }
        require(x < 100);
        return x + 1;
    }
    function boundedLoop(uint256 limit) external pure returns (uint256 i) {
        require(limit <= 100);
        while (i < limit) ++i;
        require(i <= 100);
    }
    function wrapLoop(uint256 limit) external pure returns (uint8 x) {
        for (uint256 i; i < limit; ++i) { unchecked { ++x; } }
    }
    function checked(uint256 limit) external pure returns (uint8 x) {
        for (uint256 i; i < limit; ++i) ++x;
    }
}
