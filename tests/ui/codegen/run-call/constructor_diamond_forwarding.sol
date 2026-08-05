//@ run-call: DiamondDerived::i(); constructor=[2, 0] => 2
//@ run-call: DiamondDerived::j(); constructor=[2, 0] => 2
//@ run-call: DiamondDerived::k(); constructor=[2, 0] => 1
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
