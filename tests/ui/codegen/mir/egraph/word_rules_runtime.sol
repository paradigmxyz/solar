//@ codegen-matrix: standard
//@ run-call: mixed 0x8001, 0xff80 => 0, 0x8000, 0x7f81, 0xff81, 0xff81, 0xff81, 0x7f81, 0xff81, 0x8000
//@ run-call: mixed 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0, 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0
//@ run-call: mixed 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: arithmetic 0, 1 => 0, 0, 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0
//@ run-call: arithmetic 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0 => 1, 1, 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0
//@ run-call: arithmetic 0x8000000000000000000000000000000000000000000000000000000000000000, 0 => 0x8000000000000000000000000000000000000000000000000000000000000000, 0x8000000000000000000000000000000000000000000000000000000000000000, 0x8000000000000000000000000000000000000000000000000000000000000000, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0
//@ run-call: guards 0x8000000000000000000000000000000000000000000000000000000000000001, 0 => 1, 0, 0, 0, 0
//@ run-call: guards 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2 => 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0, 1, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff

contract WordRules {
    function mixed(uint256 x, uint256 y) external pure returns (
        uint256 a, uint256 b, uint256 c, uint256 d, uint256 e,
        uint256 f, uint256 g, uint256 h, uint256 i
    ) {
        assembly {
            a := and(and(x, y), xor(x, y))
            b := and(and(x, y), or(x, y))
            c := and(or(x, y), xor(x, y))
            d := or(and(x, y), or(x, y))
            e := or(and(x, y), xor(x, y))
            f := or(or(x, y), xor(x, y))
            g := xor(and(x, y), or(x, y))
            h := xor(and(x, y), xor(x, y))
            i := xor(or(x, y), xor(x, y))
        }
    }

    function arithmetic(uint256 x, uint256 y) external pure returns (
        uint256 a, uint256 b, uint256 c, uint256 d, uint256 e
    ) {
        assembly {
            a := mul(x, not(0))
            b := sdiv(x, not(0))
            c := sub(not(x), not(y))
            d := add(x, not(x))
            e := add(mul(x, not(0)), x)
        }
    }

    function guards(uint256 x, uint256 y) external pure returns (
        uint256 a, uint256 b, uint256 c, uint256 d, uint256 e
    ) {
        assembly {
            a := div(mul(x, 2), 2)
            b := div(x, y)
            c := sdiv(x, y)
            d := mod(x, y)
            e := smod(x, y)
        }
    }
}
