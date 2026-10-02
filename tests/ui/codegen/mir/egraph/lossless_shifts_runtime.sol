//@ codegen-matrix: standard
//@ run-call: narrow 0, 0 => 0, 1, 0, 0
//@ run-call: narrow 255, 255 => 255, 1, 0, 0
//@ run-call: narrow 255, 1 => 255, 0, 0, 1
//@ run-call: narrow 0, 255 => 0, 0, 1, 0
//@ run-call: narrow 256, 0 => 0, 1, 0, 0
//@ run-call: aligned 7 => 1, 0, 0, 1, 0, 0
//@ run-call: aligned 6 => 0, 1, 0, 0, 0, 1
//@ run-call: aligned 8 => 0, 0, 1, 0, 1, 0
//@ run-call: unaligned 7 => 0, 1, 0
//@ run-call: unaligned 8 => 0, 0, 1
//@ run-call: truncating 256 => 0
//@ run-call: signedCompare 128, 1 => 1, 0
//@ run-call: signedCompare 127, 128 => 0, 1
//@ run-call: equal 0, 0 => true
//@ run-call: equal 255, 1 => false
//@ run-call: equal 127, 128 => false
//@ run-call: equal 256, 0 => true
//@ run-call: isSeven 0 => false
//@ run-call: isSeven 6 => false
//@ run-call: isSeven 7 => true
//@ run-call: isSeven 8 => false
//@ run-call: isSeven 255 => false
//@ run-call: isSeven 263 => true
//@ run-call: less 0, 0 => false
//@ run-call: less 255, 1 => false
//@ run-call: less 127, 128 => true
//@ run-call: less 256, 0 => false
//@ run-call: belowSeven 0 => true
//@ run-call: belowSeven 6 => true
//@ run-call: belowSeven 7 => false
//@ run-call: belowSeven 8 => false
//@ run-call: belowSeven 255 => false
//@ run-call: belowSeven 263 => false
//@ run-call: greater 0, 0 => false
//@ run-call: greater 255, 1 => true
//@ run-call: greater 127, 128 => false
//@ run-call: greater 256, 0 => false
//@ run-call: aboveSeven 0 => false
//@ run-call: aboveSeven 6 => false
//@ run-call: aboveSeven 7 => false
//@ run-call: aboveSeven 8 => true
//@ run-call: aboveSeven 255 => true
//@ run-call: aboveSeven 263 => false
contract LosslessShifts {
    function narrow(uint256 x, uint256 y) external pure returns (uint256 value, uint256 equal, uint256 less, uint256 greater) {
        assembly {
            let a := shl(248, byte(31, x))
            let b := shl(248, byte(31, y))
            value := shr(248, a)
            equal := eq(a, b)
            less := lt(a, b)
            greater := gt(a, b)
        }
    }

    function aligned(uint256 x) external pure returns (uint256 a, uint256 b, uint256 c, uint256 d, uint256 e, uint256 f) {
        assembly {
            let v := shl(248, byte(31, x))
            let k := shl(248, 7)
            a := eq(v, k)
            b := lt(v, k)
            c := gt(v, k)
            d := eq(k, v)
            e := lt(k, v)
            f := gt(k, v)
        }
    }

    function unaligned(uint256 x) external pure returns (uint256 a, uint256 b, uint256 c) {
        assembly {
            let v := shl(248, byte(31, x))
            let k := add(shl(248, 7), 1)
            a := eq(v, k)
            b := lt(v, k)
            c := gt(v, k)
        }
    }

    function truncating(uint256 x) external pure returns (uint256 value) {
        assembly { value := shr(248, shl(248, x)) }
    }

    function signedCompare(uint256 x, uint256 y) external pure returns (uint256 less, uint256 greater) {
        assembly {
            let a := shl(248, byte(31, x))
            let b := shl(248, byte(31, y))
            less := slt(a, b)
            greater := sgt(a, b)
        }
    }

    function equal(uint256 x, uint256 y) external pure returns (bool result) {
        assembly { result := eq(shl(248, byte(31, x)), shl(248, byte(31, y))) }
    }

    function isSeven(uint256 x) external pure returns (bool result) {
        assembly { result := eq(shl(248, byte(31, x)), shl(248, 7)) }
    }

    function less(uint256 x, uint256 y) external pure returns (bool result) {
        assembly { result := lt(shl(248, byte(31, x)), shl(248, byte(31, y))) }
    }

    function belowSeven(uint256 x) external pure returns (bool result) {
        assembly { result := lt(shl(248, byte(31, x)), shl(248, 7)) }
    }

    function greater(uint256 x, uint256 y) external pure returns (bool result) {
        assembly { result := gt(shl(248, byte(31, x)), shl(248, byte(31, y))) }
    }

    function aboveSeven(uint256 x) external pure returns (bool result) {
        assembly { result := gt(shl(248, byte(31, x)), shl(248, 7)) }
    }
}
