//@ codegen-matrix: standard
//@ run-call-fail: f
//@ run-call-fail: g

contract C {
    struct S {
        uint256 x;
    }

    function make() internal pure returns (S memory) {
        return S(make().x);
    }

    function ping() internal pure returns (S memory) {
        return S(pong().x);
    }

    function pong() internal pure returns (S memory) {
        return S(ping().x);
    }

    function f() external pure returns (uint256) {
        return make().x;
    }

    function g() external pure returns (uint256) {
        return ping().x;
    }
}
