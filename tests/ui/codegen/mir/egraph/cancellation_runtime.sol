//@ codegen-matrix: standard
//@ run-call: arithmetic 0, 1 => 0, 0, 1
//@ run-call: arithmetic 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1
//@ run-call: bitwise 0x8001, 0xff80 => 0x8001, 0xff80, 0xff80, 0x8001
//@ run-call: bitwise 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0
//@ run-call: equality 0, 0 => true, true, 0
//@ run-call: equality 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true, true, 0
//@ run-call: equality 0, 1 => false, false, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: equality 0, 0x8000000000000000000000000000000000000000000000000000000000000000 => false, false, 0x8000000000000000000000000000000000000000000000000000000000000000

//@ run-call: common 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2, 1 => 1, 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: carry 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0
//@ run-call: carry 0x8000000000000000000000000000000000000000000000000000000000000000, 0x8000000000000000000000000000000000000000000000000000000000000000 => 0
//@ run-call: dynamicPower 3, 0 => 3, 0
//@ run-call: dynamicPower 3, 255 => 0x8000000000000000000000000000000000000000000000000000000000000000, 3
//@ run-call: dynamicPower 3, 256 => 0, 0
//@ run-call: dynamicPower 3, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0, 0
//@ run-call: signedRemainder 0 => 0
//@ run-call: signedRemainder 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0
//@ run-call: signedRemainder 0x8000000000000000000000000000000000000000000000000000000000000000 => 0
//@ run-call: compare 0, 1 => false, true
//@ run-call: compare 1, 0 => true, false
//@ run-call: compare 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => false, false
//@ run-call: selects true, 17, 17, 23 => 17, 17, 17
//@ run-call: selects false, 17, 19, 23 => 19, 23, 23
//@ run-call: bitTestSelect 0x1200 => 0x1200, 0x1200
//@ run-call: bitTestSelect 0x12ff => 0x12ff, 0x1200

contract Cancellation {
    function arithmetic(uint256 x, uint256 y) external pure returns (uint256 a, uint256 b, uint256 c) {
        assembly {
            a := add(sub(x, y), y)
            b := sub(add(x, y), y)
            c := sub(add(x, y), x)
        }
    }

    function bitwise(uint256 x, uint256 y) external pure returns (uint256 a, uint256 b, uint256 c, uint256 d) {
        assembly {
            a := and(x, or(x, y))
            b := or(y, and(x, y))
            c := xor(x, xor(x, y))
            d := xor(xor(x, y), y)
        }
    }

    function equality(uint256 x, uint256 y) external pure returns (bool a, bool b, uint256 difference) {
        assembly {
            difference := sub(x, y)
            a := iszero(difference)
            b := iszero(xor(x, y))
        }
    }
    function common(uint256 x, uint256 y, uint256 z) external pure returns (uint256 a, uint256 b, uint256 c) {
        assembly {
            a := sub(add(x, y), add(x, z))
            b := sub(sub(x, z), sub(y, z))
            c := sub(sub(x, y), sub(x, z))
        }
    }

    function carry(uint256 x, uint256 y) external pure returns (uint256 result) {
        assembly { result := add(xor(x, y), shl(1, and(x, y))) }
    }

    function dynamicPower(uint256 x, uint256 shift) external pure returns (uint256 product, uint256 remainder) {
        assembly {
            product := mul(x, shl(shift, 1))
            remainder := mod(x, shl(shift, 1))
        }
    }

    function signedRemainder(uint256 x) external pure returns (uint256 result) {
        assembly { result := smod(x, not(0)) }
    }

    function compare(uint256 x, uint256 y) external pure returns (bool equal, bool less) {
        assembly {
            equal := eq(xor(x, y), x)
            less := lt(x, sub(x, y))
        }
    }

    function selects(bool c, uint256 x, uint256 y, uint256 z) external pure returns (uint256, uint256, uint256) {
        return (x == y ? x : y, c ? x : (c ? y : z), c ? (c ? x : y) : z);
    }

    function bitTestSelect(uint256 x) external pure returns (uint256, uint256) {
        return ((x & 255) == 0 ? (x & ~uint256(255)) : x, (x & 255) == 0 ? x : (x & ~uint256(255)));
    }
}
