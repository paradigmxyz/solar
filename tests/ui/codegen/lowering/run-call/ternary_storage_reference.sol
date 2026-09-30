//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ compile-flags: -Zvalidate-ir=true
//@ run-call: choose true => 7, 0
//@ run-call: choose false => 0, 7
//@ run-call: chooseArray true => 11
//@ run-call: chooseArray false => 22
//@ run-call: memoryChoice true, 0 => 7, 0, 1
//@ run-call: memoryChoice false, 0 => 0, 7, 1
//@ run-call-fail: memoryChoice true, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

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

    function memoryChoice(bool flag, uint256 index) external pure returns (uint256, uint256, uint256) {
        uint256[] memory left = new uint256[](1);
        uint256[] memory right = new uint256[](1);
        uint256 evaluations;
        (++evaluations == 1 && flag ? left : right)[index] += 7;
        return (left[0], right[0], evaluations);
    }
}
