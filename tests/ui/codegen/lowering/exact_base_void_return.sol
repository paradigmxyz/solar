//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[none] run-call: Derived::f(bool) true => 2
//@[none] run-call: Derived::f(bool) false => 2
//@[gas] compile-flags: -O gas
//@[gas] run-call: Derived::f(bool) true => 2
//@[gas] run-call: Derived::f(bool) false => 2
//@[size] compile-flags: -O size
//@[size] run-call: Derived::f(bool) true => 2
//@[size] run-call: Derived::f(bool) false => 2

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
