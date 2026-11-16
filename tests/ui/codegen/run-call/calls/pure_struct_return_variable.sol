//@ codegen-matrix: standard
//@ run-call: f() => 0
//@ run-call: g() => 1

contract C {
    struct S {
        uint256 x;
    }

    function make() internal pure returns (S memory s) {
        return S(s.x);
    }

    function make2() internal pure returns (S memory s) {
        return S(1 + (s.x));
    }

    function f() external pure returns (uint256) {
        return make().x;
    }

    function g() external pure returns (uint256) {
        return make2().x;
    }
}
