//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: C::t_payable_same => 115792089237316195423570985008687907853269984665640564039457584007913129639935
//@ run-call: C::t_payable_convert => 1461501637330902918203684832716283019655932542975
//@ run-call: C::t_postfix_dec => 16581118
//@ run-call: C::t_postfix_inc => 16712190
//@ run-call: C::t_postfix_return => 510
//@ run-call: C::t_postfix_argument => 510
//@ run-call: C::t_prefix_dec => 16580861
//@ run-call: C::t_logical_and => 2
//@ run-call: C::t_logical_and_false => 0
//@ run-call: C::t_logical_or => 2
//@ run-call: C::t_logical_or_rhs => 2
//@ run-call: C::t_logical_clean_use => 7
//@ run-call: C::t_bit_or_literal => 511
//@ run-call: C::t_bit_or_var => 511
//@ run-call: C::t_bit_xor_literal => 511
//@ run-call: C::t_bit_xor_var => 508
//@ run-call: C::t_bit_and_both_dirty => 255
//@ run-call: C::t_bit_and_clean_operand => 255
//@ run-call: C::t_bit_and_literal => 255
//@ run-call: C::t_bit_not => 0
//@ run-call: C::t_bit_or_int8 => 511
//@ run-call: C::t_bit_or_bytes1 => 511
//@ run-call: C::t_bit_or_nested => 511
//@ run-call: C::t_bit_or_widen => 255
//@ run-call: C::t_bit_or_argument => 511
//@ run-call: C::t_bit_or_return => 511
//@ run-call: C::t_bit_or_clean_use => 255
//@ run-call: C::t_bit_or_compare => 1

// Generated cases. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR:
// payable, postfix, logical and bitwise expressions forward raw bits.

contract C {
    function d() internal pure returns (uint8 r) {
        assembly { r := 0x2ff }
    }

    function raw(uint8 v) internal pure returns (uint256 r) {
        assembly { r := v }
    }

    function rawAfterInc() internal pure returns (uint256 r) {
        uint8 a;
        assembly { a := 0x1fe }
        uint8 b = a++;
        assembly { r := b }
    }

    function rawOr() internal pure returns (uint256 r) {
        uint8 a;
        assembly { a := 0x1ff }
        uint8 b = a | 0;
        assembly { r := b }
    }

    function t_payable_same() external view returns (uint256 r) { address payable p; assembly { p := not(0) } address payable q = payable(p); assembly { r := q } }
    function t_payable_convert() external view returns (uint256 r) { address a; assembly { a := not(0) } address payable q = payable(a); assembly { r := q } }
    function t_postfix_dec() external view returns (uint256 r) { uint8 a; assembly { a := 0x1fe } uint8 b = a--; assembly { r := or(b, shl(16, a)) } }
    function t_postfix_inc() external view returns (uint256 r) { uint8 a; assembly { a := 0x1fe } uint8 b = a++; assembly { r := or(b, shl(16, a)) } }
    function t_postfix_return() external view returns (uint256 r) { r = rawAfterInc(); }
    function t_postfix_argument() external view returns (uint256 r) { uint8 a; assembly { a := 0x1fe } r = raw(a++); }
    function t_prefix_dec() external view returns (uint256 r) { uint8 a; assembly { a := 0x1fe } uint8 b = --a; assembly { r := or(b, shl(16, a)) } }
    function t_logical_and() external view returns (uint256 r) { bool a; assembly { a := 2 } bool b = a && a; assembly { r := b } }
    function t_logical_and_false() external view returns (uint256 r) { bool a; assembly { a := 2 } bool b = a && false; assembly { r := b } }
    function t_logical_or() external view returns (uint256 r) { bool a; assembly { a := 2 } bool b = a || false; assembly { r := b } }
    function t_logical_or_rhs() external view returns (uint256 r) { bool a; assembly { a := 2 } bool b = false || a; assembly { r := b } }
    function t_logical_clean_use() external view returns (uint256 r) { bool a; assembly { a := 2 } return (a && a) ? 7 : 8; }
    function t_bit_or_literal() external view returns (uint256 r) { uint8 a; assembly { a := 0x1ff } uint8 b = a | 0; assembly { r := b } }
    function t_bit_or_var() external view returns (uint256 r) { uint8 a; assembly { a := 0x1ff } uint8 c = 3; uint8 b = a | c; assembly { r := b } }
    function t_bit_xor_literal() external view returns (uint256 r) { uint8 a; assembly { a := 0x1ff } uint8 b = a ^ 0; assembly { r := b } }
    function t_bit_xor_var() external view returns (uint256 r) { uint8 a; assembly { a := 0x1ff } uint8 c = 3; uint8 b = a ^ c; assembly { r := b } }
    function t_bit_and_both_dirty() external view returns (uint256 r) { uint8 a; assembly { a := 0x1ff } uint8 b = a & d(); assembly { r := b } }
    function t_bit_and_clean_operand() external view returns (uint256 r) { uint8 a; assembly { a := 0x1ff } uint8 c = 0xff; uint8 b = a & c; assembly { r := b } }
    function t_bit_and_literal() external view returns (uint256 r) { uint8 a; assembly { a := 0x1ff } uint8 b = a & 0xff; assembly { r := b } }
    function t_bit_not() external view returns (uint256 r) { uint8 a; assembly { a := 0x1ff } uint8 b = ~a; assembly { r := b } }
    function t_bit_or_int8() external view returns (uint256 r) { int8 a; assembly { a := 0x1ff } int8 b = a | 0; assembly { r := b } }
    function t_bit_or_bytes1() external view returns (uint256 r) { bytes1 a; assembly { a := 0x1ff } bytes1 b = a | bytes1(0); assembly { r := b } }
    function t_bit_or_nested() external view returns (uint256 r) { uint8 a; assembly { a := 0x1ff } uint8 b = (a | 0) | 0; assembly { r := b } }
    function t_bit_or_widen() external view returns (uint256 r) { uint8 a; assembly { a := 0x1ff } uint16 b = a | uint16(0); assembly { r := b } }
    function t_bit_or_argument() external view returns (uint256 r) { uint8 a; assembly { a := 0x1ff } r = raw(a | 0); }
    function t_bit_or_return() external view returns (uint256 r) { r = rawOr(); }
    function t_bit_or_clean_use() external view returns (uint256 r) { uint8 a; assembly { a := 0x1ff } return uint256(a | 0); }
    function t_bit_or_compare() external view returns (uint256 r) { uint8 a; assembly { a := 0x1ff } return (a | 0) == 0xff ? 1 : 0; }
}
