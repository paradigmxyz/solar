//@ codegen-matrix: standard
//@ run-call: retained 0 => 0, false
//@ run-call: retained 7 => 3166189940082864718613269121331309980362851143201109172953918312716374638592, true
//@ run-call: retained 263 => 3166189940082864718613269121331309980362851143201109172953918312716374638592, true
//@ run-call: several 6 => false, true, false
//@ run-call: several 7 => true, true, true
//@ run-call: several 8 => false, false, true
//@ run-call: several 255 => false, false, true
//@ run-call: repeated 0 => 0
//@ run-call: repeated 1 => 4
//@ run-call: repeated 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 115792089237316195423570985008687907853269984665640564039457584007913129639932
//@ run-call: repeated 57896044618658097711785492504343953926634992332820282019728792003956564819968 => 0
//@ run-call: returned 115792089237316195423570985008687907853269984665640564039457584007913129639935 => 115792089237316195423570985008687907853269984665640564039457584007913129639934, 115792089237316195423570985008687907853269984665640564039457584007913129639932
contract SharedInputCosts {
    function retained(uint256 x) external pure returns (uint256 high, bool equal) {
        assembly {
            high := shl(248, byte(31, x))
            equal := eq(high, shl(248, 7))
        }
    }

    function several(uint256 x) external pure returns (bool equal, bool less, bool greater) {
        assembly {
            let high := shl(248, byte(31, x))
            equal := eq(high, shl(248, 7))
            less := lt(high, shl(248, 8))
            greater := gt(high, shl(248, 6))
        }
    }

    function repeated(uint256 x) external pure returns (uint256 result) {
        assembly { result := shl(1, shl(1, x)) }
    }

    function returned(uint256 x) external pure returns (uint256 twice, uint256 fourTimes) {
        assembly {
            twice := shl(1, x)
            fourTimes := shl(1, twice)
        }
    }
}
