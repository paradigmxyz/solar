//@ codegen-matrix: standard
//@ run-call: Derived::v => 2001
//@ run-call: Derived::f => 43
//@ run-call: Derived::g => 56
// Named arguments bind by parameter name in every call-like argument list,
// including inheritance specifiers, base constructor calls in the constructor
// header, and modifier invocations. solc parses those lists as plain
// expression lists and rejects the named form; see TYPECK-004 in
// docs/SOLC_DIVERGENCE.md.
contract Base {
    uint256 public v;

    constructor(uint8 a, uint256 b) {
        v = uint256(a) * 1000 + b;
    }
}

contract Derived is Base({b: 1, a: 2}) {
    uint256 internal w;

    modifier m(uint256 a, uint256 b) {
        w = a * 10 + b;
        _;
    }

    function f() external m({b: 3, a: 4}) returns (uint256) {
        return w;
    }

    function g() external m({a: 5, b: 6}) returns (uint256) {
        return w;
    }
}
