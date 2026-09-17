//@ codegen-matrix: standard
//@ run-call: arithmetic 0, 1 => 0, 0, 1
//@ run-call: arithmetic 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1
//@ run-call: bitwise 0x8001, 0xff80 => 0x8001, 0xff80, 0xff80, 0x8001
//@ run-call: bitwise 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0
//@ run-call: equality 0, 0 => true, true, 0
//@ run-call: equality 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => true, true, 0
//@ run-call: equality 0, 1 => false, false, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: equality 0, 0x8000000000000000000000000000000000000000000000000000000000000000 => false, false, 0x8000000000000000000000000000000000000000000000000000000000000000

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
}
