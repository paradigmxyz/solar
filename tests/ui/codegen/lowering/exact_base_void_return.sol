//@ revisions: none gas size
//@[none] compile-flags: -O none
//@[none, gas, size] run-call: Derived::f(bool) true => 2
//@[none, gas, size] run-call: Derived::f(bool) false => 2
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size

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
