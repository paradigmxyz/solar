//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: ConstructorMultipleBaseArgumentInitialization::state => 2, 3

contract ConstructorArgumentInitializationA {
    uint256 public x = 2;

    constructor(uint256) {}

    function touchA() internal returns (uint256) {
        x = 4;
        return 0;
    }
}

contract ConstructorArgumentInitializationB {
    uint256 public y = 3;

    constructor(uint256) {}

    function touchB() internal returns (uint256) {
        y = 5;
        return 0;
    }
}

contract ConstructorMultipleBaseArgumentInitialization
    is ConstructorArgumentInitializationA, ConstructorArgumentInitializationB
{
    constructor() ConstructorArgumentInitializationA(touchB()) ConstructorArgumentInitializationB(touchA()) {}

    function state() external view returns (uint256, uint256) {
        return (x, y);
    }
}
