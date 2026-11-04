//@ codegen-matrix: standard
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ compile-flags: --allow=2018 --allow=5740
//@ run-call: C::t_uop_add => 28
//@ run-call: C::t_uop_add_raw_result => 28
//@ run-call: C::t_uop_eq => 1
//@ run-call: C::t_uop_neg => 13
//@ run-call: C::t_uop_raw_identity => 11259163
//@ run-call: C::t_uop_raw_identity_clean => 27
//@ run-call: C::t_ef_call => 77
//@ run-call: C::t_ef_address => 1
//@ run-call: C::t_ef_selector => 3017696395
//@ run-call-fail: C::t_ef_selector_call
//@ run-call: C::t_ef_eq => 1
//@ run-call: C::t_ef_encode => 1
//@ run-call: C::t_ef_encode_packed => 1
//@ run-call: C::t_ef_storage => 77
//@ run-call: C::t_ef_memory_array => 77
//@ run-call: C::t_ef_internal_arg => 77
//@ run-call: C::t_ef_return => 77
//@ run-call: C::t_ef_external_arg => 77
//@ run-call: C::t_ef_ternary => 77

// Generated cases. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR:
// external function values and user-defined operators fed dirty words.

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
    function() external pure returns (uint256) sf;
    function ping() external pure returns (uint256) { return 77; }
    function call_it(function() external pure returns (uint256) f) internal view returns (uint256) { return f(); }
    function pass_through(function() external pure returns (uint256) f) internal pure returns (function() external pure returns (uint256)) { return f; }
    function take(function() external pure returns (uint256) f) external view returns (uint256) { return f(); }
    function t_uop_add() external view returns (uint256) { U x; assembly { x := 0xabcd1b } return uint256(U.unwrap(x + U.wrap(1))); }
    function t_uop_add_raw_result() external view returns (uint256) { U x; assembly { x := 0xabcd1b } U y = x + U.wrap(1); uint256 r; assembly { r := y } return r; }
    function t_uop_eq() external view returns (uint256) { U x; assembly { x := 0xabcd1b } return x == U.wrap(27) ? 1 : 0; }
    function t_uop_neg() external view returns (uint256) { U x; assembly { x := 0xabcd1b } return uint256(U.unwrap(-x)); }
    function t_uop_raw_identity() external view returns (uint256) { U x; assembly { x := 0xabcd1b } U y = ~x; uint256 r; assembly { r := y } return r; }
    function t_uop_raw_identity_clean() external view returns (uint256) { U x; assembly { x := 0xabcd1b } return uint256(U.unwrap(~x)); }
    function t_ef_call() external view returns (uint256) { function() external pure returns (uint256) f = this.ping; assembly { f.address := or(address(), shl(160, 0xabcd)) } return f(); }
    function t_ef_address() external view returns (uint256) { function() external pure returns (uint256) f = this.ping; assembly { f.address := or(address(), shl(160, 0xabcd)) } return f.address == address(this) ? 1 : 0; }
    function t_ef_selector() external view returns (uint256) { function() external pure returns (uint256) f = this.ping; assembly { f.selector := or(0xb3de648b, shl(32, 0xabcd)) } return uint256(uint32(f.selector)); }
    function t_ef_selector_call() external view returns (uint256) { function() external pure returns (uint256) f = this.ping; assembly { f.selector := or(0xb3de648b, shl(32, 0xabcd)) } return f(); }
    function t_ef_eq() external view returns (uint256) { function() external pure returns (uint256) f = this.ping; assembly { f.address := or(address(), shl(160, 0xabcd)) } return f == this.ping ? 1 : 0; }
    function t_ef_encode() external view returns (uint256) { function() external pure returns (uint256) f = this.ping; assembly { f.address := or(address(), shl(160, 0xabcd)) } return keccak256(abi.encode(f)) == keccak256(abi.encode(this.ping)) ? 1 : 0; }
    function t_ef_encode_packed() external view returns (uint256) { function() external pure returns (uint256) f = this.ping; assembly { f.address := or(address(), shl(160, 0xabcd)) } return keccak256(abi.encodePacked(f)) == keccak256(abi.encodePacked(this.ping)) ? 1 : 0; }
    function t_ef_storage() external returns (uint256) { function() external pure returns (uint256) f = this.ping; assembly { f.address := or(address(), shl(160, 0xabcd)) } sf = f; return sf(); }
    function t_ef_memory_array() external view returns (uint256) { function() external pure returns (uint256)[] memory a = new function() external pure returns (uint256)[](1); function() external pure returns (uint256) f = this.ping; assembly { f.address := or(address(), shl(160, 0xabcd)) } a[0] = f; return a[0](); }
    function t_ef_internal_arg() external view returns (uint256) { function() external pure returns (uint256) f = this.ping; assembly { f.address := or(address(), shl(160, 0xabcd)) } return call_it(f); }
    function t_ef_return() external view returns (uint256) { function() external pure returns (uint256) f = this.ping; assembly { f.address := or(address(), shl(160, 0xabcd)) } return pass_through(f)(); }
    function t_ef_external_arg() external view returns (uint256) { function() external pure returns (uint256) f = this.ping; assembly { f.address := or(address(), shl(160, 0xabcd)) } return this.take(f); }
    function t_ef_ternary() external view returns (uint256) { function() external pure returns (uint256) f = this.ping; assembly { f.address := or(address(), shl(160, 0xabcd)) } return (gasleft() > 0 ? f : f)(); }
}
