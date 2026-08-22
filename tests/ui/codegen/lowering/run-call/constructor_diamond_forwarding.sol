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
//@[none, gas, size] run-call: DiamondDerived::i(); constructor=[2, 0] => 2
//@[none, gas, size] run-call: DiamondDerived::j(); constructor=[2, 0] => 2
//@[none, gas, size] run-call: DiamondDerived::k(); constructor=[2, 0] => 1
// ported-from: test/libsolidity/semanticTests/inheritance/constructor_with_params_diamond_inheritance.sol

contract DiamondBase {
    uint256 public i;
    uint256 public k;

    constructor(uint256 newI, uint256 newK) {
        i = newI;
        k = newK;
    }
}

abstract contract DiamondMiddle is DiamondBase {
    uint256 public j;

    constructor(uint256 newJ) {
        j = newJ;
    }
}

contract DiamondSide is DiamondBase {
    constructor(uint256 newI, uint256 newK) DiamondBase(newI, newK) {}
}

contract DiamondDerived is DiamondMiddle, DiamondSide {
    constructor(uint256 newI, uint256 newK) DiamondMiddle(newI) DiamondSide(newI, newK + 1) {}
}
