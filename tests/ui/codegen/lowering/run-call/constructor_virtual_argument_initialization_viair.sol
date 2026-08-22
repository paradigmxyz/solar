//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@[none] normalize-stdout-test: "(?s).+" -> ""
//@[gas] normalize-stdout-test: "(?s).+" -> ""
//@[size] normalize-stdout-test: "(?s).+" -> ""
//@[none] run-call: ConstructorVirtualArgumentInitialization::x() => 2
//@[gas] run-call: ConstructorVirtualArgumentInitialization::x() => 2
//@[size] run-call: ConstructorVirtualArgumentInitialization::x() => 2
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
