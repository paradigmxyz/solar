//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: Derived::f(bool) true => 2
//@ run-call: Derived::f(bool) false => 2

contract Base {
    uint256 internal x;

    function gate(bool stop) internal {
        if (stop) return;
        x = 1;
    }
}

contract Derived is Base {
    function f(bool stop) external returns (uint256) {
        Base.gate(stop);
        x = 2;
        return x;
    }
}
