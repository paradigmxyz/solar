//@ codegen-matrix: standard
//@ run-call: blend 4660, 43981, 65280 => 4813
//@ run-call: blend 4660, 43981, 3855 => 41668
//@ run-call: blend 4660, 43981, 0 => 43981
//@ run-call: blend 4660, 43981, 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 4660
//@ run-call: blend 57896044618658097711785492504343953926634992332820282019728792003956564819968, 3, 57896044618658097711785492504343953926634992332820282019728792003956564819968 => 57896044618658097711785492504343953926634992332820282019728792003956564819971
//@ run-call: constantMask 4660, 43981 => 43828, 4813
//@ run-call: narrowMask 52, 205 => 196
contract MaskBlend {
    function blend(uint256 x, uint256 y, uint256 m) external pure returns (uint256 result) {
        assembly { result := or(and(x, m), and(y, not(m))) }
    }

    function constantMask(uint256 x, uint256 y) external pure returns (uint256, uint256) {
        return ((x & 255) | (y & ~uint256(255)), (x & ~uint256(255)) | (y & 255));
    }

    function narrowMask(uint8 x, uint8 y) external pure returns (uint8) {
        return (x & 15) | (y & 240);
    }
}
