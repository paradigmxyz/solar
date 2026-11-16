//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: State::t_get => 511
//@ run-call: Immutable::t_get => 511
//@ run-call: Derived::t_get => 511
//@ run-call: SignedImmutables::t_ternary8 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: SignedImmutables::t_ternary24 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: SignedImmutables::t_bitwise8 => 115792089237316195423570985008687907853269984665640564039457584007913129639933
//@ run-call: SignedImmutables::t_bitwise24 => 115792089237316195423570985008687907853269984665640564039457584007913129639933

// Generated cases. Every expectation was captured by running the same source through
// solc 0.8.36 via-IR (`--via-ir --optimize`) on anvil, so each case matches via-IR:
// calls nested in state and immutable initializers keep raw bits.

function d() pure returns (uint8 q) {
    assembly { q := 0x1ff }
}

function g(uint8 u) pure returns (uint8) {
    return u;
}

function f(uint8 a) pure returns (uint256 r) {
    uint8 b = a;
    assembly { r := b }
}

contract State {
    uint256 s = f(g(d()));

    function t_get() external view returns (uint256) { return s; }
}

contract Immutable {
    uint256 immutable s = f(g(d()));

    function t_get() external view returns (uint256) { return s; }
}

contract StateBase {
    uint256 s = f(g(d()));
}

contract Derived is StateBase {
    function t_get() external view returns (uint256) { return s; }
}

contract SignedImmutables {
    int8 immutable I8 = -3;
    int24 immutable I24 = -3;

    function t_ternary8() external view returns (uint256 r) {
        int8 x = gasleft() > 0 ? I8 : int8(-2);
        assembly { r := x }
    }

    function t_ternary24() external view returns (uint256 r) {
        int24 x = gasleft() > 0 ? I24 : int24(-2);
        assembly { r := x }
    }

    function t_bitwise8() external view returns (uint256 r) {
        int8 x = I8 | int8(0);
        assembly { r := x }
    }

    function t_bitwise24() external view returns (uint256 r) {
        int24 x = I24 ^ int24(0);
        assembly { r := x }
    }
}
