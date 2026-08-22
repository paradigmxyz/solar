//@ filecheck:
// CHECK: @module
//@ revisions: none gas size
//@[none] compile-flags: -O none -Zdump=mir
//@[gas] compile-flags: -O gas -Zdump=mir
//@[size] compile-flags: -O size -Zdump=mir
//@ run-call: ConstructorInitOrderDerived::y() => 42
//@ run-call: NoCtorDerived::y() => 42
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
