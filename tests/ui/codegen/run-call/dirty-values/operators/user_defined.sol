//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: C::t_uop_add => 28
//@ run-call: C::t_uop_add_raw_result => 28
//@ run-call: C::t_uop_eq => 1
//@ run-call: C::t_uop_neg => 13
//@ run-call: C::t_uop_raw_identity => 11259163
//@ run-call: C::t_uop_raw_identity_clean => 27

// Generated cases. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR:
// user-defined operators fed dirty words.

type U is uint8;

using {add as +, eq as ==, neg as -, raw as ~} for U global;

function add(U a, U b) pure returns (U) {
    return U.wrap(U.unwrap(a) + U.unwrap(b));
}

function eq(U a, U b) pure returns (bool) {
    return U.unwrap(a) == U.unwrap(b);
}

function neg(U a) pure returns (U) {
    return U.wrap(U.unwrap(a) / 2);
}

function raw(U a) pure returns (U r) {
    assembly {
        r := a
    }
}

contract C {
    function t_uop_add() external view returns (uint256) { U x; assembly { x := 0xabcd1b } return uint256(U.unwrap(x + U.wrap(1))); }
    function t_uop_add_raw_result() external view returns (uint256) { U x; assembly { x := 0xabcd1b } U y = x + U.wrap(1); uint256 r; assembly { r := y } return r; }
    function t_uop_eq() external view returns (uint256) { U x; assembly { x := 0xabcd1b } return x == U.wrap(27) ? 1 : 0; }
    function t_uop_neg() external view returns (uint256) { U x; assembly { x := 0xabcd1b } return uint256(U.unwrap(-x)); }
    function t_uop_raw_identity() external view returns (uint256) { U x; assembly { x := 0xabcd1b } U y = ~x; uint256 r; assembly { r := y } return r; }
    function t_uop_raw_identity_clean() external view returns (uint256) { U x; assembly { x := 0xabcd1b } return uint256(U.unwrap(~x)); }
}
