//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: ConstructorStateOrderDerived::a() => 17
//@ run-call: ConstructorStateOrderDerived::b() => 42
//@ run-call: ConstructorStateOrderDerived::c() => 51
//@ run-call: ConstructorStateOrderDerived::bA() => 17
//@ run-call: ConstructorStateOrderDerived::bB() => 42
//@ run-call: ConstructorStateOrderDerived::bC() => 51
//@ run-call: ConstructorStateOrderDerived::d() => 23
//@ run-call: ConstructorStateOrderDerived::e() => 42
// ported-from: test/libsolidity/semanticTests/inheritance/state_variables_init_order_3.sol

contract ConstructorStateOrderBase {
    uint256 public a = 42;
    uint256 public b;
    uint256 public c;

    constructor(uint256 x) {
        b = a;
        a = x;
    }

    function f(uint256 x) public returns (uint256) {
        c = x * 3;
        return 23;
    }
}

contract ConstructorStateOrderDerived is ConstructorStateOrderBase {
    uint256 public d = f(a);
    uint256 public e = b;
    uint256 public bA;
    uint256 public bB;
    uint256 public bC;

    constructor() ConstructorStateOrderBase(17) {
        bA = a;
        bB = b;
        bC = c;
    }
}
