//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: C::t_assign_discard => 511
//@ run-call: C::t_assign_first => 50266623
//@ run-call: C::t_swap => 197119
//@ run-call: C::t_nested => 33489407
//@ run-call: C::t_ternary => 197119
//@ run-call: C::t_memory => 511
//@ run-call: C::t_memory_array_elem => 1023
//@ run-call: C::t_declare => 50266623
//@ run-call: C::t_storage_ref_declare => 511
//@ run-call: C::t_storage_ref_assign => 511
//@ run-call: C::t_storage_ref_assign_clean_use => 255

// Generated cases. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR:
// tuple assignments and declarations keep raw bits.

struct S {
    uint256 v;
}

contract C {
    S ss;

    function d() internal pure returns (uint8 r) {
        assembly { r := 0x1ff }
    }

    function dd() internal pure returns (uint8 a, uint8 b) {
        assembly { a := 0x1ff b := 0x2ff }
    }

    function gm() internal pure returns (uint8 a, uint8[] memory m) {
        m = new uint8[](1);
        assembly { a := 0x1ff mstore(add(m, 32), 0x3ff) }
    }

    function g() internal view returns (uint8 a, S storage s) {
        assembly { a := 0x1ff }
        s = ss;
    }

    function t_assign_discard() external view returns (uint256 r) { uint8 p; (p, ) = dd(); assembly { r := p } }
    function t_assign_first() external view returns (uint256 r) { uint8 p; uint8 q; (p, q) = dd(); assembly { r := or(p, shl(16, q)) } }
    function t_swap() external view returns (uint256 r) { uint8 a = d(); uint8 b = 3; (a, b) = (b, a); assembly { r := or(b, shl(16, a)) } }
    function t_nested() external view returns (uint256 r) { uint8 a = d(); uint8 p; uint8 q; uint256 c; (p, (q, c)) = (a, (a, 1)); assembly { r := or(p, shl(16, q)) } }
    function t_ternary() external view returns (uint256 r) { uint8 a = d(); uint8 b = 3; uint8 p; uint8 q; (p, q) = gasleft() > 0 ? (a, b) : (b, a); assembly { r := or(p, shl(16, q)) } }
    function t_memory() external view returns (uint256 r) { uint8 x; uint8[] memory y; (x, y) = gm(); assembly { r := x } }
    function t_memory_array_elem() external view returns (uint256 r) { uint8 x; uint8[] memory y; (x, y) = gm(); assembly { r := mload(add(y, 32)) } }
    function t_declare() external view returns (uint256 r) { (uint8 p, uint8 q) = dd(); assembly { r := or(p, shl(16, q)) } }
    function t_storage_ref_declare() external view returns (uint256 r) { (uint8 x, S storage y) = g(); y; assembly { r := x } }
    function t_storage_ref_assign() external view returns (uint256 r) { S storage y = ss; uint8 x; (x, y) = g(); y; assembly { r := x } }
    function t_storage_ref_assign_clean_use() external view returns (uint256 r) { S storage y = ss; uint8 x; (x, y) = g(); y; return uint256(x) + 0; }
}
