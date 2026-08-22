//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none, gas, size] run-call: ConstructorInitOrderDerived::y() => 42
//@[none, gas, size] run-call: NoCtorDerived::y() => 42
// ported-from: test/libsolidity/semanticTests/inheritance/constructor_inheritance_init_order.sol

contract ConstructorInitOrderBase {
    uint256 x;

    constructor() {
        x = 42;
    }

    function f() public view returns (uint256) {
        return x;
    }
}

contract ConstructorInitOrderDerived is ConstructorInitOrderBase {
    uint256 public y = f();
}

contract NoCtorBase {
    uint256 public x = 42;
}

contract NoCtorDerived is NoCtorBase {
    uint256 public y = x;
}
