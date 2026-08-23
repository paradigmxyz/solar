//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: choose(bool) true => 7, 0
//@ run-call: choose(bool) false => 0, 7
//@ run-call: chooseArray(bool) true => 11
//@ run-call: chooseArray(bool) false => 22

contract TernaryStorageReference {
    struct S {
        uint256 x;
    }

    S a;
    S b;
    uint256[] first;
    uint256[] second;

    constructor() {
        first.push(11);
        second.push(22);
    }

    function choose(bool condition) external returns (uint256, uint256) {
        S storage slot = a;
        condition ? slot = a : slot = b;
        slot.x = 7;
        return (a.x, b.x);
    }

    function chooseArray(bool condition) external view returns (uint256) {
        uint256[] storage slot;
        slot = condition ? first : second;
        return slot[0];
    }
}
