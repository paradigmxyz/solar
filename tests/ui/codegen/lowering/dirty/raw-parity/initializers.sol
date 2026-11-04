//@ codegen-matrix: standard
//@ compile-flags: --allow=2018 --allow=5740
//@[mir] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: State::t_get => 511
//@ run-call: Immutable::t_get => 511
//@ run-call: Derived::t_get => 511

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
