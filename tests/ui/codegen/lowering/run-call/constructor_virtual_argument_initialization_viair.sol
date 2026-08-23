//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: ConstructorVirtualArgumentInitialization::x() => 2
// ported-from: test/libsolidity/semanticTests/inheritance/constructor_inheritance_init_order_3_viaIR.sol

contract ConstructorVirtualArgumentBase {
    uint256 public x = 2;

    constructor(uint256) {}

    function f() public returns (uint256) {
        x = 4;
        return 0;
    }
}

contract ConstructorVirtualArgumentInitialization is ConstructorVirtualArgumentBase {
    constructor() ConstructorVirtualArgumentBase(f()) {}
}
