//@ codegen-matrix: standard
//@ run-call: f() => 0

contract C {
    struct S {
        uint256 x;
    }

    function make() internal pure returns (S memory s) {
        return S(s.x);
    }

    function f() external pure returns (uint256) {
        return make().x;
    }
}
